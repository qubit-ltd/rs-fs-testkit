// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Read preparation that deliberately provides only a range scenario.

use qubit_fs::FileSystem;
use qubit_fs::path::Path;
use qubit_fs_testkit::FileSystemFixture;
use qubit_fs_testkit::FixturePreparation;
use qubit_fs_testkit::FixtureResult;
use qubit_fs_testkit::ReadScenario;

use crate::common::MemoryFixture;

/// Keeps missing basic preparation separate from available range evidence.
pub(crate) struct SelectiveReadFixture {
    inner: MemoryFixture,
}

impl SelectiveReadFixture {
    pub(crate) fn new() -> Self {
        Self {
            inner: MemoryFixture::with_all_capabilities(),
        }
    }
}

impl FileSystemFixture for SelectiveReadFixture {
    fn file_system(&self) -> &FileSystem {
        self.inner.file_system()
    }
    fn path(&self, relative: &str) -> FixtureResult<Path> {
        self.inner.path(relative)
    }
    fn teardown(&self) -> FixtureResult<()> {
        self.inner.teardown()
    }
    fn prepare_read(
        &self,
        scenario: ReadScenario,
        relative: &str,
        bytes: &[u8],
    ) -> FixtureResult<FixturePreparation<Path>> {
        if scenario == ReadScenario::Range {
            self.inner.prepare_read(scenario, relative, bytes)
        } else {
            Ok(FixturePreparation::Unavailable {
                reason: "only the range scenario is prepared".to_owned(),
            })
        }
    }
}
