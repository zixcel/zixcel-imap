# Fresh development database

The current runtime uses one fresh baseline, `runtime/services/libs/db/migrations/0001_development_baseline.sql`. The former two-step compatibility migrations and their plaintext secret columns are removed. Existing databases are not an input to this development setup; the migrator must reject old checksum histories rather than silently overwrite them. No automatic database reset is provided.

The baseline has explicit tenant ownership and composite account foreign keys. Accounts and automatic subscriptions default to disabled. IMAP configuration requires verified implicit TLS and a valid port; processing limits must be positive and bounded. Authentication rows store secret-provider references, with no plaintext password/token columns. The runtime rejects inline authentication configuration and requires OpenBao for reading or storing account secrets. Disabled accounts are excluded from token refresh.

`runtime/services/libs/db/fixtures/development.sql` contains only synthetic identities, disabled accounts, reserved `.invalid` endpoints and inert secret references. It is not part of automatic migrations and refuses databases other than `wonderland_test_imap`. Apply the fixture within a transaction. A real execution environment and provider registration will be created separately.

CI provides a disposable PostgreSQL service. The same acceptance command can use a disposable local container:

```sh
docker run --rm -d --name imap-development-fixture \
  -p 127.0.0.1:55432:5432 \
  -e POSTGRES_DB=wonderland_test_imap \
  -e POSTGRES_HOST_AUTH_METHOD=trust postgres:16-alpine
cd runtime
IMAP_TEST_DATABASE_URL=postgresql://postgres@127.0.0.1:55432/wonderland_test_imap \
  cargo +1.97 run --locked -p db --example verify_postgres
docker stop imap-development-fixture
```

The verifier applies the baseline twice and seeds the fixture twice, checks idempotency and absence of plaintext credential columns, rejects cross-tenant references, missing tenants, unverified TLS, invalid ports/limits and missing secret references, then removes its synthetic records. It does not connect to a mail provider or read existing account data.
