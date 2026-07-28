# rs-fs-testkit matrix report

## Completed

- Replaced the uncompiled legacy `FsPath` / trait-object memory provider with a current synchronous `FileSystemSpi` implementation using public request, opened-envelope, and temporary-session boundaries.
- Added the synchronous conforming-provider matrix and isolated fault matrix. The injected faults cover wrong file metadata kind, temporary cleanup retention, and temporary persistence target reporting.
- Extended `FileSystemContractSuite::assert_temp_resources` to verify successful temporary persistence reports and publishes the requested target.
- Added an error-formatting redaction regression test proving nested source diagnostics are not rendered through `FsError` display or debug output.
- Added an `AsyncFileSystemSpi` fixture that independently blocks the native copy attempt, fallback reader open, fallback writer transfer, and writer commit. The async matrix runs the suite's manual-poll cancellation checks across all four stages without creating a runtime.
- Corrected the suite's seeded stat byte-length expectation from 12 to 13 bytes.

## Verification

```text
cargo test --manifest-path rs-fs-testkit/Cargo.toml --test conforming_matrix_tests
3 passed

cargo test --manifest-path rs-fs-testkit/Cargo.toml --all-features
all tests and the compile-fail doctest passed

cargo clippy --manifest-path rs-fs-testkit/Cargo.toml --all-targets --all-features -- -D warnings
passed

cargo doc --manifest-path rs-fs-testkit/Cargo.toml --no-deps --all-features
passed
```
