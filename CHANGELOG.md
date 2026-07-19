# Changelog

All notable changes to MCP Kali are documented here. The project follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [2.0.0] - 2026-07-18

### Added

- Declarative YAML Plugin discovery with layered packaged and administrator
  data, JSON Schema validation, safe argv templates, diagnostics, and dynamic
  MCP tool projection.
- Built-in Core Plugin for privileged argv execution and bounded local command
  exploration, plus a built-in job-management Plugin.
- Separate Capability Catalog endpoints with provider availability resolution.
- Packaged declarative definitions for Nmap, Gobuster, Dirb, Nikto, SQLmap,
  Hydra, John the Ripper, WPScan, and enum4linux.
- Monitor Tools view for registered Plugins/tools, declared command
  requirements, and isolated unavailable-Plugin diagnostics.
- MCP tool-list change notifications, so capable hosts can refresh their tool
  index after the server's Plugin projection changes.

### Changed

- Restore the public binary names to `mcp-kali` and `mcp-kali-bridge`; the
  brief `mpc-*` spelling was erroneous.
- `make install-local` now creates a self-contained non-root user installation
  under `~/.mcp-kali` with `bin`, `etc`, `share/plugins`, and `var/jobs`.
- User installation creates or updates safe `~/.local/bin` symlinks for both
  MCP Kali binaries.
- Replaced the per-user `.env` contract with a non-secret `mcp-kali.conf`
  configuration file and the canonical `--config-file` / `MCP_KALI_CONFIG_FILE`
  selectors.
- Moved the shipped Capability Catalog into the Plugin data directory and made
  per-user paths the runtime defaults.
- Set the release version to 2.0.0.
- Replaced hard-coded scanner and command submission routes with generic Plugin
  discovery and `POST /api/tools/{tool_name}/invoke`.
- Made the MCP bridge retrieve tool definitions from the server at `tools/list`.
- Extended local installation to place packaged runtime data in the
  self-contained `~/.mcp-kali/share/plugins` tree.
- Renamed the public repository from `dariohy/mpc-kali` to
  `dariohy/mcp-kali` and updated canonical project links.

### Removed

- Legacy `--env-file`, `MCP_KALI_ENV_FILE`, `mcp-kali.env`, and
  `~/.envs/.env_mcp-kali` configuration support.
- Erroneous `mpc-kali` and `mpc-kali-bridge` binary names; use `mcp-kali` and
  `mcp-kali-bridge` instead.

## [1.1.0] - 2026-07-18

### Added

- GitHub Actions CI for formatting, compile checks, strict Clippy, tests, and
  release builds on Linux.
- Dependabot configuration for Cargo and GitHub Actions dependencies.
- Public issue forms, pull-request guidance, support instructions, a code of
  conduct, and a first-time publishing checklist.

### Changed

- Updated Cargo package metadata and public documentation to use the canonical
  `dariohy/mpc-kali` repository URL.
- Corrected the minimum supported Rust version to 1.86, matching the locked
  dependency graph used by CI and release builds.

## [1.0.0] - 2026-07-18

### Added

- Separate `mcp-kali-server` and `mcp-kali-client` release binaries sharing one
  Rust library.
- Durable asynchronous jobs with bounded concurrency, wall-clock timeouts,
  cancellation, restart recovery, paged output, HTTPS completion webhooks, and
  a browser job monitor.
- MCP tools for supported scanners, generic argument-vector submission, job
  listing/status/output, cancellation, pause, resume, force-kill, and health.
- Active/queue and finished-history dashboard tabs with compact rows,
  expandable metadata, last-50-line stdout/stderr tails, and full-log downloads.
- Pause, resume, and force-kill APIs backed by Unix process groups.
- Five-second opt-in dashboard auto-refresh that preserves expanded jobs and
  avoids rebuilding unchanged job lists.
- Shared env-file support at `~/.envs/.env_mcp-kali`, explicit `--env-file`
  selection, permission warnings, and documented configuration precedence.
- Hidden `completions` commands supporting Bash, Zsh, Fish, PowerShell, and
  Elvish for both binaries.
- Make targets for verification, completions, local installation, checksums,
  dependency/security checks, and CycloneDX SBOM generation.
- Dependency policy in `deny.toml` and a commented env-file example.
- Canonical GPL-3.0 license text matching package metadata.
- Comprehensive user manual under `docs/USER_MANUAL.md`.

### Security

- Scanner and generic command execution use an executable plus structured
  arguments without invoking a shell.
- Known password and request-data arguments are redacted from public job
  records by default, with an explicit lab-only reveal override.
- Job submissions enforce limits on argument count, per-argument size, total
  command size, tool labels, timeouts, output pages, and request bodies.
- Non-loopback server binds require `--allow-remote-bind`; cleartext client HTTP
  to non-loopback hosts requires `--allow-insecure-http`.
- Dashboard output is HTML-escaped and protected with CSP, anti-framing,
  no-sniff, no-referrer, and no-store response headers.
- MCP results are wrapped as `untrusted_job_execution_data`, and initialization
  plus tool descriptions tell agents never to treat job output as instructions.
- MCP client job IDs and output streams are validated before URL construction;
  API errors are bounded and control characters are neutralized.
- Private job directories use mode `700`; job metadata, command specifications,
  and logs use mode `600` on Unix.
- Webhook destinations are kept in the private execution specification and
  omitted from public records; API/webhook payloads expose only whether one is
  configured.

### Changed

- Tool calls return HTTP `202 Accepted` and a job record instead of blocking
  until a scanner exits.
- Generic command-string compatibility input is tokenized but shell operators
  are treated as literal arguments.
- Release binaries use size optimization, full LTO, one codegen unit, stripped
  symbols, and abort-on-panic behavior.

### Known limitations

- The 1.0.0 package metadata states Rust 1.85, but its locked transitive
  dependencies require Rust 1.86. Use Rust 1.86 or newer to build 1.0.0. The
  declared minimum is corrected in version 1.1.0.
- The HTTP server has no built-in authentication. Version 1.0.0 defaults to
  loopback and requires explicit acknowledgement for remote binding; use an SSH
  tunnel or an authenticated TLS reverse proxy.
- Completion webhooks are best-effort and are not signed or retried.
- There is no automatic job-retention policy; operators must manage the private
  state directory according to their evidence-retention requirements.

[Unreleased]: https://github.com/dariohy/mcp-kali/compare/v2.0.0...HEAD
[2.0.0]: https://github.com/dariohy/mcp-kali/releases/tag/v2.0.0
[1.1.0]: https://github.com/dariohy/mcp-kali/releases/tag/v1.1.0
[1.0.0]: https://github.com/dariohy/mcp-kali/releases/tag/v1.0.0
