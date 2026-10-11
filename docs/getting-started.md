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

Configuration input is limited to 1 MiB before decoding. The opened handle must be a regular file. On Unix, symbolic links and FIFOs are rejected with non-following, non-blocking open flags; the CLI does not wait for a FIFO writer. Invalid UTF-8 and oversized files are rejected.
