// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Tests the retained writer and original error recovery contract.

use crate::contract_writer_failure::ContractWriterFailure;

#[test]
fn test_retained_writer_can_be_inspected_and_transferred() {
    let mut failure = ContractWriterFailure::new(std::io::Error::other("writer"), 3_u8);
    assert_eq!(failure.error().to_string(), "writer");
    assert_eq!(*failure.writer(), 3);
    *failure.writer_mut() = 4;
    assert_eq!(*failure.writer(), 4);
    assert!(format!("{failure:?}").contains("retained"));
    assert!(format!("{failure}").contains("recovery writer"));
    assert_eq!(std::error::Error::source(&failure).unwrap().to_string(), "writer");
    let (error, writer) = failure.into_parts();
    assert_eq!(error.to_string(), "writer");
    assert_eq!(writer, 4);
}
