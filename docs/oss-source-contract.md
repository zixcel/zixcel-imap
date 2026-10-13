# OSS source contract: zixcel/zixcel-imap

Check a IMAP mail integration configuration and produce a plan before implementing or enabling the connection.

## Responsibility boundary

Own the named reusable integration/data mechanism. Product roles/UI and operational approvals stay with consumers; transport/custody reuse managed Crowsi components where capabilities fit. Planning-only APIs must remain clearly distinguished from implemented external operations.

Same-function managed packages take priority when their actual capabilities fit. Do not add an unrelated dependency merely to connect organizations. Preserve upstream LICENSE/NOTICE and immutable release/history evidence; unused compatibility sources belong in Git history, not the supported API.

## Package ownership

| Manifest | Package | Distribution policy |
| --- | --- | --- |
| `Cargo.toml` | `zixcel-imap` | source-only; publish=false |
| `runtime/services/auth/Cargo.toml` | `auth` | source-only; publish=false |
| `runtime/services/imap-curator-auth-helper/Cargo.toml` | `imap-curator-auth-helper` | source-only; publish=false |
| `runtime/services/imap-curator-collector/Cargo.toml` | `imap-curator-collector` | source-only; publish=false |
| `runtime/services/imap-curator-operator/Cargo.toml` | `imap-curator-operator` | source-only; publish=false |
| `runtime/services/libs/checkback-core/Cargo.toml` | `checkback-core` | source-only; publish=false |
| `runtime/services/libs/db/Cargo.toml` | `db` | source-only; publish=false |
| `runtime/services/libs/imap-curator-common/Cargo.toml` | `imap-curator-common` | source-only; publish=false |
| `runtime/services/libs/oauth-transport/Cargo.toml` | `oauth-transport` | source-only; publish=false |
| `runtime/services/libs/observability/Cargo.toml` | `observability` | source-only; publish=false |

Manifest versions and publish flags are declarations, not proof of published availability. Public npm exports and Rust implementation/docs belong to the manifests above; generated dist and sibling checkouts are not source-of-truth implementations.

## Source completion criteria

1. The documented purpose, public exports, validators/schema and actual implementation agree. Remove only confirmed-unused aliases, schemas and compatibility adapters after checking consumers; a shared field name does not imply a shared trust responsibility.
2. Runtime and official dependency resolution work from an independent checkout with committed locks. No local sibling artifact, private registry or ambient owner credential is required for the OSS source path.
3. Supported behavior has reproducible examples/playground and functional tests, including invalid input and relevant failure/cancellation/replay/tenant boundaries. A policy-only CI or planning fixture does not prove external effects.
4. CI runs the declared functional acceptance without disguising skipped/unavailable checks as success. Changed public package bytes receive a new reviewed release; immutable old versions are never overwritten.
5. License/NOTICE, security reporting, usage and the actual supported/unsupported scope stay in the owning repository. The owner operates independently of a central private management checkout.
6. Source acceptance, artifact/registry acceptance and production deployment are separate records. This document defines the target; it does not declare unexecuted tests, unreleased APIs or live-account validation completed.

## Existing verification entrypoints

- [.github/workflows/ci.yml](../.github/workflows/ci.yml)
- [.github/workflows/oss-policy.yml](../.github/workflows/oss-policy.yml)

Use the package scripts/owner CI above and the repository README for exact commands. When schema files exist, their canonical validator and consumer must be checked together; deleting a definition on a text search alone is insufficient.

## Acceptance record

Inventory baseline source: `4fab8eb20055da498363eb130f9a9e13abadf52f`. This baseline records observed files and declared interfaces, not a completed audit. Subsequent source changes are accepted at their exact commit with test evidence. Production hosting, account login, secret custody installation and data migration remain outside source-only acceptance.
