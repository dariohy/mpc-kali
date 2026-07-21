---
apiVersion: mcp-kali/v1
kind: PluginReference
metadata:
  id: dnsrecon.operator-boundaries
  title: DNSRecon operator boundaries
  description: Authorization, third-party, resolver, scope-expansion, wordlist, and cache-snooping boundaries for DNSRecon.
plugin: org.mcp-kali.dnsrecon
tags: [authorization, dns, dnsrecon, guardrails, scope]
related_tools:
  - dnsrecon_certificate_transparency
  - dnsrecon_dnssec_zonewalk
  - dnsrecon_reverse_lookup
  - dnsrecon_srv_enumeration
  - dnsrecon_standard_enumeration
  - dnsrecon_subdomain_bruteforce
  - dnsrecon_zone_transfer_check
related_capabilities:
  - network.dns_configuration_analysis
  - network.dns_discovery
---

# DNSRecon operator boundaries

Local non-root execution does not establish permission to query a domain, resolver, or address range. Confirm authoritative domain scope, permitted third parties, address-space scope, wordlist size, and acceptable query rate.

## Declarative boundary

The packaged tools exclude:

- domain-list files and multiple-target runs;
- custom name servers, recursion/NXDOMAIN/BIND-version check overrides, and arbitrary TCP behavior;
- arbitrary thread counts, lifetimes, address ranges, and first-to-last ranges;
- continuing brute force through wildcard DNS with `--iw`;
- Bing and Yandex scraping;
- deep WHOIS analysis and SPF-derived reverse-range pivots;
- cache snooping against a named resolver;
- TLD expansion across unrelated registered domains; and
- XML, JSON, CSV, SQLite, and custom log output paths.

Use Core `execute_command` only when the operator explicitly selects an advanced workflow and the exact argument vector, local files, resolver, external service, and expanded scope have been reviewed.

## Scope expansion and third parties

WHOIS and SPF data can identify ranges operated by hosting providers, mail services, or other parties. Discovery does not make those ranges authorized targets. TLD expansion tests sibling registrations outside the original DNS zone. Search-engine and CT profiles contact external services whose results, quotas, and terms can change.

The packaged CT profile is explicit so the operator can decide whether contacting crt.sh is acceptable. Do not silently add Bing, Yandex, WHOIS, or other providers.

## Resolver behavior and privacy

Custom resolvers observe candidate names. DNS cache snooping targets resolver state and may infer user or organizational activity; it requires a specific privacy and authorization decision. Disabling DNSRecon's NXDOMAIN, recursion, or version checks can change false-positive and configuration results and should not be inferred from an error.

## Query volume

Brute force scales with wordlist size. Reverse lookup scales with addresses. Zone walks scale with exposed NSEC names, and AXFR success can return the complete zone. Rate limits, monitoring, and authoritative-server load still apply to read-only DNS traffic.
