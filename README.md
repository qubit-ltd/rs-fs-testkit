# qubit-fs-testkit

[![Rust CI](https://github.com/qubit-ltd/rs-fs-testkit/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-fs-testkit/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-fs-testkit/coverage-badge.json)](https://qubit-ltd.github.io/rs-fs-testkit/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-fs-testkit.svg?color=blue)](https://crates.io/crates/qubit-fs-testkit)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![中文文档](https://img.shields.io/badge/文档-中文版-blue.svg)](README.zh_CN.md)

`qubit-fs-testkit` provides reusable provider contract checks for
[`qubit-fs`](https://crates.io/crates/qubit-fs) implementations. It belongs in
provider development dependencies, keeping reusable test support out of
production dependency graphs.

The intended contract-suite architecture is recorded in the
[Chinese design document](doc/file_system_testkit_design.zh_CN.md).

## Installation

Add the testkit as a development dependency:

```bash
cargo add --dev qubit-fs-testkit
```

## Fixture

Implement `FileSystemFixture` with an isolated filesystem and a mapping from
the testkit's non-empty `/`-separated relative paths to provider paths. Use
the optional seed and observation hooks when setup must be outside the
operation being checked.

```rust
use qubit_fs::{FileSystem, FileSystemId, Path};
use qubit_fs_local::RootedLocalFileSystem;
use qubit_fs_testkit::FileSystemFixture;

struct RootedFixture {
    _directory: tempfile::TempDir,
    file_system: RootedLocalFileSystem,
}

impl RootedFixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("create fixture root");
        let id = FileSystemId::new("contract-rooted").expect("valid ID");
        let file_system = RootedLocalFileSystem::open(id, directory.path())
            .expect("open rooted filesystem");
        Self {
            _directory: directory,
            file_system,
        }
    }
}

impl FileSystemFixture for RootedFixture {
    fn file_system(&self) -> &FileSystem {
        &self.file_system
    }

    fn path(&self, relative: &str) -> qubit_fs_testkit::FixtureResult<Path> {
        Path::parse(&format!("/{relative}"))
            .map_err(|error| qubit_fs_testkit::FixtureError::new(error.to_string()))
    }

    fn list_prefix(
        &self,
        root: &Path,
        relative: &str,
    ) -> qubit_fs_testkit::FixtureResult<String> {
        Ok(format!("{}/{relative}", root.as_str().trim_end_matches('/')))
    }
}
```

## Usage

Run the complete synchronous suite against a fresh fixture:

```rust
let fixture = RootedFixture::new();
qubit_fs_testkit::FileSystemContractSuite::new(&fixture).assert_all();
```

The suite follows the advertised capability surface. It verifies positive
behavior for advertised operations and structured rejection for unavailable
operations. `AsyncFileSystemContractSuite::assert_all` provides the matching
runtime-neutral asynchronous suite.

## Contracts

The synchronous suite checks:

- non-empty filesystem and provider identifiers (which may be equal), stable
  property snapshots, dependency-consistent capabilities, and fixture-path
  compatibility;
- missing-path `stat` errors; advertised reads with caller byte limits; writes;
  direct-child and prefixed paged listings; and their structured preflight
  errors when `Read`, `Write`, or `List` is not advertised;
- advertised directory creation, file deletion, copy, rename, and temporary
  file or directory cleanup and persistence; unsupported operations in these
  groups are skipped rather than asserted;
- fallback and advertised server-side copy reporting, plus structured missing
  path and error-context checks.

`AsyncFileSystemContractSuite` provides corresponding runtime-neutral checks
for the core operations, including structured rejection for unadvertised
create, delete, rename, and temporary-resource operations. Provider-specific
path encoding, platform behavior, security boundaries, service registration,
and capabilities not listed above remain the provider crate's responsibility.
The design document describes the intended future expansion; it is not a list
of guarantees made by the current suite.

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
