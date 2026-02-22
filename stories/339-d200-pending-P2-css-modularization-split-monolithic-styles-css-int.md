---
id: 339-d200
title: "CSS modularization: split monolithic styles.css into co-located modules"
status: pending
priority: P2
created: "2026-02-21T19:11:16.300Z"
updated: "2026-02-21T19:11:22.227Z"
dependencies: ["338-6509"]
---

# CSS modularization: split monolithic styles.css into co-located modules

## Problem Statement

styles.css is 6342 lines with ~456 global classes and zero scoping. Contributors must scan the entire file to find or modify styles. Dead CSS is invisible. Class name collisions are possible. This blocks efficient UI work.

## Acceptance Criteria

- [ ] Inventory of all CSS class groups and their target components
- [ ] global.css contains only: CSS variables, resets, fonts, shared utility classes (.editor-header, .editor-btn, etc.)
- [ ] Each component directory has a co-located .module.css file with scoped selectors
- [ ] Migration done bottom-up: leaf components first, then panels, then god components
- [ ] Zero visual regressions verified by screenshots after each batch
- [ ] All existing tests pass after migration
- [ ] styles.css deleted or reduced to global.css only

## Files

- src/styles.css
- src/components/

## Related

- UX study
- frontend refactoring

## Work Log

