# qubit-fs-testkit

[![Rust CI](https://github.com/qubit-ltd/rs-fs-testkit/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-fs-testkit/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-fs-testkit/coverage-badge.json)](https://qubit-ltd.github.io/rs-fs-testkit/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-fs-testkit.svg?color=blue)](https://crates.io/crates/qubit-fs-testkit)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![中文文档](https://img.shields.io/badge/文档-中文版-blue.svg)](README.zh_CN.md)

`qubit-fs-testkit` provides reusable, capability-driven contracts for
`qubit-fs` provider implementations. Add it to a provider's development
dependencies, supply an isolated fixture, and run the synchronous or
runtime-neutral asynchronous suite without putting test support in production
dependency graphs.

## Installation

```bash
cargo add --dev qubit-fs-testkit
```

## Quick Start

For a provider that exposes a fresh test filesystem, implement
`FileSystemFixture` and run every synchronous contract against it:

```rust,ignore
let fixture = TestFixture::new();
qubit_fs_testkit::FileSystemContractSuite::new(&fixture).assert_all();
```

For an asynchronous provider, implement `AsyncFileSystemFixture` and await the
matching suite:

```rust,ignore
let fixture = AsyncTestFixture::new();
qubit_fs_testkit::AsyncFileSystemContractSuite::new(&fixture)
    .assert_all()
    .await;
```

The suite follows advertised capabilities. It checks supported behavior and
structured rejection of unavailable operations, while skipping optional
operations the provider does not advertise.

## What It Provides

- `FileSystemFixture` and `AsyncFileSystemFixture` for an isolated facade and
  provider-specific path mapping; optional hooks support setup and observations
  outside the operation under test.
- `FileSystemContractSuite::new(&fixture).assert_all()` and the asynchronous
  `AsyncFileSystemContractSuite` counterpart, each running a fixed,
  dependency-safe workflow.
- Contracts for facade properties, core operations, capability preflight,
  structured error context, cleanup, and supported optional operations. Both
  suites verify advertised append, recursive deletion, required-atomic
  rename/replacement, required-durable copy, and atomic temporary-resource
  persistence.

Provider crates remain responsible for their own platform behavior, path
encoding, security boundaries, service registration, and capabilities outside
the suite's current coverage.

## Learn More

- [English user guide](doc/user_guide.md)
- [中文用户手册](doc/user_guide.zh_CN.md)
- [API documentation](https://docs.rs/qubit-fs-testkit)
- [中文 README](README.zh_CN.md)

## Testing

```bash
# Run tests with the default feature set
cargo test

# Run tests with all declared features
cargo test --all-features

# Project CI checks
./ci-check.sh

# Check code coverage
./coverage.sh
```

## License

Copyright (c) 2025 - 2026. Haixing Hu. All rights reserved.

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for the
full license text.

## Contributing

Contributions are welcome. Please follow the Rust API guidelines, keep public
API documentation and tests current, and run `./align-ci.sh` to format code and
`./ci-check.sh` to satisfy CI requirements before submitting a pull request.

## Author

**Haixing Hu** - *Qubit Co. Ltd.*

Repository: [https://github.com/qubit-ltd/rs-fs-testkit](https://github.com/qubit-ltd/rs-fs-testkit)
