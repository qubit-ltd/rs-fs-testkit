// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use crate::ContractTempFailure;

#[test]
fn test_retained_resource_can_be_inspected_and_transferred() {
    let mut failure = ContractTempFailure::new(std::io::Error::other("temp"), 7_u8);
    assert_eq!(failure.error().to_string(), "temp");
    assert_eq!(*failure.resource(), 7);
    *failure.resource_mut() = 8;
    assert_eq!(*failure.resource(), 8);
    assert!(format!("{failure:?}").contains("retained"));
    assert!(format!("{failure}").contains("recovery resource"));
    assert_eq!(std::error::Error::source(&failure).unwrap().to_string(), "temp");
    let (error, resource) = failure.into_parts();
    assert_eq!(error.to_string(), "temp");
    assert_eq!(resource, 8);
}
