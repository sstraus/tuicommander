import { Component, For } from "solid-js";
import s from "./UiLegend.module.css";

// ---------------------------------------------------------------------------
// Legend data
// ---------------------------------------------------------------------------

interface LegendEntry {
  color: string;
  label: string;
  description: string;
  pulsing?: boolean;
}

const TERMINAL_DOT_LEGEND: LegendEntry[] = [
  { color: "var(--fg-muted)", label: "Idle", description: "No session or never ran" },
  { color: "var(--accent)", label: "Busy", description: "Producing output", pulsing: true },
  { color: "var(--success)", label: "Done", description: "Command completed" },
  { color: "var(--unseen)", label: "Unseen", description: "Completed while not viewed" },
  { color: "var(--attention)", label: "Question", description: "Agent needs input", pulsing: true },
  { color: "var(--error)", label: "Error", description: "API error or agent stuck", pulsing: true },
];

interface TabTypeEntry {
  color: string;
  label: string;
  description: string;
}

const TAB_TYPE_LEGEND: TabTypeEntry[] = [
  { color: "rgb(var(--tab-diff-rgb))", label: "Diff", description: "Git diff viewer" },
  { color: "rgb(var(--tab-edit-rgb))", label: "Editor", description: "Code editor" },
  { color: "rgb(var(--tab-md-rgb))", label: "Markdown", description: "Markdown viewer" },
  { color: "rgb(var(--tab-panel-rgb))", label: "Panel", description: "Dashboard / plugin panel" },
  { color: "rgb(var(--tab-remote-rgb))", label: "PTY", description: "Remote session (HTTP/MCP)" },
];

const PANEL_COLOR_LEGEND: TabTypeEntry[] = [
  { color: "rgb(var(--tab-diff-rgb))", label: "Diff Panel", description: "Git diff summary" },
  { color: "rgb(var(--tab-md-rgb))", label: "Markdown Panel", description: "Markdown browser" },
  { color: "rgb(var(--tab-edit-rgb))", label: "File Browser", description: "File explorer" },
  { color: "rgb(var(--tab-panel-rgb))", label: "Panel", description: "Dashboard / plugin panel" },
];

interface SymbolEntry {
  symbol: string;
  label: string;
  description: string;
  color?: string;
}

const SIDEBAR_SYMBOL_LEGEND: SymbolEntry[] = [
  { symbol: "\u2731", label: "Main branch", description: "Primary branch (main/master)", color: "var(--warning)" },
  { symbol: "\u2387", label: "Feature branch", description: "Feature or topic branch", color: "var(--fg-muted)" },
  { symbol: "?", label: "Awaiting input", description: "A terminal needs input", color: "var(--warning)" },
];

interface BadgeEntry {
  label: string;
  description: string;
  bg: string;
  fg: string;
  border?: string;
  pulsing?: boolean;
}

const PR_BADGE_LEGEND: BadgeEntry[] = [
  { label: "#N", description: "Open PR (number)", bg: "var(--accent)", fg: "#000" },
  { label: "Ready", description: "Approved and mergeable", bg: "var(--success)", fg: "#000" },
  { label: "Draft", description: "PR is a draft", bg: "transparent", fg: "var(--fg-muted)", border: "var(--fg-muted)" },
  { label: "Conflicts", description: "Merge conflicts", bg: "var(--error)", fg: "#000", pulsing: true },
  { label: "CI Failed", description: "CI checks failed", bg: "var(--error)", fg: "#000" },
  { label: "Changes Req.", description: "Changes requested", bg: "#d29922", fg: "#000" },
  { label: "Review Req.", description: "Awaiting review", bg: "transparent", fg: "#d29922", border: "#d29922" },
  { label: "CI Running", description: "CI in progress", bg: "transparent", fg: "#e3b341", border: "#e3b341", pulsing: true },
  { label: "Merged", description: "PR merged", bg: "#a371f7", fg: "#000" },
];

const STATS_LEGEND: SymbolEntry[] = [
  { symbol: "+N", label: "Additions", description: "Lines added vs main", color: "var(--success)" },
  { symbol: "-N", label: "Deletions", description: "Lines removed vs main", color: "var(--error)" },
];


// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const UiLegend: Component = () => {
  return (
    <div class={s.legend}>
      {/* Terminal dot states */}
      <div class={s.group}>
        <label class={s.groupLabel}>Terminal Status Dots</label>
        <p class={s.hint}>The colored dot on each terminal tab</p>
        <div class={s.grid}>
          <For each={TERMINAL_DOT_LEGEND}>
            {(entry) => (
              <div class={s.row}>
                <span
                  class={entry.pulsing ? s.dotPulsing : s.dot}
                  style={{ background: entry.color }}
                />
                <span class={s.label}>{entry.label}</span>
                <span class={s.desc}>{entry.description}</span>
              </div>
            )}
          </For>
        </div>
      </div>

      {/* Tab type colors */}
      <div class={s.group}>
        <label class={s.groupLabel}>Tab Types</label>
        <p class={s.hint}>Background tint and bottom border color by tab type</p>
        <div class={s.grid}>
          <For each={TAB_TYPE_LEGEND}>
            {(entry) => (
              <div class={s.row}>
                <span class={s.colorBar} style={{ background: entry.color }} />
                <span class={s.label}>{entry.label}</span>
                <span class={s.desc}>{entry.description}</span>
              </div>
            )}
          </For>
        </div>
      </div>

      {/* Panel colors */}
      <div class={s.group}>
        <label class={s.groupLabel}>Panels</label>
        <p class={s.hint}>Right-side panel accent colors</p>
        <div class={s.grid}>
          <For each={PANEL_COLOR_LEGEND}>
            {(entry) => (
              <div class={s.row}>
                <span class={s.colorBar} style={{ background: entry.color }} />
                <span class={s.label}>{entry.label}</span>
                <span class={s.desc}>{entry.description}</span>
              </div>
            )}
          </For>
        </div>
      </div>

      {/* Sidebar branch icons */}
      <div class={s.group}>
        <label class={s.groupLabel}>Sidebar Symbols</label>
        <div class={s.grid}>
          <For each={SIDEBAR_SYMBOL_LEGEND}>
            {(entry) => (
              <div class={s.row}>
                <span class={s.symbol} style={{ color: entry.color }}>
                  {entry.symbol}
                </span>
                <span class={s.label}>{entry.label}</span>
                <span class={s.desc}>{entry.description}</span>
              </div>
            )}
          </For>
        </div>
      </div>

      {/* PR badges */}
      <div class={s.group}>
        <label class={s.groupLabel}>PR Status Badges</label>
        <p class={s.hint}>Shown next to branches with a pull request</p>
        <div class={s.grid}>
          <For each={PR_BADGE_LEGEND}>
            {(entry) => (
              <div class={s.row}>
                <span
                  class={entry.pulsing ? s.badgePulsing : s.badge}
                  style={{
                    background: entry.bg,
                    color: entry.fg,
                    border: entry.border ? `1px solid ${entry.border}` : undefined,
                  }}
                >
                  {entry.label}
                </span>
                <span class={s.desc}>{entry.description}</span>
              </div>
            )}
          </For>
        </div>
      </div>

      {/* Stats */}
      <div class={s.group}>
        <label class={s.groupLabel}>Diff Stats</label>
        <div class={s.grid}>
          <For each={STATS_LEGEND}>
            {(entry) => (
              <div class={s.row}>
                <span class={s.symbol} style={{ color: entry.color }}>
                  {entry.symbol}
                </span>
                <span class={s.label}>{entry.label}</span>
                <span class={s.desc}>{entry.description}</span>
              </div>
            )}
          </For>
        </div>
      </div>

    </div>
  );
};
