# Security policy

## Supported versions

| Version | Supported |
|---|---:|
| 1.1.x | Yes |
| 1.0.x | Yes |
| Pre-1.0 development snapshots | No |

## Reporting a vulnerability

Do not open a public issue containing exploit details, credentials, customer
data, or pentest evidence. Use GitHub's private security-advisory feature for
[`dariohy/mcp-kali`](https://github.com/dariohy/mcp-kali/security/advisories/new)
when available, or contact the repository owner through a previously
established private channel.

Include:

- affected version and platform;
- affected client/server configuration;
- minimal reproduction using synthetic data;
- expected and actual security boundary;
- impact and required attacker access; and
- any suggested mitigation.

Never include real tokens, passwords, targets, raw job state, or engagement
output. Maintainers should acknowledge reports privately, reproduce them in an
isolated environment, assess affected versions, and coordinate remediation and
disclosure.

## Version 1.x security boundary

- MCP Kali is an authorized-testing tool and an API-accessible command runner.
- The HTTP server has no built-in authentication or TLS. It defaults to
  loopback, refuses accidental remote binding, and should normally be reached
  through SSH or another protected private transport.
- By default, anyone with API access can invoke the privileged Core Plugin to
  submit executables available to the server account and read job
  metadata/output. Disable `execute_command` when free execution is unnecessary.
- Plugin YAML and the Capability Catalog are trusted local configuration.
  Protect the packaged data and administrator overlay from unauthorized writes;
  declarative validation is a safety boundary, not a provenance mechanism.
- Scanner output is hostile input. The dashboard escapes it, and MCP results
  classify it as untrusted data, but host-agent policy must independently resist
  prompt injection.
- Private state contains full argv and raw output even when public command
  redaction is enabled.
- Webhooks are unsigned and best-effort.

The complete threat model and operational controls are documented in
[docs/USER_MANUAL.md](docs/USER_MANUAL.md#14-security-model).

## Secure release checks

Before distributing a release:

```bash
make verify
make security
make checksum
make sbom
```

Review dependency advisories, license/source policy, secret-scan findings,
checksums, and the SBOM. Optional tool absence must be reported rather than
silently skipped.
