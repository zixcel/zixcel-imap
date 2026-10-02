# Using zixcel-imap

Check a IMAP mail integration configuration and produce a plan before implementing or enabling the connection.

## Before you start

The current package validates configuration and plans operations. It does not provide a complete live service client.

## First steps

Run from the repository root:

```sh
cargo test --locked
```

## How to assess the result

- Validate the declared integration settings.
- Inspect a bounded plan and missing prerequisites.

A passing source-level check establishes only what that check observes. Keep missing configuration, unavailable services and unverified deployment paths visible.

## Continue reading

[Repository overview](../README.md)
