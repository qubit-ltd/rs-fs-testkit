use std::sync::Arc;

use crate::config::S3ContractConfig;
use crate::error_mapper;
use crate::path_mapper;
use crate::s3_reader::S3Reader;
use crate::s3_write_session::S3WriteSession;
use object_store::aws::AmazonS3Builder;
use object_store::{ObjectStore, ObjectStoreExt};
use qubit_fs::AsyncFileSystem;
use qubit_fs::error::{FsError, FsErrorKind, FsOperation};
use qubit_fs::metadata::{
    FileKind, FileMetadata, FileSystemCapabilities, FileSystemCapability, FileSystemId,
    FileSystemInfo, FileSystemLimit, FileSystemLimits, ResourceVersion, SymlinkPolicy,
};
use qubit_fs::path::PathConstraints;
use qubit_fs::path::PathSemantics;
use qubit_fs::spi::{
    AsyncFileSystemSpi, OpenReaderRequest, OpenWriterRequest, OpenedAsyncReader, OpenedAsyncWriter,
    ProviderOperation, ProviderOperations, ProviderProperties, SpiFuture, StatRequest,
    StatResponse,
};

pub struct S3FileSystemSpi {
    store: Arc<dyn ObjectStore>,
    config: S3ContractConfig,
    properties: ProviderProperties,
}

pub fn open(config: S3ContractConfig) -> Result<AsyncFileSystem, FsError> {
    let mut builder = AmazonS3Builder::new()
        .with_url(&config.endpoint)
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
    let info = FileSystemInfo::new(
        FileSystemId::new("s3-contract")?,
        "s3-contract",
        PathSemantics::ObjectKey,
    )
    .with_scheme("s3")?;
    let operations = ProviderOperations::new()
        .with(ProviderOperation::Stat)
        .with(ProviderOperation::OpenReader)
        .with(ProviderOperation::OpenWriter);
    let capabilities = FileSystemCapabilities::new()
        .with_conditional(FileSystemCapability::Read)
        .with_conditional(FileSystemCapability::RangeRead)
        .with_conditional(FileSystemCapability::ConditionalRead)
        .with_conditional(FileSystemCapability::Write)
        .with_conditional(FileSystemCapability::ConditionalWrite)
        .with_conditional(FileSystemCapability::Copy);
    let limits =
        FileSystemLimits::unknown().with_max_write_bytes(FileSystemLimit::Maximum(1_048_576));
    let properties = ProviderProperties::new(
        info,
        operations,
        capabilities,
        limits,
        PathConstraints::either(),
        SymlinkPolicy::Reject,
    )?;
    AsyncFileSystem::from_shared_spi(Arc::new(S3FileSystemSpi {
        store: Arc::from(store),
        config,
        properties,
    }))
}

impl AsyncFileSystemSpi for S3FileSystemSpi {
    fn properties(&self) -> ProviderProperties {
        self.properties.clone()
    }
    fn stat<'a>(
        &'a self,
        request: StatRequest<'a>,
    ) -> SpiFuture<'a, qubit_fs::FsResult<qubit_fs::spi::StatResponse>> {
        Box::pin(async move {
            let key = path_mapper::map(&self.config, request.path())?;
            let object_key = object_store::path::Path::parse(key).map_err(|e| {
                FsError::with_source(
                    FsErrorKind::InvalidPath,
                    FsOperation::Stat,
                    "invalid S3 object key",
                    e,
                )
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
            let reader =
                S3Reader::open(self.store.clone(), key, request.path().clone(), options).await?;
            let info = reader.info().clone();
            Ok(OpenedAsyncReader::new(info, Box::new(reader)))
        })
    }
    fn open_writer<'a>(
        &'a self,
        request: OpenWriterRequest<'a>,
    ) -> SpiFuture<'a, qubit_fs::FsResult<OpenedAsyncWriter>> {
        Box::pin(async move {
            let session = S3WriteSession::open(
                self.store.clone(),
                path_mapper::map(&self.config, request.path())?,
                request.options().options(),
            )?;
            let info = qubit_fs::metadata::OpenedFileInfo::new(
                self.properties.info().id().clone(),
                request.path().clone(),
            );
            Ok(OpenedAsyncWriter::new(info, Box::new(session)))
        })
    }
}
