---
name: TUICommander
description: AI-native IDE for multi-agent development
colors:
  observation-blue: "#59a8dd"
  observation-blue-hover: "#7abde5"
  cool-stone: "#1e1e1e"
  slate-surface: "#252526"
  slate-panel: "#2d2d30"
  slate-highlight: "#37373d"
  silver-text: "#cccccc"
  muted-silver: "#a0a0a0"
  graphite-dim: "#9aa1a9"
  steel-border: "#3e3e42"
  success-green: "#4ade80"
  warning-amber: "#dcdcaa"
  attention-orange: "#e8984c"
  error-red: "#ef4444"
  merged-violet: "#a371f7"
  unseen-purple: "#c084fc"
typography:
  body:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Noto Sans', 'Liberation Sans', sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.5
  label:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Noto Sans', 'Liberation Sans', sans-serif"
    fontSize: "12px"
    fontWeight: 500
    lineHeight: 1.4
  mono:
    fontFamily: "'JetBrains Mono', 'Fira Code', 'Hack', 'Cascadia Code', 'Source Code Pro', 'DejaVu Sans Mono', monospace"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.5
rounded:
  xs: "2px"
  sm: "3px"
  md: "4px"
  lg: "6px"
  xl: "8px"
  pill: "12px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "16px"
  lg: "24px"
  xl: "32px"
---

# Design System: TUICommander

## 1. Overview

**Creative North Star: "The Lab Workbench"**

TUICommander's design system is technical clarity achieved through earned simplicity. This is the workbench where experiments run in parallel, results visible at a glance. Every interface element answers "what's happening?" or "what can I do?" with precision. The system rejects decoration that doesn't inform, roundedness that softens edges for consumer comfort, and the ubiquitous Electron-IDE layout cliché (sidebar-left, tabs-top, panels-bottom).

Density is correct when users monitor many parallel sessions at once, but density without hierarchy is chaos. Typography scale, spacing rhythm, and restrained color use guide the eye. The UI assumes technical fluency: no tutorial overlays, no "simple mode" hiding power behind gentleness. Labels are direct, affordances are clear, and the interface stays out of the way until you need it.

Built for people who outgrew tmux and manual window juggling. Tools for people who build tools.

**Platform:** Desktop only (Tauri v2 webview). No responsive breakpoints, no mobile touch targets, no viewport adaptation. The `src/mobile/` directory is a separate Capacitor companion app with its own design constraints outside this system's scope.

**Key Characteristics:**
- **Flat at rest, layered by state.** Elevation signals function (dropdown, modal, overlay), not decoration.
- **Mono-forward.** Terminal content dominates; JetBrains Mono is the primary face. System sans for UI chrome.
- **Restrained accent.** Observation Blue carries ≤10% of any surface. Its rarity is the point.
- **Tactile affordances.** Buttons, inputs, and interactive elements feel engineered with crisp state feedback.
- **No lost threads.** Status dots, badges, and color-coded indicators reduce "where was that?" friction.

## 2. Colors

A palette of cool, desaturated neutrals with a single technical accent. Dark by default (developers monitoring agents at 2am on a 27-inch display in a dim room).

### Primary
- **Observation Blue** (#59a8dd): The calm blue of a monitoring LED. Used for the primary accent (focused tabs, active states, primary buttons). Intentionally restrained; presence without urgency. Limited to ≤10% of any screen.
- **Observation Blue Hover** (#7abde5): Slightly lighter for hover feedback.

### Neutral
- **Cool Stone** (#1e1e1e): Primary background for terminal panes and main content areas.
- **Slate Surface** (#252526): Secondary background for UI chrome (sidebar, tab bar, panels).
- **Slate Panel** (#2d2d30): Tertiary background for elevated panels and nested containers.
- **Slate Highlight** (#37373d): Highlight state for rows, list items, hover backgrounds.
- **Silver Text** (#cccccc): Primary foreground text color.
- **Muted Silver** (#a0a0a0): Secondary text, labels, less prominent information.
- **Graphite Dim** (#9aa1a9): Tertiary text, disabled states, subtle hints.
- **Steel Border** (#3e3e42): Default border color for dividers, panel edges, input strokes.

### Semantic
- **Success Green** (#4ade80): Git additions, successful operations, "done" badges.
- **Warning Amber** (#dcdcaa): Warnings, non-critical alerts, awaiting-input states.
- **Attention Orange** (#e8984c): Moderate urgency, "needs review" indicators.
- **Error Red** (#ef4444): Git deletions, errors, failed operations, critical alerts.
- **Merged Violet** (#a371f7): Merged PRs, completed branches.
- **Unseen Purple** (#c084fc): Terminal completed while user wasn't viewing (notification badge).

### Named Rules

**The One Voice Rule.** Observation Blue is used on ≤10% of any given screen. Its rarity makes it effective for signaling focus, active state, and primary actions. Overuse dilutes the signal.

**The Tint Rule.** Every neutral (Cool Stone, Slate Surface, Slate Panel) has an imperceptible blue tint (chroma ~0.005). Pure grayscale (#000, #333, #555) is prohibited; it reads as lifeless next to the accent.

## 3. Typography

**UI Font:** System sans stack (-apple-system, BlinkMacSystemFont, Segoe UI, Roboto)  
**Mono Font:** JetBrains Mono (primary), Fira Code, Hack, Cascadia Code (fallbacks)

**Character:** Mono-forward. Terminal content dominates screen real estate, so JetBrains Mono is the primary typeface by visual weight. System sans handles UI chrome (labels, buttons, sidebar text) with restraint. No display font, no decorative weights. Hierarchy through size + weight contrast only.

### Hierarchy
- **Body** (400, 14px, 1.5): Default UI text. Labels, panel content, settings descriptions.
- **Label** (500, 12px, 1.4): Uppercase or mixed-case labels for buttons, tabs, form fields. Slightly tighter line-height for compactness.
- **Mono** (400, 13px, 1.5): Terminal output, code editor, diff views, file paths, git hashes. Sized one step smaller than Body for density in text-heavy terminal content.

### Named Rules

**The Mono Primary Rule.** JetBrains Mono is not an accent face for code snippets; it's the primary face by visual weight. The terminal pane occupies 60–80% of screen area in typical use. System sans is the support player.

**The Scale Compression Rule.** Font sizes range 12–14px for UI chrome. No large display sizes; TUICommander has no hero headlines or marketing copy. The scale is compressed by design to maximize information density.

## 4. Elevation

Structural, not decorative. Shadows signal functional elevation (dropdown menus, modal overlays, context menus). Default surfaces are flat at rest.

### Shadow Vocabulary
- **Popup Shadow** (`0 8px 32px rgba(0, 0, 0, 0.4)`): Full-screen modals, large overlays, detached panels. Deep, diffuse shadow for clear separation from underlying content.
- **Dropdown Shadow** (`0 4px 16px rgba(0, 0, 0, 0.3)`): Context menus, select dropdowns, tooltips. Medium shadow for transient overlays.
- **Bottom-Anchor Shadow** (`0 -4px 20px rgba(0, 0, 0, 0.4)`): Bottom-docked panels or drawers that slide up. Inverted shadow (negative Y offset) for upward elevation cue.

### Named Rules

**The Flat-By-Default Rule.** Cards, panels, and containers at rest have no shadow. Elevation is state-driven: a dropdown appears on click, a modal overlays the app on open. Shadows are never used for "card depth" decoration.

**The No-Glassmorphism Rule.** Blurs and translucent glass effects are prohibited. TUICommander surfaces are opaque. Overlays use solid backgrounds + shadows, not backdrop-filter.

## 5. Components

Tactile and confident. Clear affordances, strong state feedback, crisp edges. Components feel engineered, not designed. Every interactive element gives immediate visual response (hover, focus, active).

### Buttons
- **Shape:** Minimal radius (3–4px), no pill shapes. Buttons are rectangular with subtle corners.
- **Primary:** Observation Blue background (#59a8dd), black text (#000000), padding 8px 16px. Uppercase label text (Label hierarchy, 12px, 500 weight).
- **Hover:** Background shifts to Observation Blue Hover (#7abde5).
- **Secondary / Ghost:** Transparent background, Observation Blue border (1px), Observation Blue text. Hover fills background with Slate Highlight (#37373d).
- **Focus:** 2px solid outline in Observation Blue, 2px offset for accessibility.

### Inputs / Fields
- **Style:** 1px Steel Border stroke (#3e3e42), Slate Panel background (#2d2d30), 3px radius. Padding 6px 10px.
- **Focus:** Border shifts to Observation Blue (#59a8dd), no glow. Clean state transition.
- **Error:** Border shifts to Error Red (#ef4444).
- **Disabled:** Graphite Dim text (#9aa1a9), no background change.

### Tabs
- **Style:** 32px height, horizontal layout, no rounded top corners. Active tab has Slate Panel background (#2d2d30), inactive tabs have Slate Surface (#252526). 1px Steel Border bottom divider.
- **Active Indicator:** 2px solid Observation Blue bottom border on active tab.
- **Status Dots:** Small circular badges (6px diameter) in semantic colors (Success Green, Warning Amber, Error Red, Unseen Purple) appear on tab labels to signal state. Positioned top-right of tab text.
- **Hover:** Inactive tabs lighten to Slate Highlight (#37373d).

### Cards / Panels
- **Corner Style:** 4px radius (--radius-md).
- **Background:** Slate Panel (#2d2d30) for elevated panels, Slate Surface (#252526) for sidebar/chrome.
- **Shadow Strategy:** No shadow at rest. Use Popup Shadow only for detached floating panels (e.g., detached AI Chat window).
- **Border:** 1px Steel Border (#3e3e42) for panel edges, dividers between sections.
- **Internal Padding:** 8–16px depending on content density.

### Navigation (Sidebar)
- **Style:** Vertical list, Slate Surface background (#252526), 300px wide. Each item is 28px tall, left-aligned text (Body hierarchy, 14px).
- **Default State:** Muted Silver text (#a0a0a0).
- **Hover:** Slate Highlight background (#37373d), Silver Text (#cccccc).
- **Active:** Observation Blue left border (3px), Slate Panel background (#2d2d30), Silver Text.
- **Icons:** 16px monochrome icons (Muted Silver), shift to Silver Text on hover/active.

### Status Indicators
- **Dots:** 6–8px circular badges in semantic colors (Success, Warning, Error, Unseen, Merged). Used in tab bars, sidebar items, activity dashboard.
- **Badges:** Pill-shaped labels (--radius-pill), semantic background + white/black text depending on contrast. Small font (11px), 500 weight, uppercase.
- **Progress Bars:** 4px height, Slate Highlight track, Observation Blue fill. No stripes, no animation.

## 6. Do's and Don'ts

Concrete, forceful guardrails derived from PRODUCT.md anti-references and the design system's principles.

### Do:
- **Do** use Observation Blue sparingly (≤10% of any screen). Accent overuse dilutes signal.
- **Do** apply shadows only to functional overlays (dropdowns, modals, context menus). Never decorate cards with shadows.
- **Do** give every interactive element clear hover/focus states. Tactile feedback is non-negotiable.
- **Do** use status dots and semantic colors to reduce "where was that?" cognitive load. Make state visible at a glance.
- **Do** keep borders crisp (1px) and corners minimal (2–4px). TUICommander is precise, not soft.
- **Do** respect the mono-forward hierarchy. Terminal content is primary; UI chrome is support.

### Don't:
- **Don't** use pill-shaped buttons (--radius-pill or --radius-full on buttons). Buttons are rectangular with subtle corners.
- **Don't** apply gradient backgrounds or gradient text (`background-clip: text`). Solid colors only.
- **Don't** use glassmorphism (backdrop-filter, translucent overlays). All surfaces are opaque.
- **Don't** converge toward the VS Code / Electron-IDE layout reflex (sidebar-left, tabs-top, panels-bottom). TUICommander's layout is distinct.
- **Don't** soften edges with excessive rounding or rounded-everything. This is not a consumer app (no Slack-style chatty SaaS polish).
- **Don't** hide power behind "simple mode" or tutorial overlays. Users chose TUICommander because they outgrew simpler tools; trust them to learn.
- **Don't** use side-stripe borders (border-left > 1px as colored accent on cards/list items). Use full borders, background tints, or leading icons instead.
- **Don't** style for nostalgia (ncurses ASCII-art chrome, retro terminal aesthetics). Modern native UI with clean typography.
