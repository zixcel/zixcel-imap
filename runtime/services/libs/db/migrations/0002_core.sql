CREATE TABLE IF NOT EXISTS tenants (
    tenant_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO tenants (tenant_id, name)
VALUES ('default', 'Default')
ON CONFLICT (tenant_id) DO NOTHING;

CREATE TABLE IF NOT EXISTS subjects (
    subject_id TEXT PRIMARY KEY,
    email TEXT,
    name TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS tenant_memberships (
    tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id) ON DELETE CASCADE,
    subject_id TEXT NOT NULL REFERENCES subjects(subject_id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, subject_id)
);

CREATE TABLE IF NOT EXISTS audit_log (
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

CREATE INDEX IF NOT EXISTS audit_log_tenant_idx
ON audit_log (tenant_id, occurred_at DESC);

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'account_auth' AND column_name = 'tenant_id'
    ) AND NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'account_auth' AND column_name = 'oauth_tenant_id'
    ) THEN
        EXECUTE 'ALTER TABLE account_auth RENAME COLUMN tenant_id TO oauth_tenant_id';
    END IF;
END
$$;

ALTER TABLE accounts
    ADD COLUMN IF NOT EXISTS tenant_id TEXT NOT NULL DEFAULT 'default';
-- Release legacy references before replacing the referenced account primary key.
-- Recreate each reference below using both tenant_id and account_id.
ALTER TABLE account_auth
    DROP CONSTRAINT IF EXISTS account_auth_account_id_fkey;
ALTER TABLE account_imap
    DROP CONSTRAINT IF EXISTS account_imap_account_id_fkey;
ALTER TABLE account_mailboxes
    DROP CONSTRAINT IF EXISTS account_mailboxes_account_id_fkey;
ALTER TABLE account_limits
    DROP CONSTRAINT IF EXISTS account_limits_account_id_fkey;
ALTER TABLE account_domains
    DROP CONSTRAINT IF EXISTS account_domains_account_id_fkey;
ALTER TABLE collector_state
    DROP CONSTRAINT IF EXISTS collector_state_account_id_fkey;
ALTER TABLE mail_messages
    DROP CONSTRAINT IF EXISTS mail_messages_account_id_fkey;
ALTER TABLE account_sync_status
    DROP CONSTRAINT IF EXISTS account_sync_status_account_id_fkey;

ALTER TABLE accounts
    DROP CONSTRAINT IF EXISTS accounts_pkey;
ALTER TABLE accounts
    ADD PRIMARY KEY (tenant_id, account_id);

ALTER TABLE account_auth
    ADD COLUMN IF NOT EXISTS tenant_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE account_auth
    DROP CONSTRAINT IF EXISTS account_auth_pkey;
ALTER TABLE account_auth
    DROP CONSTRAINT IF EXISTS account_auth_account_id_fkey;
ALTER TABLE account_auth
    ADD PRIMARY KEY (tenant_id, account_id);
ALTER TABLE account_auth
    ADD CONSTRAINT account_auth_account_id_fkey
    FOREIGN KEY (tenant_id, account_id)
    REFERENCES accounts(tenant_id, account_id)
    ON DELETE CASCADE;

ALTER TABLE account_imap
    ADD COLUMN IF NOT EXISTS tenant_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE account_imap
    DROP CONSTRAINT IF EXISTS account_imap_pkey;
ALTER TABLE account_imap
    DROP CONSTRAINT IF EXISTS account_imap_account_id_fkey;
ALTER TABLE account_imap
    ADD PRIMARY KEY (tenant_id, account_id);
ALTER TABLE account_imap
    ADD CONSTRAINT account_imap_account_id_fkey
    FOREIGN KEY (tenant_id, account_id)
    REFERENCES accounts(tenant_id, account_id)
    ON DELETE CASCADE;

ALTER TABLE account_mailboxes
    ADD COLUMN IF NOT EXISTS tenant_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE account_mailboxes
    DROP CONSTRAINT IF EXISTS account_mailboxes_pkey;
ALTER TABLE account_mailboxes
    DROP CONSTRAINT IF EXISTS account_mailboxes_account_id_fkey;
ALTER TABLE account_mailboxes
    ADD PRIMARY KEY (tenant_id, account_id);
ALTER TABLE account_mailboxes
    ADD CONSTRAINT account_mailboxes_account_id_fkey
    FOREIGN KEY (tenant_id, account_id)
    REFERENCES accounts(tenant_id, account_id)
    ON DELETE CASCADE;

ALTER TABLE account_limits
    ADD COLUMN IF NOT EXISTS tenant_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE account_limits
    DROP CONSTRAINT IF EXISTS account_limits_pkey;
ALTER TABLE account_limits
    DROP CONSTRAINT IF EXISTS account_limits_account_id_fkey;
ALTER TABLE account_limits
    ADD PRIMARY KEY (tenant_id, account_id);
ALTER TABLE account_limits
    ADD CONSTRAINT account_limits_account_id_fkey
    FOREIGN KEY (tenant_id, account_id)
    REFERENCES accounts(tenant_id, account_id)
    ON DELETE CASCADE;

ALTER TABLE domain_defaults
    ADD COLUMN IF NOT EXISTS tenant_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE domain_defaults
    DROP CONSTRAINT IF EXISTS domain_defaults_pkey;
ALTER TABLE domain_defaults
    ADD PRIMARY KEY (tenant_id, id);
ALTER TABLE domain_defaults
    ADD CONSTRAINT domain_defaults_tenant_id_fkey
    FOREIGN KEY (tenant_id)
    REFERENCES tenants(tenant_id)
    ON DELETE CASCADE;

ALTER TABLE account_domains
    ADD COLUMN IF NOT EXISTS tenant_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE account_domains
    DROP CONSTRAINT IF EXISTS account_domains_pkey;
ALTER TABLE account_domains
    DROP CONSTRAINT IF EXISTS account_domains_account_id_fkey;
ALTER TABLE account_domains
    ADD PRIMARY KEY (tenant_id, account_id, domain);
ALTER TABLE account_domains
    ADD CONSTRAINT account_domains_account_id_fkey
    FOREIGN KEY (tenant_id, account_id)
    REFERENCES accounts(tenant_id, account_id)
    ON DELETE CASCADE;

ALTER TABLE collector_state
    ADD COLUMN IF NOT EXISTS tenant_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE collector_state
    DROP CONSTRAINT IF EXISTS collector_state_pkey;
ALTER TABLE collector_state
    DROP CONSTRAINT IF EXISTS collector_state_account_id_fkey;
ALTER TABLE collector_state
    ADD PRIMARY KEY (tenant_id, account_id);
ALTER TABLE collector_state
    ADD CONSTRAINT collector_state_account_id_fkey
    FOREIGN KEY (tenant_id, account_id)
    REFERENCES accounts(tenant_id, account_id)
    ON DELETE CASCADE;

ALTER TABLE mail_messages
    ADD COLUMN IF NOT EXISTS tenant_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE mail_messages
    DROP CONSTRAINT IF EXISTS mail_messages_pkey;
ALTER TABLE mail_messages
    DROP CONSTRAINT IF EXISTS mail_messages_account_id_fkey;
ALTER TABLE mail_messages
    ADD PRIMARY KEY (tenant_id, account_id, uid);
ALTER TABLE mail_messages
    ADD CONSTRAINT mail_messages_account_id_fkey
    FOREIGN KEY (tenant_id, account_id)
    REFERENCES accounts(tenant_id, account_id)
    ON DELETE CASCADE;

DROP INDEX IF EXISTS mail_messages_collected_at_idx;
CREATE INDEX IF NOT EXISTS mail_messages_collected_at_idx
ON mail_messages (tenant_id, collected_at DESC);

ALTER TABLE account_sync_status
    ADD COLUMN IF NOT EXISTS tenant_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE account_sync_status
    DROP CONSTRAINT IF EXISTS account_sync_status_pkey;
ALTER TABLE account_sync_status
    DROP CONSTRAINT IF EXISTS account_sync_status_account_id_fkey;
ALTER TABLE account_sync_status
    ADD PRIMARY KEY (tenant_id, account_id, service);
ALTER TABLE account_sync_status
    ADD CONSTRAINT account_sync_status_account_id_fkey
    FOREIGN KEY (tenant_id, account_id)
    REFERENCES accounts(tenant_id, account_id)
    ON DELETE CASCADE;
