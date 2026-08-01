# qubit-fs-testkit

[![Rust CI](https://github.com/qubit-ltd/rs-fs-testkit/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-fs-testkit/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-fs-testkit/coverage-badge.json)](https://qubit-ltd.github.io/rs-fs-testkit/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-fs-testkit.svg?color=blue)](https://crates.io/crates/qubit-fs-testkit)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![English Document](https://img.shields.io/badge/Document-English-blue.svg)](README.md)

`qubit-fs-testkit` 为 `qubit-fs` provider 实现提供可复用、由 capability 驱动的契约测试。将它
作为 provider 的开发依赖，提供隔离 fixture，并运行同步或运行时无关的异步套件，无需将测试支持
放入生产依赖图。

## 安装

```bash
cargo add --dev qubit-fs-testkit
```

## 快速开始

对于能提供全新测试文件系统的 provider，请实现 `FileSystemFixture`，并注册具名同步契约：

```rust,ignore
qubit_fs_testkit::register_file_system_contract_tests! {
    module: provider_contracts,
    fixture: super::TestFixture::new,
}
```

对于异步 provider，请提供所用 runtime 的 future runner：

```rust,ignore
qubit_fs_testkit::register_async_file_system_contract_tests! {
    module: async_provider_contracts,
    fixture: super::AsyncTestFixture::new,
    runner: super::runtime::block_on,
}
```

套件按已声明 capability 运行：检查受支持操作的行为和不可用操作的结构化拒绝，同时跳过 provider
没有声明的可选操作。

## 提供的能力

- `FileSystemFixture` 和 `AsyncFileSystemFixture`：提供隔离门面与 provider 特有路径映射；可选
  hooks 可在被测操作之外执行准备和观察。
- `FileSystemContractSuite::new(&fixture).assert_all()` 与异步
  `AsyncFileSystemContractSuite` 对应方法，均按固定、依赖安全的工作流运行。
- 注册宏为每个具名 `FileSystemContract` 阶段生成独立 fixture、自动清理的测试。
- 覆盖门面属性、核心操作、capability 预检、结构化错误上下文、清理和已支持的可选操作的契约。
  同步与异步套件对全部 `FileSystemCapability` 保持对称覆盖，包括范围/条件读取、checksum、
  representation、copy policy、强化保证和临时资源持久化。
- `AsyncFileSystemContractSuite::assert_copy_cancellation()` 将异步 pending-stage 取消检查
  暴露为可独立运行的阶段。若 provider 无法控制自身 pending 阶段，`copy_cancellation_case`
  可以返回 `Unsupported`。

provider crate 仍需自行负责平台行为、路径编码、安全边界、服务注册，以及当前套件覆盖范围外的
capability。

## 延伸阅读

- [English user guide](doc/user_guide.md)
- [中文用户手册](doc/user_guide.zh_CN.md)
- [API 文档](https://docs.rs/qubit-fs-testkit)
- [English README](README.md)

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
