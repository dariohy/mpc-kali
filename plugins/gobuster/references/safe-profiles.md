---
apiVersion: mcp-kali/v1
kind: PluginReference
metadata:
  id: gobuster.safe-profiles
  title: Safe Gobuster profiles
  description: Select a mode-correct bounded Gobuster discovery profile.
plugin: org.mcp-kali.gobuster
tags: [authorized-testing, content-discovery, gobuster, wordlists]
related_tools:
  - gobuster_content_discovery
  - gobuster_dns_discovery
  - gobuster_extension_discovery
  - gobuster_fuzz_discovery
  - gobuster_rate_limited_discovery
  - gobuster_vhost_discovery
related_capabilities:
  - network.dns_discovery
  - web.content_discovery
  - web.virtual_host_discovery
---

# Safe Gobuster profiles

Gobuster performs wordlist-driven guessing and can generate substantial HTTP or DNS traffic. Confirm the exact URL/domain and wordlist scope before starting.

## Tool selection

- `gobuster_content_discovery` runs one non-recursive `dir` pass with ten workers.
- `gobuster_extension_discovery` adds up to ten bounded file extensions. Each extension multiplies the approximate request volume.
- `gobuster_dns_discovery` resolves wordlist labels beneath one base domain and reports IP addresses.
- `gobuster_vhost_discovery` connects to one HTTP(S) endpoint and appends the explicit base domain to wordlist labels for Host-header discovery.
- `gobuster_fuzz_discovery` requires one or more literal `FUZZ` markers in the URL path or query and substitutes wordlist entries there.
- `gobuster_rate_limited_discovery` performs one `dir` pass with five workers and a 200 ms delay.

Profiles suppress progress and color so captured output is stable. Gobuster does not recurse; a discovered directory is evidence for a separately authorized follow-up, not permission to automatically launch another scan.

## Inputs

HTTP targets require an explicit credential-free HTTP(S) URL. DNS and vhost base domains use bounded hostname syntax. Wordlists must be absolute local paths and cannot be stdin or an option. Extension values are short alphanumeric suffixes only.

Before invocation, the operator must ensure the wordlist is a regular authorized file, is not an unexpected symlink, has an appropriate size, and contains candidates relevant to the scoped environment. A syntactically valid path is not proof that the file is safe or intended.

## Advanced cases

Status/length filters, custom resolvers, redirects, self-signed TLS bypass, custom headers, cookies, credentials, proxies, output files, cloud buckets, TFTP, wildcard forcing, and manual multi-level discovery require additional context. Use Core `execute_command` only with a reviewed mode-specific argument vector after reading `gobuster.operator-boundaries`.
