# zixcel-imap

Issuer-neutral configuration validation and planning for IMAP observation. The current CLI is planning-only and links no IMAP/TLS client, credential store or executor.

```bash
cargo run --offline -- doctor
cargo run --offline -- capabilities
cargo run --offline -- validate examples/config.toml
cargo run --offline -- plan examples/config.toml
```

Configuration stores only account and `secret://...` references, never usernames or passwords. Body retrieval is outside current policy; plans are limited to `metadata-only` or `headers-only`.

`parse_config` validates closed TOML and `build_plan` generates deterministic proposals. Input is limited to 1 MiB, nesting to 32 levels and mailboxes to 256. Body retrieval, credential resolution and communication remain outside the implementation. The crate uses `publish = false` during local validation.

## Quality gate

These checks run within this crate without connecting to IMAP servers.

```bash
# WONDERLAND_ROOT is the workspace checkout root.
"$WONDERLAND_ROOT/bin/verify-repositories" --rust --tier standard
```

## Package integration

The package is an independently consumable unit. Callers reference its documented
interface through a versioned dependency and own application-specific composition
and integration.

## Distribution license

Apache-2.0. Copyright 2026 HAT Inc. See [LICENSE](LICENSE) and [NOTICE](NOTICE). Earlier license files and third-party terms remain applicable to their respective portions.
