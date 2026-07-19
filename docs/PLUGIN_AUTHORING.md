# MCP Kali Plugin authoring

MCP Kali 2.0 discovers declarative YAML Plugins at server startup. Plugin files
are trusted local configuration, but every definition is validated before it is
registered. Invalid Plugins are isolated and reported through
`GET /api/plugins/diagnostics`.

## Layout and precedence

```text
SYSTEM_DATA_DIR/
└── plugins/
    ├── capability-catalog.yaml
    └── <plugin>/plugin.yaml

CONFIG_DIR/
└── plugins/
    ├── capability-catalog.yaml
    └── <plugin>/plugin.yaml
```

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
names. Every declared command must be executable on the server's `PATH` at
startup. Optional `requirements.privilege` and
`policy.requires_explicit_enable` metadata mark the tool privileged in the
public projection; they do not elevate the server process.

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
```

The tool belongs to the Plugin whose manifest is in the parent directory.

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

Restart the server, then inspect:

```bash
curl -sS http://127.0.0.1:5000/api/plugins
curl -sS http://127.0.0.1:5000/api/plugins/diagnostics
curl -sS http://127.0.0.1:5000/api/tools
```

Invoke a registered tool through the generic endpoint:

```bash
curl -sS http://127.0.0.1:5000/api/tools/example_print/invoke \
  -H 'content-type: application/json' \
  -d '{"arguments":{"value":"hello"},"timeout_seconds":30}'
```

Scheduled operations return `202 Accepted` and a normal public job record.
Output, cancellation, persistence, timeouts, redaction, and webhooks are owned
by the shared scheduler.
