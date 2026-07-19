SERVER_BIN := mcp-kali
CLIENT_BIN := mcp-kali-bridge
CARGO := cargo
VERSION := $(shell awk -F '"' '/^version = / { print $$2; exit }' Cargo.toml)
MCP_KALI_HOME ?= $(HOME)/.mcp-kali
INSTALL_DIR ?= $(MCP_KALI_HOME)/bin
DATA_DIR ?= $(MCP_KALI_HOME)/share
CONFIG_DIR ?= $(MCP_KALI_HOME)/etc
STATE_DIR ?= $(MCP_KALI_HOME)/var/jobs
PLUGIN_DIR := $(DATA_DIR)/plugins
OVERLAY_PLUGIN_DIR := $(CONFIG_DIR)/plugins
CONFIG_FILE := $(CONFIG_DIR)/mcp-kali.conf
LOCAL_BIN_DIR ?= $(HOME)/.local/bin
COMPLETION_DIR := target/completions
SECURITY_DIR := target/security

.PHONY: help fmt fmt-check check clippy test build release verify run-server run-client \
	completions install-local checksum security sbom clean

help:
	@echo "MCP Kali $(VERSION) development and release targets"
	@echo "  fmt           Format Rust sources"
	@echo "  fmt-check     Verify Rust formatting without changes"
	@echo "  check         Compile all targets and features"
	@echo "  clippy        Run strict Clippy checks"
	@echo "  test          Run the full test suite"
	@echo "  build         Build debug binaries"
	@echo "  release       Build size-optimized release binaries"
	@echo "  verify        Run fmt, check, clippy, test, and release"
	@echo "  run-server    Run a local development server"
	@echo "  run-client    Run the stdio MCP client"
	@echo "  completions   Generate completion scripts for both binaries"
	@echo "  install-local Create a self-contained per-user installation under ~/.mcp-kali"
	@echo "  checksum      Generate target/release/SHA256SUMS"
	@echo "  security      Run audit, dependency policy, and secret scan"
	@echo "  sbom          Generate a CycloneDX JSON SBOM (cargo-cyclonedx required)"
	@echo "  clean         Remove Cargo build artifacts"

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all -- --check

check:
	$(CARGO) check --all-targets --all-features

clippy:
	$(CARGO) clippy --all-targets --all-features -- -D warnings

test:
	$(CARGO) test --all-features

build:
	$(CARGO) build

release:
	$(CARGO) build --release

verify: fmt-check check clippy test release

run-server:
	$(CARGO) run --bin $(SERVER_BIN) -- --state-dir ./var/jobs --system-data-dir ./share/mcp-kali --config-dir ./etc

run-client:
	$(CARGO) run --bin $(CLIENT_BIN) -- --server http://127.0.0.1:5000

completions: release
	mkdir -p "$(COMPLETION_DIR)"
	target/release/$(SERVER_BIN) completions bash > "$(COMPLETION_DIR)/$(SERVER_BIN).bash"
	target/release/$(SERVER_BIN) completions zsh > "$(COMPLETION_DIR)/_$(SERVER_BIN)"
	target/release/$(SERVER_BIN) completions fish > "$(COMPLETION_DIR)/$(SERVER_BIN).fish"
	target/release/$(SERVER_BIN) completions powershell > "$(COMPLETION_DIR)/$(SERVER_BIN).ps1"
	target/release/$(SERVER_BIN) completions elvish > "$(COMPLETION_DIR)/$(SERVER_BIN).elv"
	target/release/$(CLIENT_BIN) completions bash > "$(COMPLETION_DIR)/$(CLIENT_BIN).bash"
	target/release/$(CLIENT_BIN) completions zsh > "$(COMPLETION_DIR)/_$(CLIENT_BIN)"
	target/release/$(CLIENT_BIN) completions fish > "$(COMPLETION_DIR)/$(CLIENT_BIN).fish"
	target/release/$(CLIENT_BIN) completions powershell > "$(COMPLETION_DIR)/$(CLIENT_BIN).ps1"
	target/release/$(CLIENT_BIN) completions elvish > "$(COMPLETION_DIR)/$(CLIENT_BIN).elv"

install-local: release
	@test "$$(id -u)" -ne 0 || { echo "install-local is for a non-root user; system/service installation is not implemented yet" >&2; exit 2; }
	mkdir -p "$(INSTALL_DIR)"
	mkdir -p "$(PLUGIN_DIR)"
	mkdir -p "$(OVERLAY_PLUGIN_DIR)"
	mkdir -p "$(STATE_DIR)"
	mkdir -p "$(LOCAL_BIN_DIR)"
	@test -e "$(CONFIG_FILE)" || install -m 0644 "examples/mcp-kali.conf.example" "$(CONFIG_FILE)"
	install -m 0755 "target/release/$(SERVER_BIN)" "$(INSTALL_DIR)/$(SERVER_BIN)"
	install -m 0755 "target/release/$(CLIENT_BIN)" "$(INSTALL_DIR)/$(CLIENT_BIN)"
	@for binary in "$(SERVER_BIN)" "$(CLIENT_BIN)"; do \
		link="$(LOCAL_BIN_DIR)/$$binary"; \
		if [ -e "$$link" ] && [ ! -L "$$link" ]; then \
			echo "refusing to replace non-symlink $$link" >&2; exit 2; \
		fi; \
	done; \
	for binary in "$(SERVER_BIN)" "$(CLIENT_BIN)"; do \
		link="$(LOCAL_BIN_DIR)/$$binary"; \
		ln -sfn "$(abspath $(INSTALL_DIR))/$$binary" "$$link"; \
	done
	cp -R share/mcp-kali/plugins/. "$(PLUGIN_DIR)/"

checksum: release
	cd target/release && shasum -a 256 "$(SERVER_BIN)" "$(CLIENT_BIN)" > SHA256SUMS

security: check clippy test
	cargo audit
	cargo deny check
	gitleaks detect --source . --redact

sbom:
	mkdir -p "$(SECURITY_DIR)"
	cargo cyclonedx --format json --override-filename "mcp-kali-$(VERSION).cdx"
	mv "mcp-kali-$(VERSION).cdx.json" "$(SECURITY_DIR)/mcp-kali-$(VERSION).cdx.json"

clean:
	$(CARGO) clean
