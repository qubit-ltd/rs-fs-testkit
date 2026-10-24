// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
/// The exact adapter stage held pending by a test.
///
/// # Examples
///
/// ```
/// use qubit_fs_s3_contract::TestStage;
///
/// let stage = TestStage::BeforePut;
/// assert_eq!(stage, TestStage::BeforePut);
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TestStage {
    /// The adapter is opening the object.
    Open,
    /// The adapter is writing object data.
    Write,
    /// The adapter is flushing object data.
    Flush,
    /// The adapter is about to dispatch a PUT request.
    BeforePut,
    /// The PUT completed before its result is observed.
    AfterPutBeforeResult,
    /// The adapter is processing an abort.
    Abort,
}
