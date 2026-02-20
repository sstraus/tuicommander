---
id: 275-40bc
title: Production eprintln debug in output_parser hot path
status: complete
priority: P1
created: "2026-02-20T13:56:35.898Z"
updated: "2026-02-20T13:58:37.148Z"
dependencies: []
---

# Production eprintln debug in output_parser hot path

## Problem Statement

RateLimit DEBUG prints to stderr on every rate-limit match in the PTY reader thread at output_parser.rs:105. Events already emitted as ParsedEvent::RateLimit.

## Acceptance Criteria

- [ ] eprintln removed from output_parser.rs:105

## Files

- src-tauri/src/output_parser.rs

## Work Log

### 2026-02-20T13:58:37.079Z - Removed eprintln debug statement from parse_rate_limit hot path

