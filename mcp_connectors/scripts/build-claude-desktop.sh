#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd -P)
SOURCE_ROOT="$REPO_ROOT/mcp_connectors/claude-desktop/mcp-kali"
MCP_KALI_HOME=${MCP_KALI_HOME:-$HOME/.mcp-kali}
output_file=${CLAUDE_CONNECTOR_FILE:-$MCP_KALI_HOME/plugins/mcp-kali.mcpb}

bridge_path=${MCP_KALI_BRIDGE_PATH:-$MCP_KALI_HOME/bin/mcp-kali-bridge}
if [ -z "$bridge_path" ] || [ ! -x "$bridge_path" ]; then
  echo "mcp-kali-bridge was not found at $bridge_path; run 'make client-install' or set MCP_KALI_BRIDGE_PATH" >&2
  exit 2
fi
if ! command -v mcpb >/dev/null 2>&1; then
  echo "mcpb was not found; install it with 'npm install -g @anthropic-ai/mcpb'" >&2
  exit 2
fi

bridge_dir=$(CDPATH= cd -- "$(dirname -- "$bridge_path")" && pwd -P)
bridge_path="$bridge_dir/$(basename -- "$bridge_path")"
if [ "$(uname -s)" != "Darwin" ] || [ "$(uname -m)" != "arm64" ]; then
  echo "Claude Desktop connector builds are supported only on Apple Silicon macOS" >&2
  exit 2
fi
if ! file "$bridge_path" | grep -q 'Mach-O 64-bit executable arm64'; then
  echo "bridge is not an Apple Silicon Mach-O executable: $bridge_path" >&2
  exit 2
fi

expected_version=$(awk -F '"' '/^version = / { print $2; exit }' "$REPO_ROOT/Cargo.toml")
actual_version=$("$bridge_path" --version | awk '{ print $2 }')
if [ "$actual_version" != "$expected_version" ]; then
  echo "bridge version $actual_version does not match Cargo.toml version $expected_version" >&2
  exit 2
fi
manifest_version=$(sed -n 's/.*"version": "\([^"]*\)".*/\1/p' "$SOURCE_ROOT/manifest.json" | head -n 1)
if [ "$manifest_version" != "$expected_version" ]; then
  echo "MCPB manifest version $manifest_version does not match Cargo.toml version $expected_version" >&2
  exit 2
fi

stage_dir=$(mktemp -d "${TMPDIR:-/tmp}/mcp-kali-mcpb.XXXXXX")
trap 'rm -rf "$stage_dir"' EXIT HUP INT TERM
if [ "$output_file" != "$MCP_KALI_HOME/plugins/mcp-kali.mcpb" ]; then
  echo "Claude connector output must be $MCP_KALI_HOME/plugins/mcp-kali.mcpb" >&2
  exit 2
fi

mkdir -p "$stage_dir/server" "$(dirname -- "$output_file")"
cp "$SOURCE_ROOT/manifest.json" "$SOURCE_ROOT/README.md" "$REPO_ROOT/LICENSE" "$stage_dir/"
cp "$bridge_path" "$stage_dir/server/mcp-kali-bridge"
chmod 0755 "$stage_dir/server/mcp-kali-bridge"

rm -f "$output_file"
mcpb validate "$stage_dir"
mcpb pack "$stage_dir" "$output_file"

echo "Built Claude Desktop bundle: $output_file"
