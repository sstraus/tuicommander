use parking_lot::Mutex;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::Serialize;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "desktop")]
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::pty::spawn_reader_thread;
use crate::state::{
    AgentConfig, AppState, OUTPUT_RING_BUFFER_CAPACITY, OutputRingBuffer, PtyConfig, PtySession,
    VT_LOG_BUFFER_CAPACITY, VtLogBuffer,
};

// resolve_cli and has_cli are now in crate::cli — re-export for backwards compatibility
use crate::cli::has_cli;
pub(crate) use crate::cli::resolve_cli;

/// Format a path with line/col for --goto style editors (vscode, cursor, windsurf)
fn format_goto_arg(path: &str, line: Option<u32>, col: Option<u32>) -> String {
    match (line, col) {
        (Some(l), Some(c)) => format!("{path}:{l}:{c}"),
        (Some(l), None) => format!("{path}:{l}"),
        _ => path.to_string(),
    }
}

/// Build a Command for a --goto-style editor (vscode, cursor, windsurf, zed).
/// Falls back to `open -a` on macOS when the CLI binary isn't installed.
fn goto_editor_cmd(
    cli_name: &str,
    #[cfg_attr(not(target_os = "macos"), allow(unused))] app_name: &str,
    path: &str,
    line: Option<u32>,
    col: Option<u32>,
) -> Command {
    let resolved = resolve_cli(cli_name);
    if resolved != cli_name || has_cli(cli_name) {
        let mut c = Command::new(&resolved);
        if line.is_some() {
            c.arg("--goto");
        }
        c.arg(format_goto_arg(path, line, col));
        return c;
    }
    #[cfg(target_os = "macos")]
    {
        let mut c = Command::new("open");
        c.arg("-a").arg(app_name).arg(path);
        c
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut c = Command::new(cli_name);
        c.arg(format_goto_arg(path, line, col));
        c
    }
}

/// Build a Command for a JetBrains IDE launcher (idea, pycharm, webstorm, ...).
/// JetBrains launchers use `--line`/`--column` goto syntax. Falls back to
/// `open -a` on macOS when the CLI launcher isn't on PATH (the user hasn't
/// enabled Toolbox shell scripts).
fn jetbrains_cmd(
    cli_name: &str,
    #[cfg_attr(not(target_os = "macos"), allow(unused))] app_name: &str,
    path: &str,
    line: Option<u32>,
    col: Option<u32>,
) -> Command {
    let resolved = resolve_cli(cli_name);
    if resolved != cli_name || has_cli(cli_name) {
        let mut c = Command::new(&resolved);
        if let Some(l) = line {
            c.arg("--line").arg(l.to_string());
            if let Some(col) = col {
                c.arg("--column").arg(col.to_string());
            }
        }
        c.arg(path);
        return c;
    }
    #[cfg(target_os = "macos")]
    {
        let mut c = Command::new("open");
        c.arg("-a").arg(app_name).arg(path);
        c
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut c = Command::new(cli_name);
        c.arg(path);
        c
    }
}

/// Open a path in an IDE or application.
/// `line` and `col` are optional and only used by editors that support them.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn open_in_app(
    path: String,
    app: String,
    line: Option<u32>,
    col: Option<u32>,
) -> Result<(), String> {
    let mut cmd = match app.as_str() {
        // CLI-based editors with --goto support (cross-platform, with path resolution)
        "vscode" => goto_editor_cmd("code", "Visual Studio Code", &path, line, col),
        "cursor" => goto_editor_cmd("cursor", "Cursor", &path, line, col),
        "windsurf" => goto_editor_cmd("windsurf", "Windsurf", &path, line, col),
        // Zed uses path:line natively
        "zed" => {
            let mut c = Command::new(resolve_cli("zed"));
            c.arg(format_goto_arg(&path, line, col));
            c
        }
        // Neovim uses +line
        "neovim" => {
            let mut c = Command::new(resolve_cli("nvim"));
            if let Some(l) = line {
                c.arg(format!("+{l}"));
            }
            c.arg(&path);
            c
        }
        "smerge" => {
            let mut c = Command::new(resolve_cli("smerge"));
            c.arg(&path);
            c
        }

        // JetBrains IDEs — CLI launchers with --line/--column, `open -a` fallback on macOS
        "intellij" => jetbrains_cmd("idea", "IntelliJ IDEA", &path, line, col),
        "pycharm" => jetbrains_cmd("pycharm", "PyCharm", &path, line, col),
        "webstorm" => jetbrains_cmd("webstorm", "WebStorm", &path, line, col),
        "goland" => jetbrains_cmd("goland", "GoLand", &path, line, col),
        "clion" => jetbrains_cmd("clion", "CLion", &path, line, col),
        "phpstorm" => jetbrains_cmd("phpstorm", "PhpStorm", &path, line, col),
        "rubymine" => jetbrains_cmd("rubymine", "RubyMine", &path, line, col),
        "rider" => jetbrains_cmd("rider", "Rider", &path, line, col),
        "datagrip" => jetbrains_cmd("datagrip", "DataGrip", &path, line, col),
        "rustrover" => jetbrains_cmd("rustrover", "RustRover", &path, line, col),
        "android-studio" => jetbrains_cmd("studio", "Android Studio", &path, line, col),
        "fleet" => jetbrains_cmd("fleet", "Fleet", &path, line, col),

        // Terminal emulators with CLI (cross-platform)
        "kitty" => {
            let mut c = Command::new(resolve_cli("kitty"));
            c.arg("--directory").arg(&path);
            c
        }
        "wezterm" if has_cli("wezterm") => {
            let mut c = Command::new(resolve_cli("wezterm"));
            c.arg("start").arg("--cwd").arg(&path);
            c
        }
        "alacritty" if has_cli("alacritty") => {
            let mut c = Command::new(resolve_cli("alacritty"));
            c.arg("--working-directory").arg(&path);
            c
        }

        // macOS .app bundles (use 'open -a')
        app_name if cfg!(target_os = "macos") => match app_name {
            "xcode" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("Xcode").arg(&path);
                c
            }
            "sourcetree" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("Sourcetree").arg(&path);
                c
            }
            "github-desktop" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("GitHub Desktop").arg(&path);
                c
            }
            "fork" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("Fork").arg(&path);
                c
            }
            "gitkraken" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("GitKraken").arg(&path);
                c
            }
            "ghostty" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("Ghostty").arg(&path);
                c
            }
            "wezterm" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("WezTerm").arg(&path);
                c
            }
            "alacritty" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("Alacritty").arg(&path);
                c
            }
            "warp" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("Warp").arg(&path);
                c
            }
            "iterm2" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("iTerm").arg(&path);
                c
            }
            "tower" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("Tower").arg(&path);
                c
            }
            "terminal" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("Terminal").arg(&path);
                c
            }
            "finder" => {
                let mut c = Command::new("open");
                c.arg(&path);
                c
            }
            "editor" => {
                if let Ok(editor) = std::env::var("EDITOR") {
                    let mut c = Command::new(&editor);
                    if let Some(l) = line {
                        c.arg(format!("+{l}"));
                    }
                    c.arg(&path);
                    c
                } else {
                    return Err("$EDITOR not set".to_string());
                }
            }
            _ => return Err(format!("Unknown app: {app_name}")),
        },

        // Linux: system terminal + file manager
        #[cfg(target_os = "linux")]
        "terminal" => {
            // Try common terminals in order
            let terminals = [
                "ghostty",
                "wezterm",
                "alacritty",
                "kitty",
                "gnome-terminal",
                "konsole",
                "xterm",
            ];
            if let Some(term) = terminals.iter().find(|t| has_cli(t)) {
                let mut c = Command::new(term);
                c.arg(&path);
                c
            } else {
                return Err("No terminal emulator found".to_string());
            }
        }
        #[cfg(target_os = "linux")]
        "finder" => {
            let mut c = Command::new("xdg-open");
            c.arg(&path);
            c
        }

        // Windows: system terminal, file manager, and app launchers
        #[cfg(target_os = "windows")]
        "terminal" => {
            // Prefer Windows Terminal (wt.exe) over cmd.exe
            if has_cli("wt") {
                let mut c = Command::new("wt");
                c.args(["-d", &path]);
                c
            } else {
                let mut c = Command::new("cmd");
                c.args(["/c", "start", "cmd", "/k", "cd", "/d", &path]);
                c
            }
        }
        #[cfg(target_os = "windows")]
        "finder" => {
            let mut c = Command::new("explorer");
            c.arg(&path);
            c
        }
        #[cfg(target_os = "windows")]
        app_name
            if matches!(
                app_name,
                "sourcetree" | "github-desktop" | "fork" | "gitkraken"
            ) =>
        {
            let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
            let exe = match app_name {
                "sourcetree" => {
                    format!("{}\\Atlassian\\SourceTree\\SourceTree.exe", local_app_data)
                }
                "github-desktop" => format!("{}\\GitHubDesktop\\GitHubDesktop.exe", local_app_data),
                "fork" => format!("{}\\Fork\\Fork.exe", local_app_data),
                "gitkraken" => format!("{}\\gitkraken\\gitkraken.exe", local_app_data),
                _ => unreachable!(),
            };
            if std::path::Path::new(&exe).exists() {
                let mut c = Command::new(&exe);
                c.arg(&path);
                c
            } else {
                return Err(format!("{app_name} not found at {exe}"));
            }
        }

        _ => return Err(format!("Unknown app: {app}")),
    };

    cmd.spawn()
        .map_err(|e| format!("Failed to open in {app}: {e}"))?;

    Ok(())
}

/// Launch context for a custom tool: the paths and cursor position that feed
/// the placeholder expander. `file` is the focused editor file (absent when no
/// file is open); `repo` is the active repo/worktree root and acts as the
/// fallback for `{path}`/`{file}`/`{fileDir}`/`{cwd}`.
#[derive(serde::Deserialize)]
pub(crate) struct LaunchContext {
    /// Focused editor file. `None` → `{path}`/`{file}`/`{fileDir}` fall back to `repo`.
    file: Option<String>,
    /// Active repo/worktree root. Required; the universal fallback.
    repo: String,
    /// Focused terminal's working directory. `None` → `{cwd}` falls back to `repo`.
    cwd: Option<String>,
    line: Option<u32>,
    col: Option<u32>,
}

/// Expand placeholders in a custom launcher's argument template:
/// `{path}`/`{file}` (focused file, else repo), `{repo}`, `{fileDir}` (parent
/// of the focused file, else repo), `{cwd}` (focused terminal cwd, else repo),
/// `{home}` (user home), `{line}`/`{column}` (cursor, default 1 — e.g. when
/// opening a folder — so editor goto args still resolve).
pub(crate) fn expand_placeholders(args: &[String], ctx: &LaunchContext) -> Vec<String> {
    let file = ctx.file.as_deref().unwrap_or(&ctx.repo);
    let file_dir = ctx
        .file
        .as_deref()
        .and_then(|f| std::path::Path::new(f).parent())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| ctx.repo.clone());
    let cwd = ctx.cwd.as_deref().unwrap_or(&ctx.repo);
    let home = dirs::home_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| {
            tracing::warn!(
                source = "agent",
                "Home directory unresolvable; {{home}} placeholder expands to empty"
            );
            String::new()
        });
    let line_s = ctx.line.unwrap_or(1).to_string();
    let col_s = ctx.col.unwrap_or(1).to_string();
    args.iter()
        .map(|a| {
            a.replace("{path}", file)
                .replace("{file}", file)
                .replace("{repo}", &ctx.repo)
                .replace("{fileDir}", &file_dir)
                .replace("{cwd}", cwd)
                .replace("{home}", &home)
                .replace("{line}", &line_s)
                .replace("{column}", &col_s)
        })
        .collect()
}

/// Launch a user-defined custom tool: spawn `executable` with the
/// placeholder-expanded args. No shell parsing — args are passed verbatim, so
/// paths with spaces are safe on every platform. `executable` may be a bare
/// name (resolved on PATH) or an absolute path.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn open_in_custom(
    executable: String,
    args: Vec<String>,
    ctx: LaunchContext,
) -> Result<(), String> {
    if executable.trim().is_empty() {
        return Err("Custom launcher has no executable".to_string());
    }
    // Drop blank lines from the args editor (textarea is one-arg-per-line).
    let args: Vec<String> = args.into_iter().filter(|a| !a.trim().is_empty()).collect();
    let expanded = expand_placeholders(&args, &ctx);
    Command::new(resolve_cli(&executable))
        .args(&expanded)
        .spawn()
        .map_err(|e| format!("Failed to launch {executable}: {e}"))?;
    Ok(())
}

/// Detect installed IDE applications (cross-platform)
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn detect_installed_ides() -> Vec<String> {
    let mut installed = Vec::new();

    // CLI-detectable tools (cross-platform via which/where)
    let cli_tools: &[(&str, &str)] = &[
        ("vscode", "code"),
        ("cursor", "cursor"),
        ("zed", "zed"),
        ("windsurf", "windsurf"),
        ("neovim", "nvim"),
        ("smerge", "smerge"),
        ("kitty", "kitty"),
        // JetBrains CLI launchers (present when Toolbox shell scripts are enabled)
        ("intellij", "idea"),
        ("pycharm", "pycharm"),
        ("webstorm", "webstorm"),
        ("goland", "goland"),
        ("clion", "clion"),
        ("phpstorm", "phpstorm"),
        ("rubymine", "rubymine"),
        ("rider", "rider"),
        ("datagrip", "datagrip"),
        ("rustrover", "rustrover"),
        ("android-studio", "studio"),
        ("fleet", "fleet"),
    ];
    for (id, bin) in cli_tools {
        if has_cli(bin) {
            installed.push(id.to_string());
        }
    }

    // macOS: .app bundle detection (includes editors whose CLI symlinks may
    // not be on PATH when the app is launched from Finder)
    #[cfg(target_os = "macos")]
    {
        let app_bundles: &[(&str, &str)] = &[
            ("vscode", "/Applications/Visual Studio Code.app"),
            ("cursor", "/Applications/Cursor.app"),
            ("zed", "/Applications/Zed.app"),
            ("windsurf", "/Applications/Windsurf.app"),
            ("xcode", "/Applications/Xcode.app"),
            ("sourcetree", "/Applications/Sourcetree.app"),
            ("github-desktop", "/Applications/GitHub Desktop.app"),
            ("fork", "/Applications/Fork.app"),
            ("gitkraken", "/Applications/GitKraken.app"),
            ("tower", "/Applications/Tower.app"),
            ("ghostty", "/Applications/Ghostty.app"),
            ("wezterm", "/Applications/WezTerm.app"),
            ("alacritty", "/Applications/Alacritty.app"),
            ("warp", "/Applications/Warp.app"),
            ("iterm2", "/Applications/iTerm.app"),
            // JetBrains .app bundles (CLI symlinks may not be on PATH when
            // launched from Finder; best-effort — Toolbox naming can vary)
            ("intellij", "/Applications/IntelliJ IDEA.app"),
            ("pycharm", "/Applications/PyCharm.app"),
            ("webstorm", "/Applications/WebStorm.app"),
            ("goland", "/Applications/GoLand.app"),
            ("clion", "/Applications/CLion.app"),
            ("phpstorm", "/Applications/PhpStorm.app"),
            ("rubymine", "/Applications/RubyMine.app"),
            ("rider", "/Applications/Rider.app"),
            ("datagrip", "/Applications/DataGrip.app"),
            ("rustrover", "/Applications/RustRover.app"),
            ("android-studio", "/Applications/Android Studio.app"),
            ("fleet", "/Applications/Fleet.app"),
        ];
        for (id, path) in app_bundles {
            if std::path::Path::new(path).exists() && !installed.contains(&id.to_string()) {
                installed.push(id.to_string());
            }
        }
    }

    // Linux: additional CLI detection for apps without separate CLI
    #[cfg(target_os = "linux")]
    {
        let linux_tools: &[(&str, &str)] = &[
            ("ghostty", "ghostty"),
            ("wezterm", "wezterm"),
            ("alacritty", "alacritty"),
        ];
        for (id, bin) in linux_tools {
            if has_cli(bin) && !installed.contains(&id.to_string()) {
                installed.push(id.to_string());
            }
        }
    }

    // Windows: detect apps installed in standard locations
    #[cfg(target_os = "windows")]
    {
        let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
        let program_files = std::env::var("ProgramFiles").unwrap_or_default();
        let win_apps: &[(&str, Vec<String>)] = &[
            (
                "vscode",
                vec![
                    format!("{}\\Programs\\Microsoft VS Code\\Code.exe", local_app_data),
                    format!("{}\\Microsoft VS Code\\Code.exe", program_files),
                ],
            ),
            (
                "cursor",
                vec![format!("{}\\Programs\\cursor\\Cursor.exe", local_app_data)],
            ),
            (
                "windsurf",
                vec![format!(
                    "{}\\Programs\\windsurf\\Windsurf.exe",
                    local_app_data
                )],
            ),
            (
                "sourcetree",
                vec![format!(
                    "{}\\Atlassian\\SourceTree\\SourceTree.exe",
                    local_app_data
                )],
            ),
            (
                "github-desktop",
                vec![format!(
                    "{}\\GitHubDesktop\\GitHubDesktop.exe",
                    local_app_data
                )],
            ),
            ("fork", vec![format!("{}\\Fork\\Fork.exe", local_app_data)]),
            (
                "gitkraken",
                vec![format!("{}\\gitkraken\\gitkraken.exe", local_app_data)],
            ),
        ];
        for (id, paths) in win_apps {
            if !installed.contains(&id.to_string())
                && paths.iter().any(|p| std::path::Path::new(p).exists())
            {
                installed.push(id.to_string());
            }
        }
    }

    // $EDITOR support
    if let Ok(editor) = std::env::var("EDITOR")
        && !editor.is_empty()
    {
        installed.push("editor".to_string());
    }

    // System utilities (always available)
    installed.push("terminal".to_string());
    installed.push("finder".to_string());

    installed
}

/// Agent binary detection result
#[derive(Clone, Serialize)]
pub(crate) struct AgentBinaryDetection {
    pub(crate) path: Option<String>,
    pub(crate) version: Option<String>,
}

/// Detect any agent binary location
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn detect_agent_binary(binary: String) -> AgentBinaryDetection {
    let home = dirs::home_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    // Platform-specific candidate paths
    #[cfg(not(windows))]
    let candidates = vec![
        format!("{}/.local/bin/{}", home, binary),
        format!("/usr/local/bin/{}", binary),
        format!("/opt/homebrew/bin/{}", binary),
        format!("{}/.npm-global/bin/{}", home, binary),
        format!("{}/.cargo/bin/{}", home, binary),
        format!("{}/go/bin/{}", home, binary),
        format!("{}/.pyenv/shims/{}", home, binary),
    ];

    #[cfg(windows)]
    let candidates = {
        let program_files =
            std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".to_string());
        let mut v = vec![
            format!("{}\\.cargo\\bin\\{}.exe", home, binary),
            format!("{}\\go\\bin\\{}.exe", home, binary),
            format!(
                "{}\\AppData\\Local\\Programs\\{}\\{}.exe",
                home, binary, binary
            ),
            format!("{}\\scoop\\shims\\{}.exe", home, binary),
            format!("{}\\{}.exe", program_files, binary),
            format!("{}\\{}\\{}.exe", program_files, binary, binary),
        ];
        // Scan WinGet packages directory for matching binaries
        let winget_dir = format!("{}\\AppData\\Local\\Microsoft\\WinGet\\Packages", home);
        if let Ok(entries) = std::fs::read_dir(&winget_dir) {
            for entry in entries.flatten() {
                let exe = entry.path().join(format!("{}.exe", binary));
                if exe.exists() {
                    v.push(exe.to_string_lossy().to_string());
                }
            }
        }
        v
    };

    // Use platform-appropriate PATH lookup (which on Unix, where on Windows)
    let checker = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    let mut checker_cmd = Command::new(checker);
    checker_cmd.arg(&binary);
    crate::cli::apply_no_window(&mut checker_cmd);
    if let Ok(output) = checker_cmd.output()
        && output.status.success()
    {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // `where` on Windows may return multiple lines; take the first
        let path = path.lines().next().unwrap_or("").to_string();
        if !path.is_empty() && std::path::Path::new(&path).exists() {
            let version = get_binary_version(&path);
            return AgentBinaryDetection {
                path: Some(path),
                version,
            };
        }
    }

    // Fall back to known locations
    for candidate in &candidates {
        if !candidate.is_empty() && std::path::Path::new(candidate).exists() {
            let version = get_binary_version(candidate);
            return AgentBinaryDetection {
                path: Some(candidate.clone()),
                version,
            };
        }
    }

    AgentBinaryDetection {
        path: None,
        version: None,
    }
}

/// Batch-detect multiple agent binaries in parallel.
/// Returns a map of binary name -> detection result.
/// Skips version detection for speed; use detect_agent_binary for full info.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn detect_all_agent_binaries(
    binaries: Vec<String>,
) -> std::collections::HashMap<String, AgentBinaryDetection> {
    let handles: Vec<_> = binaries
        .into_iter()
        .map(|binary| {
            std::thread::spawn(move || {
                let detection = detect_binary_path_only(&binary);
                (binary, detection)
            })
        })
        .collect();

    let mut results = std::collections::HashMap::new();
    for handle in handles {
        if let Ok((binary, detection)) = handle.join() {
            results.insert(binary, detection);
        }
    }
    results
}

/// Fast binary detection: path lookup only, no version check.
fn detect_binary_path_only(binary: &str) -> AgentBinaryDetection {
    let home = dirs::home_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    #[cfg(not(windows))]
    let candidates = vec![
        format!("{}/.local/bin/{}", home, binary),
        format!("/usr/local/bin/{}", binary),
        format!("/opt/homebrew/bin/{}", binary),
        format!("{}/.npm-global/bin/{}", home, binary),
        format!("{}/.cargo/bin/{}", home, binary),
        format!("{}/go/bin/{}", home, binary),
        format!("{}/.pyenv/shims/{}", home, binary),
    ];

    #[cfg(windows)]
    let candidates = {
        let program_files =
            std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".to_string());
        let mut v = vec![
            format!("{}\\.cargo\\bin\\{}.exe", home, binary),
            format!("{}\\go\\bin\\{}.exe", home, binary),
            format!(
                "{}\\AppData\\Local\\Programs\\{}\\{}.exe",
                home, binary, binary
            ),
            format!("{}\\scoop\\shims\\{}.exe", home, binary),
            format!("{}\\{}.exe", program_files, binary),
            format!("{}\\{}\\{}.exe", program_files, binary, binary),
        ];
        let winget_dir = format!("{}\\AppData\\Local\\Microsoft\\WinGet\\Packages", home);
        if let Ok(entries) = std::fs::read_dir(&winget_dir) {
            for entry in entries.flatten() {
                let exe = entry.path().join(format!("{}.exe", binary));
                if exe.exists() {
                    v.push(exe.to_string_lossy().to_string());
                }
            }
        }
        v
    };

    let checker = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    let mut checker_cmd = Command::new(checker);
    checker_cmd.arg(binary);
    crate::cli::apply_no_window(&mut checker_cmd);
    if let Ok(output) = checker_cmd.output()
        && output.status.success()
    {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let path = path.lines().next().unwrap_or("").to_string();
        if !path.is_empty() && std::path::Path::new(&path).exists() {
            return AgentBinaryDetection {
                path: Some(path),
                version: None,
            };
        }
    }

    for candidate in &candidates {
        if !candidate.is_empty() && std::path::Path::new(candidate).exists() {
            return AgentBinaryDetection {
                path: Some(candidate.clone()),
                version: None,
            };
        }
    }

    AgentBinaryDetection {
        path: None,
        version: None,
    }
}

/// Get version of a binary (try --version or -v)
fn get_binary_version(path: &str) -> Option<String> {
    // Try --version first
    let mut cmd = Command::new(path);
    cmd.arg("--version");
    crate::cli::apply_no_window(&mut cmd);
    if let Ok(output) = cmd.output()
        && output.status.success()
    {
        let version = String::from_utf8_lossy(&output.stdout);
        let first_line = version.lines().next().unwrap_or("").trim();
        if !first_line.is_empty() {
            return Some(first_line.to_string());
        }
    }
    // Try -v
    let mut cmd = Command::new(path);
    cmd.arg("-v");
    crate::cli::apply_no_window(&mut cmd);
    if let Ok(output) = cmd.output()
        && output.status.success()
    {
        let version = String::from_utf8_lossy(&output.stdout);
        let first_line = version.lines().next().unwrap_or("").trim();
        if !first_line.is_empty() {
            return Some(first_line.to_string());
        }
    }
    None
}

/// Detect claude binary location (legacy, delegates to detect_agent_binary)
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn detect_claude_binary() -> Result<String, String> {
    let detection = detect_agent_binary("claude".to_string());
    detection.path.ok_or_else(|| {
        "Claude binary not found. Install with: npm install -g @anthropic-ai/claude-code"
            .to_string()
    })
}

/// Spawn an agent in a PTY
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn spawn_agent(
    _app: AppHandle,
    state: State<'_, Arc<AppState>>,
    pty_config: PtyConfig,
    agent_config: AgentConfig,
) -> Result<String, String> {
    // Determine binary path - use provided path, detect by type, or fall back to claude
    let binary_path = if let Some(ref path) = agent_config.binary_path {
        let expanded = crate::cli::expand_tilde(path);
        let p = std::path::Path::new(&expanded);
        if !p.is_absolute() {
            return Err("binary_path must be an absolute path".to_string());
        }
        if !p.is_file() {
            return Err("binary_path does not point to an existing file".to_string());
        }
        expanded
    } else if let Some(ref agent_type) = agent_config.agent_type {
        let detection = detect_agent_binary(agent_type.clone());
        detection
            .path
            .ok_or_else(|| format!("Agent binary '{agent_type}' not found"))?
    } else {
        detect_claude_binary()?
    };

    let session_id = Uuid::new_v4().to_string();
    let pty_system = native_pty_system();

    let pair = pty_system
        .openpty(PtySize {
            rows: pty_config.rows,
            cols: pty_config.cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Failed to open PTY: {e}"))?;

    // Build agent command
    let mut cmd = CommandBuilder::new(&binary_path);

    // If custom args are provided, use them directly
    if let Some(ref args) = agent_config.args {
        for arg in args {
            cmd.arg(arg);
        }
    } else {
        // Default Claude-style args for backward compatibility
        if agent_config.print_mode {
            cmd.arg("--print");
        }

        if let Some(ref format) = agent_config.output_format {
            cmd.arg("--output-format");
            cmd.arg(format);
        }

        if let Some(ref model) = agent_config.model {
            cmd.arg("--model");
            cmd.arg(model);
        }

        // Add prompt
        cmd.arg(&agent_config.prompt);
    }

    if let Some(ref cwd) = agent_config.cwd {
        cmd.cwd(crate::cli::expand_tilde(cwd));
    } else if let Some(ref cwd) = pty_config.cwd {
        cmd.cwd(crate::cli::expand_tilde(cwd));
    }

    // Inject env flags (feature flags configured in Settings → Agents)
    for (key, value) in &pty_config.env {
        cmd.env(key, value);
    }

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("Failed to spawn agent: {e}"))?;

    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("Failed to get PTY writer: {e}"))?;

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("Failed to get PTY reader: {e}"))?;

    // Store session (master handle kept for resize support)
    let paused = Arc::new(AtomicBool::new(false));
    state.sessions.insert(
        session_id.clone(),
        Mutex::new(PtySession {
            writer,
            master: pair.master,
            _child: child,
            paused: paused.clone(),
            worktree: None,
            cwd: agent_config.cwd.clone(),
            display_name: None,
            shell: binary_path.clone(),
        }),
    );
    state.metrics.total_spawned.fetch_add(1, Ordering::Relaxed);
    state
        .metrics
        .active_sessions
        .fetch_add(1, Ordering::Relaxed);

    // Create ring buffer and VT log buffer for this session
    state.output_buffers.insert(
        session_id.clone(),
        Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)),
    );
    state.vt_log_buffers.insert(
        session_id.clone(),
        Mutex::new(VtLogBuffer::new(24, 220, VT_LOG_BUFFER_CAPACITY)),
    );
    state
        .last_output_ms
        .insert(session_id.clone(), std::sync::atomic::AtomicU64::new(0));

    spawn_reader_thread(
        reader,
        paused,
        session_id.clone(),
        state.inner().clone(),
        None,
    );

    Ok(session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    // resolve_cli and extra_bin_dirs tests are now in cli.rs

    #[test]
    fn test_detect_claude_binary() {
        // This test checks that detect_claude_binary returns a result
        // It may succeed or fail depending on whether claude is installed
        let result = detect_claude_binary();
        // We just verify it doesn't panic and returns a proper Result
        match result {
            Ok(path) => {
                assert!(!path.is_empty());
                assert!(std::path::Path::new(&path).exists());
            }
            Err(msg) => {
                assert!(msg.contains("not found") || msg.contains("Install"));
            }
        }
    }

    fn ctx(
        file: Option<&str>,
        repo: &str,
        cwd: Option<&str>,
        line: Option<u32>,
        col: Option<u32>,
    ) -> LaunchContext {
        LaunchContext {
            file: file.map(str::to_string),
            repo: repo.to_string(),
            cwd: cwd.map(str::to_string),
            line,
            col,
        }
    }

    #[test]
    fn expand_placeholders_substitutes_path_and_location() {
        let args = vec!["--goto".to_string(), "{file}:{line}:{column}".to_string()];
        let out = expand_placeholders(
            &args,
            &ctx(Some("/repo/src/main.rs"), "/repo", None, Some(42), Some(7)),
        );
        assert_eq!(out, vec!["--goto", "/repo/src/main.rs:42:7"]);
    }

    #[test]
    fn expand_placeholders_path_and_file_are_aliases() {
        let args = vec!["{path}".to_string(), "{file}".to_string()];
        let out = expand_placeholders(&args, &ctx(Some("/a/b"), "/a", None, None, None));
        assert_eq!(out, vec!["/a/b", "/a/b"]);
    }

    #[test]
    fn expand_placeholders_repo_filedir_and_cwd() {
        let args = vec![
            "{repo}".to_string(),
            "{fileDir}".to_string(),
            "{cwd}".to_string(),
        ];
        let out = expand_placeholders(
            &args,
            &ctx(
                Some("/repo/src/main.rs"),
                "/repo",
                Some("/tmp/work"),
                None,
                None,
            ),
        );
        assert_eq!(out, vec!["/repo", "/repo/src", "/tmp/work"]);
    }

    #[test]
    fn expand_placeholders_no_file_falls_back_to_repo() {
        // No focused file: {path}/{file}/{fileDir} all resolve to the repo root.
        let args = vec![
            "{path}".to_string(),
            "{file}".to_string(),
            "{fileDir}".to_string(),
        ];
        let out = expand_placeholders(&args, &ctx(None, "/proj", None, None, None));
        assert_eq!(out, vec!["/proj", "/proj", "/proj"]);
    }

    #[test]
    fn expand_placeholders_cwd_falls_back_to_repo() {
        let args = vec!["{cwd}".to_string()];
        let out = expand_placeholders(&args, &ctx(Some("/repo/f.rs"), "/repo", None, None, None));
        assert_eq!(out, vec!["/repo"]);
    }

    #[test]
    fn expand_placeholders_home_is_substituted() {
        let args = vec!["{home}".to_string()];
        let out = expand_placeholders(&args, &ctx(None, "/proj", None, None, None));
        // Home resolves to a real path on the dev/CI machine — never left literal.
        assert_ne!(out[0], "{home}");
        assert!(!out[0].is_empty());
    }

    #[test]
    fn expand_placeholders_defaults_line_col_to_one() {
        // Opening a folder: no line/col → placeholders still resolve to 1.
        let args = vec![
            "{path}".to_string(),
            "+{line}".to_string(),
            "{column}".to_string(),
        ];
        let out = expand_placeholders(&args, &ctx(None, "/proj", None, None, None));
        assert_eq!(out, vec!["/proj", "+1", "1"]);
    }

    #[test]
    fn expand_placeholders_leaves_literals_untouched() {
        let args = vec!["--wait".to_string(), "--reuse-window".to_string()];
        let out = expand_placeholders(&args, &ctx(Some("/x"), "/x", None, Some(3), Some(1)));
        assert_eq!(out, vec!["--wait", "--reuse-window"]);
    }

    #[test]
    fn open_in_custom_rejects_empty_executable() {
        let err = open_in_custom("  ".to_string(), vec![], ctx(None, "/x", None, None, None));
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("no executable"));
    }
}
