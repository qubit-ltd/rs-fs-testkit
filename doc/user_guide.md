# qubit-fs-testkit User Guide

[中文](user_guide.zh_CN.md) · [README](../README.md) · [API documentation](https://docs.rs/qubit-fs-testkit)

## Purpose and Audience

This guide is for authors of synchronous or asynchronous `qubit-fs` providers.
It covers the current `qubit-fs-testkit` 0.1 contract suites, which are test
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
prefixes. Optional fixture hooks can seed/read files and prepare native-copy
cases. The async fixture supplies equivalent future-based observations and
optional copy cancellation cases.

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

Put the suite in the provider's integration tests and create a new fixture per
test run:

```rust,ignore
use qubit_fs_testkit::{FileSystemContractSuite, FileSystemFixture};

let fixture = TestFixture::new();
FileSystemContractSuite::new(&fixture).assert_all();
```

Both suites check properties, `stat`, read, write, list, directory creation,
delete, copy, rename, append, recursive deletion, required-atomic
rename/replacement, required-durable copy, temporary resources including
atomic persistence, and error context, then perform cleanup. Unadvertised core
operations are checked for structured `UnsupportedCapability` preflight.
Unadvertised stronger guarantees are checked for structured `RequirementNotMet`
preflight.

For an async facade, await the parallel suite:

```rust,ignore
use qubit_fs_testkit::AsyncFileSystemContractSuite;

let fixture = AsyncTestFixture::new();
AsyncFileSystemContractSuite::new(&fixture).assert_all().await;
```

## Advanced Usage

Use optional fixture hooks only when the generic suite needs a provider-owned
observation outside the operation being checked. Examples include `seed_file`,
`read_file`, and `copy_fast_path_case`. Return `FixtureSupport::Unsupported`
for an optional observation the provider cannot supply; it is not a fabricated
assertion.

`seed_file` and `read_file` must use an observation channel independent of the
facade under test. For example, a local-provider fixture can seed and inspect
its isolated temporary directory through native filesystem APIs. Reusing the
same facade for setup or observation can make a matching read/write defect pass
the contract suite.

When individual phases are called directly, call `finish()` afterward to clean
the resources those phases created. `assert_all()` calls it automatically; the
synchronous suite also cleans up before rethrowing an assertion panic.

`AsyncFileSystemFixture` additionally has `copy_cancellation_case` for
provider-owned pending-stage controls. It is optional, as are the other
provider-specific observations.

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
