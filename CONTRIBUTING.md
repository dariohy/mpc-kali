# Contributing

MCP Kali accepts scoped changes that preserve its asynchronous job model,
machine-readable MCP stdout, and security boundaries.

## Development setup

Requirements: Rust 1.85+, Cargo, and Make.

```bash
cargo build
make verify
```

## Change guidelines

- Keep `Cargo.toml` as the only version source of truth.
- Keep MCP JSON-RPC on stdout and all diagnostics on stderr.
- Preserve structured process execution; do not add shell invocation.
- Treat command fields, output, API responses, and webhook fields as untrusted.
- Preserve default command redaction and explicit reveal behavior.
- Add bounded input/resource handling for new parsers and endpoints.
- Keep public JSON field names and job states stable unless a documented
  versioned migration is intentional.
- Add tests for validation, persistence, scheduler lifecycle, or trust-boundary
  changes.
- Update README, the user manual, architecture notes, and changelog when their
  contracts change.

## Sensitive data

Never commit:

- `.env` files;
- tokens, passwords, hashes, or private keys;
- real target/customer data;
- `var/` job state;
- logs, scanner output, reports, or evidence; or
- generated security artifacts containing environment-specific metadata.

Use synthetic targets and payloads in tests.

## Required checks

```bash
make fmt
make fmt-check
make check
make clippy
make test
make release
```

For release/security changes also run:

```bash
make security
make completions
make checksum
make sbom
```

If an optional tool is not installed, state that in the handoff. Do not weaken
`deny.toml` or suppress security findings without documenting the reason.

## Commit and release hygiene

- Review `git status` and the complete diff.
- Stage only intended source/documentation files.
- Confirm ignored env, job, log, and `target/` artifacts remain unstaged.
- Use a clear commit message.
- Push, tag, sign, or publish only when explicitly authorized.
