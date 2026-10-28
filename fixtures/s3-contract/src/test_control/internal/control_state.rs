// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;

use super::gate::Gate;

/// Shared counters and gate state used by a test control handle.
#[derive(Default)]
pub(in crate::test_control) struct ControlState {
    /// Explicitly controlled adapter gate.
    pub(in crate::test_control) gate: Mutex<Gate>,
    /// Number of SDK PUT dispatches.
    pub(in crate::test_control) puts: AtomicUsize,
    /// Bytes acknowledged since the most recent arm.
    pub(in crate::test_control) accepted: AtomicUsize,
    /// Number of abort futures polled.
    pub(in crate::test_control) aborts: AtomicUsize,
    /// Whether the next abort should fail.
    pub(in crate::test_control) fail_abort: AtomicBool,
}
