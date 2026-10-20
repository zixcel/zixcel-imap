# OpenAPI build dependency boundary

The Swagger UI build extracts upstream assets, even with `vendored` enabled.
The released adapters reviewed during this repair still select affected ZIP
versions. Do not ignore the RustSec advisories or cast a major-only update as a
security fix.

Both Utoipa and its Swagger adapter use the same immutable upstream commit
`5aff30b375c0c72aeccfa8fa6100db8f2901b3ba` (upstream PR #1645), which requires ZIP
9.0. Pinning both avoids incompatible OpenAPI types across different Git/registry
sources. Axum is updated with the adapter; `/api/docs` and `/api/openapi.json`
remain available. Vendored assets prevent build-time UI downloads.

This is a temporary, source-pinned upstream repair, not a new published release.
When an official registry release contains this fix, replace both pins together,
regenerate Cargo.lock, and rerun strict Clippy, HTTP/OpenAPI tests, disposable
PostgreSQL integration and the complete dependency audit. Do not loosen CI.

Reference: https://github.com/juhaku/utoipa/commit/5aff30b375c0c72aeccfa8fa6100db8f2901b3ba
