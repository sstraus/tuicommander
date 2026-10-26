use parking_lot::Mutex;
use portable_pty::{CommandBuilder, PtySize};
use serde::Serialize;
use std::process::Command;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "desktop")]
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::pty::spawn_reader_thread;
use crate::state::{
    AgentConfig, AppState, OUTPUT_RING_BUFFER_CAPACITY, OutputRingBuffer, PtyConfig, PtySession,
    VT_LOG_BUFFER_CAPACITY,
};

// resolve_cli and has_cli are now in crate::cli — re-export for backwards compatibility
use crate::cli::has_cli;
pub(crate) use crate::cli::resolve_cli;

#[cfg(all(test, target_os = "macos", feature = "desktop"))]
#[path = "agent_launch_reaping_tests.rs"]
mod launch_reaping_tests;

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
    let cmd = build_open_in_app_command(&path, &app, line, col)?;

    tuic_core::child_reaping::spawn_reaped(cmd)
        .map_err(|e| format!("Failed to open in {app}: {e}"))?;

    Ok(())
}

/// Build the platform command used by [`open_in_app`] without spawning it.
/// Keeping command selection pure makes the compatibility matrix testable
/// without launching an editor, terminal, or file manager on the test host.
fn build_open_in_app_command(
    path: &str,
    app: &str,
    line: Option<u32>,
    col: Option<u32>,
) -> Result<Command, String> {
    let cmd = match app {
        // CLI-based editors with --goto support (cross-platform, with path resolution)
        "vscode" => goto_editor_cmd("code", "Visual Studio Code", path, line, col),
        "cursor" => goto_editor_cmd("cursor", "Cursor", path, line, col),
        "windsurf" => goto_editor_cmd("windsurf", "Windsurf", path, line, col),
        // Zed uses path:line natively
        "zed" => {
            let mut c = Command::new(resolve_cli("zed"));
            c.arg(format_goto_arg(path, line, col));
            c
        }
        // Neovim uses +line
        "neovim" => {
            let mut c = Command::new(resolve_cli("nvim"));
            if let Some(l) = line {
                c.arg(format!("+{l}"));
            }
            c.arg(path);
            c
        }
        "smerge" => {
            let mut c = Command::new(resolve_cli("smerge"));
            c.arg(path);
            c
        }

        // JetBrains IDEs — CLI launchers with --line/--column, `open -a` fallback on macOS
        "intellij" => jetbrains_cmd("idea", "IntelliJ IDEA", path, line, col),
        "pycharm" => jetbrains_cmd("pycharm", "PyCharm", path, line, col),
        "webstorm" => jetbrains_cmd("webstorm", "WebStorm", path, line, col),
        "goland" => jetbrains_cmd("goland", "GoLand", path, line, col),
        "clion" => jetbrains_cmd("clion", "CLion", path, line, col),
        "phpstorm" => jetbrains_cmd("phpstorm", "PhpStorm", path, line, col),
        "rubymine" => jetbrains_cmd("rubymine", "RubyMine", path, line, col),
        "rider" => jetbrains_cmd("rider", "Rider", path, line, col),
        "datagrip" => jetbrains_cmd("datagrip", "DataGrip", path, line, col),
        "rustrover" => jetbrains_cmd("rustrover", "RustRover", path, line, col),
        "android-studio" => jetbrains_cmd("studio", "Android Studio", path, line, col),
        "fleet" => jetbrains_cmd("fleet", "Fleet", path, line, col),

        // Terminal emulators with CLI (cross-platform)
        "kitty" => {
            let mut c = Command::new(resolve_cli("kitty"));
            c.arg("--directory").arg(path);
            c
        }
        "wezterm" if has_cli("wezterm") => {
            let mut c = Command::new(resolve_cli("wezterm"));
            c.arg("start").arg("--cwd").arg(path);
            c
        }
        "alacritty" if has_cli("alacritty") => {
            let mut c = Command::new(resolve_cli("alacritty"));
            c.arg("--working-directory").arg(path);
            c
        }

        // macOS .app bundles (use 'open -a')
        app_name if cfg!(target_os = "macos") => match app_name {
            "xcode" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("Xcode").arg(path);
                c
            }
            "sourcetree" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("Sourcetree").arg(path);
                c
            }
            "github-desktop" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("GitHub Desktop").arg(path);
                c
            }
            "fork" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("Fork").arg(path);
                c
            }
            "gitkraken" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("GitKraken").arg(path);
                c
            }
            "ghostty" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("Ghostty").arg(path);
                c
            }
            "wezterm" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("WezTerm").arg(path);
                c
            }
            "alacritty" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("Alacritty").arg(path);
                c
            }
            "warp" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("Warp").arg(path);
                c
            }
            "iterm2" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("iTerm").arg(path);
                c
            }
            "tower" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("Tower").arg(path);
                c
            }
            "terminal" => {
                let mut c = Command::new("open");
                c.arg("-a").arg("Terminal").arg(path);
                c
            }
            "finder" => {
                let mut c = Command::new("open");
                c.arg(path);
                c
            }
            "editor" => {
                if let Ok(editor) = std::env::var("EDITOR") {
                    let mut c = Command::new(&editor);
                    if let Some(l) = line {
                        c.arg(format!("+{l}"));
                    }
                    c.arg(path);
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
                c.arg(path);
                c
            } else {
                return Err("No terminal emulator found".to_string());
            }
        }
        #[cfg(target_os = "linux")]
        "finder" => {
            let mut c = Command::new("xdg-open");
            c.arg(path);
            c
        }

        // Windows: system terminal, file manager, and app launchers
        #[cfg(target_os = "windows")]
        "terminal" => {
            // Prefer Windows Terminal (wt.exe) over cmd.exe
            if has_cli("wt") {
                let mut c = Command::new("wt");
                c.args(["-d", path]);
                c
            } else {
                let mut c = Command::new("cmd");
                c.args(["/c", "start", "cmd", "/k", "cd", "/d", path]);
                c
            }
        }
        #[cfg(target_os = "windows")]
        "finder" => {
            let mut c = Command::new("explorer");
            c.arg(path);
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
                c.arg(path);
                c
            } else {
                return Err(format!("{app_name} not found at {exe}"));
            }
        }

        _ => return Err(format!("Unknown app: {app}")),
    };
    Ok(cmd)
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
    let mut command = Command::new(resolve_cli(&executable));
    command.args(&expanded);
    tuic_core::child_reaping::spawn_reaped(command)
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
    pub(crate) supports_no_alt_screen: bool,
}

#[derive(Default)]
struct ScreenProbeState {
    known: Option<bool>,
    retry_after: Option<std::time::Instant>,
    warned: bool,
}

fn help_advertises_flag(text: &str, flag: &str) -> bool {
    text.match_indices(flag).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + flag.len()..].chars().next();
        !before.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
            && !after.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    })
}

fn agent_probe_command(path: &str) -> Command {
    #[cfg(windows)]
    if std::path::Path::new(path)
        .extension()
        .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("cmd"))
    {
        let mut cmd = Command::new(crate::fs::system32_exe("cmd.exe"));
        cmd.arg("/D").arg("/C").arg(path);
        return cmd;
    }
    Command::new(path)
}

#[derive(Debug)]
enum ScreenProbeError {
    TimedOut,
    Io(std::io::Error),
}

impl std::fmt::Display for ScreenProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TimedOut => write!(f, "--help timed out"),
            Self::Io(error) => error.fmt(f),
        }
    }
}

#[cfg(unix)]
pub(crate) struct ScreenProbeTree;

#[cfg(unix)]
impl ScreenProbeTree {
    pub(crate) fn prepare(cmd: &mut Command) -> std::io::Result<Self> {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
        Ok(Self)
    }

    pub(crate) fn assign(&self, _child: &std::process::Child) -> std::io::Result<()> {
        Ok(())
    }

    pub(crate) fn terminate(self, pid: u32) {
        // The probe owns this group: signal its grandchildren as well as the
        // direct child. The child is reaped separately below.
        unsafe { libc::kill(-(pid as libc::pid_t), libc::SIGKILL) };
    }
}

#[cfg(windows)]
pub(crate) struct ScreenProbeTree(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl ScreenProbeTree {
    pub(crate) fn prepare(cmd: &mut Command) -> std::io::Result<Self> {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };
        use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED};

        // Rust's Child does not expose its primary thread on stable Windows.
        // Suspend creation so no launcher can fork before job assignment.
        cmd.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);

        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let job = Self(handle);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let ok = unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&limits).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(job)
    }

    pub(crate) fn assign(&self, child: &std::process::Child) -> std::io::Result<()> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;

        if unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle()) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Self::resume_primary_thread(child.id())
    }

    fn resume_primary_thread(pid: u32) -> std::io::Result<()> {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
        };
        use windows_sys::Win32::System::Threading::{
            OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
        };

        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        let mut entry = THREADENTRY32 {
            dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        let mut found = None;
        let mut has_entry = unsafe { Thread32First(snapshot, &mut entry) };
        while has_entry != 0 {
            if entry.th32OwnerProcessID == pid {
                found = Some(entry.th32ThreadID);
                break;
            }
            has_entry = unsafe { Thread32Next(snapshot, &mut entry) };
        }
        unsafe { CloseHandle(snapshot) };
        let thread_id = found.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "probe primary thread not found",
            )
        })?;
        let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, thread_id) };
        if thread.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let previous_count = unsafe { ResumeThread(thread) };
        let error = if previous_count == u32::MAX {
            Some(std::io::Error::last_os_error())
        } else if previous_count != 1 {
            Some(std::io::Error::other(format!(
                "probe primary thread had suspend count {previous_count}"
            )))
        } else {
            None
        };
        unsafe { CloseHandle(thread) };
        if let Some(error) = error {
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn terminate(self, _pid: u32) {
        // Closing the last handle kills the whole job, including cmd.exe's
        // node.exe child when a Windows npm shim hangs on --help.
        drop(self);
    }
}

#[cfg(windows)]
impl Drop for ScreenProbeTree {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

// SAFETY: the job handle is owned, never borrowed, and all access by workflow
// checks is serialized by their process mutex.
#[cfg(windows)]
unsafe impl Send for ScreenProbeTree {}

/// A help probe owns and tears down its process tree, including descendants
/// that inherited stdout or stderr. The shared git deadline helper deliberately
/// has different child-only semantics, so screen probes keep this local.
pub(crate) fn cli_help(path: &str) -> Option<String> {
    let executable = resolve_probe_executable(path);
    let mut cmd = agent_probe_command(executable.to_str().unwrap_or(path));
    cmd.arg("--help");
    crate::cli::apply_no_window(&mut cmd);
    let output = screen_probe_output(&mut cmd, std::time::Duration::from_secs(2)).ok()?;
    output
        .status
        .success()
        .then(|| {
            format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        })
        .filter(|help| crate::agent_hook_launch::claude_help_is_usable(help))
}

fn screen_probe_output(
    cmd: &mut Command,
    timeout: std::time::Duration,
) -> Result<std::process::Output, ScreenProbeError> {
    use std::io::Read;
    use std::process::Stdio;

    let tree = ScreenProbeTree::prepare(cmd).map_err(ScreenProbeError::Io)?;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(ScreenProbeError::Io)?;
    if let Err(error) = tree.assign(&child) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(ScreenProbeError::Io(error));
    }
    let mut stdout = child.stdout.take().expect("stdout piped above");
    let mut stderr = child.stderr.take().expect("stderr piped above");
    let out_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let err_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        bytes
    });
    let deadline = std::time::Instant::now() + timeout;
    let mut poll = std::time::Duration::from_millis(1);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(
                    poll.min(deadline.saturating_duration_since(std::time::Instant::now())),
                );
                poll = (poll * 2).min(std::time::Duration::from_millis(50));
            }
            Ok(None) => {
                tree.terminate(child.id());
                let _ = child.kill();
                let _ = child.wait();
                return Err(ScreenProbeError::TimedOut);
            }
            Err(error) => {
                tree.terminate(child.id());
                let _ = child.kill();
                let _ = child.wait();
                return Err(ScreenProbeError::Io(error));
            }
        }
    };
    tree.terminate(child.id());
    Ok(std::process::Output {
        status,
        stdout: out_reader.join().unwrap_or_default(),
        stderr: err_reader.join().unwrap_or_default(),
    })
}

fn preferred_agent_path(output: &str) -> Option<&str> {
    let paths = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    #[cfg(windows)]
    {
        let choices: Vec<&str> = paths.collect();
        choices
            .iter()
            .copied()
            .find(|path| {
                std::path::Path::new(path)
                    .extension()
                    .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("exe"))
            })
            .or_else(|| {
                choices.iter().copied().find(|path| {
                    std::path::Path::new(path)
                        .extension()
                        .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("cmd"))
                })
            })
            .or_else(|| choices.first().copied())
    }
    #[cfg(not(windows))]
    {
        let mut paths = paths;
        paths.next()
    }
}

fn resolve_probe_executable(path: &str) -> std::path::PathBuf {
    let enriched = crate::cli::enriched_path();
    #[cfg(windows)]
    let suffixes = &["exe", "cmd"];
    #[cfg(not(windows))]
    let suffixes: &[&str] = &[];
    resolve_probe_executable_from_dirs(
        path,
        std::env::split_paths(std::ffi::OsStr::new(&enriched)),
        suffixes,
    )
}

fn resolve_probe_executable_from_dirs(
    path: &str,
    dirs: impl IntoIterator<Item = std::path::PathBuf>,
    suffixes: &[&str],
) -> std::path::PathBuf {
    let given = std::path::Path::new(path);
    if given.components().count() != 1 {
        return given.to_path_buf();
    }
    for dir in dirs {
        let candidate = dir.join(path);
        if (suffixes.is_empty() || given.extension().is_some()) && candidate.is_file() {
            return candidate;
        }
        for suffix in suffixes {
            let candidate = dir.join(format!("{path}.{suffix}"));
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    given.to_path_buf()
}

/// Probe the configured CLI with a bounded, single-flight help request. A
/// timeout is final for this binary version; quick inconclusive exits can be
/// retried after a short cooldown.
// DEFERRED (2026-09-25) — Codex `--no-alt-screen` and OpenCode `--mini`
// capture fixtures (story 939-475b): recording them launches a TUIC binary,
// which can rewrite the user's agent MCP configs until the mcp-config-guard
// fix (story 949-0421) lands. Capture them right after that landing.
pub(crate) fn supports_no_alt_screen(agent_type: &str, path: &str) -> bool {
    let binary_name = std::path::Path::new(agent_type)
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or(agent_type);
    let binary_name = binary_name.to_ascii_lowercase();
    let binary_name = binary_name
        .strip_suffix(".exe")
        .or_else(|| binary_name.strip_suffix(".cmd"))
        .unwrap_or(&binary_name);
    let Some((flag, _)) = crate::agent_hook_launch::screen_policy(binary_name) else {
        return false;
    };
    type Entry = Arc<Mutex<ScreenProbeState>>;
    static CACHE: OnceLock<Mutex<std::collections::HashMap<String, Entry>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let executable = resolve_probe_executable(path);
    let target = std::fs::canonicalize(&executable).unwrap_or_else(|_| executable.clone());
    let metadata = std::fs::metadata(&target).ok();
    let key = format!(
        "{binary_name}:{}:{:?}:{:?}",
        target.display(),
        metadata.as_ref().map(std::fs::Metadata::len),
        metadata.as_ref().and_then(|info| info.modified().ok())
    );
    let entry = cache
        .lock()
        .entry(key)
        .or_insert_with(|| Arc::new(Mutex::new(ScreenProbeState::default())))
        .clone();
    let mut state = entry.lock();
    if let Some(known) = state.known {
        return known;
    }
    if state
        .retry_after
        .is_some_and(|at| std::time::Instant::now() < at)
    {
        return false;
    }
    let mut cmd = agent_probe_command(executable.to_str().unwrap_or(path));
    cmd.arg("--help");
    let enriched = crate::cli::enriched_path();
    let mut dirs = Vec::new();
    if let Some(parent) = executable
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
    {
        dirs.push(parent.to_path_buf());
    }
    dirs.extend(std::env::split_paths(std::ffi::OsStr::new(&enriched)));
    cmd.env(
        "PATH",
        std::env::join_paths(dirs).unwrap_or_else(|_| std::ffi::OsString::from(enriched)),
    );
    crate::cli::apply_no_window(&mut cmd);
    let mut timed_out = false;
    let result = match screen_probe_output(&mut cmd, std::time::Duration::from_secs(2)) {
        Ok(output) => {
            let help = format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            if help_advertises_flag(&help, flag) {
                Ok(true)
            } else if output.status.success() && !help.trim().is_empty() {
                Ok(false)
            } else {
                Err(format!(
                    "--help exited {:?}: {}",
                    output.status.code(),
                    help.lines().next().unwrap_or("")
                ))
            }
        }
        Err(error) => {
            timed_out = matches!(error, ScreenProbeError::TimedOut);
            Err(error.to_string())
        }
    };
    match result {
        Ok(supported) => {
            state.known = Some(supported);
            supported
        }
        Err(error) => {
            if timed_out {
                state.known = Some(false);
            } else {
                state.retry_after =
                    Some(std::time::Instant::now() + std::time::Duration::from_millis(500));
            }
            if !state.warned {
                tracing::warn!(
                    source = "agent",
                    agent_type,
                    path,
                    error,
                    "Agent screen capability probe failed"
                );
                state.warned = true;
            }
            false
        }
    }
}

/// Agent binaries TUIC knows how to launch — the Rust-side mirror of `AGENTS` in
/// `src/agents.ts`. The MCP/HTTP detect surface reports exactly this set; anything missing
/// here is invisible to an orchestrator even when it is installed.
pub(crate) const KNOWN_AGENT_BINARIES: &[&str] = &[
    "claude",
    "codex",
    "gemini",
    "grok",
    "opencode",
    "amp",
    "cursor-agent",
    "goose",
    "droid",
    "pi",
    "ego",
];

pub(crate) fn is_known_agent_type(agent_type: &str) -> bool {
    let binary = if agent_type == "cursor" {
        "cursor-agent"
    } else {
        agent_type
    };
    KNOWN_AGENT_BINARIES.contains(&binary)
}

/// Detect any agent binary location
pub(crate) fn detect_agent_binary_sync(binary: String) -> AgentBinaryDetection {
    let direct_path = std::path::Path::new(&binary);
    if direct_path.is_absolute() && direct_path.is_file() {
        return AgentBinaryDetection {
            path: Some(binary.clone()),
            version: get_binary_version(&binary),
            supports_no_alt_screen: supports_no_alt_screen(&binary, &binary),
        };
    }
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
        let listed = String::from_utf8_lossy(&output.stdout);
        let path = preferred_agent_path(&listed).unwrap_or("").to_string();
        if !path.is_empty() && std::path::Path::new(&path).exists() {
            let version = get_binary_version(&path);
            let supports_no_alt_screen = supports_no_alt_screen(&binary, &path);
            return AgentBinaryDetection {
                path: Some(path),
                version,
                supports_no_alt_screen,
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
                supports_no_alt_screen: supports_no_alt_screen(&binary, candidate),
            };
        }
    }

    AgentBinaryDetection {
        path: None,
        version: None,
        supports_no_alt_screen: false,
    }
}

/// The desktop command runs on an async executor so CLI discovery and help
/// probing never park the WebView's command thread.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn detect_agent_binary(binary: String) -> AgentBinaryDetection {
    tokio::task::spawn_blocking(move || detect_agent_binary_sync(binary))
        .await
        .unwrap_or(AgentBinaryDetection {
            path: None,
            version: None,
            supports_no_alt_screen: false,
        })
}

/// Batch-detect multiple agent binaries in parallel.
/// Returns a map of binary name -> detection result.
/// Skips version detection for speed; use detect_agent_binary for full info.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn detect_all_agent_binaries(
    binaries: Vec<String>,
) -> std::collections::HashMap<String, AgentBinaryDetection> {
    let handles: Vec<_> = binaries
        .into_iter()
        .filter(|binary| !binary.trim().is_empty())
        .map(|binary| {
            tokio::task::spawn_blocking(move || {
                let detection = detect_binary_path_only(&binary);
                (binary, detection)
            })
        })
        .collect();

    let mut results = std::collections::HashMap::new();
    for handle in handles {
        if let Ok((binary, detection)) = handle.await {
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
        let listed = String::from_utf8_lossy(&output.stdout);
        let path = preferred_agent_path(&listed).unwrap_or("").to_string();
        if !path.is_empty() && std::path::Path::new(&path).exists() {
            return AgentBinaryDetection {
                path: Some(path),
                version: None,
                supports_no_alt_screen: false,
            };
        }
    }

    for candidate in &candidates {
        if !candidate.is_empty() && std::path::Path::new(candidate).exists() {
            return AgentBinaryDetection {
                path: Some(candidate.clone()),
                version: None,
                supports_no_alt_screen: false,
            };
        }
    }

    AgentBinaryDetection {
        path: None,
        version: None,
        supports_no_alt_screen: false,
    }
}

/// Get version of a binary (try --version or -v)
fn get_binary_version(path: &str) -> Option<String> {
    for flag in ["--version", "-v"] {
        let mut cmd = agent_probe_command(path);
        cmd.arg(flag).env("PATH", crate::cli::enriched_path());
        crate::cli::apply_no_window(&mut cmd);
        if let Ok(output) =
            crate::git_cli::output_with_deadline(&mut cmd, std::time::Duration::from_secs(2))
            && output.status.success()
        {
            let version = String::from_utf8_lossy(&output.stdout);
            let first_line = version.lines().next().unwrap_or("").trim();
            if !first_line.is_empty() {
                return Some(first_line.to_string());
            }
        }
    }
    None
}

/// Whether an agent's CLI accepts a bare positional prompt as its first argument.
///
/// Only Claude's CLI does. codex/gemini/goose require a subcommand or an
/// explicit flag, and hand a bare prompt straight to their clap parser, which
/// rejects it with a usage error and exits with code 2. Spawn paths use this to
/// fail fast instead of launching a process that dies immediately with no report.
pub(crate) fn agent_accepts_bare_prompt(agent_type: &str) -> bool {
    agent_type == "claude"
}

/// Default spawn arguments (with a `{prompt}` placeholder) for an agent launched
/// via MCP without an explicit run config. Mirrors the per-agent `spawnArgs` the
/// frontend uses in `src/agents.ts` — the authoritative, shipped knowledge of
/// each CLI's invocation. Lets `agent action=spawn agent_type=codex` work out of
/// the box. Model/print flags are layered on separately by
/// `merge_mcp_params_into_args`, so this only encodes the prompt-carrying shape.
/// Returns `None` for agents we can't launch non-interactively (caller must pass
/// explicit `args`).
pub(crate) fn default_prompt_args(agent_type: &str) -> Option<Vec<String>> {
    let args: &[&str] = match agent_type {
        // Claude: bare positional prompt, submitted and run immediately.
        // (Folded from the old dedicated spawn branch — story 092. Flag order
        // is preserved by merge_mcp_params_into_args's claude flags-first rule.)
        "claude" => &["{prompt}"],
        // Positional prompt (interactive with the task pre-filled).
        "gemini" | "codex" | "opencode" | "grok" | "amp" | "cursor" | "droid" => &["{prompt}"],
        // Goose 1.49: `session` takes no positional prompt (exit 2); `run -s -t`
        // runs the text, then stays interactive.
        "goose" => &["run", "-s", "-t", "{prompt}"],
        _ => return None,
    };
    Some(args.iter().map(|s| s.to_string()).collect())
}

/// Agents whose positional argv cannot carry the task for an orchestrated
/// spawn (verified live, story 091):
/// - codex 0.142: the positional prompt only PREFILLS the interactive TUI input
///   without submitting — the child parks at its ready prompt forever.
/// - opencode: the default positional is a PROJECT PATH (`opencode [project]`),
///   so a prompt argv crashes it with ENAMETOOLONG; `opencode run` is one-shot.
///
/// For these agents the spawn path launches the bare TUI and delivers the
/// initial prompt through the pending-injection path (bracketed-paste + CR
/// split write) once the TUI reaches its first idle, unifying initial-task
/// delivery with peer-message delivery. One-shot subcommands (`codex exec`,
/// `opencode run`) are NOT an alternative here: they exit after the task,
/// collapsing the persistent session an orchestrator needs to keep messaging.
pub(crate) fn prompt_prefill_only(agent_type: &str) -> bool {
    matches!(agent_type, "codex" | "opencode")
}

/// Detect claude binary location (legacy, delegates to detect_agent_binary)
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn detect_claude_binary() -> Result<String, String> {
    let detection = detect_agent_binary("claude".to_string()).await;
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
        let detection = detect_agent_binary(agent_type.clone()).await;
        detection
            .path
            .ok_or_else(|| format!("Agent binary '{agent_type}' not found"))?
    } else {
        detect_claude_binary().await?
    };

    let session_id = Uuid::new_v4().to_string();

    let attachment_dir =
        crate::attachments::prepare_session_directory(&state.data_dir, "pty", &session_id)?;

    let spawn_binary_path = binary_path.clone();
    let spawn_agent_config = agent_config.clone();
    let spawn_pty_config = pty_config.clone();
    let state_for_env = state.inner().clone();
    let session_id_for_env = session_id.clone();
    let spawn_tuic_session = pty_config.tuic_session.clone();
    let launch_receipt = Arc::new(Mutex::new(None));
    let captured_receipt = Arc::clone(&launch_receipt);
    let (pair, child) = crate::pty::spawn_pty_pair_with_retry_async(
        PtySize {
            rows: pty_config.rows,
            cols: pty_config.cols,
            pixel_width: 0,
            pixel_height: 0,
        },
        move || {
            // Build agent command
            let mut cmd = CommandBuilder::new(&spawn_binary_path);
            crate::pty::sanitize_pty_parent_env(&mut cmd);

            let mut launch_args = Vec::new();
            if let Some(ref args) = spawn_agent_config.args {
                launch_args.extend(args.iter().cloned());
            } else {
                // Default Claude-style args for backward compatibility
                if spawn_agent_config.print_mode {
                    launch_args.push("--print".to_string());
                }

                if let Some(ref format) = spawn_agent_config.output_format {
                    launch_args.push("--output-format".to_string());
                    launch_args.push(format.clone());
                }

                if let Some(ref model) = spawn_agent_config.model {
                    launch_args.push("--model".to_string());
                    launch_args.push(model.clone());
                }

                // Add prompt
                launch_args.push(spawn_agent_config.prompt.clone());
            }
            let agent_type = spawn_agent_config.agent_type.as_deref().unwrap_or("claude");
            let final_args = crate::agent_hook_launch::launch_args(
                agent_type,
                &spawn_binary_path,
                &launch_args,
                &crate::config::config_dir(),
                &attachment_dir,
            );
            crate::launch_instructions::apply_env(
                &mut cmd,
                agent_type,
                &launch_args,
                &crate::config::config_dir(),
            );
            *captured_receipt.lock() = Some(crate::prompt_receipt::PromptReceipt::capture(
                &spawn_agent_config.prompt,
                "TUIC spawn_agent",
                &final_args,
                spawn_agent_config
                    .cwd
                    .as_deref()
                    .or(spawn_pty_config.cwd.as_deref()),
                false,
            ));
            for arg in final_args {
                cmd.arg(arg);
            }

            if let Some(ref cwd) = spawn_agent_config.cwd {
                cmd.cwd(crate::cli::expand_tilde(cwd));
            } else if let Some(ref cwd) = spawn_pty_config.cwd {
                cmd.cwd(crate::cli::expand_tilde(cwd));
            }

            crate::pty::apply_agent_screen_env(&mut cmd, &spawn_pty_config.env);
            // Inject env flags (feature flags configured in Settings → Agents)
            for (key, value) in &spawn_pty_config.env {
                cmd.env(key, value);
            }
            crate::pty::bind_pty_identity(
                &state_for_env,
                &mut cmd,
                &session_id_for_env,
                spawn_tuic_session.as_deref(),
            );
            cmd
        },
    )
    .await?;

    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("Failed to get PTY writer: {e}"))?;

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("Failed to get PTY reader: {e}"))?;

    let mut session_state = crate::state::SessionState {
        spawn_root_role: crate::state::SpawnRootRole::DirectProgram,
        ..Default::default()
    };
    session_state.seed_configured_agent(agent_config.agent_type.clone());
    session_state.hook_instrumented = crate::pty::hook_instrumented_for(
        &crate::config::load_agents_config(),
        agent_config.agent_type.as_deref(),
    );
    state
        .session_maps
        .session_states
        .insert(session_id.clone(), session_state);

    // Store session (master handle kept for resize support)
    let paused = Arc::new(AtomicBool::new(false));
    state.session_maps.sessions.insert(
        session_id.clone(),
        Mutex::new(PtySession {
            launch_receipt: launch_receipt.lock().take(),
            writer: Arc::new(Mutex::new(writer)),
            master: pair.master,
            _child: child,
            paused: paused.clone(),
            worktree: None,
            initial_cwd: agent_config.cwd.clone(),
            cwd: agent_config.cwd.clone(),
            display_name: None,
            display_name_is_custom: false,
            display_name_from_spawn: false,
            is_remote: false,
            shell: binary_path.clone(),
        }),
    );
    crate::prompt_receipt::adopt_mcp_instructions(&state, &session_id);
    state.metrics.total_spawned.fetch_add(1, Ordering::Relaxed);
    state
        .metrics
        .active_sessions
        .fetch_add(1, Ordering::Relaxed);

    // Create ring buffer and VT log buffer for this session
    state.session_maps.output_buffers.insert(
        session_id.clone(),
        Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)),
    );
    state.grid.vt_log_buffers.insert(
        session_id.clone(),
        Mutex::new(state.new_vt_log_buffer(
            pty_config.rows,
            pty_config.cols,
            VT_LOG_BUFFER_CAPACITY,
        )),
    );
    state
        .session_maps
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

    #[test]
    fn windows_probe_resolution_prefers_runnable_shims_over_extensionless_script() {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let npm = root.path().join("npm");
        std::fs::create_dir(&npm).unwrap();
        std::fs::write(npm.join("codex"), "#!/bin/sh\n").unwrap();
        std::fs::write(npm.join("codex.cmd"), "@echo off\r\n").unwrap();
        assert_eq!(
            resolve_probe_executable_from_dirs("codex", [npm.clone()], &["exe", "cmd"]),
            npm.join("codex.cmd")
        );

        std::fs::write(npm.join("codex.exe"), []).unwrap();
        assert_eq!(
            resolve_probe_executable_from_dirs("codex", [npm.clone()], &["exe", "cmd"]),
            npm.join("codex.exe")
        );
        assert_eq!(
            resolve_probe_executable_from_dirs("codex.cmd", [npm.clone()], &["exe", "cmd"]),
            npm.join("codex.cmd")
        );

        std::fs::remove_file(npm.join("codex.exe")).unwrap();
        std::fs::remove_file(npm.join("codex.cmd")).unwrap();
        assert_eq!(
            resolve_probe_executable_from_dirs("codex", [npm.clone()], &["exe", "cmd"]),
            std::path::PathBuf::from("codex")
        );

        let later = root.path().join("later");
        std::fs::create_dir(&later).unwrap();
        std::fs::write(later.join("codex.cmd"), "@echo off\r\n").unwrap();
        assert_eq!(
            resolve_probe_executable_from_dirs(
                "codex",
                [npm.clone(), later.clone()],
                &["exe", "cmd"]
            ),
            later.join("codex.cmd")
        );
        assert_eq!(
            resolve_probe_executable_from_dirs("codex", [npm.clone()], &[]),
            npm.join("codex")
        );
    }

    #[test]
    fn screen_help_flag_matching_requires_option_boundaries() {
        assert!(help_advertises_flag("  --mini  compact mode", "--mini"));
        assert!(help_advertises_flag("[--no-alt-screen]", "--no-alt-screen"));
        assert!(!help_advertises_flag("--minimal", "--mini"));
        assert!(!help_advertises_flag(
            "--no-alt-screen-extra",
            "--no-alt-screen"
        ));
    }

    // Catches: failed help probes log a generic warning but lose the timeout or IO cause.
    #[cfg(unix)]
    #[test]
    fn screen_probe_errors_keep_timeout_and_io_diagnostics() {
        #[derive(Clone)]
        struct Sink(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Sink {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.lock().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Sink {
            type Writer = Sink;
            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        let script = crate::test_support::fake_ssh_script(
            "screen-probe-warning-timeout",
            "exec sleep 8",
            "echo --no-alt-screen",
        );
        let dir = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
        let missing = dir.path().join("missing-codex");
        let output = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_writer(Sink(output.clone()))
            .with_ansi(false)
            .without_time()
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            assert!(!supports_no_alt_screen("codex", &script.to_string_lossy()));
            assert!(!supports_no_alt_screen("codex", &missing.to_string_lossy()));
        });
        let log = String::from_utf8(output.lock().clone()).unwrap();
        let warnings: Vec<_> = log
            .lines()
            .filter(|line| line.contains("Agent screen capability probe failed"))
            .collect();
        assert_eq!(warnings.len(), 2, "{log}");
        assert!(warnings[0].contains("WARN"), "{log}");
        assert!(warnings[0].contains("--help timed out"), "{log}");
        assert!(warnings[1].contains("WARN"), "{log}");
        assert!(warnings[1].contains("os error 2"), "{log}");
    }

    #[cfg(unix)]
    #[test]
    fn screen_help_probe_has_a_deadline_and_reaps_its_child() {
        let script = crate::test_support::fake_ssh_script(
            "screen-help-timeout",
            "exec sleep 8",
            "echo --no-alt-screen",
        );
        let start = std::time::Instant::now();
        assert!(!supports_no_alt_screen("codex", &script.to_string_lossy()));
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
    }

    #[cfg(unix)]
    #[test]
    fn timed_out_screen_help_is_probed_once_per_binary_version() {
        let script = crate::test_support::fake_ssh_script(
            "screen-help-hang-version",
            "printf x >> \"${0%/*}/screen-help-hang-version.count\"; sleep 8",
            "echo --no-alt-screen",
        );
        let marker = script.with_file_name("screen-help-hang-version.count");
        let _ = std::fs::remove_file(&marker);
        let path = script.to_string_lossy();
        assert!(!supports_no_alt_screen("codex", &path));
        std::thread::sleep(std::time::Duration::from_millis(750));
        assert!(!supports_no_alt_screen("codex", &path));
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "x");

        let replacement = crate::test_support::fake_ssh_script(
            "screen-help-hang-version",
            "printf '%s\\n' '--no-alt-screen'",
            "echo --no-alt-screen",
        );
        assert_eq!(script, replacement);
        assert!(supports_no_alt_screen("codex", &path));
    }

    #[cfg(unix)]
    #[test]
    fn timed_out_screen_help_stops_its_grandchild() {
        let script = crate::test_support::fake_ssh_script(
            "screen-help-hang-tree",
            "(i=0; while [ \"$i\" -lt 35 ]; do printf x >> \"${0%/*}/screen-help-hang-tree.count\"; i=$((i + 1)); sleep 0.1; done) & wait",
            "echo --no-alt-screen",
        );
        let marker = script.with_file_name("screen-help-hang-tree.count");
        let _ = std::fs::remove_file(&marker);
        assert!(!supports_no_alt_screen("codex", &script.to_string_lossy()));
        let at_timeout = std::fs::read(&marker).unwrap().len();
        assert!(at_timeout > 0, "the grandchild must have run");
        std::thread::sleep(std::time::Duration::from_millis(400));
        assert_eq!(
            std::fs::read(&marker).unwrap().len(),
            at_timeout,
            "the timed-out probe must stop its grandchild"
        );
    }

    #[cfg(unix)]
    #[test]
    fn agent_version_detection_does_not_wait_for_a_hung_version_flag() {
        let script = crate::test_support::fake_ssh_script(
            "agent-version-timeout",
            "if [ \"$1\" = '--version' ]; then exec sleep 8; fi; echo 0.1.0",
            "echo 0.1.0",
        );
        let start = std::time::Instant::now();
        assert_eq!(
            get_binary_version(&script.to_string_lossy()).as_deref(),
            Some("0.1.0")
        );
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
    }

    #[cfg(unix)]
    #[test]
    fn concurrent_screen_help_requests_share_one_probe() {
        let script = crate::test_support::fake_ssh_script(
            "screen-help-single-flight",
            "printf x >> \"${0%/*}/screen-help-single-flight.count\"; sleep 1; printf '%s\\n' '--no-alt-screen'",
            "echo --no-alt-screen",
        );
        let marker = script.with_file_name("screen-help-single-flight.count");
        let _ = std::fs::remove_file(&marker);
        let path = script.to_string_lossy().into_owned();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    supports_no_alt_screen("codex", &path)
                })
            })
            .collect();
        barrier.wait();
        for handle in handles {
            assert!(handle.join().unwrap());
        }
        assert_eq!(std::fs::read_to_string(marker).unwrap(), "x");
    }

    #[cfg(unix)]
    #[test]
    // Catches: subtracting the cooldown makes an inconclusive probe retry immediately.
    fn failed_screen_help_probe_waits_for_cooldown_before_retrying() {
        let script = crate::test_support::fake_ssh_script(
            "screen-help-retry",
            "marker=\"${0%/*}/screen-help-retry.ready\"; if [ ! -f \"$marker\" ]; then touch \"$marker\"; exit 1; fi; printf '%s\\n' '--no-alt-screen'",
            "echo --no-alt-screen",
        );
        let marker = script.with_file_name("screen-help-retry.ready");
        let _ = std::fs::remove_file(&marker);
        let path = script.to_string_lossy();
        assert!(!supports_no_alt_screen("codex", &path));
        assert!(!supports_no_alt_screen("codex", &path));
        std::thread::sleep(std::time::Duration::from_millis(750));
        assert!(supports_no_alt_screen("codex", &path));
    }

    #[cfg(unix)]
    #[test]
    fn empty_successful_help_is_inconclusive_and_retried() {
        let script = crate::test_support::fake_ssh_script(
            "screen-help-empty",
            "marker=\"${0%/*}/screen-help-empty.ready\"; if [ ! -f \"$marker\" ]; then touch \"$marker\"; exit 0; fi; printf '%s\\n' '--no-alt-screen'",
            "echo --no-alt-screen",
        );
        let marker = script.with_file_name("screen-help-empty.ready");
        let _ = std::fs::remove_file(&marker);
        let path = script.to_string_lossy();
        assert!(!supports_no_alt_screen("codex", &path));
        std::thread::sleep(std::time::Duration::from_millis(750));
        assert!(supports_no_alt_screen("codex", &path));
    }

    #[cfg(unix)]
    #[test]
    fn screen_help_probe_finds_sibling_tools_missing_from_parent_path() {
        let helper = crate::test_support::fake_ssh_script(
            "screen-help-sibling-helper",
            "exit 0",
            "exit /b 0",
        );
        let helper_name = helper.file_name().unwrap().to_string_lossy();
        let script = crate::test_support::fake_ssh_script(
            "screen-help-sibling-path",
            &format!(
                "command -v '{helper_name}' >/dev/null || exit 1; printf '%s\\n' '--no-alt-screen'"
            ),
            "echo --no-alt-screen",
        );
        assert!(supports_no_alt_screen("codex", &script.to_string_lossy()));
    }

    #[cfg(unix)]
    #[test]
    fn screen_help_probe_accepts_a_flag_printed_on_stderr() {
        let script = crate::test_support::fake_ssh_script(
            "screen-help-stderr",
            "printf '%s\\n' '--mini' >&2; exit 1",
            "echo --mini",
        );
        assert!(supports_no_alt_screen(
            "opencode",
            &script.to_string_lossy()
        ));
    }

    #[cfg(unix)]
    #[test]
    fn replacing_an_agent_binary_invalidates_its_screen_capability() {
        let old = crate::test_support::fake_ssh_script(
            "screen-help-upgrade",
            "printf '%s\\n' 'old help'",
            "echo old help",
        );
        assert!(!supports_no_alt_screen("codex", &old.to_string_lossy()));
        let new = crate::test_support::fake_ssh_script(
            "screen-help-upgrade",
            "printf '%s\\n' 'new help --no-alt-screen'",
            "echo new help --no-alt-screen",
        );
        assert_eq!(old, new);
        assert!(supports_no_alt_screen("codex", &new.to_string_lossy()));
    }

    // resolve_cli and extra_bin_dirs tests are now in cli.rs

    #[test]
    fn direct_agent_binary_path_is_detected_without_using_path_search() {
        let path = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let detected = detect_agent_binary_sync(path.clone());
        assert_eq!(detected.path.as_deref(), Some(path.as_str()));
        assert!(!detected.supports_no_alt_screen);
    }

    /// The MCP/HTTP detect surface reports KNOWN_AGENT_BINARIES verbatim, so an agent added to
    /// the frontend registry but not here is installed-yet-invisible to an orchestrator.
    /// `git` and the empty-binary `api` entry are not agent CLIs.
    #[test]
    fn known_agent_binaries_cover_the_frontend_registry() {
        let agents_ts = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/agents.ts"),
        )
        .expect("read src/agents.ts");

        let declared: Vec<&str> = agents_ts
            .lines()
            .filter_map(|l| l.trim().strip_prefix("binary: \""))
            .filter_map(|rest| rest.split('"').next())
            .filter(|b| !b.is_empty() && *b != "git")
            .collect();

        assert!(
            !declared.is_empty(),
            "parser found no binaries — did the shape of agents.ts change?"
        );
        for binary in declared {
            assert!(
                KNOWN_AGENT_BINARIES.contains(&binary),
                "agents.ts declares '{binary}' but KNOWN_AGENT_BINARIES omits it, so `agent detect` will never report it"
            );
        }
    }

    // Catches: a removed harness still reported by MCP/HTTP agent detection.
    #[test]
    fn known_agent_binaries_exclude_removed_aider() {
        assert!(!KNOWN_AGENT_BINARIES.contains(&"aider"));
    }

    #[tokio::test]
    async fn batch_agent_detection_uses_async_runtime_and_skips_empty_names() {
        let detections = detect_all_agent_binaries(vec![String::new(), "git".to_string()]).await;

        assert!(!detections.contains_key(""));
        assert!(detections.contains_key("git"));
    }

    #[test]
    fn only_claude_accepts_a_bare_prompt() {
        assert!(agent_accepts_bare_prompt("claude"));
        // Every other known agent CLI would exit with code 2 on a bare prompt.
        for agent in ["codex", "gemini", "goose", "opencode", "amp"] {
            assert!(
                !agent_accepts_bare_prompt(agent),
                "{agent} must not be treated as accepting a bare prompt"
            );
        }
    }

    #[test]
    fn default_prompt_args_cover_known_agents() {
        // Positional-prompt agents.
        for agent in [
            "gemini", "codex", "opencode", "grok", "amp", "cursor", "droid",
        ] {
            assert_eq!(
                default_prompt_args(agent),
                Some(vec!["{prompt}".to_string()]),
                "{agent} should get a positional prompt template"
            );
        }
        // Goose 1.49: `goose session <prompt>` exits 2 (unrecognized subcommand);
        // `run -s -t` runs the text and stays interactive.
        assert_eq!(
            default_prompt_args("goose"),
            Some(["run", "-s", "-t", "{prompt}"].map(String::from).to_vec())
        );
        // Every template must contain the placeholder so substitution works.
        for agent in ["gemini", "codex", "goose"] {
            assert!(
                default_prompt_args(agent)
                    .unwrap()
                    .iter()
                    .any(|a| a.contains("{prompt}")),
                "{agent} template must carry the {{prompt}} placeholder"
            );
        }
        // Claude: folded into the table (story 092) — bare positional prompt.
        assert_eq!(
            default_prompt_args("claude"),
            Some(vec!["{prompt}".to_string()])
        );
        // Unknown agents have no template → caller must pass explicit args.
        assert_eq!(default_prompt_args("totally-unknown"), None);
    }

    #[tokio::test]
    async fn test_detect_claude_binary() {
        // This test checks that detect_claude_binary returns a result
        // It may succeed or fail depending on whether claude is installed
        let result = detect_claude_binary().await;
        // We just verify it doesn't panic and returns a proper Result
        match result {
            Ok(path) => {
                assert_ne!(path, "");
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
        assert_ne!(out[0], "");
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

    fn command_args(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn open_in_app_builds_zed_location_without_spawning() {
        let command = build_open_in_app_command("/repo/src/main.rs", "zed", Some(42), Some(7))
            .expect("zed command");

        assert_eq!(command_args(&command), ["/repo/src/main.rs:42:7"]);
    }

    #[test]
    fn open_in_app_builds_neovim_line_without_column() {
        let command = build_open_in_app_command("/repo/src/main.rs", "neovim", Some(42), Some(7))
            .expect("neovim command");

        assert_eq!(command_args(&command), ["+42", "/repo/src/main.rs"]);
    }

    #[test]
    fn open_in_app_builds_kitty_working_directory() {
        let command =
            build_open_in_app_command("/repo", "kitty", None, None).expect("kitty command");

        assert_eq!(command_args(&command), ["--directory", "/repo"]);
    }

    #[test]
    fn open_in_app_rejects_unknown_application_before_spawning() {
        let error = build_open_in_app_command("/repo", "not-a-real-app", None, None)
            .expect_err("unknown applications must be rejected");

        assert_eq!(error, "Unknown app: not-a-real-app");
    }

    #[test]
    fn open_in_custom_rejects_empty_executable() {
        let err = open_in_custom("  ".to_string(), vec![], ctx(None, "/x", None, None, None));
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("no executable"));
    }
}
