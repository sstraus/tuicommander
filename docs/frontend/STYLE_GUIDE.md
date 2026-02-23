# TUICommander — Visual Style Guide

Reference for all UI/CSS/layout work. Every visual change MUST follow this guide.

## Design Philosophy

**VS Code Dark theme** adapted for a terminal-first, developer-focused interface. The UI is a frame for terminal content — chrome recedes, content dominates. No bright whites. Muted UI elements, vivid status colors. Everything monospace except UI labels.

## Application Layout

```
┌─────────────────────────────────────────────────────────────────────┐
│ #toolbar (38px macOS / 32px Win+Linux, --bg-primary, drag region)  │
│ ┌──────────────┬────────────────────────────────────┬────────────┐ │
│ │ toolbar-left │ toolbar-center (tab bar)            │toolbar-right│ │
│ │ (sidebar w)  │ [Tab1] [Tab2] [Tab3] [+]           │ [IDE btns] │ │
│ └──────────────┴────────────────────────────────────┴────────────┘ │
├─────────────────────────────────────────────────────────────────────┤
│ #app-body (flex: 1, flex-direction: row)                           │
│ ┌──────────┬─────────────────────────────────┬───────────────────┐ │
│ │ #sidebar │ #main                            │ Side panels      │ │
│ │ 300px    │ (flex: 1)                        │ (400px, optional)│ │
│ │ --bg-    │                                  │                  │ │
│ │ secondary│ ┌──────────────────────────────┐ │ ┌──────────────┐ │ │
│ │          │ │ #terminal-container          │ │ │ Diff or      │ │ │
│ │ REPOS    │ │ (flex: 1, --bg-primary)      │ │ │ Markdown or  │ │ │
│ │  section │ │                              │ │ │ Notes/Ideas  │ │ │
│ │  title   │ │  xterm terminal fills this   │ │ │ panel        │ │ │
│ │  repo    │ │  entire area                 │ │ │              │ │ │
│ │   header │ │                              │ │ │ panel-header │ │ │
│ │   branch │ │                              │ │ │ panel-content│ │ │
│ │   branch │ │                              │ │ │              │ │ │
│ │          │ │                              │ │ └──────────────┘ │ │
│ │ FOOTER   │ │                              │ │                  │ │
│ │ [+ Add]  │ └──────────────────────────────┘ │                  │ │
│ │ [icons]  │                                  │                  │ │
│ └──────────┴─────────────────────────────────┴───────────────────┘ │
├─────────────────────────────────────────────────────────────────────┤
│ #status-bar (28px, --bg-secondary, border-top)                     │
│ [zoom][sessions]  [branch ↑2][PR #42][CI ✓]  [toggles][💡][⚙][?] │
└─────────────────────────────────────────────────────────────────────┘
```

**Key structural rules:**
- `#app` is `flex-direction: column`, fills 100vh × 100vw.
- `#app-body` is `flex-direction: row`, `flex: 1`, `min-height: 0`.
- Sidebar is fixed-width (resizable 200–500px), main area fills remaining space.
- Side panels (Diff, Markdown, Notes) appear right of `#main`, width 400px, max 50vw.
- All sections have `overflow: hidden` — scrolling is on inner content areas only.
- Status bar is always at the bottom, never scrolls.

## Color Palette

### CSS Variables (`:root` in `styles.css`)

| Variable | Value | Usage |
|----------|-------|-------|
| `--bg-primary` | `#1e1e1e` | Main canvas — terminals, panel bodies |
| `--bg-secondary` | `#252526` | Sidebar, tab bar, status bar |
| `--bg-tertiary` | `#2d2d30` | Inputs, settings rows, button defaults |
| `--bg-highlight` | `#37373d` | Hover states, active branch bg |
| `--fg-primary` | `#cccccc` | Primary text (max brightness for text) |
| `--fg-secondary` | `#a0a0a0` | Labels, secondary text |
| `--fg-muted` | `#9aa1a9` | Section titles, tertiary text |
| `--accent` | `#59a8dd` | Primary actions, active indicators, links |
| `--accent-hover` | `#7abde5` | Hover on accent elements |
| `--success` | `#4ec9b0` | Positive states, open PRs (teal) |
| `--warning` | `#dcdcaa` | Caution, pending, main branch icon (yellow) |
| `--error` | `#f48771` | Errors, failures, closed PRs (coral) |
| `--border` | `#3e3e42` | All borders and dividers |
| `--text-on-accent` | `#000000` | Black text on colored badge backgrounds |
| `--text-on-error` | `#000000` | Black text on error backgrounds |
| `--text-on-success` | `#000000` | Black text on success backgrounds |

### Extended Palette (hardcoded, contextual only)

| Color | Context |
|-------|---------|
| `#a371f7` | PR merged badge (purple) |
| `#d29922` | Changes requested / review required (orange) |
| `#e3b341` | CI pending (golden) |
| `#ffd700` | Rate limit, question icon (gold) |
| `rgba(122, 162, 247, *)` | Branch ahead/behind tint, pulse glow |
| `rgba(158, 206, 106, *)` | Diff additions bg, CI success tint |
| `rgba(247, 118, 142, *)` | Diff deletions bg, CI failure tint |

### Background Stacking Order (darkest → lightest)

```
#1e1e1e  --bg-primary    Terminal canvas, main area
#252526  --bg-secondary   Sidebar, tab bar, status bar, modals
#2d2d30  --bg-tertiary    Buttons, inputs, settings rows, panel headers
#37373d  --bg-highlight   Hover, active branch, selected items
```

Every surface uses exactly one of these four levels. Elevation = lighter.

## Typography

| Variable | Stack | Usage |
|----------|-------|-------|
| `--font-mono` | JetBrains Mono, Fira Code, Hack, Cascadia Code, Source Code Pro, DejaVu Sans Mono, monospace | Terminals, branch names, stats badges, PR badges, code |
| `--font-ui` | -apple-system, BlinkMacSystemFont, Segoe UI, Roboto, Noto Sans, Liberation Sans, sans-serif | UI labels, buttons, headings, descriptions, settings |

### Size Scale

| Variable | Size | Where |
|----------|------|-------|
| `--font-2xs` | 9px | Smallest labels |
| `--font-xs` | 10px | Badge text, hotkey hints, metadata |
| `--font-sm` | 11px | Section titles (REPOS), secondary labels |
| `--font-md` | 12px | Branch names, tab names, settings labels — **default for UI** |
| `--font-base` | 13px | Body text, document default |
| `--font-lg` | 14px | Panel headings, chevrons |
| `--font-xl` | 16px | Dialog titles |

Font weight: 400 normal, 500 medium (branch names), 600 semibold (repo names, badges), 700 bold (headings only).

## Spacing

### Fixed Dimensions

| Variable | Value |
|----------|-------|
| `--sidebar-width` | 300px (resizable: min 200px, max 500px) |
| `--toolbar-height` | 38px macOS / 32px Win+Linux |
| `--tab-bar-height` | 33px |
| `--status-height` | 28px |

### Spacing Scale

| Size | Usage |
|------|-------|
| 1–2px | Branch item vertical margin, micro separation |
| 4px | Sidebar content top padding, compact flex gaps, micro padding |
| 6px | Icon-to-text gaps, sidebar footer gaps, repo header padding |
| 8px | Button padding, form gaps, sidebar footer padding, standard gap |
| 12px | Branch item horizontal padding, panel header padding, medium padding |
| 16px | Sidebar section margin, branch list left indent, modal padding |
| 20px | Dialog content padding, sidebar empty state padding |

Use `gap` on flex containers, not margins between children.

## Border Radius

| Variable | Value | Usage |
|----------|-------|-------|
| `--radius-xs` | 2px | Minimal — focus rings |
| `--radius-sm` | 3px | Small interactive elements |
| `--radius-md` | 4px | **Standard** — buttons, badges, inputs, branch items |
| `--radius-lg` | 6px | Larger controls — dropdowns, add-repo button, form inputs |
| `--radius-xl` | 8px | Modals, panels, dialogs |
| `--radius-pill` | 12px | PR badges, status pills |
| `--radius-full` | 50% | Circles — toggle thumbs, repo initials avatar |

## Shadows

| Variable | Value | Usage |
|----------|-------|-------|
| `--shadow-popup` | `0 8px 32px rgba(0,0,0,0.4)` | Modals, dialogs |
| `--shadow-dropdown` | `0 4px 16px rgba(0,0,0,0.3)` | Menus, popovers, context menus |
| `--shadow-bottom-anchor` | `0 -4px 20px rgba(0,0,0,0.4)` | Bottom-anchored panels |

Three levels only. Never invent new shadow values.

## Transitions & Animation

### Durations

| Duration | Usage |
|----------|-------|
| 0.1s | Hover backgrounds, active states — instant feedback |
| 0.15s | Standard — opacity, color, transform, border changes |
| 0.2s | Layout — sidebar collapse, toggle switches, chevron rotation |

### Keyframe Animations

**`pulse-opacity`**: Opacity 0.4 → 1.0 → 0.4, infinite. Duration 1.5s or 2s.
Used for: active branch icon, CI pending badge, rate limit indicator.

**`pulse-question`**: Box-shadow 0 → `0 0 12px 4px rgba(122,162,247,0.4)` → 0.
Variants exist with red (error) and orange (confirm) colors.
Used for: terminal tab glow when agent awaits input.

**Tab status dot color scheme** (single `●` indicator left of tab name):
- Grey (opacity 0.3): shell process running (default)
- Green (`--success`): shell idle, prompt ready
- Blue (`--accent`, pulse ×3): background tab produced output
- Orange (`--warning`, pulse infinite): awaiting user input (question or confirmation)
- Red (`--error`, pulse infinite): awaiting error acknowledgement

Always use `ease` timing. Respect `prefers-reduced-motion`. Never `transition: all`.

## Component Reference

### Sidebar

```css
#sidebar {
  width: var(--sidebar-width);    /* 300px */
  min-width: 200px;
  max-width: 500px;
  background: var(--bg-secondary);
  border-right: 1px solid var(--border);
  display: flex;
  flex-direction: column;
}
```

**Section title** (e.g. "REPOS"):
- `font-size: --font-sm`, `text-transform: uppercase`, `color: --fg-muted`
- `letter-spacing: 0.05em`, `padding: 4px 16px`

**Repo header**:
- Flex row, `gap: 6px`, `padding: 6px 12px 3px`
- Repo initials: 28×28px circle, `--accent` bg, `--text-on-accent` text, `--font-xs`, semibold
- Repo name: `--font-sm`, semibold, uppercase, `--fg-secondary`, truncated with ellipsis
- Chevron: `--font-lg`, `--fg-muted`, rotates 0→90° on expand (150ms ease)
- Actions (⋯, +): hidden by default (`opacity: 0`), shown on repo-header hover

**Branch item** (the most complex sidebar element):
```css
.branch-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 12px;
  border-radius: var(--radius-md);
  margin: 1px 0;
  transition: background 0.1s;
}
.branch-item:hover { background: var(--bg-highlight); }
.branch-item.active {
  background: var(--bg-highlight);
  border-left: 2px solid var(--accent);
  padding-left: 10px;  /* compensate for border */
}
```

Branch item anatomy (left to right):
```
[icon 18px] [name flex:1] [stats badge?] [PR badge?] [actions on hover]
```

- **Icon** (18px wide, centered): `★` yellow for main, `Y` muted for feature, `Y` accent+pulse when agent active, `Y` green when shell idle, `?` warning (orange)+pulse when awaiting input
- **Name**: `--font-md`, weight 500, `--fg-primary`, ellipsis on overflow
- **Stats badge** (optional): `--font-xs`, monospace, `--bg-tertiary` bg, `--border` border, `--radius-lg`, shows `+N -N` in green/red
- **PR badge** (optional): `--font-xs`, monospace, semibold, `--radius-pill`, colored by state (see Status Badges below)
- **Actions** (on hover only): `max-width: 0 → 44px`, two 20×20px buttons (+, ×)

### Tab Bar

Located inside `#toolbar`, center section. Background matches toolbar (`--bg-primary`).

```css
.tab {
  height: var(--tab-bar-height);  /* 33px */
  padding: 0 12px;
  font-size: var(--font-md);
  font-family: var(--font-mono);
  color: var(--fg-secondary);
  background: transparent;
  border: none;
  border-top: 2px solid transparent;
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 80px;
  max-width: 200px;
}
.tab.active {
  color: var(--fg-primary);
  border-top-color: var(--accent);
  background: var(--bg-secondary);
}
.tab:hover:not(.active) {
  color: var(--fg-primary);
  background: var(--bg-tertiary);
}
```

Tab anatomy: `[agent badge?] [name, truncated] [close × on hover]`
- Agent badge: small colored prefix (e.g. `C claude`, `G gemini`)
- Close button: invisible by default, `opacity: 1` on tab hover
- New tab button `[+]`: 28px circle, `--accent` color

### Status Bar

```css
#status-bar {
  height: var(--status-height);   /* 28px */
  background: var(--bg-secondary);
  border-top: 1px solid var(--border);
  display: flex;
  align-items: center;
  padding: 0 8px;
  font-size: var(--font-xs);
}
```

Three sections: left (zoom, session count), center (git status), right (toggle buttons).

**Status badges** (in center section): inline-flex, monospace, `--font-xs`, `--radius-md`, padding `2px 6px`.
- Branch: `--bg-tertiary` bg, shows `⎇ main ↑2`
- Branch with ahead: tinted blue background `rgba(122,162,247,0.12)`
- PR: `--accent` bg, `--text-on-accent` text
- CI: semi-transparent colored bg (green/red/yellow, 0.2 alpha)

**Toggle buttons** (right section): `--bg-tertiary` bg, `--border` border, `--font-xs`, `2px 8px` padding.
Active: `--accent` bg, white text. Each has a hotkey hint overlay positioned below.

### Side Panels (Diff, Markdown, Notes/Ideas)

All follow the same structure:
```css
.panel {
  width: 400px;
  min-width: 300px;
  max-width: 50vw;
  border-left: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  background: var(--bg-primary);
}
.panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 12px;
  background: var(--bg-tertiary);
  border-bottom: 1px solid var(--border);
}
.panel-title { font-size: var(--font-lg); font-weight: bold; }
.file-count-badge {
  background: var(--accent);
  color: var(--text-on-accent);
  border-radius: var(--radius-pill);
  padding: 1px 6px;
  font-size: var(--font-xs);
}
.panel-content { flex: 1; overflow-y: auto; }
```

### Dialog / Modal

```css
.overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.65);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}
.dialog {
  width: 480px;
  max-width: 90vw;
  max-height: 80vh;
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--radius-xl);
  box-shadow: var(--shadow-popup);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.dialog-header {
  padding: 16px;
  border-bottom: 1px solid var(--border);
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.dialog-header h2 { font-size: var(--font-xl); }
.dialog-content { padding: 16px; overflow-y: auto; flex: 1; }
.dialog-actions {
  padding: 12px 16px;
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  border-top: 1px solid var(--border);
}
```

**Primary button**: `background: var(--accent); color: var(--text-on-accent); padding: 8px 16px; border-radius: var(--radius-lg);`
**Secondary button**: `background: var(--bg-tertiary); color: var(--fg-secondary); same padding/radius.`
**Danger button**: `background: var(--error); color: var(--text-on-error);`

### Form Controls

```css
input, select, textarea {
  height: 36px;                        /* standard height */
  background: var(--bg-tertiary);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  color: var(--fg-primary);
  font-size: var(--font-md);
  padding: 0 8px;
}
input:focus, select:focus, textarea:focus {
  border-color: var(--accent);
  outline: none;
}
```

**Toggle switch**: 36×20px track, `--radius-full` (10px), off=`--bg-tertiary`, on=`--accent`. White thumb slides with 0.2s transition.

**Range slider**: 4px track height, 16px circular thumb in `--accent`.

### Settings Panel

Full-screen overlay. Inner panel: `--bg-secondary`, 700px max-width, 80vh max-height.
- Left sidebar with tabs (General, Notifications, Dictation, Terminal, Agents)
- Right content area with sections
- Section heading: `<h3>`, `--font-lg`, bold, `margin-bottom: 12px`
- Settings row: flex space-between, label left, control right, `padding: 8px 0`

## Status Badges Reference

### PR State (sidebar `.branch-pr-badge`)

| State | Background | Border | Text Color | Extra |
|-------|-----------|--------|------------|-------|
| Open | `--success` | none | `--text-on-success` | — |
| Draft | transparent | `1px dashed --fg-muted` | `--fg-muted` | — |
| Merged | `#a371f7` | none | `--text-on-accent` | — |
| Closed | `--error` | none | `--text-on-error` | — |
| Conflict | `--error` | none | `--text-on-error` | pulse-opacity 1.5s |
| CI Failed | `--error` | none | `--text-on-error` | bold |
| CI Pending | transparent | `1px solid #e3b341` | `#e3b341` | pulse-opacity 2s |
| Changes Req. | `#d29922` | none | `--text-on-accent` | — |
| Review Req. | transparent | `1px solid #d29922` | `#d29922` | — |

All badges: `font-size: --font-xs`, `font-family: --font-mono`, `font-weight: 600`, `border-radius: --radius-pill`, `padding: 1px 6px`.

### CI State (status bar)

| State | Background | Text |
|-------|-----------|------|
| Success | `rgba(158,206,106,0.2)` | `#9ece6a` |
| Failure | `rgba(247,118,142,0.2)` | `#f7768e` |
| Pending | `rgba(224,175,104,0.2)` | `#e0af68` |

### Agent/Usage (tab + status bar)

| State | Style |
|-------|-------|
| Agent running | Tab has colored agent prefix badge |
| Usage normal | Accent tinted background |
| Usage ≥70% | Yellow/warning background |
| Usage ≥90% | Red/error background + pulse |
| Rate limited | `#ffd700` text, gold bg at 0.1 alpha, pulse 2s |
| Update available | `#4ec9b0` text, teal bg at 0.15 alpha |

## Icons

**No icon library.** Text symbols and Unicode only. Emoji sparingly, always with `filter: grayscale(1) brightness(1.5)` to match the monochrome UI.

| Symbol | Meaning | Where |
|--------|---------|-------|
| `★` | Main/primary branch | Sidebar branch icon |
| `Y` | Feature branch | Sidebar branch icon |
| `?` | Awaiting input | Branch icon (warning/orange, pulsing) |
| `+` | Add/create | Buttons |
| `×` | Close/remove | Tab close, panel close, dialog close |
| `⋯` | Context menu | Repo header |
| `✎` | Edit/rename | Branch double-click |
| `▶` | Send/execute | Notes panel send button |
| `>` | Chevron (expand/collapse) | Repo sections |
| `●` | Tab status dot | Tab bar (grey=running, green=idle, blue-pulse=activity, orange-pulse=awaiting, red-pulse=error) |
| `⎇` | Git branch symbol | Status bar |
| `💡` | Ideas panel | Status bar, panel header |

Icon dimensions: 18px wide container for branch icons. `--font-md` or `--font-lg` size. Always left of text with `gap: 6–8px`.

## Scrollbars

```css
::-webkit-scrollbar { width: 8px; height: 8px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb {
  background: var(--bg-highlight);
  border-radius: var(--radius-md);
}
::-webkit-scrollbar-thumb:hover { background: var(--fg-muted); }
```

Terminal xterm scrollbar overridden to 8px with `!important`.

## Interactive States

### Hover
- Background: one level up (`--bg-secondary` → `--bg-tertiary`, or `--bg-tertiary` → `--bg-highlight`)
- Text: `--fg-secondary` → `--fg-primary`
- Border: transparent → `--accent` (for add-repo button)
- Duration: 0.1s

### Active / Selected
- Active branch: `--bg-highlight` bg + `2px solid var(--accent)` left border
- Active tab: `--bg-secondary` bg + `2px solid var(--accent)` top border + `--fg-primary` text
- Active toggle: `--accent` bg + white text
- No hover animation on already-active items

### Focus
```css
:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
```
Excluded on toggle buttons and mic button.

### Disabled
- `opacity: 0.3` (strong) or `0.5` (mild)
- `cursor: not-allowed`
- No hover, no transitions

### Hidden-until-hover
Pattern used for actions that clutter the UI when always visible:
- Repo actions: `opacity: 0; pointer-events: none;` → `opacity: 1; pointer-events: auto;` on `.repo-header:hover`
- Branch actions: `max-width: 0; overflow: hidden;` → `max-width: 44px;` on `.branch-item:hover`
- Tab close button: `opacity: 0` → `opacity: 1` on `.tab:hover`

## Platform Differences

| Property | macOS | Windows/Linux |
|----------|-------|---------------|
| Toolbar height | 38px | 32px |
| Traffic light offset | `.platform-macos .toolbar-left { padding-left: 78px; }` | None |
| System font | -apple-system first | Segoe UI (Win) / Roboto (Linux) first |
| Quit menu | App menu | File menu |
| Check for Updates | App menu | Help menu |

CSS classes on `<html>`: `.platform-macos`, `.platform-windows`, `.platform-linux`.

## Accessibility

- Primary text (#cccccc on #1e1e1e): 10:1+ contrast ratio (exceeds WCAG AAA).
- Secondary text (#a0a0a0 on #252526): 6:1+ (exceeds WCAG AA).
- Status communicated by **color + shape + icon** — never color alone.
- `:focus-visible` outlines for keyboard navigation.
- `prefers-reduced-motion` query disables all animations.
- Custom scrollbars maintain 8px touch target.

## Anti-Patterns (DO NOT)

- **No bright whites** — max text brightness is `--fg-primary` (#cccccc).
- **No new shadows** — only the three defined levels exist.
- **No `transition: all`** — always list specific properties.
- **No hardcoded core colors** — use CSS variables for the four bg levels, three fg levels, and status colors.
- **No icon libraries** — text/unicode/emoji only.
- **No off-scale radius** — only `--radius-xs` through `--radius-full`.
- **No `!important`** — except xterm scrollbar overrides.
- **No pixel values outside the spacing scale** unless component-specific dimension (like 28px repo initials).
- **No inline styles for theming** — all colors and spacing in `styles.css`.
