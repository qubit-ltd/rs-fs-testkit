// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_fs::{
    FileSystemCapabilities,
    FileSystemId,
    FileSystemInfo,
    FileSystemLimits,
    FileSystemProperties,
    PathSemantics,
};

struct Properties {
    info: FileSystemInfo,
    limits: FileSystemLimits,
}

impl FileSystemProperties for Properties {
    fn info(&self) -> &FileSystemInfo {
        &self.info
    }

    fn capabilities(&self) -> FileSystemCapabilities {
        FileSystemCapabilities::default()
    }

    fn limits(&self) -> &FileSystemLimits {
        &self.limits
    }
}

/// Verifies the reusable properties contract accepts a valid provider.
#[test]
fn test_properties_contract_accepts_valid_identity() {
    let properties = Properties {
        info: FileSystemInfo::new(
            FileSystemId::new("test").expect("the ID should validate"),
            "test-provider",
            PathSemantics::Hierarchical,
        ),
        limits: FileSystemLimits::unknown(),
    };
    qubit_fs_testkit::assert_properties_contract(&properties);
}
