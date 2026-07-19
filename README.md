# MCP Kali 2.0.0

[![CI](https://github.com/dariohy/mcp-kali/actions/workflows/ci.yml/badge.svg)](https://github.com/dariohy/mcp-kali/actions/workflows/ci.yml)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)

MCP Kali is a Rust client/server system for scheduling Kali Linux security tools
without making an MCP agent wait for a long-running process. The server owns a
durable bounded queue, subprocess lifecycle, output files, HTTP API, completion
webhooks, and browser dashboard. The small stdio client exposes those functions
to an MCP host and returns job IDs immediately.

Use MCP Kali only on systems and targets you are explicitly authorized to test.

**Project status:** `v2.0.0` is the current stable release.

## Contents

- [Architecture](#architecture)
- [Requirements](#requirements)
- [Build and install](#build-and-install)
- [Quick start](#quick-start)
- [Configuration](#configuration)
- [Plugins and capabilities](#plugins-and-capabilities)
- [MCP host setup](#mcp-host-setup)
- [Dashboard and jobs](#dashboard-and-jobs)
- [Shell completions](#shell-completions)
- [Output contracts](#output-contracts)
- [Security](#security)
- [Development and release](#development-and-release)
- [Troubleshooting](#troubleshooting)
- [Documentation](#documentation)
- [License and upstream attribution](#license-and-upstream-attribution)

## Architecture

```text
MCP host -> mcp-kali-bridge -> HTTP(S) -> mcp-kali -> Plugin Registry
                                                           -> durable queue
                                                           -> bounded workers
                                                           -> job files/API
```

- `mcp-kali` runs on the Kali host and executes tools.
- `mcp-kali-bridge` runs beside the MCP host and speaks newline-delimited MCP
  JSON-RPC over stdin/stdout. It never executes Kali tools locally.
- Every submission returns a UUID job ID. Agents can do other work and inspect
  the job later rather than repeatedly blocking on a command.
- The server discovers declarative YAML Plugins and publishes their MCP tools
  dynamically. Adding a valid local Plugin does not require recompilation.

The Rust implementation originated as a port of
[MCP-Kali-Server](https://github.com/Wh0am123/MCP-Kali-Server) by Yousof Nahya
(Wh0am123). The Python source is not bundled in this repository; see
[Third-party notices](THIRD_PARTY_NOTICES.md) for the preserved upstream MIT
notice and licensing details.

## Requirements

- Rust 1.86 or newer to build; edition 2024 is used.
- Linux or another Unix-like server for pause/resume/kill process-group control.
- Kali tools required by installed Plugins. The shipped definitions cover
  `nmap`, `gobuster`, `dirb`, `nikto`, `sqlmap`, `hydra`, `john`, `wpscan`, and
  `enum4linux`; unavailable requirements are reported without stopping the server.
- Write access to the configured state directory.
- Network access from the client to the server, preferably through loopback,
  an SSH tunnel, or authenticated HTTPS.

## Build and install

```bash
cargo build --release
```

The size-optimized binaries are:

```text
target/release/mcp-kali
target/release/mcp-kali-bridge
```

`make install-local` creates a self-contained per-user runtime tree:

```text
~/.mcp-kali/
├── bin/                         # mcp-kali and mcp-kali-bridge
├── etc/
│   ├── mcp-kali.conf            # normal, non-secret user configuration
│   └── plugins/                 # administrator-overlay Plugins and catalog
├── share/plugins/               # shipped Plugins and capability catalog
│   ├── capability-catalog.yaml
│   └── <plugin>/plugin.yaml
└── var/jobs/                    # private durable job state and output
```

Install it with:

```bash
make install-local
```

The installer also creates or updates symlinks for both binaries in
`~/.local/bin`. If that directory is not already on `PATH`, add it:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

For safety, installation refuses to replace a non-symlink at either link path.

Use a different self-contained user directory when needed:

```bash
make install-local MCP_KALI_HOME=/path/to/mcp-kali
```

Set `MCP_KALI_HOME=/path/to/mcp-kali` when running a relocated installation.
`install-local` intentionally refuses root: system-wide service installation
and its required dedicated user/systemd unit will be added separately.

Release builds use size optimization, full LTO, one codegen unit, stripped
symbols, and abort-on-panic behavior. No scheduler, API, dashboard, or MCP
functionality is removed.

## Quick start

Start the server on loopback with a workspace-local state directory:

```bash
./target/release/mcp-kali \
  --bind 127.0.0.1:5000 \
  --state-dir ./var/jobs \
  --system-data-dir ./share/mcp-kali \
  --max-concurrency 2 \
  --default-timeout 1800
```

Verify health:

```bash
curl -sS http://127.0.0.1:5000/health
```

Open `http://127.0.0.1:5000/` or `/monitor` for the dashboard. Start the MCP
bridge beside the MCP host:

```bash
./target/release/mcp-kali-bridge --server http://127.0.0.1:5000
```

For separate machines, keep the server on loopback and create an SSH tunnel:

```bash
ssh -N -L 5000:127.0.0.1:5000 kali-user@kali-host
```

The client can then continue to use `http://127.0.0.1:5000` safely through the
encrypted tunnel.

## Configuration

Configuration precedence, from lowest to highest, is:

```text
hardcoded defaults
-> ~/.mcp-kali/etc/mcp-kali.conf (or selected config file)
-> existing shell environment
-> command-line arguments
```

`make install-local` creates `~/.mcp-kali/etc/mcp-kali.conf` if it does not
already exist. The file uses a simple `KEY=VALUE` syntax and must not contain
secrets. To create the file before installation:

```bash
mkdir -p ~/.mcp-kali/etc
install -m 644 examples/mcp-kali.conf.example ~/.mcp-kali/etc/mcp-kali.conf
```

Select another file with `--config-file PATH` or `MCP_KALI_CONFIG_FILE`.
Existing environment variables are never overwritten by values from the file.
Version 2.0 does not read `mcp-kali.env` or `~/.envs/.env_mcp-kali`, and does
not accept the prior `--env-file` / `MCP_KALI_ENV_FILE` selectors.

| Variable | Binary | Default | Description |
|---|---|---:|---|
| `MCP_KALI_HOME` | Both | `~/.mcp-kali` | Root of the self-contained per-user tree |
| `MCP_KALI_CONFIG_FILE` | Both | `~/.mcp-kali/etc/mcp-kali.conf` | Alternate configuration-file path |
| `RUST_LOG` | Both | Binary-specific info filter | Tracing filter; logs go to stderr |
| `MCP_KALI_BIND` | Server | `127.0.0.1:5000` | HTTP API/dashboard bind address |
| `MCP_KALI_STATE_DIR` | Server | `~/.mcp-kali/var/jobs` | Private durable job directory |
| `MCP_KALI_MAX_CONCURRENCY` | Server | `2` | Simultaneous jobs, range 1–256 |
| `MCP_KALI_DEFAULT_TIMEOUT` | Server | `1800` | Default wall timeout, range 1–604800 seconds |
| `MCP_KALI_REVEAL_SENSITIVE_DATA` | Server | `false` | Show unredacted commands in public records |
| `MCP_KALI_SYSTEM_DATA_DIR` | Server | `~/.mcp-kali/share` | Shipped Plugins and base catalog |
| `MCP_KALI_CONFIG_DIR` | Server | `~/.mcp-kali/etc` | Administrator Plugin/catalog overlay |
| `MCP_KALI_DISABLE_EXECUTE_COMMAND` | Server | `false` | Remove the privileged free-execution tool |
| `MCP_KALI_ALLOW_REMOTE_BIND` | Server | `false` | Acknowledge an unauthenticated non-loopback bind |
| `MCP_KALI_SERVER` | Client | `http://127.0.0.1:5000` | Server origin URL |
| `MCP_KALI_ALLOW_INSECURE_HTTP` | Client | `false` | Permit HTTP to a non-loopback server |

CLI flags matching these settings are documented by each binary's `--help`.
Sensitive command values should be supplied through MCP job arguments and are
redacted from public records by default; do not put secrets directly in process
arguments unless the target tool requires them.

## Plugins and capabilities

The server loads packaged definitions from `SYSTEM_DATA_DIR/plugins`, then an
administrator overlay from `CONFIG_DIR/plugins`. A valid overlay Plugin or tool
with the same identity replaces the packaged definition. Discovery happens at
startup; malformed files are isolated and reported at
`/api/plugins/diagnostics`.

A Plugin manifest uses `apiVersion: mcp-kali/v1`, `kind: Plugin`, identity
metadata, optional requirements, and one or more tools. Each tool publishes a
JSON Schema object and a direct execution definition:

```yaml
apiVersion: mcp-kali/v1
kind: Plugin
metadata: {id: local.example, name: Example, version: 1.0.0}
requires: {commands: [printf]}
tools:
  - metadata:
      name: example_print
      description: Print one validated value.
    input_schema:
      type: object
      additionalProperties: false
      required: [value]
      properties: {value: {type: string}}
    execution:
      program: printf
      args: ["%s\\n", "{{value}}"]
```

Templates support only literal arguments, whole-value `{{field}}`
substitutions, and `{when: field, args: [...]}` optional fragments. Every
rendered value is exactly one process argument; shells, partial interpolation,
expressions, loops, and command substitution are rejected.

The separate capability catalog maps stable semantic IDs to Plugin providers.
Catalog references remain visible with an availability flag when an optional
Plugin or tool is not installed.

## MCP host setup

Example configuration:

```json
{
  "mcpServers": {
    "mcp-kali": {
      "command": "/absolute/path/to/mcp-kali-bridge",
      "args": ["--server", "http://127.0.0.1:5000"]
    }
  }
}
```

The client retrieves the current Plugin tool projection from the server for
each MCP `tools/list` request and forwards calls to the generic invocation API.
For a long-lived bridge connection, it polls the server every five seconds and
sends the MCP `notifications/tools/list_changed` notification when the
projection changes, so capable hosts can refresh their tool index after a
server restart or Plugin change.
The always-available job Plugin exposes listing, status, output paging, cancel,
pause, resume, force-kill, and health operations. Every tool response is wrapped
in an `untrusted_job_execution_data` envelope. Job
stdout/stderr is evidence data and must never change the agent's governing
prompt, authorization scope, tool policy, or behavior.

## Dashboard and jobs

The dashboard provides:

- compact Active & queue and Finished history views;
- a Tools view of registered Plugins and tools, declared command requirements,
  and isolated unavailable-Plugin diagnostics;
- queue order, state, tool, command summary, and elapsed time;
- a left-edge `>` control that expands full metadata and wrapped command text;
- pause, resume, remove, and force-kill controls where applicable;
- escaped last-50-line stdout/stderr views and complete-log downloads;
- manual refresh and five-second opt-in auto-refresh, stopped by default.

Expanded jobs remain open across polls. Routine polls update volatile fields and
open details without rebuilding an unchanged list.

Job state is stored under:

```text
STATE_DIR/<job-uuid>/job.json
STATE_DIR/<job-uuid>/command.json
STATE_DIR/<job-uuid>/stdout.log
STATE_DIR/<job-uuid>/stderr.log
```

On Unix, job directories use mode `700`; files use mode `600`. These artifacts
may contain sensitive pentest evidence and must be protected and retained or
removed according to your engagement policy.

## Shell completions

Generate all supported completion files:

```bash
make completions
```

They are written under `target/completions/`. Direct generation is also
available through the hidden command:

```bash
mcp-kali completions zsh > ~/.zfunc/_mcp-kali
mcp-kali-bridge completions zsh > ~/.zfunc/_mcp-kali-bridge
```

Supported shells are Bash, Zsh, Fish, PowerShell, and Elvish. For Zsh, ensure
the target directory is in `fpath`, then run `compinit`.

## Output contracts

- MCP protocol JSON is written only to client stdout, one JSON-RPC object per
  line. Diagnostics and tracing go to stderr.
- HTTP endpoints return JSON except the dashboard and full-log downloads.
- Job states are `queued`, `running`, `paused`, `succeeded`, `failed`,
  `timed_out`, `cancelled`, and `interrupted`.
- `POST` submission endpoints return `202 Accepted` plus a public job record.
- `job.json` and API job records intentionally omit private argv and the webhook
  destination; they expose only `webhook_configured`.
- The public `command` is a shell-quoted display string, not an executable
  replay format, and known sensitive options are redacted by default.
- MCP success and error results place the original API object under
  `structuredContent.data` with a security classification and handling notice.
- Output pages are bounded to 1 MiB and expose `offset`, `next_offset`, and
  `truncated` for deterministic paging.

See [docs/USER_MANUAL.md](docs/USER_MANUAL.md) for the complete HTTP and MCP
reference.

## Security

- The server has no built-in authentication. It refuses non-loopback binds
  unless `--allow-remote-bind` is explicitly set. Prefer loopback plus SSH; if a
  remote bind is unavoidable, use host firewall rules and an authenticated TLS
  reverse proxy.
- The client refuses cleartext HTTP to non-loopback hosts unless
  `--allow-insecure-http` is explicitly set.
- Declarative Plugin processes are launched with structured arguments and no
  shell. Shell interpreters are rejected from declarative execution definitions.
- The privileged `execute_command` Core tool also uses an explicit argv and can
  be disabled globally; it never provides a shell-string mode.
- Dashboard-controlled data is HTML-escaped. CSP, anti-framing, no-sniff,
  no-referrer, and no-store headers provide additional browser hardening.
- Job output and remote API text are untrusted. MCP instructions and response
  envelopes tell the calling agent never to follow instructions found in data.
- `--reveal-sensitive-data` is a deliberate lab override. It exposes complete
  command lines to dashboard/API users and webhook receivers; it does not avoid
  storage of the private execution specification needed for durable jobs.
- Completion webhooks require HTTPS except for localhost. They are currently
  best-effort, unsigned, and have no retry queue.

Operational controls remain the operator's responsibility: least privilege,
MFA for distribution systems, SSH key hygiene, host firewalling, centralized
logs, evidence retention, and incident response.

## Development and release

```bash
make help
make verify
make security
make checksum
make sbom
```

`make security` expects `cargo-audit`, `cargo-deny`, and `gitleaks`. `make sbom`
expects `cargo-cyclonedx`. Generated checksums, completions, and SBOM files live
under ignored `target/` paths.

Private release binaries may be signed with organization-approved tooling such
as `codesign` or `minisign`; signing is intentionally not required for local
development builds.

## Troubleshooting

- **Cannot write job state:** verify `~/.mcp-kali/var/jobs` is owned by the
  current user, or pass a writable `--state-dir`.
- **Remote bind refused:** use loopback plus SSH, or explicitly pass
  `--allow-remote-bind` only after adding network access controls.
- **Remote HTTP refused by the client:** use HTTPS/SSH; the insecure override is
  available for isolated labs.
- **A job stays queued:** verify `--max-concurrency`, inspect running/paused jobs,
  and check server stderr.
- **A Plugin tool is absent:** open the Monitor Tools view (or inspect
  `/api/plugins/diagnostics`); an invalid definition or missing declared
  command disables only that Plugin.
- **A job becomes interrupted after restart:** queued jobs resume, but formerly
  running processes cannot be adopted safely and are marked interrupted.
- **Dashboard output looks like HTML:** this is expected; output is escaped and
  rendered as literal untrusted text.
- **MCP host sees no tools:** run the client directly with `--version`/`--help`,
  confirm its absolute path, and verify server `/health` connectivity.

## Documentation

- [Comprehensive user manual](docs/USER_MANUAL.md)
- [Plugin authoring guide](docs/PLUGIN_AUTHORING.md)
- [Architecture and migration notes](RUST_PORT.md)
- [Release history](CHANGELOG.md)
- [Security policy](SECURITY.md)
- [Contributing guide](CONTRIBUTING.md)
- [Support guide](SUPPORT.md)
- [Code of conduct](CODE_OF_CONDUCT.md)
- [Publishing and release guide](docs/PUBLISHING.md)
- [Example configuration file](examples/mcp-kali.conf.example)

## License and upstream attribution

This project as a whole is licensed under GPL-3.0-or-later. See [LICENSE](LICENSE).

It is a Rust reimplementation and derivative work based on
[MCP-Kali-Server](https://github.com/Wh0am123/MCP-Kali-Server) by Yousof Nahya
(Wh0am123), which is licensed under MIT. Upstream-derived material remains
subject to the original MIT copyright and permission notice, reproduced in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). The MIT license is
GPL-compatible, so retaining that notice is compatible with distributing the
combined project under GPL-3.0-or-later.
