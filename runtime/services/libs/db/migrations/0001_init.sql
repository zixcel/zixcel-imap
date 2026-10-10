CREATE TABLE IF NOT EXISTS accounts (
    account_id TEXT PRIMARY KEY,
    label TEXT NOT NULL,
    provider TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS account_auth (
    account_id TEXT PRIMARY KEY REFERENCES accounts(account_id) ON DELETE CASCADE,
    auth_type TEXT NOT NULL CHECK (auth_type IN ('xoauth2', 'password')),
    username TEXT NOT NULL,
    tenant_id TEXT,
    client_id TEXT,
    client_secret TEXT,
    client_secret_ref TEXT,
    refresh_token TEXT,
    refresh_token_ref TEXT,
    scope TEXT,
    app_password TEXT,
    access_token TEXT,
    app_password_ref TEXT,
    access_token_ref TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE account_auth
    ADD COLUMN IF NOT EXISTS client_secret_ref TEXT;
ALTER TABLE account_auth
    ADD COLUMN IF NOT EXISTS refresh_token_ref TEXT;
ALTER TABLE account_auth
    ADD COLUMN IF NOT EXISTS app_password_ref TEXT;
ALTER TABLE account_auth
    ADD COLUMN IF NOT EXISTS access_token_ref TEXT;

CREATE TABLE IF NOT EXISTS account_imap (
    account_id TEXT PRIMARY KEY REFERENCES accounts(account_id) ON DELETE CASCADE,
    host TEXT NOT NULL,
    port INTEGER NOT NULL,
    tls_mode TEXT NOT NULL DEFAULT 'implicit',
    tls_verify BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE IF NOT EXISTS account_mailboxes (
    account_id TEXT PRIMARY KEY REFERENCES accounts(account_id) ON DELETE CASCADE,
    inbox TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS account_limits (
    account_id TEXT PRIMARY KEY REFERENCES accounts(account_id) ON DELETE CASCADE,
    search_page_size INTEGER NOT NULL DEFAULT 5000,
    move_batch_size INTEGER NOT NULL DEFAULT 200
);

CREATE TABLE IF NOT EXISTS domain_defaults (
    id SMALLINT PRIMARY KEY CHECK (id = 1),
    auto_subscribe BOOLEAN NOT NULL DEFAULT TRUE,
    move_limit_per_run INTEGER NOT NULL DEFAULT 2000,
    collect_batch_size INTEGER NOT NULL DEFAULT 5000,
    exclude_domains TEXT[] NOT NULL DEFAULT '{}'
);

INSERT INTO domain_defaults (id)
VALUES (1)
ON CONFLICT (id) DO NOTHING;

CREATE TABLE IF NOT EXISTS account_domains (
    account_id TEXT NOT NULL REFERENCES accounts(account_id) ON DELETE CASCADE,
    domain TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    folder TEXT NOT NULL,
    first_seen TIMESTAMPTZ NOT NULL,
    last_seen TIMESTAMPTZ NOT NULL,
    count_estimate BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (account_id, domain)
);

CREATE TABLE IF NOT EXISTS collector_state (
    account_id TEXT PRIMARY KEY REFERENCES accounts(account_id) ON DELETE CASCADE,
    mailbox TEXT NOT NULL,
    last_uid_processed INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS service_status (
    service TEXT PRIMARY KEY,
    last_seen TIMESTAMPTZ NOT NULL,
    interval_secs INTEGER NOT NULL,
    detail TEXT
);

CREATE TABLE IF NOT EXISTS mail_messages (
    account_id TEXT NOT NULL REFERENCES accounts(account_id) ON DELETE CASCADE,
    uid BIGINT NOT NULL,
    mailbox TEXT NOT NULL,
    from_address TEXT,
    subject TEXT,
    domain TEXT,
    collected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (account_id, uid)
);

DO $$
BEGIN
    IF to_regclass('public.mail_samples') IS NOT NULL THEN
        INSERT INTO mail_messages (account_id, uid, mailbox, from_address, subject, domain, collected_at)
        SELECT account_id, uid, mailbox, from_address, subject, domain, collected_at
        FROM mail_samples
        ON CONFLICT (account_id, uid) DO NOTHING;
    END IF;
END
$$;

CREATE TABLE IF NOT EXISTS account_sync_status (
    account_id TEXT NOT NULL REFERENCES accounts(account_id) ON DELETE CASCADE,
    service TEXT NOT NULL,
    last_run_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_success_at TIMESTAMPTZ,
    last_error_at TIMESTAMPTZ,
    last_error TEXT,
    PRIMARY KEY (account_id, service)
);

CREATE INDEX IF NOT EXISTS mail_messages_collected_at_idx
ON mail_messages (collected_at DESC);
