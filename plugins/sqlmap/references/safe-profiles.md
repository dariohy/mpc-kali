---
apiVersion: mcp-kali/v1
kind: PluginReference
metadata:
  id: sqlmap.safe-profiles
  title: Safe SQLmap profiles
  description: Select bounded low-risk SQL injection detection and metadata profiles.
plugin: org.mcp-kali.sqlmap
tags: [authorized-testing, low-risk, sql-injection, sqlmap]
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

# Safe SQLmap profiles

SQLmap actively injects payloads. Even these bounded profiles require explicit authorization for the exact application, URL, request method, and parameter.

## Tool selection

- `sqlmap_parameter_test` performs a first-pass test of URL parameters and, when supplied, a bounded POST body.
- `sqlmap_get_parameter_test` tests only one named URL parameter. Prefer it when the suspected input is known.
- `sqlmap_post_parameter_test` supplies a bounded POST body and tests only one named body parameter.
- `sqlmap_database_context` obtains the DBMS banner, current database, current user, and DBA status after injection is confirmed.
- `sqlmap_database_inventory` enumerates database names but does not retrieve tables or rows.
- `sqlmap_table_inventory` enumerates table names only within one explicitly named database.

Every profile is non-interactive and fixes `--level=1`, `--risk=1`, and `--technique=BEUTQ`. The technique set excludes `S` stacked queries. Profiles do not dump rows, enumerate password hashes, or access the server filesystem or operating system.

## Input boundaries

URLs must use `http://` or `https://`, cannot contain embedded credentials, and cannot become sqlmap options. POST data is one argument, limited to 4096 characters, and cannot contain CR or LF. Parameter and database names use bounded identifier character sets.

POST bodies can still contain sensitive application data. Minimize the body, use non-production values where possible, and remember that submitted arguments may appear in job records and process inspection.

## Progression

Start with a named GET or POST parameter at the default profile. If injection is confirmed, use database context before deciding whether database-name or table-name inventory is necessary. Stop when the evidence answers the operator's question.

Use Core `execute_command` only for a reviewed advanced workflow. Read `sqlmap.operator-boundaries` before using sessions, cookies, captured requests, crawling, higher level/risk, tamper scripts, extraction, or host access.
