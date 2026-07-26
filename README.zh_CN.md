# Qubit FS Testkit

为 [`qubit-fs`](https://crates.io/crates/qubit-fs) 的实现提供可复用的
provider 契约测试。

请将本 crate 用作开发依赖，从而避免可复用测试支持进入文件系统核心和 provider
的生产依赖图。

## Fixture

实现 `FileSystemFixture`，提供一个隔离的文件系统，并把 testkit 给出的单层相对
名称映射为 provider 路径。每个会修改状态的断言都应创建全新的 fixture。

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

provider 的测试 crate 将每项契约作为独立测试调用：

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

## 契约范围

同步契约套件检查：

- 稳定的身份、限制和 capability 依赖；
- `stat` 和 `exists`；
- 完整读取和调用方字节上限；
- 创建、替换和 create-new 写入；
- 追加和强制原子替换；
- provider I/O 之前的选项预检；
- 未声明操作返回的结构化错误。

provider 特有的路径编码、平台行为、安全边界和服务注册仍由 provider crate 自己
负责。当前尚未包含异步文件系统契约。
