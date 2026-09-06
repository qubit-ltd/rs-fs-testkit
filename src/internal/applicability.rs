// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
//! Pure applicability decisions shared by contract phase implementations.

use qubit_fs::metadata::FileSystemCapability;

use crate::FixtureCase;

/// Classifies a fixture case before any provider operation is attempted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Applicability {
    /// The provider capability is absent and a rejection probe is required.
    Unsupported,
    /// The fixture can prepare this conditional scenario.
    Supported,
    /// The provider advertises a capability but this fixture cannot prepare
    /// the requested conditional scenario.
    ConditionalUnavailable,
}

/// Converts a capability and fixture declaration into a three-state decision.
pub(crate) const fn classify(
    capability_supported: bool,
    case_support: bool,
) -> Applicability {
    match (capability_supported, case_support) {
        (false, _) => Applicability::Unsupported,
        (true, true) => Applicability::Supported,
        (true, false) => Applicability::ConditionalUnavailable,
    }
}

/// Returns whether a fixture case is conditional on fixture-specific setup.
pub(crate) const fn is_conditional(case: FixtureCase) -> bool {
    match case {
        FixtureCase::Capability(capability) => matches!(
            capability,
            FileSystemCapability::ConditionalRead
                | FileSystemCapability::ConditionalWrite
                | FileSystemCapability::ConditionalDelete
                | FileSystemCapability::ChecksumValidation
                | FileSystemCapability::ServerSideCopy
                | FileSystemCapability::AtomicFileCopy
                | FileSystemCapability::AtomicTreeCopy
                | FileSystemCapability::DurableFileCopy
                | FileSystemCapability::DurableTreeCopy
        ),
        FixtureCase::CopyOverwrite
        | FixtureCase::CopyTree
        | FixtureCase::ReadIfMatch
        | FixtureCase::ReadIfNoneMatch
        | FixtureCase::WriteIfAbsent
        | FixtureCase::WriteIfMatch
        | FixtureCase::DeleteIfMatch => true,
    }
}
