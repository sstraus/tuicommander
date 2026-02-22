import type { ITheme } from "@xterm/xterm";

/** Available terminal color themes */
export const TERMINAL_THEMES: Record<string, ITheme> = {
  "commander": {
    background: "#1e1e1e",
    foreground: "#d4d4d4",
    cursor: "#d4d4d4",
    cursorAccent: "#1e1e1e",
    selectionBackground: "#264f78",
    black: "#1e1e1e",
    red: "#f14c4c",
    green: "#23d18b",
    yellow: "#e5e510",
    blue: "#3b8eea",
    magenta: "#d670d6",
    cyan: "#29b8db",
    white: "#d4d4d4",
    brightBlack: "#666666",
    brightRed: "#f14c4c",
    brightGreen: "#23d18b",
    brightYellow: "#f5f543",
    brightBlue: "#3b8eea",
    brightMagenta: "#d670d6",
    brightCyan: "#29b8db",
    brightWhite: "#ffffff",
  },
  "vscode-dark": {
    background: "#1e1e1e",
    foreground: "#cccccc",
    cursor: "#cccccc",
    cursorAccent: "#1e1e1e",
    selectionBackground: "#264f78",
    black: "#000000",
    red: "#cd3131",
    green: "#0dbc79",
    yellow: "#e5e510",
    blue: "#2472c8",
    magenta: "#bc3fbc",
    cyan: "#11a8cd",
    white: "#e5e5e5",
    brightBlack: "#666666",
    brightRed: "#f14c4c",
    brightGreen: "#23d18b",
    brightYellow: "#f5f543",
    brightBlue: "#3b8eea",
    brightMagenta: "#d670d6",
    brightCyan: "#29b8db",
    brightWhite: "#ffffff",
  },
  "tokyo-night": {
    background: "#1a1b26",
    foreground: "#a9b1d6",
    cursor: "#c0caf5",
    cursorAccent: "#1a1b26",
    selectionBackground: "#33467c",
    black: "#15161e",
    red: "#f7768e",
    green: "#9ece6a",
    yellow: "#e0af68",
    blue: "#7aa2f7",
    magenta: "#bb9af7",
    cyan: "#7dcfff",
    white: "#a9b1d6",
    brightBlack: "#414868",
    brightRed: "#f7768e",
    brightGreen: "#9ece6a",
    brightYellow: "#e0af68",
    brightBlue: "#7aa2f7",
    brightMagenta: "#bb9af7",
    brightCyan: "#7dcfff",
    brightWhite: "#c0caf5",
  },
  "vscode-light": {
    background: "#ffffff",
    foreground: "#333333",
    cursor: "#333333",
    cursorAccent: "#ffffff",
    selectionBackground: "#add6ff",
    black: "#000000",
    red: "#cd3131",
    green: "#107c10",
    yellow: "#949800",
    blue: "#0451a5",
    magenta: "#bc05bc",
    cyan: "#0598bc",
    white: "#555555",
    brightBlack: "#666666",
    brightRed: "#cd3131",
    brightGreen: "#14ce14",
    brightYellow: "#b5ba00",
    brightBlue: "#0451a5",
    brightMagenta: "#bc05bc",
    brightCyan: "#0598bc",
    brightWhite: "#a5a5a5",
  },
  "dracula": {
    background: "#282a36",
    foreground: "#f8f8f2",
    cursor: "#f8f8f2",
    cursorAccent: "#282a36",
    selectionBackground: "#44475a",
    black: "#21222c",
    red: "#ff5555",
    green: "#50fa7b",
    yellow: "#f1fa8c",
    blue: "#bd93f9",
    magenta: "#ff79c6",
    cyan: "#8be9fd",
    white: "#f8f8f2",
    brightBlack: "#6272a4",
    brightRed: "#ff6e6e",
    brightGreen: "#69ff94",
    brightYellow: "#ffffa5",
    brightBlue: "#d6acff",
    brightMagenta: "#ff92df",
    brightCyan: "#a4ffff",
    brightWhite: "#ffffff",
  },
  "monokai": {
    background: "#272822",
    foreground: "#f8f8f2",
    cursor: "#f8f8f0",
    cursorAccent: "#272822",
    selectionBackground: "#49483e",
    black: "#272822",
    red: "#f92672",
    green: "#a6e22e",
    yellow: "#f4bf75",
    blue: "#66d9ef",
    magenta: "#ae81ff",
    cyan: "#a1efe4",
    white: "#f8f8f2",
    brightBlack: "#75715e",
    brightRed: "#f92672",
    brightGreen: "#a6e22e",
    brightYellow: "#f4bf75",
    brightBlue: "#66d9ef",
    brightMagenta: "#ae81ff",
    brightCyan: "#a1efe4",
    brightWhite: "#f9f8f5",
  },
  "catppuccin-mocha": {
    background: "#1e1e2e",
    foreground: "#cdd6f4",
    cursor: "#f5e0dc",
    cursorAccent: "#1e1e2e",
    selectionBackground: "#45475a",
    black: "#45475a",
    red: "#f38ba8",
    green: "#a6e3a1",
    yellow: "#f9e2af",
    blue: "#89b4fa",
    magenta: "#f5c2e7",
    cyan: "#94e2d5",
    white: "#bac2de",
    brightBlack: "#585b70",
    brightRed: "#f38ba8",
    brightGreen: "#a6e3a1",
    brightYellow: "#f9e2af",
    brightBlue: "#89b4fa",
    brightMagenta: "#f5c2e7",
    brightCyan: "#94e2d5",
    brightWhite: "#a6adc8",
  },
  "github-dark": {
    background: "#0d1117",
    foreground: "#c9d1d9",
    cursor: "#c9d1d9",
    cursorAccent: "#0d1117",
    selectionBackground: "#264f78",
    black: "#484f58",
    red: "#ff7b72",
    green: "#3fb950",
    yellow: "#d29922",
    blue: "#58a6ff",
    magenta: "#bc8cff",
    cyan: "#39c5cf",
    white: "#b1bac4",
    brightBlack: "#6e7681",
    brightRed: "#ffa198",
    brightGreen: "#56d364",
    brightYellow: "#e3b341",
    brightBlue: "#79c0ff",
    brightMagenta: "#d2a8ff",
    brightCyan: "#56d4dd",
    brightWhite: "#f0f6fc",
  },
  "solarized-dark": {
    background: "#002b36",
    foreground: "#839496",
    cursor: "#839496",
    cursorAccent: "#002b36",
    selectionBackground: "#073642",
    black: "#073642",
    red: "#dc322f",
    green: "#859900",
    yellow: "#b58900",
    blue: "#268bd2",
    magenta: "#d33682",
    cyan: "#2aa198",
    white: "#eee8d5",
    brightBlack: "#586e75",
    brightRed: "#cb4b16",
    brightGreen: "#586e75",
    brightYellow: "#657b83",
    brightBlue: "#839496",
    brightMagenta: "#6c71c4",
    brightCyan: "#93a1a1",
    brightWhite: "#fdf6e3",
  },
  "nord": {
    background: "#2e3440",
    foreground: "#d8dee9",
    cursor: "#d8dee9",
    cursorAccent: "#2e3440",
    selectionBackground: "#434c5e",
    black: "#3b4252",
    red: "#bf616a",
    green: "#a3be8c",
    yellow: "#ebcb8b",
    blue: "#81a1c1",
    magenta: "#b48ead",
    cyan: "#88c0d0",
    white: "#e5e9f0",
    brightBlack: "#4c566a",
    brightRed: "#bf616a",
    brightGreen: "#a3be8c",
    brightYellow: "#ebcb8b",
    brightBlue: "#81a1c1",
    brightMagenta: "#b48ead",
    brightCyan: "#8fbcbb",
    brightWhite: "#eceff4",
  },
};

/** Display names for theme selector */
export const THEME_NAMES: Record<string, string> = {
  "commander": "Commander",
  "vscode-dark": "VS Code Dark",
  "tokyo-night": "Tokyo Night",
  "vscode-light": "VS Code Light",
  "dracula": "Dracula",
  "monokai": "Monokai",
  "catppuccin-mocha": "Catppuccin Mocha",
  "github-dark": "GitHub Dark",
  "solarized-dark": "Solarized Dark",
  "nord": "Nord",
};

/** Get a theme by key, falling back to vscode-dark */
export function getTerminalTheme(key: string): ITheme {
  return TERMINAL_THEMES[key] ?? TERMINAL_THEMES["vscode-dark"];
}

/** Parse a hex color (#rrggbb) to [r, g, b] in 0–255 */
function hexToRgb(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  return [
    parseInt(h.slice(0, 2), 16),
    parseInt(h.slice(2, 4), 16),
    parseInt(h.slice(4, 6), 16),
  ];
}

/** WCAG 2.x relative luminance (0 = black, 1 = white) */
function relativeLuminance(hex: string): number {
  const [r, g, b] = hexToRgb(hex).map((c) => {
    const s = c / 255;
    return s <= 0.04045 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** WCAG contrast ratio between two hex colors (range 1–21) */
export function contrastRatio(hex1: string, hex2: string): number {
  const l1 = relativeLuminance(hex1);
  const l2 = relativeLuminance(hex2);
  const lighter = Math.max(l1, l2);
  const darker = Math.min(l1, l2);
  return (lighter + 0.05) / (darker + 0.05);
}

/** App-wide color scheme applied to the UI chrome (sidebar, tabs, toolbar, etc.) */
export interface IAppTheme {
  bgPrimary: string;
  bgSecondary: string;
  bgTertiary: string;
  bgHighlight: string;
  fgPrimary: string;
  fgSecondary: string;
  fgMuted: string;
  accent: string;
  accentHover: string;
  border: string;
  success: string;
  warning: string;
  error: string;
  textOnAccent: string;
  textOnError: string;
  textOnSuccess: string;
}

/** App chrome colors for each theme, derived from official palettes */
export const APP_THEMES: Record<string, IAppTheme> = {
  "commander": {
    bgPrimary: "#1b1b1b",
    bgSecondary: "#222222",
    bgTertiary: "#2a2a2a",
    bgHighlight: "#353535",
    fgPrimary: "#d4d4d4",
    fgSecondary: "#a0a0a0",
    fgMuted: "#737373",
    accent: "#2563b8",
    accentHover: "#3b8eea",
    border: "#2e2e2e",
    success: "#23d18b",
    warning: "#e5e510",
    error: "#f14c4c",
    textOnAccent: "#ffffff",
    textOnError: "#000000",
    textOnSuccess: "#000000",
  },
  "vscode-dark": {
    bgPrimary: "#1e1e1e",
    bgSecondary: "#252526",
    bgTertiary: "#2d2d30",
    bgHighlight: "#37373d",
    fgPrimary: "#cccccc",
    fgSecondary: "#a0a0a0",
    fgMuted: "#9aa1a9",
    accent: "#59a8dd",
    accentHover: "#7abde5",
    border: "#3e3e42",
    success: "#4ec9b0",
    warning: "#dcdcaa",
    error: "#f48771",
    textOnAccent: "#000000",
    textOnError: "#000000",
    textOnSuccess: "#000000",
  },
  "tokyo-night": {
    bgPrimary: "#1a1b26",
    bgSecondary: "#1f2335",
    bgTertiary: "#24283b",
    bgHighlight: "#292e42",
    fgPrimary: "#a9b1d6",
    fgSecondary: "#787c99",
    fgMuted: "#565a6e",
    accent: "#7aa2f7",
    accentHover: "#89b4fa",
    border: "#292e42",
    success: "#9ece6a",
    warning: "#e0af68",
    error: "#f7768e",
    textOnAccent: "#000000",
    textOnError: "#000000",
    textOnSuccess: "#000000",
  },
  "vscode-light": {
    bgPrimary: "#ffffff",
    bgSecondary: "#f8f8f8",
    bgTertiary: "#f3f3f3",
    bgHighlight: "#e8e8e8",
    fgPrimary: "#333333",
    fgSecondary: "#6f6f6f",
    fgMuted: "#767676",
    accent: "#005fb8",
    accentHover: "#0258a8",
    border: "#e5e5e5",
    success: "#2ea043",
    warning: "#895503",
    error: "#c72e0f",
    textOnAccent: "#ffffff",
    textOnError: "#ffffff",
    textOnSuccess: "#000000",
  },
  "dracula": {
    bgPrimary: "#282a36",
    bgSecondary: "#21222c",
    bgTertiary: "#343746",
    bgHighlight: "#44475a",
    fgPrimary: "#f8f8f2",
    fgSecondary: "#bfbfbf",
    fgMuted: "#6272a4",
    accent: "#bd93f9",
    accentHover: "#caa4fa",
    border: "#44475a",
    success: "#50fa7b",
    warning: "#f1fa8c",
    error: "#ff5555",
    textOnAccent: "#000000",
    textOnError: "#000000",
    textOnSuccess: "#000000",
  },
  "monokai": {
    bgPrimary: "#272822",
    bgSecondary: "#2d2e27",
    bgTertiary: "#3e3d32",
    bgHighlight: "#49483e",
    fgPrimary: "#f8f8f2",
    fgSecondary: "#c0c0b0",
    fgMuted: "#75715e",
    accent: "#66d9ef",
    accentHover: "#78e1f4",
    border: "#3e3d32",
    success: "#a6e22e",
    warning: "#f4bf75",
    error: "#f92672",
    textOnAccent: "#000000",
    textOnError: "#000000",
    textOnSuccess: "#000000",
  },
  "catppuccin-mocha": {
    bgPrimary: "#1e1e2e",
    bgSecondary: "#181825",
    bgTertiary: "#313244",
    bgHighlight: "#45475a",
    fgPrimary: "#cdd6f4",
    fgSecondary: "#a6adc8",
    fgMuted: "#585b70",
    accent: "#89b4fa",
    accentHover: "#9cc3fb",
    border: "#313244",
    success: "#a6e3a1",
    warning: "#f9e2af",
    error: "#f38ba8",
    textOnAccent: "#000000",
    textOnError: "#000000",
    textOnSuccess: "#000000",
  },
  "github-dark": {
    bgPrimary: "#0d1117",
    bgSecondary: "#161b22",
    bgTertiary: "#21262d",
    bgHighlight: "#30363d",
    fgPrimary: "#c9d1d9",
    fgSecondary: "#8b949e",
    fgMuted: "#6e7681",
    accent: "#58a6ff",
    accentHover: "#79c0ff",
    border: "#30363d",
    success: "#3fb950",
    warning: "#d29922",
    error: "#ff7b72",
    textOnAccent: "#000000",
    textOnError: "#000000",
    textOnSuccess: "#000000",
  },
  "solarized-dark": {
    bgPrimary: "#002b36",
    bgSecondary: "#073642",
    bgTertiary: "#0a4050",
    bgHighlight: "#1a5568",
    fgPrimary: "#839496",
    fgSecondary: "#657b83",
    fgMuted: "#586e75",
    accent: "#268bd2",
    accentHover: "#2e9fe0",
    border: "#0a4050",
    success: "#859900",
    warning: "#b58900",
    error: "#dc322f",
    textOnAccent: "#000000",
    textOnError: "#ffffff",
    textOnSuccess: "#000000",
  },
  "nord": {
    bgPrimary: "#2e3440",
    bgSecondary: "#3b4252",
    bgTertiary: "#434c5e",
    bgHighlight: "#4c566a",
    fgPrimary: "#d8dee9",
    fgSecondary: "#b0b8c8",
    fgMuted: "#7b88a1",
    accent: "#81a1c1",
    accentHover: "#8fabc8",
    border: "#4c566a",
    success: "#a3be8c",
    warning: "#ebcb8b",
    error: "#bf616a",
    textOnAccent: "#000000",
    textOnError: "#000000",
    textOnSuccess: "#000000",
  },
};

/** Get an app theme by key, falling back to vscode-dark */
export function getAppTheme(key: string): IAppTheme {
  return APP_THEMES[key] ?? APP_THEMES["vscode-dark"];
}

/** Convert camelCase property name to a CSS custom property (e.g. bgPrimary -> --bg-primary) */
function camelToKebab(str: string): string {
  return `--${str.replace(/[A-Z]/g, (m) => `-${m.toLowerCase()}`)}`;
}

/** Apply an app theme by setting CSS custom properties on the document root */
export function applyAppTheme(key: string): void {
  if (!(key in APP_THEMES)) {
    console.warn(`Unknown theme "${key}", falling back to vscode-dark`);
  }
  const theme = getAppTheme(key);
  const root = document.documentElement.style;
  for (const [prop, value] of Object.entries(theme)) {
    root.setProperty(camelToKebab(prop), value);
  }
}
