# qubit-fs-testkit

[![Rust CI](https://github.com/qubit-ltd/rs-fs-testkit/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-fs-testkit/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-fs-testkit/coverage-badge.json)](https://qubit-ltd.github.io/rs-fs-testkit/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-fs-testkit.svg?color=blue)](https://crates.io/crates/qubit-fs-testkit)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![中文文档](https://img.shields.io/badge/文档-中文版-blue.svg)](README.zh_CN.md)

`qubit-fs-testkit` 0.8 provides reusable, capability-driven contracts for
`qubit-fs` 0.3 provider implementations. Add it to a provider's development
dependencies, supply an isolated fixture, and run the synchronous or
runtime-neutral asynchronous suite without putting test support in production
dependency graphs.

## Installation

```bash
cargo add --dev qubit-fs-testkit@0.8
```

The synchronous contract suite is available without features. Add
`features = ["async"]` to the development dependency when using the
asynchronous suite.

## Quick Start

You are shipping a provider and need an isolated fixture to check its advertised
capabilities. The provider already depends on `qubit-fs` 0.3; add the testkit
only as a development dependency:

```toml
[dev-dependencies]
qubit-fs-testkit = "0.8"
```

Implement `FileSystemFixture` around the provider's isolated facade. Its
`file_system()` returns the configured `FileSystem` whose
`FileSystemProperties` the suite checks; `path()` maps suite names into the
fixture namespace, and independent hooks seed, read, observe, and tear down
resources. The complete executable example is
[`fs_testkit_contract.rs`](https://github.com/qubit-ltd/rs-fs-local/blob/main/examples/fs_testkit_contract.rs).
It shows a rooted local facade, independent native-file observations, and the
`run_all().assert_satisfied()` assertion. The command below builds and runs the
example; a provider contract violation makes that assertion fail:

```bash
cargo run --manifest-path ../rs-fs-local/Cargo.toml --example fs_testkit_contract
```

The example's fixture type is `RootedFixture`. A fresh suite checks the
provider's `FileSystemProperties` in its first phase; use a separate fixture
and suite for another run:

```rust,ignore
use qubit_fs_testkit::FileSystemContract;
use qubit_fs_testkit::FileSystemContractSuite;

let fixture = RootedFixture::new();
let mut suite = FileSystemContractSuite::new(&fixture);
suite
    .run_contract(FileSystemContract::Properties)
    .assert_satisfied();

let fixture = RootedFixture::new();
let mut suite = FileSystemContractSuite::new(&fixture);
suite.run_all().assert_satisfied();
```

For an asynchronous provider, implement `AsyncFileSystemFixture` and supply the
provider runtime's future runner to the registration macro. Its development
dependency enables the feature explicitly:

```toml
[dev-dependencies]
qubit-fs-testkit = { version = "0.8", features = ["async"] }
```

The suite follows advertised capabilities. It checks supported behavior and
structured rejection of unavailable operations. Inapplicable checks and missing
optional instrumentation remain explicit in the report.

The default assertion accepts a missing optional cancellation probe only when
the report records a non-empty `SkippedOptional` reason. This means the run's
required checks passed, while `run.all_applicable_checks_verified()` remains
false until every applicable optional probe has evidence. Use
`run.assert_satisfied_with(&[check_id])` when a particular optional probe is a
release requirement; any executed probe failure still fails the run.

## Why This Project Exists

Each `qubit-fs` provider must show that declared capabilities match real
filesystem behavior—preflight rejection, structured limits, publication
outcomes, and cleanup. Rebuilding that matrix in every crate duplicates effort
and drifts from the shared contract. `qubit-fs-testkit` centralizes
capability-driven checks behind fixtures and registration macros so validation
stays in development dependencies and out of production graphs.

## What It Provides

- `FileSystemFixture` and `AsyncFileSystemFixture` for an isolated facade and
  provider-specific path mapping; optional hooks support setup and observations
  outside the operation under test.
- Borrowed `run_all()` and `run_contract(...)` entry points on both suites.
  The suite retains the `ContractRun`, including check evidence and cleanup
  failures. Call `run.assert_satisfied()` to enforce the complete result.
- Registration macros that create one isolated, cleanup-safe test per named
  `FileSystemContract` phase.
- Contracts for facade properties, core operations, capability preflight,
  structured error context, cleanup, and supported optional operations. Both
  suites cover every `FileSystemCapability`, including ranges, conditions,
  checksums, representations, copy policies, required guarantees, and temporary
  resource persistence.
- `prepare_copy_cancellation` and `prepare_write_cancellation` provide optional
  stage-acknowledged probes. The suite verifies cancellation state, explicit
  recovery, and independent publication evidence. Missing probes can be
  required with `run.assert_satisfied_with(&[check_id])`; executed failures
  always fail the run.

Provider crates remain responsible for their own platform behavior, path
encoding, security boundaries, service registration, and capabilities outside
the suite's current coverage.

## Learn More

- [English user guide](doc/user_guide.md)
- [中文用户手册](doc/user_guide.zh_CN.md)
- [English design](doc/file_system_testkit_design.md)
- [中文设计](doc/file_system_testkit_design.zh_CN.md)
- [API documentation](https://docs.rs/qubit-fs-testkit)
- [中文 README](README.zh_CN.md)

Version 0.8 targets `qubit-fs` 0.3. Version 0.7 targeted `qubit-fs` 0.2;
historical list, cancellation, and recovery details are distinguished in the
[user guide](doc/user_guide.md).

## Testing

```bash
# Run tests with the default feature set
cargo test

# Run tests with all declared features
cargo test --all-features

# Project CI checks
./.infra/bin/ci-check.sh

# Check code coverage
./.infra/bin/coverage.sh
```

## License

Copyright (c) 2025 - 2026. Haixing Hu. All rights reserved.

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for the
full license text.

## Contributing

Contributions are welcome. Please follow the Rust API guidelines, keep public
API documentation and tests current, and run `./.infra/bin/align-ci.sh` to format code and
`./.infra/bin/ci-check.sh` to satisfy CI requirements before submitting a pull request.

## Author

**Haixing Hu** - *Qubit Co. Ltd.*

Repository: [https://github.com/qubit-ltd/rs-fs-testkit](https://github.com/qubit-ltd/rs-fs-testkit)
