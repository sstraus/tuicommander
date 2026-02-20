# TUI Commander - Project Instructions

## Documentation Sync Reminder

**IMPORTANT:** When implementing new features or keyboard shortcuts, you MUST update the HelpPanel component to keep documentation in sync.

### Files to update:
- `src/components/HelpPanel/HelpPanel.tsx` - Add new shortcuts/features to the help content
- `SPEC.md` - Update feature status

### What to document:
1. **Keyboard shortcuts** - All Cmd/Ctrl combinations
2. **Git operations** - Available git commands and their triggers
3. **Terminal features** - Zoom, clear, copy/paste, tabs
4. **Sidebar interactions** - Repository management, branch operations
5. **Panel toggles** - Diff, Markdown, Settings, Help

### Checklist for new features:
- [ ] Feature works correctly
- [ ] Keyboard shortcut added (if applicable)
- [ ] HelpPanel updated with new shortcut/feature
- [ ] SPEC.md feature status updated

## Testing Tracker

**`to-test.md`** contains features awaiting manual testing when TUI is more usable.

When implementing minor features, add test items to `to-test.md` instead of testing immediately.

## Visual Style Guide

**All UI work MUST follow [`docs/frontend/STYLE_GUIDE.md`](docs/frontend/STYLE_GUIDE.md).** This defines the color palette, typography, spacing, component patterns, animations, and anti-patterns. Read it before any visual/CSS/layout change.

**Icons MUST be monochrome inline SVGs.** Never use emoji for UI icons — they render inconsistently across platforms and break the visual style. Use `<svg>` elements with `fill="currentColor"` so they inherit the text color.

## Visual Verification

**IMPORTANT:** After EVERY visual/CSS/layout change to the TUI, you MUST take a screenshot to verify the result. You cannot reliably judge rendering from code alone.

## Branch Management

**NEVER create branches autonomously.** Boss works with multiple windows simultaneously and autonomous branch creation causes conflicts. Only create/switch branches when explicitly asked.

## Cross-Platform

**This app targets macOS, Windows, and Linux.** Always design and implement features for all three platforms. Use Cmd/Ctrl abstractions, avoid platform-specific APIs without fallbacks, and use Tauri's cross-platform primitives (menus, dialogs, shortcuts, etc.).

### Release-build environment differences

**CRITICAL:** Code that works in `tauri dev` may fail in compiled release builds. Release builds launched from Finder/Explorer/desktop have a minimal environment — they do NOT inherit the user's shell PATH, environment variables, or locale settings. Every feature MUST be tested against these constraints:

- **Never assume CLI tools are on PATH.** Use `resolve_cli()` (in `agent.rs`) to probe well-known directories, or detect apps via `.app` bundles / registry entries instead of `which`/`where`.
- **Never assume environment variables exist.** `$EDITOR`, `$SHELL`, `$HOME` etc. may be absent or different. Always provide fallbacks.
- **Never assume window dimensions are valid.** Persisted window state can contain 0x0 or off-screen positions. Always validate and clamp to sane defaults.
- **Test features in release mode** (`cargo tauri build`) before considering them complete, not just `tauri dev`.

## Panel Refresh Pattern

**All panels that display repo-dependent data MUST subscribe to the `revision` signal from `repositoriesStore.getRevision(repoPath)` inside their `createEffect`.** The Rust `repo_watcher` monitors `.git/` for changes (index, refs, HEAD, merge state) and emits `"repo-changed"` events, which bump the revision counter via `repositoriesStore.bumpRevision()`. Do NOT implement per-panel file watchers or polling.

Example:
```typescript
createEffect(() => {
  const repoPath = props.repoPath;
  const _rev = repoPath ? repositoriesStore.getRevision(repoPath) : 0;
  // ... fetch logic re-runs when revision bumps
});
```

## Architecture Rule: Logic in Rust

**All business logic, data transformation, and parsing MUST be implemented in Rust (backend), NOT in the UI layer (TypeScript/SolidJS stores or components).** The frontend should only handle rendering and user interaction — never data reshaping or computation.

## Release & Tag Checklist

When Boss asks to tag a release:

1. **Update version** in `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`
2. **Update SPEC.md** header version and date
3. **Update CHANGELOG.md** — move Unreleased items under the new version heading
4. **Commit** with message `chore: bump version to vX.Y.Z`
5. **Tag** with `git tag vX.Y.Z`
6. **GitHub release** — create via `gh release create vX.Y.Z --generate-notes`
7. **Milestone** — close the matching milestone if one exists, create the next one

### GitHub Issue Management

- **Labels**: Use `type:`, `P0-P3:`, `area:`, `effort:` prefixes. Apply `needs triage` to new issues.
- **Milestones**: Assign issues to version milestones (v0.4.0, v1.0.0, etc.)
- **Issue templates**: Bug reports and feature requests use `.github/ISSUE_TEMPLATE/*.yml` forms
- **Token for project ops**: Use `GH_TOKEN=$GH_STRAUS gh ...` when commands need the `project` scope (the default `gh auth` token only has `repo` + `workflow`)

## Ideas Tracker

**`IDEAS.md`** is a shared memory for feature concepts under evaluation. Ideas live here until we validate and promote them to SPEC.md for implementation.

- When proposing new features, check IDEAS.md first to avoid duplicating discussion
- When validating an idea, update its status (`concept` -> `validated` -> `designed` -> `moved`)
- When rejecting an idea, mark it `rejected` with reasoning - don't delete it
- When implementing an idea, move it to SPEC.md and mark it `moved` in IDEAS.md
