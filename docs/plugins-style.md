# Plugin Dashboard Style Guide

This document is the source of truth for plugin dashboard visuals. Every plugin that renders a dashboard (analytics, status, reports) MUST follow it so dashboards feel like a coherent part of TUICommander rather than a patchwork of third-party panels.

The reference implementation is the built-in **Claude Usage** dashboard (`src/components/ClaudeUsageDashboard/`). New plugin dashboards should visually match it at a glance.

## Rules

1. **Use the shared classes.** Do not hand-roll CSS for layout, cards, tables, stats, or typography. The classes below are injected into every plugin panel iframe via `PLUGIN_BASE_CSS` (`src/components/PluginPanel/pluginBaseStyles.ts`).
2. **Never hardcode colors.** Use CSS variables (`var(--accent)`, `var(--fg-primary)`, `var(--bg-secondary)`, …). These follow the active theme.
3. **Never hardcode pixel fonts for common text.** Headings/body are sized by the base stylesheet.
4. **Register via `host.registerDashboard(...)`** so the Settings → Plugins row shows a one-click Dashboard button. Do not rely on context menus as the only entry point.

## Layout skeleton

```html
<div class="dashboard">
  <div class="dash-header">
    <h1 class="dash-title">My Plugin</h1>
    <button class="primary" id="refresh">Refresh</button>
  </div>

  <div class="dash-section">
    <h2 class="dash-section-title">
      Overview
      <span class="dash-section-hint">last 7 days</span>
    </h2>
    <div class="dash-stat-grid">
      <div class="dash-stat">
        <div class="dash-stat-label">Commands</div>
        <div class="dash-stat-value">1.2k</div>
        <div class="dash-stat-sub">+8% vs prev</div>
      </div>
      <!-- more .dash-stat cards … -->
    </div>
  </div>

  <div class="dash-section">
    <h2 class="dash-section-title">Details</h2>
    <table>
      <thead><tr><th>Name</th><th class="num">Count</th></tr></thead>
      <tbody>…</tbody>
    </table>
  </div>
</div>
```

## Class reference

| Class | Purpose |
|---|---|
| `.dashboard` | Outer flex container. Provides padding, gap, vertical stacking. |
| `.dash-header` | Top row with title + optional controls (refresh, selectors). |
| `.dash-title` | `18px / 600` title. |
| `.dash-subtitle` | Small muted subtitle (breadcrumb, repo name). |
| `.dash-section` | Logical group. Inside `.dashboard` they auto-space via `gap: 16px`. |
| `.dash-section-title` | Uppercase muted section label (`12px / 600`). |
| `.dash-section-hint` | Inline secondary hint next to a section title. |
| `.dash-stat-grid` | Auto-fill grid for headline numbers (`minmax(160px, 1fr)`). |
| `.dash-stat` | Single stat card. |
| `.dash-stat-label` | Uppercase 10px label. |
| `.dash-stat-value` | 22px tabular value. |
| `.dash-stat-sub` | Secondary caption under a value. |
| `.dash-meter` / `.dash-meter-fill` | Horizontal progress bar. Add `.ok`, `.warn`, or `.critical` for color. |
| `.num` | Right-aligned tabular cell (use inside table `<th>`/`<td>`). |

Generic classes from `PLUGIN_BASE_CSS` also apply inside dashboards: `.card`, `.badge`, `.empty-state`, `button.primary`, `.hint`, etc. See `pluginBaseStyles.ts`.

## Do

- Keep the `<style>` block in `buildPanelHtml()` **empty** unless you need plugin-specific visual tweaks that cannot be expressed via the standard classes.
- Place the refresh button inside `.dash-header`, right-aligned via `justify-content: space-between`.
- Use `<table>` + `.num` for tabular data. The base stylesheet already themes it correctly.
- Use `.dash-stat` cards for headline numbers, not custom grids.
- Use `.empty-state` for "no data yet" screens.

## Don't

- Don't redefine `.card`, `.stat-card`, `.stat-grid`, `h1`/`h2`/`h3` sizes. These are global.
- Don't introduce custom color tokens — pick one of: `--accent`, `--success`, `--warning`, `--error`, `--fg-primary`, `--fg-secondary`, `--fg-muted`, `--bg-primary`, `--bg-secondary`, `--bg-tertiary`, `--border`.
- Don't set explicit `background: #...` anywhere.
- Don't wrap the dashboard in max-width containers narrower than the panel — let it fill the iframe.
- Don't use emoji icons. Use monochrome inline SVGs with `fill="currentColor"` (TUICommander convention).

## Registering the dashboard

```js
export default {
  id: "my-plugin",
  async onload(host) {
    // ... existing capability setup ...
    host.registerDashboard({
      label: "My Plugin",      // optional — defaults to "Dashboard"
      icon: MY_PLUGIN_ICON,    // optional inline SVG
      open: () => openDashboard(host),
    });
  },
};
```

The registered entry powers the **Dashboard** button in *Settings → Plugins* next to the plugin's enable toggle. The host also closes the Settings panel automatically on click so the dashboard becomes visible.

## Checklist before shipping

- [ ] Dashboard uses `.dashboard` + `.dash-*` classes — no duplicated layout CSS
- [ ] No hardcoded colors or font sizes for common elements
- [ ] Registered via `host.registerDashboard(...)`
- [ ] Empty/error states use `.empty-state` + generic `.card.error-card` if applicable
- [ ] Verified visually against `ClaudeUsageDashboard` side-by-side
