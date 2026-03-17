/**
 * Base CSS stylesheet injected into every plugin panel iframe.
 *
 * Provides a consistent foundation so plugins don't need to reinvent
 * typography, colors, buttons, inputs, cards, and tables. All values
 * use the CSS custom properties already injected by extractThemeVars().
 *
 * Plugins can override any of these styles — this is a default, not a cage.
 */

export const PLUGIN_BASE_CSS = `
/* ── Reset ── */
*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

/* ── Root ── */
body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Noto Sans, Liberation Sans, sans-serif;
  font-size: 13px;
  line-height: 1.5;
  color: var(--fg-primary, #ccc);
  background: var(--bg-primary, #1e1e1e);
  overflow-x: hidden;
}

/* ── Typography ── */
h1, h2, h3, h4, h5, h6 {
  color: var(--fg-primary, #ccc);
  font-weight: 600;
  line-height: 1.3;
}
h1 { font-size: 20px; margin-bottom: 12px; }
h2 { font-size: 17px; margin-bottom: 8px; }
h3 { font-size: 15px; margin-bottom: 6px; }
h4 { font-size: 13px; margin-bottom: 4px; }

p { margin-bottom: 8px; }
small { font-size: 11px; color: var(--fg-secondary, #a0a0a0); }
code {
  font-family: "JetBrains Mono", "Fira Code", "Cascadia Code", "Source Code Pro", monospace;
  font-size: 12px;
  background: var(--bg-tertiary, #2d2d30);
  padding: 1px 4px;
  border-radius: 3px;
}
a {
  color: var(--accent, #59a8dd);
  text-decoration: none;
}
a:hover {
  color: var(--accent-hover, #7abde5);
  text-decoration: underline;
}

/* ── Buttons ── */
button, .btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 4px 10px;
  font-size: 12px;
  font-weight: 500;
  font-family: inherit;
  color: var(--fg-primary, #ccc);
  background: var(--bg-tertiary, #2d2d30);
  border: 1px solid var(--border, #3e3e42);
  border-radius: 4px;
  cursor: pointer;
  transition: background 0.1s, border-color 0.15s;
  line-height: 1.4;
}
button:hover, .btn:hover {
  background: var(--bg-highlight, #37373d);
}
button:active, .btn:active {
  background: var(--bg-secondary, #252526);
}
button.primary, .btn-primary {
  background: var(--accent, #59a8dd);
  color: var(--text-on-accent, #000);
  border-color: var(--accent, #59a8dd);
}
button.primary:hover, .btn-primary:hover {
  background: var(--accent-hover, #7abde5);
  border-color: var(--accent-hover, #7abde5);
}
button.danger, .btn-danger {
  background: transparent;
  color: var(--error, #f48771);
  border-color: var(--error, #f48771);
}
button.danger:hover, .btn-danger:hover {
  background: var(--error, #f48771);
  color: var(--text-on-error, #000);
}
button:disabled, .btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

/* ── Inputs ── */
input[type="text"], input[type="search"], input[type="number"],
input[type="email"], input[type="url"], input[type="password"],
textarea, select {
  padding: 4px 8px;
  font-size: 13px;
  font-family: inherit;
  color: var(--fg-primary, #ccc);
  background: var(--bg-tertiary, #2d2d30);
  border: 1px solid var(--border, #3e3e42);
  border-radius: 4px;
  outline: none;
  transition: border-color 0.15s;
}
input:focus, textarea:focus, select:focus {
  border-color: var(--accent, #59a8dd);
}
input::placeholder, textarea::placeholder {
  color: var(--fg-muted, #9aa1a9);
}

/* ── Checkboxes ── */
input[type="checkbox"] {
  accent-color: var(--accent, #59a8dd);
}

/* ── Cards ── */
.card {
  background: var(--bg-secondary, #252526);
  border: 1px solid var(--border, #3e3e42);
  border-radius: 4px;
  padding: 8px;
  transition: transform 0.1s, box-shadow 0.1s;
}
.card:hover {
  transform: translateY(-1px);
  box-shadow: 0 2px 6px rgba(0,0,0,0.3);
}

/* ── Tables ── */
table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}
th {
  text-align: left;
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--fg-secondary, #a0a0a0);
  padding: 6px 10px;
  border-bottom: 1px solid var(--border, #3e3e42);
}
td {
  padding: 6px 10px;
  border-bottom: 1px solid var(--border, #3e3e42);
  color: var(--fg-primary, #ccc);
}
tr:hover td {
  background: var(--bg-highlight, #37373d);
}

/* ── Badges ── */
.badge {
  display: inline-block;
  font-size: 10px;
  font-weight: 700;
  padding: 1px 6px;
  border-radius: 3px;
  line-height: 1.5;
}
.badge-p1, .badge-error {
  background: var(--error, #f48771);
  color: var(--text-on-error, #000);
}
.badge-p2, .badge-warning {
  background: var(--warning, #dcdcaa);
  color: var(--text-on-accent, #000);
}
.badge-p3, .badge-muted {
  background: var(--fg-muted, #9aa1a9);
  color: var(--text-on-accent, #000);
}
.badge-success {
  background: var(--success, #4ec9b0);
  color: var(--text-on-success, #000);
}
.badge-accent {
  background: var(--accent, #59a8dd);
  color: var(--text-on-accent, #000);
}

/* ── Labels ── */
label {
  display: block;
  font-size: 12px;
  font-weight: 500;
  color: var(--fg-secondary, #a0a0a0);
  margin-bottom: 4px;
}
.hint {
  font-size: 11px;
  color: var(--fg-muted, #9aa1a9);
  margin-top: 2px;
}

/* ── Dividers ── */
hr {
  border: none;
  border-top: 1px solid var(--border, #3e3e42);
  margin: 12px 0;
}

/* ── Filter bar (common pattern) ── */
.filter-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border, #3e3e42);
  flex-shrink: 0;
}

/* ── Empty state ── */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 32px 16px;
  color: var(--fg-muted, #9aa1a9);
  font-size: 14px;
  text-align: center;
}
.empty-state .hint {
  margin-top: 8px;
  font-size: 12px;
  opacity: 0.7;
}

/* ── Toast notifications ── */
.toast {
  position: fixed;
  bottom: 12px;
  left: 50%;
  transform: translateX(-50%);
  padding: 6px 14px;
  border-radius: 4px;
  font-size: 12px;
  font-weight: 600;
  opacity: 0;
  transition: opacity 0.2s;
  pointer-events: none;
  z-index: 100;
}
.toast.show { opacity: 1; }
.toast.error {
  background: var(--error, #f48771);
  color: var(--text-on-error, #000);
}
.toast.success {
  background: var(--success, #4ec9b0);
  color: var(--text-on-success, #000);
}

/* ── Scrollbar styling ── */
::-webkit-scrollbar { width: 8px; height: 8px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb {
  background: var(--border, #3e3e42);
  border-radius: 4px;
}
::-webkit-scrollbar-thumb:hover {
  background: var(--fg-muted, #9aa1a9);
}
`;
