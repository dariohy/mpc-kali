---
apiVersion: mcp-kali/v1
kind: PluginReference
metadata:
  id: dnsrecon.safe-profiles
  title: Safe DNSRecon profiles
  description: Select bounded DNS record, subdomain, transfer, reverse, CT, and DNSSEC discovery profiles.
plugin: org.mcp-kali.dnsrecon
tags: [authorized-testing, dns, dnsrecon, reconnaissance]
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

# Safe DNSRecon profiles

Use DNSRecon only for domains and address ranges the operator is authorized to enumerate. DNS queries are observable, and some profiles can produce significant resolver or authoritative-server traffic.

## Tool selection

- `dnsrecon_standard_enumeration` collects the normal SOA, NS, address, MX, and common service record footprint with ten workers and a five-second response lifetime.
- `dnsrecon_srv_enumeration` focuses on common SRV service records.
- `dnsrecon_zone_transfer_check` tests each authoritative server for AXFR exposure over TCP. It does not combine the transfer attempt with other enumeration.
- `dnsrecon_certificate_transparency` queries crt.sh for certificate names under the domain. This contacts a third-party service and depends on its availability and terms.
- `dnsrecon_subdomain_bruteforce` resolves an absolute wordlist beneath the domain with ten workers and filters results matching the wildcard address.
- `dnsrecon_reverse_lookup` performs PTR queries only within an explicit IPv4 CIDR from `/24` through `/32`.
- `dnsrecon_dnssec_zonewalk` attempts an NSEC walk over TCP. NSEC3 zones normally resist straightforward walking.

Start with standard enumeration, then choose one focused follow-up based on the operator's question. Do not automatically run every profile.

## Input boundaries

`domain` is one bounded DNS name and cannot become an option. Reverse ranges are IPv4 CIDRs no broader than `/24`; the operator must still verify that every address is in scope. Wordlists must use absolute paths and cannot be stdin or an option.

Before brute forcing, verify that the wordlist is a regular authorized file, not an unexpected symlink, and appropriately sized. Query volume is approximately the number of candidates plus DNSRecon's wildcard and safety checks.

Profiles capture output through the scheduler and do not accept XML, JSON, CSV, SQLite, log, domain-list, or other local output/input paths.

Read `dnsrecon.operator-boundaries` before using custom resolvers, passive search providers, WHOIS/SPF pivots, cache snooping, TLD expansion, wildcard override, bulk targets, or arbitrary ranges.
