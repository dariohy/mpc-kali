# MCP Kali Plugin authoring

MCP Kali 2.x discovers declarative YAML Plugins at startup and on `SIGHUP`
reload. Plugin files are trusted local configuration, but every definition is
validated before it is registered. Invalid Plugins are isolated and reported
through `GET /api/plugins/diagnostics`. A reload that produces diagnostics keeps
the last-known-good Plugin and capability runtime active.

## Layout and precedence

```text
SYSTEM_DATA_DIR/
└── plugins/
    ├── capability-catalog.yaml
    └── <plugin>/
        ├── plugin.yaml
        ├── tools/*.yaml
        └── references/*.md

CONFIG_DIR/
├── plugins/                     # executable definition overlays
│   ├── capability-catalog.yaml
│   └── <plugin>/
│       ├── plugin.yaml
│       ├── tools/*.yaml
│       └── references/*.md
└── references/                  # guidance-only overlays/imports
    └── <plugin-id>/*.md
```

For a user installation, `SYSTEM_DATA_DIR` is `~/.mcp-kali/share` and
`CONFIG_DIR` is `~/.mcp-kali/etc`. For a system installation, packaged data is
read-only under `/usr/lib/mcp-kali` and the administrator overlay is
`/etc/mcp-kali`. In a source checkout, packaged definitions are in the
repository's top-level `plugins/` directory; use the repository root as
`SYSTEM_DATA_DIR` when running directly from source.

The packaged system layer loads first. A valid administrator-overlay Plugin
with the same `metadata.id`, or tool with the same `metadata.name`, replaces the
packaged entry. Duplicate identities within one layer are rejected. Catalog
entries merge by capability ID, with the administrator entry replacing the
packaged entry.

Plugin manifests may declare tools inline or place `PluginTool` documents under
a sibling `tools/` directory. Directory names have no semantic meaning.

## Minimal manifest

```yaml
apiVersion: mcp-kali/v1
kind: Plugin
metadata:
  id: local.example
  name: Example
  version: 1.0.0
  description: Optional human description.
categories: [information-gathering]
tags: [local]
requires:
  commands: [printf]
tools:
  - metadata:
      name: example_print
      description: Print one validated value.
    input_schema:
      type: object
      additionalProperties: false
      required: [value]
      properties:
        value: {type: string, minLength: 1}
    execution:
      program: printf
      args: ["%s\\n", "{{value}}"]
    policy:
      timeout_seconds: 30
```

Plugin IDs use lowercase ASCII letters, digits, dots, and hyphens. MCP tool
names use lowercase ASCII letters, digits, and underscores. The input schema
must be a valid JSON Schema object schema.

`requires.commands` and tool-level `requirements.commands` contain bare command
names. Every declared command must be executable on the server's `PATH` at load
time; a missing command makes that Plugin unavailable and produces a diagnostic.

Tool-level `requirements.privilege` accepts only `root`. It is published in MCP
metadata and the Monitor Tools view. With the default
`MCP_KALI_PRIVILEGE_ELEVATION=auto`, a root-required declarative tool runs as
the server identity when the server is root; otherwise its rendered argv is
prefixed with `sudo -n --`. No interactive sudo prompt is opened.

At load time, the runtime resolves each root-required program and checks that
specific executable with `sudo -n -l /absolute/path/to/program`; it does not
use `sudo -v`. In auto mode, a failed per-program check marks only that tool
disabled in MCP (`_meta.enabled: false` and `_meta.elevation`) and in the
Monitor. This avoids a password-required sudo rule incorrectly masking a later
`NOPASSWD` rule for the declared program. Operators can test the same condition
for a service account without executing the program:

```bash
sudo -u <service-user> /usr/bin/sudo -n -l /usr/bin/nmap
```

Set the runtime mode to `none` to leave a root-required tool's argv unchanged.
Plugin-level privilege remains descriptive; declare it on every tool that
requires elevation. `policy.requires_explicit_enable` only marks a tool
privileged in public metadata; it does not grant or broker privilege.

This mechanism applies only to declarative Plugin tools. The Core
`execute_command` tool remains explicit raw argv; its caller may invoke
`sudo -n -- command ...` itself when appropriate.

## Safe argument templates

`execution.program` is a literal program name or approved path. It cannot be a
template or a shell interpreter. Each `execution.args` item is one of:

```yaml
- --literal
- "{{top_level_string_field}}"
- when: optional_field
  args: [--flag, "{{optional_field}}"]
```

A condition is active when its top-level value is `true` or non-null/non-empty.
Every rendered substitution must be a string and becomes exactly one process
argument. Partial interpolation (`--flag={{value}}`), nested paths, expressions,
loops, pipes, shell expansion, and command substitution are unsupported and
rejected.

When an executable requires an option and its value in one argv element (for
example `--option=value`), model that complete argument as a schema-constrained
input string. Do not introduce partial interpolation to assemble it.

Use JSON Schema constraints such as `required`, `enum`, `oneOf`, lengths, and
`additionalProperties: false` to make the invocation contract precise. Values
must never rely on shell quoting because no shell is involved.

### Managed analysis paths

Scheduled tools automatically receive the reserved runtime inputs
`save_stdout_to` and `save_stderr_to`; Plugin schemas must not define them. To
let a native executable write one or more artifacts, declare a top-level string
field in `input_schema`, render it as a whole argument, and list the native
suffixes under `execution.analysis_paths`:

```yaml
input_schema:
  type: object
  additionalProperties: false
  properties:
    output_basename:
      type: string
      minLength: 1
      maxLength: 4096
execution:
  program: nmap
  args:
    - when: output_basename
      args: [-oA, "{{output_basename}}"]
  analysis_paths:
    output_basename: [.nmap, .xml, .gnmap]
```

Before rendering, the runtime replaces a supplied field with an absolute path
beneath `MCP_KALI_PROJECTS_DIR` and records each suffixed path in the public job
metadata. Each declared field must exist as a string property. Suffixes are
limited to 32 safe characters and cannot contain separators, control
characters, or `..`. This mechanism is for output files only; do not use it to
model arbitrary scanner input paths.

## External tool files

A file under `tools/` uses this shape:

```yaml
apiVersion: mcp-kali/v1
kind: PluginTool
metadata:
  name: example_version
  description: Read the local Example version.
input_schema:
  type: object
  additionalProperties: false
execution:
  program: example
  args: [--version]
requirements:
  commands: [example]
policy:
  timeout_seconds: 30
```

The tool belongs to the Plugin whose manifest is in the parent directory.

## Plugin references

A Plugin may include Markdown documents that help operators and MCP clients
select and interpret its declarative tools. References are guidance only: they
cannot add tools, change schemas, grant privilege, or authorize a target.

Place packaged references under the Plugin's `references/` directory. Each
file starts with validated YAML front matter:

```markdown
---
apiVersion: mcp-kali/v1
kind: PluginReference
metadata:
  id: example.safe-use
  title: Safe Example use
  description: Choose and interpret the Example Plugin tools.
plugin: local.example
tags: [example, operations]
related_tools: [example_print]
related_capabilities: [example.printing]
---

# Safe Example use

Use `example_print` when...
```

Reference IDs use lowercase ASCII letters, digits, dots, and hyphens. Related
tools must be registered by the declared Plugin, and related capabilities must
exist in the resolved catalog. Files must be UTF-8 Markdown, no larger than
256 KiB, and must not be symlinks. An overlay reference with the same ID
replaces its packaged counterpart.

The server exposes the one merged registry through the Monitor References tab,
`GET /api/references`, and MCP `resources/list` / `resources/read`. Markdown is
displayed as escaped text in the Monitor. Imported or packaged guidance cannot
override an MCP client's governing instructions, authorization, or tool policy.

Operators can wrap and copy a local Markdown guide into
`CONFIG_DIR/references/<plugin-id>/` with:

```bash
mcp-kali references import ./guide.md \
  --id example.operator-guide \
  --plugin local.example \
  --title "Example operator guide" \
  --description "Local approved use of Example." \
  --related-tool example_print \
  --related-capability example.printing
```

Import refuses to overwrite an existing reference. Send `SIGHUP` after a
successful import; a reload that finds reference diagnostics retains the
last-known-good Plugin, capability, and reference runtime.

## Capability catalog

Capabilities are semantic discovery records, not executable definitions:

```yaml
apiVersion: mcp-kali/v1
kind: CapabilityCatalog
version: 1
capabilities:
  - id: example.printing
    description: Print an example value.
    providers:
      - plugin: local.example
        tools: [example_print]
```

`plugin` is required; `tools` is optional. References to absent Plugins or tools
remain in the API with `available: false`, allowing optional providers to be
documented without breaking startup.

## Validate and invoke

Start/restart the server, or send `SIGHUP` after changing an installed
definition, then inspect:

```bash
curl -sS http://127.0.0.1:5000/api/plugins
curl -sS http://127.0.0.1:5000/api/plugins/diagnostics
curl -sS http://127.0.0.1:5000/api/tools
curl -sS http://127.0.0.1:5000/api/references
curl -sS http://127.0.0.1:5000/api/references/diagnostics
```

Invoke a registered tool through the generic endpoint:

```bash
curl -sS http://127.0.0.1:5000/api/tools/example_print/invoke \
  -H 'content-type: application/json' \
  -d '{"arguments":{"value":"hello"},"timeout_seconds":30}'
```

Scheduled operations return `202 Accepted` and a normal public job record.
Output, cancellation, persistence, timeouts, redaction, and webhooks are owned
by the shared scheduler. Root-required tools that are not enabled return their
elevation diagnostic rather than being scheduled.
