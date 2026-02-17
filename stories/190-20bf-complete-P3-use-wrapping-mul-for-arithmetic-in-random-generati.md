---
id: 190-20bf
title: Use wrapping_mul for arithmetic in random generation
status: complete
priority: P3
created: "2026-02-16T07:17:10.816Z"
updated: "2026-02-16T07:38:23.542Z"
dependencies: []
---

# Use wrapping_mul for arithmetic in random generation

## Problem Statement

seed.wrapping_add(attempt * 7) uses regular multiplication which can overflow in debug mode. Should use wrapping_mul for consistency.

## Acceptance Criteria

- [ ] Change attempt * 7 to attempt.wrapping_mul(7)

## Files

- src-tauri/src/lib.rs

## Related

- RS-08

## Work Log

### 2026-02-16T07:38:23.478Z - Changed attempt * 7/13/31 to attempt.wrapping_mul(7/13/31) in random name generation.

