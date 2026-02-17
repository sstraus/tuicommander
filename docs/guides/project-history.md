# Project History

## Timeline

**Started:** February 5, 2026
**Total commits:** 475+
**Contributor:** Stefano Straus (solo developer)
**Convention:** Conventional commits with story references

## Milestones

| Date | Milestone | Key Changes |
|------|-----------|-------------|
| **Feb 5** | Project inception | First commit, worktree terminal support |
| **Feb 6-7** | Core infrastructure | Sidebar, toolbar, tab system, terminal persistence |
| **Feb 8** | Stability & testing | PTY stability overhaul, 830 tests at 80% coverage |
| **Feb 8** | GitHub integration | PR monitoring, CI rings, batch status checks |
| **Feb 15** | Voice dictation | Whisper.rs integration, push-to-talk, model management |
| **Feb 15** | Remote access | HTTP server, WebSocket streaming, MCP bridge |
| **Feb 15** | Settings unification | Consolidated settings, Rust config infrastructure |
| **Feb 16** | Architecture refactor | App.tsx split into hooks, lib.rs split into modules |
| **Feb 16** | Rust migration | 14 business logic functions moved from TS to Rust |
| **Feb 16** | Cross-platform | Windows/Linux support, platform detection |
| **Feb 16** | Native menu | System menu with keyboard shortcuts |

## Architecture Evolution

### Phase 1: Rapid Prototyping (Feb 5-7)
- Monolithic `App.tsx` (~2000+ lines)
- localStorage for persistence
- Frontend-heavy business logic
- Working but unmaintainable

### Phase 2: Stabilization (Feb 8)
- PTY reliability hardened (UTF-8 boundaries, ANSI escape handling, DashMap concurrency)
- Vitest test infrastructure added (830 tests across 4 tiers)
- Performance tuning (WebGL renderer, flow control with watermark backpressure)

### Phase 3: Feature Expansion (Feb 8-15)
- GitHub integration (batch PR queries, CI ring visualization)
- Voice dictation (Whisper with Metal acceleration)
- Remote access (HTTP/WebSocket, MCP SSE transport)
- Split panes, prompt library, settings consolidation

### Phase 4: Architectural Maturity (Feb 16)
- **Hook extraction:** App.tsx split into 8 focused hooks (useTerminalLifecycle, useGitOperations, useAppInit, etc.)
- **Rust module extraction:** lib.rs monolith split into state.rs, pty.rs, git.rs, github.rs, agent.rs, worktree.rs, etc.
- **Business logic migration:** 14 functions moved from TypeScript to Rust, following the "Logic in Rust" architecture mandate
- **Cross-platform support:** Shell detection, platform config directories, conditional compilation

## Feature Commit Distribution

| Area | Commits | Percentage |
|------|---------|------------|
| Git/Worktree Operations | ~272 | ~57% |
| GitHub Integration | ~52 | ~11% |
| UI & Styling | ~36 | ~8% |
| Terminal & PTY | ~29 | ~6% |
| Architecture Refactors | ~23 | ~5% |
| Settings & Config | ~15 | ~3% |
| Testing | ~14 | ~3% |
| Voice Dictation | ~14 | ~3% |
| Remote Access & MCP | ~12 | ~3% |
| Cross-Platform | ~6 | ~1% |

## Commit Conventions

- **Conventional commits:** `feat:`, `fix:`, `refactor:`, `chore:`, `perf:`, `test:`, `style:`, `docs:`
- **Story references:** `(Story 093)` suffix when linked to a story
- **Git notes:** Every commit has a git note for additional context
- **Story tracking:** 214+ stories in `stories/` directory with YAML frontmatter

## Story System

Stories are tracked as markdown files in `stories/`:

```
stories/
  200-18c5-complete-P2-move-error-classification-logic-from-ts-to-rust.md
  ┃       ┃         ┃    ┗━ Slug from title
  ┃       ┃         ┗━ Priority (P1/P2/P3)
  ┃       ┗━ Status (complete/pending/blocked/wontfix)
  ┗━ Sequence number + 4-char hex suffix
```

Format: YAML frontmatter (id, title, status, priority, dates) + markdown body with work log.
