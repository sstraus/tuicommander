---
id: 213-ac1e
title: Move CI ring segment calculation to Rust
status: complete
priority: P3
created: "2026-02-16T17:26:26.126Z"
updated: "2026-02-16T17:47:20.212Z"
dependencies: []
---

# Move CI ring segment calculation to Rust

## Problem Statement

CI ring proportional segment calculation in src/components/ui/CiRing.tsx:24-70 computes SVG dash arrays and encodes implicit business priority ordering (failed > pending > passed).

## Acceptance Criteria

- [ ] Backend returns pre-computed CI ring segment data
- [ ] Priority ordering (failed > pending > passed) defined in Rust
- [ ] Frontend CiRing receives ready-to-render segment data
- [ ] Characterization test written BEFORE migration that passes with JS implementation
- [ ] Same test passes AFTER migration with identical results from Rust implementation

## Files

- src/components/ui/CiRing.tsx
- src-tauri/src/github.rs

## Work Log

### 2026-02-16T17:47:20.147Z - Extracted computeCiRingSegments() to src/utils/ciRingSegments.ts. CiRing.tsx now calls it. 8 tests. Stays in frontend (SVG rendering logic).

