# qubit-fs-testkit User Guide

[中文](user_guide.zh_CN.md) · [README](../README.md) · [API documentation](https://docs.rs/qubit-fs-testkit)

## Purpose and Audience

This guide is for authors of synchronous or asynchronous `qubit-fs` providers.
It covers the current `qubit-fs-testkit` 0.4 contract suites, which are test
support and therefore belong in provider development dependencies.

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
cargo add --dev qubit-fs-testkit
```

Implement `FileSystemFixture` for a fixture that owns or otherwise retains the
resources required to keep its filesystem isolated. At minimum, implement
`file_system`, `path`, and independent `teardown`; `list_prefix` has a default implementation. Implement
`AsyncFileSystemFixture` for an asynchronous facade; it has the same required
mapping methods and is `Sync`.

## Core Workflow

Use the registration macro to create a fresh fixture and a precise test name for
each contract phase:

```rust,ignore
qubit_fs_testkit::register_file_system_contract_tests! {
    module: provider_contracts,
    fixture: super::TestFixture::new,
}
```

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

The read budget applies to the selected window, rather than to the complete
resource. If opened metadata reports a total length, the suite computes
`min(max(0, total_length - offset), requested_length)` (or the remaining length
when no length is requested) and compares that value with `max_bytes`. For
example, reading `offset = 2`, `length = 3` from `0123456789` with
`max_bytes = 3` must return `234`; a budget of `2` must return
`ResourceLimitExceeded`. Unknown resource length cannot be preflight-rejected,
but the actual stream is still checked against the budget. The suite always
opens the reader first, including for a zero-length window, so not-found,
permission, and condition errors are preserved.

After a successful writer commit, a second commit is an `InvalidState` error
whose `WriteFailureState` is `Published`. It must not issue another provider
commit or automatically abort the writer, and the published target remains
observable. A retryable `NotPublished` failure remains the only non-terminal
case that may be committed again.

For an async facade, pass a runtime-specific future runner:

```rust,ignore
qubit_fs_testkit::register_async_file_system_contract_tests! {
    module: async_provider_contracts,
    fixture: super::AsyncTestFixture::new,
    runner: super::runtime::block_on,
}
```

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

Each suite is a single execution session. A completed or interrupted session
cannot run again; create a fresh fixture and suite. For an asynchronous suite,
await the same entry points using your own runtime. If its future is dropped,
inspect `suite.run()` and retry `suite.finish().await` for cleanup. Dropping the
entire suite performs no asynchronous I/O, so the fixture owner must arrange
independent teardown.

The independent `write/abort` check prepares a fresh `Abort` scenario and uses
`exists_out_of_band` before and after abort. `NotPublished` requires the target
to remain absent; `Published` reports a changed target, and `Indeterminate`
preserves uncertainty. The suite also checks the corresponding writer state.
If streaming or abort fails, the run retains a `ContractWriterFailure<W>` inside
`ContractSource`, where `W` is `FileWriter` or `AsyncFileWriter`. Take the source,
downcast it, and use `writer_mut()` for explicit recovery. The original error
remains available through `error()`; successful recovery does not change the
failed run into a passing one.

Unexpected failures from prepared asynchronous whole-file writes retain
`ContractAsyncWriteFailure`. Its `failure()` preserves the publication state and
accepted byte count. `operation_mut()` exposes the operation that owns any
recovery writer; call `take_recovery_writer()` to take that writer for explicit
abort. Admission failures have no operation. Taking the source transfers recovery
responsibility; dropping it does not perform asynchronous abort.

Copy probe execution failures retain `ContractAsyncCopyFailure`, pairing the
original `AsyncCopyFailure` snapshot with the admitted operation. If a write or
copy fails before the requested cancellation stage, the runner first drops its
execution future and disarms the probe, then retains the operation for recovery.
If recovery abort itself fails, `ContractWriterFailure<AsyncFileWriter>` keeps
the writer available for an explicit retry.

Temporary-resource failures retain `ContractTempFailure<T>`, where `T` is the
concrete synchronous or asynchronous temporary file or directory type. Take and
downcast the `ContractSource`, inspect the original error with `error()`, then use
`resource_mut()` for explicit recovery. Ownership transfer does not clear the
failed run, and dropping the wrapper does not perform asynchronous cleanup.

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
guarantee and returned path identities.

Listing checks prepare separate namespaces. Subtree results use a fixed expected
directory and file set; pagination has its own three-entry request with a page
size hint of one. Hierarchical paths use subtree filters, while raw object keys
use `ListOptions::object_keys()` and literal-prefix filters. Cross-model filters
must produce structured rejection. Returned entries are bounded so duplicates
or unexpected entries cannot grow the collected result without limit.

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
conflicts, cancellation, and cleanup. It is intentionally outside the
published provider dependency graph. A successful local testkit run does not
constitute remote-backend evidence: any claim about S3 compatibility must cite
that crate's environment, backend version, lockfile, and recorded run output.
This guide records the validation boundary only and does not claim that the
remote suite has been run.

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
| State leaks across runs | Create an isolated fixture and ensure its resources remain alive for the suite; cleanup is only attempted when delete is available. |
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

## Whole-file asynchronous write recovery (0.3)

`qubit-fs` 0.4 requires owned payloads for `begin_write_all`. The async Write
phase checks `write/owning-operation` and `write/repeated-execute`, and actually
calls `prepare_write_cancellation` for Open, Write, Flush and Commit.
Implement `WriteCancellationProbe::poll_reached` to acknowledge that the selected
provider stage is pending; provide a wake while waiting and release the gate in
`disarm` without starting filesystem I/O. The suite cancels only the execution
future, preserves its operation, verifies confirmed progress and recovers any
writer after releasing the gate.

An advertised Write capability without a probe yields `Unverified` for the
corresponding `write/cancel-*` check; `report.assert_complete()` then fails.
Request-only cases do not prove a pending stage was reached. Missing a single
stage is enough to make the report incomplete. Providers without Write are
checked for preflight rejection; asynchronous cancellation does not apply.
The synchronous Write catalog contains none of these async requirements.

The memory fixture self-tests count all four probe preparations and deliberately
omit one stage to verify strict completeness. These gates use deterministic
polling, not elapsed-time sleeps. Existing copy cancellation hooks have their
own check identifiers and applicability rules.
