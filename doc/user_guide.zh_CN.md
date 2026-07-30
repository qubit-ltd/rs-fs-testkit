# qubit-fs-testkit 用户手册

[English](user_guide.md) · [README](../README.zh_CN.md) · [API 文档](https://docs.rs/qubit-fs-testkit)

## 手册目标与读者

本手册面向同步或异步 `qubit-fs` provider 作者，覆盖当前 `qubit-fs-testkit` 0.1 契约套件。
它是测试支持，因此应作为 provider 的开发依赖使用。

## 概念模型

```text
provider 测试
   │
   ├─ 隔离的 FileSystemFixture ─────► FileSystemContractSuite
   │
   └─ 隔离的 AsyncFileSystemFixture ► AsyncFileSystemContractSuite
                                               │
                                               ▼
                                  capability 驱动的契约断言
```

fixture 暴露待测具体门面，并将 testkit 的非空、`/` 分隔相对名称映射为 provider 路径，同时映射
list prefix。可选 fixture hook 可预置/读取文件并准备 native-copy 用例。异步 fixture
提供对应的 future 观察，以及可选 copy cancellation 用例。

## 贯穿场景

你正在新增 provider，需要确认其已声明 capability 与可观察的文件系统行为一致。成功标准是一个全新、
隔离的 fixture 能完成套件；当支持 delete 时，套件还会清理其创建的测试资源。

## 安装与最小配置

在 provider crate 中将 testkit 添加为开发依赖：

```bash
cargo add --dev qubit-fs-testkit
```

为拥有或保留隔离文件系统资源的 fixture 实现 `FileSystemFixture`。至少实现 `file_system`、`path`
和 `list_prefix`。对于异步门面实现 `AsyncFileSystemFixture`；它有相同的必需映射方法，并要求 `Sync`。

## 核心工作流

在 provider 的集成测试中放入套件，并为每次测试运行新建 fixture：

```rust,ignore
use qubit_fs_testkit::{FileSystemContractSuite, FileSystemFixture};

let fixture = TestFixture::new();
FileSystemContractSuite::new(&fixture).assert_all();
```

同步与异步套件都会依次检查 properties、`stat`、read、write、list、创建目录、delete、copy、
rename、追加写、递归删除、必需原子 rename/replace、必需持久 copy、包括原子持久化在内的临时
资源和错误上下文，随后执行清理。门面未声明的核心操作会被检查是否返回结构化的
`UnsupportedCapability` 预检错误；未声明的强化保证会被检查是否返回结构化的
`RequirementNotMet` 预检错误。

对于异步门面，await 对应套件：

```rust,ignore
use qubit_fs_testkit::AsyncFileSystemContractSuite;

let fixture = AsyncTestFixture::new();
AsyncFileSystemContractSuite::new(&fixture).assert_all().await;
```

## 进阶用法

只有当通用套件需要在被测操作外进行 provider 所有的观察时，才实现可选 fixture hook。例如
`seed_file`、`read_file` 和 `copy_fast_path_case`。对于 provider 无法提供的可选观察，返回
`FixtureSupport::Unsupported`，而非伪造断言。

若直接单独调用阶段方法，结束后应调用 `finish()` 清理这些阶段创建的资源。`assert_all()` 会
自动调用它；同步套件在断言 panic 后重新抛出前也会执行清理。

`AsyncFileSystemFixture` 还提供 `copy_cancellation_case`，用于 provider 所有的 pending-stage 控制。
它与其他 provider 特有观察一样是可选的。

## 错误与诊断

套件以包含阶段信息的断言消息报告失败。当 capability 未被声明时，套件期望结构化的
`UnsupportedCapability` 错误，其中包含对应 operation 和 required capability context。fixture 映射
或 hook 失败会以 `FixtureError`/`FixtureResult` 失败呈现。

## 排障

| 现象 | 检查项 |
| --- | --- |
| properties 阶段失败 | 确保 ID 非空、capability 没有缺失依赖，且 fixture 路径符合门面约束。 |
| 未声明的核心操作导致失败 | 返回结构化 unsupported-capability 预检错误，而非成功或无关错误。 |
| 多次运行之间状态泄漏 | 创建隔离 fixture，并在套件期间保持其资源存活；仅在支持 delete 时尝试清理。 |
| 无法完成 provider 特有断言 | 保持相应可选 hook 为 unsupported，并为该行为添加 provider 自有测试。 |

## 限制与最佳实践

- 契约由 capability 驱动，并不宣称每个 provider 都具备相同 feature 集。
- 平台行为、路径编码、安全边界、服务注册和当前套件覆盖范围外的 capability，仍需由 provider
  自己测试。
- testkit 是开发依赖；不要将其加入 provider 的生产依赖面。

## 延伸阅读

- [README](../README.zh_CN.md)
- [English user guide](user_guide.md)
- [API 文档](https://docs.rs/qubit-fs-testkit)
