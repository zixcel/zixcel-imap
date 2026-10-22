# Zixcel Mail optional IMAP curator runtime

Selected IMAP-curator logic is maintained here: collector, domain discovery,
folder operator, account OAuth/session helper, auth session validation,
PostgreSQL schema and observability. The MIT LICENSE is retained. The unrelated
profile/inventory MCP gateway, generated docs, old frontend/deployment, local
credentials, Git copies and tool caches are not adopted.

This is a separate Rust workspace. The separate zixcel-mail-planning crate remains a
planning-only package. Every runtime package has `publish = false`.

## Configuration

Supply DATABASE_URL and account connection/limits explicitly. Account secrets
are runtime input or OpenBao references; never check them into this repository.
AUTH_SESSION_KEY is required; the auth server rejects keys shorter than 32 bytes.
OAuth/OIDC requires positive IMAP_OAUTH_HTTP_TIMEOUT_MS and
IMAP_OAUTH_HTTP_MAX_RESPONSE_BYTES settings. The shared transport uses HTTPS,
certificate verification, no redirects, bounded responses and errors without URL
or response contents. The auth service uses OAuth2 authorization-code/PKCE exchange and AWS-LC-backed
jsonwebtoken verification. Its supported OIDC profile requires RS256, HTTPS
metadata/JWKS endpoints, a matching issuer, and the `openid` scope. Providers that
require other signing algorithms are rejected. Token-controlled key URLs,
private keys and ambiguous keys are rejected. Identity tokens must bind issuer,
audience, authorized party, expiry, issue time and nonce; optional access-token
and code hashes are checked. This adapter does not implement every OIDC flow or
signing algorithm. No real provider/account validation is claimed.
Pending states are single-use and bounded by positive `OIDC_STATE_TTL_SECS`
(default 600) and `OIDC_MAX_PENDING_STATES` (default 4096).
Auth listeners default to loopback and can be configured using AUTH_WEB_ADDR or
IMAP_AUTH_HELPER_ADDR. Review all service CLI options before enabling external
listeners. OTEL_EXPORTER_OTLP_ENDPOINT is optional, never implicitly connected.

Only certificate-verified implicit IMAP TLS is supported. Insecure/unsupported
TLS configuration is rejected before network access. Raw mail headers are not
written to logs. Operator folder moves are real external effects: do not run
against live accounts for migration validation.

## Verification

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --workspace
```

The CI runtime job tests, lints and builds every runtime package, audits its lockfile,
and checks embedded migrations and tenant foreign keys against disposable PostgreSQL.
To repeat the database check locally, use a disposable loopback PostgreSQL instance,
set `IMAP_TEST_DATABASE_URL` to its `wonderland_test_imap` database, and run
`cargo run --locked -p db --example verify_postgres`. The example rejects remote
hosts and other database names. CI's password-free fixture is ephemeral and must
never be reused for deployed services. The fixture verifies SQL migration identity,
repeat application, cross-tenant foreign keys and cascading account deletion.
Live provider compatibility, application-level tenant authorization and end-to-end
mail collection/move acceptance remain separate deployment gates.
Earlier generated reports are not evidence for this modified source.

Database pool and retry settings are required deployment configuration:
`DATABASE_MAX_CONNECTIONS`, `DATABASE_RETRY_ATTEMPTS`, and
`DATABASE_RETRY_DELAY_SECS` must be positive integers. Missing or invalid settings
fail startup. Migration 1 retains its original bytes. Migration 2 fixes the legacy foreign-key
removal order for fresh database initialization. Its checksum intentionally changes
in this unpublished runtime adoption; an existing deployed database with recorded
old checksums requires a separate migration compatibility decision before deployment.
SQLx macro/database-driver generation is not needed.
Compiler, migration-unit and PostgreSQL acceptance tests remain required.

The runtime depends directly on maintained `sqlx-core` and `sqlx-postgres`, with
explicit PostgreSQL/JSON/chrono/UUID features. The umbrella package and its
unused MySQL/SQLite/macro graph are removed; query binding and tenant filters
are retained. No new database, migration SQL or runtime deployment is introduced.

The IMAP wrapper pins `imap` 3.0.0-alpha.15 to remove the old nom/lexical-core
parser's soundness advisory. This is an upstream prerelease; verified implicit
TLS is established explicitly before reading the greeting. Production server
compatibility remains a deployment gate. OpenAPI generation uses utoipa 5 and
Swagger UI 8 to remove the unmaintained proc-macro-error dependency while keeping
the runtime's current Axum 0.7 API.

The browser session flow requires the browser itself to call `/api/oidc/authorize`
with cookies enabled (`credentials: "include"` for cross-origin fetch within the
configured CORS allowlist). The response sets a single-use transaction cookie;
the callback rejects missing, duplicate or mismatched browser state before code
exchange. A successful callback clears the transaction cookie. Use
`--cookie-secure` for HTTPS deployment to enforce the `__Host-` cookie boundary.
Only one pending login per browser cookie is supported. Server-side or CLI calls
cannot transfer this browser session flow without its originating browser cookie.
Backslashes, control characters and authority-like return paths are rejected.

HTTPS session issuers use `__Host-auth_session`; both auth services share the
same cookie-name selection and reject duplicate session cookies. Production
public URLs require HTTPS and `--cookie-secure`. Plain HTTP is restricted to
loopback development and uses `auth_session`; HTTPS readers never fall back to
that unprefixed cookie. Application base URLs reject credentials, queries and
fragments. Set the helper's session issuer to the auth service's public base URL.

Unauthenticated authorization JSON bodies are bounded before parsing by positive
`OIDC_MAX_REQUEST_BYTES` (default 4096), preventing large return paths from
exhausting the pending-state store. URL, session and request/state-limit configuration is validated before provider
discovery.

HTTPS deployment requires a new login if a browser still holds the old unprefixed
`auth_session` cookie. The reader never accepts it as a fallback for an HTTPS issuer.
Cookie-authenticated helper requests require an explicit CORS allowlist; wildcard
origins do not support browser credentials.

The auth and account-helper endpoints must share the browser-facing hostname,
for example through reverse-proxy routes: host-prefixed cookies are not shared
across subdomains. Frontends using cookie-based fetch must also be same-site
and listed explicitly in CORS. Cross-site frontend federation is outside this
local browser-session profile.
