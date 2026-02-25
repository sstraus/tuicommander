import { Component, For, Show } from "solid-js";
import { notificationsStore } from "../../../stores/notifications";
import type { NotificationSound } from "../../../notifications";
import { t } from "../../../i18n";
import s from "../Settings.module.css";

// ---------------------------------------------------------------------------
// Sound pattern visualizations (inline SVG showing pitch contour)
// ---------------------------------------------------------------------------

/** Mini musical staff showing the note pattern for each sound.
 *  5 staff lines, note heads positioned by pitch, stems going up. */
function SoundPatternSvg(props: { sound: NotificationSound }) {
  // Staff: 5 lines from y=4 to y=20, spaced 4px apart
  // Note positions: y maps to pitch (lower y = higher pitch)
  // Each note: x position, y (staff position), filled noteHead
  const patterns: Record<NotificationSound, { x: number; y: number }[]> = {
    question:   [{ x: 14, y: 16 }, { x: 30, y: 8 }],             // C5 → E5 ascending
    completion: [{ x: 10, y: 16 }, { x: 24, y: 10 }, { x: 38, y: 4 }], // C5 → E5 → G5
    error:      [{ x: 14, y: 8 }, { x: 30, y: 16 }],             // E4 → C4 descending
    warning:    [{ x: 14, y: 12 }, { x: 30, y: 12 }],            // A4 × 2 same pitch
    info:       [{ x: 22, y: 4 }],                                 // G5 single note
  };

  const colors: Record<NotificationSound, string> = {
    question: "var(--warning)",
    completion: "var(--success)",
    error: "var(--error)",
    warning: "var(--accent)",
    info: "var(--fg-muted)",
  };

  const notes = patterns[props.sound];
  const color = colors[props.sound];
  const w = props.sound === "completion" ? 36 : props.sound === "info" ? 24 : 32;

  return (
    <svg viewBox={`0 0 ${w} 18`} width={w} height="14" style={{ "vertical-align": "middle", "flex-shrink": "0" }}>
      {/* Staff lines */}
      <For each={[3, 6, 9, 12, 15]}>
        {(ly) => (
          <line x1="1" y1={ly} x2={w - 1} y2={ly} stroke="var(--border)" stroke-width="0.4" />
        )}
      </For>
      {/* Note heads + stems */}
      <For each={notes}>
        {(note) => {
          const sy = (note.y / 20) * 15;
          const sx = (note.x / 48) * w;
          return (
            <>
              <ellipse cx={sx} cy={sy} rx="2.5" ry="1.8" fill={color} transform={`rotate(-15 ${sx} ${sy})`} />
              <line x1={sx + 2.3} y1={sy - 0.5} x2={sx + 2.3} y2={sy - 7} stroke={color} stroke-width="0.7" />
            </>
          );
        }}
      </For>
    </svg>
  );
}

// ---------------------------------------------------------------------------
// Legend data
// ---------------------------------------------------------------------------

interface LegendEntry {
  color: string;
  label: string;
  description: string;
  pulsing?: boolean;
}

/** Terminal tab dot colors — the small circle on the left of each terminal tab */
const TERMINAL_DOT_LEGEND: LegendEntry[] = [
  { color: "var(--fg-muted)", label: "Idle", description: "No activity" },
  { color: "var(--accent)", label: "Activity", description: "Producing output", pulsing: true },
  { color: "var(--success)", label: "Done", description: "Command completed" },
  { color: "var(--warning)", label: "Input", description: "Needs user input", pulsing: true },
  { color: "var(--error)", label: "Error", description: "Error state" },
];

/** Tab type colors — the background tint and bottom border that distinguish tab types */
interface TabTypeEntry {
  color: string;
  label: string;
  description: string;
}

const TAB_TYPE_LEGEND: TabTypeEntry[] = [
  { color: "#ef4444", label: "Diff", description: "Git diff viewer" },
  { color: "#7aa2f7", label: "Editor", description: "Code editor" },
  { color: "#22c55e", label: "Markdown", description: "Markdown viewer" },
  { color: "#a78bfa", label: "Panel", description: "Dashboard / plugin panel" },
];

/** Panel colors — the accent color of right-side panels */
const PANEL_COLOR_LEGEND: TabTypeEntry[] = [
  { color: "#ef4444", label: "Diff Panel", description: "Git diff summary" },
  { color: "#22c55e", label: "Markdown Panel", description: "Markdown browser" },
  { color: "#7aa2f7", label: "File Browser", description: "File explorer" },
  { color: "#a78bfa", label: "Panel", description: "Dashboard / plugin panel" },
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

export const NotificationsTab: Component = () => {
  const sounds: { key: NotificationSound; label: string }[] = [
    { key: "question", label: t("notifications.sound.question", "Question") },
    { key: "error", label: t("notifications.sound.error", "Error") },
    { key: "completion", label: t("notifications.sound.completion", "Completion") },
    { key: "warning", label: t("notifications.sound.warning", "Warning") },
    { key: "info", label: t("notifications.sound.info", "Info") },
  ];

  return (
    <>
      <div class={s.section}>
        <h3>{t("notifications.heading.notificationSettings", "Notification Settings")}</h3>

        <Show
          when={notificationsStore.state.isAvailable}
          fallback={
            <p class={s.warning}>
              {t("notifications.warning.notAvailable", "Audio notifications are not available on this platform")}
            </p>
          }
        >
          <div class={s.group}>
            <label>{t("notifications.label.enableAudio", "Enable Audio")}</label>
            <div class={s.toggle}>
              <input
                type="checkbox"
                checked={notificationsStore.state.config.enabled}
                onChange={(e) => notificationsStore.setEnabled(e.currentTarget.checked)}
              />
              <span>{t("notifications.toggle.enableAudio", "Enable audio notifications")}</span>
            </div>
          </div>

          <div class={s.group}>
            <label>{t("notifications.label.masterVolume", "Master Volume")}</label>
            <div class={s.slider}>
              <input
                type="range"
                min="0"
                max="100"
                value={Math.round(notificationsStore.state.config.volume * 100)}
                onInput={(e) => notificationsStore.setVolume(parseInt(e.currentTarget.value) / 100)}
              />
              <span>{Math.round(notificationsStore.state.config.volume * 100)}%</span>
            </div>
            <p class={s.hint}>{t("notifications.hint.masterVolume", "Overall volume for all notification sounds")}</p>
          </div>

          <div class={s.group}>
            <label>{t("notifications.label.notificationEvents", "Notification Events")}</label>
            <p class={s.hint} style={{ "margin-bottom": "12px" }}>
              {t("notifications.hint.notificationEvents", "Choose which events play a sound")}
            </p>
            <For each={sounds}>
              {(sound) => (
                <div class={s.soundRow}>
                  <div class={s.toggle}>
                    <input
                      type="checkbox"
                      checked={notificationsStore.state.config.sounds[sound.key]}
                      onChange={(e) =>
                        notificationsStore.setSoundEnabled(sound.key, e.currentTarget.checked)
                      }
                    />
                    <span>{sound.label}</span>
                  </div>
                  <SoundPatternSvg sound={sound.key} />
                  <button
                    class={s.testBtn}
                    onClick={() => notificationsStore.testSound(sound.key)}
                  >
                    {t("notifications.btn.test", "Test")}
                  </button>
                </div>
              )}
            </For>
          </div>

          <div class={s.actions}>
            <button onClick={() => notificationsStore.reset()}>{t("notifications.btn.resetDefaults", "Reset Defaults")}</button>
          </div>
        </Show>
      </div>

      {/* UI Legend */}
      <div class={s.section} style={{ "margin-top": "24px" }}>
        <h3>UI Legend</h3>

        {/* Terminal dot states */}
        <div class={s.group}>
          <label>Terminal Status Dots</label>
          <p class={s.hint} style={{ "margin-bottom": "8px" }}>
            The colored dot on each terminal tab
          </p>
          <div class={s.legendGrid}>
            <For each={TERMINAL_DOT_LEGEND}>
              {(entry) => (
                <div class={s.legendRow}>
                  <span
                    class={entry.pulsing ? s.legendDotPulsing : s.legendDot}
                    style={{ background: entry.color }}
                  />
                  <span class={s.legendLabel}>{entry.label}</span>
                  <span class={s.legendDesc}>{entry.description}</span>
                </div>
              )}
            </For>
          </div>
        </div>

        {/* Tab type colors */}
        <div class={s.group}>
          <label>Tab Types</label>
          <p class={s.hint} style={{ "margin-bottom": "8px" }}>
            Background tint and bottom border color by tab type
          </p>
          <div class={s.legendGrid}>
            <For each={TAB_TYPE_LEGEND}>
              {(entry) => (
                <div class={s.legendRow}>
                  <span class={s.legendColorBar} style={{ background: entry.color }} />
                  <span class={s.legendLabel}>{entry.label}</span>
                  <span class={s.legendDesc}>{entry.description}</span>
                </div>
              )}
            </For>
          </div>
        </div>

        {/* Panel colors */}
        <div class={s.group}>
          <label>Panels</label>
          <p class={s.hint} style={{ "margin-bottom": "8px" }}>
            Right-side panel accent colors
          </p>
          <div class={s.legendGrid}>
            <For each={PANEL_COLOR_LEGEND}>
              {(entry) => (
                <div class={s.legendRow}>
                  <span class={s.legendColorBar} style={{ background: entry.color }} />
                  <span class={s.legendLabel}>{entry.label}</span>
                  <span class={s.legendDesc}>{entry.description}</span>
                </div>
              )}
            </For>
          </div>
        </div>

        {/* Sidebar branch icons */}
        <div class={s.group}>
          <label>Sidebar Symbols</label>
          <div class={s.legendGrid}>
            <For each={SIDEBAR_SYMBOL_LEGEND}>
              {(entry) => (
                <div class={s.legendRow}>
                  <span class={s.legendSymbol} style={{ color: entry.color }}>
                    {entry.symbol}
                  </span>
                  <span class={s.legendLabel}>{entry.label}</span>
                  <span class={s.legendDesc}>{entry.description}</span>
                </div>
              )}
            </For>
          </div>
        </div>

        {/* PR badges */}
        <div class={s.group}>
          <label>PR Status Badges</label>
          <p class={s.hint} style={{ "margin-bottom": "8px" }}>
            Shown next to branches with a pull request
          </p>
          <div class={s.legendGrid}>
            <For each={PR_BADGE_LEGEND}>
              {(entry) => (
                <div class={s.legendRow}>
                  <span
                    class={entry.pulsing ? s.legendBadgePulsing : s.legendBadge}
                    style={{
                      background: entry.bg,
                      color: entry.fg,
                      "border": entry.border ? `1px solid ${entry.border}` : undefined,
                    }}
                  >
                    {entry.label}
                  </span>
                  <span class={s.legendDesc}>{entry.description}</span>
                </div>
              )}
            </For>
          </div>
        </div>

        {/* Stats */}
        <div class={s.group}>
          <label>Diff Stats</label>
          <div class={s.legendGrid}>
            <For each={STATS_LEGEND}>
              {(entry) => (
                <div class={s.legendRow}>
                  <span class={s.legendSymbol} style={{ color: entry.color }}>
                    {entry.symbol}
                  </span>
                  <span class={s.legendLabel}>{entry.label}</span>
                  <span class={s.legendDesc}>{entry.description}</span>
                </div>
              )}
            </For>
          </div>
        </div>
      </div>
    </>
  );
};
