# qubit-fs-testkit User Guide

[中文](user_guide.zh_CN.md) · [README](../README.md) · [API documentation](https://docs.rs/qubit-fs-testkit)

## Purpose and Audience

This guide is for authors of synchronous or asynchronous `qubit-fs` 0.3
providers using the current `qubit-fs-testkit` 0.9 contract suites. The testkit
is test support and belongs in provider development dependencies. References
to testkit 0.7 and `qubit-fs` 0.2 below describe historical interfaces.

## Conceptual Model

```text
provider test
   │
   ├─ isolated FileSystemFixture ─────► FileSystemContractSuite
   │
   └─ isolated AsyncFileSystemFixture ► AsyncFileSystemContractSuite
                                               │
                                               ▼
                                  capability-driven contract assertions
```

A fixture exposes the concrete facade under test and maps non-empty,
`/`-separated testkit-relative names to provider paths. It also maps list
prefixes. Optional fixture hooks can seed/read files, observe resource versions,
seed empty directories or symlinks, and prepare native-copy cases. The async
fixture supplies equivalent future-based observations and optional copy
cancellation cases.

## Scenario

You are adding a provider and need confidence that its advertised capabilities
agree with observable filesystem behavior. The success condition is a fresh,
isolated fixture whose suite completes and leaves its test resources cleaned up
including when deletion is unavailable.

## Installation and Minimal Configuration

Add the testkit as a development dependency in the provider crate:

```bash
cargo add --dev qubit-fs-testkit@0.9
```

The provider itself uses `qubit-fs` 0.3. The repository's complete local
provider fixture, including properties, independent seed/read/exists hooks,
and teardown, is [`fs_testkit_contract.rs`](https://github.com/qubit-ltd/rs-fs-local/blob/main/examples/fs_testkit_contract.rs).
With sibling repositories checked out, compile and run that exact example:

```bash
cargo run --manifest-path ../rs-fs-local/Cargo.toml --example fs_testkit_contract
```

Implement `FileSystemFixture` for a fixture that owns or otherwise retains the
resources required to keep its filesystem isolated. At minimum, implement
`file_system`, `path`, and independent `teardown`; `list_prefix` has a default implementation. Implement
`AsyncFileSystemFixture` for an asynchronous facade; it has the same required
mapping methods and is `Sync`.

## Core Workflow

Use the registration macro to create a fresh fixture and a precise test name for
each contract phase. The synchronous `RootedFixture` below is defined in the
complete [`fs_testkit_contract.rs`](https://github.com/qubit-ltd/rs-fs-local/blob/main/examples/fs_testkit_contract.rs)
example linked above:

```rust,ignore
qubit_fs_testkit::register_file_system_contract_tests! {
    module: provider_contracts,
    fixture: super::RootedFixture::new,
    required_checks: |phase| match phase {
        qubit_fs_testkit::FileSystemContract::Read => vec![
            qubit_fs_testkit::ContractCheckId::RandomAccessIdentity,
        ],
        _ => Vec::new(),
    },
}
```

The trailing `required_checks` field is optional and accepts a function or
closure with the logical signature
`FnOnce(FileSystemContract) -> Vec<ContractCheckId>`. It is evaluated once for
each generated phase before that phase runs. Returning an empty vector leaves
optional checks optional; selecting a skipped or unverified check makes the
assertion fail. A check ID that is not part of that phase also fails with an
“outside this run” diagnostic. `NotApplicable` outcomes keep the suite's
existing applicability rules, and failures from checks that did run always
fail. Omitting the field keeps the previous default behavior.

For manual control, keep the suite and fixture alive for the entire run, then
assert both the contract result and independent cleanup:

```rust,ignore
use qubit_fs_testkit::FileSystemContractSuite;

let fixture = RootedFixture::new();
let mut suite = FileSystemContractSuite::new(&fixture);
let run = suite.run_all();
run.assert_satisfied();
assert!(run.cleanup().completed());
```

The registration macro applies the same assertion policy for generated tests.

Both suites check properties, `stat`, read, write, list, directory creation,
delete, copy, rename, append, recursive deletion, required-atomic
rename/replacement, required-durable copy, temporary resources including
atomic persistence, and error context, then perform cleanup. Unadvertised core
operations are checked for structured `UnsupportedCapability` preflight. Copy
is the exception: when `Copy` is not advertised, the facade skips the native
fast path and may use the allowlisted stream fallback when `Read` and `Write`
are available; missing fallback prerequisites still produce a structured
unsupported-capability failure.
Unadvertised stronger guarantees are checked for structured `RequirementNotMet`
preflight.

The read budget applies to bytes consumed from the selected window, rather than
to the complete resource or its metadata length. For example, reading
`offset = 2`, `length = 3` from `0123456789` with `max_bytes = 3` must return
`234`; a budget of `2` must return `ResourceLimitExceeded`. An overestimated
metadata length cannot reject a shorter stream before it is read. The suite
always opens the reader first, including for a zero-length window, so
not-found, permission, and condition errors are preserved.

For range reads, an offset at or beyond EOF returns an empty window, and a requested
length extending past EOF is truncated. A zero-length request still opens the
resource, preserving not-found, permission, and condition errors. Opening a
reader does not promise a snapshot of the resource contents.

The `ReadRange` check also reads the same non-empty, non-zero-offset window
with a caller budget equal to its length and verifies those exact bytes, then
retries the request with a budget one byte smaller and requires
`ResourceLimitExceeded` for `Read`. This caller budget is enforced by the
facade and is separate from the provider's declared maximum range length.
`max_bytes` is not passed as a parameter in the SPI request, so this verifies
the caller-side consumption budget rather than a provider or service-side byte
budget.

After a successful writer commit, a second commit is an `InvalidState` error
whose `WriteFailureState` is `Published`. It must not issue another provider
commit or automatically abort the writer, and the published target remains
observable. A retryable `NotPublished` failure remains the only non-terminal
case that may be committed again.

For an async facade, implement `AsyncFileSystemFixture` for your provider
fixture and pass a runner backed by the runtime already used by that provider.
For example, with Tokio enabled in the provider's dev-dependencies:

```toml
tokio = { version = "1", features = ["rt"] }
```

```rust,ignore
use std::future::Future;

fn run_on_tokio<F: Future<Output = ()>>(future: F) {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime must start")
        .block_on(future);
}
```

The runner must drive the supplied future until it finishes. If it returns
early or catches a contract assertion panic, the generated test fails with a
`[fs-testkit:runner/incomplete]` diagnostic. This check does not impose a
deadline or recover async cleanup after the future is dropped. To keep run
state and retry cleanup after a timeout, manage the suite directly and use
separate run and cleanup budgets.

Then register the provider's `AsyncFileSystemFixture` implementation and that
defined runner:

```rust,ignore
qubit_fs_testkit::register_async_file_system_contract_tests! {
    module: async_provider_contracts,
    fixture: super::MyAsyncProviderFixture::new,
    runner: super::run_on_tokio,
    required_checks: super::required_checks,
}
```

The callback has the same phase-to-ID behavior as the synchronous macro. The
async wrapper evaluates it once outside the runner, then moves its list into
the future and checks it after the phase completes.

## Advanced Usage

Use optional fixture hooks only when the generic suite needs a provider-owned
observation outside the operation being checked. Examples include `seed_file`,
`read_file`, `resource_version`, `seed_empty_directory`, `seed_symlink`, and
`copy_fast_path_case`. Return `FixtureSupport::Unsupported`
for an optional observation the provider cannot supply; it is not a fabricated
assertion.

`seed_file` and `read_file` must use an observation channel independent of the
facade under test. For example, a local-provider fixture can seed and inspect
its isolated temporary directory through native filesystem APIs. Reusing the
same facade for setup or observation can make a matching read/write defect pass
the contract suite.

Use `prepare_read(ReadScenario, ...)` and `prepare_write(WriteScenario, ...)`
to prepare the requested scenario. Return `FixturePreparation::Ready` with the
path or request, `Unavailable` when preparation cannot supply evidence, or
`NotApplicable` with a reason for the suite to evaluate. An actual setup error
must remain `Err(FixtureError)`. Neither missing preparation nor an unsupported
observation proves that an advertised capability works.

Read scenarios prepare their own contents. Conditional write, atomic replacement,
durable write, and append checks also prepare independently: missing basic
creation evidence does not suppress these checks. The run still fails its
requirements if basic creation remains `Unverified`.

Default write preparation supplies fresh creation and durable creation requests,
and uses `seed_file` for creation conflicts, atomic replacement and append. A
`CreateConflict` request must use `CreateNew` against an existing target whose
contents differ from the requested payload. Override preparation for
`Replace` when the default seeded target is unsuitable: the initial contents
must be longer than the new payload so that the suite can verify truncation.
The default prepares this target through `seed_file`. Override preparation for
`IfAbsent` and `IfMatch`: preserve the requested payload and condition, use an
absent target for `IfAbsent`, and independently seed an existing target and read
its current version for `IfMatch`. Atomic replacement requires different initial
bytes; append requires nonempty initial contents. The suite independently checks
those premises and the published contents rather than accepting weaker expected
results from the fixture.

Use `let mut suite = FileSystemContractSuite::new(&fixture)` and call
`suite.run_contract(phase)` or `suite.run_all()`. The returned `&ContractRun`
remains owned by the suite. Call `run.assert_satisfied()` to check execution,
required evidence, and cleanup together; a report alone does not prove cleanup
succeeded. Registration macros enforce this same policy.

Each suite is a single execution session. The first `finish` attempt closes
the preparation and run entry points. If cleanup fails or its future is
cancelled, retry cleanup with `finish`; do not seed resources or start another
run. Create a fresh fixture and suite for a new run. For an asynchronous suite,
await the same entry points using your own runtime. If its run future is
dropped, inspect `suite.run()` and retry `suite.finish().await` for cleanup.
Dropping the entire suite performs no asynchronous I/O, so the fixture owner
must arrange independent teardown.

`ContractReport` assertions and queries describe check evidence only. A
successful report does not establish that execution completed or cleanup
succeeded. Use the corresponding `ContractRun` methods, such as
`run.assert_satisfied()`, to validate the complete run.

The independent `write/abort` check prepares a fresh `Abort` scenario and uses
`exists_out_of_band` before and after abort. `NotPublished` requires the target
to remain absent; `Published` requires the target to be independently
observable, without requiring its bytes to differ from the earlier contents;
`Indeterminate` preserves uncertainty. The suite also checks the corresponding
writer state.
If streaming or abort fails, the run retains a `ContractWriterFailure<W>` inside
`ContractSource`, where `W` is `FileWriter` or `AsyncFileWriter`. Take the source,
downcast it, and use `writer_mut()` for explicit recovery. The original error
remains available through `error()`; successful recovery does not change the
failed run into a passing one.

Unexpected failures from prepared asynchronous whole-file writes retain
`ContractAsyncWriteFailure`. Its `failure()` preserves the publication state and
accepted byte count. `operation_mut()` exposes the operation that owns any
recovery writer; call `take_recovery()` to take that writer for explicit
abort. Admission failures have no operation. Taking the source transfers recovery
responsibility; dropping it does not perform asynchronous abort.

Copy probe execution failures retain `ContractAsyncCopyFailure`, pairing the
original `AsyncCopyFailure` snapshot with the admitted operation. If a write or
copy fails before the requested cancellation stage, the runner first drops its
execution future and disarms the probe, then retains the operation for recovery.
If recovery abort itself fails, `ContractWriterFailure<AsyncFileWriter>` keeps
the writer available for an explicit retry.

Cancellation acknowledgement, disarm, or state-diagnostic failures retain the
owning operation as `ContractWriterFailure<AsyncWriteAllOperation>` or
`ContractWriterFailure<AsyncCopyOperation>`, including stages where no writer
has opened. Take and downcast the `ContractSource`; `error()` is the original
diagnostic `ContractFailure`, and `writer_mut()` exposes the operation. If it
has recovery, call `take_recovery()` and explicitly `abort_async()` the opened
writer. Execution failures before the requested stage continue to use
`ContractAsyncWriteFailure` or `ContractAsyncCopyFailure`, whose
`operation_mut()` exposes the operation. Taking either source transfers the
recovery duty; dropping the value does not perform async I/O.

Temporary-resource failures retain `ContractTempFailure<T>`, where `T` is the
concrete synchronous or asynchronous temporary file or directory type. Take and
downcast the `ContractSource`, inspect the original error with `error()`, then use
`resource_mut()` for explicit recovery. Ownership transfer does not clear the
failed run, and dropping the wrapper does not perform asynchronous cleanup.

Before keep or persist, temporary-file checks seed non-empty `b"temp-data"`
through `write_file_out_of_band` and independently confirm it with `read_file`.
Directory checks seed two distinct child payloads and verify the same relative
paths beneath `outcome.target()` after keep, persist, and overwrite. These hooks
must not depend on the facade's Write or CreateDirectory capability. If either
hook is unsupported, content evidence is `Unverified` with the missing hook
named; a setup error remains a failure.

`TempAtomic` executes its own required-atomic persistence requests. Ordinary
temporary-resource checks use preferred atomicity and verify publication,
cleanup, keep, naming options and directory replacement independently. When no
temporary resource type is supported, the atomic and repeated-lifecycle checks
are explicitly not applicable.

`teardown` is mandatory, independent, and idempotent. It must reclaim partially
prepared resources and staging data, including paths unknown to the suite,
even when the provider does not advertise `Delete`. Successful independent
teardown does not erase a failed facade cleanup operation.

Directory creation uses independently confirmed absent targets. For recursive
creation, `exists_out_of_band` must observe both the new parent and child;
reporting success for the child alone is insufficient. The suite also verifies
directory metadata and the `already_existed` outcome of a repeated request.

`prepare_delete(DeleteScenario, relative, bytes)` prepares an existing file for
basic or conditional deletion. Conditional deletion requires distinct current
and stale version observations: stale deletion must reject without changing
content, then the current version must allow removal. Missing-ok deletion checks
absence both before and after execution. Recursive deletion prepares directories
and children through fixture hooks and independently confirms removal at each
depth; it does not require the provider's directory-creation operation for setup.

Empty-directory and symlink representation checks run independently. Missing
capabilities produce justified `NotApplicable` outcomes; they do not claim a
rejection was exercised. Advertised representations without fixture preparation
remain `Unverified`.

Each rename check prepares distinct source and destination paths. Conflict checks
confirm that rejection preserves both contents before testing explicit overwrite.
Successful basic, atomic and durable renames all require independent evidence of
source removal and exact destination content, in addition to the requested
guarantee and returned path identities. If a required observation hook returns
`Unsupported`, the check is `Unverified` and stops before another rename depends
on that missing evidence. Incorrect supported observations and fixture errors
remain failures.

Listing checks prepare separate namespaces. Subtree results use a fixed expected
directory and file set; pagination has its own three-entry request with a page
size hint of one. Hierarchical paths use subtree filters, while raw object keys
use `ListOptions::object_keys()` and literal-prefix filters. Cross-model filters
must produce structured rejection. Returned entries are bounded so duplicates
or unexpected entries cannot grow the collected result without limit.
For hierarchical paths, `ListBasic` checks direct files and child directories
while excluding grandchildren. `ListPrefix` checks recursive traversal under
its selected subtree, including the subtree directory and deeper descendants.
Use `seed_empty_directory` to prepare every directory independently; do not rely
on file seeding to make parent directories visible. The suite checks each
returned entry against its seed category: directories must be `Directory`, and
seeded files may be `File` or `Object`. It does not infer directory type from a
path string or claim that every metadata field is verified.

Random-access checks are applicable only when the captured filesystem
properties include `Read`. Without it, the checks are recorded as
`NotApplicable` before random-access declaration or preparation hooks run.
`ReadBasic` still checks the facade's structured rejection for missing `Read`.

The random-access evidence is split by claim. `RandomAccessBounds`
(`read/random-access-bounds`) checks an empty buffer, a read at EOF, clipping to
the reader's open-time length, overflow rejection, and successful reuse of the
same reader after that error when a non-empty boundary probe is possible. A
valid `Maximum(0)` makes those non-empty probes impossible, so Bounds is
`SkippedOptional` with a reason. It does not test the provider's declared
maximum range length. `RandomAccessRangeLimit`
(`read/random-access-range-limit`) independently seeds a reader and, for a
finite `Maximum(n)`, verifies that a request of `n` bytes is accepted, `n + 1`
bytes is rejected with `ResourceLimitExceeded` for `Read`, and the reader still
works afterward. When `n` is zero, both the accepted request and the
post-rejection reader check are empty reads; the one-byte request must still be
rejected. Zero is a valid declaration, and this check does not claim that a
non-empty read succeeds under a zero-byte maximum. The range-limit fixture
seed and request are bounded to 64 bytes. A maximum of 64 bytes or more is
reported as `SkippedOptional` with a reason because the suite will not make the
larger probe; `Unknown`, `Unbounded`, and `NotApplicable` limits yield
`NotApplicable` because there is no finite maximum to test. If the fixture
cannot seed the independent sample, the result is `Unverified`; setup/open
errors or incorrect bytes and errors fail the check.
If the filesystem lacks `Read`, both random-access checks are `NotApplicable`.
An undeclared random-access operation is `SkippedOptional`; an explicit
declaration that it is unsupported is checked for structured rejection.

Callers that need both ordinary boundary behavior and the provider's finite
range-limit evidence must require both IDs, for example with
`run.assert_satisfied_with(&[ContractCheckId::RandomAccessBounds,
ContractCheckId::RandomAccessRangeLimit])`. Requiring only `Bounds` makes no
claim about the range limit; requiring only `RangeLimit` makes no claim about
EOF, clipping, or overflow behavior.

The async `RandomAccessCancel` check injects a pending read through fixture
instrumentation. Override `random_access_pending_read_reached(&self, path)` to
report whether that read entered the gate; the default hook returns
`FixtureSupport::Unsupported`. The suite polls the read once, drops its future,
then queries the independent hook and disarms the gate. Only an observed
`Pending` together with `FixtureSupport::Supported(true)` can pass; no pending
poll, `Supported(false)`, or `Unsupported` evidence is `Unverified`. No gate
injection remains an optional skip. Hook errors, panics, or a reader that cannot
be used again after disarming fail the check. The observation hook must inspect
fixture instrumentation only and perform no I/O. The suite has no runtime
dependency for this probe.

Property path-limit probes submit oversized `stat` requests through the facade
and require structured resource-limit rejection. Requests remain within the
testkit allocation budget and respect the declared absolute/relative path form.
A different path limit that would mask the selected boundary is reported as a
skipped optional probe, with a reason.

`prepare_copy_cancellation` and `prepare_write_cancellation` return optional
stage-acknowledged probes. A probe must confirm a real pending provider stage;
the suite drops execution before disarming it. Independent observations verify
source preservation and publication claims during recovery. Missing optional
probes are reported explicitly. Use `run.assert_satisfied_with(&[check_id])`
to require selected probes; an executed probe failure always fails the run.

For a second, remote-provider signal, the repository has a separate unpublished
validation crate at `fixtures/s3-contract/`. It uses an S3-compatible endpoint
and the same public testkit to exercise real range reads, create-only writes,
conflicts, cancellation, and cleanup. The bucket must already exist; tests
write and remove objects only under a unique child of the configured prefix and
never create a bucket or clean an entire shared prefix. It is intentionally outside the
published provider dependency graph. A successful local testkit run does not
constitute remote-backend evidence: any claim about S3 compatibility must cite
that crate's environment, backend version, lockfile, and recorded run output.
This guide records the validation boundary only and does not claim that the
remote suite has been run.

The temporary-directory overwrite case starts with an independently seeded
empty destination and checks the published replacement contents. It does not
claim that providers must overwrite non-empty directories. When a cancellation
check reports `Published`, treat that as evidence that a target is observable;
the suite does not require it to equal the complete requested payload.

## Bounded asynchronous runs and cleanup

The testkit does not choose a runtime or deadline. A caller can bound the run
and cleanup independently. Keep the fixture and suite outside the timed async
block so the suite remains available after cancellation:

```rust,ignore
use std::time::Duration;
use qubit_fs_testkit::{AsyncFileSystemFixture, AsyncFileSystemContractSuite, ContractCheckId};

async fn check_with_deadlines<F: AsyncFileSystemFixture>(fixture: &F, id: ContractCheckId) {
    let mut suite = AsyncFileSystemContractSuite::new(fixture);
    let run = tokio::time::timeout(Duration::from_secs(30), async {
        suite.run_check(id).await;
    }).await;
    if run.is_err() {
        let interrupted = suite.run().was_interrupted();
        let cleanup = tokio::time::timeout(Duration::from_secs(30), suite.finish()).await;
        panic!("contract {id}: run deadline reached; interrupted={interrupted}; cleanup completed={}", cleanup.is_ok() && suite.run().cleanup().completed());
    }
    suite.run().assert_satisfied_with(&[id]);
}
```

The run and cleanup budgets are separate and may be adjusted for the provider.
The outer block returns `()`, so it does not keep a borrow of the suite alive.
After a run deadline, inspect retained interruption evidence, attempt cleanup,
then fail the original contract test even if cleanup succeeds. If cleanup also
times out, report cleanup as incomplete and let the fixture owner continue
recovery; do not claim asynchronous resources were reclaimed. Cooperative
runtime timeouts cannot preempt a blocking `poll`.

## Whole-file asynchronous write recovery

`qubit-fs` 0.3 requires owned payloads for `begin_write_all`. The async Write
phase checks `write/owning-operation` and `write/repeated-execute`, and actually
calls `prepare_write_cancellation` for Open, Write, Flush and Commit.
Implement `WriteCancellationProbe::poll_reached` to acknowledge that the selected
provider stage is pending; provide a wake while waiting and release the gate in
`disarm` without starting filesystem I/O. The suite cancels only the execution
future, preserves its operation, verifies confirmed progress and recovers any
writer after releasing the gate.

An advertised Write capability without a probe records `SkippedOptional` with a
reason for the corresponding `write/cancel-*` check. The normal
`run.assert_satisfied()` accepts that explicit optional skip, while
`run.all_applicable_checks_verified()` or
`run.assert_satisfied_with(&[ContractCheckId::WriteCancelOpen])` requires the
selected evidence. Request-only cases do not prove a pending stage was reached.
Providers without Write are checked for preflight rejection; asynchronous
cancellation does not apply.
The synchronous Write catalog contains none of these async requirements.

The memory fixture self-tests count all four probe preparations and deliberately
omit one stage to verify strict completeness. These gates use deterministic
polling, not elapsed-time sleeps. Existing copy cancellation hooks have their
own check identifiers and applicability rules.

## Filesystem contract update

The List contract includes independently selectable `list/namespace` and
`list/raw-root-prefix` checks. Flat fixtures implement `snapshot_namespace_paths`
using their SDK or fixture model, including keys created before the current
check. Without that observation, namespace completeness is **Unverified**.
The raw-prefix check distinguishes `folder`, `folder/a`, and `folderish` from
unrelated keys; literal filters are tested relative to a nonempty root.

For a sibling checkout of `rs-fs`, `rs-fs-local`, `rs-fs-registry`,
`rs-fs-testkit`, and `rs-mime`, use the ecosystem command below. The `tests`
phase runs this explicit matrix with `--no-default-features` on every command:
`rs-fs` with no extra feature and with `async`; `rs-fs-local` with no extra
feature and with `registry`; `rs-fs-registry` with no extra feature and with
`async`; `rs-fs-testkit` with no extra feature and with `async`; and `rs-mime`
with no extra feature. The test phase also invokes the separate
`fixtures/s3-contract/` test command. The `ci` phase
runs only `rs-fs-testkit/.infra/bin/ci-check.sh`; it does not add downstream
checks to this crate's CI. Both phases validate the core dependency graph and
compare Cargo.lock snapshots for their selected crate set, failing if a
lockfile changes. S3 tests are opt-in and are not counted as passing when
ignored.

```bash
python3 scripts/check-fs-ecosystem.py --sibling-root .. --phase tests
python3 scripts/check-fs-ecosystem.py --sibling-root .. --phase ci
```

## Recovery ownership

Negative writer/temp open checks retain the complete `OpenFailure<R>` when the
provider violates its contract. Inspect `OpenFailureStage` and match recovery
variants; rejected sessions only permit explicit cleanup. Whole-file operations
use `WriterRecovery` / `AsyncWriterRecovery`, with `Opened` and `Rejected` variants.
The suite does not add synthetic mandatory commit failures to healthy providers:
its own recording providers test immutable failure state and confirmed bytes.

An expected conditional rejection may still own an opened writer. Before marking
IfAbsent, IfMatch, CreateConflict or WriteLimit satisfied, the suite confirms
non-publication and explicitly aborts that session when one is retained. Without
a recovery session, the original failure state is preserved and the target result
is checked through independent observation. Failed cleanup retains
`ContractWriterFailure<WriteAllFailure>` or
`ContractWriterFailure<ContractAsyncWriteFailure>`: `error()` describes cleanup,
while `writer()` owns the original failure and its recovery responsibility.
Recovering it never changes historical state or bytes. For `WriteLimit`, the
suite independently records whether the over-limit destination existed before
the request. Existing `CreateOrReplace` targets also require an observed byte
baseline. After typed recovery cleanup, the suite independently confirms
absence or unchanged existence and bytes. Missing observation support is
`Unverified`, and observation errors or changed state fail the check; teardown
success is not evidence about the rejected write. Missing optional write
cancellation probes are explicit skips; executed probe failures remain failures.
Existing check IDs remain unchanged; range checks include zero and EOF windows
when capability permits.

For temporary resources, testkit 0.9 follows the qubit-fs 0.3 contract that keeps
publication facts independent from source qualification. This suite checks the
existing repeated lifecycle, including the publication target after a successful
`keep`. Core and adapter regressions cover the new source states, invalid retries,
cleanup failures, and cancellation.

## Errors and Diagnostics

Suites use assertion failures with phase-specific messages. When a capability
is not advertised, they expect a structured `UnsupportedCapability` error with
the relevant operation and required capability context. Failures in fixture
mapping or hooks are surfaced as `FixtureError`/`FixtureResult` failures.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| The properties phase fails | Ensure IDs are non-empty, capabilities have no missing dependencies, and fixture paths satisfy facade constraints. |
| A core unadvertised operation fails the suite | Return the structured unsupported-capability preflight error instead of succeeding or using an unrelated error. |
| State leaks across runs | Create an isolated fixture, keep its resources alive for the suite, and make independent `teardown` reclaim all resources even when `Delete` is unavailable. |
| A provider-specific assertion is impossible | Leave the relevant optional hook unsupported and add a provider-owned test for that behavior. |

## Limitations and Best Practices

- The contracts are capability-driven, not a claim that every provider has the
  same feature set.
- Platform behavior, path encoding, security boundaries, service registration,
  and capabilities outside current suite coverage remain provider-owned tests.
- The testkit is a development dependency; do not add it to the provider's
  production dependency surface.

## Further Reading

- [README](../README.md)
- [中文用户手册](user_guide.zh_CN.md)
- [API documentation](https://docs.rs/qubit-fs-testkit)
- [English design](../doc/file_system_testkit_design.md)
- [中文设计](../doc/file_system_testkit_design.zh_CN.md)
