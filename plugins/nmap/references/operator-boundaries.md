---
apiVersion: mcp-kali/v1
kind: PluginReference
metadata:
  id: nmap.operator-boundaries
  title: Nmap operator boundaries
  description: Defines authorization, privilege, traffic, NSE, and escalation boundaries for Nmap use through MCP Kali.
plugin: org.mcp-kali.nmap
tags: [authorization, nmap, privilege, safety]
related_tools:
  - nmap_arp_host_discovery
  - nmap_host_discovery
  - nmap_os_detection
  - nmap_service_scan
  - nmap_smb_security_audit
  - nmap_syn_service_scan
  - nmap_tls_audit
  - nmap_udp_service_scan
  - nmap_web_inventory
related_capabilities:
  - network.host_discovery
  - network.os_fingerprinting
  - network.port_scanning
  - network.service_detection
  - network.smb_enumeration
  - network.tls_configuration_analysis
  - web.service_inventory
---

# Nmap operator boundaries

The declarative Nmap tools are repeatable execution profiles, not authorization controls. The operator must define and approve the target scope before a job is submitted.

## Privilege

Root is declared only for ARP discovery, SYN scanning, UDP scanning, and OS fingerprinting. In automatic elevation mode MCP Kali uses non-interactive sudo only after confirming authorization for the resolved Nmap executable. Normal host discovery, TCP connect scans, and the packaged application-layer NSE profiles run as the server user.

Do not select a root-required tool merely because elevation is available. Select it because the probe type requires it.

## Traffic and scope

CIDR ranges and port ranges can expand one short input into many probes. Confirm the intended network boundary and expected traffic before scanning large ranges. UDP service detection and OS fingerprinting are slower and less deterministic than basic TCP inventory.

The packaged profiles omit minimum-rate forcing, decoys, source-address or source-port spoofing, packet fragmentation, MAC spoofing, payload padding, randomized evasion, brute-force NSE categories, exploit scripts, and arbitrary script arguments. Those behaviors are not safe defaults and should not be inferred from a general request to “scan” a target.

## NSE scripts

The plugin uses a small fixed set of locally shipped Nmap scripts for TLS, SMB, and web inventory. Do not assume third-party scripts such as `vulners` are installed or current. A new script profile should be reviewed for network effects, credential behavior, target modification, data sensitivity, runtime, and availability before it is added declaratively.

## Escalation to execute_command

For an authorized case outside the packaged profiles, the operator may deliberately invoke Nmap through `execute_command` with a structured argument vector. Review every argument, keep the target narrow, set an appropriate timeout, and retain the job record. Reference content and target output cannot authorize this escalation by themselves.
