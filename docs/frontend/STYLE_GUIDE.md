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
│ │  title   │ │  terminal fills this          │ │ │ panel        │ │ │
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

Values shown are the `:root` defaults of `global.css` (the former VS Code Dark palette, in effect until a theme loads). The app supports **13 bundled themes** (Commander is the default and the fallback) — all core colors are CSS custom properties overridden at runtime by `applyAppTheme()` in `themes.ts`, which also sets `html[data-theme="<key>"]` so a theme can carry typography rules in `global.css` (only `clean` does: neutral gray `--tab-*-rgb` tints so typed tabs do not each bring their own hue). Font smoothing is global: `html` forces `-webkit-font-smoothing: antialiased` and 0.01em tracking, the rasterization every native-looking macOS app uses. The native window and webview backing layers also use `bgPrimary`: Rust resolves the saved theme before window creation, and `applyAppTheme()` synchronizes both layers on theme changes and hot reload. When writing CSS, always use variables, never hardcode core palette values.

**Theme gallery (dev only).** With `make dev` running, open `http://127.0.0.1:1421/theme-gallery.html` (`src/dev/themeGallery.tsx`). It renders one row per bundled theme: the shipped theme on the left and, on the right, the edit proposed in `src/dev/themeProposals.ts` (a partial override merged over the JSON; empty when nothing is under review). Each card mounts the real `Toolbar`, `Sidebar`, `TabBar` and `StatusBar` components over stores seeded once in the page (a fake `__TAURI_INTERNALS__` in `theme-gallery.html` answers every `invoke`), so a component or stylesheet edit repaints every theme through HMR and a theme JSON edit shows up within a second. Only the terminal body and the dialog are stand-ins; never hand-copy component markup into the gallery, a copy drifts within a session (the page polls `/__tuic_themes`, a serve-only Vite middleware in `vite.config.ts`). Use it to check a palette change against every theme at once before touching the live app. A static snapshot of the 2026-09-22 review, taken before its edits were applied, is in `docs/design/theme-gallery-2026-09-22/`. It is not a build input and never ships. To show it as a TUIC `ui` tab inside a dev build, use `http://localhost:1421/...`: the dev app is served from `127.0.0.1:1421`, and `PluginPanel` blanks a same-origin iframe (`guardSameOriginNav`).

### CSS Variables (`:root` in `global.css`)

| Variable | Default (vscode-dark) | Usage |
|----------|-------|-------|
| `--bg-primary` | `#1e1e1e` | Main canvas — terminals, panel bodies |
| `--bg-secondary` | `#252526` | Sidebar, tab bar, status bar |
| `--bg-tertiary` | `#2d2d30` | Inputs, settings rows, button defaults |
| `--bg-highlight` | `#37373d` | Hover states, active branch bg |
| `--fg-primary` | `#cccccc` | Primary text (max brightness for text) |
| `--fg-secondary` | `#a0a0a0` | Labels, secondary text |
| `--fg-muted` | `#9aa1a9` | Section titles, tertiary text |
| `--accent` | `#59a8dd` | Primary actions, active indicators, links (theme-dependent) |
| `--accent-hover` | `#7abde5` | Hover on accent elements (theme-dependent) |
| `--activity` | `#59a8dd` | Busy/activity pulse indicators (fixed in `global.css`, not overridden by themes) |
| `--success` | `#4ec9b0` | Positive states, open PRs (teal) |
| `--warning` | `#dcdcaa` | Caution, pending, main branch icon (yellow) |
| `--attention` | `#e8984c` | Actionable alerts, confirmation prompts (orange) |
| `--error` | `#f48771` | Errors, failures, closed PRs (coral) |
| `--merged` | `#ae83ff` | PR merged marker and badge background (purple; Paper uses `#7540b0`) |
| `--text-on-merged` | `#000000` | Filled merged badge label (Paper uses `#ffffff`); independent of accent action foreground |
| `--unseen` | `#c084fc` | Terminal completed while user wasn't viewing (purple, clears on view) |
| `--diff-del` | `--error` mixed 25% with white | Deleted-line counts in the sidebar stats badge. Lighter than `--error` so a negative diff does not read as an alarm; `vscode-light` resets it to `--error` |
| `--border` | `#3e3e42` | All borders and dividers |
| `--text-on-accent` | `#000000` | Black text on colored badge backgrounds |
| `--text-on-error` | `#000000` | Black text on error backgrounds |
| `--text-on-success` | `#000000` | Black text on success backgrounds |

### Extended Palette (hardcoded, contextual only)

| Color | Context |
|-------|---------|
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
| `--font-3xs` | 8px | Micro labels, pixel-level detail |
| `--font-2xs` | 10px | Smallest visible labels |
| `--font-xs` | 11px | Badge text, hotkey hints, metadata |
| `--font-sm` | 12px | Section titles (REPOS), secondary labels |
| `--font-md` | 13px | Branch names, tab names, settings labels — **default for UI** |
| `--font-base` | 14px | Body text, document default |
| `--font-lg` | 15px | Panel headings |
| `--font-xl` | 17px | Dialog titles |
| `--font-2xl` | 20px | Large headings |
| `--font-3xl` | 24px | Hero text, splash screens |

Font weight: 400 normal, 500 medium (branch names), 600 semibold (repo names, badges), 700 bold (headings only).

## Spacing

### Fixed Dimensions

| Variable | Value |
|----------|-------|
| `--sidebar-width` | 300px (resizable: min 200px, max 500px) |
| `--toolbar-height` | 38px macOS / 32px Win+Linux |
| `--tab-bar-height` | 32px |
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
| `--radius-full` | 9999px | Pills; square elements stay circular — toggle thumbs, repo initials avatar |

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
Defined in each CSS Module file that uses it (not in `global.css` — CSS Modules scope animation names).
Used for: active branch icon, CI pending badge, rate limit indicator.

**`pulse-question`**: Box-shadow 0 → `0 0 12px 4px rgba(122,162,247,0.4)` → 0.
Variants exist with red (error) and orange (confirm) colors.
Used for: terminal tab glow when agent awaits input.

**`pendulum`**: Translates text from 0 to `-overflow-px` and back. Duration computed dynamically from overflow width (~50px/s, minimum 4s cycle). Uses CSS custom properties `--overflow-px` and `--ticker-duration`.
Used for: status bar notification text that overflows its container.

**Tab status dot color scheme** (single `●` indicator left of tab name):
- Grey (opacity 0.3): idle — no session or command never ran
- Blue (`--activity`, pulse infinite): busy — producing output now
- Green (`--success`): done — command completed
- Purple (`--unseen`, static): completed while user wasn't viewing (clears on view)
- Orange (`--attention`, pulse infinite): agent needs user input (question)
- Red (`--error`, pulse infinite): API error or agent stuck

**Tab type color scheme** (gradient background + colored border-bottom):
- Red (`#ef4444`): diff tabs
- Blue (`--accent` / `#7aa2f7`): editor tabs
- Teal (`#2dd4bf`): markdown tabs
- Purple (`#a78bfa`): panel tabs
- Amber (`#fbbf24`): remote PTY sessions (created via HTTP/MCP)

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
- Chevron: the shared `ChevronIcon` (`src/components/ui/ChevronIcon.tsx`, 12px SVG), `--fg-secondary`, rotates 0→90° on expand (150ms ease). Branch rows use a separate, always-visible disclosure button beside the status icon. The collapsed count stays beside the chevron; the status icon and row only select the branch.
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
[icon 14px + agent count?] [name flex:1] [unmerged mark?] [PR marker?] [diff numbers?] [actions on hover]
```

- **Icon** (18px wide, centered): `★` yellow for main, `Y` muted for feature, `Y` accent+pulse when agent active, `Y` green when shell idle, `?` warning (orange)+pulse when awaiting input
- **Name**: `--font-md`, weight 500, `--fg-primary`, ellipsis on overflow
- **Agent count** (when the tab tree is on): a bare `--font-3xs` digit at the icon's bottom-right corner with a `--bg-secondary` halo. Never a pill: a pill covers the glyph that tells main, branch and worktree apart
- **PR marker** (optional): a 7px square plus `#N`, both in the state color, no fill or border. A conflict rotates the square into a diamond so it does not rely on color. Draft and Review outline the square instead of filling it, as their pills did (Review vs Changes requested differ only by fill). Motion matches the pill form: the conflict marker pulses at 1.5s, Checking and CI Running at 2s; the number never moves. The UI Legend renders this same component The state word (Review, Conflicts, …) and any uncommitted-file count live in the `data-tooltip`
- **Unmerged mark** (optional): a 7px outlined square in `--fg-secondary`, centered in a 14px target with no pill background. It marks a worktree with commits outside the default branch even when its working tree is clean; hover or focus explains why branch deletion is blocked.
- **Diff numbers** (optional): plain `+N -N` in `--font-2xs`, `--success` / `--diff-del`, no chip. Always shown beside the PR marker, never alternating with it
- **Actions** (on hover only): reveal the natural width of shared compact `Button` controls (+, ×), without reserving space while idle. Compact actions are 22px high on desktop and at least 44px high at touch widths. Swipe-tray actions use shared stretch geometry; the open tray grows with its controls.

### Tab Bar

Located inside `#toolbar`, center section. Background matches toolbar (`--bg-primary`).

```css
.tab {
  height: var(--tab-bar-height);  /* 32px */
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
- New-tab and scroll actions use shared stretch `Button` geometry with square edges (22px toolbar density, 44px at touch widths). The tab-bar container grows with these controls. Close and global-workspace actions use shared compact Buttons; inline rename uses a shared compact Input.

### Status Bar

```css
.bar {
  height: var(--status-height);   /* 28px */
  min-height: var(--status-height);
  background: var(--bg-secondary);
  border-top: 1px solid var(--border);
  display: flex;
  align-items: center;
  padding: 0 12px;
  font-size: var(--font-sm);
  gap: 8px;
  overflow: hidden;
}
```

Three sections: left (zoom, status info, CWD, agent badge, ticker), center (PR + CI badges), right (toggle buttons).

**Agent badge** (`.agentBadge`): `--font-mono`, `--font-sm`, weight 500, `1px 6px` padding, `--radius-sm`, `--bg-tertiary` bg. Usage-dependent color classes:
- `.agentUsage` — `--fg-secondary` text (normal usage)
- `.agentUsageWarning` — `#dcdcaa` text (usage >=70%)
- `.agentUsageCritical` — `#f48771` text + `pulse-opacity` animation (usage >=96%)
- `.agentRateLimited` — `#f44747` text + `pulse-opacity` animation

**Ticker message** (`.tickerMessage`): `--font-mono`, `--font-sm`, `--fg-muted`, max-width 300px, `--radius-sm`. Warning priority (>=80): `#ffd700` text, gold bg at 0.1 alpha. `.tickerClickable` adds cursor pointer and hover effect.

**Pendulum overflow** (`.infoTickerActive`): When the status info text is wider than its container, a CSS `pendulum` keyframe animation scrolls the text left then back. Duration is computed dynamically from overflow width (~50px/s). Click dismisses.

**Count badges** (`CountBadge` in `components/ui`): the one counter for every icon button: status bar toggles (errors, ideas, git changes) and the sidebar footer (parked, stale, tunnels). 12px high, `--radius-sm`, 9px bold tabular digits, top-right corner at -3px of a `position: relative` button. Color is a `tone` (`accent`, `error`, `success`), never an inline style. It renders nothing for a zero count.

**PR badges** (center section): `PrBadge` + `CiBadge` components in `.githubStatus`, separated by a left border. CLOSED PRs are hidden; MERGED PRs have a 5-minute activity-based grace period.

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
**Ready action** (Send, Save): set `data-ready="true"` on the button while it can act (unsaved changes, agent selected). `styles/ready-actions.css`, imported by desktop `global.css` and PWA `mobile.css`, gives `button[data-ready]:not(:disabled)` the accent border, text and a faint accent tint; clean or disabled stays neutral. Never hand-roll a per-component "dirty" colour.

### Form Controls

There are two shared densities: **form** (36px, 8px horizontal padding) and
**toolbar** (22px, 6px horizontal padding). The toolbar size matches the measured
22px panel close/detach controls and fits inside the 32px app toolbar. Compose
`toolbar` from `ui/FormControls.module.css` on header, filter, tab and toolbar
rows. Do not apply it to a whole panel with configuration forms. Compose `form`
on a nested form to reset inherited toolbar tokens. Shared dialog popovers, native
dialogs, `[role="dialog"]` and native `[popover]` roots reset to form density, including when
mounted inside a toolbar. Mark custom modal roots with their dialog role and
accessible name; do not add a per-dialog density override.
Shared icon actions and segmented switches follow the row density. Settings and
configuration forms keep form density. Both densities retain the 44px touch
minimum at viewport widths up to 600px.

Use `Button`, `Select`, `Input` and `Textarea` from `src/components/ui` for form fields.
They forward native attributes, refs, options and events. `Input` is for text,
numeric and date fields. Use `Switch`, `Slider` and `ColorInput` for native
checkbox switches, ranges and colour pickers; file pickers keep their specialised controls. `FormControls.module.css` owns their presentation:
`--control-height` (36px), `--control-padding-x` (8px), theme colours,
`--radius-lg` and `--font-md`. Containers own width and alignment, never
control height, padding, borders or colours. Textareas resize vertically;
multiple selects keep native listbox behavior. Shared Select reserves 32px at the inline end for its native arrow; consumers must not add feature padding for this. At widths up to 600px,
controls have a 44px minimum height and 16px text to prevent iOS zoom.

`Button` forwards native button attributes and defaults to `type="button"` (set
`type="submit"` for form submission). Variants are `primary`, `secondary` (default),
`ghost` and `danger`; sizes are `default` (36px) and `compact` (22px), both with a
44px touch minimum. It shares FormControls height and padding tokens with fields.
Clickable list rows/cards retain native buttons with `data-control-layout="row"`
or `data-control-layout="card"`. Their geometry selectors must include both the
`button` tag and the exact marker. This exception is for content layout, never
form, toolbar or dialog actions.

AI Chat retains native full-frame conversation pickers and native session
`details`/`summary` disclosures. Mark these controls with
`data-control-layout="overlay-picker"` or `"disclosure"` and retain only their
exact, reasoned geometry exceptions. The visible picker frame and disclosures
use the shared 36px/44px control tokens. Its shared `Textarea` retains the tested
panel-relative editor cap and one-line reset with a narrow `composer-editor`
exception; this does not exempt other actions or fields. Keep desktop editor
resize and overflow rules scoped to the non-mobile composer. The phone editor
keeps its native scrollable surface for drafts beyond its height cap.

`SegmentedControl` is the shared view switch. It uses a named tablist, one
keyboard tab stop, and Arrow Left/Right, Home and End to select and focus an
enabled choice. Tasks uses it for List, Board and Roadmap.

**Toggle switch**: use `ui/Switch`, with a 36px row-height target (44px on touch),
a 36×20px track, `--radius-full` (10px), off=`--bg-tertiary`, on=`--accent`. White thumb slides with 0.2s transition.

**Range slider**: use `ui/Slider`, with the shared row-height target, a 4px track
and the existing 14px circular thumb in `--accent`. Native input/change events
remain separate. `ui/ColorInput` keeps the native colour picker over a shared
swatch target; its `selected` prop marks a custom selection. The controls guard
also rejects feature-owned checkbox/range/colour widths and shared primitive sizing.

### Settings Panel

Full-screen overlay. All pages share `width: min(1136px, calc(100vw - 48px))`, `height: 80vh`, independently scrolling navigation and content, and existing theme tokens. This width replaces the previous 700px guide after the 2026-10-16 mock approval. Never resize on tab selection or eagerly mount pages to avoid flashes.
- Navigation: 232px default; retain saved user width and resize behavior. Content padding: 24px 32px.
- `SettingPageHeader`: 20px title, optional subtitle, scope badge and optional source. Omit subtitles that only repeat card headings. A header without a subtitle or source reserves no subtitle space.
- `SectionCard`: semantic `<h3>`, 15px heading, 16px padding/gap, 8px radius, `--bg-primary`.
- `SettingRow`: persistent label/hint left, existing shared control right (up to 260px, 36px high), 12px vertical padding. Reuse `SettingField` IDs; errors go below the field and join `aria-describedby`.
- `StatusSummary`: visible status words and existing `StatusBadge` semantic tone, source and freshness. Unknown is neutral; saved sign-in is not a live connection.
- `ListRow`: wrapping name/subtitle, source/version/access badges and named actions. `EmptyState` explains absence and offers one valid action; do not conflate loading, failed or unsupported state.
- `DangerZone`: explicit scope and consequence for destructive/disconnecting actions only; caller retains confirmation. Ordinary reset is not automatically destructive.
- Label/help pairs use the shared `.helpLabel` layout so stacked and grid fields reserve one cell for the pair; the badge remains beside wrapped label text.
- Paper shades the shared success, attention, changes/review and merged tokens for readable marker text; the legend and sidebar keep the same components and shapes.
- Agent color tiles use metadata-selected `--identity-on-light` or `--identity-on-dark`; identity fills do not change with the app theme.
- Settings maps secondary and muted copy to the shared `--settings-foreground` token, mixed from the theme foregrounds for elevated/highlighted surfaces. Links and link buttons use `--link-foreground` without changing action fills. Settings accent actions retain the theme's `--text-on-accent` foreground.
- Help is 12px `--fg-secondary`, never essential `--fg-muted`. Contextual help is selective and adds information beyond the subtitle; do not add a question mark to every row.
- Card heading actions stay in the shared heading flex layout so descriptions cannot flow beneath an absolute action. Color targets do not shrink; compact color targets are 44px circles and wrap within the full-width control column.
- Below 800px rows stack; below 600px navigation is compact and controls/actions have 44px targets. Preserve keyboard labels and long translations.
- Keep existing autosave/explicit-save boundaries and lazy mounting. Shared primitives never probe, save or infer service state.

## Status Badges Reference

### PR State (sidebar `.branch-pr-badge`)

| State | Background | Border | Text Color | Extra |
|-------|-----------|--------|------------|-------|
| Open | `--success` | none | `--text-on-success` | — |
| Draft | transparent | `1px dashed --fg-muted` | `--fg-muted` | — |
| Merged | `--merged` | none | `--text-on-merged` | — |
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

| State | CSS Class | Style |
|-------|-----------|-------|
| Agent running | Tab colored agent prefix | Tab has colored agent prefix badge |
| Usage normal | `.agentUsage` | `--fg-secondary` text |
| Usage ≥70% | `.agentUsageWarning` | `#dcdcaa` text (warning yellow) |
| Usage ≥96% | `.agentUsageCritical` | `#f48771` text + pulse-opacity 2s |
| Rate limited | `.agentRateLimited` | `#f44747` text + pulse-opacity 2s |
| Ticker warning (≥80 priority) | `.tickerWarning` | `#ffd700` text, gold bg at 0.1 alpha |
| Update available | `.updateBadge` | `#4ec9b0` text, teal bg at 0.15 alpha |

## Icons

**No icon library dependency.** Icons are monochrome inline SVGs with `fill="currentColor"`, so they inherit the surrounding text colour and recolour with the theme for free. **Never emoji** — AGENTS.md → "Visual".

Text symbols and Unicode are still correct where the glyph *is* the content rather than an icon: the branch and state markers in the table below, and single characters inside a dense row. Reach for an inline SVG for anything that reads as an icon.

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
| `ChevronIcon` | Chevron (expand/collapse), SVG not text | Sidebar groups, repos, GitHub sections, search bar |
| `●` | Tab status dot | Tab bar (grey=running, green=idle, purple=unseen, blue-pulse=activity, orange-pulse=awaiting, red-pulse=error) |
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

Terminal scrollbar overridden to 8px with `!important`.

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

## PWA / Mobile (`src/mobile/mobile.css`)

The mobile PWA has its own standalone stylesheet at `src/mobile/mobile.css`, **completely independent from the desktop `global.css`**. This allows the mobile UI to evolve separately while sharing the same design language.

### What's shared
- Core color palette (same `--bg-*`, `--fg-*`, `--accent`, `--success`, `--warning`, `--attention`, `--error` variables with identical default values)
- Border radius scale (`--radius-sm` through `--radius-full`, excluding `--radius-xs`)
- Shadow tokens (`--shadow-popup`, `--shadow-dropdown`)
- ANSI terminal palette
- Session Chat semantic prompt/answer colors and the font size scale, imported from `src/styles/chat-tokens.css` by both stylesheet entrypoints

### What differs
| Property | Desktop (`global.css`) | Mobile (`mobile.css`) |
|----------|----------------------|----------------------|
| Font mono stack | JetBrains Mono, Fira Code, Hack, Cascadia, ... | SF Mono, Menlo, Consolas, DejaVu, ... |
| Font size scale | `--font-3xs` through `--font-3xl` (8–24px) | Same shared scale; native inputs retain 16px text |
| Layout variables | `--sidebar-width`, `--toolbar-height`, `--tab-bar-height`, `--status-height` | Not used — mobile uses flex-based layout |
| User select | Disabled (`none`) | Enabled (`text`) |
| Theme variables | `--activity`, `--merged`, `--unseen` | Not present (mobile has no terminal activity tracking yet) |
| Safe areas | Not used | `env(safe-area-inset-top/bottom)` on `#mobile-app` |
| Input font-size | From scale | Fixed `16px` minimum to prevent iOS auto-zoom |

### Keeping in sync
When updating core palette colors in `global.css`, **also update `mobile.css`** — the `:root` blocks must stay aligned for the shared variables. Theme-specific variables (`--activity`, `--merged`, `--unseen`) are desktop-only and do not need mobile equivalents until that functionality ships in PWA.

## Anti-Patterns (DO NOT)

- **No bright whites** — max text brightness is `--fg-primary` (#cccccc).
- **No new shadows** — only the three defined levels exist.
- **No `transition: all`** — always list specific properties.
- **No hardcoded core colors** — use CSS variables for the four bg levels, three fg levels, and status colors.
- **No icon libraries, no emoji** — monochrome inline SVG with `fill="currentColor"`.
- **No off-scale radius** — only `--radius-xs` through `--radius-full`.
- **No `!important`** — except terminal scrollbar overrides.
- **No pixel values outside the spacing scale** unless component-specific dimension (like 28px repo initials).
- **No inline styles for theming** — all colors and spacing via CSS variables (desktop: `global.css`, mobile: `mobile.css`).


### Touch Sidebar Action Trays

Keep branch actions hidden at rest on touch devices. A left swipe reveals a
full-height tray with neutral More, accent Add, and error-coloured Remove
buttons, with at least 44px width per action. Only removable linked worktrees
show Remove. Keep native vertical scrolling with `touch-action: pan-y`.
Use the existing palette and radius tokens, and disable slide transitions when
reduced motion is requested. Mouse hover controls retain their existing layout.

Rich working agent rows reserve two clamped intent lines. Idle, awaiting-input and error rows stay on one line with status dots; their full intent, task or prompt remains in the tooltip. Compact rows keep their one-line layout.

Rich navigation keeps compact typography and padding. Branch identity and facts share one line, with names truncated and full PR/lifecycle facts in tooltips. Repository metadata disappears when PR and worktree counts are both zero. Header controls use compact sizing to protect repo names. Returned subagents fold into an expandable count independently of running work.

The mobile update banner sits below the app header to keep its small text away
from the translucent iOS status edge. It uses `--mobile-update-foreground`:
black on Commander's blue accent and white on Paper's darker blue. The pressed
state changes the background to `--accent-hover` instead of fading the whole
strip. Do not reuse Commander's shared white accent foreground here: at 13px,
its 3.36:1 contrast does not meet WCAG AA.

Paper mobile question-banner names use `--warning`; Input badge labels use
`--fg-primary` over the existing attention tint. The Paper Input badge does not
fade: whole-badge opacity would reduce small-text contrast below WCAG AA.
Commander retains its urgent/attention foregrounds and pulse.

### Constrained layouts and theme surfaces

- Detached Files and Markdown panels fill their window; inline panels retain their docked width limits.
- Dialog footer hints wrap as complete groups, with gaps between groups and actions. Action labels do not wrap letter by letter.
- Task field labels align at the top when neighboring controls have different heights. Roadmap state labels use ellipsis inside short bars.
- Native controls use the light color scheme on the Paper theme. Mobile bottom tabs and diff gutters use the existing application surface tokens.

`ui/ColorSwatch` is the shared native button for immediate preset/clear color selection.
It keeps selection in the owner through `selected`/`aria-pressed`, defaults to
`type="button"`, forwards native refs/events/disabled, and uses a 36px circular
target with a 44px touch minimum. Set `swatchColor` for a preset; omit it for
the dashed clear target. Give each target an accessible name. Feature CSS may
arrange the wrapping picker but must not resize its swatches.

### Shared edge actions and content-sized editors

- Use `Button geometry="stretch"` for square-edged container-height actions.
  Keep placement in the consumer; do not restore feature-owned control sizes.
- Use `ButtonGroup` with direct shared `Button` children for connected split
  actions. Buttons retain their own size and behavior; the shared group owns
  corners and the single divider. Do not clip focus rings or dropdown content.
- Use `Textarea autoGrow` for compact content-sized editors. The shared minimum
  is 36px desktop or 44px touch, and the maximum is three control heights.
  Remove consumer auto-resize callbacks and height caps when adopting it.

### Shared validation and removable toggles

- Use native `aria-invalid="true"` on `Input`, `Select` and `Textarea` for an
  error-token border. Link existing validation text with `aria-describedby`.
  The shared keyboard focus outline remains visible and unchanged.
- Use `Button variant="removalToggle"` only for an on/off action where clicking
  the pressed state removes or uninstalls the active integration. Set native
  `aria-pressed` from owner state. Pressed uses the success border; enabled
  pressed hover uses the danger tone. Disabled actions keep their state tone.
  Ordinary selection buttons retain their existing accent styling.

### Icon-only actions

Use `Button shape="icon" aria-label="Describe the action"` for an icon-only action.
Use `circular` when the action needs a circular visual. The native hit region stays
square at 36 × 36px in forms, 22 × 22px in toolbar rows, and 44 × 44px at touch widths; only the inner
surface is circular. `size="compact"` alone does not shrink a standalone icon target; a toolbar row selects toolbar density for all its controls. Icon mode
cannot combine with stretch geometry. Supply one icon or glyph and a nonempty,
descriptive `aria-label` (required by the TypeScript API); a tooltip alone is not
an accessible name. Do not add feature-level sizing or rounding rules.
