# MCP Kali architecture and Rust migration notes

This document records why the Rust design differs from the earlier Python MCP
Kali Server and describes the implementation boundaries that maintainers must
preserve. User-facing instructions are in [docs/USER_MANUAL.md](docs/USER_MANUAL.md).

## Version 2.0 Plugin runtime

Version 2.0 moves utility-specific knowledge out of Rust and into declarative
YAML Plugins. At startup the server registers the built-in `mcp-kali.core` and
`mcp-kali.jobs` Plugins, loads packaged definitions, overlays administrator
definitions, validates JSON Schemas and safe argv templates, then resolves the
separate Capability Catalog. The MCP bridge retrieves `/api/tools`; it no
longer contains a scanner catalogue or scanner-specific routing.

The scheduler remains the execution boundary. A declarative invocation renders
one literal program plus one argument per template value and submits the result
through the same persistence, concurrency, process-group, timeout, output, and
webhook machinery. Shell interpreters, partial interpolation, and expression
evaluation are rejected from declarative definitions.

Plugin load failures are isolated and public through diagnostics. A valid
administrator overlay replaces matching packaged Plugin/tool identities.
Discovery is startup-only in 2.0; hot reload and executable extension ABIs are
out of scope.

## Motivation

The earlier synchronous design tied each HTTP/MCP request to a child-process
wait, buffered output in memory, encouraged agents to poll continuously, and
executed generic strings through a shell. Long scans therefore occupied the
request until completion and had no durable identity or lifecycle controls.

Version 1.0.0 separates submission, execution, monitoring, and MCP transport:

```text
MCP host -> mcp-kali-bridge -> HTTP(S) -> mcp-kali
                                             |
                       +---------------------+---------------------+
                       |                     |                     |
                  durable queue        dashboard/API         HTTPS webhook
                       |
                 bounded workers
                       |
               private per-job files
```

## Crate and binary structure

```text
src/bin/client.rs  CLI/config bootstrap for the stdio MCP client
src/bin/server.rs  CLI/config bootstrap for the scheduler/API server
src/config.rs      shared configuration-file selection and defaults
src/mcp.rs         MCP JSON-RPC transport and dynamic API forwarding
src/api.rs         Axum routes, validation adapters, dashboard response headers
src/jobs.rs        durable scheduler, process groups, output files, webhooks
src/plugins.rs     Plugin registry, catalogs, schemas, templates, core operations
src/models.rs      stable serialized job, output, and health models
src/dashboard.html embedded dashboard HTML/CSS/JavaScript
share/mcp-kali/    packaged Plugin manifests and base Capability Catalog
```

`Cargo.toml` is the version source of truth. Both Clap binaries, MCP
`serverInfo.version`, and `/health.version` use the package version.

## Submission and execution boundary

`SubmitJob.argv` carries the executable and each argument separately. The
scheduler calls `tokio::process::Command::new(argv[0]).args(argv[1..])`. It does
not invoke a shell.

Through version 1.1, `/api/command` and the old MCP `execute_command` shape performed
shell-style lexical splitting only. Pipes, redirection, substitution, and
separators remain literal arguments. New integrations should use argv directly.

Metasploit is a special case because `msfconsole -x` is itself an interpreter.
Module names, option keys, and option values are constrained before a script is
constructed. Those validations are security-sensitive and must not be relaxed
without a threat-model review.

Job submission resource limits are enforced before persistence:

- 1024 arguments;
- 64 KiB per argument;
- 256 KiB combined argument data;
- 128-byte tool labels without control characters;
- 1–604800 second timeouts; and
- 512 KiB HTTP bodies.

The stdio client independently limits one MCP request line to 1 MiB and drains
oversized lines without allocating their full contents.

## Scheduler invariants

- Jobs are persisted before entering the in-memory queue.
- A semaphore bounds simultaneous worker tasks.
- The oldest queued job is reserved as running while the jobs lock is held,
  preventing duplicate dispatch.
- Each child starts a new Unix process group.
- Cancel/timeout sends `SIGTERM`, waits up to five seconds, then sends `SIGKILL`.
- Force-kill targets the complete process group.
- Pause/resume use `SIGSTOP`/`SIGCONT` and do not suspend wall-clock timeout.
- Child stdin is null; stdout and stderr are private files.

Queued jobs with intact private argv resume after restart. Recorded running jobs
become `interrupted`; adopting unknown orphan processes would require a separate
supervisor protocol and is intentionally out of scope.

## Persistence model

```text
STATE_DIR/<uuid>/job.json
STATE_DIR/<uuid>/command.json
STATE_DIR/<uuid>/stdout.log
STATE_DIR/<uuid>/stderr.log
```

`job.json` is the public record. `command.json` stores private argv plus the
webhook destination, both skipped during API/webhook serialization. Public
records expose only `webhook_configured`. On Unix, the state root and job
directories are mode `700`; files are created as mode `600` from their first
open. Metadata updates write a private temporary file then rename it.

The public command field is derived from argv. Known secret-bearing flags are
redacted unless the server starts with `--reveal-sensitive-data`. On every
startup, the public display is recomputed for the current reveal setting.

## Output model

Output remains on disk rather than in memory. Consumers may:

- read a byte page with offset/next-offset/truncation metadata;
- request up to 500 recent lines from the most recent 1 MiB;
- stream the entire current stdout/stderr file; or
- view the latest 50 lines in the dashboard.

JSON views use lossy UTF-8 conversion. Full-log downloads stream the underlying
file. Log retention and maximum total job size remain operational policy.

## MCP transport and trust boundary

The hand-written stdio transport implements the lifecycle required by this
project: `initialize`, `ping`, `tools/list`, and `tools/call`. Protocol objects
are newline-delimited JSON-RPC. Stdout is protocol-only; tracing goes to stderr.

Client input validation includes:

- a 1 MiB request-line limit;
- UUID parsing for job routes;
- `stdout`/`stderr` stream enumeration;
- output limit clamping;
- typed HTTP(S) server URLs; and
- rejection of URL credentials, queries, fragments, and API paths.

Every tool success/error is wrapped as:

```json
{
  "security_classification": "untrusted_job_execution_data",
  "handling": "security boundary text",
  "data": {}
}
```

Initialization instructions and tool descriptions repeat that process output is
data, never instructions. This is defense in depth; the MCP host remains
responsible for enforcing its governing prompt and approval policy.

## Dashboard security and refresh design

The dashboard escapes every job-controlled value before using HTML templates,
including command display, tool/error metadata, stdout, and stderr. A CSP plus
anti-framing, no-sniff, no-referrer, and no-store headers harden the page.

Auto-refresh is stopped by default and runs every five seconds when enabled. A
structural signature avoids replacing unchanged job lists; elapsed values and
open details are refreshed in place. Expanded job IDs are maintained in client
state. The compact left-edge control toggles the detail panel.

## Network boundary

The server has no built-in authentication or TLS. Safe defaults and explicit
acknowledgements reduce accidental exposure:

- default bind is `127.0.0.1:5000`;
- non-loopback bind requires `--allow-remote-bind`;
- the client accepts loopback HTTP;
- remote HTTP requires `--allow-insecure-http`; and
- webhook HTTP is allowed only for localhost.

The recommended two-host topology is loopback plus an SSH tunnel. Other
deployments require external transport and network access controls compatible
with their environment.

## Configuration model

Both binaries load `~/.mcp-kali/etc/mcp-kali.conf` before Clap parsing.
`MCP_KALI_HOME` relocates the complete per-user tree; an explicit
`--config-file` or `MCP_KALI_CONFIG_FILE` selects another file. The `KEY=VALUE`
configuration file is non-secret: credentials, passwords, and tokens do not
belong there. Dotenv values do not override the pre-existing shell environment;
CLI flags override configuration values.

## API summary

```text
GET  / and /monitor
GET  /health
GET  /api/jobs
GET  /api/jobs/{id}
GET  /api/jobs/{id}/output
GET  /api/jobs/{id}/tail
GET  /api/jobs/{id}/logs/{stdout|stderr}
POST /api/jobs/{id}/cancel
POST /api/jobs/{id}/pause
POST /api/jobs/{id}/resume
POST /api/jobs/{id}/kill
GET  /api/plugins
GET  /api/plugins/{plugin_id}
GET  /api/plugins/diagnostics
GET  /api/capabilities
GET  /api/capabilities/{capability_id}/tools
GET  /api/tools
POST /api/tools/{tool_name}/invoke
```

Scheduled invocation returns `202 Accepted`; synchronous runtime operations
return `200`. State conflicts return `409`. Public job records never serialize
private argv.

## Release engineering

Version 1.0.0 adds:

- Rust 1.85 minimum-version metadata (the locked dependency graph requires
  Rust 1.86; the declared minimum is corrected in version 1.1.0);
- size-focused release profile;
- hidden Bash/Zsh/Fish/PowerShell/Elvish completions;
- `make verify`, `security`, `checksum`, `sbom`, and `completions`;
- `deny.toml` dependency policy;
- ignored generated security/completion artifacts; and
- a non-secret configuration-file example plus complete operations manual.

## Known limitations and future work

- No built-in authentication, authorization, tenancy, or TLS termination.
- Completion webhooks are unsigned, best-effort, and have no retries.
- No automatic job retention or disk quota.
- No API pagination/filtering for large finished-job histories.
- No supervisor protocol for adopting processes after restart.
- The MCP transport intentionally implements a focused protocol subset rather
  than depending on a pinned official Rust MCP SDK.
- Windows cannot provide Unix process-group lifecycle semantics.

These are explicit 1.0.0 constraints, not undocumented guarantees. Addressing
authentication, retention, or supervisor adoption requires a design/versioning
review because each changes stable operational boundaries.
