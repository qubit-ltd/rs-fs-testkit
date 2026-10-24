// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use crate::ContractCheckId;
use crate::ContractCheckOutcome;
use crate::ContractFailure;
use crate::FileSystemContract;
use crate::FileSystemFixture;
use crate::common::MemoryFixture;
use crate::contract_context::ContractContext;

#[test]
fn test_fail_without_check_keeps_the_failure_without_changing_check_outcomes() {
    let fixture = MemoryFixture::new();
    let mut context = ContractContext::new(fixture.file_system().properties());
    let outcomes_before = context.report().checks().len();

    context.fail(ContractFailure::message_only("unattributed failure"));

    assert_eq!(context.run.failures.len(), 1);
    assert_eq!(context.run.failures[0].check(), None);
    assert_eq!(context.report().checks().len(), outcomes_before);
}

#[test]
fn test_phase_registration_and_attributed_failure_share_one_report() {
    let fixture = MemoryFixture::new();
    let mut context = ContractContext::new(fixture.file_system().properties());
    context.begin("read/basic");
    assert_eq!(context.current_contract(), "read/basic");
    assert!(context.relative_name("source").starts_with("read-basic-1-"));

    context.prepare_phase(FileSystemContract::Read, false);
    let check = context
        .report()
        .checks()
        .iter()
        .find(|check| check.id() == ContractCheckId::ReadBasic)
        .expect("read check registered before execution");
    assert!(matches!(check.outcome(), ContractCheckOutcome::NotRun { .. }));

    context.fail(ContractFailure::message_only("read evidence failed").at(ContractCheckId::ReadBasic));
    assert_eq!(context.run.failures.len(), 1);
    assert_eq!(context.run.failures[0].check(), Some(ContractCheckId::ReadBasic));
    let recorded = context
        .report()
        .checks()
        .iter()
        .find(|check| check.id() == ContractCheckId::ReadBasic)
        .expect("failed check remains registered");
    assert!(matches!(recorded.outcome(), ContractCheckOutcome::Failed { .. }));
}

#[cfg(feature = "async")]
#[test]
fn test_asynchronous_phase_registration_includes_async_only_checks() {
    let fixture = MemoryFixture::new();
    let mut context = ContractContext::new(fixture.file_system().properties());
    context.prepare_phase(FileSystemContract::Copy, true);

    let checks = context.report().checks();
    assert!(!checks.is_empty());
    assert!(checks.iter().any(|check| check.id() == ContractCheckId::CopyBasic));
    assert!(
        checks
            .iter()
            .any(|check| check.id() == ContractCheckId::AsyncCopyCancelCommit)
    );
}
