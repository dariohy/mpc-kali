# Contributing

MCP Kali accepts scoped changes that preserve its asynchronous job model,
machine-readable MCP stdout, and security boundaries.

By participating, you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Branch and version policy

- `main` represents the latest stable release.
- `v1.0.0` and `v1.1.0` are immutable release history and must not be moved or
  rewritten.
- Open pull requests against `main` unless a maintainer announces a dedicated
  development or maintenance branch.
- Change `Cargo.toml` only while preparing a release. Do not use a future stable
  version merely to identify a development branch.

## Development setup

Requirements: Rust 1.86+, Cargo, and Make.

```bash
cargo build
make verify
```

## Change guidelines

- Keep `Cargo.toml` as the only version source of truth.
- Keep MCP JSON-RPC on stdout and all diagnostics on stderr.
- Preserve structured process execution; do not add shell invocation.
- Keep utility-specific command syntax in declarative Plugins, not Rust. Preserve
  whole-value templates, reserved built-in identities, and startup diagnostics.
- Keep the server registry as the MCP tool source of truth; the stdio bridge
  must remain tool-agnostic.
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

See [docs/PUBLISHING.md](docs/PUBLISHING.md) for the maintainer release process.
