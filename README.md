# MCP Kali 2.2.1

[![CI](https://github.com/dariohy/mcp-kali/actions/workflows/ci.yml/badge.svg)](https://github.com/dariohy/mcp-kali/actions/workflows/ci.yml)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)

MCP Kali is a Rust client/server system for scheduling Kali Linux security tools
without making an MCP agent wait for a long-running process. The server owns a
durable bounded queue, subprocess lifecycle, output files, HTTP API, completion
webhooks, and browser dashboard. The small stdio client exposes those functions
to an MCP host and returns job IDs immediately.

Use MCP Kali only on systems and targets you are explicitly authorized to test.

**Project status:** `v2.2.1` is the current stable release. Version 2.1.0 was
withdrawn because of system-installation and sudo-readiness defects.

## Contents

- [Architecture](#architecture)
- [Requirements](#requirements)
- [Build and install](#build-and-install)
- [Quick start](#quick-start)
- [Configuration](#configuration)
- [Service logging](#service-logging)
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
  `enum4linux`, and `dnsrecon`; unavailable requirements are reported without
  stopping the server.
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
│   ├── mcp-kali.config          # normal, non-secret ready-to-run configuration
│   ├── mcp-kali.config.example  # reference for every available setting
│   ├── plugins/                 # administrator Plugin/catalog overlay
│   └── references/              # operator-imported reference overlay
├── share/
│   └── plugins/                 # packaged Plugins, catalog, and references
│       ├── capability-catalog.yaml
│       └── <plugin>/
│           ├── plugin.yaml
│           └── references/*.md
└── var/
    ├── lib/
    │   ├── jobs/                # private durable job state and output
    │   └── archive/jobs/        # timestamp-windowed .tar.gz terminal-job archives
    └── log/mcp-kali/            # split structured server logs
```

If the MCP host needs only the bridge, use the smaller local installation:

```bash
make client-install
```

It builds and installs only `mcp-kali-bridge` plus its `~/.local/bin` symlink;
it does not create server configuration, Plugin data, or job state.

Install it with:

```bash
make install
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
uses a separate, explicit workflow.

To remove an installation, use `make uninstall`. As a regular user, it removes
the selected `MCP_KALI_HOME` tree and only matching `~/.local/bin` symlinks. As
root, it stops and disables `mcp-kali.service`, then removes the system
binaries, configuration, and active durable job state. Recoverable system job
archives are preserved for the administrator.
System service logs under `/var/log/mcp-kali` are also preserved.

### Systemd installation (Kali/Linux)

The repository includes a systemd unit template and a root-only installer. It
does not create an account or sudoers rule: choose an existing authorized Kali
user that has the required tools and, when root-required Plugin tools are used,
noninteractive sudo permission.

```bash
sudo make install
sudo make systemd-reload enable-system
```

When run as a regular user, `make install` creates the per-user tree. When run
as root, it performs the system install as the `kali` user by default. Select a
different existing account when needed, for example `sudo make install
MCP_KALI_USER=hutt MCP_KALI_GROUP=hutt`. `install-local` and `install-system`
remain available when automation needs to force a mode.

The system install places binaries under `/usr/local/bin`, immutable Plugin,
catalog, and reference data under `/usr/lib/mcp-kali`, administrator overlays
under `/etc/mcp-kali`, private state under `/var/lib/mcp-kali/jobs`, recoverable
archives under `/var/lib/mcp-kali/archive/jobs`, structured logs under
`/var/log/mcp-kali`, a rotation policy under `/etc/logrotate.d/mcp-kali`, and the generated unit at
`/usr/lib/systemd/system/mcp-kali.service`. Review the configuration and
sudoers policy before enabling it. Use `make status-system`, `make logs-json-system`,
and `make archive-jobs-system` for operations; `systemctl reload mcp-kali` maps
to `SIGHUP`. The service runs from the selected service user's home directory;
standalone runs retain the invoking shell's working directory.

When no `--config-file` or `MCP_KALI_CONFIG_FILE` is selected, MCP Kali uses
`/etc/mcp-kali/mcp-kali.config` if it exists. If it does not, it falls back to
the per-user `~/.mcp-kali/etc/mcp-kali.config` lookup. Legacy `.conf` files are
used only when neither canonical `.config` file exists.

Release builds use size optimization, full LTO, one codegen unit, stripped
symbols, and abort-on-panic behavior. No scheduler, API, dashboard, or MCP
functionality is removed.

## Quick start

Start the server on loopback with a workspace-local state directory:

```bash
./target/release/mcp-kali \
  --bind 127.0.0.1:5000 \
  --state-dir ./var/jobs \
  --system-data-dir . \
  --max-concurrency 2 \
  --default-timeout 432000
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
-> /etc/mcp-kali/mcp-kali.config when present, otherwise ~/.mcp-kali/etc/mcp-kali.config (or selected config file)
-> existing shell environment
-> command-line arguments
```

`make install-local` renders `~/.mcp-kali/etc/mcp-kali.config` if it does not
already exist. The file uses a simple `KEY=VALUE` syntax and must not contain
secrets. The repository template contains install-path placeholders, so use the
installer rather than copying it directly:

```bash
make install-local
```

Select another file with `--config-file PATH` or `MCP_KALI_CONFIG_FILE`.
Existing environment variables are never overwritten by values from the file.
Version 2.0 does not read `mcp-kali.env` or `~/.envs/.env_mcp-kali`, and does
not accept the prior `--env-file` / `MCP_KALI_ENV_FILE` selectors.

| Variable | Binary | Default | Description |
|---|---|---:|---|
| `MCP_KALI_HOME` | Both | `~/.mcp-kali` | Root of the self-contained per-user tree |
| `MCP_KALI_CONFIG_FILE` | Both | `/etc/mcp-kali/mcp-kali.config` when present, otherwise `~/.mcp-kali/etc/mcp-kali.config` | Alternate configuration-file path |
| `RUST_LOG` | Both | Binary-specific info filter | Tracing filter; server output follows `MCP_KALI_LOG_DIR`, while bridge diagnostics stay on stderr |
| `MCP_KALI_BIND` | Server | `127.0.0.1:5000` | HTTP API/dashboard bind address |
| `MCP_KALI_STATE_DIR` | Server | `~/.mcp-kali/var/lib/jobs` | Private durable job directory |
| `MCP_KALI_LOG_DIR` | Server | Installed configuration sets a user or system log directory; otherwise unset | Existing writable directory for split JSONL logs; absent or unusable falls back to stdout |
| `MCP_KALI_JOB_ARCHIVE_DIR` | Server | `~/.mcp-kali/var/lib/archive/jobs` | Private recoverable archive for terminal jobs |
| `MCP_KALI_JOB_ARCHIVE_AFTER_MINUTES` | Server | `60` | Minimum terminal-job age used by `SIGUSR1`, range 1–5256000 minutes |
| `MCP_KALI_MAX_CONCURRENCY` | Server | `4` | Simultaneous jobs, range 1–256 |
| `MCP_KALI_DEFAULT_TIMEOUT` | Server | `432000` (five days) | Default wall timeout, range 1–604800 seconds (seven days) |
| `MCP_KALI_REVEAL_SENSITIVE_DATA` | Server | `false` | Show unredacted commands in public records |
| `MCP_KALI_SYSTEM_DATA_DIR` | Server | `~/.mcp-kali/share` | Packaged Plugin, catalog, and reference data |
| `MCP_KALI_CONFIG_DIR` | Server | `~/.mcp-kali/etc` | Administrator Plugin, catalog, and reference overlays |
| `MCP_KALI_DISABLE_EXECUTE_COMMAND` | Server | `false` | Remove the privileged free-execution tool |
| `MCP_KALI_PRIVILEGE_ELEVATION` | Server | `auto` | `auto` uses `sudo -n` for declarative root-required tools unless already root; `none` runs them directly |
| `MCP_KALI_ALLOW_REMOTE_BIND` | Server | `false` | Acknowledge an unauthenticated non-loopback bind |
| `MCP_KALI_BRIDGE_SERVER` | Client | `http://127.0.0.1:5000` | Server origin URL |
| `MCP_KALI_BRIDGE_ALLOW_INSECURE_HTTP` | Client | `false` | Permit HTTP to a non-loopback server |

Local state created by earlier releases remains at `~/.mcp-kali/var/jobs` and
is not moved automatically. To retain access to it, either set
`MCP_KALI_STATE_DIR` and `MCP_KALI_JOB_ARCHIVE_DIR` to the old locations, or
move the directories to the new layout while the server is stopped.

CLI flags matching these settings are documented by each binary's `--help`.
`MCP_KALI_LOG_DIR` is configuration/environment-only and has no CLI flag.
Sensitive command values should be supplied through MCP job arguments and are
redacted from public records by default; do not put secrets directly in process
arguments unless the target tool requires them.

## Service logging

When `MCP_KALI_LOG_DIR` names an existing writable non-symlink directory, the
server writes newline-delimited JSON with UTC event timestamps to two fixed
private files:

```text
mcp-kali.jsonl        # TRACE, DEBUG, and INFO only
mcp-kali.error.jsonl  # WARN and ERROR only
```

Events are exclusive to one file. Both files use mode `0600`; installed log
directories use `0700`. If the variable is absent or either file cannot be
opened, all server tracing uses human-readable stdout instead. The stdio bridge
is unchanged: its protocol stays on stdout and its diagnostics stay on stderr.

System installations include daily logrotate configuration with 30 rotations,
compression, and a post-rotation SIGHUP. SIGHUP flushes and reopens both fixed
filenames before performing the existing runtime reload. Changing the configured
directory still requires a service restart.

```bash
sudo make logs-json-system
sudo tail -F /var/log/mcp-kali/mcp-kali.jsonl \
  /var/log/mcp-kali/mcp-kali.error.jsonl
sudo jq -c 'select(.level == "ERROR")' /var/log/mcp-kali/mcp-kali.error.jsonl
sudo journalctl -u mcp-kali.service -f  # stdout fallback and process diagnostics
```

rsyslog can follow `/var/log/mcp-kali/mcp-kali*.jsonl` with `imfile`, and
syslog-ng can use a `wildcard-file()` source followed by `json-parser()`. Their
service identities must be able to traverse the private directory. Configure
persistent file offsets and exclude rotated/compressed names to avoid replay.

Service events never contain request bodies, command arguments, webhook
payloads, job output, or captured artifacts. Job stdout/stderr remains only in
`STATE_DIR/<job-id>/`; log shippers must treat both trees as separate data.

## Plugins and capabilities

The server loads packaged definitions from `SYSTEM_DATA_DIR/plugins`, then an
administrator overlay from `CONFIG_DIR/plugins`. A valid overlay Plugin or tool
with the same identity replaces the packaged definition. Discovery happens at
startup; malformed files are isolated and reported at
`/api/plugins/diagnostics`.

In a source checkout, the packaged definitions live in `./plugins`; use the
repository root as `SYSTEM_DATA_DIR` when running directly from source.

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

For a declarative tool that needs privileged probes, set its tool-level
`requirements.privilege: root`. With the default
`MCP_KALI_PRIVILEGE_ELEVATION=auto`, the server runs that tool directly when it
is already root, otherwise as `sudo -n -- program args...`; it never prompts.
At startup, auto mode verifies that the server user can use non-interactive
sudo and publishes root-required tools as disabled when it cannot. `none`
disables that automatic prefix. This setting does not change the Core
`execute_command` tool: callers may explicitly invoke `sudo -n` as its program
when needed.

The separate capability catalog maps stable semantic IDs to Plugin providers.
Catalog references remain visible with an availability flag when an optional
Plugin or tool is not installed.

Plugins may ship validated Markdown guidance under `<plugin>/references/`.
Packaged references are loaded first; files under `CONFIG_DIR/references/` or
an overlay Plugin's `references/` directory may add or replace them by stable
reference ID. The dashboard and MCP Resources read this same registry. Invalid
references are isolated at `/api/references/diagnostics`.

Import an operator guide without editing packaged data:

```bash
mcp-kali references import ./internal-nmap.md \
  --id nmap.internal-discovery \
  --plugin org.mcp-kali.nmap \
  --title "Internal Nmap discovery" \
  --description "Approved internal discovery procedure." \
  --tag nmap \
  --related-tool nmap_host_discovery \
  --related-capability network.host_discovery
```

The import refuses symlinks, files over 256 KiB, invalid identifiers, and an
existing destination. Imported content is guidance only and cannot add an
executable tool. Send `SIGHUP` after importing while the service is running.

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

`command` must be an absolute executable path; MCP hosts do not expand `~`.
For example, on macOS use `/Users/you/.local/bin/mcp-kali-bridge`, after running
`make client-install` on that Mac. The bridge runs beside the MCP host, not on
the Kali server, and connects to the server URL supplied in `args`.

The client retrieves current tools and references from the server for MCP
`tools/list`, `resources/list`, and `resources/read` requests. It forwards tool
calls to the generic invocation API. For a long-lived bridge connection, it
polls the server every five seconds and sends tool- or resource-list change
notifications when either projection changes.

### Runtime signals

`SIGTERM` (and `SIGINT`) performs a graceful shutdown: the server stops
accepting jobs, cancels queued work, sends `SIGTERM` to active job process
groups, waits up to 10 seconds, then force-kills survivors and persists their
terminal state before exiting. `SIGHUP` flushes and reopens configured server
log files, then atomically reloads the
Plugin/catalog/reference runtime. When `MCP_KALI_MAX_CONCURRENCY` comes from the loaded
configuration file rather than an environment variable or CLI flag, `SIGHUP`
also applies its new value without interrupting running jobs. A lower limit
drains naturally; a higher limit starts queued jobs immediately. A reload with
configuration, Plugin, or reference diagnostics retains the prior runtime.
Send a second `SIGTERM` (or `SIGINT`) to skip the grace period and immediately
force-kill active job process groups.
`SIGUSR1` archives terminal jobs whose finish time is at least
`MCP_KALI_JOB_ARCHIVE_AFTER_MINUTES` old. It never touches queued, running, or
paused jobs. For a system installation, `sudo make archive-jobs-system` sends
this signal and the service journal records the result.
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
- a References view of packaged and operator-imported guidance, displayed as
  escaped Markdown text with provenance and isolated diagnostics;
- queue order, state, tool, command summary, and elapsed time;
- a left-edge `>` control that expands full metadata and wrapped command text;
- pause, resume, remove, and force-kill controls where applicable;
- previewed, confirmed archiving of terminal jobs by age in minutes;
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
STATE_DIR/<job-uuid>/integrity.json
```

Every terminal job receives a private `integrity.json` manifest containing
SHA-256 hashes and byte counts for its final metadata, execution specification,
and output files. Archiving verifies that manifest, then writes a timestamped
`jobs_<oldest-start>_to_<newest-finish>_<count>.tar.gz` file under
`JOB_ARCHIVE_DIR`. Archived jobs disappear from the API and dashboard but retain
their metadata, private execution specification, output, and integrity manifest.
MCP Kali does not automatically delete archived evidence; retention is an
administrator policy.

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
  line. Bridge diagnostics and tracing go to stderr. Server tracing uses split
  JSONL when configured and otherwise falls back to stdout.
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

- **Cannot write job state:** verify `~/.mcp-kali/var/lib/jobs` is owned by the
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
- **Bridge reports invalid JSON:** its error now identifies the HTTP status,
  content type, and response size without printing the body. Verify the exact
  bridge `--server` URL with `curl -i`; for SSH tunnelling, prefer an unused
  local port such as `5500` when another local process owns `5000`.

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
- [Default configuration template](examples/mcp-kali.config)
- [Configuration reference](examples/mcp-kali.config.example)

## License and upstream attribution

This project as a whole is licensed under GPL-3.0-or-later. See [LICENSE](LICENSE).

It is a Rust reimplementation and derivative work based on
[MCP-Kali-Server](https://github.com/Wh0am123/MCP-Kali-Server) by Yousof Nahya
(Wh0am123), which is licensed under MIT. Upstream-derived material remains
subject to the original MIT copyright and permission notice, reproduced in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). The MIT license is
GPL-compatible, so retaining that notice is compatible with distributing the
combined project under GPL-3.0-or-later.
