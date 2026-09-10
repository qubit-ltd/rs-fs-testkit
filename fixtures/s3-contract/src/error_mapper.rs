use object_store::Error as StoreError;
use qubit_fs::error::FsError;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;

pub fn map(error: StoreError, operation: FsOperation) -> FsError {
    let kind = match &error {
        StoreError::NotFound { .. } => FsErrorKind::NotFound,
        StoreError::Precondition { .. } => FsErrorKind::PreconditionFailed,
        StoreError::AlreadyExists { .. } => FsErrorKind::AlreadyExists,
        StoreError::Unauthenticated { .. } => FsErrorKind::AuthenticationFailed,
        StoreError::PermissionDenied { .. } => FsErrorKind::PermissionDenied,
        _ => FsErrorKind::Io,
    };
    FsError::with_source(kind, operation, "S3 operation failed", error)
}
