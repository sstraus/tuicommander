---
id: 146-7fed
title: Serve frontend static files from embedded dist/
status: complete
priority: P1
created: "2026-02-15T23:38:45.007Z"
updated: "2026-02-16T00:03:32.092Z"
dependencies: ["145-5f38"]
---

# Serve frontend static files from embedded dist/

## Problem Statement

The axum server has no static file routes. Browser clients need the SolidJS app served via HTTP. Use include_dir to embed dist/ at compile time for single-binary distribution.

## Acceptance Criteria

- [ ] Add include_dir and mime_guess dependencies to Cargo.toml
- [ ] Embed dist/ directory at compile time with include_dir!
- [ ] Add routes: GET / serves index.html, GET /assets/* and GET /fonts/* serve static files with correct MIME types
- [ ] SPA fallback: unknown paths return index.html for client-side routing
- [ ] Static routes do NOT conflict with existing API routes (/sessions, /repo, etc.)
- [ ] CORS headers for browser clients

## Files

- src-tauri/src/mcp_http.rs
- src-tauri/Cargo.toml

## Work Log

### 2026-02-16T00:03:32.017Z - Embedded dist/ with include_dir, added static file serving (index.html, assets/*, fonts/*), SPA fallback for client-side routing, CORS layer, 6 new tests. All 103 Rust + 975 frontend tests pass.

