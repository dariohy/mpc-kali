---
apiVersion: mcp-kali/v1
kind: PluginReference
metadata:
  id: wpscan.safe-profiles
  title: Safe WPScan profiles
  description: Select bounded token-free WordPress fingerprinting and inventory profiles.
plugin: org.mcp-kali.wpscan
tags: [authorized-testing, inventory, wordpress, wpscan]
related_tools:
  - wpscan_exposure_scan
  - wpscan_passive_scan
  - wpscan_plugin_inventory
  - wpscan_rate_limited_scan
  - wpscan_theme_inventory
  - wpscan_user_enumeration
  - wpscan_web_scan
related_capabilities:
  - web.vulnerability_analysis
  - web.wordpress_inventory
---

# Safe WPScan profiles

Use WPScan only for an exact WordPress URL the operator is authorized to assess. The packaged profiles do not accept API tokens or target credentials and use the administrator-installed vulnerability database without updating it during a job.

## Tool selection

- `wpscan_web_scan` performs mixed detection and explicitly inventories popular plugins, popular themes, and TimThumb artifacts.
- `wpscan_passive_scan` uses passive core, plugin, and plugin-version detection with popular plugin/theme enumeration. It reduces probing but is not traffic-free.
- `wpscan_plugin_inventory` inventories popular plugins with mixed plugin and version detection.
- `wpscan_theme_inventory` inventories popular themes and version indicators.
- `wpscan_user_enumeration` checks only user IDs 1 through 10. It does not attempt authentication.
- `wpscan_exposure_scan` checks TimThumb artifacts, configuration backups, and database exports. Findings can expose highly sensitive data even though the tool does not launch a password attack.
- `wpscan_rate_limited_scan` uses mixed bounded inventory with a 750 ms delay, one effective worker, and longer connection/request timeouts.

All profiles suppress the banner, disable per-job database updates, and emit uncolored CLI output for stable capture.

## Input and data boundaries

The URL must use HTTP(S), cannot embed credentials, and cannot become another WPScan option. Profiles do not accept output files, cookie jars, custom local directories, passwords, wordlists, cookies, headers, or API tokens.

Without an API token, WPScan can inventory components and versions but cannot reliably annotate them with the current WPScan vulnerability database service. Do not describe token-free inventory as a complete vulnerability correlation.

Use Core `execute_command` only after reviewing the exact need and reading `wpscan.operator-boundaries`. API tokens, authenticated access, aggressive enumeration, forced scans, proxies, TLS bypass, custom paths, and password attacks are advanced workflows.
