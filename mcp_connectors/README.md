# MCP connector packages

This directory contains source-only packaging for Codex and Claude Desktop on
Apple Silicon macOS. Compiled binaries and generated connector artifacts are
installed under `~/.mcp-kali/`; do not commit them.

## Prerequisites

Build and install the local stdio bridge on the Mac that runs the MCP host:

```bash
make client-install
```

The builders use `~/.mcp-kali/bin/mcp-kali-bridge`, which `make client-install`
creates. Set `MCP_KALI_HOME=/absolute/path` to stage an isolated installation
root. They reject non-Apple-Silicon binaries and versions that differ from
`Cargo.toml`.

## Codex

Prepare a local marketplace whose MCP configuration contains the bridge's
absolute path:

```bash
make connector-codex
```

The result is `~/.mcp-kali/codex`. Build it and install the `mcp-kali` plugin
through the Codex executable bundled with ChatGPT:

```bash
make connector-codex-install
```

That target adds the generated directory as the `personal` marketplace,
installs `mcp-kali@personal`, and lists the configured marketplaces. It passes
an absolute path because a quoted `~` is not expanded by the shell.

The plugin bundles the `use-mcp-kali` skill and uses the bridge's normal
config-file or environment precedence for the server URL.

## Claude Desktop

Install the MCPB CLI once:

```bash
npm install -g @anthropic-ai/mcpb
```

Then build the bundle:

```bash
make connector-claude-desktop
```

The resulting `~/.mcp-kali/plugins/mcp-kali.mcpb` can be dragged onto Claude
Desktop. It includes a copy of `~/.mcp-kali/bin/mcp-kali-bridge`. Its setup form
defaults to the loopback MCP Kali server and requires an explicit opt-in for
remote cleartext HTTP.

## Validation

```bash
make connectors-check
```

This always checks JSON syntax, version synchronization, and unfinished
placeholders. It also runs the official Codex validators when their scripts and
PyYAML are available, and validates the MCPB manifest when `mcpb` is installed.
