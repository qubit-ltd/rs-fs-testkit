# S3 contract fixture

This unpublished standalone crate validates `qubit-fs` against an existing
S3-compatible service. It never creates buckets and does not discover credentials
through ambient SDK configuration. The real-service test creates objects under a
unique child of the explicitly configured prefix and cleans only its exact keys.

Set `RS_FS_S3_ENDPOINT`, `RS_FS_S3_BUCKET`, `RS_FS_S3_REGION`,
`RS_FS_S3_ACCESS_KEY_ID`, `RS_FS_S3_SECRET_ACCESS_KEY`, and a unique non-empty
`RS_FS_S3_PREFIX`. For local HTTP services set `RS_FS_S3_ALLOW_HTTP=true`.
Credentials are omitted from debug output. Explicitly running the ignored test
fails when configuration is missing; ignoring it is not real-service evidence.
It performs actual PUT, GET, HEAD and exact-result LIST operations before cleanup.

The adapter declares Conditional List, Read, RangeRead, and Write capabilities.
Create-only writes are bounded to 1 MiB per session. It does not advertise
ConditionalRead, rename, deletion, append, replacement, durable publication, or
temporary resources. The SDK-backed test fixture prepares, observes, snapshots,
and cleans data independently of the facade. The default local matrix covers
Properties, Stat, Read, List and selected supported CreateNew writer contracts.

`ListScope::Namespace` covers exactly the configured namespace. A path scope is a
raw logical-key prefix: `folder` selects `folder`, `folder/a`, and `folderish`.
`LiteralPrefix` is relative to that scope, so scope `folder/` plus filter `a`
selects `folder/a` and `folder/ab`. Because object_store 0.14.1 lists by component
prefix, the SDK query uses only the configured namespace and matching is applied
locally. Each stream scans at most 1024 SDK entries, including nonmatches; the
1025th entry raises `ResourceLimitExceeded`. This scan ceiling is separate from
`ListOptions::max_entries`. Returned keys outside the configured prefix are a
provider contract violation, never an alternative namespace.

Resource keys and configured prefixes must round-trip through SDK Path parsing
exactly. Empty keys, leading/trailing slashes, repeated slashes, literal dot
segments, and NUL are rejected before I/O. Percent-encoded text, spaces, and
Unicode remain literal when the SDK can represent them exactly. Query prefixes
are not resource keys: a listing scope ending in `/` remains valid.
`read_prefix` does not automatically add ranges for this Conditional provider.
Reader stream failures retain the typed filesystem error and SDK source chain.

```bash
cargo +1.94.0 check --all-targets
cargo test --lib --test adapter_unit_tests --test s3_listing_tests
cargo test --test s3_contract_tests -- --ignored --test-threads=1
```
