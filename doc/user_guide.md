# qubit-fs-testkit User Guide

[中文](user_guide.zh_CN.md) · [README](../README.md) · [API documentation](https://docs.rs/qubit-fs-testkit)

## Purpose and Audience

This guide is for authors of synchronous or asynchronous `qubit-fs` providers.
It covers the current `qubit-fs-testkit` 0.3 contract suites, which are test
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
when deletion is available.

## Installation and Minimal Configuration

Add the testkit as a development dependency in the provider crate:

```bash
cargo add --dev qubit-fs-testkit
```

Implement `FileSystemFixture` for a fixture that owns or otherwise retains the
resources required to keep its filesystem isolated. At minimum, implement
`file_system` and `path`; `list_prefix` has a default implementation. Implement
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

The `seed` and `required_seed` helpers on `FileSystemContractSuite` and
`AsyncFileSystemContractSuite` take `&mut self` so resource ownership is tracked
in the suite ledger. Providers calling these helpers directly must make the
corresponding call sites mutable; this is a source-compatible migration only
for code that already owns a mutable suite.

Prefer `assert_contract(FileSystemContract)` for one phase because it cleans up
before resuming a panic. When low-level phase methods are called directly, call
`finish()` afterward. `assert_all()` and both registration macros clean up
automatically, including asynchronous assertion panics.

`AsyncFileSystemFixture` additionally has `copy_cancellation_case` for
provider-owned pending-stage controls. The corresponding
`AsyncFileSystemContractSuite::assert_copy_cancellation()` phase can be run
independently when a provider wants a focused check. The hook is optional, as
are the other provider-specific observations.

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
