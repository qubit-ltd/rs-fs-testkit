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
以 `/` 分隔的相对路径映射为 provider 路径。若准备工作不应成为被测操作，
请使用可选的带外预置和观察钩子。

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
}
```

## 使用

针对新的 fixture 运行完整同步契约套件：

```rust
let fixture = RootedFixture::new();
qubit_fs_testkit::FileSystemContractSuite::new(&fixture).assert_all();
```

套件按已声明的 capability 面运行：验证已声明操作的正向行为，并验证不可用
操作的结构化拒绝。对应的运行时无关异步套件是
`AsyncFileSystemContractSuite::assert_all`。

## 契约范围

同步契约套件检查：

- 稳定的身份、所有派生 capability 依赖，以及可安全探测的有限路径、组件和写入上限；
- `stat`、`exists`、完整读取、调用方字节上限和分页列目录；
- 创建、替换、create-new 的成功与冲突路径、追加和强制原子写入；
- 即使缺少 metadata 也完整返回子项的列目录、目录创建策略、递归删除、重命名、文件和目录树复制；
- 已声明的范围读取、条件读写删除、必需校验和读取和服务端复制正向行为，包括 ETag 匹配和不匹配转换；
- provider-neutral 的文件/对象与目录/前缀表示，以及已声明临时资源的生命周期和原子持久化行为；
- 每项未声明的读取、写入、删除和复制需求都在 provider I/O 前完成结构化预检；
- 每项未声明同步操作（包括读取和写入）都返回结构化错误。

provider 特有的路径编码、平台行为、安全边界和服务注册仍由 provider crate
负责。符号链接和空目录表示检查需要 fixture 提供探针，因为核心 trait 没有创建
这两类资源的操作。异步契约作为独立断言提供，而不纳入同步宏。校验和契约确认
`ChecksumPolicy::Required` 可以兑现；通用黑盒 fixture 无法独立注入存储损坏，
因而不能证明 provider 内部校验和实现的细节。

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
