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

契约套件的目标架构见[设计文档](doc/file_system_testkit_design.zh_CN.md)。

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

    fn list_prefix(
        &self,
        root: &Path,
        relative: &str,
    ) -> qubit_fs_testkit::FixtureResult<String> {
        Ok(format!("{}/{relative}", root.as_str().trim_end_matches('/')))
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

- 非空的 filesystem 和 provider 标识（两者可以相同）、稳定的属性快照、依赖一致的
  capability，以及 fixture 路径兼容性；
- 缺失路径的 `stat` 错误；已声明读取的调用方字节上限；写入；直接子项与带前缀的
  分页列目录；以及未声明 `Read`、`Write` 或 `List` 时的结构化预检错误；
- 已声明的目录创建、文件删除、复制、重命名，以及临时文件或目录的清理和持久化；
  这些类别中未声明的操作会被跳过，而不是断言；
- 回退与已声明服务端复制的结果报告，以及缺失路径和错误上下文的结构化检查。

`AsyncFileSystemContractSuite` 为核心操作提供对应的运行时无关检查，并对未声明的
创建、删除、重命名和临时资源操作验证结构化拒绝。provider 特有的路径编码、平台
行为、安全边界、服务注册和上述未列出的 capability 仍由 provider crate 负责。设计
文档说明的是预期的后续扩展，而非当前套件已经作出的保证。

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

# 强制执行已配置的逐源文件覆盖率阈值
COVERAGE_ENFORCE_THRESHOLDS=1 ./coverage.sh
```

默认只生成覆盖率报告。如需强制执行已配置的逐源文件阈值，请设置
`COVERAGE_ENFORCE_THRESHOLDS=1`。

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
