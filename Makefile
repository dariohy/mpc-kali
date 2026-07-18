SERVER_BIN := mcp-kali-server
CLIENT_BIN := mcp-kali-client
CARGO := cargo
VERSION := $(shell awk -F '"' '/^version = / { print $$2; exit }' Cargo.toml)
INSTALL_DIR ?= $(HOME)/.local/bin
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
	@echo "  install-local Install both binaries under INSTALL_DIR"
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
	$(CARGO) run --bin $(SERVER_BIN) -- --state-dir ./var/jobs

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
	mkdir -p "$(INSTALL_DIR)"
	install -m 0755 "target/release/$(SERVER_BIN)" "$(INSTALL_DIR)/$(SERVER_BIN)"
	install -m 0755 "target/release/$(CLIENT_BIN)" "$(INSTALL_DIR)/$(CLIENT_BIN)"

checksum: release
	cd target/release && shasum -a 256 "$(SERVER_BIN)" "$(CLIENT_BIN)" > SHA256SUMS

security: check clippy test
	cargo audit
	cargo deny check
	gitleaks detect --source . --redact

sbom:
	mkdir -p "$(SECURITY_DIR)"
	cargo cyclonedx --format json --output-cdx "$(SECURITY_DIR)/mcp-kali-$(VERSION).cdx.json"

clean:
	$(CARGO) clean
