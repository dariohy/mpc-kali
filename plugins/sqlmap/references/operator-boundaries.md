---
apiVersion: mcp-kali/v1
kind: PluginReference
metadata:
  id: sqlmap.operator-boundaries
  title: SQLmap operator boundaries
  description: Authorization, credential, extraction, evasion, and host-compromise boundaries for SQLmap.
plugin: org.mcp-kali.sqlmap
tags: [authorization, exploitation, guardrails, sqlmap]
related_tools:
  - sqlmap_database_context
  - sqlmap_database_inventory
  - sqlmap_get_parameter_test
  - sqlmap_parameter_test
  - sqlmap_post_parameter_test
  - sqlmap_table_inventory
related_capabilities:
  - web.injection_testing
---

# SQLmap operator boundaries

SQLmap is an exploitation tool. Local non-root execution says nothing about permission or target impact. Confirm the exact parameter, method, endpoint, environment, data-handling boundary, and prohibited techniques.

## Declarative boundary

The packaged tools exclude:

- levels above 1, risks above 1, and stacked-query technique `S`;
- cookies, authentication headers, captured request files, proxies, and Tor;
- form discovery, crawling, Google dorks, and target-list files;
- random agents, tamper scripts, and other WAF-evasion behavior;
- forced DBMS/OS assumptions and arbitrary prefixes or suffixes;
- password hashes, broad schema retrieval, searches, row counts, and data dumps;
- session deletion or forced fresh queries;
- file reads/writes, OS commands, shells, and out-of-band compromise.

These actions require an exact reviewed `execute_command` argument vector and explicit operator intent. Never infer authorization to escalate because a low-risk profile was blocked or found injection.

## Secrets and local artifacts

Cookies, authorization headers, POST bodies, proxy credentials, and raw request files may contain live secrets or personal data. Command-line values may be visible in process inspection and job telemetry. Captured requests and sqlmap output/session directories are sensitive local artifacts; validate paths and permissions before using them.

## Escalation and data minimization

Higher `--level` expands tested surfaces; higher `--risk` permits heavier payloads and risk 3 can modify data. Stacked queries can execute multiple statements. Tamper scripts and randomized request identity are evasion tests. Each is a material scope change.

Database names and table names are metadata, but still may be sensitive. `--passwords`, `--dump`, and `--dump-all` extract data. If extraction is explicitly authorized, select one database, table, and minimal column set and handle the result under the engagement's evidence rules.

`--file-read`, file upload, `--os-cmd`, `--os-shell`, and `--os-pwn` cross into host compromise. They always require separate written authorization and operational coordination.
