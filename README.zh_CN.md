# qubit-fs-testkit

[![Rust CI](https://github.com/qubit-ltd/rs-fs-testkit/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-fs-testkit/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-fs-testkit/coverage-badge.json)](https://qubit-ltd.github.io/rs-fs-testkit/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-fs-testkit.svg?color=blue)](https://crates.io/crates/qubit-fs-testkit)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![English Document](https://img.shields.io/badge/Document-English-blue.svg)](README.md)

`qubit-fs-testkit` 为
[`qubit-fs`](https://crates.io/crates/qubit-fs) provider 实现提供可复用的契约
测试。它应作为 provider 的开发依赖使用，避免测试支持进入生产依赖图。

## 安装

将 testkit 添加为开发依赖：

```bash
cargo add --dev qubit-fs-testkit
```

## Fixture

实现 `FileSystemFixture`，提供隔离的文件系统，并将 testkit 给出的非空、
以 `/` 分隔的相对路径映射为 provider 路径。宏生成的每项测试都会创建全新的
fixture。

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

## 使用

通过一次宏调用生成完整的同步契约套件：

```rust
qubit_fs_testkit::sync_file_system_contract_tests!(
    rooted,
    super::RootedFixture::new(),
);
```

完整套件要求 fixture 声明 `Read`、`Write`、`List`、`CreateDirectory`、
`Delete`、`Rename` 和 `Copy`。对于操作面较小的 provider，仍可单独调用
`assert_read_contract`、`assert_unsupported_operations_contract` 等断言。

## 契约范围

同步契约套件检查：

- 稳定的身份、限制以及所有派生 capability 依赖；
- `stat`、`exists`、完整读取和调用方字节上限；
- 创建、替换、create-new、追加和强制原子写入；
- 列目录、递归创建目录、删除、重命名和文件复制；
- 读取、写入、删除、重命名和复制选项在 provider I/O 前完成预检；
- 每项未声明同步操作（包括读取和写入）都返回结构化错误。

provider 特有的路径编码、平台行为、安全边界和服务注册仍由 provider crate
负责。当前尚未包含临时资源成功路径和异步契约。

## 测试

```bash
# 使用默认 feature 集运行测试
cargo test

# 使用项目声明的全部 feature 运行测试
cargo test --all-features

# 运行项目 CI 检查
./ci-check.sh

# 检查代码覆盖率
./coverage.sh
```

## 许可证

Copyright (c) 2025 - 2026. Haixing Hu. All rights reserved.

本项目基于 Apache License 2.0 授权。完整许可证文本请参阅
[LICENSE](LICENSE)。

## 贡献

欢迎贡献。请遵循 Rust API 指南，及时更新公共 API 文档与测试，并在提交
Pull Request 前运行 `./align-ci.sh`格式化代码，运行`./ci-check.sh`对齐CI要求。

## 作者

**Haixing Hu** - *Qubit Co. Ltd.*

仓库地址：[https://github.com/qubit-ltd/rs-fs-testkit](https://github.com/qubit-ltd/rs-fs-testkit)
