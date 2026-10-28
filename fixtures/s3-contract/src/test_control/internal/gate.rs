// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use std::task::Waker;

use crate::TestStage;

/// State for a single explicitly armed adapter gate.
#[derive(Default)]
pub(in crate::test_control) struct Gate {
    /// Stage currently held by the gate, if any.
    pub(in crate::test_control) stage: Option<TestStage>,
    /// Whether an operation has reached the armed stage.
    pub(in crate::test_control) reached: bool,
    /// Waker for the observer waiting for gate arrival.
    pub(in crate::test_control) observer: Option<Waker>,
    /// Waker for the operation waiting for release.
    pub(in crate::test_control) operation: Option<Waker>,
}
