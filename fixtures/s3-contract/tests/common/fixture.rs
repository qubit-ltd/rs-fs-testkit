//! Independent SDK-backed preparation, observation, and cleanup for S3 tests.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;

use futures_util::TryStreamExt;
use object_store::ObjectStore;
use object_store::ObjectStoreExt;
use object_store::path::Path as ObjectPath;
use qubit_fs::AsyncFileSystem;
use qubit_fs::Path;
use qubit_fs::metadata::ResourceVersion;
use qubit_fs::write::WriteDisposition;
use qubit_fs::write::WriteOptions;
use qubit_fs::write::WritePrecondition;
use qubit_fs_s3_contract::S3ContractConfig;
use qubit_fs_s3_contract::TestControl;
use qubit_fs_s3_contract::TestStage;
use qubit_fs_s3_contract::open_with_control;
use qubit_fs_testkit::AsyncFileSystemFixture;
use qubit_fs_testkit::AsyncWriteCancellationStage;
use qubit_fs_testkit::AsyncWriteFixtureCase;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixtureFuture;
use qubit_fs_testkit::FixturePreparation;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;
use qubit_fs_testkit::WriteCancellationProbe;
use qubit_fs_testkit::WriteFixtureCase;
use qubit_fs_testkit::WriteScenario;

/// The facade and independent observer share only the underlying SDK store.
pub struct Fixture {
    pub filesystem: AsyncFileSystem,
    pub store: Arc<dyn ObjectStore>,
    pub config: S3ContractConfig,
    pub control: TestControl,
    owned: Mutex<HashSet<ObjectPath>>,
}

impl Fixture {
    /// Creates an isolated in-memory namespace with independent SDK access.
    #[allow(dead_code, reason = "real-service test binary uses Fixture::new instead")]
    pub fn memory(prefix: &str) -> Self {
        let store: Arc<dyn ObjectStore> = Arc::new(object_store::memory::InMemory::new());
        let config = S3ContractConfig {
            endpoint: "memory://".into(),
            bucket: "contract".into(),
            region: "local".into(),
            access_key_id: "test".into(),
            secret_access_key: "test".into(),
            prefix: prefix.into(),
            allow_http: false,
        };
        Self::new(config, store)
    }

    /// Uses only the explicit SDK store and records exact keys owned by this
    /// run.
    pub fn new(config: S3ContractConfig, store: Arc<dyn ObjectStore>) -> Self {
        let control = TestControl::default();
        let filesystem =
            open_with_control(config.clone(), Arc::clone(&store), control.clone()).expect("fixture facade");
        Self {
            filesystem,
            store,
            config,
            control,
            owned: Mutex::new(HashSet::new()),
        }
    }

    /// Maps an already valid logical key to its isolated SDK identity.
    fn object_path(&self, path: &Path) -> FixtureResult<ObjectPath> {
        let key = format!("{}/{}", self.config.prefix, path.as_str());
        ObjectPath::parse(key).map_err(|error| FixtureError::with_source("SDK key failed", error))
    }
}

impl AsyncFileSystemFixture for Fixture {
    /// Returns the facade being tested.
    fn file_system(&self) -> &AsyncFileSystem {
        &self.filesystem
    }
    /// Maps test names without a facade operation.
    fn path(&self, relative: &str) -> FixtureResult<Path> {
        let path =
            Path::parse_literal(relative).map_err(|error| FixtureError::with_source("fixture path failed", error))?;
        self.owned
            .lock()
            .expect("owned key lock")
            .insert(self.object_path(&path)?);
        Ok(path)
    }
    /// Seeds directly through the SDK; facade writes cannot validate
    /// themselves.
    fn seed_file<'a>(&'a self, relative: &'a str, bytes: &'a [u8]) -> FixtureFuture<'a, FixtureSupport<Path>> {
        Box::pin(async move {
            let path = self.path(relative)?;
            let key = self.object_path(&path)?;
            self.store
                .put_opts(
                    &key,
                    bytes::Bytes::copy_from_slice(bytes).into(),
                    object_store::PutOptions {
                        mode: object_store::PutMode::Create,
                        ..Default::default()
                    },
                )
                .await
                .map_err(|error| FixtureError::with_source("independent seed failed", error))?;
            Ok(FixtureSupport::Supported(path))
        })
    }
    /// Reads complete stored bytes directly through the SDK.
    fn read_file<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<Vec<u8>>> {
        Box::pin(async move {
            let result = self
                .store
                .get(&self.object_path(path)?)
                .await
                .map_err(|error| FixtureError::with_source("independent get failed", error))?;
            result
                .bytes()
                .await
                .map(|bytes| FixtureSupport::Supported(bytes.to_vec()))
                .map_err(|error| FixtureError::with_source("independent body failed", error))
        })
    }
    /// Observes resource existence without invoking the facade stat method.
    fn exists_out_of_band<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<bool>> {
        Box::pin(async move {
            match self.store.head(&self.object_path(path)?).await {
                Ok(_) => Ok(FixtureSupport::Supported(true)),
                Err(object_store::Error::NotFound { .. }) => Ok(FixtureSupport::Supported(false)),
                Err(error) => Err(FixtureError::with_source("independent existence probe failed", error)),
            }
        })
    }
    /// Captures an SDK version independently of facade metadata mapping.
    fn resource_version<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<ResourceVersion>> {
        Box::pin(async move {
            let metadata = self
                .store
                .head(&self.object_path(path)?)
                .await
                .map_err(|error| FixtureError::with_source("independent head failed", error))?;
            Ok(metadata
                .e_tag
                .map(|etag| FixtureSupport::Supported(ResourceVersion::new(etag)))
                .unwrap_or(FixtureSupport::Unsupported))
        })
    }
    /// Enumerates all SDK objects in this fresh, isolated configured namespace.
    fn snapshot_namespace_paths(&self) -> FixtureFuture<'_, FixtureSupport<Vec<Path>>> {
        Box::pin(async move {
            let prefix = ObjectPath::parse(&self.config.prefix).unwrap();
            let entries = self
                .store
                .list(Some(&prefix))
                .try_collect::<Vec<_>>()
                .await
                .map_err(|error| FixtureError::with_source("independent snapshot failed", error))?;
            let boundary = format!("{}/", self.config.prefix);
            let paths = entries
                .iter()
                .map(|entry| {
                    let logical = entry
                        .location
                        .as_ref()
                        .strip_prefix(&boundary)
                        .ok_or_else(|| FixtureError::new("SDK snapshot escaped namespace"))?;
                    self.path(logical)
                })
                .collect::<FixtureResult<Vec<_>>>()?;
            Ok(FixtureSupport::Supported(paths))
        })
    }
    /// Deletes only the exact keys recorded before this fixture's operations.
    fn teardown(&self) -> FixtureFuture<'_, ()> {
        Box::pin(async move {
            self.control.release();
            let owned: Vec<_> = self.owned.lock().expect("owned key lock").iter().cloned().collect();
            let mut first_error = None;
            for key in owned {
                match self.store.delete(&key).await {
                    Ok(()) | Err(object_store::Error::NotFound { .. }) => {
                        self.owned.lock().expect("owned key lock").remove(&key);
                    }
                    Err(error) => {
                        if first_error.is_none() {
                            first_error = Some(error);
                        }
                    }
                }
            }
            match first_error {
                Some(error) => Err(FixtureError::with_source("exact-key cleanup failed", error)),
                None => Ok(()),
            }
        })
    }

    /// Prepares supported conditional requests through the independent SDK.
    fn prepare_write<'a>(
        &'a self,
        scenario: WriteScenario,
        relative: &'a str,
        bytes: &'a [u8],
    ) -> FixtureFuture<'a, FixturePreparation<WriteFixtureCase>> {
        Box::pin(async move {
            let path = self.path(relative)?;
            let options = match scenario {
                WriteScenario::Create | WriteScenario::Abort => {
                    WriteOptions::default().with_disposition(WriteDisposition::CreateNew)
                }
                WriteScenario::CreateConflict => {
                    let FixtureSupport::Supported(_) = self.seed_file(relative, b"a").await? else {
                        return Ok(FixturePreparation::Unavailable {
                            reason: "SDK seed unavailable".into(),
                        });
                    };
                    WriteOptions::default().with_disposition(WriteDisposition::CreateNew)
                }
                WriteScenario::IfAbsent => WriteOptions::default().with_precondition(WritePrecondition::IfAbsent),
                WriteScenario::IfMatch => {
                    let FixtureSupport::Supported(_) = self.seed_file(relative, b"before").await? else {
                        return Ok(FixturePreparation::Unavailable {
                            reason: "SDK seed unavailable".into(),
                        });
                    };
                    let FixtureSupport::Supported(version) = self.resource_version(&path).await? else {
                        return Ok(FixturePreparation::Unavailable {
                            reason: "SDK has no ETag".into(),
                        });
                    };
                    WriteOptions::default().with_precondition(WritePrecondition::IfMatch(version))
                }
                _ => {
                    return Ok(FixturePreparation::Unavailable {
                        reason: "single-PUT fixture does not support this write scenario".into(),
                    });
                }
            };
            Ok(FixturePreparation::Ready(WriteFixtureCase::new(
                path,
                bytes.to_vec(),
                options,
            )))
        })
    }

    /// Supplies a known noncurrent version; ETags observed from SDK remain
    /// separate.
    fn stale_resource_version<'a>(&'a self, path: &'a Path) -> FixtureFuture<'a, FixtureSupport<ResourceVersion>> {
        Box::pin(async move {
            let FixtureSupport::Supported(current) = self.resource_version(path).await? else {
                return Ok(FixtureSupport::Unsupported);
            };
            Ok(FixtureSupport::Supported(ResourceVersion::new(format!(
                "{}-stale",
                current.as_ref()
            ))))
        })
    }

    /// Arms an adapter boundary and supplies an independently observable
    /// target.
    fn prepare_write_cancellation<'a>(
        &'a self,
        stage: AsyncWriteCancellationStage,
        relative: &'a str,
    ) -> FixtureFuture<'a, FixtureSupport<Box<dyn WriteCancellationProbe>>> {
        Box::pin(async move {
            let path = self.path(relative)?;
            self.control.arm(match stage {
                AsyncWriteCancellationStage::Open => TestStage::Open,
                AsyncWriteCancellationStage::Write => TestStage::Write,
                AsyncWriteCancellationStage::Flush => TestStage::Flush,
                AsyncWriteCancellationStage::Commit => TestStage::BeforePut,
            });
            let key = self.object_path(&path)?;
            let probe: Box<dyn WriteCancellationProbe> = Box::new(S3WriteProbe {
                case: AsyncWriteFixtureCase::new(
                    path,
                    b"cancel probe".to_vec(),
                    WriteOptions::default().with_disposition(WriteDisposition::CreateNew),
                ),
                store: Arc::clone(&self.store),
                key,
                control: self.control.clone(),
            });
            Ok(FixtureSupport::Supported(probe))
        })
    }
}

/// Stage acknowledgement and target observation never consult facade state.
struct S3WriteProbe {
    case: AsyncWriteFixtureCase,
    store: Arc<dyn ObjectStore>,
    key: ObjectPath,
    control: TestControl,
}
impl WriteCancellationProbe for S3WriteProbe {
    fn case(&self) -> &AsyncWriteFixtureCase {
        &self.case
    }
    fn poll_reached(&self, cx: &mut std::task::Context<'_>) -> std::task::Poll<FixtureResult<()>> {
        self.control.poll_reached(cx).map(Ok)
    }
    fn accepted_bytes(&self) -> FixtureResult<u64> {
        Ok(self.control.accepted_bytes())
    }
    fn observe_target(&self) -> FixtureFuture<'_, Option<Vec<u8>>> {
        Box::pin(async move {
            match self.store.get(&self.key).await {
                Ok(result) => result
                    .bytes()
                    .await
                    .map(|bytes| Some(bytes.to_vec()))
                    .map_err(|error| FixtureError::with_source("SDK target body observation failed", error)),
                Err(object_store::Error::NotFound { .. }) => Ok(None),
                Err(error) => Err(FixtureError::with_source("SDK target observation failed", error)),
            }
        })
    }
    fn disarm(&self) -> FixtureResult<()> {
        self.control.release();
        Ok(())
    }
}
