# Qubit FS Testkit 设计

> 状态：已批准的目标设计。本文定义 `qubit-fs-testkit` 在具体门面/SPI 重构后的
> provider contract 测试架构；当前实现迁移前可能与本文不同。

## 1. 定位

`qubit-fs-testkit` 是 provider 的 development dependency。它通过
`qubit-fs` 的公开门面验证 provider-neutral 契约，不进入生产依赖图。

```text
provider fixture
      │
      ▼
FileSystemContractSuite
      │ 只调用 public facade
      ▼
FileSystem / AsyncFileSystem
      │
      ▼
provider SPI 与实际实现
```

Testkit 不构造 `qubit_fs::spi::*Request`，也不直接调用 operation SPI。

## 2. 目标与非目标

目标：

1. 为每个 provider 提供可复用的同步/异步黑盒 contract suite；
2. 验证 properties、capability、limit、operation、outcome、error 和 lifecycle；
3. 根据 capability 快照决定适用测试；
4. 同时验证未声明 capability 会在副作用前被门面拒绝；
5. 支持 out-of-band seed、observation 和 provider-specific path mapping；
6. 用故意错误的 provider 自证每条 contract 确实能检测违规；
7. 所有 assertion 由统一 suite 类型的方法组织，不暴露散落 free function。

非目标：

- 测试 provider 内部算法实现细节；
- 绕过门面测试 raw SPI；
- 代替 provider 的平台、协议、性能或安全测试；
- 在 testkit 中绑定所有 async provider 必须使用的 runtime；
- 通过宏承载或复制 contract 执行逻辑；
- 要求只支持部分能力的 provider 为测试伪造额外生产能力。

## 3. Fixture

### 3.1 同步 fixture

```rust
pub trait FileSystemFixture {
    fn file_system(&self) -> &FileSystem;
    fn path(&self, relative: &str) -> FsPath;

    fn list_prefix(
        &self,
        root: &FsPath,
        relative: &str,
    ) -> String;

    fn seed_file(
        &self,
        relative: &str,
        bytes: &[u8],
    ) -> Option<FsPath>;

    fn read_file(
        &self,
        path: &FsPath,
    ) -> Option<Vec<u8>>;

    fn empty_directory_path(&self) -> Option<FsPath>;
    fn symlink_path(&self) -> Option<FsPath>;
}
```

关键变化是 `file_system()` 返回 `&FileSystem`，不再返回 `&dyn FileSystem`。

Fixture 负责：

- 为一次 suite run 提供隔离 namespace；
- 把 testkit relative name 映射为 provider path；
- 在必要时提供不依赖被测 capability 的 setup/observation；
- 在 drop 或显式 teardown 中释放测试资源；
- 明确无法提供的 representation probe。

Fixture 不负责重新解释 capability 或放宽 contract。

### 3.2 异步 fixture

```rust
pub trait AsyncFileSystemFixture: Sync {
    fn file_system(&self) -> &AsyncFileSystem;
    fn path(&self, relative: &str) -> FsPath;
    // 对应的异步 setup/observation hooks
}
```

Async fixture hook 返回 runtime-neutral future。Testkit 不创建全局 runtime。

## 4. Stateful contract suite

删除 public `assert_*` free function。所有 contract 逻辑都进入 suite 方法。

同步 suite：

```rust
pub struct FileSystemContractSuite<'a> {
    fixture: &'a dyn FileSystemFixture,
}

impl<'a> FileSystemContractSuite<'a> {
    pub fn new(
        fixture: &'a dyn FileSystemFixture,
    ) -> Self;

    pub fn assert_all(&self);
    pub fn assert_properties(&self);
    pub fn assert_stat(&self);
    pub fn assert_read(&self);
    pub fn assert_write(&self);
    pub fn assert_list(&self);
    pub fn assert_create_directory(&self);
    pub fn assert_delete(&self);
    pub fn assert_copy(&self);
    pub fn assert_rename(&self);
    pub fn assert_temp_resources(&self);
    pub fn assert_error_context(&self);
}
```

异步 suite：

```rust
pub struct AsyncFileSystemContractSuite<'a> {
    fixture: &'a dyn AsyncFileSystemFixture,
}
```

它提供同名 async assertion methods。调用者自行使用所选 runtime 的 `#[test]` /
`#[tokio::test]` 等入口。

Provider 可以运行完整 suite：

```rust
#[test]
fn test_file_system_contract() {
    let fixture = RootedFixture::new();
    FileSystemContractSuite::new(&fixture).assert_all();
}
```

也可以为受限 provider 单独运行适用方法。

可选薄 macro 命名为 `register_file_system_contract_tests!`，为 Rust test harness
生成多个独立 `#[test]` wrapper，以获得并行执行和精确测试名称。该 macro 只能调用
suite 方法，不能包含 assertion、fixture mutation 或 capability 判断；不使用 macro
的 provider 必须能获得完全相同的 contract coverage。

## 5. Suite 运行上下文

Suite 在构造时读取一次门面 properties snapshot，并为每个 contract 分配唯一相对
namespace。它内部维护：

- fixture reference；
- properties snapshot reference/clone；
- 唯一测试 name generator；
- 已创建资源清单；
- 当前 contract 名称，用于失败信息。

这些状态说明 suite 应为 struct，而不是零变体 enum。

每个 assertion：

1. 创建独立测试资源；
2. 执行 precondition setup；
3. 通过门面调用被测操作；
4. 检查 side effect、outcome、metadata 和 error；
5. 显式完成需要确认的 writer/temp cleanup；
6. 清理本次 contract 资源；
7. 产生包含 provider、contract、path 和预期/实际差异的失败消息。

## 6. Capability-driven contract

Capability 处理遵循两个方向。

### 6.1 已声明 capability

只要 provider 声明 capability，suite 就验证对应正向语义。例如：

- `Read`：完整读取与 open identity；
- `RangeRead`：range 边界与返回长度；
- `ConditionalRead`：match/non-match；
- `Write`、`Append`、`ConditionalWrite`；
- `List` 与 page-size hint；
- `CreateDirectory`；
- `Delete`、`RecursiveDelete`、`ConditionalDelete`；
- `Rename`、`AtomicRename`、`AtomicReplace`；
- `Copy`、`ServerSideCopy`；
- `TempFile`、`TempDirectory`、`AtomicTempPersist`；
- checksum validation。

声明能力但只能返回 `UnsupportedCapability` 视为 contract failure。

### 6.2 未声明 capability

Suite 构造要求该能力的公开 options，并验证：

- 门面在 provider side effect 前返回 `UnsupportedCapability` 或
  `RequirementNotMet`；
- error 携带准确 `required_capability`；
- source 和 target 保持不变；
- provider 不通过 silent fallback 返回较弱 success。

黑盒 fixture 无法直接观察 SPI call count 时，通过 out-of-band observation 验证无
side effect。门面“SPI 完全未调用”的白盒保证由 `qubit-fs` 自身 recording SPI 测试。

## 7. Properties 与 limits contract

`assert_properties` 验证：

- filesystem id 和 provider id 有效且不同概念不混用；
- properties getter 不执行 I/O；
- facade clone 观察同一不可变 snapshot；
- capabilities dependency 自洽；
- path semantics 与 fixture mapping 相容；
- finite limit 可被安全探测；
- unknown/inapplicable/unbounded 不被误当作数值；
- provider diagnostics 不包含 credential-like key。

Limit contract 至少覆盖：

- path text bytes；
- component text bytes；
- read range bytes；
- write bytes；
- list page entries。

测试只在可安全构造边界输入时探测，不通过可能耗尽 CI 资源的巨大分配验证 limit。

## 8. I/O 与 namespace contract

### 8.1 Metadata 与 read

验证：

- `stat` 的 kind、size、location 和 final-symlink 语义；
- `exists` 只吞掉 `NotFound`；
- reader opened location 与请求一致；
- open-time metadata 是 snapshot；
- `read_all` 同时遵守 caller limit 和 filesystem limit；
- range、condition 与 checksum requirement。

### 8.2 Write

验证：

- create、replace、create-new 和 append；
- write precondition；
- required atomicity；
- `WriteOutcome` 的 method/atomicity/version；
- commit failure state；
- abort 不回滚已 published target；
- `write_all` failure 保留需要恢复的 writer。

### 8.3 List 与 directory

验证：

- direct children 与 recursive/prefix 规则；
- entry path 不离开请求 root；
- metadata absent 与 unknown 的区别；
- page-size hint 不突破 provider limit；
- stream 中途错误进入终止 failed state；
- provider contract violation 被门面识别。

### 8.4 Namespace mutation

验证：

- create-directory policies；
- file/directory delete；
- recursive delete 不遗留 child；
- copy 不删除 source；
- rename 成功后 source 消失；
- type conflict、overwrite 和 precondition；
- required atomicity 不降级；
- outcome source/target context 完整。

## 9. Temporary resource contract

同步与异步 suite 都覆盖：

- temp resource 绑定创建它的原始 filesystem 门面；
- `Owned → Persisted/Kept/Cleaned`；
- `NotPublished → Owned`；
- `Published → Persisted`；
- `PublishedSourceRetained → CleanupRequired`；
- `Indeterminate → Indeterminate`；
- `CleanupRequired` drop 只清理 source；
- `Indeterminate` drop 不自动修改 source/target；
- persist required atomicity；
- child/descendant path 安全；
- async lifecycle future cancellation。

需要注入 failure 的状态主要由 `qubit-fs` fake SPI 测试。Provider 黑盒 suite 在 provider
声明相应 fixture fault hook 时复用这些 contract；不要求生产 provider 暴露故障注入。

## 10. Error contract

每个操作都验证结构化错误，而不只断言 `is_err()`：

- `FsErrorKind`；
- `FsOperation`；
- source path；
- target path；
- provider id；
- required capability；
- indeterminate/partial-progress state；
- source error 是否保留；
- `Display` / `Debug` 不泄漏 secret。

Provider 返回非法 opened location、entry 或 outcome 时，门面必须产生
`ProviderContractViolation`。

## 11. 同步与异步一致性

同步和异步 suite 共享：

- test data model；
- capability applicability；
- expected outcome/error 值；
- lifecycle transition table；
- fixture path naming rules。

I/O driver 和 cancellation assertion 分开实现。公共 API 不通过宏生成，以免同步和异步
差异被隐藏。

异步 suite 特别验证：

- open 本身是异步的；
- future 不借用短生命周期临时配置；
- cancellation 后 handle state 正确；
- drop 不启动 executor 或阻塞；
- stream/backpressure 行为符合 `qubit-io` contract。

## 12. Testkit 自验证

Testkit 必须包含 conforming provider 和一组最小 broken provider：

- unstable/inconsistent properties；
- ignored option；
- late preflight side effect；
- wrong opened location；
- wrong file kind；
- list entry 越界；
- truncated/invalid stream；
- atomicity downgrade；
- copy 删除 source；
- rename 保留 source；
- recursive delete 遗留 child；
- invalid error context；
- writer/temp failure state 错误；
- secret 泄漏；
- async cancellation state 错误。

每个 broken provider 只破坏一个 contract，相应 self-test 必须证明 suite 能精确捕获。
这样避免 assertion 只对 conforming provider 运行而实际上没有判别力。

## 13. 与其他 crate 的验证分工

| Crate | 主要验证 |
| --- | --- |
| `qubit-fs` | 门面 preflight、request hard boundary、outcome 复核、状态机 |
| `qubit-local-files` | native 算法、平台分支、root containment、publication |
| `qubit-fs-local` | request/result/error/session 映射及完整 suite |
| `qubit-fs-registry` | selection、resolution、canonical URI、concrete facade |
| `qubit-fs-testkit` | provider 黑盒公共契约与 assertion 自验证 |

Testkit 不重复其他 crate 的白盒测试。

## 14. 模块组织

```text
src/
├── file_system_fixture.rs
├── async_file_system_fixture.rs
├── file_system_contract_suite.rs
├── async_file_system_contract_suite.rs
├── contract_context.rs
├── properties_contract.rs
├── io_contract.rs
├── namespace_contract.rs
├── optional_capability_contract.rs
├── temp_contract.rs
├── error_contract.rs
└── representation_contract.rs
```

各 contract 模块的执行入口是 suite 的私有方法；无状态 helper 使用私有零变体 enum
的关联方法组织。Crate 根部只重导出 fixture、suite 和必要的 contract 配置/报告类型，
不重导出 assertion free function。

## 15. 验收标准

- `cargo test` 可在无真实网络依赖下完成 testkit 自验证；
- provider 只需实现 fixture 并创建 suite；
- 完整 suite 对 conforming provider 通过；
- 每个 broken provider 被对应 contract 拒绝；
- capability 缺失不会导致无意义正向测试；
- capability 声明不会被静默跳过；
- 同步与异步 contract coverage 对称；
- public API 中不再出现 `&dyn FileSystem` 或 assertion free function；
- 可选 registration macro 只生成 test wrapper，不承载 contract 逻辑。
