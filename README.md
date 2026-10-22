# Zixcel Mail

Provider-neutral Rust mail clients, message handling and bounded execution contracts.
The canonical repository is https://github.com/zixcel/zixcel-mail.

| Package | Responsibility |
| --- | --- |
| `zixcel-mail` | Facade reexporting `core`, `imap`, `smtp`, `message`, `planning` |
| `zixcel-mail-core` | Explicit endpoints, credentials supplied by owner, TLS/CA verification and wire limits |
| `zixcel-imap-client` | Verified implicit TLS, password/XOAUTH2, read-only EXAMINE, UIDVALIDITY-bound header reads |
| `zixcel-smtp-client` | Verified implicit TLS or required STARTTLS, authentication/NOOP, explicit one-use SMTP submission |
| `zixcel-mail-message` | MIME text-message composition and bounded header decoding |
| `zixcel-mail-planning` | Former planning-only IMAP configuration/plan CLI; it does not connect |
| `runtime/` | Separately licensed optional collector/operator and web-session services; independent PostgreSQL workspace |

The client workspace requires no database, daemon, tenant-wide administration,
PowerShell, Microsoft SDK or central repository. Microsoft-specific authorization
and mailbox management stay in `zixcel-microsoft`; credential custody stays with
Crowsi/the calling host. No OAuth app or secret store is created here.

## Library usage

```rust,no_run
use zixcel_mail::{core::{Auth, Credentials, Endpoint, TlsMode}, imap::Client};
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let endpoint = Endpoint { host: "imap.example.org".into(), port: 993, tls: TlsMode::Implicit };
let credentials = Credentials::new("user@example.org".into(), "synthetic-token".into(), Auth::Xoauth2)?;
let mut client = Client::connect(&endpoint, &credentials).await?;
let mailbox = client.examine("INBOX").await?;
client.logout().await?;
# Ok(()) }
```

No example contains usable credentials. Hosts resolve actual credentials through
custody. Do not put credentials in arguments, configuration files or logs.

## Supported boundaries

- Read-only IMAP uses EXAMINE and BODY.PEEK, pins UIDVALIDITY and exact requested UIDs,
  and rejects missing, duplicate, truncated or oversized observations.
- SMTP auth occurs only after certificate-verified TLS. STARTTLS never falls back
  to plaintext. The server's pre-TLS capabilities are discarded.
- SMTP plans bind endpoint, account, recipients, prepared message bytes, request ID
  and expiry. The host owns human approval; a hash or custody unlock alone grants
  no authority. Durable attempt markers prevent replay after restart. No retries.
- SMTP acceptance is not proof of delivery. Ambiguous outcomes must be reconciled
  independently; do not remove attempt markers to resend.
- Every operation has a 30-second timeout, IMAP sessions a 120-second lifetime,
  and streams a 2 MiB wire ceiling. MIME input/encoded messages are bounded.
- Private enterprise CAs are an explicit caller trust decision; there is no
  disable-verification switch. Production trust enrollment is outside the library.

This first client API does not implement POP3, a mail server/MTA, DKIM key management,
attachment extraction, arbitrary IMAP commands, or a native zix mail extension.
Existing curator folder changes remain in the optional runtime and are not
implicitly executed by the client facade. MIME summaries are untrusted content,
not executable LLM instructions; no HTML rendering or automatic URL fetching occurs.

All crates currently use `publish=false`; source availability does not imply a
crates.io package or downloadable native CLI. No live account compatibility or
actual delivery is claimed by synthetic tests.

## Development

Heavy tests run in owner CI. Use a bounded disposable environment for local tests:

```sh
cargo test --locked --workspace --all-targets --all-features
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo fmt --all --check
```

The [optional runtime](runtime/README.md) has its own lockfile, MIT notice,
PostgreSQL fixtures and CI job. Those services are not started by client tests.
[Usage](docs/getting-started.md) · [License](LICENSE) · [Notices](NOTICE) · [Security](SECURITY.md)
