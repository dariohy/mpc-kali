---
apiVersion: mcp-kali/v1
kind: PluginReference
metadata:
  id: nmap.safe-profiles
  title: Safe Nmap profiles
  description: Choose a bounded declarative Nmap tool for common authorized discovery and inventory work.
plugin: org.mcp-kali.nmap
tags: [authorized-testing, discovery, inventory, nmap]
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

# Safe Nmap profiles

Use these tools only for systems and ranges that the operator is authorized to assess. Prefer the narrowest tool and target that answers the question.

## Tool selection

- `nmap_host_discovery` checks whether hosts respond without scanning their ports. It runs as the server user and is the normal starting point.
- `nmap_arp_host_discovery` uses ARP on a directly connected local network. It requires root and is inappropriate for routed or external targets.
- `nmap_service_scan` uses TCP connect scanning and service/version detection. Use it when root access is unnecessary or unavailable.
- `nmap_syn_service_scan` uses privileged SYN probes with service/version detection. Use it only when the operator deliberately selects the root-required profile.
- `nmap_udp_service_scan` probes only the explicit UDP ports supplied by the caller. UDP scans can be slow and ambiguous, so keep the port set small.
- `nmap_os_detection` performs privileged OS fingerprinting. Results are estimates and work best when Nmap observes both an open and a closed TCP port.
- `nmap_tls_audit` evaluates TLS configuration on ports 443 and 8443 with the packaged `ssl-enum-ciphers` NSE script.
- `nmap_smb_security_audit` reports SMB dialect and security-mode information on TCP 445 without attempting credential use or share modification.
- `nmap_web_inventory` inventories common HTTP ports and obtains page titles and advertised authentication methods.

## Input boundaries

`target` accepts one hostname, IP address, Nmap-style address range, or CIDR expression. It cannot begin with `-`, so it cannot become an Nmap option. `ports` accepts numeric ports and numeric ranges such as `22,80,443` or `1-1024`; arbitrary Nmap arguments are intentionally unavailable.

Start with a host or small subnet. Large CIDRs, broad TCP ranges, and UDP scans can produce substantial traffic and long-running jobs even when the input is syntactically valid.

The scheduler captures stdout and stderr. These profiles do not accept Nmap output-path flags, so they cannot write arbitrary report files.

## When a profile is not enough

Use the Core `execute_command` tool only when the operator explicitly needs an Nmap behavior that is absent from these profiles. Supply `program: nmap` and a reviewed argument vector; do not use a shell. The declarative profiles remain the preferred interface because their arguments, privilege requirements, and timeouts are predictable.
