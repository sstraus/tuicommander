---
id: "358-4eec"
title: "Move UI path manipulation to Rust (cross-platform safety)"
status: pending
priority: P2
created: 2026-02-22T16:16:43.685Z
updated: 2026-02-22T16:16:43.685Z
dependencies: []
---

# Move UI path manipulation to Rust (cross-platform safety)

## Problem Statement

Several UI components (RepoSection, DiffTab, FileTree) manipulate file paths using JS string operations (split, join, replace). This is fragile on Windows where the separator is backslash. Path operations belong in Rust which has proper cross-platform path handling.

## Acceptance Criteria

- [ ] Path display/formatting moved to Rust responses (basename, relative path already computed server-side)
- [ ] No raw string path manipulation for separators in TypeScript UI components
- [ ] Windows path scenarios covered in Rust tests

## Files

- src/components/Sidebar/RepoSection.tsx
- src/components/DiffTab/DiffTab.tsx
- src-tauri/src/git.rs

## Work Log

