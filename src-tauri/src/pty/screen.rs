use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AgentScreenActivity {
    Working,
    Ready,
    Interrupted,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProcessTreeEntry {
    pub(super) pid: u32,
    pub(super) parent_pid: u32,
    pub(super) name: String,
    pub(super) command: String,
    /// Seconds this process has been alive, when the platform snapshot reports
    /// it. `None` on Windows, whose `PROCESSENTRY32` carries no creation time —
    /// see [`started_with_agent`] for what the absence costs.
    pub(super) age_seconds: Option<u64>,
}

#[derive(Default)]
struct ProcessSnapshotState {
    generation: u64,
    pub(super) current: Option<Arc<Vec<ProcessTreeEntry>>>,
}

#[derive(Default)]
pub(crate) struct ProcessSnapshotCache {
    state: parking_lot::RwLock<ProcessSnapshotState>,
}

impl ProcessSnapshotCache {
    pub(super) fn store(&self, snapshot: Option<Vec<ProcessTreeEntry>>) {
        let mut state = self.state.write();
        state.generation = state.generation.wrapping_add(1);
        state.current = snapshot.map(Arc::new);
    }

    pub(super) fn load(&self) -> Option<(u64, Arc<Vec<ProcessTreeEntry>>)> {
        let state = self.state.read();
        Some((state.generation, Arc::clone(state.current.as_ref()?)))
    }

    fn generation(&self) -> u64 {
        self.state.read().generation
    }
}

fn normalized_process_name(value: &str) -> &str {
    value
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(value)
        .trim_end_matches(".exe")
}

/// Apply the same basename/path convention used by `process_name_from_pid`.
/// Claude's installer notably uses a version number as the executable basename,
/// so the containing `claude/versions/` path is authoritative.
pub(super) fn classify_agent_name_or_path(value: &str) -> Option<&'static str> {
    let normalized = value.trim_end_matches(" (deleted)").to_ascii_lowercase();
    let basename = normalized_process_name(&normalized);
    classify_agent(basename).or_else(|| {
        // Claude installs version-number executables under this exact layout.
        // Arbitrary agent-named ancestors do not identify the running program.
        let parts: Vec<_> = normalized.split(['/', '\\']).collect();
        let tail = parts.as_slice();
        if tail.len() >= 3
            && tail[tail.len() - 3] == "claude"
            && tail[tail.len() - 2] == "versions"
            && basename.starts_with(|c: char| c.is_ascii_digit())
            && basename.chars().all(|c| c.is_ascii_digit() || c == '.')
        {
            Some("claude")
        } else {
            None
        }
    })
}

fn is_persistent_agent_helper(process: &ProcessTreeEntry) -> bool {
    is_persistent_agent_helper_with_command_line(process, cfg!(not(windows)))
}

pub(super) fn is_standalone_timed_caffeinate(command: &str) -> bool {
    let mut argv = command.split_whitespace();
    let executable = argv.next().map(normalized_process_name).unwrap_or("");
    if executable != "caffeinate" {
        return false;
    }
    let first = argv.next();
    let second = argv.next();
    let third = argv.next();
    if argv.next().is_some() {
        return false;
    }
    let positive_timeout = |value: &str| value.parse::<u64>().is_ok_and(|seconds| seconds > 0);
    matches!((first, second, third), (Some("-i"), Some("-t"), Some(value)) if positive_timeout(value))
        || matches!((first, second, third), (Some("-t"), Some(value), Some("-i")) if positive_timeout(value))
}

pub(super) fn is_persistent_agent_helper_with_command_line(
    process: &ProcessTreeEntry,
    command_line_authoritative: bool,
) -> bool {
    let name = process.name.to_ascii_lowercase();
    let name = normalized_process_name(&name);
    let command = process.command.to_ascii_lowercase();
    let mut argv = command.split_whitespace();
    let executable = argv.next().map(normalized_process_name).unwrap_or("");
    let script = argv.next().map(normalized_process_name).unwrap_or("");
    matches!(name, "mdkb" | "tuic-bridge" | "node_repl")
        || (command_line_authoritative
            && (matches!(executable, "mdkb" | "tuic-bridge" | "node_repl")
                || (matches!(executable, "node" | "nodejs")
                    && script.trim_end_matches(".js") == "node_repl")
                || is_standalone_timed_caffeinate(&command)))
}

/// How long after the agent's own start a descendant may appear and still count
/// as session plumbing rather than work.
///
/// Measured against a live 14-session instance: every integration daemon came up
/// within 18s of its agent (`codex-code-mode-host` 12–18s, MCP servers 0–10s),
/// while work spawned by a turn was hundreds to thousands of seconds younger
/// than its agent. 60s sits in that gap with room for a cold MCP start.
pub(super) const AGENT_STARTUP_WINDOW_SECS: u64 = 60;

/// Whether `descendant` came up alongside the agent instead of being spawned by
/// a turn.
///
/// [`is_persistent_agent_helper`] answers the same question by name, and a name
/// list cannot keep up: `codex-code-mode-host` arrived with Codex 0.149.0, and
/// an MCP server started through `npm exec` reports as `npm` — a name that must
/// stay meaningful because a turn also runs npm. Both pinned every session on
/// this machine to `working` forever, because `background_work` outranks both
/// `completion_declared` and an idle shell in the agent-state ladder.
///
/// Age is the property that actually separates the two, and it needs no
/// per-tool knowledge. When either age is missing this returns false, leaving
/// the name list as the sole rule — which is exactly the Windows behaviour, and
/// errs toward reporting work rather than hiding it.
// DEFERRED (2026-08-23) — a daemon that dies and respawns mid-session escapes
// this window and is then counted as work for the rest of the session. Measured
// once over the live 14-session instance: 1 session, whose
// `codex-code-mode-host` had restarted 2494s after its agent. The remaining 13
// were classified correctly, against 14 wrong before the window existed. Fixing
// it needs per-session memory of pids already judged plumbing, which is state
// this pure function does not have — do not reach for a wider window instead,
// that is the same name-list mistake measured in seconds.
//
// DEFERRED (2026-08-25) — the mirror blind spot: real work spawned inside the
// agent's own first 60s is classified as plumbing, and the difference of two
// ages is constant, so the misclassification lasts that process's whole life.
// It bites a fast first turn that declares completion while a build it started
// keeps running — the session then reads idle. Do NOT "fix" it by skipping the
// window while the agent is young: every integration daemon comes up in the
// first 18s, so that trades this narrow false-idle for a guaranteed
// false-working minute on every session ever opened. Same per-session pid
// memory as above is the real fix.
fn started_with_agent(descendant: &ProcessTreeEntry, agent_age_seconds: Option<u64>) -> bool {
    let (Some(agent_age), Some(descendant_age)) = (agent_age_seconds, descendant.age_seconds)
    else {
        return false;
    };
    agent_age.saturating_sub(descendant_age) <= AGENT_STARTUP_WINDOW_SECS
}

pub(super) fn agent_process_root(
    session_root: u32,
    agent_type: &str,
    processes: &[ProcessTreeEntry],
) -> Option<u32> {
    let mut children = std::collections::HashMap::<u32, Vec<&ProcessTreeEntry>>::new();
    let mut by_pid = std::collections::HashMap::<u32, &ProcessTreeEntry>::new();
    for process in processes {
        children
            .entry(process.parent_pid)
            .or_default()
            .push(process);
        by_pid.insert(process.pid, process);
    }
    by_pid.get(&session_root)?;
    let mut queue = std::collections::VecDeque::from([session_root]);
    while let Some(pid) = queue.pop_front() {
        if let Some(process) = by_pid.get(&pid) {
            let executable_arg = process.command.split_whitespace().next().unwrap_or("");
            if classify_agent_name_or_path(&process.name) == Some(agent_type)
                || classify_agent_name_or_path(executable_arg) == Some(agent_type)
            {
                return Some(pid);
            }
        }
        if let Some(descendants) = children.get(&pid) {
            queue.extend(descendants.iter().map(|process| process.pid));
        }
    }
    // A configured custom alias may have no classifiable executable path. The
    // process-group leader is then the established foreground-process fallback;
    // descendants, rather than the alias process itself, represent background work.
    Some(session_root)
}

/// Return whether `root_pid` owns at least one meaningful live descendant.
/// Helper roots and their entire subtrees are ignored: integration daemons are
/// session plumbing, not evidence that the agent still owns autonomous work.
/// A daemon is recognised either by name ([`is_persistent_agent_helper`]) or by
/// having started with the agent ([`started_with_agent`]).
pub(super) fn has_meaningful_descendant(root_pid: u32, processes: &[ProcessTreeEntry]) -> bool {
    let mut children = std::collections::HashMap::<u32, Vec<&ProcessTreeEntry>>::new();
    let mut agent_age = None;
    for process in processes {
        children
            .entry(process.parent_pid)
            .or_default()
            .push(process);
        if process.pid == root_pid {
            agent_age = process.age_seconds;
        }
    }
    let mut stack = vec![root_pid];
    while let Some(parent) = stack.pop() {
        let Some(descendants) = children.get(&parent) else {
            continue;
        };
        for descendant in descendants {
            if is_persistent_agent_helper(descendant) || started_with_agent(descendant, agent_age) {
                continue;
            }
            return true;
        }
    }
    false
}

pub(super) fn background_work_from_snapshot(
    session_root: u32,
    agent_type: &str,
    processes: &[ProcessTreeEntry],
) -> Option<bool> {
    let agent_root = agent_process_root(session_root, agent_type, processes)?;
    Some(has_meaningful_descendant(agent_root, processes))
}

/// Interactive shells, and the privilege wrappers that exist only to start one.
/// `login` and `doas` are here for the same reason as `sudo`/`su`: on their own
/// they are not work, they are the two hops between the outer prompt and the
/// inner one.
const PROMPT_SHELL_NAMES: &[&str] = &[
    "sh", "bash", "zsh", "fish", "dash", "ksh", "mksh", "csh", "tcsh", "ash",
];
const PROMPT_SHELL_WRAPPERS: &[&str] = &["sudo", "su", "doas", "login"];

/// Whether this process is a shell sitting at a prompt rather than running a
/// script. A login shell reports as `-zsh`, so the leading dash is stripped;
/// `-c` means the shell was handed a command and is therefore work.
pub(super) fn is_prompt_shell_process(process: &ProcessTreeEntry) -> bool {
    let name = process.name.to_ascii_lowercase();
    let name = normalized_process_name(&name)
        .trim_start_matches('-')
        .to_string();
    if PROMPT_SHELL_WRAPPERS.contains(&name.as_str()) {
        return true;
    }
    if !PROMPT_SHELL_NAMES.contains(&name.as_str()) {
        return false;
    }
    !process
        .command
        .split_whitespace()
        .skip(1)
        .any(|argument| argument == "-c")
}

/// Whether the PTY's foreground process group is nothing but shells.
///
/// OSC 133 marks a command busy once and clears it once, so an interactive
/// subshell (`sh`, `sudo su`, `bash -l`) latches the outer shell BUSY for its
/// entire life — the inner shell has no integration of its own and never emits
/// the closing marker. The user is looking at an idle prompt while the tab says
/// working, which is what this repairs.
///
/// The whole subtree must qualify, not just its root: `sudo dd …` is a wrapper
/// with real work underneath it, and `sudo` on macOS allocates its own PTY, so
/// the inner shell is only reachable through the parent chain.
pub(super) fn foreground_group_at_prompt(root_pid: u32, processes: &[ProcessTreeEntry]) -> bool {
    let mut children = std::collections::HashMap::<u32, Vec<&ProcessTreeEntry>>::new();
    let mut root = None;
    for process in processes {
        children
            .entry(process.parent_pid)
            .or_default()
            .push(process);
        if process.pid == root_pid {
            root = Some(process);
        }
    }
    let Some(root) = root else {
        return false;
    };
    let mut stack = vec![root];
    while let Some(process) = stack.pop() {
        if !is_prompt_shell_process(process) {
            return false;
        }
        if let Some(descendants) = children.get(&process.pid) {
            stack.extend(descendants.iter().copied());
        }
    }
    true
}

/// The pid whose process tree represents this session's foreground work.
fn session_foreground_pid(state: &AppState, session_id: &str) -> Option<u32> {
    let entry = state.session_maps.sessions.get(session_id)?;
    let session = entry.value().lock();
    #[cfg(not(windows))]
    {
        session.master.process_group_leader().map(|pid| pid as u32)
    }
    #[cfg(windows)]
    {
        session._child.process_id()
    }
}

/// Cheap precondition for the nested-prompt probe: a plain shell, currently
/// BUSY, silent long enough that no running command would still be quiet.
/// Shared by the probe itself and by the process-snapshot demand check, so the
/// snapshot is only enumerated while a session could actually use it.
pub(super) fn prompt_probe_applies(state: &AppState, session_id: &str) -> bool {
    if state
        .session_maps
        .session_states
        .get(session_id)
        .is_none_or(|session| session.agent_type.is_some())
    {
        return false;
    }
    if state
        .session_maps
        .shell_states
        .get(session_id)
        .is_none_or(|shell| shell.load(std::sync::atomic::Ordering::Acquire) != SHELL_BUSY)
    {
        return false;
    }
    let last_ms = state
        .session_maps
        .last_output_ms
        .get(session_id)
        .map(|ts| ts.load(std::sync::atomic::Ordering::Relaxed))
        .unwrap_or(0);
    last_ms != 0 && now_epoch_ms().saturating_sub(last_ms) >= SHELL_PROMPT_PROBE_SILENCE_MS
}

/// Whether an explicit OSC 133 busy marker should be overruled because the
/// session is parked at a nested shell prompt. Must be evaluated before the
/// SilenceState lock is taken: it locks the PtySession to read the foreground
/// process group.
pub(super) fn explicit_busy_is_a_nested_prompt(state: &AppState, session_id: &str) -> bool {
    if !prompt_probe_applies(state, session_id) {
        return false;
    }
    let Some((_, processes)) = state.process_snapshot_cache.load() else {
        return false;
    };
    let Some(root_pid) = session_foreground_pid(state, session_id) else {
        return false;
    };
    foreground_group_at_prompt(root_pid, &processes)
}

#[cfg(not(windows))]
pub(super) fn process_tree_snapshot() -> Option<Vec<ProcessTreeEntry>> {
    let output = std::process::Command::new("ps")
        .args(["-ww", "-axo", "pid=,ppid=,etime=,comm=,args="])
        .output()
        .ok()?;
    parse_process_tree_snapshot(
        output.status.success(),
        &String::from_utf8_lossy(&output.stdout),
    )
}

/// Fresh process inventory for the idle-close guard. Detached `tuic bg`
/// runners are outside the agent's PTY process tree.
pub(crate) fn live_bg_runner_commands() -> Option<Vec<String>> {
    process_tree_snapshot().map(|processes| {
        processes
            .into_iter()
            .map(|process| process.command)
            .collect()
    })
}

#[cfg(not(windows))]
pub(super) fn parse_process_tree_snapshot(
    success: bool,
    text: &str,
) -> Option<Vec<ProcessTreeEntry>> {
    if !success {
        return None;
    }
    let mut result = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let (pid, rest) = take_process_snapshot_field(line)?;
        let (parent_pid, rest) = take_process_snapshot_field(rest)?;
        let (elapsed, rest) = take_process_snapshot_field(rest)?;
        let (name, command) = take_process_snapshot_field(rest)?;
        result.push(ProcessTreeEntry {
            pid: pid.parse().ok()?,
            parent_pid: parent_pid.parse().ok()?,
            name: name.to_string(),
            command: command.trim_start().to_string(),
            age_seconds: parse_elapsed_time(elapsed),
        });
    }
    (!result.is_empty()).then_some(result)
}

/// Parse the POSIX `ps -o etime` field — `[[dd-]hh:]mm:ss` — into seconds.
///
/// Returns `None` for anything else so an unparsed field degrades to "age
/// unknown" rather than to a fabricated age. `ps` always emits at least
/// `mm:ss`, so a lone number is not a valid reading.
#[cfg(not(windows))]
pub(super) fn parse_elapsed_time(value: &str) -> Option<u64> {
    let (days, clock) = match value.split_once('-') {
        Some((days, clock)) => (days.parse::<u64>().ok()?, clock),
        None => (0, value),
    };
    let mut seconds: u64 = 0;
    let mut fields = 0;
    for field in clock.split(':') {
        seconds = seconds
            .checked_mul(60)?
            .checked_add(field.parse::<u64>().ok()?)?;
        fields += 1;
    }
    (2..=3).contains(&fields).then_some(days * 86400 + seconds)
}

#[cfg(not(windows))]
fn take_process_snapshot_field(value: &str) -> Option<(&str, &str)> {
    let value = value.trim_start();
    let end = value.find(char::is_whitespace).unwrap_or(value.len());
    (end > 0).then(|| (&value[..end], &value[end..]))
}

#[cfg(windows)]
pub(super) fn process_tree_snapshot() -> Option<Vec<ProcessTreeEntry>> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next, TH32CS_SNAPPROCESS,
    };

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            return None;
        }
        let mut result = Vec::new();
        let mut entry: PROCESSENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;
        if Process32First(snapshot, &mut entry) == 0 {
            CloseHandle(snapshot);
            return valid_process_snapshot(false, result);
        }
        loop {
            let name_bytes: Vec<u8> = entry
                .szExeFile
                .iter()
                .take_while(|&&byte| byte != 0)
                .map(|&byte| byte as u8)
                .collect();
            let name = String::from_utf8_lossy(&name_bytes).into_owned();
            result.push(ProcessTreeEntry {
                pid: entry.th32ProcessID,
                parent_pid: entry.th32ParentProcessID,
                command: String::new(),
                name,
                age_seconds: None,
            });
            if Process32Next(snapshot, &mut entry) == 0 {
                break;
            }
        }
        CloseHandle(snapshot);
        valid_process_snapshot(true, result)
    }
}

#[cfg(any(windows, test))]
pub(super) fn valid_process_snapshot(
    enumeration_succeeded: bool,
    processes: Vec<ProcessTreeEntry>,
) -> Option<Vec<ProcessTreeEntry>> {
    (enumeration_succeeded && !processes.is_empty()).then_some(processes)
}

pub(super) fn emit_suggest_event(
    state: &AppState,
    session_id: &str,
    turn_epoch: u64,
    items: Vec<String>,
) {
    let parsed = ParsedEvent::Suggest { items };
    if let Ok(mut json) = serde_json::to_value(&parsed) {
        if let Some(object) = json.as_object_mut() {
            object.insert("_turn_epoch".to_string(), turn_epoch.into());
        }
        #[cfg(feature = "desktop")]
        if let Some(app) = state.app_handle.read().as_ref() {
            let _ = app.emit(&format!("pty-parsed-{session_id}"), &json);
        }
        state.emit_pty_event(crate::state::AppEvent::PtyParsed {
            session_id: session_id.to_string(),
            parsed: json.into(),
        });
    }
}

pub(super) fn set_background_work_for_epoch(
    state: &AppState,
    session_id: &str,
    observed_turn_epoch: u64,
    snapshot_generation: u64,
    active: bool,
) -> bool {
    set_background_work_for_epoch_with_hook(
        state,
        session_id,
        observed_turn_epoch,
        snapshot_generation,
        active,
        || {},
    )
}

pub(super) fn set_background_work_for_epoch_with_hook<F: FnOnce()>(
    state: &AppState,
    session_id: &str,
    observed_turn_epoch: u64,
    snapshot_generation: u64,
    active: bool,
    after_lifecycle_snapshot: F,
) -> bool {
    let Some(silence) = state
        .session_maps
        .silence_states
        .get(session_id)
        .map(|entry| Arc::clone(entry.value()))
    else {
        return false;
    };
    after_lifecycle_snapshot();
    let mut silence_state = silence.lock();
    let still_owns_lifecycle = state
        .session_maps
        .silence_states
        .get(session_id)
        .is_some_and(|current| Arc::ptr_eq(current.value(), &silence));
    if !still_owns_lifecycle || !state.session_maps.shell_states.contains_key(session_id) {
        return false;
    }
    let Some(mut session) = state.session_maps.session_states.get_mut(session_id) else {
        return false;
    };
    if session.turn_epoch != observed_turn_epoch
        || snapshot_generation <= session.background_snapshot_generation
    {
        return false;
    }
    let reconciled_probe = if session.has_pending_background_probe() {
        let Some(boundary) = session.background_probe_after_generation else {
            return false;
        };
        if snapshot_generation <= boundary {
            return false;
        }
        session.background_probe_turn_epoch = None;
        session.background_probe_after_generation = None;
        session.background_probe_satisfied_turn_epoch = Some(observed_turn_epoch);
        true
    } else if !session.background_work {
        return false;
    } else {
        false
    };
    session.background_snapshot_generation = snapshot_generation;
    if session.background_work == active {
        if !reconciled_probe || active {
            return true;
        }
    } else {
        session.background_work = active;
    }
    drop(session);

    let mut parent_dispatch = None;
    let settled_idle = !active
        && state
            .session_maps
            .shell_states
            .get(session_id)
            .is_some_and(|shell| shell.load(Ordering::Acquire) == SHELL_IDLE);
    if settled_idle {
        let completion = silence_state.drain_pending_suggest_with_epoch();
        match completion {
            Some((turn_epoch, items)) if turn_epoch == observed_turn_epoch => {
                emit_suggest_event(state, session_id, turn_epoch, items);
                parent_dispatch = enqueue_state_change_to_parent(
                    state,
                    session_id,
                    serde_json::json!({
                        "type": "state_change",
                        "state": "completed",
                        "session_id": session_id,
                    }),
                );
            }
            Some((turn_epoch, _)) => {
                if silence_state.completion_turn_epoch == turn_epoch {
                    silence_state.completion_declared = false;
                    silence_state.completion_turn_epoch = 0;
                }
            }
            None => {
                parent_dispatch = enqueue_state_change_to_parent(
                    state,
                    session_id,
                    serde_json::json!({
                        "type": "state_change",
                        "state": "idle",
                        "session_id": session_id,
                    }),
                );
            }
        }
    }
    drop(silence_state);
    if let Some(dispatch) = parent_dispatch {
        dispatch_parent_lifecycle(state, dispatch);
    }
    if settled_idle {
        reevaluate_orchestrator_mail_wake(state, session_id);
    }
    true
}

/// What the foreground probe knows about the current turn.
///
/// It used to be a bare `bool` meaning "satisfied or not applicable", which
/// gated the idle transition while recording nothing (#771-4733). The gate is
/// only two of the three answers; the third is a real observation of the
/// process table and belongs in the evidence model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ForegroundProbe {
    /// A process snapshot taken after the probe was armed was reconciled
    /// inside this turn, and it found no meaningful live descendant under the
    /// agent's process root. The agent owns nothing that is still running —
    /// [`EvidenceRank::Process`] evidence, not a gate result.
    Quiet,
    /// The gate is open without a process observation: either the session has
    /// no agent process to probe at all, or the reconciled snapshot still
    /// shows work under the agent root. Neither held the transition before
    /// this change and neither holds it now.
    Open,
    /// No snapshot newer than the arming boundary has landed yet. One is now
    /// armed for this turn; hold the transition until it reconciles.
    Pending,
}

pub(super) fn foreground_probe(
    state: &AppState,
    session_id: &str,
    silence: &Arc<Mutex<SilenceState>>,
) -> ForegroundProbe {
    let still_owns_lifecycle = state
        .session_maps
        .silence_states
        .get(session_id)
        .is_some_and(|current| Arc::ptr_eq(current.value(), silence));
    if !still_owns_lifecycle || !state.session_maps.shell_states.contains_key(session_id) {
        return ForegroundProbe::Pending;
    }
    let Some(mut session) = state.session_maps.session_states.get_mut(session_id) else {
        return ForegroundProbe::Pending;
    };
    if session.agent_type.is_none() {
        return ForegroundProbe::Open;
    }
    let turn_epoch = session.turn_epoch;
    if session.background_probe_satisfied_turn_epoch == Some(turn_epoch) {
        // `background_work` is the verdict of the snapshot that satisfied the
        // probe (`set_background_work_for_epoch_with_hook`). Only its negative
        // is an observation worth ranking: a tree that still shows work says
        // nothing about *this* turn ending, and it never held the transition.
        return if session.background_work {
            ForegroundProbe::Open
        } else {
            ForegroundProbe::Quiet
        };
    }
    if !session.has_pending_background_probe() {
        session.background_probe_turn_epoch = Some(turn_epoch);
        session.background_probe_after_generation = Some(state.process_snapshot_cache.generation());
    }
    ForegroundProbe::Pending
}

/// Invalidate only the process-snapshot boundary for the current working
/// episode. The caller must hold this session's SilenceState lifecycle lock.
pub(super) fn invalidate_background_probe_boundary_locked(state: &AppState, session_id: &str) {
    let Some(mut session) = state.session_maps.session_states.get_mut(session_id) else {
        return;
    };
    session.background_probe_turn_epoch = None;
    session.background_probe_after_generation = None;
    session.background_probe_satisfied_turn_epoch = None;
}

pub(super) fn arm_explicit_idle_background_probe(
    state: &AppState,
    session_id: &str,
    turn_epoch: u64,
) {
    let Some(mut session) = state.session_maps.session_states.get_mut(session_id) else {
        return;
    };
    if session.agent_type.is_none() || session.turn_epoch != turn_epoch {
        return;
    }
    session.background_probe_turn_epoch = Some(turn_epoch);
    session.background_probe_after_generation = Some(state.process_snapshot_cache.generation());
    session.background_probe_satisfied_turn_epoch = None;
}

pub(super) fn refresh_background_work(state: &AppState, session_id: &str) {
    let agent_type = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|session| session.agent_type.clone());
    let observed_turn_epoch = state
        .session_maps
        .session_states
        .get(session_id)
        .map(|session| session.turn_epoch);
    let root_pid = session_foreground_pid(state, session_id);
    let (Some(root_pid), Some(agent_type), Some(observed_turn_epoch)) =
        (root_pid, agent_type, observed_turn_epoch)
    else {
        return;
    };
    refresh_background_work_from_cached_snapshot(
        state,
        session_id,
        root_pid,
        &agent_type,
        observed_turn_epoch,
        state.process_snapshot_cache.load(),
    );
}

pub(super) fn refresh_background_work_from_cached_snapshot(
    state: &AppState,
    session_id: &str,
    root_pid: u32,
    agent_type: &str,
    observed_turn_epoch: u64,
    cached: Option<(u64, Arc<Vec<ProcessTreeEntry>>)>,
) -> bool {
    let Some((generation, processes)) = cached else {
        return false;
    };
    let Some(active) = background_work_from_snapshot(root_pid, agent_type, &processes) else {
        return false;
    };
    set_background_work_for_epoch(state, session_id, observed_turn_epoch, generation, active)
}

pub(super) fn process_snapshot_is_demanded(state: &AppState) -> bool {
    state.session_maps.session_states.iter().any(|session| {
        (if session.agent_type.is_some() {
            session.has_pending_background_probe() || session.background_work
        } else {
            prompt_probe_applies(state, session.key())
        }) && state
            .session_maps
            .silence_states
            .contains_key(session.key())
            && state.session_maps.shell_states.contains_key(session.key())
    })
}

fn reconcile_process_snapshot_demand(state: &AppState) {
    let sessions: Vec<String> = state
        .session_maps
        .session_states
        .iter()
        .filter(|session| {
            session.agent_type.is_some()
                && (session.has_pending_background_probe() || session.background_work)
                && state
                    .session_maps
                    .silence_states
                    .contains_key(session.key())
                && state.session_maps.shell_states.contains_key(session.key())
        })
        .map(|session| session.key().clone())
        .collect();
    for session_id in sessions {
        refresh_background_work(state, &session_id);
    }
}

pub(super) fn refresh_process_snapshot_if_demanded<F>(state: &AppState, enumerate: F) -> bool
where
    F: FnOnce() -> Option<Vec<ProcessTreeEntry>>,
{
    if !process_snapshot_is_demanded(state) {
        return false;
    }
    state.process_snapshot_cache.store(enumerate());
    reconcile_process_snapshot_demand(state);
    true
}

/// Enumerate the OS process table at most once per lifecycle cadence on
/// Tokio's blocking pool while a probe or tracked child needs reconciliation.
/// Every demanding session reads the resulting app-wide cache.
pub(crate) fn spawn_process_snapshot_refresher(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let refresh_state = Arc::clone(&state);
            let _ = tokio::task::spawn_blocking(move || {
                refresh_process_snapshot_if_demanded(&refresh_state, process_tree_snapshot)
            })
            .await;
        }
    });
}

/// The live `›` composer row of a Codex screen.
pub(super) fn find_codex_prompt_row(rows: &[String]) -> Option<usize> {
    const CODEX_PROMPT_WINDOW: usize = 4;

    let index = find_live_prompt_row(rows, CODEX_PROMPT_WINDOW, |row| {
        let t = row.trim_start();
        matches!(t.chars().next(), Some('\u{203A}' | '\u{00BB}'))
            && !t.starts_with("\u{203A}\u{203A}")
    })?;
    let content_end = rows
        .iter()
        .rposition(|row| !row.trim().is_empty())
        .map_or(0, |last| last + 1);
    // The fourth row only holds the composer when Codex's layout follows it: a blank
    // row, then the footer. Output rows below a `›` mean it is a history row.
    if index + CODEX_PROMPT_WINDOW == content_end && !rows[index + 1].trim().is_empty() {
        return None;
    }
    Some(index)
}

/// Inspect Codex's live prompt neighborhood on the UNFILTERED screen.
///
/// `find_chrome_cutoff` cannot be used here: Codex separators delimit tool
/// output from summaries, not its prompt box. When a recent separator sits
/// above `• Working`, the generic cutoff intentionally trims the whole region
/// and used to hide the strongest activity signal from both reader and timer.
/// Restricting the match to a few rows immediately above the lowest `›` prompt
/// prevents a historical Working line elsewhere in the viewport from latching
/// the session busy.
pub(super) fn detect_codex_screen_activity(rows: &[String]) -> AgentScreenActivity {
    const PROMPT_NEIGHBORHOOD: usize = 6;

    let Some(prompt_idx) = find_codex_prompt_row(rows) else {
        return AgentScreenActivity::Unknown;
    };
    let start = prompt_idx.saturating_sub(PROMPT_NEIGHBORHOOD);
    let neighborhood = &rows[start..prompt_idx];

    if neighborhood
        .iter()
        .any(|row| crate::chrome::is_working_status_row(row))
    {
        return AgentScreenActivity::Working;
    }
    if neighborhood
        .iter()
        .any(|row| row.trim_start().starts_with("■ Conversation interrupted"))
    {
        return AgentScreenActivity::Interrupted;
    }
    AgentScreenActivity::Ready
}

/// Find a prompt only in the current bottom chrome zone.
///
/// The rendered viewport includes transcript history, so a whole-screen search
/// can mistake an old submitted prompt or markdown quote for the live composer.
/// Prefer the structurally detected input box (including tall custom HUDs); if
/// no box can be identified, accept only the final `window` non-padding rows: Codex 0.159
/// draws the composer, a blank row and a two-line footer below it, so Codex passes
/// a window of four; every other agent passes three.
fn find_live_prompt_row<F>(rows: &[String], window: usize, is_prompt: F) -> Option<usize>
where
    F: Fn(&str) -> bool,
{
    let content_end = rows
        .iter()
        .rposition(|row| !row.trim().is_empty())
        .map_or(0, |index| index + 1);
    if content_end == 0 {
        return None;
    }
    let refs: Vec<&str> = rows[..content_end].iter().map(String::as_str).collect();
    if let Some(prompt) = crate::chrome::find_input_box_prompt_row(&refs)
        && is_prompt(&rows[prompt])
    {
        return Some(prompt);
    }
    (content_end.saturating_sub(window)..content_end)
        .rev()
        .find(|&index| is_prompt(&rows[index]))
}

/// Claude's active status is presence-based because current Claude versions can
/// keep the empty composer visible while a long tool call is still running.
/// The live marker is deliberately semantic rather than glyph-only: an animated
/// spinner prefix plus an ellipsis in the phase name (`✽ Nucleating… (3m 50s)`)
/// means active, while completed summaries (`✻ Sautéed for 1m 25s`), HUD bars,
/// hints, and banner art remain inert. This also holds BUSY when DEC 2026 frame
/// coalescing makes consecutive spinner paints text-identical.
pub(super) fn detect_claude_screen_activity(rows: &[String]) -> AgentScreenActivity {
    let content_end = rows
        .iter()
        .rposition(|row| !row.trim().is_empty())
        .map_or(0, |idx| idx + 1);
    let chrome_start = content_end.saturating_sub(crate::chrome::CHROME_SCAN_ROWS);
    let prompt_idx = rows[chrome_start..content_end]
        .iter()
        .rposition(|row| row.trim() == "\u{276F}")
        .map(|idx| chrome_start + idx);
    let activity_end = prompt_idx.unwrap_or(content_end);
    let activity_start = activity_end.saturating_sub(crate::chrome::CHROME_SCAN_ROWS);
    if rows[activity_start..activity_end].iter().any(|row| {
        crate::chrome::is_spinner_row(row)
            && row.contains('\u{2026}')
            && row.contains('(')
            && row.contains(')')
    }) {
        return AgentScreenActivity::Working;
    }
    if prompt_idx.is_some() {
        AgentScreenActivity::Ready
    } else {
        AgentScreenActivity::Unknown
    }
}

fn gemini_prompt_present(rows: &[String]) -> bool {
    find_live_prompt_row(rows, 3, |row| {
        let t = row.trim_start();
        t == ">" || t.starts_with("> ")
    })
    .is_some()
}

/// Prompt-based only — see `detect_claude_screen_activity` for the rationale.
pub(super) fn detect_gemini_screen_activity(rows: &[String]) -> AgentScreenActivity {
    if gemini_prompt_present(rows) {
        AgentScreenActivity::Ready
    } else {
        AgentScreenActivity::Unknown
    }
}

/// Prompt-based only — see `detect_claude_screen_activity` for the rationale.
/// During generation Aider has no bottom input box (prompt_toolkit returned),
/// so the screen reads Unknown and BUSY is held by spinner movement + silence.
pub(super) fn detect_aider_screen_activity(rows: &[String]) -> AgentScreenActivity {
    if rows.iter().rev().take(3).any(|row| row.trim() == ">") {
        AgentScreenActivity::Ready
    } else {
        AgentScreenActivity::Unknown
    }
}

/// True for grok's composer row. Builds from 0.2.11x draw it inside a rounded box
/// (`│ ❯                    │`); earlier builds emitted a bare `❯ Ask anything`. Missing the
/// boxed form left the session stuck BUSY for the whole process, because Ready never fired.
fn is_grok_composer_row(row: &str) -> bool {
    let trimmed = row.trim_start();
    let inner = trimmed
        .strip_prefix('\u{2502}')
        .unwrap_or(trimmed)
        .trim_start();
    let mut chars = inner.chars();
    chars.next() == Some('\u{276F}') && chars.next().is_none_or(char::is_whitespace)
}

/// Grok keeps its composer visible while a turn is running, so the prompt
/// alone is not enough to declare the session ready. Its turn-status row is
/// structurally stronger: it starts with the animated braille spinner already
/// recognized by `is_spinner_row` and disappears when the turn completes.
fn detect_grok_screen_activity(rows: &[String]) -> AgentScreenActivity {
    let content_end = rows
        .iter()
        .rposition(|row| !row.trim().is_empty())
        .map_or(0, |idx| idx + 1);
    let chrome_start = content_end.saturating_sub(crate::chrome::CHROME_SCAN_ROWS);
    let footer = &rows[chrome_start..content_end];

    if footer.iter().any(|row| crate::chrome::is_spinner_row(row)) {
        return AgentScreenActivity::Working;
    }
    if footer.iter().any(|row| is_grok_composer_row(row)) {
        AgentScreenActivity::Ready
    } else {
        AgentScreenActivity::Unknown
    }
}

/// True for pi's bottom status row: `↑1.3k ↓1.8k … 3.4%/272k (auto)   (openai) gpt-5.6-sol • medium`.
/// The context-usage `N%/Nk` pair plus the ` • ` model separator is unique to that row and
/// present in every state, so it identifies a pi screen without asserting readiness.
pub(super) fn is_pi_status_row(row: &str) -> bool {
    let trimmed = row.trim();
    if !trimmed.contains(" \u{2022} ") {
        return false;
    }
    // `%/` only ever appears in the context gauge (`3.4%/272k`).
    let Some(pos) = trimmed.find("%/") else {
        return false;
    };
    trimmed[..pos]
        .chars()
        .next_back()
        .is_some_and(|c| c.is_ascii_digit())
}

/// pi keeps its composer, separators and status row on screen for the whole turn, and the
/// composer carries no prompt glyph (it is a bare reverse-video cursor block), so readiness
/// cannot be read from a prompt char. What does change is the composer row itself: while a
/// turn runs it is replaced by an animated ` ⠏ Working...` row that `is_spinner_row` already
/// recognises. Ready is therefore "this is a pi screen and nothing is spinning".
fn detect_pi_screen_activity(rows: &[String]) -> AgentScreenActivity {
    let content_end = rows
        .iter()
        .rposition(|row| !row.trim().is_empty())
        .map_or(0, |idx| idx + 1);
    let chrome_start = content_end.saturating_sub(crate::chrome::CHROME_SCAN_ROWS);
    let footer = &rows[chrome_start..content_end];

    if footer.iter().any(|row| crate::chrome::is_spinner_row(row)) {
        return AgentScreenActivity::Working;
    }
    if footer.iter().any(|row| is_pi_status_row(row)) {
        AgentScreenActivity::Ready
    } else {
        AgentScreenActivity::Unknown
    }
}

/// True for a row of OpenCode's composer frame: the heavy vertical `┃` (U+2503)
/// running down the left edge of the prompt box.
fn is_opencode_frame_row(row: &str) -> bool {
    row.trim_start().starts_with('\u{2503}')
}

/// True for the row that closes OpenCode's composer frame: `╹` (U+2579) followed by a
/// run of `▀` (U+2580). Present in every OpenCode state — welcome, mid-turn, finished.
fn is_opencode_frame_close_row(row: &str) -> bool {
    row.trim_start()
        .strip_prefix('\u{2579}')
        .is_some_and(|rest| rest.starts_with("\u{2580}\u{2580}\u{2580}\u{2580}"))
}

/// OpenCode is a full-screen Bubble Tea TUI, so neither of the two generic signals works:
/// it paints no prompt glyph (`❯`/`›`/`>`), and its activity indicator is a `⬝`/`■`
/// progress bar rather than anything `is_spinner_row` recognises. What IS stable across
/// every state is the composer frame — `┃` rows closed by a `╹▀▀▀…` run — with the status
/// bar painted underneath it. OpenCode only offers `esc interrupt` in that status bar while
/// a turn is running (verified live on v1.18.5 across the model phase AND a tool phase, at
/// both 120 and 62 columns), so Ready is "this is an OpenCode screen and nothing down there
/// is offering an interrupt".
///
/// Declaring Ready additionally requires the status bar's `ctrl+p commands` hint, which is
/// present in every state: without it a frame whose status bar has not been painted yet
/// would read Ready mid-turn — exactly the false idle that lets auto-standby SIGSTOP a live
/// session. The interrupt hint is checked first so a working screen is never downgraded.
#[cfg(test)]
pub(super) fn detect_opencode_screen_activity(rows: &[String]) -> AgentScreenActivity {
    detect_opencode_screen_activity_at(rows, None)
}

/// `columns` is the terminal width when the caller knows it; the `--mini` adapter
/// needs it to tell a status row from tool output on a screen too narrow to paint one.
fn detect_opencode_screen_activity_at(
    rows: &[String],
    columns: Option<usize>,
) -> AgentScreenActivity {
    const STATUS_BAR_HINT: &str = "ctrl+p commands";
    const INTERRUPT_HINT: &str = "esc interrupt";

    let Some(close_idx) = rows
        .iter()
        .rposition(|row| is_opencode_frame_close_row(row))
    else {
        return detect_opencode_mini_screen_activity(rows, columns);
    };
    if !rows[..close_idx]
        .iter()
        .any(|row| is_opencode_frame_row(row))
    {
        return AgentScreenActivity::Unknown;
    }
    let status_bar = &rows[close_idx + 1..];

    if status_bar.iter().any(|row| row.contains(INTERRUPT_HINT)) {
        return AgentScreenActivity::Working;
    }
    if status_bar.iter().any(|row| row.contains(STATUS_BAR_HINT)) {
        AgentScreenActivity::Ready
    } else {
        AgentScreenActivity::Unknown
    }
}

/// OpenCode's `--mini` interface, which `agent_hook_launch` adds to every launch,
/// has no composer frame. Its only fixed element is a status row at the bottom,
/// captured live on 1.18.30 (replayed from the fixtures, and re-observed in tmux at
/// 120, 64 and 40 columns):
///
/// ```text
/// fresh:    ` BUILD                                                       ctrl+p cmd`
/// ready:    ` BUILD                                 52.9K (26%) · ctrl+p cmd`
/// working:  ` BUILD  ⬝⬝⬝■■■■■ esc interrupt                      ctrl+p cmd`
/// narrow:   ` BUILD`   /   ` BUILD  ⬝⬝■■■■■■ esc interrupt`
/// ```
///
/// A false Ready feeds standby/SIGSTOP and types the queue into a live turn, so Ready
/// accepts exactly the observed shapes and nothing wider: an uppercase label followed
/// by `ctrl+p cmd`, by `<usage> (<n>%) · ctrl+p cmd`, or by nothing for the `BUILD` and
/// `PLAN` primary agents of the narrow layout. A progress bar glyph or `esc interrupt`
/// marks a running turn (the bar is painted before its text and the text is cut at
/// narrow widths). Every other last row, such as tool output opening with an uppercase
/// word and a number, is Unknown.
///
/// DEFERRED (2026-10-01) — forms never observed stay Unknown, which delays the queue
/// instead of typing into a live turn: a cost token (`$0.12`, a free local model prints
/// none), a lowercase `k`, a user-defined agent label shown bare at narrow width.
/// Widen only from a live capture of the form. Below 46 columns OpenCode paints no
/// status row at all, so screen evidence cannot release the queue there: it drains
/// only once the pane is widened.
fn detect_opencode_mini_screen_activity(
    rows: &[String],
    columns: Option<usize>,
) -> AgentScreenActivity {
    const INTERRUPT_HINT: &str = "esc interrupt";
    const BAR_GLYPHS: [char; 2] = ['\u{2B1D}', '\u{25A0}'];
    const BARE_LABELS: [&str; 2] = ["BUILD", "PLAN"];
    // Narrowest width at which OpenCode 1.18.30 paints the status row: ` BUILD` shows from
    // 46 columns up (verified at 46..63 in tmux) and is absent at 45 and below, so a
    // `BUILD` or `PLAN` line on a narrower screen is tool output.
    const MIN_STATUS_ROW_COLUMNS: usize = 46;

    let Some(status) = rows.iter().rev().find(|row| !row.trim().is_empty()) else {
        return AgentScreenActivity::Unknown;
    };
    // A running turn is recognised before the label gate: the hint and the bar are
    // distinctive, and an agent name with a dot or space must not read Unknown mid-turn.
    if status.contains(INTERRUPT_HINT) || status.contains(BAR_GLYPHS) {
        return AgentScreenActivity::Working;
    }
    if columns.is_some_and(|columns| columns < MIN_STATUS_ROW_COLUMNS) {
        return AgentScreenActivity::Unknown;
    }
    let mut tokens = status.split_whitespace();
    let Some(label) = tokens.next() else {
        return AgentScreenActivity::Unknown;
    };
    let is_label = label.chars().count() >= 2
        && label.starts_with(|c: char| c.is_ascii_uppercase())
        && label
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if !is_label {
        return AgentScreenActivity::Unknown;
    }
    // `52.9K`: digits with at most one dot and an optional K/M/B suffix.
    let is_usage = |token: &str| {
        let number = token.strip_suffix(['K', 'M', 'B']).unwrap_or(token);
        number.starts_with(|c: char| c.is_ascii_digit())
            && number.ends_with(|c: char| c.is_ascii_digit())
            && number.chars().all(|c| c.is_ascii_digit() || c == '.')
            && number.matches('.').count() <= 1
    };
    // `(26%)`
    let is_percent = |token: &str| {
        token
            .strip_prefix('(')
            .and_then(|inner| inner.strip_suffix("%)"))
            .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
    };
    let rest: Vec<&str> = tokens.collect();
    let is_status_row = match rest.as_slice() {
        [] => BARE_LABELS.contains(&label),
        ["ctrl+p", "cmd"] => true,
        [usage, percent, "\u{00B7}", "ctrl+p", "cmd"] => is_usage(usage) && is_percent(percent),
        _ => false,
    };
    if is_status_row {
        AgentScreenActivity::Ready
    } else {
        AgentScreenActivity::Unknown
    }
}

/// goose keeps a one-line composer footer at the bottom of the screen and swaps
/// it for a spinner row while a turn runs. Captured live on goose 1.49.0 at 120
/// columns (#699-c6e0), the two states are:
///
/// ```text
/// ready:    > Enter to send · Ctrl+J newline
/// working:  ◓  Merging memory matrices...  (Ctrl+C to interrupt)
/// ```
///
/// After Ctrl+C the composer placeholder changes, and `Enter to send` is gone
/// (#1301-87fd):
///
/// ```text
/// > Interrupted, what should goose work on instead?
/// ```
///
/// Neither generic signal works here. The spinner glyph cycles `◐◓◒`, which
/// `is_spinner_row` does not recognise, and the message beside it is whimsical
/// and changes between turns — "Merging memory matrices…" is one of a set, so
/// matching it would pin the adapter to a string goose is free to reword. What
/// does not move is the **hint** at each end: `Ctrl+C to interrupt` appears only
/// while a turn can be interrupted, and `Enter to send` (or the post-Ctrl+C
/// `Interrupted, what should goose work on instead?` placeholder) only when the
/// composer is accepting input.
///
/// The lowest hint row wins, so a working screen is never downgraded by a hint
/// above its spinner, and Ready demands the composer footer rather than merely the absence of a
/// spinner — a half-painted screen must read Unknown, not idle. A false Ready is
/// the expensive direction: it is what lets auto-standby SIGSTOP a live turn.
fn detect_goose_screen_activity(rows: &[String]) -> AgentScreenActivity {
    const INTERRUPT_HINT: &str = "Ctrl+C to interrupt";
    const COMPOSER_HINTS: [&str; 2] = [
        "Enter to send",
        "Interrupted, what should goose work on instead?",
    ];

    let content_end = rows
        .iter()
        .rposition(|row| !row.trim().is_empty())
        .map_or(0, |idx| idx + 1);
    let chrome_start = content_end.saturating_sub(crate::chrome::CHROME_SCAN_ROWS);
    let footer = &rows[chrome_start..content_end];

    // The lowest hint row decides: after Ctrl+C the spinner row stays on screen
    // above the new composer, so the interrupt hint alone is stale there.
    let lowest_hint = footer.iter().rev().find_map(|row| {
        if row.contains(INTERRUPT_HINT) {
            Some(AgentScreenActivity::Working)
        } else if COMPOSER_HINTS.iter().any(|hint| row.contains(hint)) {
            Some(AgentScreenActivity::Ready)
        } else {
            None
        }
    });
    lowest_hint.unwrap_or(AgentScreenActivity::Unknown)
}

/// #744-138c: call counter so a test can measure that the reader chunk path
/// and the silence timer no longer each classify the screen independently —
/// see `cached_screen_activity`. Not gated behind `#[cfg(test)]` on the
/// counter itself (the increment is one relaxed atomic add, negligible), only
/// the accessor used to read it is test-only.
static SCREEN_CLASSIFY_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(super) fn screen_classify_calls() -> usize {
    SCREEN_CLASSIFY_CALLS.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
pub(super) fn detect_agent_screen_activity(
    agent_type: Option<&str>,
    rows: &[String],
) -> AgentScreenActivity {
    detect_agent_screen_activity_at(agent_type, rows, None)
}

/// [`detect_agent_screen_activity`] with the terminal width, for the adapters whose
/// reading depends on it.
pub(super) fn detect_agent_screen_activity_at(
    agent_type: Option<&str>,
    rows: &[String],
    columns: Option<usize>,
) -> AgentScreenActivity {
    SCREEN_CLASSIFY_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    match agent_type {
        Some("claude") => detect_claude_screen_activity(rows),
        Some("codex") => detect_codex_screen_activity(rows),
        Some("gemini") => detect_gemini_screen_activity(rows),
        Some("aider") => detect_aider_screen_activity(rows),
        Some("grok") => detect_grok_screen_activity(rows),
        Some("pi") => detect_pi_screen_activity(rows),
        Some("opencode") => detect_opencode_screen_activity_at(rows, columns),
        Some("goose") => detect_goose_screen_activity(rows),
        _ => AgentScreenActivity::Unknown,
    }
}

/// Classify the existing screen signal for an MCP submission receipt.
/// Raw-output movement is checked by the caller first; `terminal_output` means
/// the child moved but its agent adapter has no stronger current-state label.
pub(crate) fn agent_submission_ack_kind(state: &AppState, session_id: &str) -> &'static str {
    let agent_type = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|session| session.agent_type.clone());
    let activity = state
        .grid
        .vt_log_buffers
        .get(session_id)
        .map(|vt| {
            let vt = vt.lock();
            detect_agent_screen_activity_at(
                agent_type.as_deref(),
                &vt.screen_rows(),
                Some(vt.grid_columns()),
            )
        })
        .unwrap_or(AgentScreenActivity::Unknown);
    match activity {
        AgentScreenActivity::Working => "working_screen",
        AgentScreenActivity::Ready => "ready_screen",
        AgentScreenActivity::Interrupted => "interrupted_screen",
        AgentScreenActivity::Unknown => "terminal_output",
    }
}

/// Agents listed here recover to idle from the screen. An agent that is MISSING here and
/// whose foreground command is long-lived stays busy for the whole process, because OSC 133
/// marks that command busy once and nothing else ever clears it (#523-1df4, #534-e30c,
/// #535-d4f5).
///
/// DEFERRED (2026-08-02, narrowed 2026-09-07) — amp, cursor and droid were audited for
/// the same failure while fixing opencode and could NOT be verified: none of those three
/// binaries is installed on this machine, and an adapter written from documentation
/// instead of a live capture is how grok first shipped green tests over a UI that stayed
/// stuck BUSY. They are tracked in `to-test.md`; give each one an adapter only after
/// capturing its real screens. **goose left this list on 2026-09-07** — it was installed,
/// captured live at 1.49.0, and now has `detect_goose_screen_activity`.
///
/// **The remaining three cannot be excused instead of adapted, and that is proved rather
/// than assumed.** An agent needs no entry here only if something else returns it to idle:
/// either a protocol signal, or a foreground command short-lived enough that OSC 133
/// clears on its own. Checked 2026-09-07, both routes are shut for all three:
/// `HOOK_SUPPORT` in `src/agents.ts` is `false` for amp, cursor and droid, so no explicit
/// Stop ever sets `idle_confirmed`; and each launches as a long-lived interactive process
/// (`amp "{prompt}"`, `cursor-agent`, `droid`), which is exactly the shape the paragraph
/// above describes as latching busy forever. `pi` is the contrast that proves the rule —
/// also `HOOK_SUPPORT: false`, and it is in the list precisely because a screen adapter is
/// the only thing that can idle it. So the three need captures from installed binaries; no
/// amount of static analysis substitutes for that.
pub(crate) fn has_ready_screen_adapter(agent_type: Option<&str>) -> bool {
    matches!(
        agent_type,
        Some("claude" | "codex" | "gemini" | "aider" | "grok" | "pi" | "opencode" | "goose")
    )
}

/// Script interpreters that execute an agent CLI in their own process image.
/// For these the executable path names the interpreter, not the tool the user
/// launched, so the real identity has to come from argv[0]. Windows exposes no
/// argv in its process snapshot, so only the macOS and Linux arms use this.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(super) fn is_script_interpreter(name: &str) -> bool {
    matches!(
        name,
        "node" | "bun" | "deno" | "python" | "python3" | "ruby" | "perl"
    )
}

/// Look up the process name for a given PID using OS-native syscalls.
/// On macOS uses `proc_pidpath`, on Linux resolves `/proc/{pid}/exe` with a comm fallback.
/// Returns None if the lookup fails.
#[cfg(target_os = "macos")]
pub(crate) fn process_name_from_pid(pid: u32) -> Option<String> {
    let mut buf = [0u8; libc::MAXPATHLEN as usize];
    // SAFETY: proc_pidpath writes into the provided buffer up to buffersize bytes.
    // The buffer is stack-allocated with known size. pid is a valid u32 cast to i32.
    let ret = unsafe { libc::proc_pidpath(pid as i32, buf.as_mut_ptr().cast(), buf.len() as u32) };
    if ret <= 0 {
        return None;
    }
    let path = std::str::from_utf8(&buf[..ret as usize]).ok()?;
    if let Some(agent_type) = classify_agent_name_or_path(path) {
        return Some(agent_type.to_string());
    }
    let basename = normalized_process_name(path);
    // An npm-installed agent CLI (pi ships as a node script) reports the node
    // binary here, so classify_agent would never see the tool's own name and the
    // session stayed an unclassified shell — no ready-screen adapter, stuck BUSY.
    // Only interpreters pay the extra syscall; every native binary returns above.
    if is_script_interpreter(basename)
        && let Some(argv0) = crate::process_env::read_process_argv0(pid)
    {
        if let Some(agent_type) = classify_agent_name_or_path(&argv0) {
            return Some(agent_type.to_string());
        }
        return Some(normalized_process_name(&argv0).to_string());
    }
    Some(basename.to_string())
}

#[cfg(target_os = "linux")]
pub(crate) fn process_name_from_pid(pid: u32) -> Option<String> {
    // Native Claude installs a numeric executable; comm alone cannot name it.
    // Keep comm/process-title discovery when readlink is denied or the path
    // describes an interpreter instead of the agent it is hosting.
    if let Ok(path) = std::fs::read_link(format!("/proc/{pid}/exe"))
        && let Some(agent) = classify_agent_name_or_path(&path.to_string_lossy())
    {
        return Some(agent.to_string());
    }
    let comm = std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())?;
    // See the macOS arm: an npm-installed agent CLI reports its interpreter here,
    // so fall back to argv[0] to recover the tool's own name.
    if is_script_interpreter(&comm)
        && let Some(argv0) = crate::process_env::read_process_argv0(pid)
    {
        return Some(normalized_process_name(&argv0).to_string());
    }
    Some(comm)
}

#[cfg(windows)]
pub(crate) fn process_name_from_pid(pid: u32) -> Option<String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next, TH32CS_SNAPPROCESS,
    };

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

static FOREGROUND_PROBE_GENERATION: AtomicU64 = AtomicU64::new(1);

/// Discover and record the foreground agent for desktop and headless consumers.
/// Identity alone never confirms readiness or bypasses the composer guards.
/// Returns the effective foreground detection, not the retained session identity:
/// a shell foreground returns None even while a startup preset remains armed.
pub(crate) fn refresh_session_agent(state: &AppState, session_id: &str) -> Option<String> {
    let (generation, detected, fg_is_shell, input_blocked, fg_name) = {
        let entry = state.session_maps.sessions.get(session_id)?;
        let session = entry.value().lock();
        let generation = FOREGROUND_PROBE_GENERATION.fetch_add(1, Ordering::Relaxed);
        let role = state
            .session_maps
            .session_states
            .get(session_id)
            .map(|s| s.spawn_root_role)
            .unwrap_or_default();
        let root = session._child.process_id();
        #[cfg(not(windows))]
        let foreground = session.master.process_group_leader().map(|pid| pid as u32);
        #[cfg(windows)]
        let foreground = root.and_then(deepest_descendant_pid);
        match (role, root, foreground) {
            (crate::state::SpawnRootRole::Shell, Some(root), Some(fg)) => {
                let at_root = fg == root;
                let name = process_name_from_pid(fg);
                // exec replaces the shell's image without changing its PID.
                // Root equality proves shell return only if it is not an agent.
                let detected = name.as_deref().and_then(classify_agent).map(str::to_string);
                let fg_is_shell = at_root && name.is_some() && detected.is_none();
                // DEFERRED (2026-10-03) — retain identity but hold input on a
                // lookup error until the next refresh. A per-PID name cache
                // needs exec-aware invalidation: an unchanged PID can now own
                // a different image, so blindly retaining readiness is unsafe.
                (
                    generation,
                    detected,
                    fg_is_shell,
                    name.is_none(),
                    name.unwrap_or_else(|| "unavailable".into()),
                )
            }
            (crate::state::SpawnRootRole::DirectProgram, Some(root), Some(fg)) => {
                let name = process_name_from_pid(root);
                let detected = name.as_deref().and_then(classify_agent).map(str::to_string);
                (
                    generation,
                    detected,
                    false,
                    fg != root || name.is_none(),
                    name.unwrap_or_else(|| "unavailable".into()),
                )
            }
            _ => {
                tracing::warn!(
                    session_id,
                    "Foreground root role or PID unavailable; unattended agent input is held"
                );
                (generation, None, false, true, "unavailable".into())
            }
        }
    };

    apply_foreground_agent_observation(
        state,
        session_id,
        generation,
        detected,
        fg_is_shell,
        input_blocked,
        fg_name,
    )
}

/// Apply only observations newer than the last committed OS snapshot. Keeping
/// sampling and application separate makes the cross-caller ordering explicit.
pub(super) fn apply_foreground_agent_observation(
    state: &AppState,
    session_id: &str,
    generation: u64,
    detected: Option<String>,
    fg_is_shell: bool,
    input_blocked: bool,
    fg_name: String,
) -> Option<String> {
    let (effective, identity_changed) = {
        let mut entry = state.session_maps.session_states.get_mut(session_id)?;
        if generation <= entry.foreground_probe_generation {
            return entry.foreground_probe_result.clone();
        }
        entry.foreground_probe_generation = generation;
        entry.foreground_input_blocked = input_blocked;
        // Non-shell helpers do not prove that the agent exited.
        let effective = detected.clone().or_else(|| {
            if fg_is_shell {
                None
            } else {
                entry.agent_type.clone()
            }
        });
        entry.foreground_probe_result = effective.clone();
        if detected.is_none()
            && !fg_is_shell
            && effective.is_none()
            && !entry.unknown_foreground_warned
        {
            entry.unknown_foreground_warned = true;
            tracing::warn!(session_id, foreground_process = %fg_name, "Unrecognized non-shell foreground process; if this is an agent, Enter uses the safe gap");
        }
        if let Some(agent) = detected.as_ref() {
            if entry.agent_type.as_ref() != Some(agent) {
                entry.agent_type_from_run_config = false;
            }
            entry.agent_foreground_observed = true;
        } else if !fg_is_shell && entry.agent_type_from_run_config && effective.is_some() {
            // Any non-shell carrying a preset counts, including unknown wrappers.
            // direnv/nvm startup helpers may disarm early: fail closed rather than
            // allow unattended submit into the returned shell.
            entry.agent_foreground_observed = true;
        }
        let next = if fg_is_shell {
            if entry.agent_type_from_run_config && !entry.agent_foreground_observed {
                entry.agent_type.clone()
            } else {
                entry.agent_type_from_run_config = false;
                None
            }
        } else {
            effective.clone()
        };
        let changed = entry.agent_type != next;
        if changed {
            entry.telegram_registration_lifetime = None;
            entry.agent_type = next;
            entry.hook_instrumented = hook_instrumented_for(
                &crate::config::load_agents_config(),
                entry.agent_type.as_deref(),
            );
        }
        (effective, changed)
    };

    // The screen may already be quiet when the process is discovered. Its
    // cached shell-era verdict is not evidence about the newly known agent.
    // Use the reader's grid -> silence lock order; future chunks share the cache.
    if identity_changed && let Some(vt) = state.grid.vt_log_buffers.get(session_id) {
        let vt = vt.lock();
        // A newer accepted snapshot owns the cache too. The grid lock orders
        // reclassification; do not publish an older identity after its successor.
        if state
            .session_maps
            .session_states
            .get(session_id)
            .is_none_or(|entry| entry.foreground_probe_generation != generation)
        {
            return state
                .session_maps
                .session_states
                .get(session_id)
                .and_then(|entry| entry.foreground_probe_result.clone());
        }
        let activity = detect_agent_screen_activity_at(
            effective.as_deref(),
            &vt.screen_rows(),
            Some(vt.grid_columns()),
        );
        let offset = state
            .session_maps
            .output_buffers
            .get(session_id)
            .map(|buffer| buffer.lock().total_written)
            .unwrap_or(0);
        if let Some(silence) = state.session_maps.silence_states.get(session_id) {
            silence.lock().record_screen_activity(activity, offset);
        }
    }

    effective
}

/// Walk the process tree from `root_pid` and return the deepest descendant PID.
/// On Windows, this finds the "foreground" process in a PTY session by following
/// Normalize a path by resolving `.` and `..` components logically
/// (without requiring the path to exist on disk).
pub(super) fn normalize_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut result = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::CurDir => {}
            other => result.push(other),
        }
    }
    result
}

/// PID of a session's deepest foreground process — the agent itself, not the
/// shell that launched it.
///
/// On Unix the process group leader answers it directly; on Windows there is no
/// such notion, so the process tree is walked. Both the Tauri command and the
/// HTTP handler call this rather than keeping a copy of the split.
pub(crate) fn session_leaf_pid(state: &AppState, session_id: &str) -> Option<u32> {
    let entry = state.session_maps.sessions.get(session_id)?;
    let session = entry.value().lock();
    #[cfg(not(windows))]
    {
        let pgid = session.master.process_group_leader()?;
        u32::try_from(pgid).ok()
    }
    #[cfg(windows)]
    {
        deepest_descendant_pid(session._child.process_id()?)
    }
}

/// the chain: shell → agent CLI (e.g. claude.exe).
#[cfg(windows)]
pub(crate) fn deepest_descendant_pid(root_pid: u32) -> Option<u32> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next, TH32CS_SNAPPROCESS,
    };

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
        let mut children_map: std::collections::HashMap<u32, Vec<u32>> =
            std::collections::HashMap::new();
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
        while let Some([only_child]) = children_map.get(&current).map(Vec::as_slice) {
            current = *only_child;
        }

        Some(current)
    }
}

/// Map a process name to a known agent type, or None for non-agent processes.
///
/// A versioned basename counts as the tool. grok 1.0.5 installs
/// `~/.grok/bin/grok` as a symlink to `grok-1.0.5`, and `proc_pidpath` resolves
/// the link, so the foreground process reads `grok-1.0.5`. Against an
/// exact-match table that is None: the session gets no `agent_type`, so
/// `has_ready_screen_adapter` is false and the OSC 133 busy bit set once by the
/// long-lived `grok` command is never cleared — the tab stays working for the
/// whole process. An agent that renames its binary per release must not be able
/// to un-detect itself.
pub(crate) fn classify_agent(process_name: &str) -> Option<&'static str> {
    exact_agent_name(process_name).or_else(|| exact_agent_name(strip_version_suffix(process_name)))
}

/// Drop a trailing `-<version>` from an executable basename (`grok-1.0.5` →
/// `grok`). The suffix must start with a digit, so a hyphenated tool name
/// (`cursor-agent`) keeps its own identity.
fn strip_version_suffix(name: &str) -> &str {
    match name.rsplit_once('-') {
        Some((base, suffix)) if suffix.starts_with(|c: char| c.is_ascii_digit()) => base,
        _ => name,
    }
}

fn exact_agent_name(process_name: &str) -> Option<&'static str> {
    match process_name {
        "claude" => Some("claude"),
        "gemini" => Some("gemini"),
        "opencode" => Some("opencode"),
        "aider" => Some("aider"),
        "codex" => Some("codex"),
        "amp" => Some("amp"),
        "cursor-agent" => Some("cursor"),
        "goose" => Some("goose"),
        "grok" => Some("grok"),
        "droid" => Some("droid"),
        "pi" => Some("pi"),
        // The terminal CLI only: the AI Chat ego runs over ACP with no PTY, so no
        // process tree of a terminal session ever reaches this name for it.
        "ego" => Some("ego"),
        _ => None,
    }
}

/// Per-process resource usage for the process manager modal.
#[derive(Clone, Serialize)]
pub(crate) struct ProcessStats {
    pub(crate) session_id: Option<String>,
    pub(crate) name: String,
    pub(crate) pid: u32,
    pub(crate) rss_kb: u64,
    pub(crate) cpu_pct: f32,
}

/// Collect CPU/memory stats for TUIC itself and all PTY child process trees.
pub(crate) fn collect_process_stats(state: &AppState) -> Vec<ProcessStats> {
    let mut pids: Vec<(Option<String>, String, u32)> = Vec::new();

    // TUIC's own process
    let own_pid = std::process::id();
    pids.push((None, "TUICommander".to_string(), own_pid));

    // One process-table query serves every session below.
    let parent_map = process_parent_map();

    // Collect child PIDs from all PTY sessions
    for entry in state.session_maps.sessions.iter() {
        let session_id = entry.key().clone();
        let session = entry.value().lock();
        let display = session
            .display_name
            .clone()
            .unwrap_or_else(|| session_id.chars().take(8).collect());

        #[cfg(not(windows))]
        let child_pid = session.master.process_group_leader().map(|p| p as u32);
        #[cfg(windows)]
        let child_pid = session._child.process_id();
        drop(session);

        if let Some(pid) = child_pid {
            pids.push((Some(session_id.clone()), display.clone(), pid));
            // Walk descendants out of the shared map
            for dpid in parent_map
                .as_ref()
                .map(|map| descendants_from_parent_map(map, pid))
                .unwrap_or_default()
            {
                let name = process_name_from_pid(dpid).unwrap_or_else(|| format!("pid:{dpid}"));
                pids.push((Some(session_id.clone()), name, dpid));
            }
        }
    }

    if pids.is_empty() {
        return vec![];
    }

    let pid_list: Vec<u32> = pids.iter().map(|(_, _, p)| *p).collect();
    let stats_map = query_process_stats(&pid_list);

    pids.into_iter()
        .map(|(sid, name, pid)| {
            let (rss_kb, cpu_pct) = stats_map.get(&pid).copied().unwrap_or((0, 0.0));
            ProcessStats {
                session_id: sid,
                name,
                pid,
                rss_kb,
                cpu_pct,
            }
        })
        .collect()
}

/// Map every live process onto its children, from a SINGLE OS query.
///
/// The process-manager refresh walks one subtree per session. Querying the
/// table inside that loop forked `ps` once per session — N+1 forks per refresh,
/// each one taken while the session lock was held. One shared map serves every
/// root, so the cost no longer scales with the number of sessions.
pub(super) fn process_parent_map() -> Option<std::collections::HashMap<u32, Vec<u32>>> {
    #[cfg(not(windows))]
    {
        let output = std::process::Command::new("ps")
            .args(["-eo", "pid,ppid"])
            .output()
            .ok()?;
        let parent_map = parse_process_parent_map(&String::from_utf8_lossy(&output.stdout));
        (!parent_map.is_empty()).then_some(parent_map)
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
        let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snap.is_null() {
            return None;
        }
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut parent_map: std::collections::HashMap<u32, Vec<u32>> =
            std::collections::HashMap::new();
        if unsafe { Process32FirstW(snap, &mut entry) } != 0 {
            loop {
                parent_map
                    .entry(entry.th32ParentProcessID)
                    .or_default()
                    .push(entry.th32ProcessID);
                if unsafe { Process32NextW(snap, &mut entry) } == 0 {
                    break;
                }
            }
        }
        let _ = unsafe { CloseHandle(snap) };
        (!parent_map.is_empty()).then_some(parent_map)
    }
}

/// Parse `ps -eo pid,ppid` output into a parent -> children map.
///
/// Rows that do not read as two PIDs (the header, a truncated line) are
/// skipped. Aborting on the first unreadable row would report every session as
/// childless, and one shared map makes that failure global instead of local.
#[cfg(not(windows))]
pub(super) fn parse_process_parent_map(text: &str) -> std::collections::HashMap<u32, Vec<u32>> {
    let mut parent_map: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(Ok(pid)), Some(Ok(parent_pid))) = (
            parts.next().map(str::parse::<u32>),
            parts.next().map(str::parse::<u32>),
        ) else {
            continue;
        };
        parent_map.entry(parent_pid).or_default().push(pid);
    }
    parent_map
}

/// Every transitive descendant of `root`, excluding the root itself.
///
/// `seen` guards the walk: the table comes from the OS, and a self-parented row
/// would otherwise spin forever inside a stats refresh.
pub(super) fn descendants_from_parent_map(
    parent_map: &std::collections::HashMap<u32, Vec<u32>>,
    root: u32,
) -> Vec<u32> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::from([root]);
    let mut stack = vec![root];
    while let Some(pid) = stack.pop() {
        let Some(children) = parent_map.get(&pid) else {
            continue;
        };
        for &child in children {
            if seen.insert(child) {
                result.push(child);
                stack.push(child);
            }
        }
    }
    result
}

/// Query RSS (KB) and CPU% for a batch of PIDs using `ps` on Unix.
#[cfg(not(windows))]
pub(super) fn query_process_stats(pids: &[u32]) -> std::collections::HashMap<u32, (u64, f32)> {
    let mut map = std::collections::HashMap::new();
    if pids.is_empty() {
        return map;
    }
    let pid_args: Vec<String> = pids.iter().map(|p| p.to_string()).collect();
    let Ok(output) = std::process::Command::new("ps")
        .args(["-o", "pid,rss,%cpu", "-p"])
        .arg(pid_args.join(","))
        .output()
    else {
        return map;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3
            && let (Ok(pid), Ok(rss), Ok(cpu)) = (
                parts[0].parse::<u32>(),
                parts[1].parse::<u64>(),
                parts[2].parse::<f32>(),
            )
        {
            map.insert(pid, (rss, cpu));
        }
    }
    map
}

/// Query RSS (KB) and CPU% for a batch of PIDs on Windows.
#[cfg(windows)]
pub(super) fn query_process_stats(pids: &[u32]) -> std::collections::HashMap<u32, (u64, f32)> {
    let mut map = std::collections::HashMap::new();
    for &pid in pids {
        if let Some((rss, cpu)) = query_single_process_windows(pid) {
            map.insert(pid, (rss, cpu));
        }
    }
    map
}

#[cfg(windows)]
fn query_single_process_windows(pid: u32) -> Option<(u64, f32)> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };
    let handle = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut mem_info: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    mem_info.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    let ok = unsafe {
        GetProcessMemoryInfo(
            handle,
            &mut mem_info,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
    };
    let _ = unsafe { CloseHandle(handle) };
    if ok == 0 {
        return None;
    }
    let rss_kb = mem_info.WorkingSetSize / 1024;
    Some((rss_kb as u64, 0.0))
}
