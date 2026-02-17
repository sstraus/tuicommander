---
id: "085-bfe0"
title: "Add ANSI stripping in Rust via strip-ansi-escapes crate"
status: pending
priority: P2
created: 2026-02-08T10:18:04.004Z
updated: 2026-02-08T10:18:04.004Z
dependencies: []
---

# Add ANSI stripping in Rust via strip-ansi-escapes crate

## Problem Statement

Status line detection requires stripping ANSI escape sequences before pattern matching. Currently done in JS via regex. The strip-ansi-escapes crate provides a well-tested Rust implementation that integrates with the OutputParser.

## Acceptance Criteria

- [ ] Add strip-ansi-escapes to Cargo.toml
- [ ] Use in OutputParser for status line detection
- [ ] No ANSI stripping regex in JavaScript
- [ ] Correctly handles all ANSI escape types (CSI, OSC, SGR)

## Files

- src-tauri/Cargo.toml
- src-tauri/src/output_parser.rs

## Related

- 084

## Work Log

