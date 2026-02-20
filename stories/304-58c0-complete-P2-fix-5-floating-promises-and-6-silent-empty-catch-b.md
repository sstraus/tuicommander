---
id: 304-58c0
title: Fix 5 floating promises and 6 silent empty catch blocks
status: complete
priority: P2
created: "2026-02-20T19:25:07.615Z"
updated: "2026-02-20T19:51:32.861Z"
dependencies: []
---

# Fix 5 floating promises and 6 silent empty catch blocks

## Problem Statement

Floating promises: pollAll() in github.ts:183,192; invoke open_in_app in App.tsx:322; checkForUpdate() in App.tsx:447 and GeneralTab.tsx:117; attachSessionListeners() in Terminal.tsx:320. Silent .catch with no logging: checkForUpdate at App.tsx:211; PTY close in App.tsx:260; beforeunload PTY in useAppInit.ts; menu-action listen in App.tsx:461; getDiffStats in useRepository.ts; pty.write in useDictation.ts.

## Acceptance Criteria

- [ ] Verify before: grep for floating promise patterns and empty catch blocks in listed files
- [ ] Add .catch with console.error to all 5 floating promises
- [ ] Replace empty .catch with .catch with console.warn/error in all 6 silent catches
- [ ] getDiffStats: add console.debug in catch before returning zeros
- [ ] handleDictationStop: wrap pty.write in try/catch, show error status on failure
- [ ] Run make check and existing tests pass

## Files

- src/stores/github.ts
- src/App.tsx
- src/components/SettingsPanel/tabs/GeneralTab.tsx
- src/components/Terminal/Terminal.tsx
- src/hooks/useAppInit.ts
- src/hooks/useRepository.ts
- src/hooks/useDictation.ts

## Work Log

### 2026-02-20T19:51:32.790Z - Fixed 5 floating promises (.catch with logging) and 5 remaining silent catch blocks. Files: github.ts (pollAll x2), App.tsx (open_in_app, checkForUpdate menu, listen .catch, pty.close), Terminal.tsx (attachSessionListeners), useRepository.ts (getDiffStats debug log), useDictation.ts (pty.write try/catch).

