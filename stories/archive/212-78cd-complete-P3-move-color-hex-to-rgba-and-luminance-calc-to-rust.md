---
id: 212-78cd
title: Move color hex-to-RGBA and luminance calc to Rust
status: complete
priority: P3
created: "2026-02-16T17:26:26.125Z"
updated: "2026-02-16T18:09:28.349Z"
dependencies: []
---

# Move color hex-to-RGBA and luminance calc to Rust

## Problem Statement

hexToRgba() and isLightColor() in src/components/PrDetailPopover/PrDetailPopover.tsx:92-105 perform color parsing and luminance calculation (ITU-R BT.709). Mathematical logic that could be shared across clients.

## Acceptance Criteria

- [ ] Color conversion and luminance calculation in Rust
- [ ] Backend returns pre-computed text color for label backgrounds
- [ ] Frontend color utility functions removed
- [ ] Characterization test written BEFORE migration that passes with JS implementation
- [ ] Same test passes AFTER migration with identical results from Rust implementation

## Files

- src/components/PrDetailPopover/PrDetailPopover.tsx
- src-tauri/src/github.rs

## Work Log

### 2026-02-16T18:09:28.276Z - Already implemented by story 204 agent: hex_to_rgba() and is_light_color() in github.rs, frontend uses pre-computed text_color and background_color. 11 Rust tests + 5 TS characterization tests pass.

