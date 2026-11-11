# Qubit FS Testkit 设计

> 状态：当前实现说明。本文描述 `qubit-fs-testkit` 0.9 与 `qubit-fs` 0.3 的
> provider contract 测试架构；0.7/0.2 仅作为历史版本记录。

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
- 在宏中承载或复制 contract 断言逻辑；宏只负责注册 suite phase；
- 要求只支持部分能力的 provider 为测试伪造额外生产能力。

## 3. Fixture

### 3.1 同步 fixture

```rust
pub type FixtureResult<T> = Result<T, FixtureError>;

pub enum FixtureSupport<T> {
    Supported(T),
    Unsupported,
}

pub trait FileSystemFixture {
    fn file_system(&self) -> &FileSystem;
    fn path(&self, relative: &str) -> FixtureResult<Path>;

    fn list_prefix(
        &self,
        root: &Path,
        relative: &str,
    ) -> FixtureResult<String>;

    fn seed_file(
        &self,
        relative: &str,
        bytes: &[u8],
    ) -> FixtureResult<FixtureSupport<Path>>;

    fn read_file(
        &self,
        path: &Path,
    ) -> FixtureResult<FixtureSupport<Vec<u8>>>;

    fn resource_version(
        &self,
        path: &Path,
    ) -> FixtureResult<FixtureSupport<ResourceVersion>>;

    fn seed_empty_directory(
        &self,
        relative: &str,
    ) -> FixtureResult<FixtureSupport<Path>>;

    fn seed_symlink(
        &self,
        relative: &str,
    ) -> FixtureResult<FixtureSupport<Path>>;

    fn copy_fast_path_case(
        &self,
        method: CopyMethod,
    ) -> FixtureResult<FixtureSupport<CopyFixtureCase>>;

    fn teardown(&self) -> FixtureResult<()>;
}
```

以上为接口节选。Fixture 必须提供独立、幂等的显式 `teardown`，能够处理部分准备完成及
staging 资源；`Drop` 可以补充清理，但不能替代此职责。

第一次尝试 `finish` 后，准备和运行入口会关闭。若清理失败或异步清理 future 被取消，调用方只能
通过 `finish` 重试清理；再次运行前必须创建新的 fixture 和 suite。`ContractReport` 只验证检查证据，
不包含运行是否完成、执行失败或清理结果。完整验收应使用 `ContractRun` 的断言方法。

关键变化是 `file_system()` 返回 `&FileSystem`，不再返回 `&dyn FileSystem`。

Fixture 负责：

- 为一次 suite run 提供隔离 namespace；
- 把 testkit relative name 映射为 provider path；
- 在必要时提供不依赖被测 capability 的 setup/observation；
- 通过显式 teardown 释放测试资源；
- 明确无法提供的 representation probe。

Fixture 不负责重新解释 capability 或放宽 contract。

`FixtureSupport::Unsupported` 只表示 provider/fixture 无法提供某个可选 probe。
`FixtureError` 表示 setup、observation 或 teardown 实际失败，suite 必须报告失败，
不能把它当成 unsupported 而跳过测试。构造核心 logical `Path` 是所有 fixture 的基本
责任，因此 `path()` 返回 `FixtureResult<Path>`，没有 `Unsupported` 分支。

`resource_version`、representation seed 和 `copy_fast_path_case` 有默认
`Unsupported` 实现。`CopyFixtureCase` 包含已经准备好的 source、target 和 options；
它必须保证请求的 `CopyMethod` 在 provider 声明的静态适用范围内。
`FixtureError` 是具体、可保留 typed source 的错误，并满足同步
与异步 fixture 所需的 `Send + Sync + 'static` 边界。

### 3.2 异步 fixture

```rust
pub trait AsyncFileSystemFixture: Sync {
    fn file_system(&self) -> &AsyncFileSystem;
    fn path(&self, relative: &str) -> FixtureResult<Path>;
    // 对应的异步 setup/observation hooks
    fn teardown(&self) -> FixtureFuture<'_, ()>;
}
```

以上同样为接口节选。异步 fixture 必须提供独立、幂等的显式异步 `teardown`，覆盖部分准备
和 staging 资源；`Drop` 不执行异步 I/O，也不能替代 teardown。

Async fixture hook 返回 runtime-neutral future。Testkit 不创建全局 runtime。
异步可选 hook 同样返回 `FixtureResult<FixtureSupport<T>>`，不能用 ready `None`
混淆 unsupported 与 setup failure。

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

    pub fn run_all(&mut self) -> &ContractRun;
    pub fn run_contract(&mut self, contract: FileSystemContract) -> &ContractRun;
    pub fn run(&self) -> &ContractRun;
    pub fn finish(&mut self);
}
```

异步 suite：

```rust
pub struct AsyncFileSystemContractSuite<'a> {
    fixture: &'a dyn AsyncFileSystemFixture,
}
```

它提供同名 async 运行和清理方法，`run()` 读取结果不需要 await。调用者自行使用所选 runtime 的 `#[test]` /
`#[tokio::test]` 等入口。

Suite 持有 name generator、资源清单和 ContractRun。运行入口借用 `&mut self`，
结果保留在套件中；运行 future 被取消后仍可读取证据并重试清理。
一个套件只执行一次，重跑必须创建新的 fixture 和套件。

Provider 可以运行完整 suite：

```rust
#[test]
fn test_file_system_contract() {
    let fixture = RootedFixture::new();
    let mut suite = FileSystemContractSuite::new(&fixture);
    suite.run_all().assert_satisfied();
}
```

也可以为受限 provider 单独运行适用方法。

`register_file_system_contract_tests!` 和异步对应宏为 Rust test harness
生成多个独立 `#[test]` wrapper，以获得并行执行和精确测试名称。两个宏都可在末尾接受
`required_checks` 回调：输入当前阶段，返回要强制要求的 `ContractCheckId` 列表。wrapper 在 suite
运行前求值一次，并将列表交给 `assert_satisfied_with`；省略时使用空列表，保持既有行为。宏不会过滤
ID：若请求的 ID 不属于当前阶段，正常断言会报告失败。异步 wrapper 在调用 runner 前求值，并将列表
移动到 future 中。wrapper 不自行实现契约 assertion、fixture mutation 或 capability 判断；不使用宏的
provider 必须能获得完全相同的 contract coverage。目录拒绝和独立负面副作用证据仍由 suite 与 fixture
hooks 负责；回调只选择证据，不定义或伪造证据。

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

当前实现会对 `FileSystemCapability::ALL` 中全部已声明 capability 验证稳定语义保证：

- `Read`：完整读取；
- `Write`、`Append`；
- `List` 与 page-size hint；
- `CreateDirectory`；
- `Delete`、`RecursiveDelete`；
- `Rename`、`AtomicRename`、`AtomicReplace`；
- `Copy`、`ServerSideCopy`、`AtomicFileCopy`、`AtomicTreeCopy`、
  `DurableFileCopy`、`DurableTreeCopy`；
- `TempFile`、`TempDirectory`、`AtomicTempPersist`；
- `RangeRead`、`ConditionalRead`、checksum validation；
- `ConditionalWrite`、`ConditionalDelete`；
- `EmptyDirectory`、`Symlink` representation。

声明能力但对 capability 明确保证范围内的 fixture case 只能返回
`UnsupportedCapability`，视为 contract failure。

Capability 不是某一次 native fast path 的强制分派开关。`Copy` 可以由门面流式
fallback 提供；`ServerSideCopy`、native clone 等 fast path 还可能受 bucket、device、
region 或具体路径对影响。正向 fast-path contract 必须使用 fixture 明确提供的
applicable case；普通路径对得到无副作用 `CopyAttempt::Declined` 本身不是 contract
failure，最终门面结果仍需满足 resolved requirements。

如果 provider 声明 `ServerSideCopy`、clone 等需要动态适用 case 的 capability，完整
suite 要求 fixture 至少提供一个对应 `copy_fast_path_case`。此时返回
`FixtureSupport::Unsupported` 是 fixture contract failure，不能借此跳过已声明能力。
对于未声明 capability，`Unsupported` 才表示合法跳过。

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

Properties contract 验证：

- filesystem id 和 provider id 非空；
- info、capabilities、limits 与 symlink policy 的快照在检查时保持一致；
- capabilities dependency 自洽；
- path semantics、`PathConstraints` 与 fixture mapping 相容；
- 在预算内探测 finite limit，并明确处理 unknown/inapplicable/unbounded limit。

Limit contract 至少覆盖：

- path text bytes；
- component text bytes；
- read range bytes；
- write bytes；
- list page entries。

套件能直接观察的是公开门面的操作结果、必需副作用、选定元数据，以及 fixture 独立 hook
提供的前置条件和结果。若无法独立观察必要事实，结果为 `Unverified`，不代表通过。取消恢复结果
`Published` 只表示目标可被独立观察，不表示目标必然包含完整请求载荷。

| 证据主张 | 负责验证的测试 |
| --- | --- |
| provider fixture 通过公开门面和独立 hook 实际观察到的结果及副作用 | 对证据完整且实际执行的检查，由 provider contract suite 负责 |
| 套件记账、恢复所有权，以及对注入违规的拒绝能力 | 使用 recording provider 和故障 fixture 的 testkit 自验证 |
| SPI 分派／预检、编译期借用性质、drop/runtime 行为，以及 provider 专属平台或服务保证 | 由拥有该行为的 core 或 provider 测试负责 |

随机读取的正向样本请求遵守声明的 `max_read_range_bytes`，并累积短读直到取得完整期望样本。
`RandomAccessBounds` 在非空边界探针可执行时检查空缓冲、EOF、尾部裁剪、溢出拒绝，以及错误后复用同一个 reader。
合法的 `Maximum(0)` 不允许非空读取，因此 Bounds 会带原因记为 `SkippedOptional`。
这项检查不证明 provider 的最大范围长度。`RandomAccessRangeLimit` 是独立检查：针对 `Maximum(n)`，
它单独准备 reader，验证 `n` 字节可读、`n + 1` 字节以 `Read` 操作返回
`ResourceLimitExceeded`，再确认 reader 仍可用。`Maximum(0)` 时，允许的请求和拒绝后的复查都是空读取，
单字节请求仍须被拒绝。零是合法上限，但这项检查不声称它允许成功读取非空内容。
种子与请求均受 64 字节探针预算约束。
有限上限达到该预算时，因不执行更大的探针而记录带原因的 `SkippedOptional`；
`Unknown`、`Unbounded` 和 `NotApplicable` 则因不存在有限上限而记录 `NotApplicable`。
缺少 seed hook 是 `Unverified`；执行错误或字节、错误结果不符均为失败。缺少 `Read` 时，所有随机读取
检查会在调用能力声明或准备 hook 前记为 `NotApplicable`；`ReadBasic` 仍独立核实门面对缺少能力的结构化拒绝。
需要完整边界证据的调用方必须在 `assert_satisfied_with` 中同时选择 Bounds 和 RangeLimit 两个 ID。

异步 `RandomAccessCancel` 只有在对注入读取进行 poll 后得到 `Pending`，且
`random_access_pending_read_reached` 返回 `FixtureSupport::Supported(true)` 时才通过。
该 hook 默认返回 `Unsupported`，必须仅检查 fixture 独立 instrumentation，不得执行 I/O。
poll 后套件丢弃 future、查询 hook、解除 gate，并确认 reader 可以继续使用。无法注入 gate 是可选跳过；
缺少 gate 确认或 hook 返回 false 是 `Unverified`；hook 错误、panic 或解除 gate 后 reader 不能继续读取则失败。
这样记录的是实际到达 Pending gate 的取消证据，而非仅构造了可能阻塞的请求。

rename 只有在结构化错误正确，且独立的前后观测证明相关路径保持不变时，才能记为
`RejectedAsExpected`。provider 返回的
`RenameFailureState::Unchanged` 不能替代独立证据；缺少必需观测时结果为 `Unverified`。

testkit 自身的 recording provider 和故障 fixture 用于验证套件记账、恢复所有权及已覆盖检查能否拒绝
指定违规；这些自测不构成 provider 的行为保证。Provider suite 的黑盒观察不能证明 properties getter
不执行 I/O、facade clone 共享同一 snapshot identity，或 diagnostics 不含 credential-like key。这些要求
由 core/provider 的白盒测试或实现审查负责。门面的 SPI 分派和预检由 core 测试负责；native 行为、平台
语义及套件观察范围以外的服务保证由 provider 自己验证。

测试只在可安全构造边界输入时探测，不通过可能耗尽 CI 资源的巨大分配验证 limit。
对于层次路径，`ListBasic` 包含直接文件和子目录，但排除孙级条目。`ListPrefix` 使用
`Subtree` 过滤和递归遍历，期望集合包括所选目录及其全部预置后代。Fixture 必须通过
`seed_empty_directory` 独立预置目录；预置文件不代表目录一定可见。

## 8. I/O 与 namespace contract

### 8.1 Metadata 与 read

当前 suite 的实际检查与责任边界如下：

| 范围 | Suite 当前证据 | 其他责任／未由本 suite 证明 |
| --- | --- | --- |
| `stat` | `StatBasic` 对独立生成的 missing path 检查结构化 `NotFound/Stat`；`StatFileKind` 对独立 seed 的资源检查 file-like kind，并在长度已知时比较 seed 长度 | `len == None` 被允许；logical path、final-symlink 解析与平台行为由核心门面和 provider 各自测试负责，本轮未确认这些保证的具体下游测试位置 |
| `exists` | provider suite 没有具名检查独立验证 `exists` 对错误的吞并行为 | “只吞掉 NotFound”的门面分派语义由核心测试负责 |
| reader/read | `ReadBasic` 等场景检查 seeded 内容和相应结构化错误；`ReadIfMatch*`、`ReadIfNoneMatch*`、`ReadChecksum` 和 `ReadChecksumCorruption` 在适用时验证条件／checksum 场景；范围检查包含零长度、EOF 及 EOF 之后的空窗口 | reader 的 `FileSystemId + Path` 身份与 open-time snapshot 不是普通 reader 检查的独立证据；reader identity、snapshot 与 final-symlink 语义由核心/provider 测试负责。随机访问 identity 检查不代表普通 reader identity 覆盖 |
| range/limits | `read_all` 按所选窗口实际消费字节应用 caller limit；range 检查独立验证内容、EOF 截断/空窗口和适用的 filesystem range limit | 不证明读取内容来自不可变快照，也不证明服务端实际采用特定分页或范围实现 |

suite 的 missing-path `stat` 和 read scenario 观察不能证明门面 `exists` 只吞掉 `NotFound`；该分派语义属于核心测试责任。上表未标为 suite 证据的属性不应被描述成 provider contract suite 已验证。

`read_all` 的 `max_bytes` 预算按本次读取窗口实际消费的字节计算。打开元数据中的 `len`
仍表示完整资源长度，但不用于预先拒绝读取。完整资源为 `0123456789` 时，
`offset = 2, length = 3, max_bytes = 3` 必须成功并返回 `234`，`max_bytes = 2` 必须返回
`ResourceLimitExceeded`；即使元数据高估长度，实际较短的流也应成功。窗口长度为零也
必须先执行 open，以保留 NotFound、权限和条件错误。offset 位于 EOF 或之后时，范围读取返回空窗口；
请求长度超过 EOF 时截断。成功打开不表示读取到资源内容快照，窗口内容也不保证来自同一时刻的资源状态。

### 8.2 Write

验证：

- create、replace、create-new 和 append；
- write precondition；
- required atomicity；
- `WriteOutcome` 的 method/atomicity/version；
- commit failure state；
- abort 不回滚已 published target；
- `write_all` failure 保留需要恢复的 writer。

成功 commit 后重复 commit 是 `InvalidState`，但其 `WriteFailureState` 必须反映已知发布事实：
writer 已处于 `Committed` 或 `Published` 时报告 `Published`，不得再次调用 SPI commit 或自动
abort，且目标内容仍可读取。只有 `RetryableNotPublished` 才允许再次 commit；首次处于
`Indeterminate` 的 writer 重复调用仍报告 `Indeterminate`，不能被错误降级为未发布。

### 8.3 List 与 directory

验证：

列举契约比较独立准备的条目集合、路径和过滤结果；请求 metadata 时检查其存在性。独立 seed 的
资源类别也会被核对：目录必须报告 `Directory`，文件可报告 `File` 或 `Object`。类别来自
seed 操作，不会从路径文本推断目录，也不表示所有 metadata 字段都已验证。分页检查以
`page_size = 1` 请求并验证结果完整，不证明真实服务分页大小或实际 SPI 参数。页大小属性检查验证公开
clamp 计算。metadata 字段 absent/unknown 的完整语义、SPI 分派参数、流错误后的终止状态和全部
provider 违规识别不由本段套件结果统一证明，应分别参考核心白盒回归或 provider 自有验证。

### 8.4 Namespace mutation

验证：

- create-directory policies；
- file/directory delete；
- recursive delete 不遗留 child；
- copy 不删除 source；
- copy 的 `CopyMethod`、`used_fallback`、atomicity、durability、metadata 与统计符合
  实际路径；
- 门面 fallback 只覆盖允许的普通文件 create-new/skip 组合；
- provider-native `Completed` 不再执行门面 fallback；
- native `Declined` 只有在 fallback allowlist 满足时才成功；
- native failure 不因错误种类触发第二次 copy；
- `CopyFailureState` 与 partial stats 符合可观察 target 状态；
- rename 成功后 source 消失；
- `RenameFailureState::{Unchanged, Renamed, Indeterminate}` 与可观察状态一致；
- type conflict、overwrite 和 precondition；
- required atomicity 不降级；
- outcome source/target context 完整。

缺能力的 rename 拒绝还需要独立确认初始资源状态及请求后的状态。资源变化或观察错误是失败；
缺少必需观察能力时记为 `Unverified`。门面是否在进入 SPI 前完成预检，仍由 `qubit-fs` 的
recording SPI 测试验证。
成功 rename 的必需观察 hook 返回 `Unsupported` 时也记为 `Unverified`，并在缺少的证据被
后续 rename 依赖前停止检查；已支持但不正确的观察结果及 fixture 错误仍为失败。

## 9. Temporary resource contract

同步与异步 suite 都覆盖：

- temp handle 保存创建它的原始 filesystem identity；
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
- successful `keep` returns a reachable generated target and terminal state;
- recoverable keep failure retains the temporary resource for retry.

需要注入 failure 的状态主要由 `qubit-fs` fake SPI 测试。Provider 黑盒 suite 在 provider
声明相应 fixture fault hook 时复用这些 contract；不要求生产 provider 暴露故障注入。

## 10. Error contract

在对应已执行的负面场景中，suite 校验该场景要求的结构化字段及保留的错误证据，而不只断言
`is_err()`：

- `FsErrorKind`；
- `FsOperation`；
- source path；
- target path；
- provider id；
- required capability；
- indeterminate/partial-progress state；
- 当该负面场景检查 source 保留时，确认 source 证据仍可用。

具体校验哪些字段以及是否保留 source 由各检查场景决定。逐操作完整错误链与
`Display`／`Debug` 的格式化和安全性质属于 core/provider 的测试责任，不是 provider suite
已统一验证的保证。

Provider 返回非法 opened identity、entry 或 outcome 时，门面必须产生
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

异步 provider suite 只能验证通过 fixture hook 和公开门面实际可观察的行为，例如已提供取消探针时
取消后的结果与清理状态。future 借用生命周期、drop 是否启动 executor 或阻塞、SPI 分派以及完整的
stream/backpressure 保证，不由一次真实 provider suite 的黑盒结果证明，应由编译期、core 或
`qubit-io` 专项测试负责。

testkit 的可控 future 和故障模型自验证 `AsyncCopyOperation`：

- `begin_copy` 的状态与 testkit 预期一致；facade 是否未调用 provider 由 `qubit-fs` recording SPI 测试负责；
- `Ready → Running → Completed` 成功路径；
- explicit failure 进入 `Failed(reported CopyFailureState)`；
- 在 native attempt、reader、writer 和 commit await 点取消时进入
  `Failed(Indeterminate)`；
- fallback 已创建 writer 后取消，operation 仍持有 recovery writer；
- `take_recovery` 显式转移 cleanup/recovery responsibility；
- 未 poll 的 execute future 被 drop 不改变 `Ready`；
- operation drop 不执行 I/O；
- 非 `Ready` operation 不能盲目重新 execute。

这些自验证说明 testkit 的故障模型和断言有效，不构成 provider 自身行为保证。

取消测试不绑定具体 runtime：testkit 自验证使用可控 `Pending` future 手工 poll；
真实 provider suite 只有在 fixture 提供对应 fault/cancellation hook 时才运行阶段性
取消 probe。

### 调用方控制异步 deadline

runtime-neutral suite 不内置 deadline。调用方可用所用 runtime 的 timeout 包裹手动管理的
`run_check` future；计时 future 被 drop 后，可检查 `suite.run()` 保留的运行证据，并使用独立的
cleanup budget 调用 `suite.finish()`。即使清理完成，run timeout 仍表示 contract 失败。若清理也超时，
保留的 cleanup evidence 不证明异步资源已经回收。协作式 timeout 无法抢占阻塞的 `poll`。
注册宏 runner 拥有整个 fixture/suite wrapper；超时丢弃 wrapper 后，调用方无法取回 retained run
进行检查或重试 `finish()`。

## 12. Testkit 自验证

以下是 testkit 自验证及 core/provider 故障测试覆盖的场景目录，不表示它们全部由 provider suite 捕获：

- unstable/inconsistent properties；
- ignored option；
- late preflight side effect；
- wrong opened identity；
- wrong file kind；
- list entry 越界；
- truncated/invalid stream；
- atomicity downgrade；
- copy fast path decline 后遗留 staging；
- copy native failure 后门面错误重试；
- copy failure state/partial stats 错误；
- copy 删除 source；
- rename 保留 source；
- rename 已完成却报告 `Unchanged`；
- recursive delete 遗留 child；
- invalid error context；
- writer/temp failure state 错误；
- secret 泄漏；
- async copy cancellation 丢失 recovery writer；
- async cancellation state 错误。

Testkit 自验证使用 conforming provider 及故障模型，验证套件自身的记账、恢复和已实现断言；只有
suite 实际执行并取得独立观察证据的场景才计入其覆盖。门面 preflight、SPI 分派及 provider contract
violation 转换由 core 测试负责；provider/native 行为、secret 泄漏及平台语义由 provider 自己的白盒或
故障注入测试负责。各项是否由 suite 覆盖，以实际测试实现和场景证据为准，不要求 suite 捕获此目录
中的每一种故障。

## 13. 与其他 crate 的验证分工

| Crate | 主要验证 |
| --- | --- |
| `qubit-fs` | 门面 preflight、request hard boundary、outcome 复核、状态机 |
| `qubit-local-files` | native 算法、平台分支、root containment、publication |
| `qubit-fs-local` | request/result/error/session 映射及完整 suite |
| `qubit-fs-registry` | selection、resolution、canonical URI、concrete facade |
| `qubit-fs-testkit` | provider 黑盒公共契约与 assertion 自验证 |

Testkit 不重复其他 crate 的白盒测试。

### 13.1 真实后端验证边界

第二种真实 provider 的证据由独立、`publish = false` 的验证工程
`fixtures/s3-contract/` 提供。该工程使用 S3 兼容 SDK 对接由环境变量指定的专用 endpoint，要求
bucket 已存在，并通过真实 GET metadata/range、create-only PUT、冲突、取消后的发布状态和清理验证
公共契约；测试只在配置 prefix 下的唯一子路径创建并清理对象，不创建 bucket，也不清理整个共享
prefix。它不成为 `qubit-fs` 或 testkit 的生产依赖，也不等同于一个可发布的 S3 provider。SDK 无法保真表达的
object key 子集必须在请求前拒绝，并在验证记录中明确限制范围。

本设计只定义验证入口和证据要求，不把本地 testkit 或编译成功当作远程后端已验证。只有在独立
工程实际运行后，连同后端版本、依赖 lockfile、配置变量（不含 secret）和可追踪的成功/冲突/取消/
清理结果一并记录，才可以得出远程 provider 结论。

## 14. 模块组织

同步与异步 suite 共享 contract identity、report、run、context、fixture
准备和 scenario 类型；各 suite 再按 phase 拆分为 copy、read、write、delete、
list、properties、temp 等模块。取消、失败恢复和 capability 预检位于独立的
typed helper 模块中，避免把状态机隐藏在宏或 fixture trait 中。

```text
src/
├── contract_check_id.rs / contract_check.rs
├── contract_report.rs / contract_run.rs / contract_context.rs
├── file_system_fixture.rs / async_file_system_fixture.rs
├── file_system_contract_suite/
│   ├── copy.rs, read.rs, write.rs, delete.rs, temp_file_check.rs, ...
└── async_file_system_contract_suite/
    ├── copy.rs, read.rs, write.rs, delete.rs, temp_file_check.rs, ...
```

各 contract 模块的执行入口是 suite 的私有方法；无状态 helper 使用私有零变体 enum
的关联方法组织。Crate 根部只重导出 fixture、suite 和必要的 contract 配置/报告类型，
不重导出 assertion free function。

## 15. 验收标准

- `cargo test` 可在无真实网络依赖下完成 testkit 自验证；
- provider 只需实现 fixture 并创建 suite；
- fixture setup/observation failure 不会被当成 unsupported 跳过；
- 完整 suite 对 conforming provider 通过；
- 已实施的 suite 自验证覆盖对应的故障模型；core/provider 故障测试各自覆盖其责任范围；
- capability 缺失不会导致无意义正向测试；
- capability 声明不会被静默跳过；
- 同步与异步 contract coverage 对称；
- effective copy、provider-native fast path 与门面 fallback 的责任被分别验证；
- `AsyncCopyOperation` 的状态、取消和 recovery responsibility 被确定性验证；
- public API 中不再出现 `&dyn FileSystem` 或 assertion free function；
- 可选 registration macro 只生成 test wrapper，不承载 contract 逻辑。

覆盖率阈值适用于全部源码文件，不设置豁免。依赖 provider 能力的契约分支必须由
确定性的 fixture profile 和聚焦回归测试执行，包括 native capability 与失败组合。


## 恢复所有权

writer／临时资源打开的负面检查遇到 provider 契约违例时，会完整保留 `OpenFailure<R>`。
应检查 `OpenFailureStage`，并区分恢复类型；隔离会话只允许显式清理。
整文件操作通过 `WriterRecovery` / `AsyncWriterRecovery` 的 `Opened` 和 `Rejected`
变体交回所有权。套件不要求健康 provider 人为制造提交失败；不可变失败状态和确认字节数
由 testkit 自身的 recording provider 回归测试验证。

预期的条件拒绝也可能带有已打开 writer。IfAbsent、IfMatch、CreateConflict、WriteLimit
检查在持有会话时，只有确认未发布并显式 abort 后才通过。没有恢复会话时，
保留原始失败状态，并通过独立观察验证目标结果。清理失败时保留
`ContractWriterFailure<WriteAllFailure>` 或
`ContractWriterFailure<ContractAsyncWriteFailure>`：`error()` 表示清理错误，
`writer()` 持有原始失败及恢复责任。恢复不会改变历史状态和字节数。
缺少的可选写取消探针会明确记录为跳过；已执行探针失败仍然失败。既有检查 ID 不变，
范围能力允许时，读取契约增加零长度和 EOF 窗口验证。

`ReadRange` 还会在选定窗口上验证调用方的 `read_all` 字节预算：等于窗口长度的预算必须返回
seed 字节，少一个字节的预算必须以 `Read` 操作返回 `ResourceLimitExceeded`。这是门面预算检查，
与 provider 的 `max_read_range_bytes` 限制分开。`max_bytes` 由门面执行，不会作为参数传入 SPI
request；这只验证调用方侧的消费预算，不代表 provider 或服务端的字节预算。`WriteLimit` 会在拒绝请求前独立记录目标存在性；
fixture 准备 `CreateOrReplace` 时还会记录既有内容。清理保留的 recovery session 后，
套件再次独立确认目标仍缺失或保持原状。观测能力缺失记为未验证；观测错误或目标变化会使检查失败。

取消诊断的所有权迁移如下：

| 失败路径 | 保留的 source | 恢复方式 |
| --- | --- | --- |
| 异步写入／复制在目标阶段前执行失败 | `ContractAsyncWriteFailure`／`ContractAsyncCopyFailure` | `operation_mut().take_recovery()` |
| ACK、探针解除或 operation 状态诊断失败 | `ContractWriterFailure<AsyncWriteAllOperation>`／`ContractWriterFailure<AsyncCopyOperation>` | 取出并向下转换 `ContractSource`，用 `error()` 检查 `ContractFailure`，再调用 `writer_mut().take_recovery()` |

第二种形式即使没有已打开的 writer，也会保留 operation 的诊断状态。
存在 recovery 时由调用方取出并显式 abort writer；取出 source 即转移该责任。

临时资源检查通过独立 hook 写入并核对非空文件内容及目录子项，覆盖 keep、persist、覆盖发布和
重复生命周期。缺少内容 hook 时结果标为 `Unverified`，setup 错误仍然失败；临时资源失败保留
typed recovery source。同步的 expected-write cleanup 会在 provider abort 边界捕获 panic，同时保留
原 `WriteAllFailure`、writer recovery 和 panic payload。testkit 0.7 曾对应 qubit-fs 0.2；这只是
历史版本，不是当前 0.9 的兼容目标。目录覆盖以独立预置的空目标目录为基线，不据此承诺覆盖非空
目录的行为。
