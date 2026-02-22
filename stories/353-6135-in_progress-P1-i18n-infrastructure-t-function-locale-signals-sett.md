---
id: 353-6135
title: "i18n infrastructure: t() function, locale signals, settings integration"
status: in_progress
priority: P1
created: "2026-02-22T07:20:56.915Z"
updated: "2026-02-22T07:21:12.663Z"
dependencies: []
---

# i18n infrastructure: t() function, locale signals, settings integration

## Problem Statement

No i18n infrastructure exists. Components have hardcoded English strings. Adding i18n later would require touching every component again, so we add t() wrapping in the same pass as CSS modules.

## Acceptance Criteria

- [ ] src/i18n/t.ts exports t(), locale, setLocale, registerLocale
- [ ] src/i18n/en.json exists with initial keys
- [ ] settings.ts has language field and setLanguage action
- [ ] GeneralTab has Language dropdown
- [ ] setLocale called on hydrate

## Files

- src/i18n/t.ts
- src/i18n/index.ts
- src/i18n/en.json
- src/stores/settings.ts
- src/components/SettingsPanel/tabs/GeneralTab.tsx

## Work Log

