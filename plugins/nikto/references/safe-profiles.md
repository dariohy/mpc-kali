---
apiVersion: mcp-kali/v1
kind: PluginReference
metadata:
  id: nikto.safe-profiles
  title: Safe Nikto profiles
  description: Select a bounded non-destructive Nikto profile for an authorized web server.
plugin: org.mcp-kali.nikto
tags: [authorized-testing, nikto, non-destructive, web-server]
related_tools:
  - nikto_configuration_scan
  - nikto_https_scan
  - nikto_rate_limited_scan
  - nikto_software_scan
  - nikto_vhost_scan
  - nikto_web_scan
related_capabilities:
  - web.vulnerability_analysis
---

# Safe Nikto profiles

Nikto performs active web-server testing and is deliberately visible in access logs, monitoring systems, IDS, and WAF telemetry. Use these tools only for an exact HTTP(S) URL the operator is authorized to assess.

## Tool selection

- `nikto_web_scan` provides broad coverage while excluding tuning categories `6`, `0`, `8`, `9`, and `c`: denial of service, file upload, command execution, SQL injection, and remote source inclusion.
- `nikto_configuration_scan` selects categories `2`, `3`, and `b` for misconfiguration/default files, information disclosure, and software identification. Prefer it when broad active coverage is unnecessary.
- `nikto_software_scan` selects only category `b` to collect software identification and version indicators.
- `nikto_https_scan` requires an explicit `https://` URL, supports a port in that URL, and forces Nikto's TLS mode.
- `nikto_vhost_scan` connects to `target` while sending the explicit `vhost` value in the HTTP `Host` header. Both the connection endpoint and virtual host must be in scope.
- `nikto_rate_limited_scan` uses the broad non-destructive tuning with a one-second pause between tests and a twenty-minute per-host Nikto limit. It reduces request rate but remains active and noisy.

Every profile adds `-nointeractive`, `-ask no`, and `-nocheck`. This prevents an unattended job from prompting or checking for database updates and makes it use the administrator-installed Nikto databases.

## Input boundaries

`target` must be one explicit `http://` or `https://` URL. A bare hostname, local path, and target-list file are rejected so Nikto cannot reinterpret the value as a batch input. User information is not accepted in the URL authority. The URL cannot contain whitespace or become another Nikto option.

`vhost` is a hostname without a scheme, path, header syntax, whitespace, or arbitrary characters. The Plugin invokes Nikto directly with separate arguments and does not use a shell.

The scheduler captures stdout and stderr. Declarative profiles do not accept Nikto output or response-save paths, so they cannot write arbitrary report or evidence files.

## Coverage tradeoffs

Non-destructive tuning reduces risk; it does not make Nikto passive or quiet. A scan can still send hundreds or thousands of requests, stress a fragile application, trigger rate limits, create application records, or exercise unexpected server behavior.

Do not describe an incomplete or time-boxed run as full coverage. Use the focused profiles when they answer the operator's question and the rate-limited profile when coordinated production testing needs a gentler request rate.

For behavior outside these profiles, use Core `execute_command` only after reviewing the exact target, options, local paths, credentials, traffic effects, and authorization. Read `nikto.operator-boundaries` first.
