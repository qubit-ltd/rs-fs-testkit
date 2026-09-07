# qubit-fs-testkit 用户手册

[English](user_guide.md) · [README](../README.zh_CN.md) · [API 文档](https://docs.rs/qubit-fs-testkit)

## 手册目标与读者

本手册面向同步或异步 `qubit-fs` provider 作者，覆盖当前 `qubit-fs-testkit` 0.4 契约套件。
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
list prefix。可选 fixture hook 可预置/读取文件、观察资源版本、预置空目录或 symlink，并准备
native-copy 用例。异步 fixture 提供对应的 future 观察，以及可选 copy cancellation 用例。

## 实战场景

你正在新增 provider，需要确认其已声明 capability 与可观察的文件系统行为一致。成功标准是一个全新、
隔离的 fixture 能完成套件；当支持 delete 时，套件还会清理其创建的测试资源。

## 安装与最小配置

在 provider crate 中将 testkit 添加为开发依赖：

```bash
cargo add --dev qubit-fs-testkit
```

为拥有或保留隔离文件系统资源的 fixture 实现 `FileSystemFixture`。至少实现 `file_system`、`path` 和独立的 `teardown`；
`list_prefix` 有默认实现。对于异步门面实现 `AsyncFileSystemFixture`；它有相同的必需映射方法，并要求
`Sync`。

## 核心工作流

使用注册宏为每个契约阶段创建全新 fixture 和精确测试名：

```rust,ignore
qubit_fs_testkit::register_file_system_contract_tests! {
    module: provider_contracts,
    fixture: super::TestFixture::new,
}
```

同步与异步套件都会依次检查 properties、`stat`、read、write、list、创建目录、delete、copy、
rename、追加写、递归删除、必需原子 rename/replace、必需持久 copy、包括原子持久化在内的临时
资源和错误上下文，随后执行清理。门面未声明的核心操作会被检查是否返回结构化的
`UnsupportedCapability` 预检错误。`Copy` 是例外：未声明 `Copy` 时，门面会跳过 native
fast path；当 `Read` 与 `Write` 可用且请求属于 allowlist 时，仍可使用流式 fallback，只有
fallback 前提缺失时才返回结构化的 unsupported-capability 错误。未声明的强化保证会被检查
是否返回结构化的 `RequirementNotMet` 预检错误。

读取预算针对本次选中的窗口，而不是整个资源。若打开时元数据提供总长度，套件会计算
`min(max(0, total_length - offset), requested_length)`；未指定 length 时使用扣除 offset
后的剩余长度，再将结果与 `max_bytes` 比较。例如，从 `0123456789` 读取
`offset = 2`、`length = 3`，且 `max_bytes = 3`，必须返回 `234`；预算为 `2` 时必须返回
`ResourceLimitExceeded`。总长度未知时不能据此预检拒绝，但实际流仍必须遵守预算。套件始终
先打开 reader，即使窗口长度为零也如此，因此 NotFound、权限和条件错误仍会保留。

writer 成功 commit 后再次 commit，应返回 `InvalidState`，并将 `WriteFailureState` 报告为
`Published`。它不得再次调用 provider commit，也不得自动 abort；已发布目标仍应可观察。
只有可重试的 `NotPublished` 失败仍允许再次 commit。

对于异步门面，传入 runtime 对应的 future runner：

```rust,ignore
qubit_fs_testkit::register_async_file_system_contract_tests! {
    module: async_provider_contracts,
    fixture: super::AsyncTestFixture::new,
    runner: super::runtime::block_on,
}
```

## 进阶用法

只有当通用套件需要在被测操作外进行 provider 所有的观察时，才实现可选 fixture hook。例如
`seed_file`、`read_file`、`resource_version`、`seed_empty_directory`、`seed_symlink`
和 `copy_fast_path_case`。对于 provider 无法提供的可选观察，返回
`FixtureSupport::Unsupported`，而非伪造断言。

`seed_file` 与 `read_file` 必须通过独立于待测门面的通道完成观察。例如，本地 provider 的 fixture 可
直接用原生文件系统 API 预置和检查隔离临时目录。若用同一个门面完成准备或观察，彼此匹配的读写缺陷可能
仍会通过契约套件。

使用 `prepare_read(ReadScenario, ...)` 和 `prepare_write(WriteScenario, ...)`
准备指定场景。准备成功时返回包含路径或请求的 `FixturePreparation::Ready`；无法提供证据时
返回 `Unavailable`；认为场景不适用时返回带原因的 `NotApplicable`，由套件判断。
实际准备错误必须保留为 `Err(FixtureError)`。缺少准备或无法提供观察，都不能证明声明的能力有效。

读取场景分别准备各自的内容。条件写入、原子替换、持久化写入和追加检查也独立准备：缺少基础创建
证据不会阻止这些检查执行。基础创建仍为 `Unverified` 时，整个运行依然不能通过验收。

默认写入准备提供新目标创建、持久化创建请求，并通过 `seed_file` 准备创建冲突、原子替换和追加目标。
`CreateConflict` 请求必须使用 `CreateNew`，目标已存在且其内容不同于待写入载荷。
`Replace` 默认也通过 `seed_file` 预置目标；如果该目标不适合 provider，可重写准备方法。
初始内容必须比新载荷更长，以便套件验证旧内容已被截断。
`IfAbsent` 与 `IfMatch` 需要重写准备方法：保持请求的载荷和条件，前者使用不存在的目标，
后者独立预置已有目标并读取当前版本。原子替换的初始内容必须不同于新内容；追加的初始内容必须非空。
套件会独立检查这些前提和发布内容，不接受 fixture 提供的更弱预期。

先构造 `let mut suite = FileSystemContractSuite::new(&fixture)`，再调用
`suite.run_contract(phase)` 或 `suite.run_all()`。返回的 `&ContractRun` 仍由套件持有。
使用 `run.assert_satisfied()` 同时检查执行、必需证据和清理；仅报告通过不能证明清理成功。
注册宏执行相同的严格策略。

每个套件只对应一次运行。已完成或被中断的会话不能再次执行，需创建新的 fixture 和套件。
异步套件由调用方选择 runtime 并 await 相同入口。运行 future 被丢弃后，可通过 `suite.run()`
检查保留结果，并用 `suite.finish().await` 重试清理。丢弃整个套件不会执行异步 I/O，
fixture 所有者必须安排独立 teardown。

独立的 `write/abort` 检查准备新的 `Abort` 场景，并在 abort 前后调用 `exists_out_of_band`。
`NotPublished` 要求目标仍不存在；`Published` 表示目标已改变；`Indeterminate` 保留不确定性。
套件也会检查对应的 writer 状态。流式写入或 abort 失败时，运行结果通过 `ContractSource`
保留 `ContractWriterFailure<W>`，其中 `W` 为 `FileWriter` 或 `AsyncFileWriter`。
取出 source 并向下转换后，可通过 `writer_mut()` 显式恢复，通过 `error()` 检查原始错误。
恢复成功不会把原先失败的运行变成通过。

已准备的异步整文件写入发生非预期失败时，保留 `ContractAsyncWriteFailure`。
其 `failure()` 保存发布状态和已接受字节数；`operation_mut()` 提供持有恢复 writer 的
operation，可调用 `take_recovery_writer()` 取出 writer 并显式 abort。
请求准入失败时没有 operation。取出 source 即转移恢复责任；丢弃它不会执行异步 abort。

复制探针的执行失败通过 `ContractAsyncCopyFailure` 同时保留原始 `AsyncCopyFailure`
快照和已准入的 operation。写入或复制在目标取消阶段之前失败时，运行器先丢弃执行 future、
解除探针，再保留 operation 供恢复使用。若恢复过程中的 abort 失败，
`ContractWriterFailure<AsyncFileWriter>` 会继续保留 writer，允许显式重试。

临时资源失败通过 `ContractTempFailure<T>` 保留，其中 `T` 是具体的同步或异步临时文件、
临时目录类型。取出 `ContractSource` 并向下转换后，可通过 `error()` 检查原始错误，
通过 `resource_mut()` 显式恢复。转移所有权不会清除运行失败；丢弃包装对象不会执行异步清理。

`TempAtomic` 独立执行强制原子持久化请求。普通临时资源检查使用优先原子性，分别验证
发布、清理、keep、命名选项及目录替换。若任何临时资源类型都不受支持，原子和重复生命周期
检查会明确记录为不适用。

`teardown` 是必需、独立且幂等的操作。即使 provider 没有声明 `Delete`，它也必须回收部分准备
产生的资源和暂存数据，包括套件未知的路径。独立 teardown 成功不能抹去门面清理的失败。

目录创建要求独立确认目标原先不存在。递归创建时，`exists_out_of_band` 必须观察新父目录
和子目录；仅报告子目录创建成功不能满足契约。套件还验证目录元数据，以及重复请求的
`already_existed` 结果。

`prepare_delete(DeleteScenario, relative, bytes)` 为基本删除或条件删除准备现有文件。
条件删除要求独立观察不同的当前版本和陈旧版本：陈旧版本请求必须拒绝且内容不变，
随后当前版本请求必须成功删除。忽略缺失目标的删除会在执行前后验证目标不存在。
递归删除通过 fixture 钩子准备目录及子文件，并逐层独立确认移除；准备过程不依赖
被测 provider 的目录创建操作。

空目录与符号链接检查相互独立。未声明能力时记录带原因的 `NotApplicable`，不会虚构
已执行的拒绝证据；声明能力却无法准备对应资源时保持 `Unverified`。

每项重命名检查分别准备不同的源和目标路径。冲突检查先确认拒绝后两者内容均未改变，
再验证显式覆盖。基本、原子及持久重命名成功后，除检查请求的保证及返回路径外，
还必须独立确认源已移除、目标内容完全正确。

列举检查分别准备命名空间。子树检查使用固定的目录及文件预期集合；分页检查独立准备
三个条目，并请求大小为一的分页提示。层次路径使用子树过滤，原始对象键使用
`ListOptions::object_keys()` 和字面前缀过滤；跨路径模型的过滤必须产生结构化拒绝。
结果收集有条目上限，重复或非预期条目不能造成无限增长。

属性中的路径限制探针通过门面提交超限 `stat` 请求，要求结构化的资源限制拒绝。
请求遵守套件的分配预算及声明的绝对／相对路径形式。若另一项路径限制会掩盖所选边界，
则记录带原因的可选探针跳过结果。

`prepare_copy_cancellation` 和 `prepare_write_cancellation` 返回可选的阶段确认探针。
探针必须确认提供者确实停在指定 pending 阶段；套件先丢弃执行 future，再解除门闩。
独立观察验证源内容保持及恢复过程中的发布声明。缺失的可选探针会明确记录，可通过
`run.assert_satisfied_with(&[check_id])` 强制要求指定探针；已执行的探针失败始终使运行失败。

仓库还提供一个独立且不可发布的真实远程后端验证 crate：`fixtures/s3-contract/`。它使用
S3 兼容 endpoint 和同一套公共 testkit，验证真实 range read、create-only write、冲突、取消
以及清理，并且不进入已发布 provider 的依赖图。一次本地 testkit 通过不能作为远程后端证据；
任何关于 S3 兼容性的结论都必须同时记录该 crate 的环境、后端版本、lockfile 和运行输出。
本手册只说明验证边界，不声称远程套件已经运行。

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
