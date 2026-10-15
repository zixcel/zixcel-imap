# Atomic collector persistence

Each collected batch commits mail rows, domain observations and the collector cursor
in one PostgreSQL transaction. Working configuration is copied per batch and becomes
the current in-memory configuration only after successful commit. After any account
failure, durable state is reloaded before another account can write shared configuration.
This also handles a commit whose acknowledgement was lost without rewinding its cursor.

The collector resumes after the persisted scanned UID range, including missing UIDs
and excluded domain observations. Existing mail upsert identity remains unchanged.
This change does not claim concurrent collectors or mailbox UIDVALIDITY changes are
supported; those require separate contracts.

The owner CI runs the ignored `batch_persistence_contract` explicitly against its
isolated PostgreSQL service, with IMAP_TEST_DATABASE_URL pointing only at the
`wonderland_test_imap` database on literal loopback as postgres without a password.
The test creates and drops only its unique schema, using synthetic .invalid data.

Run from runtime: `cargo test --locked -p imap-curator-collector batch_persistence_contract -- --ignored`.
The test fails if the disposable database is absent. It checks mail-insert failure,
transaction termination before commit, restart after commit, idempotent replay,
empty ranges, domain counts, cursor position and tenant isolation.
