# qubit-fs-testkit design

This document describes the public contract model implemented by
`qubit-fs-testkit`. The crate is development-only test support: it exercises a
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
an asynchronous caller owns the runtime and may retry `finish().await`.

Writer and copy cancellation checks retain the operation and publication
state across cancellation. Recovery is explicit: taking a retained writer or
operation transfers responsibility to the caller, while the original failure
remains available for diagnostics.

## Compatibility and verification policy

The testkit checks advertised capability preconditions before side effects and
requires structured `UnsupportedCapability` or `RequirementNotMet` errors when
an operation or stronger guarantee is unavailable. Copy checks separately
cover basic copy, conflict policy, server-side copy, atomic/durable file copy,
and atomic/durable tree copy. Tree fixtures must seed the root, descendants,
and target independently.

The repository contains an in-memory provider model for deterministic faults
and an unpublished S3-compatible fixture under `fixtures/s3-contract/`.
The latter requires explicit endpoint, bucket, credentials, region, and a
unique prefix; it never creates remote resources and must not be treated as
evidence for an unconfigured service.

Coverage thresholds continue to apply to the catalog, report, run, and
resource-ledger core. Provider-dependent contract branches are listed as
reviewed threshold exemptions because one fixture cannot truthfully exercise
every native capability and failure combination; the deterministic matrix and
focused regression tests remain the executable coverage for those branches.
