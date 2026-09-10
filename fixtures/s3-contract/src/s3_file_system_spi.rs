use std::sync::Arc;

use object_store::ObjectStore;
use object_store::ObjectStoreExt;
use object_store::aws::AmazonS3Builder;
use qubit_fs::AsyncFileSystem;
use qubit_fs::error::FsError;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileKind;
use qubit_fs::metadata::FileMetadata;
use qubit_fs::metadata::FileSystemCapabilities;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::metadata::FileSystemId;
use qubit_fs::metadata::FileSystemInfo;
use qubit_fs::metadata::FileSystemLimit;
use qubit_fs::metadata::FileSystemLimits;
use qubit_fs::metadata::ResourceVersion;
use qubit_fs::metadata::SymlinkPolicy;
use qubit_fs::path::PathConstraints;
use qubit_fs::path::PathSemantics;
use qubit_fs::spi::AsyncFileSystemSpi;
use qubit_fs::spi::ListRequest;
use qubit_fs::spi::OpenReaderRequest;
use qubit_fs::spi::OpenWriterRequest;
use qubit_fs::spi::OpenedAsyncDirectoryStream;
use qubit_fs::spi::OpenedAsyncReader;
use qubit_fs::spi::OpenedAsyncWriter;
use qubit_fs::spi::ProviderOperation;
use qubit_fs::spi::ProviderOperations;
use qubit_fs::spi::ProviderProperties;
use qubit_fs::spi::SpiFuture;
use qubit_fs::spi::StatRequest;
use qubit_fs::spi::StatResponse;

use crate::TestControl;
use crate::TestStage;
use crate::config::S3ContractConfig;
use crate::error_mapper;
use crate::path_mapper;
use crate::s3_directory_stream::S3DirectoryStream;
use crate::s3_directory_stream::filter as list_filter;
use crate::s3_reader::S3Reader;
use crate::s3_write_session::S3WriteSession;

pub struct S3FileSystemSpi {
    store: Arc<dyn ObjectStore>,
    config: S3ContractConfig,
    properties: ProviderProperties,
    control: TestControl,
}

pub fn open(config: S3ContractConfig) -> Result<AsyncFileSystem, FsError> {
    let mut builder = AmazonS3Builder::new()
        .with_endpoint(&config.endpoint)
        .with_allow_http(config.allow_http)
        .with_bucket_name(&config.bucket)
        .with_region(&config.region)
        .with_access_key_id(&config.access_key_id)
        .with_secret_access_key(&config.secret_access_key);
    builder = builder.with_retry(object_store::RetryConfig {
        max_retries: 0,
        ..Default::default()
    });
    let store = builder.build().map_err(|e| {
        FsError::with_source(
            FsErrorKind::InvalidOptions,
            FsOperation::Provider,
            "invalid S3 configuration",
            e,
        )
    })?;
    open_with_store(config, Arc::from(store))
}

/// Builds a deterministic adapter backed by object_store's in-memory store.
///
/// This is used by the local contract matrix; the ignored test remains the
/// end-to-end harness for a real S3-compatible service.
pub fn open_in_memory(prefix: impl Into<String>) -> Result<AsyncFileSystem, FsError> {
    let config = S3ContractConfig {
        endpoint: "memory://".into(),
        bucket: "contract".into(),
        region: "local".into(),
        access_key_id: "test".into(),
        secret_access_key: "test".into(),
        prefix: prefix.into(),
        allow_http: false,
    };
    open_with_store(config, Arc::new(object_store::memory::InMemory::new()))
}

pub fn open_with_store(config: S3ContractConfig, store: Arc<dyn ObjectStore>) -> Result<AsyncFileSystem, FsError> {
    open_with_control(config, store, TestControl::default())
}

/// Builds an adapter with explicitly supplied fixture-only stage control.
pub fn open_with_control(
    config: S3ContractConfig,
    store: Arc<dyn ObjectStore>,
    control: TestControl,
) -> Result<AsyncFileSystem, FsError> {
    path_mapper::configured_prefix(&config)?;
    let info = FileSystemInfo::new(
        FileSystemId::new("s3-contract")?,
        "s3-contract",
        PathSemantics::ObjectKey,
    )
    .with_scheme("s3")?;
    let operations = ProviderOperations::new()
        .with(ProviderOperation::Stat)
        .with(ProviderOperation::List)
        .with(ProviderOperation::OpenReader)
        .with(ProviderOperation::OpenWriter);

    let capabilities = FileSystemCapabilities::new()
        .with_conditional(FileSystemCapability::List)
        .with_conditional(FileSystemCapability::Read)
        .with_conditional(FileSystemCapability::RangeRead)
        .with_conditional(FileSystemCapability::Write)
        .with_conditional(FileSystemCapability::ConditionalWrite);
    let limits = FileSystemLimits::unknown().with_max_write_bytes(FileSystemLimit::Maximum(1_048_576));
    let properties = ProviderProperties::new(
        info,
        operations,
        capabilities,
        limits,
        PathConstraints::either(),
        SymlinkPolicy::Reject,
    )?;
    AsyncFileSystem::from_shared_spi(Arc::new(S3FileSystemSpi {
        store,
        config,
        properties,
        control,
    }))
}

impl AsyncFileSystemSpi for S3FileSystemSpi {
    fn properties(&self) -> ProviderProperties {
        self.properties.clone()
    }

    fn list<'a>(&'a self, request: ListRequest<'a>) -> SpiFuture<'a, qubit_fs::FsResult<OpenedAsyncDirectoryStream>> {
        Box::pin(async move {
            let prefix = path_mapper::configured_prefix(&self.config)?;
            let filter = list_filter(request.options())?;
            // SDK listing uses component prefixes. Query only the configured
            // namespace here, then apply raw caller prefix matching locally.
            let stream = self.store.list(Some(&prefix));
            Ok(OpenedAsyncDirectoryStream::new(Box::new(S3DirectoryStream::new(
                self.config.clone(),
                request.scope().clone(),
                stream,
                filter,
                request.options().options().include_metadata(),
            ))))
        })
    }
    fn stat<'a>(&'a self, request: StatRequest<'a>) -> SpiFuture<'a, qubit_fs::FsResult<qubit_fs::spi::StatResponse>> {
        Box::pin(async move {
            let key = path_mapper::map(&self.config, request.path())?;
            let object_key = object_store::path::Path::parse(key).map_err(|e| {
                FsError::with_source(FsErrorKind::InvalidPath, FsOperation::Stat, "invalid S3 object key", e)
            })?;
            let meta = self
                .store
                .head(&object_key)
                .await
                .map_err(|e| error_mapper::map(e, FsOperation::Stat))?;
            let mut value = FileMetadata::new(FileKind::Object).with_len(Some(meta.size as u64));
            if let Some(etag) = meta.e_tag {
                value = value.with_etag(Some(ResourceVersion::new(etag)));
            }
            Ok(StatResponse::new(request.path().clone(), value))
        })
    }
    fn open_reader<'a>(
        &'a self,
        request: OpenReaderRequest<'a>,
    ) -> SpiFuture<'a, qubit_fs::FsResult<OpenedAsyncReader>> {
        Box::pin(async move {
            let key = path_mapper::map(&self.config, request.path())?;
            let options = request.options().options();
            let reader = S3Reader::open(self.store.clone(), key, request.path().clone(), options).await?;
            let info = reader.info().clone();
            Ok(OpenedAsyncReader::new(info, Box::new(reader)))
        })
    }
    fn open_writer<'a>(
        &'a self,
        request: OpenWriterRequest<'a>,
    ) -> SpiFuture<'a, qubit_fs::FsResult<OpenedAsyncWriter>> {
        Box::pin(async move {
            self.control.wait(TestStage::Open).await;
            let session = S3WriteSession::open(
                self.store.clone(),
                path_mapper::map(&self.config, request.path())?,
                request.options().options(),
                self.control.clone(),
            )?;
            let info =
                qubit_fs::metadata::OpenedFileInfo::new(self.properties.info().id().clone(), request.path().clone());
            Ok(OpenedAsyncWriter::new(info, Box::new(session)))
        })
    }
}
