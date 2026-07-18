# Publishing and release guide

This guide is for the repository maintainer. It separates making the source
public from creating a software release.

## Current release model

- `main` and tag `v1.1.0` identify the current stable release.
- Tags `v1.0.0` and `v1.1.0` are immutable. Do not move or recreate them.
- Branch `1.1.0` is retained as the release branch for version 1.1.0.
- Create or announce a new development branch before beginning the next release
  line.
- Keep completed development changes under `CHANGELOG.md` → `Unreleased`.

The tagged 1.0.0 source declared Rust 1.85, but its locked dependency graph
requires Rust 1.86. This cannot be corrected inside an immutable tag. The
1.1.0 release documents and enforces Rust 1.86; build 1.0.0 with
Rust 1.86 or newer.

## Before changing repository visibility

From the release branch, verify the publication infrastructure:

```bash
git status --short --branch
make verify
make security
cargo package --locked --allow-dirty
gitleaks detect --source . --log-opts="--all" --redact
```

Confirm that:

- the Git history contains no credentials, private targets, customer data, job
  output, or generated evidence;
- `LICENSE` and `THIRD_PARTY_NOTICES.md` are included in the Cargo package;
- the upstream MIT attribution remains intact;
- the repository description, topics, and URL refer to `dariohy/mcp-kali`;
- GitHub account MFA and recovery methods are configured; and
- the default branch remains `main`.

Changing visibility is a separate, deliberate GitHub action. Review GitHub's
visibility warning before confirming because forks, Actions behavior, security
features, and exposed history can change.

## Immediately after making the repository public

1. Enable branch protection or a repository ruleset for `main`:
   - require pull requests;
   - require the `Rust verification` CI check;
   - require branches to be current before merging;
   - block force pushes and branch deletion; and
   - include administrators when practical.
2. Enable Dependabot alerts and security updates.
3. Enable secret scanning and push protection if GitHub offers them for the
   repository.
4. Enable private vulnerability reporting under **Security → Advisories**.
5. Verify that the CI workflow has completed successfully on `main` and the
   active release branch.
6. Check the public README, license detection, issue forms, and security-policy
   links while signed out or in a private browser window.

## Publishing the existing 1.0.0 release

Create a GitHub Release from the existing `v1.0.0` tag. Do not generate a new
tag. Use the 1.0.0 section of `CHANGELOG.md` as the release notes and clearly
repeat the unauthenticated-server warning.

Build distributable binaries from the tag, not from the `1.1.0` branch:

```bash
git switch --detach v1.0.0
make verify
make checksum
```

Return to development afterward:

```bash
git switch 1.1.0
```

Attach platform-specific release binaries only when they were built and tested
on that platform. Attach `SHA256SUMS` and an SBOM when available. Never attach
job state, logs, `.env` files, or locally generated evidence.

## Preparing a future release

1. Ensure all intended changes are committed on the release branch and CI is
   green.
2. Change `Cargo.toml` to the intended semantic version and update `Cargo.lock`.
3. Rename the `Unreleased` changelog section to the new version and date, then
   add a fresh empty `Unreleased` section.
4. Verify both binaries report the intended version with `--version`.
5. Run all checks, package verification, completions, checksums, and the SBOM.
6. Merge the release commit to `main` without rewriting published history.
7. Create an annotated version tag on the merged release commit and push it.
8. Create a GitHub Release from that tag and attach verified artifacts.

Never use the branch name as proof of the binary version. `Cargo.toml` and the
annotated release tag are the release sources of truth.
