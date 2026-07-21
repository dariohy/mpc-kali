SERVER_BIN := mcp-kali
CLIENT_BIN := mcp-kali-bridge
CARGO := cargo
VERSION := $(shell awk -F '"' '/^version = / { print $$2; exit }' Cargo.toml)
MCP_KALI_HOME ?= $(HOME)/.mcp-kali
INSTALL_DIR ?= $(MCP_KALI_HOME)/bin
CONFIG_DIR ?= $(MCP_KALI_HOME)/etc
DATA_DIR ?= $(MCP_KALI_HOME)/share
STATE_DIR ?= $(MCP_KALI_HOME)/var/jobs
PLUGIN_DIR := $(DATA_DIR)/plugins
REFERENCE_OVERLAY_DIR := $(CONFIG_DIR)/references
CONFIG_FILE := $(CONFIG_DIR)/mcp-kali.conf
LOCAL_BIN_DIR ?= $(HOME)/.local/bin
COMPLETION_DIR := target/completions
SECURITY_DIR := target/security
SYSTEM_PREFIX ?= /usr/local
SYSTEM_BIN_DIR ?= $(SYSTEM_PREFIX)/bin
SYSTEM_CONFIG_DIR ?= /etc/mcp-kali
SYSTEM_DATA_DIR ?= /usr/lib/mcp-kali
SYSTEM_PLUGIN_DIR := $(SYSTEM_DATA_DIR)/plugins
SYSTEM_REFERENCE_OVERLAY_DIR := $(SYSTEM_CONFIG_DIR)/references
SYSTEM_STATE_DIR ?= /var/lib/mcp-kali/jobs
SYSTEMD_UNIT_DIR ?= /usr/lib/systemd/system
MCP_KALI_USER ?= kali
MCP_KALI_GROUP ?= $(MCP_KALI_USER)
SYSTEM_CONFIG_FILE := $(SYSTEM_CONFIG_DIR)/mcp-kali.conf
SYSTEM_UNIT_FILE := $(SYSTEMD_UNIT_DIR)/mcp-kali.service

.PHONY: help fmt fmt-check check clippy test build client release verify run-server run-client \
	completions client-install install-local checksum security sbom clean \
	install install-local install-system uninstall uninstall-local uninstall-system \
	systemd-reload enable-system disable-system status-system logs-system

help:
	@echo "MCP Kali $(VERSION) development and release targets"
	@echo "  fmt           Format Rust sources"
	@echo "  fmt-check     Verify Rust formatting without changes"
	@echo "  check         Compile all targets and features"
	@echo "  clippy        Run strict Clippy checks"
	@echo "  test          Run the full test suite"
	@echo "  build         Build debug binaries"
	@echo "  client        Build only the debug MCP bridge binary"
	@echo "  release       Build size-optimized release binaries"
	@echo "  verify        Run fmt, check, clippy, test, and release"
	@echo "  run-server    Run a local development server"
	@echo "  run-client    Run the stdio MCP client"
	@echo "  completions   Generate completion scripts for both binaries"
	@echo "  client-install Build and locally install only mcp-kali-bridge"
	@echo "  install       Install locally as a user, or system-wide as root"
	@echo "  install-local Create a self-contained per-user installation under ~/.mcp-kali"
	@echo "  install-system Install binaries, read-only data, config template, and systemd unit (root; defaults to kali)"
	@echo "  uninstall     Remove the local user install, or the system install as root"
	@echo "  systemd-reload Reload systemd unit files after install-system"
	@echo "  enable-system  Enable and start mcp-kali.service"
	@echo "  disable-system Disable and stop mcp-kali.service"
	@echo "  status-system  Show mcp-kali.service status"
	@echo "  logs-system    Follow mcp-kali.service journal logs"
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

client:
	$(CARGO) build --bin $(CLIENT_BIN)

release:
	$(CARGO) build --release

verify: fmt-check check clippy test release

run-server:
	$(CARGO) run --bin $(SERVER_BIN) -- --state-dir ./var/jobs --system-data-dir ./ --config-dir ./etc

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

client-install:
	@test "$$(id -u)" -ne 0 || { echo "client-install is for a non-root user" >&2; exit 2; }
	$(CARGO) build --release --bin $(CLIENT_BIN)
	mkdir -p "$(INSTALL_DIR)" "$(LOCAL_BIN_DIR)"
	@if [ -e "$(LOCAL_BIN_DIR)/$(CLIENT_BIN)" ] && [ ! -L "$(LOCAL_BIN_DIR)/$(CLIENT_BIN)" ]; then \
		echo "refusing to replace non-symlink $(LOCAL_BIN_DIR)/$(CLIENT_BIN)" >&2; exit 2; \
	fi
	install -m 0755 "target/release/$(CLIENT_BIN)" "$(INSTALL_DIR)/$(CLIENT_BIN)"
	ln -sfn "$(abspath $(INSTALL_DIR))/$(CLIENT_BIN)" "$(LOCAL_BIN_DIR)/$(CLIENT_BIN)"

install-local: release
	@test "$$(id -u)" -ne 0 || { echo "install-local is for a non-root user; use make install MCP_KALI_USER=<authorized-user> as root" >&2; exit 2; }
	mkdir -p "$(INSTALL_DIR)"
	mkdir -p "$(PLUGIN_DIR)" "$(CONFIG_DIR)/plugins" "$(REFERENCE_OVERLAY_DIR)"
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
	cp -R plugins/. "$(PLUGIN_DIR)/"

install:
	@if [ "$$(id -u)" -eq 0 ]; then \
		$(MAKE) install-system MCP_KALI_USER="$(MCP_KALI_USER)" MCP_KALI_GROUP="$(MCP_KALI_GROUP)"; \
	else \
		$(MAKE) install-local; \
	fi

uninstall:
	@if [ "$$(id -u)" -eq 0 ]; then \
		$(MAKE) uninstall-system; \
	else \
		$(MAKE) uninstall-local; \
	fi

uninstall-local:
	@test "$$(id -u)" -ne 0 || { echo "uninstall-local is for a non-root user; use make uninstall as root for a system install" >&2; exit 2; }
	@case "$(MCP_KALI_HOME)" in ""|/|"$(HOME)") echo "refusing unsafe MCP_KALI_HOME=$(MCP_KALI_HOME)" >&2; exit 2;; esac
	@for binary in "$(SERVER_BIN)" "$(CLIENT_BIN)"; do \
		link="$(LOCAL_BIN_DIR)/$$binary"; expected="$(abspath $(INSTALL_DIR))/$$binary"; \
		if [ -L "$$link" ] && [ "$$(readlink "$$link")" = "$$expected" ]; then rm "$$link"; fi; \
	done
	rm -rf "$(MCP_KALI_HOME)"

uninstall-system:
	@test "$$(id -u)" -eq 0 || { echo "uninstall-system must run as root" >&2; exit 2; }
	@case "$(SYSTEM_CONFIG_DIR)" in ""|/) echo "refusing unsafe SYSTEM_CONFIG_DIR=$(SYSTEM_CONFIG_DIR)" >&2; exit 2;; esac
	@case "$(SYSTEM_DATA_DIR)" in */mcp-kali) :;; *) echo "refusing unsafe SYSTEM_DATA_DIR=$(SYSTEM_DATA_DIR); expected a path ending in /mcp-kali" >&2; exit 2;; esac
	@case "$(SYSTEM_STATE_DIR)" in ""|/) echo "refusing unsafe SYSTEM_STATE_DIR=$(SYSTEM_STATE_DIR)" >&2; exit 2;; esac
	@if command -v systemctl >/dev/null 2>&1 && systemctl list-unit-files --no-legend mcp-kali.service 2>/dev/null | grep -q '^mcp-kali.service'; then \
		systemctl disable --now mcp-kali.service; \
	fi
	rm -f "$(SYSTEM_UNIT_FILE)" "$(SYSTEM_BIN_DIR)/$(SERVER_BIN)" "$(SYSTEM_BIN_DIR)/$(CLIENT_BIN)"
	@if command -v systemctl >/dev/null 2>&1; then systemctl daemon-reload; fi
	rm -rf "$(SYSTEM_CONFIG_DIR)" "$(SYSTEM_DATA_DIR)" "$(SYSTEM_STATE_DIR)"

install-system: release
	@test "$$(id -u)" -eq 0 || { echo "install-system must run as root" >&2; exit 2; }
	@case "$(MCP_KALI_USER)" in ""|*[!A-Za-z0-9_.-]*) echo "MCP_KALI_USER must be a simple account name" >&2; exit 2;; esac
	@case "$(MCP_KALI_GROUP)" in ""|*[!A-Za-z0-9_.-]*) echo "MCP_KALI_GROUP must be a simple account name" >&2; exit 2;; esac
	@id -u "$(MCP_KALI_USER)" >/dev/null 2>&1 || { echo "service user $(MCP_KALI_USER) does not exist; create or select an authorized account" >&2; exit 2; }
	@getent group "$(MCP_KALI_GROUP)" >/dev/null 2>&1 || { echo "service group $(MCP_KALI_GROUP) does not exist" >&2; exit 2; }
	@service_home="$$(getent passwd "$(MCP_KALI_USER)" | awk -F: 'NR == 1 { print $$6 }')"; test -n "$$service_home" && test -d "$$service_home" || { echo "service user $(MCP_KALI_USER) has no usable home directory" >&2; exit 2; }
	install -d -m 0755 "$(SYSTEM_BIN_DIR)" "$(SYSTEM_PLUGIN_DIR)" "$(SYSTEM_CONFIG_DIR)/plugins" "$(SYSTEM_REFERENCE_OVERLAY_DIR)" "$(SYSTEMD_UNIT_DIR)"
	install -d -o "$(MCP_KALI_USER)" -g "$(MCP_KALI_GROUP)" -m 0700 "$(SYSTEM_STATE_DIR)"
	install -m 0755 "target/release/$(SERVER_BIN)" "$(SYSTEM_BIN_DIR)/$(SERVER_BIN)"
	install -m 0755 "target/release/$(CLIENT_BIN)" "$(SYSTEM_BIN_DIR)/$(CLIENT_BIN)"
	cp -R plugins/. "$(SYSTEM_PLUGIN_DIR)/"
	@test -e "$(SYSTEM_CONFIG_FILE)" || install -m 0644 "examples/mcp-kali.system.conf.example" "$(SYSTEM_CONFIG_FILE)"
	@service_home="$$(getent passwd "$(MCP_KALI_USER)" | awk -F: 'NR == 1 { print $$6 }')"; \
		sed -e 's|@MCP_KALI_USER@|$(MCP_KALI_USER)|g' -e 's|@MCP_KALI_GROUP@|$(MCP_KALI_GROUP)|g' -e 's|@MCP_KALI_HOME@|'"$$service_home"'|g' -e 's|@MCP_KALI_BIN@|$(SYSTEM_BIN_DIR)/$(SERVER_BIN)|g' -e 's|@MCP_KALI_CONFIG_FILE@|$(SYSTEM_CONFIG_FILE)|g' -e 's|@MCP_KALI_SYSTEM_DATA_DIR@|$(SYSTEM_DATA_DIR)|g' -e 's|@MCP_KALI_CONFIG_DIR@|$(SYSTEM_CONFIG_DIR)|g' "systemd/mcp-kali.service.in" > "$(SYSTEM_UNIT_FILE)"
	chmod 0644 "$(SYSTEM_UNIT_FILE)"
	@echo "Installed $(SYSTEM_UNIT_FILE). Run: make systemd-reload enable-system"

systemd-reload:
	systemctl daemon-reload

enable-system:
	systemctl enable --now mcp-kali.service

disable-system:
	systemctl disable --now mcp-kali.service

status-system:
	systemctl status mcp-kali.service

logs-system:
	journalctl -u mcp-kali.service -f

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
