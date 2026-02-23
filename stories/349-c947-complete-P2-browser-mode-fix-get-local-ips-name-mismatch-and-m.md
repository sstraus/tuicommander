---
id: 349-c947
title: "Browser mode: fix get_local_ips name mismatch and missing route"
status: complete
priority: P2
created: "2026-02-21T20:34:55.186Z"
updated: "2026-02-23T08:02:00.004Z"
dependencies: []
---

# Browser mode: fix get_local_ips name mismatch and missing route

## Problem Statement

ServicesTab.tsx calls invoke("get_local_ips") (plural) but transport.ts only maps get_local_ip (singular). Additionally the /system/local-ip route is missing from the Rust HTTP router (mod.rs). ServicesTab crashes in browser mode trying to show the QR code IP addresses.

## Acceptance Criteria

- [ ] Add /system/local-ip route to Rust HTTP router in mod.rs
- [ ] Reconcile get_local_ips vs get_local_ip naming: either update transport.ts to handle get_local_ips or update ServicesTab to call get_local_ip
- [ ] ServicesTab QR code IP display works in browser mode

## Files

- src/components/ServicesTab/ServicesTab.tsx
- src/transport.ts
- src-tauri/src/mcp_http/mod.rs

## Work Log

### 2026-02-23T07:52:04.416Z - Triaged: implement now

### 2026-02-23T08:02:00.083Z - Fixed get_local_ips name, added HTTP route with IPv6 support

