-- Fresh development baseline. Existing databases must not be reused.
-- Secrets are stored in the secret provider; this database contains references only.
-- No implicit tenant, seeded production account, or compatibility migration.

CREATE TABLE tenants (
    tenant_id TEXT PRIMARY KEY CHECK (length(tenant_id) BETWEEN 1 AND 256 AND btrim(tenant_id) <> ''),
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE subjects (
    subject_id TEXT PRIMARY KEY,
    email TEXT,
    name TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE tenant_memberships (
    tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id) ON DELETE CASCADE,
    subject_id TEXT NOT NULL REFERENCES subjects(subject_id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, subject_id)
);

CREATE TABLE audit_log (
    id BIGSERIAL PRIMARY KEY,
    tenant_id TEXT,
    subject_id TEXT,
    subject_email TEXT,
    action TEXT NOT NULL,
    resource_type TEXT,
    resource_id TEXT,
    success BOOLEAN NOT NULL,
    error_code TEXT,
    detail JSONB,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE accounts (
    tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id) ON DELETE CASCADE,
    account_id TEXT,
    label TEXT NOT NULL,
    provider TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, account_id)
);

CREATE TABLE account_auth (
    tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id) ON DELETE CASCADE,
    account_id TEXT,
    auth_type TEXT NOT NULL CHECK (auth_type IN ('xoauth2', 'password')),
    username TEXT NOT NULL,
    oauth_tenant_id TEXT,
    client_id TEXT,
    client_secret_ref TEXT,
    refresh_token_ref TEXT,
    scope TEXT,
    app_password_ref TEXT,
    access_token_ref TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((auth_type = 'password' AND app_password_ref IS NOT NULL AND length(app_password_ref) > 0)
        OR (auth_type = 'xoauth2' AND access_token_ref IS NOT NULL AND length(access_token_ref) > 0)),
    PRIMARY KEY (tenant_id, account_id),
    FOREIGN KEY (tenant_id, account_id) REFERENCES accounts(tenant_id, account_id) ON DELETE CASCADE
);

CREATE TABLE account_imap (
    tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id) ON DELETE CASCADE,
    account_id TEXT,
    host TEXT NOT NULL,
    port INTEGER NOT NULL CHECK (port BETWEEN 1 AND 65535),
    tls_mode TEXT NOT NULL DEFAULT 'implicit' CHECK (tls_mode = 'implicit'),
    tls_verify BOOLEAN NOT NULL DEFAULT TRUE CHECK (tls_verify),
    PRIMARY KEY (tenant_id, account_id),
    FOREIGN KEY (tenant_id, account_id) REFERENCES accounts(tenant_id, account_id) ON DELETE CASCADE
);

CREATE TABLE account_mailboxes (
    tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id) ON DELETE CASCADE,
    account_id TEXT,
    inbox TEXT NOT NULL,
    PRIMARY KEY (tenant_id, account_id),
    FOREIGN KEY (tenant_id, account_id) REFERENCES accounts(tenant_id, account_id) ON DELETE CASCADE
);

CREATE TABLE account_limits (
    tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id) ON DELETE CASCADE,
    account_id TEXT,
    search_page_size INTEGER NOT NULL DEFAULT 5000 CHECK (search_page_size BETWEEN 1 AND 100000),
    move_batch_size INTEGER NOT NULL DEFAULT 200 CHECK (move_batch_size BETWEEN 1 AND 100000),
    PRIMARY KEY (tenant_id, account_id),
    FOREIGN KEY (tenant_id, account_id) REFERENCES accounts(tenant_id, account_id) ON DELETE CASCADE
);

CREATE TABLE domain_defaults (
    tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id) ON DELETE CASCADE,
    id SMALLINT CHECK (id = 1),
    auto_subscribe BOOLEAN NOT NULL DEFAULT FALSE,
    move_limit_per_run INTEGER NOT NULL DEFAULT 2000 CHECK (move_limit_per_run BETWEEN 1 AND 100000),
    collect_batch_size INTEGER NOT NULL DEFAULT 5000 CHECK (collect_batch_size BETWEEN 1 AND 100000),
    exclude_domains TEXT[] NOT NULL DEFAULT '{}',
    PRIMARY KEY (tenant_id, id)
);

CREATE TABLE account_domains (
    tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id) ON DELETE CASCADE,
    account_id TEXT NOT NULL,
    domain TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    folder TEXT NOT NULL,
    first_seen TIMESTAMPTZ NOT NULL,
    last_seen TIMESTAMPTZ NOT NULL,
    count_estimate BIGINT NOT NULL DEFAULT 0 CHECK (count_estimate >= 0),
    PRIMARY KEY (tenant_id, account_id, domain),
    FOREIGN KEY (tenant_id, account_id) REFERENCES accounts(tenant_id, account_id) ON DELETE CASCADE
);

CREATE TABLE collector_state (
    tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id) ON DELETE CASCADE,
    account_id TEXT,
    mailbox TEXT NOT NULL,
    last_uid_processed INTEGER NOT NULL DEFAULT 0 CHECK (last_uid_processed >= 0),
    PRIMARY KEY (tenant_id, account_id),
    FOREIGN KEY (tenant_id, account_id) REFERENCES accounts(tenant_id, account_id) ON DELETE CASCADE
);

CREATE TABLE service_status (
    service TEXT PRIMARY KEY,
    last_seen TIMESTAMPTZ NOT NULL,
    interval_secs INTEGER NOT NULL,
    detail TEXT
);

CREATE TABLE mail_messages (
    tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id) ON DELETE CASCADE,
    account_id TEXT NOT NULL,
    uid BIGINT NOT NULL,
    mailbox TEXT NOT NULL,
    from_address TEXT,
    subject TEXT,
    domain TEXT,
    collected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, account_id, uid),
    FOREIGN KEY (tenant_id, account_id) REFERENCES accounts(tenant_id, account_id) ON DELETE CASCADE
);

CREATE TABLE account_sync_status (
    tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id) ON DELETE CASCADE,
    account_id TEXT NOT NULL,
    service TEXT NOT NULL,
    last_run_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_success_at TIMESTAMPTZ,
    last_error_at TIMESTAMPTZ,
    last_error TEXT,
    PRIMARY KEY (tenant_id, account_id, service),
    FOREIGN KEY (tenant_id, account_id) REFERENCES accounts(tenant_id, account_id) ON DELETE CASCADE
);

CREATE INDEX audit_log_tenant_idx ON audit_log (tenant_id, occurred_at DESC);
CREATE INDEX mail_messages_collected_at_idx ON mail_messages (tenant_id, collected_at DESC);
