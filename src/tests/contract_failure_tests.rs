// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::error::Error;

use crate::ContractCheckId;
use crate::ContractFailure;

#[test]
fn test_panic_diagnostics_preserve_payload_variants_and_context() {
    let owned = ContractFailure::panicked("contract", Box::new(String::from("owned panic")));
    assert_eq!(owned.message(), "contract: owned panic");
    assert_eq!(
        owned
            .take_panic_payload()
            .unwrap()
            .downcast::<String>()
            .unwrap()
            .as_str(),
        "owned panic"
    );

    let borrowed = ContractFailure::panicked("contract", Box::new("borrowed panic"));
    assert_eq!(borrowed.message(), "contract: borrowed panic");
    assert_eq!(
        *borrowed.take_panic_payload().unwrap().downcast::<&str>().unwrap(),
        "borrowed panic"
    );

    let opaque = ContractFailure::panicked("contract", Box::new(7_u8));
    assert_eq!(opaque.message(), "contract: non-string panic payload retained");
    assert_eq!(*opaque.take_panic_payload().unwrap().downcast::<u8>().unwrap(), 7);
}

#[test]
fn test_diagnostics_format_check_and_keep_typed_source() {
    let failure = ContractFailure::with_source("operation failed", std::io::Error::other("io cause"))
        .at(ContractCheckId::WriteBasic);
    assert!(failure.to_string().contains("write/basic: operation failed"));
    assert_eq!(failure.source().unwrap().to_string(), "io cause");
    assert!(format!("{failure:?}").contains("operation failed"));

    let plain = ContractFailure::message_only("plain failure");
    assert_eq!(plain.to_string(), "plain failure");
    assert!(plain.source().is_none());
    assert!(plain.take_panic_payload().is_none());
}

#[test]
fn test_formatting_does_not_consume_a_retained_panic_payload() {
    let failure =
        ContractFailure::panicked("operation", Box::new(String::from("provider panic"))).at(ContractCheckId::ReadBasic);
    assert_eq!(failure.to_string(), "read/basic: operation: provider panic");
    assert!(format!("{failure:?}").contains("operation: provider panic"));
    assert_eq!(
        failure
            .take_panic_payload()
            .unwrap()
            .downcast::<String>()
            .unwrap()
            .as_str(),
        "provider panic"
    );
    assert!(failure.take_panic_payload().is_none());
}
