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

## Installation

Add the testkit as a development dependency:

```bash
cargo add --dev qubit-fs-testkit
```

## Fixture

Implement `FileSystemFixture` with an isolated filesystem and a mapping from
the testkit's non-empty `/`-separated relative paths to provider paths. Every
generated test creates a fresh fixture.

```rust
use qubit_fs::{FileSystem, FileSystemId, FsPath};
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
    fn file_system(&self) -> &dyn FileSystem {
        &self.file_system
    }

    fn path(&self, relative: &str) -> FsPath {
        FsPath::parse(&format!("/{relative}")).expect("valid contract path")
    }
}
```

## Usage

Generate the complete synchronous suite with one macro invocation:

```rust
qubit_fs_testkit::sync_file_system_contract_tests!(
    rooted,
    super::RootedFixture::new(),
);
```

The complete suite requires the fixture to advertise `Read`, `Write`, `List`,
`CreateDirectory`, `Delete`, `Rename`, and `Copy`. Individual assertions such
as `assert_read_contract` and `assert_unsupported_operations_contract` remain
available for providers with a smaller operation surface.

## Contracts

The synchronous suite checks:

- stable identity, limits, and all derived capability dependencies;
- `stat`, `exists`, complete reads, caller byte limits, and paged listings;
- create, replace, successful and conflicting create-new, append, and required
  atomic writes;
- listings with complete children even when metadata is absent; directory
  creation policies; recursive deletion; rename; and file and tree copy;
- positive advertised range, conditional, checksum-required, and server-side
  copy behavior, including ETag match and non-match transitions;
- per-contract structured option preflight for read, write, delete, and copy
  requirements that are not advertised;
- structured errors for every unadvertised synchronous operation, including
  reads and writes.

Provider-specific path encoding, platform behavior, security boundaries, and
service registration remain the provider crate's responsibility. Temporary
resource success paths and asynchronous contracts are not included yet. The
checksum contract confirms that `ChecksumPolicy::Required` can be honored; a
black-box fixture cannot independently inject storage corruption to prove a
provider's internal checksum implementation.

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
