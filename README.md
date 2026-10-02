# zixcel-imap

Check a IMAP mail integration configuration and produce a plan before implementing or enabling the connection.

## What you can do

- Validate the declared integration settings.
- Inspect a bounded plan and missing prerequisites.

## Current scope

The current package validates configuration and plans operations. It does not provide a complete live service client.

Package distribution is not activated by this documentation. Use the checked-in source and the declared dependency versions; published availability must be verified separately.

## Getting started

Install Rust 1.97 or newer and make the declared dependencies available. Use the configured private registry when a dependency is not distributed publicly. Run from this repository:

```sh
cargo test --locked
```

## Documentation and source

[Usage guide](docs/getting-started.md)

[Examples](examples) · [Implementation and public interfaces](src) · [Verification cases](tests) · [Contributing](CONTRIBUTING.md) · [Security reporting](SECURITY.md) · [License](LICENSE) · [Attribution notices](NOTICE)
