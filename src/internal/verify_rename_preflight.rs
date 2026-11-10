// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Validation for structured rename preflight rejection evidence.

use qubit_fs::error::FsError;
use qubit_fs::error::FsErrorKind;
use qubit_fs::error::FsOperation;
use qubit_fs::metadata::FileSystemCapability;
use qubit_fs::path::Path;
use qubit_fs::rename::RenameFailureState;

/// Checks all structured fields returned for a rename preflight rejection.
///
/// # Parameters
/// * `error` - Structured error returned by the public filesystem facade.
/// * `state` - Rename state retained by the failure.
/// * `source` - Requested source path.
/// * `target` - Requested target path.
/// * `provider` - Provider identifier from the suite's properties snapshot.
/// * `capability` - Capability required by the selected check.
///
/// # Returns
/// `true` only when the error and failure state match the unavailable request.
pub(crate) fn is_expected_unavailable_rename(
    error: &FsError,
    state: RenameFailureState,
    source: &Path,
    target: &Path,
    provider: &str,
    capability: FileSystemCapability,
) -> bool {
    let kind = if capability == FileSystemCapability::Rename {
        FsErrorKind::UnsupportedCapability
    } else {
        FsErrorKind::RequirementNotMet
    };
    error.kind() == kind
        && error.operation() == FsOperation::Rename
        && error.path() == Some(source)
        && error.target() == Some(target)
        && error.provider() == Some(provider)
        && error.required_capability() == Some(capability)
        && state == RenameFailureState::Unchanged
}

#[cfg(test)]
mod tests {
    use qubit_fs::error::FsError;
    use qubit_fs::error::FsErrorKind;
    use qubit_fs::error::FsOperation;
    use qubit_fs::metadata::FileSystemCapability;
    use qubit_fs::path::Path;
    use qubit_fs::rename::RenameFailureState;

    use super::is_expected_unavailable_rename;

    fn source() -> Path {
        Path::parse("/rename-source").expect("valid source")
    }

    fn target() -> Path {
        Path::parse("/rename-target").expect("valid target")
    }

    fn expected_error(source: &Path, target: &Path, capability: FileSystemCapability) -> FsError {
        let kind = if capability == FileSystemCapability::Rename {
            FsErrorKind::UnsupportedCapability
        } else {
            FsErrorKind::RequirementNotMet
        };
        FsError::new(kind, FsOperation::Rename, "preflight rejected rename")
            .with_path(source.clone())
            .with_target(target.clone())
            .with_provider("test-provider")
            .with_required_capability(capability)
    }

    #[test]
    fn test_accepts_complete_unavailable_rename_error() {
        let source = source();
        let target = target();
        let error = expected_error(&source, &target, FileSystemCapability::Rename);
        assert!(is_expected_unavailable_rename(
            &error,
            RenameFailureState::Unchanged,
            &source,
            &target,
            "test-provider",
            FileSystemCapability::Rename,
        ));
    }

    #[test]
    fn test_rejects_unavailable_rename_error_with_any_mismatched_field() {
        let source = source();
        let target = target();
        let wrong_source = Path::parse("/wrong-source").expect("valid path");
        let wrong_target = Path::parse("/wrong-target").expect("valid path");
        let valid = expected_error(&source, &target, FileSystemCapability::Rename);
        let wrong_kind = FsError::new(FsErrorKind::Io, FsOperation::Rename, "wrong kind")
            .with_path(source.clone())
            .with_target(target.clone())
            .with_provider("test-provider")
            .with_required_capability(FileSystemCapability::Rename);
        let wrong_operation = FsError::new(FsErrorKind::UnsupportedCapability, FsOperation::Stat, "wrong operation")
            .with_path(source.clone())
            .with_target(target.clone())
            .with_provider("test-provider")
            .with_required_capability(FileSystemCapability::Rename);
        let wrong_source_error = FsError::new(FsErrorKind::UnsupportedCapability, FsOperation::Rename, "wrong source")
            .with_path(wrong_source)
            .with_target(target.clone())
            .with_provider("test-provider")
            .with_required_capability(FileSystemCapability::Rename);
        let wrong_target_error = FsError::new(FsErrorKind::UnsupportedCapability, FsOperation::Rename, "wrong target")
            .with_path(source.clone())
            .with_target(wrong_target)
            .with_provider("test-provider")
            .with_required_capability(FileSystemCapability::Rename);
        let wrong_provider = FsError::new(
            FsErrorKind::UnsupportedCapability,
            FsOperation::Rename,
            "wrong provider",
        )
        .with_path(source.clone())
        .with_target(target.clone())
        .with_provider("other-provider")
        .with_required_capability(FileSystemCapability::Rename);
        let wrong_capability = FsError::new(
            FsErrorKind::UnsupportedCapability,
            FsOperation::Rename,
            "wrong capability",
        )
        .with_path(source.clone())
        .with_target(target.clone())
        .with_provider("test-provider")
        .with_required_capability(FileSystemCapability::Read);

        for error in [
            wrong_kind,
            wrong_operation,
            wrong_source_error,
            wrong_target_error,
            wrong_provider,
            wrong_capability,
        ] {
            assert!(!is_expected_unavailable_rename(
                &error,
                RenameFailureState::Unchanged,
                &source,
                &target,
                "test-provider",
                FileSystemCapability::Rename,
            ));
        }
        assert!(!is_expected_unavailable_rename(
            &valid,
            RenameFailureState::Renamed,
            &source,
            &target,
            "test-provider",
            FileSystemCapability::Rename,
        ));
    }
}
