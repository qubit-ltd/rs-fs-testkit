// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shared in-memory provider data model used by sync and async fixtures.

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum FixtureHook {
    Snapshot,
    ReadFile,
    WriteFile,
    ResourceVersion,
    StaleResourceVersion,
    ChecksumFailureCase,
    SeedSymlink,
    CopyFastPathCase,
}

#[derive(Default)]
pub(crate) struct FixtureHookErrorPlan {
    target: Option<(FixtureHook, usize)>,
    calls: HashMap<FixtureHook, usize>,
}

impl FixtureHookErrorPlan {
    pub(crate) fn fail_at(&mut self, hook: FixtureHook, call: usize) {
        self.target = Some((hook, call));
    }

    pub(crate) fn should_fail(&mut self, hook: FixtureHook) -> bool {
        let call = self.calls.entry(hook).or_default();
        *call += 1;
        self.target == Some((hook, *call))
    }

    pub(crate) fn call_count(&self, hook: FixtureHook) -> usize {
        self.calls.get(&hook).copied().unwrap_or(0)
    }
}

#[derive(Clone)]
pub(crate) enum Entry {
    File(Vec<u8>),
    Directory,
    Symlink,
}
