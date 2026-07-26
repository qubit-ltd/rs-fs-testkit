# Qubit FS Testkit

Reusable provider contract checks for implementations of
[`qubit-fs`](https://crates.io/crates/qubit-fs).

Use this crate as a development dependency. It keeps reusable test support out
of the filesystem core and provider production dependency graphs.

## Fixture

Implement `FileSystemFixture` with an isolated filesystem and a mapping from
the testkit's single-component relative names to provider paths. Create a fresh
fixture for every mutating assertion.

```rust
use qubit_fs::{FileSystem, FileSystemId, FsPath};
use qubit_fs_local::RootedLocalFileSystem;
use qubit_fs_testkit::FileSystemFixture;

struct RootedFixture {
    directory: tempfile::TempDir,
    file_system: RootedLocalFileSystem,
}

impl FileSystemFixture for RootedFixture {
    fn file_system(&self) -> &dyn FileSystem {
        &self.file_system
    }

    fn path(&self, relative: &str) -> FsPath {
        FsPath::parse(&format!("/{relative}")).expect("valid contract path")
    }
}

fn fixture() -> RootedFixture {
    let directory = tempfile::tempdir().expect("create fixture root");
    let id = FileSystemId::new("contract-rooted").expect("valid ID");
    let file_system =
        RootedLocalFileSystem::open(id, directory.path()).expect("open root");
    RootedFixture {
        directory,
        file_system,
    }
}
```

The provider test crate calls each contract as an independent test:

```rust
#[test]
fn properties_contract() {
    qubit_fs_testkit::assert_properties_contract(&fixture());
}

#[test]
fn read_contract() {
    qubit_fs_testkit::assert_read_contract(&fixture());
}
```

## Contracts

The synchronous suite checks:

- stable identity, limits, and capability dependencies;
- `stat` and `exists`;
- complete reads and caller byte limits;
- create, replace, and create-new writes;
- append and required atomic replacement;
- option preflight before provider I/O;
- structured errors for unadvertised operations.

Provider-specific path encoding, platform behavior, security boundaries, and
service registration remain the provider crate's responsibility. Asynchronous
filesystem contracts are not included yet.
