---
apiVersion: mcp-kali/v1
kind: PluginReference
metadata:
  id: gobuster.operator-boundaries
  title: Gobuster operator boundaries
  description: Authorization, wordlist, credential, scope-expansion, and traffic boundaries for Gobuster.
plugin: org.mcp-kali.gobuster
tags: [authorization, gobuster, guardrails, scope, wordlists]
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

# Gobuster operator boundaries

Authorization must cover the endpoint or DNS namespace, the guessing technique, request volume, timing, and candidate list. Gobuster's local privilege level does not determine remote permission.

## Declarative boundary

The packaged tools exclude:

- stdin, relative wordlists, pattern files, and arbitrary output paths;
- custom thread, timeout, delay, status, length, or wildcard behavior;
- redirects, which may move requests outside the original origin;
- TLS verification bypass;
- usernames, passwords, cookies, authorization headers, and custom user agents;
- proxies and custom DNS resolvers;
- recursive automation of discovered paths;
- TFTP filename enumeration; and
- S3 and GCS bucket-name enumeration.

These are not interchangeable flags. Gobuster option meanings depend on the selected mode; notably `-r` follows redirects in `dir` mode but selects a resolver in `dns` mode. Always review mode-specific help and use separate argv arguments without a shell.

## Wordlists and traffic

Request volume is approximately the number of candidates, multiplied by extensions or patterns and by any repeated follow-up passes. Inspect wordlist line count and content before use. High thread counts can overload applications, resolvers, proxies, and monitoring pipelines. Rate limiting reduces throughput but does not make brute-force traffic passive or invisible.

Do not automatically recurse into discovered paths. Gobuster has no native recursion, and scripting follow-ups can produce explosive traffic and leave the authorized base path.

## Credentials and routing

Passwords, cookies, bearer tokens, and custom headers appear in child arguments and may be captured in job telemetry or process inspection. Proxies change the network route and may store every request. Obtain explicit approval for the identity and route, and follow the deployment's secret policy.

Redirect following can reach a different hostname. TLS bypass removes endpoint verification. Custom resolvers disclose candidate names to the selected DNS service. Each is a separate operator decision.

Cloud bucket and TFTP modes address infrastructure beyond the supplied web origin. Confirm ownership and provider/service scope before using them through a reviewed advanced command.
