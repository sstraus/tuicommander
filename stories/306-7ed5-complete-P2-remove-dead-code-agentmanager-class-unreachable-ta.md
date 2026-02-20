---
id: 306-7ed5
title: "Remove dead code: AgentManager class, unreachable tabs, legacy CI popover, dead pr-url case"
status: complete
priority: P2
created: "2026-02-20T19:25:07.616Z"
updated: "2026-02-20T19:59:52.366Z"
dependencies: []
---

# Remove dead code: AgentManager class, unreachable tabs, legacy CI popover, dead pr-url case

## Problem Statement

AgentManager class (140 lines) in agents.ts never imported in production. AgentsTab.tsx and AppearanceTab.tsx unreachable. Legacy CI popover in StatusBar.tsx (~50 lines) superseded by PrDetailPopover. Dead pr-url case in Terminal.tsx switch. globalErrorHandler export never imported.

## Acceptance Criteria

- [ ] Verify before: grep for each item confirming zero production imports
- [ ] Delete AgentManager class and agentManager singleton from agents.ts (keep AGENTS and AGENT_DISPLAY)
- [ ] Delete AgentsTab.tsx and AppearanceTab.tsx (confirm not being wired in elsewhere)
- [ ] Remove legacy CI popover state and markup from StatusBar.tsx; wire CI badge click to open PrDetailPopover
- [ ] Remove dead pr-url case from Terminal.tsx switch and from ParsedEvent union
- [ ] Remove globalErrorHandler export from error-handler.ts
- [ ] Run make check and all tests pass

## Files

- src/agents.ts
- src/components/SettingsPanel/tabs/AgentsTab.tsx
- src/components/SettingsPanel/tabs/AppearanceTab.tsx
- src/components/StatusBar/StatusBar.tsx
- src/components/Terminal/Terminal.tsx
- src/error-handler.ts

## Work Log

### 2026-02-20T19:59:52.295Z - Removed AgentManager class+singleton+tests (140 lines + 2 test files). Removed AgentsTab.tsx and AppearanceTab.tsx. Removed legacy CI popover from StatusBar (CI badge now opens PrDetailPopover). Removed pr-url ParsedEvent type and dead case. Removed globalErrorHandler export. Updated StatusBar tests (7 CI popover tests → 1 new test). 1607 tests pass.

