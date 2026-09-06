# S3 contract fixture

This unpublished standalone crate validates `qubit-fs` against an existing
S3-compatible service. It never creates buckets or cloud resources and does
not discover credentials through ambient SDK configuration.

Set `RS_FS_S3_ENDPOINT`, `RS_FS_S3_BUCKET`, `RS_FS_S3_REGION`,
`RS_FS_S3_ACCESS_KEY_ID`, `RS_FS_S3_SECRET_ACCESS_KEY`, and a unique non-empty
`RS_FS_S3_PREFIX`. For local HTTP services set `RS_FS_S3_ALLOW_HTTP=true`.
Credentials are omitted from debug output. The ignored integration test fails
when configuration is missing.

The adapter supports object-key paths, bounded create-only writes (1 MiB per
session), reads, range reads, and conditional reads. It intentionally does not
claim listing, rename, deletion, append, replacement, durable publication, or
temporary resources. Literal dot segments are rejected; percent-encoded text
is preserved as object-key text.

```bash
cargo +1.94.0 check --all-targets
cargo test --test adapter_unit_tests
cargo test --test s3_contract_tests -- --ignored --test-threads=1
```
