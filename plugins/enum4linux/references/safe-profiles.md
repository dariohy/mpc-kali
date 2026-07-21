---
apiVersion: mcp-kali/v1
kind: PluginReference
metadata:
  id: enum4linux.safe-profiles
  title: Safe enum4linux profiles
  description: Select a bounded anonymous enum4linux profile for authorized SMB inventory.
plugin: org.mcp-kali.enum4linux
tags: [anonymous, authorized-testing, enum4linux, smb]
related_tools:
  - enum4linux_enumerate
  - enum4linux_groups
  - enum4linux_netbios_info
  - enum4linux_os_info
  - enum4linux_password_policy
  - enum4linux_printers
  - enum4linux_rid_users
  - enum4linux_shares
  - enum4linux_users
related_capabilities:
  - network.smb_enumeration
---

# Safe enum4linux profiles

Use these tools only for a host the operator is authorized to assess. Each profile accepts one hostname or IP address, uses the default anonymous session, runs without a shell, and does not accept arbitrary enum4linux options.

## Tool selection

- `enum4linux_enumerate` runs the broad `-a` baseline: users, shares, groups, password policy, default RID cycling, SMB OS information, NetBIOS names, and printers. It creates substantially more traffic than a focused query.
- `enum4linux_users` requests the directly enumerable user list with `-U`.
- `enum4linux_groups` requests built-in, local, and domain groups and attempts to resolve their members with `-G`.
- `enum4linux_shares` lists shares with `-S` and reports whether the anonymous session can map and list them. It does not guess hidden share names or request write checks.
- `enum4linux_password_policy` requests password and lockout policy with `-P`. The installed enum4linux package must include its `polenum` dependency.
- `enum4linux_os_info` requests SMB-advertised operating-system and server hints with `-o`.
- `enum4linux_netbios_info` queries the NetBIOS name table with `-n`; UDP port 137 must be reachable.
- `enum4linux_printers` requests printer inventory over RPC with `-i`.
- `enum4linux_rid_users` uses `-r` and enum4linux's default finite RID ranges. It is a deliberate follow-up when direct user listing is blocked and produces multiple RPC lookups.

Prefer the narrowest profile that answers the question. Start with SMB service confirmation, then run focused queries. Use the broad baseline when the operator explicitly wants the combined inventory.

## Input and execution boundaries

`target` is one hostname or IP address, not a CIDR, address range, shell expression, or option. It cannot begin with `-`. The profiles do not accept credentials, workgroup overrides, wordlists, custom RID ranges, verbose command disclosure, LDAP options, or aggressive mode.

enum4linux is a wrapper around local Samba utilities. A profile can be available while an individual underlying query still fails because a dependency is missing, a port is filtered, or anonymous access is denied. Such a failure is a result to interpret, not permission to broaden the operation.

## Advanced cases

Use Core `execute_command` only after the operator asks for behavior outside these profiles and the exact argument vector has been reviewed. Supply `program: enum4linux` and separate arguments without a shell. Read `enum4linux.operator-boundaries` before handling credentials, extended RID enumeration, wordlists, LDAP, or write checks.
