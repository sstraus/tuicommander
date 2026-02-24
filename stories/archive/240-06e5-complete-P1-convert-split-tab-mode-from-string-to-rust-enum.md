---
id: 240-06e5
title: Convert split_tab_mode from String to Rust enum
status: complete
priority: P1
created: "2026-02-17T11:05:06.221Z"
updated: "2026-02-17T11:09:58.491Z"
dependencies: []
---

# Convert split_tab_mode from String to Rust enum

## Problem Statement

split_tab_mode is a String in config.rs, accepting any value including invalid ones. 7 review findings across 5 reviewers trace back to this. Frontend uses unsafe as SplitTabMode casts.

## Acceptance Criteria

- [ ] Define enum SplitTabMode { Separate, Unified } with serde rename_all lowercase in config.rs
- [ ] Replace String field with SplitTabMode enum in AppConfig
- [ ] Remove default_split_tab_mode() function, use #[serde(default)] on enum
- [ ] Add type guard in GeneralTab.tsx onChange replacing as SplitTabMode cast
- [ ] Fix splitIndex as 0|1 assertion in useTerminalLifecycle.ts with explicit check
- [ ] Update existing tests and add test for invalid config deserialization
- [ ] cargo test passes and npx tsc --noEmit passes and npx vitest run passes

## Files

- src-tauri/src/config.rs
- src/components/SettingsPanel/tabs/GeneralTab.tsx
- src/hooks/useTerminalLifecycle.ts
- src/__tests__/stores/settings.test.ts

## Work Log

### 2026-02-17T11:09:53.753Z - Defined SplitTabMode enum in config.rs, replaced String field, added type guard in GeneralTab.tsx, fixed splitIndex assertion in useTerminalLifecycle.ts. cargo test 273 passed, tsc clean, vitest 1492 passed.

