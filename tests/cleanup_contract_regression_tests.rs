//! Cleanup lifecycle regressions for contract suites.

mod common;

use common::MemoryFixture;
use qubit_fs_testkit::FileSystemContractSuite;

#[test]
fn finish_is_idempotent_after_successful_cleanup() {
    let fixture = MemoryFixture::new();
    let mut suite = FileSystemContractSuite::new(&fixture);
    suite.assert_write();
    suite.finish();
    suite.finish();
    assert!(fixture.is_empty());
}
