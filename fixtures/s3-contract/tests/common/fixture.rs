//! Independent SDK-backed preparation, observation, and cleanup for S3 tests.

use std::sync::Arc;

use futures_util::TryStreamExt;
use object_store::ObjectStore;
use object_store::ObjectStoreExt;
use object_store::path::Path as ObjectPath;
use qubit_fs::AsyncFileSystem;
use qubit_fs::Path;
use qubit_fs::metadata::ResourceVersion;
use qubit_fs_s3_contract::S3ContractConfig;
use qubit_fs_s3_contract::open_with_store;
use qubit_fs_testkit::AsyncFileSystemFixture;
use qubit_fs_testkit::FixtureError;
use qubit_fs_testkit::FixtureFuture;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::FixtureSupport;

/// The facade and independent observer share only the underlying SDK store.
pub struct Fixture {
    pub filesystem: AsyncFileSystem,
    pub store: Arc<dyn ObjectStore>,
    pub config: S3ContractConfig,
}

impl Fixture {
    /// Creates an isolated in-memory namespace with independent SDK access.
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
        let filesystem = open_with_store(config.clone(), Arc::clone(&store)).unwrap();
        Self {
            filesystem,
            store,
            config,
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
        Path::parse_literal(relative).map_err(|error| FixtureError::with_source("fixture path failed", error))
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
    /// Deletes only keys under the fresh in-memory fixture namespace.
    fn teardown(&self) -> FixtureFuture<'_, ()> {
        Box::pin(async move {
            let prefix = ObjectPath::parse(&self.config.prefix).unwrap();
            let entries = self
                .store
                .list(Some(&prefix))
                .try_collect::<Vec<_>>()
                .await
                .map_err(|error| FixtureError::with_source("independent cleanup list failed", error))?;
            for entry in entries {
                self.store
                    .delete(&entry.location)
                    .await
                    .map_err(|error| FixtureError::with_source("independent cleanup delete failed", error))?;
            }
            Ok(())
        })
    }
}
