---
id: 181-3ecc
title: Enable useUnknownInCatchVariables in tsconfig
status: wontfix
priority: P3
created: "2026-02-16T07:12:39.083Z"
updated: "2026-02-16T07:55:41.666Z"
dependencies: []
---

# Enable useUnknownInCatchVariables in tsconfig

## Problem Statement

372 catch clauses use bare err without explicit unknown type annotation. Implicit any in catch blocks bypasses type safety.

## Acceptance Criteria

- [ ] Enable useUnknownInCatchVariables in tsconfig.json
- [ ] Fix resulting type errors with proper narrowing

## Files

- tsconfig.json

## Related

- TS-02

## Work Log

### 2026-02-16T07:55:41.601Z - FALSE POSITIVE: strict: true in tsconfig.json already enables useUnknownInCatchVariables (TypeScript 5.6.3). tsc --noEmit passes cleanly with zero errors.

