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

The adapter declares Conditional List, Read, RangeRead, Write, and ConditionalWrite
capabilities. Buffered single-PUT writes are bounded to 1 MiB per session. Supported
combinations are CreateNew with no precondition or IfAbsent, CreateOrReplace with
IfAbsent, and CreateOrReplace with IfMatch using an ETag. The SDK applies Create or
Update atomically at PUT; a HEAD followed by an unconditional PUT is never used.
Unconditional replacement, ConditionalRead, rename, deletion, append, durable
publication, and temporary resources are not advertised. The SDK-backed test fixture prepares, observes, snapshots,
and cleans data independently of the facade. The default local matrix covers
Properties, Stat, Read, List, supported creation and conditional writes, and
open/write/flush/commit cancellation contracts with acknowledged stage probes.

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
cargo test --locked --lib --test adapter_unit_tests --test s3_listing_tests --test s3_recovery_tests --test s3_inflight_put_tests --test s3_range_http_tests
cargo test --test s3_contract_tests -- --ignored --test-threads=1
```

## Recovery evidence and its limits

`TestControl` is fixture-only instrumentation. It gates Open, Write, Flush,
BeforePut, AfterPutBeforeResult, and Abort, records acknowledged bytes and PUT/abort
calls, and can fail one abort. Tests wait for explicit stage acknowledgement;
there are no timing sleeps. The session enters Indeterminate before awaiting PUT,
so cancellation of an in-flight request cannot make abort claim NotPublished.
Conditional conflicts are NotPublished; unknown transport outcomes remain
Indeterminate. SDK retries are disabled and the facade never resends automatically.
Abort never deletes an already published target.

| Evidence | What it proves | What it does not prove |
| --- | --- | --- |
| InMemory shared matrix | Conditional ETag behavior, stage ownership, failed-abort retry, independent GET observations | Real service or transport behavior |
| SDK result suppression | PUT completed and GET sees bytes while the facade's result is withheld | Actual network response loss |
| Local HTTP in-flight PUT | Request received while response is pending; cancellation and abort remain Indeterminate | Remote service commit state |
| Explicit ignored service matrix | Shared checks against the configured endpoint, independent SDK observations | Other services or untested network failure modes |

Zero-length reads issue HEAD and return an empty stream with full-resource
metadata. An EOF range rejection becomes an empty result only after a conditional
HEAD confirms the offset is at or beyond object length. Other failures, including
NotFound and authentication/permission failures, are not converted to empty data.
Range error classification is tied to object_store 0.14.1: SDK-native range errors
are distinguished through their source type and variant; HTTP 416 is recognized
from that version's narrow status-error prefix and requires the metadata proof.
Revalidate that boundary when upgrading the SDK.

The real recovery test and the InMemory test call the same matrix. The fixture
records exact keys prepared for the run and attempts their cleanup even when a
matrix assertion panics. It never deletes by listing an entire shared prefix.
A missing environment leaves ignored tests unverified; local passing results do
not claim remote validation.
