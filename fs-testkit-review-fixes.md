# rs-fs-testkit review fixes

## Scope addressed

- Expanded the synchronous in-memory conformance fixture to execute the full
  advertised core surface: read, write, list, directory creation, deletion,
  streamed copy, rename, temporary files, and temporary directories.
- Changed the asynchronous matrix to run `AsyncFileSystemContractSuite::assert_all`
  and added an isolated asynchronous fault that the full suite rejects.
- Added asynchronous phase assertions for advertised and unavailable list,
  directory creation, delete, rename, temporary-file, and temporary-directory
  operations, while retaining the native/read/write/commit cancellation matrix.
- Made `ContractContext` own recorded-path cleanup and made the synchronous
  suite record published resources for end-of-run cleanup.
- Strengthened temporary resource checks for files and directories: explicit
  cleanup, publication target, unadvertised required-atomic preflight,
  `NotPublished` failure state, and source cleanup responsibility.
- Removed the old uncompiled synchronous/asynchronous free-assertion and macro
  modules so the public stateful suites are the single contract implementation.
- Updated both READMEs to show the current typed fixture API and stateful
  synchronous/asynchronous suite entry points.

## Verification

Completed after the changes:

```text
cargo test --manifest-path rs-fs-testkit/Cargo.toml --test conforming_matrix_tests
3 passed

cargo test --manifest-path rs-fs-testkit/Cargo.toml --test async_matrix_tests
2 passed

cargo fmt --manifest-path rs-fs-testkit/Cargo.toml --check
passed

cargo test --manifest-path rs-fs-testkit/Cargo.toml --all-features
6 integration tests and the compile-fail doctest passed

cargo clippy --manifest-path rs-fs-testkit/Cargo.toml --all-targets --all-features -- -D warnings
passed

cargo doc --manifest-path rs-fs-testkit/Cargo.toml --no-deps --all-features
passed
```

## Final review follow-up

- The asynchronous suite now executes positive contracts for every advertised
  delete, rename, temporary-file, and temporary-directory capability, including
  observable postconditions rather than only unsupported-capability paths.
- Asynchronous read checks now consume the seeded bytes through the real reader;
  write checks publish bytes through a writer commit and observe the exact
  content through the fixture.
- `ContractContext::cleanup` now skips deletion entirely when `Delete` is not
  advertised, retaining fixture-owned resources instead of making an
  unconditional unsupported delete call.
- Added regression faults for wrong async read content, dropped write content,
  delete no-op, rename no-op, and temporary-cleanup no-op, plus a synchronous
  no-delete-capability cleanup regression test.

## Final verification

```text
cargo fmt --manifest-path rs-fs-testkit/Cargo.toml --check
passed

cargo clippy --manifest-path rs-fs-testkit/Cargo.toml --all-targets --all-features -- -D warnings
passed

cargo test --manifest-path rs-fs-testkit/Cargo.toml --all-features
7 integration tests and the compile-fail doctest passed

cargo doc --manifest-path rs-fs-testkit/Cargo.toml --no-deps --all-features
passed
```
