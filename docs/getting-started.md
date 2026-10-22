# Using Zixcel Mail

Use the root facade or a specific crate from the same source workspace. The
client APIs accept explicit endpoints and owner-supplied credentials. No global
configuration, database, daemon, implicit login or secret resolution is required.

IMAP: `Client::connect`, `examine`, `headers`, `logout`. Header selection must bind
the observed UIDVALIDITY and exact UID list (1..20). Reads use EXAMINE/BODY.PEEK;
message reads never mark messages as seen. A failed or timed-out operation closes
the session to prevent reuse of a partially consumed protocol stream.

SMTP: `check`/`check_with_trust` authenticate and NOOP only. Prepare a MIME message
once, create `Plan::new`, review its digest/fields in the host approval UI, and pass
`Submission` to `send_once`. The journal must be an existing caller-owned private
0700 directory. An attempt marker is synced before any connection and survives
restarts. A failure after admission is conservatively unknown; automatic resends
are forbidden. Reconcile server/recipient observations before a new intentional
submission. Do not infer final delivery from SMTP acceptance.

The planning-only CLI is explicitly separate:

```sh
cargo run --locked -p zixcel-mail-planning -- doctor
cargo run --locked -p zixcel-mail-planning -- plan examples/config.toml
```

Use existing example filenames from `examples/`; the planning CLI has no mail
transport. Runtime collector/operator services retain their separate configuration
and license. This migration does not provision a database or migrate live mail.

Microsoft 365 IMAP/SMTP use Exchange-audience OAuth scopes. A Microsoft Graph
token or identity-only login is not interchangeable. New app/consent, private CA
trust enrollment and actual send/recipient changes remain explicit owner actions.
