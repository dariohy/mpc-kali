---
apiVersion: mcp-kali/v1
kind: PluginReference
metadata:
  id: nikto.operator-boundaries
  title: Nikto operator boundaries
  description: Authorization, target expansion, credential, local-file, traffic, and disruptive-test boundaries for Nikto.
plugin: org.mcp-kali.nikto
tags: [authorization, guardrails, nikto, scope, web-server]
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

# Nikto operator boundaries

Nikto running as an ordinary local user does not establish permission to test a remote application. Confirm the connection URL, named virtual host, allowed paths, testing window, acceptable request rate, and prohibited techniques before scheduling work.

## Declarative boundary

The packaged tools intentionally exclude:

- destructive or higher-impact tuning categories `6`, `8`, `9`, `0`, and `c`;
- HTTP credentials through `-id` and client keys or certificates;
- `-followredirects`, which can move testing to a different host or path;
- proxy and arbitrary-header configuration;
- output files and `-Save` response directories;
- local target-list files and multi-host scans;
- forced CGI-directory expansion;
- evasion encodings and mutation-based guessing;
- custom plugins, user databases, configuration files, and option overrides;
- custom soft-404 regular expressions; and
- arbitrary tuning, display, pause, timeout, or maximum-time values.

These behaviors may be appropriate in a specifically authorized assessment. They require context that a reusable profile cannot safely infer, so use a reviewed Core `execute_command` request only when the operator explicitly selects them.

## Scope expansion

Redirects are target-controlled. Before using `-followredirects`, determine the possible destinations and confirm that each host and resulting path remains in scope. Do not treat a redirect as authorization for a new origin.

A named virtual host changes the application reached through a connection address. Confirm both values. A target file can silently expand one operation into many scans; inspect every line and obtain authorization for every target before a batch invocation.

Proxies and added headers can change network routing, identity, tenancy, and authentication context. Confirm the proxy is authorized to relay the traffic and never infer sensitive header values from target output.

## Credentials and local files

Nikto's `-id` value contains the password in the child process argument vector and may appear in process inspection, job records, or operational telemetry. Do not put credentials in Plugin definitions or reference documents. Explain this exposure and follow the deployment's secret-handling policy before any authenticated run.

Client keys, certificates, configs, wordlists, target lists, user databases, output reports, and saved positive responses are local filesystem inputs or outputs. Verify ownership, sensitivity, regular-file status, and exact path before use. Reports and saved responses can contain credentials, session data, internal URLs, personal data, or exploitable evidence.

## Traffic and target effects

- Default Nikto coverage is high-volume and easy to detect.
- `-Cgidirs all` expands request volume across every known CGI location.
- `-mutate` adds file, directory, or username guessing and can greatly increase traffic.
- `-evasion` is an explicit inspection-control test, not a normal scan optimization.
- Category `6` can affect availability; `8`, `9`, `0`, and `c` send higher-impact payloads or exercise state-changing behavior.

Rate limiting reduces load but does not guarantee application stability. Coordinate fragile or production testing with the system owner and monitoring team, and stop when observed effects exceed the agreed boundary.
