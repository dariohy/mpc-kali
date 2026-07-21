# Changelog

All notable changes to MCP Kali are documented here. The project follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Private structured server logging with exclusive normal/error JSONL streams,
  stdout fallback, SIGHUP reopen support, and installed user/system log paths.
- A rendered system logrotate policy with daily rotation, compression, 30-file
  retention, selected-account ownership, and coordinated service signaling.

### Changed

- Namespace bridge-only configuration as `MCP_KALI_BRIDGE_SERVER` and
  `MCP_KALI_BRIDGE_ALLOW_INSECURE_HTTP`; the previous names remain migration
  aliases when the new names are unset.
- Restrict HTTP tracing to method, path, status, and latency metadata while
  keeping request bodies, query strings, command data, and job output out of
  service logs.
- Raise default scheduler concurrency to 4 and default job timeout to five days
  while retaining the seven-day maximum.
- Consolidate configuration into a ready-to-run `mcp-kali.config` and a sibling
  `mcp-kali.config.example` reference with `RUST_LOG` guidance and safe limits.

## [2.2.1] - 2026-07-21

### Added

- Recoverable terminal-job archiving through a previewed dashboard action,
  REST endpoints, `SIGUSR1`, and `make archive-jobs-system`, with configurable
  minute-based age and archive paths and no automatic archive deletion.
- SHA-256 integrity manifests for terminal job evidence and timestamp-windowed
  gzip archives named from the oldest job start and newest job finish times.
- Layered validated Plugin reference documents, REST discovery, a Monitor
  References tab, and MCP `resources/list` / `resources/read` projection with
  list-change notifications.
- `mcp-kali references import` for adding bounded, non-symlink Markdown guides
  to the administrator reference overlay without changing executable tools.
- Curated Nmap profiles for ARP discovery, privileged SYN and UDP service
  detection, OS fingerprinting, TLS configuration, SMB security posture, and
  web-service inventory, with packaged selection and interpretation guidance.
- Focused enum4linux profiles for anonymous SMB users, groups, shares, password
  policy, OS and NetBIOS identity, printers, and bounded RID discovery, with
  packaged workflow, boundary, and result-interpretation guidance.
- Non-destructive Nikto profiles for broad web-server assessment, focused
  configuration and software checks, HTTPS, virtual hosts, and fixed
  rate-limited production scanning, with packaged operator guidance.
- Low-risk SQLmap profiles for targeted GET/POST detection and bounded DBMS,
  database-name, and table-name inventory without extraction or host access.
- Mode-correct Gobuster profiles for directory, extension, DNS, virtual-host,
  URL-fuzz, and fixed rate-limited discovery.
- Token-free WPScan profiles for WordPress fingerprinting, component inventory,
  bounded user enumeration, exposed artifacts, and fixed rate limiting.
- A bundled DNSRecon Plugin with standard and SRV records, AXFR checks,
  certificate-transparency lookup, wildcard-filtered subdomain discovery,
  bounded IPv4 reverse lookup, and DNSSEC zone-walk profiles.

### Changed

- Align per-user durable state with the system layout: active jobs now default
  to `~/.mcp-kali/var/lib/jobs` and recoverable archives to
  `~/.mcp-kali/var/lib/archive/jobs`.
- Install immutable Plugin, capability, and reference data under
  `/usr/lib/mcp-kali`, keep administrator overlays under `/etc/mcp-kali`, and
  install the service unit under `/usr/lib/systemd/system`.
- Make normal Nmap host discovery and TCP service detection unprivileged,
  reserve root metadata for probe types that require raw-packet access, and
  constrain targets and port expressions in every Nmap profile.
- Constrain enum4linux profiles to one non-option host and keep credentials,
  unbounded enumeration, brute-force guessing, LDAP expansion, and write tests
  outside the declarative default surface.
- Require explicit HTTP(S) URLs for Nikto profiles, disable interactive update
  behavior, and exclude disruptive tuning categories from broad declarative
  scans while preserving advanced behavior through reviewed command execution.
- Constrain SQLmap to level 1, risk 1, and non-stacked techniques; require
  explicit credential-free URLs; and keep extraction, evasion, credentials,
  higher risk, filesystem, and OS access outside the declarative surface.
- Replace Gobuster's ambiguous cross-mode invocation with mode-specific schemas,
  absolute wordlist paths, fixed worker profiles, and stable captured output.
- Make WPScan enumeration explicit and repeatable without API tokens, database
  updates, credentials, aggressive detection, or password attacks.

### Fixed

- Restore Rust 1.86 compatibility for MCP reference-list change notifications.
- Force Nmap's explicit unprivileged mode for normal host discovery, TCP
  connect scans, and the packaged TLS, SMB, and web profiles so non-root
  service execution does not attempt raw sockets through privilege-assuming
  Nmap launchers.
- Preserve Nmap's `-p` port argument in public command displays while still
  redacting Hydra and Medusa password arguments.
- Include the HTTP status, response content type, and body length in MCP bridge
  invalid-JSON errors without exposing the response body.

## [2.1.1] - 2026-07-20

### Fixed

- Check non-interactive sudo authorization for each root-required declarative
  program without executing it, instead of using the overly strict `sudo -v`
  credential-validation probe.
- Render a concrete systemd service-user home directory, stop injecting the
  application configuration through `EnvironmentFile`, and retain access to
  the service user's writable working directory.
- Default system installs to the existing Kali `kali` account while preserving
  explicit service-user and group overrides.

### Changed

- Replaces the withdrawn 2.1.0 release, which contained systemd installation
  and sudo-readiness defects. Use 2.1.1 or later.

## 2.1.0 - 2026-07-19 (withdrawn)

### Added

- Declarative tool-level `requirements.privilege: root`, with default
  non-interactive `sudo -n` elevation controlled by
  `MCP_KALI_PRIVILEGE_ELEVATION=auto|none`.
- Root requirement metadata in MCP tool projections and the Monitor Tools tab.
- Startup non-interactive sudo readiness checks, with MCP and Monitor status for
  whether each root-required tool is enabled.
- Unix `SIGTERM`/`SIGINT` graceful shutdown and `SIGHUP` atomic Plugin/catalog
  reload, including configuration-file scheduler-concurrency reload.
- Base systemd unit template, system configuration example, and explicit
  root-only Makefile install/enable/status/log targets.
- Unified user and system Plugin/catalog declaration locations under their
  respective `etc/plugins` configuration trees.
- Moved the repository's packaged Plugin manifests and Capability Catalog to
  the top-level `plugins/` directory.
- Added focused `make client` and `make client-install` workflows for building
  and locally installing only `mcp-kali-bridge`.
- Unified `make uninstall` workflow for removing either a per-user installation
  or, when run as root, a systemd-backed system installation.

### Changed

- Marked Nmap host discovery as root-required so it uses privileged discovery
  probes in the default runtime mode.

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
- Replaced the per-user `.env` contract with a non-secret `mcp-kali.config`
  configuration file and the canonical `--config-file`
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

[Unreleased]: https://github.com/dariohy/mcp-kali/compare/v2.2.1...HEAD
[2.2.1]: https://github.com/dariohy/mcp-kali/compare/v2.1.1...v2.2.1
[2.1.1]: https://github.com/dariohy/mcp-kali/releases/tag/v2.1.1
[2.0.0]: https://github.com/dariohy/mcp-kali/releases/tag/v2.0.0
[1.1.0]: https://github.com/dariohy/mcp-kali/releases/tag/v1.1.0
[1.0.0]: https://github.com/dariohy/mcp-kali/releases/tag/v1.0.0
