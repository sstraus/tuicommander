---
id: 296-f8bc
title: Non-null assertion in useAppInit and any type on GroupSection children
status: complete
priority: P2
created: "2026-02-20T13:57:16.844Z"
updated: "2026-02-20T14:16:33.680Z"
dependencies: []
---

# Non-null assertion in useAppInit and any type on GroupSection children

## Problem Statement

Non-null assertion at useAppInit.ts:60 and children:any at Sidebar.tsx:391 violate TS standards.

## Acceptance Criteria

- [ ] Remove non-null assertion with proper narrowing
- [ ] Type children as JSX.Element

## Files

- src/hooks/useAppInit.ts
- src/components/Sidebar/Sidebar.tsx

## Work Log

### 2026-02-20T14:09:59.863Z - Changed children:any to JSX.Element on GroupSection props

### 2026-02-20T14:16:33.614Z - Removed non-null assertion in useAppInit with proper narrowing

