-- Synthetic fixture only. Never included in automatic schema migration.
-- The account endpoints are reserved .invalid names; every account is disabled.
DO $$ BEGIN
    IF current_database() <> 'wonderland_test_imap' THEN
        RAISE EXCEPTION 'development fixture requires wonderland_test_imap';
    END IF;
END $$;

INSERT INTO tenants (tenant_id, name)
VALUES ('fixture-a', 'Synthetic tenant A'), ('fixture-b', 'Synthetic tenant B')
ON CONFLICT DO NOTHING;

INSERT INTO subjects (subject_id, email, name)
VALUES ('fixture-subject', 'operator@fixture.invalid', 'Synthetic operator')
ON CONFLICT DO NOTHING;

INSERT INTO tenant_memberships (tenant_id, subject_id, role)
VALUES ('fixture-a', 'fixture-subject', 'admin') ON CONFLICT DO NOTHING;

INSERT INTO accounts (tenant_id, account_id, label, provider, enabled)
VALUES ('fixture-a', 'fixture-account', 'Synthetic password account', 'fixture', false),
       ('fixture-b', 'fixture-oauth', 'Synthetic OAuth account', 'fixture', false)
ON CONFLICT DO NOTHING;

INSERT INTO account_auth (tenant_id, account_id, auth_type, username, app_password_ref)
VALUES ('fixture-a', 'fixture-account', 'password', 'mail@fixture.invalid', 'secret/fixture/password')
ON CONFLICT DO NOTHING;

INSERT INTO account_auth (tenant_id, account_id, auth_type, username,
    oauth_tenant_id, client_id, client_secret_ref, refresh_token_ref, access_token_ref)
VALUES ('fixture-b', 'fixture-oauth', 'xoauth2', 'oauth@fixture.invalid',
    'synthetic-oauth-tenant', 'synthetic-client', 'secret/fixture/client',
    'secret/fixture/refresh', 'secret/fixture/access') ON CONFLICT DO NOTHING;

INSERT INTO account_imap (tenant_id, account_id, host, port)
VALUES ('fixture-a', 'fixture-account', 'imap.fixture.invalid', 993),
       ('fixture-b', 'fixture-oauth', 'imap.fixture.invalid', 993)
ON CONFLICT DO NOTHING;

INSERT INTO account_mailboxes (tenant_id, account_id, inbox)
VALUES ('fixture-a', 'fixture-account', 'INBOX'), ('fixture-b', 'fixture-oauth', 'INBOX')
ON CONFLICT DO NOTHING;

INSERT INTO account_limits (tenant_id, account_id)
VALUES ('fixture-a', 'fixture-account'), ('fixture-b', 'fixture-oauth')
ON CONFLICT DO NOTHING;

INSERT INTO domain_defaults (tenant_id, id)
VALUES ('fixture-a', 1), ('fixture-b', 1) ON CONFLICT DO NOTHING;

INSERT INTO mail_messages (tenant_id, account_id, uid, mailbox, from_address, subject, domain)
VALUES ('fixture-a', 'fixture-account', 1, 'INBOX', 'sender@fixture.invalid',
    'Synthetic fixture message', 'fixture.invalid') ON CONFLICT DO NOTHING;
