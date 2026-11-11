# qubit-fs-testkit design

This document describes the public contract model implemented by
`qubit-fs-testkit` 0.9 against `qubit-fs` 0.3. The crate is development-only test support: it exercises a
provider through `qubit_fs::FileSystem` or `qubit_fs::AsyncFileSystem` and never
depends on provider SPI types.

## Architecture

```text
provider fixture -> contract suite -> public qubit-fs facade -> provider
```

`FileSystemFixture` and `AsyncFileSystemFixture` own an isolated namespace,
map test-relative names to provider paths, and expose independent setup and
observation hooks. The suite owns one borrowed `ContractContext`, a resource
ledger, one immutable properties snapshot, and one `ContractRun`. Fixtures
must use an observation channel independent from the facade when they seed or
inspect a premise; otherwise the same provider defect could satisfy both sides
of a check.

The `read_all` budget applies to bytes consumed from the selected window, not
the full resource or its opened metadata length. A stale length overestimate
cannot reject a shorter stream before reading it. The suite opens the reader
even for a zero-length window so provider errors remain observable. For a range
read, an offset at EOF or beyond EOF returns an empty window, and a requested
length extending past EOF is truncated. This successful open/read does not
promise a content snapshot.

## Evidence boundaries

The suite directly observes behavior available through the public facade and
independent fixture hooks: operation results, required side effects, selected
metadata, and fixture-reported outcomes. `Unverified` means a required premise
or result could not be independently observed; it is not a pass. A cancellation
recovery reported as `Published` means the target is independently observable,
not that it contains the complete requested payload.

The testkit's own recording and deliberately faulty fixtures test suite
bookkeeping, recovery ownership, and whether checks reject selected violations.
They do not make those internal checks provider guarantees. In particular,
properties getter I/O behavior, clone snapshot identity, secret-free
diagnostics, and every operation's `Display`/`Debug`/`Error::source` contract
are not established by provider-suite observations. Core facade/SPI tests own
core dispatch and preflight guarantees; providers own native behavior, platform
semantics, and service-specific guarantees beyond the suite's observations.

| Evidence claim | Responsible tests |
| --- | --- |
| Public facade results and side effects independently observed by a provider fixture | The provider contract suite, for the checks it executes with complete evidence |
| Suite bookkeeping, recovery ownership, and rejection of injected violations | Testkit self-validation with recording and deliberately faulty fixtures |
| SPI dispatch/preflight, compile-time borrowing properties, drop/runtime behavior, and provider-specific platform or service guarantees | Core or provider tests at the layer that owns the behavior |

Positive random-access samples respect the declared `max_read_range_bytes`
limit and accumulate short reads until the expected sample is complete.
`RandomAccessBounds` checks an empty buffer, EOF, clipping, overflow rejection,
and reuse of the same reader after the error when non-empty boundary probes are
possible. A valid `Maximum(0)` makes those probes impossible, so Bounds is
`SkippedOptional` with a reason. It does not establish the provider's maximum
range length. `RandomAccessRangeLimit` is a separate check: for `Maximum(n)` it
independently prepares a reader, verifies that `n` bytes are accepted and
`n + 1` are rejected with `ResourceLimitExceeded` for `Read`, then verifies
reader reuse. For `Maximum(0)`, the accepted and post-rejection reads are empty
requests; the one-byte request must still be rejected. Zero is a valid limit,
but this check does not claim a non-empty read can succeed under it. Its seed
and requests stay within a 64-byte probe budget. A finite maximum at or above
that budget is `SkippedOptional` with a reason; `Unknown`, `Unbounded`, and
`NotApplicable` declarations produce
`NotApplicable`. Missing seed support is `Unverified`; execution errors and
incorrect bytes or errors fail. Without `Read`, all random-access checks are
`NotApplicable` before declaration or preparation hooks run; `ReadBasic` still
checks the facade's structured capability rejection. Callers requiring both
boundary and finite-limit evidence must select both IDs in
`assert_satisfied_with`.

The async `RandomAccessCancel` check passes only if polling the injected read
returns `Pending` and `random_access_pending_read_reached` reports
`FixtureSupport::Supported(true)`. The hook is independent fixture
instrumentation and must not perform I/O; its default is `Unsupported`.
After polling, the suite drops the future, queries the hook, disarms the gate,
and confirms the reader remains usable. No injected gate is an optional skip;
missing or negative gate confirmation is `Unverified`; hook errors, panics,
or failed post-disarm reader use are failures. This distinguishes cancellation
evidence from merely constructing a request that might block.
An expected rename rejection is `RejectedAsExpected` only when its structured
error is correct and independent before/after observations prove the relevant
paths unchanged. A provider-reported `RenameFailureState::Unchanged` is not
independent evidence; missing required observations produce `Unverified`.

Listing checks compare independently prepared entry sets, paths, and filters,
and require metadata presence when requested. A `page_size` hint of one tests
result completeness, not actual service pagination or SPI arguments. The page
limit property check exercises the public clamp calculation. These observations
do not establish every metadata field's absent/unknown semantics, stream
termination after errors, or all provider-violation detection; core and
provider-specific tests own those additional guarantees.
For hierarchical paths, `ListBasic` includes direct files and child directories
but excludes grandchildren. `ListPrefix` uses a `Subtree` filter and recursive
traversal, and its expected set includes the selected directory and all seeded
descendants. Fixtures must seed directories independently with
`seed_empty_directory`; file seeding does not imply directory visibility.

## Metadata and read evidence boundaries

| Area | Evidence checked by this suite | Other ownership / not established here |
| --- | --- | --- |
| `stat` | `StatBasic` checks structured `NotFound/Stat` for an independently generated missing path. `StatFileKind` checks file-like kind for an independently seeded resource and compares its length when known. | `len == None` is allowed. Logical path, final-symlink resolution, and platform behavior belong to core facade and provider tests; this pass did not confirm specific downstream test locations. |
| `exists` | No named provider-suite check independently verifies `exists` error swallowing. | The facade rule that only `NotFound` is swallowed belongs to core dispatch tests. |
| reader/read | `ReadBasic` and related scenarios check seeded content and applicable structured errors. `ReadIfMatch*`, `ReadIfNoneMatch*`, `ReadChecksum`, and `ReadChecksumCorruption` exercise their declared conditional/checksum cases when applicable. Range checks include zero-length, EOF, and beyond-EOF empty windows. | Ordinary reader checks do not independently prove `FileSystemId + Path` identity or open-time snapshot semantics. Reader identity, snapshots, and final-symlink behavior belong to core/provider tests. Random-access identity checks do not establish ordinary reader identity coverage. |
| range/limits | `read_all` applies the caller limit to bytes consumed from the selected window; range checks independently verify bytes, EOF truncation/empty windows, and applicable filesystem range limits. | These checks do not prove immutable snapshot contents or a specific service-side range implementation. |

The suite's missing-path `stat` and read-scenario observations do not establish that the facade's `exists` swallows only `NotFound`; that dispatch rule is owned by core tests. Do not describe properties absent from the evidence column as provider-suite guarantees.

## Typed catalog and execution

Every check has one `ContractCheckId`, one `FileSystemContract` phase, a
capability claim, a typed scenario where setup needs one, and an optional/core
policy. The catalog is registered before provider code runs. A phase cannot
claim success while a registered check is missing, pending, unverified, or
failed. `run_check(id)` executes one catalog entry; `run_contract(phase)`
executes that phase; `run_all()` executes all phases. Each suite is a single
session and must be recreated for another run.

Outcomes distinguish `Passed`, `RejectedAsExpected`, `NotApplicable`,
`Unverified`, `SkippedOptional`, `Failed`, and `NotRun`. Missing instrumentation
is explicit. An optional check may be skipped only with a reason, while an
executed optional check that fails still fails the run. Callers can require
selected optional checks with `ContractRun::assert_satisfied_with` or demand
all applicable evidence with `all_applicable_checks_verified`.

## Lifecycle and failures

Resources are recorded as soon as setup publishes or stages them. Cleanup
continues after individual failures, retains unresolved ledger entries, and
records every attempt. `ContractRun` keeps execution failures, cleanup
failures, and their typed source values. No asynchronous I/O occurs in `Drop`;
an asynchronous caller owns the runtime and may retry `finish().await`. Sync
and async fixtures must both provide an independent, idempotent explicit
teardown that handles partial setup and staging resources. `Drop` may supplement
cleanup but cannot replace teardown; async `Drop` performs no I/O.

The first `finish` attempt closes the preparation and run entry points. If
cleanup fails or an asynchronous cleanup future is cancelled, callers may retry
cleanup with `finish`, but must create a new fixture and suite before another
run. `ContractReport` validates check evidence only; it does not include run
completion, execution failures, or cleanup. Use `ContractRun` assertions to
validate the complete run.

Writer and copy cancellation checks retain the operation and publication
state across cancellation. Recovery is explicit: taking a retained writer or
operation transfers responsibility to the caller, while the original failure
remains available for diagnostics.

## Compatibility and verification policy

The synchronous and asynchronous registration macros accept an optional
trailing `required_checks` function or closure. It maps each generated
`FileSystemContract` phase to a `Vec<ContractCheckId>` and is evaluated once
before that phase runs. The wrapper forwards the list to
`ContractRun::assert_satisfied_with`; no field means an empty list and retains
the prior policy. The wrapper does not filter IDs, so a check selected outside
the current phase fails with the report's normal diagnostic. Async registration
evaluates the callback before calling the runner and moves the list into its
future. Macros only wrap phase execution and assertions; they do not make
capability decisions or implement evidence checks. Directory rejection and
independent negative side-effect evidence remain responsibilities of the suite
and its fixture hooks, not the selector callback.

The testkit checks advertised capability preconditions before side effects and
requires structured `UnsupportedCapability` or `RequirementNotMet` errors when
an operation or stronger guarantee is unavailable. Copy checks separately
cover basic copy, conflict policy, server-side copy, atomic/durable file copy,
and atomic/durable tree copy. Tree fixtures must seed the root, descendants,
and target independently.

The repository contains an in-memory provider model for deterministic faults
and an unpublished S3-compatible fixture under `fixtures/s3-contract/`.
The latter requires an explicit endpoint, an existing bucket, credentials,
region, and a unique prefix. It creates and cleans only objects under a unique
child of that configured prefix; it never creates a bucket or cleans an entire
shared prefix. It must not be treated as evidence for an unconfigured service.

Coverage thresholds apply to every source file without exemptions. Provider-
dependent contract branches must be exercised by deterministic fixture profiles
and focused regression tests, including their native capability and failure
combinations.


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
Recovering it never changes historical state or bytes. Missing optional write
cancellation probes are explicit skips; executed probe failures remain failures.
Existing check IDs remain unchanged; range checks now include zero and EOF
windows when capability permits.

`ReadRange` also exercises the caller's `read_all` byte budget on its selected
window: a budget equal to the window length must return the seeded bytes, and
one byte less must fail with `ResourceLimitExceeded` for `Read`. This is a
facade budget check, separate from the provider's `max_read_range_bytes` limit.
The `max_bytes` budget is enforced by the facade and is not passed in the SPI
request; this checks caller-side consumption, not a provider or service-side
byte budget.
`WriteLimit` captures an independent existence baseline before the rejected
request, captures existing bytes when the fixture prepared `CreateOrReplace`,
cleans any retained recovery session, then independently confirms the target
remains absent or unchanged. Missing observation support is unverified; errors
or changed target state fail the check.

Cancellation diagnostics use the following ownership mapping:

| Failure path | Retained source | Recovery |
| --- | --- | --- |
| Async write/copy execution fails before the requested stage | `ContractAsyncWriteFailure` / `ContractAsyncCopyFailure` | `operation_mut().take_recovery()` |
| Acknowledgement, disarm, or operation-state diagnostic fails | `ContractWriterFailure<AsyncWriteAllOperation>` / `ContractWriterFailure<AsyncCopyOperation>` | Downcast `ContractSource`, inspect `error()` as `ContractFailure`, then use `writer_mut().take_recovery()` |

The second form also retains operations without an opened writer, preserving
their diagnostic state. If recovery is present, the caller explicitly aborts
the recovered writer; taking the source transfers that responsibility.

### Caller-owned asynchronous deadlines

The runtime-neutral suite has no built-in deadline. Callers may wrap a
manually managed `run_check` future in their runtime's timeout, then inspect
`suite.run()` after the timed future is dropped and call `suite.finish()` under
a separate cleanup budget. A run timeout remains a failed contract even when
cleanup completes. If cleanup also times out, retained cleanup evidence does
not prove that asynchronous resources were reclaimed. Cooperative timeouts
cannot preempt a blocking `poll`. The registration macro runner owns the whole
fixture/suite wrapper; timing out that wrapper does not expose its retained run
for inspection or another `finish()` attempt.

Temporary-resource checks independently seed and verify non-empty file bytes
and directory child contents across keep, persist, overwrite, and repeated
lifecycle operations. Unsupported content hooks are reported as `Unverified`;
setup errors remain failures. The suite also retains its typed temporary
resource recovery path. Testkit 0.7 targeted `qubit-fs` 0.2; that is historical,
not the compatibility target of this 0.9 design. Directory overwrite uses an
independently seeded empty destination as its baseline; it does not establish
behavior for overwriting a non-empty directory.

Synchronous expected-write cleanup catches an abort panic at the provider
boundary and retains both the original `WriteAllFailure` and panic payload for
explicit caller recovery. This does not change the recorded failed run.
