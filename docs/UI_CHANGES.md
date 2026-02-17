# UI Changes - Non-Actionable Elements

This document tracks UI elements that were removed or relocated during interface improvements, and explains where to find their functionality now.

## Status Bar Changes

### Removed Elements

#### 1. IDE Selector Dropdown (Bottom Status Bar)
- **Previous Location**: Bottom status bar, right side
- **Previous Functionality**: Quick selector to switch between IDEs (VS Code, Cursor, Zed, Windsurf)
- **Current Location**: Settings Panel → General Settings → Default IDE
- **How to Access**: Click Settings icon (gear) in top-right → General tab → Default IDE dropdown
- **Rationale**: Consolidating infrequently-changed preferences into Settings panel reduces visual clutter

#### 2. Agent Selector Dropdown (Bottom Status Bar)
- **Previous Location**: Bottom status bar, right side
- **Previous Functionality**: Quick selector to switch between AI agents (Claude Code, Gemini CLI, OpenCode, Aider, Codex)
- **Current Location**: Settings Panel → Agents Configuration → Primary Agent
- **How to Access**: Click Settings icon (gear) in top-right → Agents tab → Primary Agent dropdown
- **Rationale**: Consolidating agent configuration into dedicated settings section for better organization

## Settings Panel Improvements

### Visual Improvements (No Functionality Removed)

The Settings Panel was redesigned to improve clarity and organization while **keeping all existing functionality intact**:

1. **Better Labels**: More descriptive section headers and control labels
   - "General" → "General Settings"
   - "Agents" → "Agent Configuration"
   - "Error Strategy" → "Error Handling Strategy"

2. **Enhanced Hints**: All settings now have clear explanatory text
   - Font size: Explains zoom shortcuts
   - Error handling: Clarifies retry behavior
   - Notifications: Describes what events trigger sounds

3. **Improved Organization**: Settings grouped more logically
   - Terminal tab now clearly separates zoom controls from tab management
   - Notification events labeled as "Notification Events" with explanation

4. **Visual Hierarchy**: Better use of spacing and typography
   - Bold text in hints highlights keyboard shortcuts
   - Consistent spacing between groups
   - Clear visual separation between sections

### All Functionality Preserved

**Important**: No settings, options, or features were removed. All functionality remains accessible:

- ✅ All 5 tabs remain (General, Appearance, Terminal, Agents, Notifications)
- ✅ All configuration options preserved
- ✅ All buttons and actions remain functional
- ✅ All keyboard shortcuts work as before

## Migration Notes for Users

If you previously used:

1. **Bottom status bar IDE selector** → Use Settings (gear icon) → General → Default IDE
2. **Bottom status bar Agent selector** → Use Settings (gear icon) → Agents → Primary Agent

These settings are persistent and don't need to be changed frequently, so the extra click to access them via Settings is reasonable.

## Keyboard Shortcuts (Unchanged)

All keyboard shortcuts remain functional:

- **Cmd+T**: New terminal
- **Cmd+W**: Close current terminal tab
- **Cmd+1-9**: Switch to terminal tab 1-9
- **Cmd+Plus**: Zoom in terminal
- **Cmd+Minus**: Zoom out terminal
- **Cmd+0**: Reset terminal zoom to default
- **Cmd+K**: Toggle command palette (if implemented)
- **Cmd+,**: Open settings (if implemented)

## Status Bar (Current State)

The status bar now displays:

**Left Section:**
- Zoom indicator (shows current font size)
- Session count (active/max sessions)
- Status information

**Middle Section:**
- Git branch badge (if in repo)
- Pull request status badge (if applicable)
- CI/CD status badge (if applicable)

**Right Section:**
- MD toggle button (show/hide Markdown panel)
- Diff toggle button (show/hide Diff panel)

---

**Last Updated**: 2026-02-05
**Version**: v0.1.0
