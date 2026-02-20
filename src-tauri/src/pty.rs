use parking_lot::Mutex;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::Serialize;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::output_parser::{extract_last_question_line, OutputParser, ParsedEvent};
use crate::state::{
    AppState, EscapeAwareBuffer, OrchestratorStats, OutputRingBuffer, PtyConfig, PtyOutput,
    PtySession, Utf8ReadBuffer, MAX_CONCURRENT_SESSIONS, OUTPUT_RING_BUFFER_CAPACITY,
};
use crate::worktree::{create_worktree_internal, remove_worktree_internal, WorktreeConfig, WorktreeResult};

/// Get the platform-appropriate default shell when no override is configured.
pub(crate) fn default_shell() -> String {
    #[cfg(windows)]
    {
        std::env::var("COMSPEC").unwrap_or_else(|_| "powershell.exe".to_string())
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
    }
}

/// Build a CommandBuilder for the given shell with platform-appropriate flags.
pub(crate) fn build_shell_command(shell: &str) -> CommandBuilder {
    #[allow(unused_mut)]
    let mut cmd = CommandBuilder::new(shell);
    // Login shell flag is Unix-only; PowerShell/cmd.exe don't support -l
    #[cfg(not(windows))]
    cmd.arg("-l");
    // GUI-launched Tauri apps don't inherit terminal env vars from a parent shell.
    // Set the essentials so tools detect color/encoding support correctly.
    #[cfg(not(windows))]
    {
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        if let Ok(lang) = std::env::var("LANG") {
            cmd.env("LANG", lang);
        } else {
            // Fallback: ensure UTF-8 is available even when LANG is completely unset
            cmd.env("LANG", "en_US.UTF-8");
        }
    }
    // Prevent macOS from sourcing /etc/zshrc_Apple_Terminal which prints
    // a spurious "Restored session:" message on every new shell
    #[cfg(target_os = "macos")]
    cmd.env("TERM_PROGRAM", "tui-commander");
    cmd
}

/// Resolve the shell to use: explicit override > env default > platform default.
pub(crate) fn resolve_shell(override_shell: Option<String>) -> String {
    override_shell.unwrap_or_else(default_shell)
}

/// How long the agent must be silent after printing a `?`-ending line before
/// we treat it as a question waiting for input.
const SILENCE_QUESTION_THRESHOLD: std::time::Duration = std::time::Duration::from_secs(5);

/// How often the timer thread wakes up to check for silence.
const SILENCE_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// Shared state between the PTY reader thread and the silence-detection timer thread.
pub(crate) struct SilenceState {
    /// When the last chunk of output was received from the PTY.
    pub(crate) last_output_at: std::time::Instant,
    /// The last line ending with `?` that hasn't been resolved yet.
    pub(crate) pending_question_line: Option<String>,
    /// Whether a Question event has already been emitted for the current pending line
    /// (either by the instant regex detector or by the silence timer).
    pub(crate) question_already_emitted: bool,
}

impl SilenceState {
    fn new() -> Self {
        Self {
            last_output_at: std::time::Instant::now(),
            pending_question_line: None,
            question_already_emitted: false,
        }
    }

    /// Called by the reader thread after each chunk.
    /// `regex_found_question`: true if `parse()` already emitted a Question event.
    /// `last_question_line`: the last `?`-ending line in the chunk, if any.
    pub(crate) fn on_chunk(&mut self, regex_found_question: bool, last_question_line: Option<String>) {
        self.last_output_at = std::time::Instant::now();

        if regex_found_question {
            // The instant detector already fired — suppress the silence timer.
            self.pending_question_line = None;
            self.question_already_emitted = true;
        } else if let Some(line) = last_question_line {
            // New candidate for silence-based detection.
            self.pending_question_line = Some(line);
            self.question_already_emitted = false;
        } else {
            // The agent printed more output that doesn't end with `?` — it moved on.
            self.pending_question_line = None;
        }
    }

    /// Called by the timer thread. Returns the question text if the silence
    /// threshold has been reached and we haven't emitted yet.
    pub(crate) fn check_silence(&mut self) -> Option<String> {
        if self.question_already_emitted {
            return None;
        }
        if let Some(ref line) = self.pending_question_line
            && self.last_output_at.elapsed() >= SILENCE_QUESTION_THRESHOLD
        {
            self.question_already_emitted = true;
            return Some(line.clone());
        }
        None
    }
}

/// Spawn a reader thread that reads from a PTY, emits Tauri events, and writes to the ring buffer.
/// Shared by `create_pty`, `create_pty_with_worktree`, and `spawn_agent` to avoid duplication.
pub(crate) fn spawn_reader_thread(
    mut reader: Box<dyn Read + Send>,
    paused: Arc<AtomicBool>,
    session_id: String,
    app: AppHandle,
    state: Arc<AppState>,
) {
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    let running = Arc::new(AtomicBool::new(true));

    // Spawn silence-detection timer thread
    {
        let silence = silence.clone();
        let running = running.clone();
        let app = app.clone();
        let session_id = session_id.clone();
        std::thread::spawn(move || {
            while running.load(Ordering::Relaxed) {
                std::thread::sleep(SILENCE_CHECK_INTERVAL);
                if !running.load(Ordering::Relaxed) {
                    break;
                }
                let question = silence.lock().check_silence();
                if let Some(prompt_text) = question {
                    let _ = app.emit(
                        &format!("pty-parsed-{session_id}"),
                        &ParsedEvent::Question { prompt_text },
                    );
                }
            }
        });
    }

    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut utf8_buf = Utf8ReadBuffer::new();
        let mut esc_buf = EscapeAwareBuffer::new();
        let parser = OutputParser::new();
        loop {
            while paused.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    state.metrics.bytes_emitted.fetch_add(n, Ordering::Relaxed);
                    let utf8_data = utf8_buf.push(&buf[..n]);
                    let data = esc_buf.push(&utf8_data);
                    if !data.is_empty() {
                        // Write to ring buffer for MCP consumers
                        if let Some(ring) = state.output_buffers.get(&session_id) {
                            ring.lock().write(data.as_bytes());
                        }
                        // Broadcast to WebSocket clients
                        if let Some(mut clients) = state.ws_clients.get_mut(&session_id) {
                            clients.retain(|tx| tx.send(data.clone()).is_ok());
                        }
                        // Emit parsed events before raw output
                        let events = parser.parse(&data);
                        let regex_found_question = events.iter().any(|e| matches!(e, ParsedEvent::Question { .. }));
                        for event in &events {
                            let _ = app.emit(
                                &format!("pty-parsed-{session_id}"),
                                event,
                            );
                        }

                        // Update silence state for fallback question detection
                        let last_q_line = extract_last_question_line(&data);
                        silence.lock().on_chunk(regex_found_question, last_q_line);

                        let _ = app.emit(
                            &format!("pty-output-{session_id}"),
                            PtyOutput {
                                session_id: session_id.clone(),
                                data,
                            },
                        );
                    }
                }
                Err(_) => break,
            }
        }
        // Signal timer thread to stop
        running.store(false, Ordering::Relaxed);

        // Flush both buffers at EOF
        let utf8_tail = utf8_buf.flush();
        let mut remaining = if utf8_tail.is_empty() {
            esc_buf.flush()
        } else {
            let mut flushed = esc_buf.push(&utf8_tail);
            flushed.push_str(&esc_buf.flush());
            flushed
        };
        if remaining.is_empty() {
            remaining = String::new();
        }
        if !remaining.is_empty() {
            if let Some(ring) = state.output_buffers.get(&session_id) {
                ring.lock().write(remaining.as_bytes());
            }
            if let Some(mut clients) = state.ws_clients.get_mut(&session_id) {
                clients.retain(|tx| tx.send(remaining.clone()).is_ok());
            }
            let _ = app.emit(
                &format!("pty-output-{session_id}"),
                PtyOutput {
                    session_id: session_id.clone(),
                    data: remaining,
                },
            );
        }
        let _ = app.emit(
            &format!("pty-exit-{session_id}"),
            serde_json::json!({ "session_id": session_id }),
        );
        state.sessions.remove(&session_id);
        state.output_buffers.remove(&session_id);
        state.ws_clients.remove(&session_id);
        state.metrics.active_sessions.fetch_sub(1, Ordering::Relaxed);
    });
}

/// Reader thread for sessions created via HTTP (no AppHandle available).
/// Writes to ring buffer only — MCP consumers poll via GET /sessions/{id}/output.
pub(crate) fn spawn_headless_reader_thread(
    mut reader: Box<dyn Read + Send>,
    paused: Arc<AtomicBool>,
    session_id: String,
    state: Arc<AppState>,
) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut utf8_buf = Utf8ReadBuffer::new();
        let mut esc_buf = EscapeAwareBuffer::new();
        loop {
            while paused.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    state.metrics.bytes_emitted.fetch_add(n, Ordering::Relaxed);
                    let utf8_data = utf8_buf.push(&buf[..n]);
                    let data = esc_buf.push(&utf8_data);
                    if !data.is_empty() {
                        if let Some(ring) = state.output_buffers.get(&session_id) {
                            ring.lock().write(data.as_bytes());
                        }
                        // Broadcast to WebSocket clients
                        if let Some(mut clients) = state.ws_clients.get_mut(&session_id) {
                            clients.retain(|tx| tx.send(data.clone()).is_ok());
                        }
                    }
                }
                Err(_) => break,
            }
        }
        let utf8_tail = utf8_buf.flush();
        let remaining = if utf8_tail.is_empty() {
            esc_buf.flush()
        } else {
            let mut flushed = esc_buf.push(&utf8_tail);
            flushed.push_str(&esc_buf.flush());
            flushed
        };
        if !remaining.is_empty() {
            if let Some(ring) = state.output_buffers.get(&session_id) {
                ring.lock().write(remaining.as_bytes());
            }
            if let Some(mut clients) = state.ws_clients.get_mut(&session_id) {
                clients.retain(|tx| tx.send(remaining.clone()).is_ok());
            }
        }
        state.sessions.remove(&session_id);
        state.output_buffers.remove(&session_id);
        state.ws_clients.remove(&session_id);
        state.metrics.active_sessions.fetch_sub(1, Ordering::Relaxed);
    });
}

/// Create a new PTY session with optional worktree
#[tauri::command]
pub(crate) async fn create_pty(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    config: PtyConfig,
) -> Result<String, String> {
    let session_id = Uuid::new_v4().to_string();
    let pty_system = native_pty_system();

    let shell = resolve_shell(config.shell);

    // Guard against invalid dimensions from zero-sized windows
    let rows = config.rows.max(24);
    let cols = config.cols.max(80);

    // Retry PTY spawn up to 3 times with increasing delay (Story 059)
    let max_retries = 3;
    let mut last_err = String::new();
    let mut pair_and_child = None;

    for attempt in 0..max_retries {
        let pair = match pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            Ok(p) => p,
            Err(e) => {
                last_err = format!("Failed to open PTY (attempt {}): {}", attempt + 1, e);
                if attempt < max_retries - 1 {
                    tokio::time::sleep(std::time::Duration::from_millis(100 * (attempt as u64 + 1))).await;
                }
                continue;
            }
        };

        let mut cmd = build_shell_command(&shell);

        if let Some(ref cwd) = config.cwd {
            cmd.cwd(cwd);
        }

        match pair.slave.spawn_command(cmd) {
            Ok(child) => {
                pair_and_child = Some((pair, child));
                break;
            }
            Err(e) => {
                last_err = format!("Failed to spawn shell (attempt {}): {}", attempt + 1, e);
                if attempt < max_retries - 1 {
                    tokio::time::sleep(std::time::Duration::from_millis(100 * (attempt as u64 + 1))).await;
                }
            }
        }
    }

    let (pair, child) = pair_and_child.ok_or(last_err)?;

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
            cwd: config.cwd,
        }),
    );
    state.metrics.total_spawned.fetch_add(1, Ordering::Relaxed);
    state.metrics.active_sessions.fetch_add(1, Ordering::Relaxed);

    // Create ring buffer for this session
    state.output_buffers.insert(
        session_id.clone(),
        Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)),
    );

    spawn_reader_thread(
        reader,
        paused,
        session_id.clone(),
        app,
        state.inner().clone(),
    );

    Ok(session_id)
}

/// Create a PTY session with a dedicated git worktree
#[tauri::command]
pub(crate) async fn create_pty_with_worktree(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    pty_config: PtyConfig,
    worktree_config: WorktreeConfig,
) -> Result<WorktreeResult, String> {
    // Create the worktree first
    let worktree = create_worktree_internal(&state.worktrees_dir, &worktree_config)?;
    let worktree_path = worktree.path.clone();

    // Wrap PTY creation so we can clean up the worktree on failure
    let pty_result = (|| -> Result<_, String> {
        let session_id = Uuid::new_v4().to_string();
        let pty_system = native_pty_system();

        // Guard against invalid dimensions from zero-sized windows
        let rows = pty_config.rows.max(24);
        let cols = pty_config.cols.max(80);

        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Failed to open PTY: {e}"))?;

        let shell = resolve_shell(pty_config.shell);

        let mut cmd = build_shell_command(&shell);
        cmd.cwd(&worktree_path);

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("Failed to spawn shell: {e}"))?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("Failed to get PTY writer: {e}"))?;

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("Failed to get PTY reader: {e}"))?;

        Ok((session_id, pair.master, child, writer, reader))
    })();

    let (session_id, master, child, writer, reader) = match pty_result {
        Ok(result) => result,
        Err(e) => {
            // Clean up the worktree since PTY creation failed
            if let Err(cleanup_err) = remove_worktree_internal(&worktree) {
                eprintln!("Warning: Failed to cleanup worktree after PTY failure: {cleanup_err}");
            }
            return Err(e);
        }
    };

    let branch = worktree.branch.clone();
    let worktree_cwd = Some(worktree.path.to_string_lossy().to_string());

    // Store session with worktree info (master handle kept for resize support)
    let paused = Arc::new(AtomicBool::new(false));
    state.sessions.insert(
        session_id.clone(),
        Mutex::new(PtySession {
            writer,
            master,
            _child: child,
            paused: paused.clone(),
            worktree: Some(worktree),
            cwd: worktree_cwd,
        }),
    );
    state.metrics.total_spawned.fetch_add(1, Ordering::Relaxed);
    state.metrics.active_sessions.fetch_add(1, Ordering::Relaxed);

    // Create ring buffer for this session
    state.output_buffers.insert(
        session_id.clone(),
        Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)),
    );

    spawn_reader_thread(
        reader,
        paused,
        session_id.clone(),
        app,
        state.inner().clone(),
    );

    Ok(WorktreeResult {
        session_id,
        worktree_path: worktree_path.to_string_lossy().to_string(),
        branch,
    })
}

/// List all active worktrees
#[tauri::command]
pub(crate) fn list_worktrees(state: State<'_, Arc<AppState>>) -> Vec<serde_json::Value> {
    state.sessions
        .iter()
        .filter_map(|entry| {
            let session = entry.value().lock();
            session.worktree.as_ref().map(|wt| {
                serde_json::json!({
                    "session_id": entry.key(),
                    "name": wt.name,
                    "path": wt.path.to_string_lossy(),
                    "branch": wt.branch,
                    "base_repo": wt.base_repo.to_string_lossy(),
                })
            })
        })
        .collect()
}

/// Write data to a PTY session
#[tauri::command]
pub(crate) fn write_pty(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    if let Some(entry) = state.sessions.get(&session_id) {
        let mut session = entry.lock();
        session
            .writer
            .write_all(data.as_bytes())
            .map_err(|e| format!("Failed to write to PTY: {e}"))?;
        session
            .writer
            .flush()
            .map_err(|e| format!("Failed to flush PTY: {e}"))?;
        Ok(())
    } else {
        Err("Session not found".to_string())
    }
}

/// Resize a PTY session
#[tauri::command]
pub(crate) fn resize_pty(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    if rows == 0 || cols == 0 {
        return Err("Invalid dimensions: rows and cols must be > 0".to_string());
    }
    let entry = state.sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))?;
    let session = entry.lock();
    session
        .master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Failed to resize PTY: {e}"))
}

/// Pause PTY reader thread (flow control: frontend buffer full)
#[tauri::command]
pub(crate) fn pause_pty(state: State<'_, Arc<AppState>>, session_id: String) -> Result<(), String> {
    let entry = state.sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))?;
    entry.lock().paused.store(true, Ordering::Relaxed);
    state.metrics.pauses_triggered.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// Resume PTY reader thread (flow control: frontend buffer drained)
#[tauri::command]
pub(crate) fn resume_pty(state: State<'_, Arc<AppState>>, session_id: String) -> Result<(), String> {
    let entry = state.sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))?;
    entry.lock().paused.store(false, Ordering::Relaxed);
    Ok(())
}

/// Close a PTY session with graceful shutdown and optional worktree cleanup.
/// Sends Ctrl-C (0x03) and waits briefly for the process to exit cleanly
/// before forcibly dropping handles.
#[tauri::command]
pub(crate) fn close_pty(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    cleanup_worktree: bool,
) -> Result<(), String> {
    if let Some((_, session_mutex)) = state.sessions.remove(&session_id) {
        state.output_buffers.remove(&session_id);
        state.metrics.active_sessions.fetch_sub(1, Ordering::Relaxed);
        let mut session = session_mutex.into_inner();

        // Send Ctrl-C (0x03) to give the process a chance to clean up
        let _ = session.writer.write_all(&[0x03]);
        let _ = session.writer.flush();

        // Wait up to 100ms for process to exit gracefully
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
        loop {
            match session._child.try_wait() {
                Ok(Some(_)) => break, // Process exited cleanly
                Ok(None) if std::time::Instant::now() >= deadline => break,
                _ => std::thread::sleep(std::time::Duration::from_millis(10)),
            }
        }

        // Extract worktree info before dropping session
        let worktree_to_cleanup = if cleanup_worktree {
            session.worktree.clone()
        } else {
            None
        };

        // Drop session to release file handles (forcibly kills if still running)
        drop(session);

        // Cleanup worktree if requested
        if let Some(worktree) = worktree_to_cleanup
            && let Err(e) = remove_worktree_internal(&worktree) {
                eprintln!("Warning: Failed to cleanup worktree: {e}");
            }
    }

    Ok(())
}

/// Look up the process name for a given PID using `ps`.
/// Returns None on Windows or if the lookup fails.
#[cfg(not(windows))]
pub(crate) fn process_name_from_pid(pid: u32) -> Option<String> {
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if name.is_empty() {
        return None;
    }
    // ps may return full path; extract just the binary name
    name.rsplit('/').next().map(|s| s.to_string())
}

#[cfg(windows)]
pub(crate) fn process_name_from_pid(pid: u32) -> Option<String> {
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32First, Process32Next,
        PROCESSENTRY32, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::Foundation::CloseHandle;

    // SAFETY: CreateToolhelp32Snapshot/Process32First/Process32Next are Windows API
    // functions that operate on a process snapshot handle. We zero-initialize the
    // PROCESSENTRY32 struct and set dwSize before use (required by the API). The
    // snapshot handle is closed via CloseHandle before returning. All pointer
    // arguments point to stack-local owned memory with valid lifetimes.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            return None;
        }

        let mut entry: PROCESSENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;

        let mut found = None;
        if Process32First(snapshot, &mut entry) != 0 {
            loop {
                if entry.th32ProcessID == pid {
                    // szExeFile is a [i8; 260] (MAX_PATH) null-terminated C string
                    let name_bytes: Vec<u8> = entry
                        .szExeFile
                        .iter()
                        .take_while(|&&b| b != 0)
                        .map(|&b| b as u8)
                        .collect();
                    // Use from_utf8_lossy to handle non-ASCII process names
                    // (e.g. apps with accented characters) instead of silently dropping them
                    let name = String::from_utf8_lossy(&name_bytes);
                    // Strip .exe suffix for consistent matching with classify_agent
                    let name = name.strip_suffix(".exe").unwrap_or(&name).to_string();
                    found = Some(name);
                    break;
                }
                if Process32Next(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }

        CloseHandle(snapshot);
        found
    }
}

/// Walk the process tree from `root_pid` and return the deepest descendant PID.
/// On Windows, this finds the "foreground" process in a PTY session by following
/// the chain: shell → agent CLI (e.g. claude.exe).
#[cfg(windows)]
fn deepest_descendant_pid(root_pid: u32) -> Option<u32> {
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32First, Process32Next,
        PROCESSENTRY32, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::Foundation::CloseHandle;

    // SAFETY: Same API contract as process_name_from_pid above. We take a full
    // process snapshot, iterate it to collect (pid, parent_pid) pairs into owned
    // Vecs, then close the handle. The PROCESSENTRY32 struct is zero-initialized
    // with dwSize set before the first call, satisfying the API precondition.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            return None;
        }

        // Collect all (pid, parent_pid) pairs and build parent->children map
        let mut children_map: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
        let mut entry: PROCESSENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;

        if Process32First(snapshot, &mut entry) != 0 {
            loop {
                children_map
                    .entry(entry.th32ParentProcessID)
                    .or_default()
                    .push(entry.th32ProcessID);
                if Process32Next(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);

        // Walk from root_pid to the deepest single child — O(depth) via HashMap
        let mut current = root_pid;
        loop {
            match children_map.get(&current).map(Vec::as_slice) {
                Some([only_child]) => current = *only_child,
                _ => break, // 0 children (leaf) or multiple children (ambiguous)
            }
        }

        Some(current)
    }
}

/// Map a process name to a known agent type, or None for non-agent processes.
pub(crate) fn classify_agent(process_name: &str) -> Option<&'static str> {
    match process_name {
        "claude" => Some("claude"),
        "gemini" => Some("gemini"),
        "opencode" => Some("opencode"),
        "aider" => Some("aider"),
        "codex" => Some("codex"),
        _ => None,
    }
}

/// Get the foreground process of a PTY session and classify it as a known agent.
/// Returns the agent name (e.g. "claude") or None if the foreground process is
/// not a recognized agent or the session doesn't exist.
#[tauri::command]
pub(crate) fn get_session_foreground_process(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Option<String> {
    let entry = state.sessions.get(&session_id)?;
    let session = entry.value().lock();
    #[cfg(not(windows))]
    {
        let pgid = session.master.process_group_leader()?;
        let name = process_name_from_pid(pgid as u32)?;
        classify_agent(&name).map(|s| s.to_string())
    }
    #[cfg(windows)]
    {
        // On Windows, walk the process tree from the shell child to find the
        // deepest descendant — the equivalent of the "foreground process".
        let child_pid = session._child.process_id()?;
        let leaf = deepest_descendant_pid(child_pid)?;
        let name = process_name_from_pid(leaf)?;
        classify_agent(&name).map(|s| s.to_string())
    }
}

/// Get orchestrator stats
#[tauri::command]
pub(crate) fn get_orchestrator_stats(state: State<'_, Arc<AppState>>) -> OrchestratorStats {
    state.orchestrator_stats()
}

/// Get PTY session metrics for observability
#[tauri::command]
pub(crate) fn get_session_metrics(state: State<'_, Arc<AppState>>) -> serde_json::Value {
    state.session_metrics_json()
}

/// Check if we can spawn a new session
#[tauri::command]
pub(crate) fn can_spawn_session(state: State<'_, Arc<AppState>>) -> bool {
    state.sessions.len() < MAX_CONCURRENT_SESSIONS
}

/// Info about an active PTY session for frontend reconnection
#[derive(Clone, Serialize)]
pub(crate) struct ActiveSessionInfo {
    session_id: String,
    cwd: Option<String>,
    worktree_path: Option<String>,
    worktree_branch: Option<String>,
}

/// List all active PTY sessions for reconnection after frontend reload
#[tauri::command]
pub(crate) fn list_active_sessions(state: State<'_, Arc<AppState>>) -> Vec<ActiveSessionInfo> {
    state
        .sessions
        .iter()
        .map(|entry| {
            let session_id = entry.key().clone();
            let session = entry.value().lock();
            ActiveSessionInfo {
                session_id,
                cwd: session.cwd.clone(),
                worktree_path: session
                    .worktree
                    .as_ref()
                    .map(|w| w.path.to_string_lossy().to_string()),
                worktree_branch: session.worktree.as_ref().and_then(|w| w.branch.clone()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_agent_claude() {
        assert_eq!(classify_agent("claude"), Some("claude"));
    }

    #[test]
    fn test_classify_agent_gemini() {
        assert_eq!(classify_agent("gemini"), Some("gemini"));
    }

    #[test]
    fn test_classify_agent_aider() {
        assert_eq!(classify_agent("aider"), Some("aider"));
    }

    #[test]
    fn test_classify_agent_codex() {
        assert_eq!(classify_agent("codex"), Some("codex"));
    }

    #[test]
    fn test_classify_agent_opencode() {
        assert_eq!(classify_agent("opencode"), Some("opencode"));
    }

    #[test]
    fn test_classify_agent_unknown_returns_none() {
        assert_eq!(classify_agent("bash"), None);
        assert_eq!(classify_agent("zsh"), None);
        assert_eq!(classify_agent("node"), None);
        assert_eq!(classify_agent("python"), None);
        assert_eq!(classify_agent("vim"), None);
    }

    // --- SilenceState tests ---

    #[test]
    fn test_silence_state_no_pending_returns_none() {
        let mut s = SilenceState::new();
        assert!(s.check_silence().is_none());
    }

    #[test]
    fn test_silence_state_pending_but_too_soon() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Continue?".to_string()));
        // Just set — not enough time has passed
        assert!(s.check_silence().is_none());
    }

    #[test]
    fn test_silence_state_pending_after_threshold() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Continue?".to_string()));
        // Simulate time passing by backdating last_output_at
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), Some("Continue?".to_string()));
    }

    #[test]
    fn test_silence_state_no_double_emission() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Continue?".to_string()));
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert!(s.check_silence().is_some());
        // Second check should return None (already emitted)
        assert!(s.check_silence().is_none());
    }

    #[test]
    fn test_silence_state_regex_suppresses_timer() {
        let mut s = SilenceState::new();
        // regex_found_question = true means instant detection already fired
        s.on_chunk(true, Some("Would you like to proceed?".to_string()));
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert!(s.check_silence().is_none());
    }

    #[test]
    fn test_silence_state_new_output_clears_pending() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Continue?".to_string()));
        // Agent prints more non-question output — it moved past the question
        s.on_chunk(false, None);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert!(s.check_silence().is_none());
    }

    #[test]
    fn test_silence_state_new_question_replaces_old() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("First question?".to_string()));
        s.on_chunk(false, Some("Second question?".to_string()));
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), Some("Second question?".to_string()));
    }
}
