---
id: "089-2f40"
title: "Evaluate Tauri Channels API for PTY output streaming"
status: pending
priority: P3
created: 2026-02-08T10:18:04.007Z
updated: 2026-02-08T10:18:04.007Z
dependencies: []
---

# Evaluate Tauri Channels API for PTY output streaming

## Problem Statement

Tauri event system was not designed for high-throughput streaming. The Channels API is purpose-built for ordered data delivery and may offer better performance for PTY output at scale. Need to benchmark before committing to migration.

## Acceptance Criteria

- [ ] Benchmark current event-based PTY output: measure latency and throughput for single session
- [ ] Implement prototype using Tauri Channel for one session
- [ ] Benchmark Channel-based PTY output: same latency and throughput metrics
- [ ] Document results with numbers
- [ ] If Channels are measurably better, create follow-up story to migrate all sessions
- [ ] If no significant difference, document finding and close story

## Files

- src-tauri/src/lib.rs
- src/components/Terminal/Terminal.tsx

## Work Log

