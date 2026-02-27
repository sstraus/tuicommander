---
id: 416-439e
title: "Update agent registry: resume commands for Amp/Cursor, remove Jules/ONA"
status: complete
priority: P2
created: "2026-02-27T06:32:53.131Z"
updated: "2026-02-27T11:05:48.969Z"
dependencies: []
---

# Update agent registry: resume commands for Amp/Cursor, remove Jules/ONA

## Problem Statement

agents.ts has incomplete resume commands (Amp and Cursor have null but support resume) and includes Jules and ONA which have no local PTY sessions and are incompatible with TUICommander terminal model.

## Acceptance Criteria

- [ ] Amp resumeCommand set to `amp threads continue`
- [ ] Cursor Agent resumeCommand set to `cursor-agent resume`
- [ ] Jules removed from AGENTS registry, AGENT_DISPLAY, MCP_SUPPORT, AgentType union
- [ ] ONA removed from AGENTS registry, AGENT_DISPLAY, MCP_SUPPORT, AgentType union
- [ ] All references to jules and ona types cleaned up (tests, docs, components)
- [ ] docs/user-guide/ai-agents.md updated to reflect changes

## Files

- src/agents.ts
- docs/user-guide/ai-agents.md
- docs/FEATURES.md

## QA

None — covered by tests and type checking

## Work Log

### 2026-02-27T11:05:48.837Z - Completed: Added resume commands for Amp ('amp threads continue') and Cursor ('cursor-agent resume'). Removed Jules and ONA from AgentType union, AGENTS, AGENT_DISPLAY, MCP_SUPPORT, AgentIcon, classify_agent, plugin_docs, and all docs. Updated README count to 9. Added Droid to AgentsTab. All 717 Rust + 12 agent detection TS tests pass. TypeScript compiles clean.

