use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A fully resolved theme entry ready for the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ThemeEntry {
    pub key: String,
    pub name: String,
    pub terminal: TerminalColors,
    pub app_chrome: AppChromeColors,
}

/// Terminal colors (ANSI 0-15 + special).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TerminalColors {
    pub background: String,
    pub foreground: String,
    pub cursor: String,
    pub cursor_accent: Option<String>,
    pub selection_background: Option<String>,
    pub ansi: [String; 16],
}

/// App chrome colors (mapped to internal IAppTheme field names).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AppChromeColors {
    pub bg_primary: String,
    pub bg_secondary: String,
    pub bg_tertiary: String,
    pub bg_highlight: String,
    pub fg_primary: String,
    pub fg_secondary: String,
    pub fg_muted: String,
    pub accent: String,
    pub accent_hover: String,
    pub border: String,
    pub success: String,
    pub warning: String,
    pub error: String,
    pub text_on_accent: String,
    pub text_on_error: String,
    pub text_on_success: String,
}

/// Raw JSON shape for Windows Terminal format + our extensions.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WtJson {
    name: Option<String>,
    // ANSI normal 0-7
    black: Option<String>,
    red: Option<String>,
    green: Option<String>,
    yellow: Option<String>,
    blue: Option<String>,
    purple: Option<String>,
    cyan: Option<String>,
    white: Option<String>,
    // ANSI bright 8-15
    bright_black: Option<String>,
    bright_red: Option<String>,
    bright_green: Option<String>,
    bright_yellow: Option<String>,
    bright_blue: Option<String>,
    bright_purple: Option<String>,
    bright_cyan: Option<String>,
    bright_white: Option<String>,
    // Special
    background: Option<String>,
    foreground: Option<String>,
    cursor_color: Option<String>,
    cursor_accent: Option<String>,
    selection_background: Option<String>,
    // Extended: app chrome (optional, uses standard naming on disk)
    app_chrome: Option<AppChromeJson>,
}

/// JSON shape for appChrome section (standard names on disk).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppChromeJson {
    background: Option<String>,
    surface: Option<String>,
    surface_elevated: Option<String>,
    highlight: Option<String>,
    foreground: Option<String>,
    foreground_secondary: Option<String>,
    muted_foreground: Option<String>,
    accent: Option<String>,
    accent_hover: Option<String>,
    border: Option<String>,
    success: Option<String>,
    warning: Option<String>,
    error: Option<String>,
    accent_foreground: Option<String>,
    error_foreground: Option<String>,
    success_foreground: Option<String>,
}

/// Load all theme JSON files from a directory.
pub(crate) fn load_themes(dir: &Path) -> Vec<ThemeEntry> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut themes: Vec<ThemeEntry> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                parse_theme_file(&path)
            } else {
                None
            }
        })
        .collect();
    themes.sort_by(|a, b| a.name.cmp(&b.name));
    themes
}

fn parse_theme_file(path: &Path) -> Option<ThemeEntry> {
    let content = std::fs::read_to_string(path).ok()?;
    let wt: WtJson = serde_json::from_str(&content).ok()?;

    let key = path.file_stem()?.to_str()?.to_lowercase();

    let bg = wt.background.as_deref().unwrap_or("#000000");
    let fg = wt.foreground.as_deref().unwrap_or("#ffffff");

    let ansi = [
        wt.black.clone().unwrap_or_else(|| "#000000".into()),
        wt.red.clone().unwrap_or_else(|| "#cc0000".into()),
        wt.green.clone().unwrap_or_else(|| "#00cc00".into()),
        wt.yellow.clone().unwrap_or_else(|| "#cccc00".into()),
        wt.blue.clone().unwrap_or_else(|| "#0000cc".into()),
        wt.purple.clone().unwrap_or_else(|| "#cc00cc".into()),
        wt.cyan.clone().unwrap_or_else(|| "#00cccc".into()),
        wt.white.clone().unwrap_or_else(|| "#cccccc".into()),
        wt.bright_black.clone().unwrap_or_else(|| "#666666".into()),
        wt.bright_red.clone().unwrap_or_else(|| "#ff0000".into()),
        wt.bright_green.clone().unwrap_or_else(|| "#00ff00".into()),
        wt.bright_yellow.clone().unwrap_or_else(|| "#ffff00".into()),
        wt.bright_blue.clone().unwrap_or_else(|| "#0000ff".into()),
        wt.bright_purple.clone().unwrap_or_else(|| "#ff00ff".into()),
        wt.bright_cyan.clone().unwrap_or_else(|| "#00ffff".into()),
        wt.bright_white.clone().unwrap_or_else(|| "#ffffff".into()),
    ];

    let display_name = wt.name.unwrap_or_else(|| {
        key.replace('-', " ")
            .split_whitespace()
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().to_string() + c.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    });

    let app_chrome = if let Some(ac) = wt.app_chrome {
        map_app_chrome_json(&ac)
    } else {
        derive_app_chrome(bg, fg, &ansi)
    };

    Some(ThemeEntry {
        key,
        name: display_name,
        terminal: TerminalColors {
            background: bg.to_string(),
            foreground: fg.to_string(),
            cursor: wt.cursor_color.unwrap_or_else(|| fg.to_string()),
            cursor_accent: wt.cursor_accent,
            selection_background: wt.selection_background,
            ansi,
        },
        app_chrome,
    })
}

fn map_app_chrome_json(ac: &AppChromeJson) -> AppChromeColors {
    AppChromeColors {
        bg_primary: ac.background.clone().unwrap_or_default(),
        bg_secondary: ac.surface.clone().unwrap_or_default(),
        bg_tertiary: ac.surface_elevated.clone().unwrap_or_default(),
        bg_highlight: ac.highlight.clone().unwrap_or_default(),
        fg_primary: ac.foreground.clone().unwrap_or_default(),
        fg_secondary: ac.foreground_secondary.clone().unwrap_or_default(),
        fg_muted: ac.muted_foreground.clone().unwrap_or_default(),
        accent: ac.accent.clone().unwrap_or_default(),
        accent_hover: ac.accent_hover.clone().unwrap_or_default(),
        border: ac.border.clone().unwrap_or_default(),
        success: ac.success.clone().unwrap_or_default(),
        warning: ac.warning.clone().unwrap_or_default(),
        error: ac.error.clone().unwrap_or_default(),
        text_on_accent: ac.accent_foreground.clone().unwrap_or_default(),
        text_on_error: ac.error_foreground.clone().unwrap_or_default(),
        text_on_success: ac.success_foreground.clone().unwrap_or_default(),
    }
}

// ---------------------------------------------------------------------------
// AppChrome derivation from terminal colors
// ---------------------------------------------------------------------------

fn parse_hex(hex: &str) -> (u8, u8, u8) {
    let h = hex.trim_start_matches('#');
    if h.len() < 6 {
        return (0, 0, 0);
    }
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(0);
    (r, g, b)
}

fn to_hex(r: u8, g: u8, b: u8) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn srgb_luminance(r: u8, g: u8, b: u8) -> f64 {
    fn linearize(c: u8) -> f64 {
        let s = c as f64 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * linearize(r) + 0.7152 * linearize(g) + 0.0722 * linearize(b)
}

fn is_light(hex: &str) -> bool {
    let (r, g, b) = parse_hex(hex);
    srgb_luminance(r, g, b) > 0.4
}

fn blend(c1: &str, c2: &str, t: f64) -> String {
    let (r1, g1, b1) = parse_hex(c1);
    let (r2, g2, b2) = parse_hex(c2);
    let r = (r1 as f64 * (1.0 - t) + r2 as f64 * t).round() as u8;
    let g = (g1 as f64 * (1.0 - t) + g2 as f64 * t).round() as u8;
    let b = (b1 as f64 * (1.0 - t) + b2 as f64 * t).round() as u8;
    to_hex(r, g, b)
}

fn adjust_brightness(hex: &str, factor: f64) -> String {
    let (r, g, b) = parse_hex(hex);
    let adjust = |c: u8| -> u8 {
        if factor > 0.0 {
            (c as f64 + (255.0 - c as f64) * factor).round().min(255.0) as u8
        } else {
            (c as f64 * (1.0 + factor)).round().max(0.0) as u8
        }
    };
    to_hex(adjust(r), adjust(g), adjust(b))
}

fn contrast_text(bg_hex: &str) -> String {
    let (r, g, b) = parse_hex(bg_hex);
    let lum = srgb_luminance(r, g, b);
    if lum > 0.18 {
        "#000000".into()
    } else {
        "#ffffff".into()
    }
}

fn derive_app_chrome(bg: &str, fg: &str, ansi: &[String; 16]) -> AppChromeColors {
    let light = is_light(bg);
    let dir = if light { -1.0 } else { 1.0 };

    let bg_secondary = adjust_brightness(bg, dir * 0.05);
    let bg_tertiary = adjust_brightness(bg, dir * 0.10);
    let bg_highlight = adjust_brightness(bg, dir * 0.15);
    let border_color = adjust_brightness(bg, dir * 0.08);
    let accent = &ansi[4]; // blue
    let accent_hover = adjust_brightness(accent, 0.12);

    AppChromeColors {
        bg_primary: bg.to_string(),
        bg_secondary,
        bg_tertiary,
        bg_highlight,
        fg_primary: fg.to_string(),
        fg_secondary: blend(fg, bg, 0.30),
        fg_muted: blend(fg, bg, 0.55),
        accent: accent.clone(),
        accent_hover,
        border: border_color,
        success: ansi[2].clone(), // green
        warning: ansi[3].clone(), // yellow
        error: ansi[1].clone(),   // red
        text_on_accent: contrast_text(accent),
        text_on_error: contrast_text(&ansi[1]),
        text_on_success: contrast_text(&ansi[2]),
    }
}

// ---------------------------------------------------------------------------
// Built-in theme seeding
// ---------------------------------------------------------------------------

const BUILTIN_THEMES: &[(&str, &str)] = &[
    ("commander.json", include_str!("themes/commander.json")),
    ("vscode-dark.json", include_str!("themes/vscode-dark.json")),
    ("tokyo-night.json", include_str!("themes/tokyo-night.json")),
    (
        "vscode-light.json",
        include_str!("themes/vscode-light.json"),
    ),
    ("dracula.json", include_str!("themes/dracula.json")),
    ("monokai.json", include_str!("themes/monokai.json")),
    (
        "catppuccin-mocha.json",
        include_str!("themes/catppuccin-mocha.json"),
    ),
    ("github-dark.json", include_str!("themes/github-dark.json")),
    (
        "solarized-dark.json",
        include_str!("themes/solarized-dark.json"),
    ),
    ("nord.json", include_str!("themes/nord.json")),
    ("darksun.json", include_str!("themes/darksun.json")),
    (
        "delicate-one.json",
        include_str!("themes/delicate-one.json"),
    ),
    ("deep-black.json", include_str!("themes/deep-black.json")),
    (
        "minimal-kiwi.json",
        include_str!("themes/minimal-kiwi.json"),
    ),
];

/// Seed the themes directory with built-in themes (only when the dir doesn't exist).
pub(crate) fn seed_builtin_themes(dir: &Path) -> std::io::Result<()> {
    if dir.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;
    for (filename, content) in BUILTIN_THEMES {
        std::fs::write(dir.join(filename), content)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Hot-reload watcher
// ---------------------------------------------------------------------------

#[cfg(feature = "desktop")]
pub(crate) fn start_theme_watcher(themes_dir: PathBuf, state: &std::sync::Arc<crate::AppState>) {
    use notify::{RecursiveMode, Watcher};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    let app_handle = state.app_handle.read().clone();
    let last_emit = std::sync::Arc::new(AtomicU64::new(0));

    let watcher =
        notify::recommended_watcher(move |result: Result<notify::Event, notify::Error>| {
            let event = match result {
                Ok(e) => e,
                Err(err) => {
                    tracing::warn!(source = "theme_watcher", "Watcher error: {err}");
                    return;
                }
            };

            let dominated_by_json = event
                .paths
                .iter()
                .any(|p| p.extension().is_some_and(|e| e == "json"));
            if !dominated_by_json {
                return;
            }

            // 500ms debounce
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_millis() as u64;
            let prev = last_emit.load(Ordering::Relaxed);
            if now_ms.saturating_sub(prev) < 500 {
                return;
            }
            last_emit.store(now_ms, Ordering::Relaxed);

            tracing::info!(
                source = "theme_watcher",
                "Themes directory changed, emitting themes-changed"
            );
            if let Some(ref app) = app_handle {
                use tauri::Emitter as _;
                let _ = app.emit("themes-changed", ());
            }
        });

    match watcher {
        Ok(mut w) => {
            if let Err(e) = w.watch(&themes_dir, RecursiveMode::NonRecursive) {
                tracing::warn!(source = "theme_watcher", "Failed to watch themes dir: {e}");
                return;
            }
            tracing::info!(source = "theme_watcher", path = %themes_dir.display(), "Started");
            *state.theme_watcher.lock() = Some(w);
        }
        Err(e) => {
            tracing::warn!(source = "theme_watcher", "Failed to create watcher: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Tauri command
// ---------------------------------------------------------------------------

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn list_themes(
    _state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Vec<ThemeEntry> {
    let themes_dir = crate::config::config_dir().join("themes");
    load_themes(&themes_dir)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_wt_json(name: &str) -> String {
        serde_json::json!({
            "name": name,
            "black": "#000000", "red": "#cc0000", "green": "#00cc00",
            "yellow": "#cccc00", "blue": "#0000cc", "purple": "#cc00cc",
            "cyan": "#00cccc", "white": "#cccccc",
            "brightBlack": "#666666", "brightRed": "#ff0000", "brightGreen": "#00ff00",
            "brightYellow": "#ffff00", "brightBlue": "#0000ff", "brightPurple": "#ff00ff",
            "brightCyan": "#00ffff", "brightWhite": "#ffffff",
            "background": "#1e1e1e", "foreground": "#d4d4d4",
            "cursorColor": "#d4d4d4", "selectionBackground": "#264f78"
        })
        .to_string()
    }

    fn make_wt_json_with_chrome(name: &str) -> String {
        serde_json::json!({
            "name": name,
            "black": "#000000", "red": "#cc0000", "green": "#00cc00",
            "yellow": "#cccc00", "blue": "#0000cc", "purple": "#cc00cc",
            "cyan": "#00cccc", "white": "#cccccc",
            "brightBlack": "#666666", "brightRed": "#ff0000", "brightGreen": "#00ff00",
            "brightYellow": "#ffff00", "brightBlue": "#0000ff", "brightPurple": "#ff00ff",
            "brightCyan": "#00ffff", "brightWhite": "#ffffff",
            "background": "#1e1e1e", "foreground": "#d4d4d4",
            "cursorColor": "#d4d4d4", "selectionBackground": "#264f78",
            "appChrome": {
                "background": "#1b1b1b", "surface": "#222222",
                "surfaceElevated": "#2a2a2a", "highlight": "#353535",
                "foreground": "#d4d4d4", "foregroundSecondary": "#a0a0a0",
                "mutedForeground": "#737373",
                "accent": "#2563b8", "accentHover": "#3b8eea",
                "border": "#2e2e2e",
                "success": "#23d18b", "warning": "#e5e510", "error": "#f14c4c",
                "accentForeground": "#ffffff", "errorForeground": "#000000",
                "successForeground": "#000000"
            }
        })
        .to_string()
    }

    #[test]
    fn key_derived_from_filename_lowercased() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("My-Theme.json"), make_wt_json("My Theme")).unwrap();

        let themes = load_themes(dir.path());
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].key, "my-theme");
        assert_eq!(themes[0].name, "My Theme");
    }

    #[test]
    fn name_derived_from_filename_when_missing() {
        let dir = TempDir::new().unwrap();
        let json = serde_json::json!({
            "black": "#000000", "red": "#cc0000", "green": "#00cc00",
            "yellow": "#cccc00", "blue": "#0000cc", "purple": "#cc00cc",
            "cyan": "#00cccc", "white": "#cccccc",
            "brightBlack": "#666666", "brightRed": "#ff0000", "brightGreen": "#00ff00",
            "brightYellow": "#ffff00", "brightBlue": "#0000ff", "brightPurple": "#ff00ff",
            "brightCyan": "#00ffff", "brightWhite": "#ffffff",
            "background": "#1e1e1e", "foreground": "#d4d4d4"
        })
        .to_string();
        fs::write(dir.path().join("cool-theme.json"), json).unwrap();

        let themes = load_themes(dir.path());
        assert_eq!(themes[0].name, "Cool Theme");
    }

    #[test]
    fn purple_mapped_to_magenta_position() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.json"), make_wt_json("Test")).unwrap();

        let themes = load_themes(dir.path());
        assert_eq!(themes[0].terminal.ansi[5], "#cc00cc"); // purple -> index 5 (magenta)
        assert_eq!(themes[0].terminal.ansi[13], "#ff00ff"); // brightPurple -> index 13
    }

    #[test]
    fn cursor_color_mapped_to_cursor() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.json"), make_wt_json("Test")).unwrap();

        let themes = load_themes(dir.path());
        assert_eq!(themes[0].terminal.cursor, "#d4d4d4");
    }

    #[test]
    fn app_chrome_from_json_uses_standard_names() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("test.json"),
            make_wt_json_with_chrome("Test"),
        )
        .unwrap();

        let themes = load_themes(dir.path());
        let ac = &themes[0].app_chrome;
        assert_eq!(ac.bg_primary, "#1b1b1b");
        assert_eq!(ac.bg_secondary, "#222222");
        assert_eq!(ac.bg_tertiary, "#2a2a2a");
        assert_eq!(ac.text_on_accent, "#ffffff");
    }

    #[test]
    fn app_chrome_derived_when_missing() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.json"), make_wt_json("Test")).unwrap();

        let themes = load_themes(dir.path());
        let ac = &themes[0].app_chrome;
        assert_eq!(ac.bg_primary, "#1e1e1e");
        assert_eq!(ac.accent, "#0000cc"); // blue ANSI color
        assert_eq!(ac.success, "#00cc00"); // green ANSI color
        assert_eq!(ac.warning, "#cccc00"); // yellow ANSI color
        assert_eq!(ac.error, "#cc0000"); // red ANSI color
        assert!(!ac.fg_primary.is_empty());
        assert!(!ac.fg_secondary.is_empty());
        assert!(!ac.fg_muted.is_empty());
    }

    #[test]
    fn skips_non_json_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.json"), make_wt_json("Test")).unwrap();
        fs::write(dir.path().join("readme.txt"), "not a theme").unwrap();
        fs::write(dir.path().join(".DS_Store"), "").unwrap();

        let themes = load_themes(dir.path());
        assert_eq!(themes.len(), 1);
    }

    #[test]
    fn skips_malformed_json() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("good.json"), make_wt_json("Good")).unwrap();
        fs::write(dir.path().join("bad.json"), "{ broken json }}}").unwrap();

        let themes = load_themes(dir.path());
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].name, "Good");
    }

    #[test]
    fn returns_empty_for_nonexistent_dir() {
        let themes = load_themes(Path::new("/nonexistent/path/themes"));
        assert!(themes.is_empty());
    }

    #[test]
    fn themes_sorted_by_name() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("z-theme.json"), make_wt_json("Zebra")).unwrap();
        fs::write(dir.path().join("a-theme.json"), make_wt_json("Alpha")).unwrap();
        fs::write(dir.path().join("m-theme.json"), make_wt_json("Middle")).unwrap();

        let themes = load_themes(dir.path());
        assert_eq!(themes[0].name, "Alpha");
        assert_eq!(themes[1].name, "Middle");
        assert_eq!(themes[2].name, "Zebra");
    }

    #[test]
    fn seed_creates_dir_and_files_when_missing() {
        let dir = TempDir::new().unwrap();
        let themes_dir = dir.path().join("themes");

        seed_builtin_themes(&themes_dir).unwrap();

        assert!(themes_dir.exists());
        assert_eq!(
            fs::read_dir(&themes_dir).unwrap().count(),
            BUILTIN_THEMES.len()
        );
        for (filename, _) in BUILTIN_THEMES {
            assert!(themes_dir.join(filename).exists(), "Missing: {filename}");
        }
    }

    #[test]
    fn seed_is_noop_when_dir_exists() {
        let dir = TempDir::new().unwrap();
        let themes_dir = dir.path().join("themes");
        fs::create_dir_all(&themes_dir).unwrap();
        fs::write(themes_dir.join("custom.json"), make_wt_json("Custom")).unwrap();

        seed_builtin_themes(&themes_dir).unwrap();

        // Only the custom file should exist — no built-ins seeded
        assert_eq!(fs::read_dir(&themes_dir).unwrap().count(), 1);
    }

    #[test]
    fn builtin_themes_parse_successfully() {
        let dir = TempDir::new().unwrap();
        let themes_dir = dir.path().join("themes");
        seed_builtin_themes(&themes_dir).unwrap();

        let themes = load_themes(&themes_dir);
        assert_eq!(themes.len(), BUILTIN_THEMES.len());

        for theme in &themes {
            assert!(!theme.key.is_empty());
            assert!(!theme.name.is_empty());
            assert!(!theme.terminal.background.is_empty());
            assert!(!theme.terminal.foreground.is_empty());
            assert!(!theme.terminal.cursor.is_empty());
            assert_eq!(theme.terminal.ansi.len(), 16);
            for color in &theme.terminal.ansi {
                assert!(
                    color.starts_with('#') && color.len() == 7,
                    "Invalid ANSI color {color} in theme {}",
                    theme.key
                );
            }
            assert!(!theme.app_chrome.bg_primary.is_empty());
            assert!(!theme.app_chrome.accent.is_empty());
        }
    }

    #[test]
    fn derive_dark_theme_chrome() {
        let ansi = [
            "#000000".into(),
            "#cc0000".into(),
            "#00cc00".into(),
            "#cccc00".into(),
            "#0000cc".into(),
            "#cc00cc".into(),
            "#00cccc".into(),
            "#cccccc".into(),
            "#666666".into(),
            "#ff0000".into(),
            "#00ff00".into(),
            "#ffff00".into(),
            "#0000ff".into(),
            "#ff00ff".into(),
            "#00ffff".into(),
            "#ffffff".into(),
        ];
        let ac = derive_app_chrome("#1e1e1e", "#d4d4d4", &ansi);

        assert_eq!(ac.bg_primary, "#1e1e1e");
        assert_eq!(ac.fg_primary, "#d4d4d4");
        assert_eq!(ac.accent, "#0000cc");
        assert_eq!(ac.error, "#cc0000");
        assert_eq!(ac.success, "#00cc00");
        assert_eq!(ac.warning, "#cccc00");
        // Dark bg → text on dark accent should be white
        assert_eq!(ac.text_on_accent, "#ffffff");
        // bg_secondary should be lighter than bg_primary for dark themes
        let (_, _, b_primary) = parse_hex(&ac.bg_primary);
        let (_, _, b_secondary) = parse_hex(&ac.bg_secondary);
        assert!(
            b_secondary > b_primary,
            "secondary should be lighter for dark theme"
        );
    }

    #[test]
    fn derive_light_theme_chrome() {
        let ansi = [
            "#000000".into(),
            "#cd3131".into(),
            "#107c10".into(),
            "#949800".into(),
            "#0451a5".into(),
            "#bc05bc".into(),
            "#0598bc".into(),
            "#555555".into(),
            "#666666".into(),
            "#cd3131".into(),
            "#14ce14".into(),
            "#b5ba00".into(),
            "#0451a5".into(),
            "#bc05bc".into(),
            "#0598bc".into(),
            "#a5a5a5".into(),
        ];
        let ac = derive_app_chrome("#ffffff", "#333333", &ansi);

        assert_eq!(ac.bg_primary, "#ffffff");
        assert_eq!(ac.fg_primary, "#333333");
        // Light bg → text on accent should contrast properly
        assert!(ac.text_on_accent == "#000000" || ac.text_on_accent == "#ffffff");
        // bg_secondary should be darker than bg_primary for light themes
        let (r_primary, _, _) = parse_hex(&ac.bg_primary);
        let (r_secondary, _, _) = parse_hex(&ac.bg_secondary);
        assert!(
            r_secondary < r_primary,
            "secondary should be darker for light theme"
        );
    }

    #[test]
    fn contrast_text_returns_white_for_dark_bg() {
        assert_eq!(contrast_text("#000000"), "#ffffff");
        assert_eq!(contrast_text("#1e1e1e"), "#ffffff");
        assert_eq!(contrast_text("#282a36"), "#ffffff");
    }

    #[test]
    fn contrast_text_returns_black_for_light_bg() {
        assert_eq!(contrast_text("#ffffff"), "#000000");
        assert_eq!(contrast_text("#f8f8f2"), "#000000");
        assert_eq!(contrast_text("#d4d4d4"), "#000000");
    }

    #[test]
    fn parse_hex_handles_edge_cases() {
        assert_eq!(parse_hex("#000000"), (0, 0, 0));
        assert_eq!(parse_hex("#ffffff"), (255, 255, 255));
        assert_eq!(parse_hex("#FF00FF"), (255, 0, 255));
        assert_eq!(parse_hex("abc"), (0, 0, 0)); // too short
    }

    #[test]
    fn blend_interpolates_colors() {
        assert_eq!(blend("#000000", "#ffffff", 0.5), "#808080");
        assert_eq!(blend("#ff0000", "#0000ff", 0.0), "#ff0000");
        assert_eq!(blend("#ff0000", "#0000ff", 1.0), "#0000ff");
    }
}
