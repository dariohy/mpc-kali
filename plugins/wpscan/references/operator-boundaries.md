---
apiVersion: mcp-kali/v1
kind: PluginReference
metadata:
  id: wpscan.operator-boundaries
  title: WPScan operator boundaries
  description: Authorization, API-token, credential, aggressive-enumeration, and password-attack boundaries for WPScan.
plugin: org.mcp-kali.wpscan
tags: [authorization, credentials, guardrails, wordpress, wpscan]
related_tools:
  - wpscan_exposure_scan
  - wpscan_passive_scan
  - wpscan_plugin_inventory
  - wpscan_rate_limited_scan
  - wpscan_theme_inventory
  - wpscan_user_enumeration
  - wpscan_web_scan
related_capabilities:
  - web.vulnerability_analysis
  - web.wordpress_inventory
---

# WPScan operator boundaries

Confirm the WordPress origin and base path, allowed enumeration types, request rate, monitoring window, and whether exposed backup or user discovery is in scope. Local non-root execution does not establish remote permission.

## Declarative boundary

The packaged tools exclude:

- WPScan API tokens and vulnerability-service queries;
- HTTP/proxy authentication, cookies, and custom user agents;
- password lists, usernames supplied for login attempts, and all password-attack modes;
- aggressive plugin/theme enumeration and arbitrary user/media ranges;
- `--force` bypass of WordPress/403 prechecks;
- TLS verification bypass, proxies, and custom content/plugin paths;
- custom login URIs, exclusion regular expressions, and arbitrary thread/throttle values;
- output files and cookie-jar paths; and
- database updates during a scheduled job.

Use a reviewed Core `execute_command` argument vector only when the operator explicitly authorizes an advanced behavior.

## Tokens and credentials

`--api-token`, `--http-auth`, `--proxy-auth`, cookies, and password-attack values appear in child arguments and may be visible through process inspection and job telemetry. Do not store them in Plugin definitions or reference documents. Explain the exposure and follow deployment secret policy before use.

The WPScan API token authorizes a rate-limited external service call and may associate the lookup with an account. Confirm quota and policy. Consider a controlled runtime secret mechanism instead of ad hoc command arguments before making token-backed scanning routine.

## High-volume and active behavior

Aggressive plugin enumeration can request thousands of known paths. `--force` bypasses WPScan's initial target checks and can waste or misdirect traffic. User enumeration discloses account identifiers; the packaged profile limits it to IDs 1–10.

Password attacks attempt real authentication, can lock out users, and may trigger incident response. They always require separate explicit authorization, a reviewed account set and wordlist, rate/lockout safeguards, and coordination with the system owner.

TLS bypass removes endpoint verification. Proxies change routing and can retain sensitive traffic. Custom WordPress paths change scan reach. Each is a distinct operator decision.
