//! Discussion: side questions answered in a headless, read-only fork of the
//! agent session running in a terminal. Plan: `plans/discussion.md`.
//!
//! A fork is affordable only when it reads the main session's prefix from the
//! prompt cache, so the fork runs with the same model, tools and system prompt
//! as the main session and every instruction goes into the user message.

pub(crate) mod capture;
pub(crate) mod claude;
pub(crate) mod codex;
pub(crate) mod ego;
pub(crate) mod header;
mod markers;
pub(crate) mod service;
pub(crate) mod store;
mod wrap;

#[cfg(test)]
mod codex_tests;
#[cfg(test)]
mod ego_tests;
#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tokio::sync::oneshot;

pub(crate) use header::{Category, Classified};

/// Longest a fork may run before it is killed.
pub(crate) const FORK_TIMEOUT: Duration = Duration::from_secs(300);
/// Stderr kept for an error message, in chars.
const STDERR_TAIL_CHARS: usize = 2000;

/// One agent CLI's headless fork. Everything else (header, storage, events)
/// is shared.
pub(crate) trait Adapter: Send + Sync {
    /// Arguments after the program name.
    fn args(&self, target: &ForkTarget, turn: &Turn, prompt: &str) -> Vec<String>;
    /// Parse the complete stdout of a finished run.
    fn parse(&self, stdout: &str) -> Result<ForkOutcome, String>;
    /// True when the CLI cannot continue a fork: every turn forks the main
    /// session again and a follow-up prompt carries the thread's exchanges.
    fn replays_thread(&self) -> bool {
        false
    }
    /// Arguments of the command that creates the fork, for a CLI that cannot
    /// fork and ask in one command. The runner executes it before a new
    /// thread's first turn and then calls `args` with a `Turn::FollowUp` on
    /// the id from `parse_fork`.
    fn fork_args(&self, _target: &ForkTarget) -> Option<Vec<String>> {
        None
    }
    /// The fork session id in the stdout of the `fork_args` command.
    fn parse_fork(&self, _stdout: &str) -> Result<String, String> {
        Err("this agent forks and asks in one command".to_owned())
    }
}

/// Where and how to start the agent CLI for a fork.
#[derive(Debug, Clone)]
pub(crate) struct ForkTarget {
    pub program: PathBuf,
    pub cwd: PathBuf,
    /// The running agent's session env (config dir), so the fork reads the
    /// same transcript store and settings.
    pub env: Vec<(String, String)>,
    pub main_session_id: String,
    /// The main session's current model; `None` lets the CLI pick its default.
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Turn {
    /// A new question: fork the main session as it stands now.
    NewFork,
    /// A follow-up: continue an existing fork, or fork again and replay
    /// `prior` (`Adapter::replays_thread`).
    FollowUp {
        fork_session_id: String,
        prior: Vec<Exchange>,
    },
}

/// An answered question of a thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Exchange {
    pub question: String,
    pub answer: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub(crate) struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cost_usd: Option<f64>,
}

impl Usage {
    /// Add one API call's `usage` object (Anthropic field names).
    fn add_call(&mut self, usage: &Value) {
        let n = |key: &str| usage[key].as_u64().unwrap_or(0);
        self.input_tokens += n("input_tokens");
        self.output_tokens += n("output_tokens");
        self.cache_read_tokens += n("cache_read_input_tokens");
        self.cache_creation_tokens += n("cache_creation_input_tokens");
    }

    /// Sum a per-model usage map (`{model: {inputTokens, ..., costUSD}}`).
    fn from_model_usage(models: &Value) -> Option<Self> {
        let models = models.as_object().filter(|m| !m.is_empty())?;
        let mut usage = Usage {
            cost_usd: Some(0.0),
            ..Usage::default()
        };
        for model in models.values() {
            let n = |key: &str| model[key].as_u64().unwrap_or(0);
            usage.input_tokens += n("inputTokens");
            usage.output_tokens += n("outputTokens");
            usage.cache_read_tokens += n("cacheReadInputTokens");
            usage.cache_creation_tokens += n("cacheCreationInputTokens");
            usage.cost_usd =
                Some(usage.cost_usd.unwrap_or(0.0) + model["costUSD"].as_f64().unwrap_or(0.0));
        }
        Some(usage)
    }
}

/// What the CLI returned for one turn.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ForkOutcome {
    pub fork_session_id: String,
    pub text: String,
    pub usage: Usage,
}

/// A finished turn: the CLI outcome plus the parsed header.
#[derive(Debug, Clone)]
pub(crate) struct Answer {
    pub outcome: ForkOutcome,
    pub classified: Classified,
}

/// Resolve the fork target of the agent running in a TUIC terminal.
///
/// Only an exact binding is accepted: forking a guessed session would answer
/// about another conversation.
pub(crate) fn resolve_target(
    state: &crate::state::AppState,
    session_id: &str,
) -> Result<(Box<dyn Adapter>, ForkTarget), String> {
    let agent_type = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|s| s.agent_type.clone())
        .ok_or("no agent is running in this terminal")?;
    match agent_type.as_str() {
        "claude" => resolve_claude(state, session_id),
        "codex" => resolve_codex(state, session_id),
        "ego" => resolve_ego(state, session_id),
        _ => Err(format!("Discussion does not support {agent_type} yet")),
    }
}

fn resolve_claude(
    state: &crate::state::AppState,
    session_id: &str,
) -> Result<(Box<dyn Adapter>, ForkTarget), String> {
    let source = crate::subagent_map::transcript_source(state, session_id)
        .ok_or("no Claude session is bound to this terminal")?;
    let transcript = source.parent_transcript;
    let main_session_id = transcript
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("the bound Claude transcript has no session id")?
        .to_owned();
    let (model, cwd) = claude::transcript_model_and_cwd(&transcript)
        .map_err(|e| format!("cannot read the Claude transcript: {e}"))?;
    let cwd = cwd.ok_or("the Claude transcript records no working directory")?;
    let (program, env) = live_process(state, session_id, "claude")?;
    Ok((
        Box::new(claude::Claude),
        ForkTarget {
            program,
            cwd,
            env,
            main_session_id,
            model,
        },
    ))
}

fn resolve_codex(
    state: &crate::state::AppState,
    session_id: &str,
) -> Result<(Box<dyn Adapter>, ForkTarget), String> {
    let rollout = crate::codex_binding::transcript(state, session_id)
        .ok_or("no exact Codex hook binding for this terminal")?;
    let main_session_id = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|s| s.codex_binding.as_ref().map(|b| b.session_id.clone()))
        .ok_or("no exact Codex hook binding for this terminal")?;
    let rollout = codex::rollout_state(&rollout)
        .map_err(|e| format!("cannot read the Codex rollout: {e}"))?;
    let cwd = rollout.cwd.ok_or("the Codex rollout records no turn yet")?;
    let (program, env) = live_process(state, session_id, "codex")?;
    let target = ForkTarget {
        program,
        cwd,
        env,
        main_session_id,
        model: rollout.model,
    };
    Ok((
        Box::new(codex::Codex {
            inherited: rollout.totals,
        }),
        target,
    ))
}

/// ego in a terminal only: AI Chat ego runs over ACP and has no PTY session.
fn resolve_ego(
    state: &crate::state::AppState,
    session_id: &str,
) -> Result<(Box<dyn Adapter>, ForkTarget), String> {
    let pid =
        crate::pty::session_leaf_pid(state, session_id).ok_or("the ego process has exited")?;
    let (home, main_session_id) = crate::agent_session::ego_chat_session_for_pid(pid)
        .ok_or("no exact ego lease binding for this terminal")?;
    let manifest = home
        .join("sessions")
        .join(&main_session_id)
        .join("manifest.json");
    let (model, cwd) = ego::manifest_model_and_cwd(&manifest)?;
    let (program, env) = live_process(state, session_id, "ego")?;
    Ok((
        Box::new(ego::Ego),
        ForkTarget {
            program,
            cwd,
            env,
            main_session_id,
            model,
        },
    ))
}

/// Binary and session env (config dir) of the agent running in the terminal.
fn live_process(
    state: &crate::state::AppState,
    session_id: &str,
    agent: &str,
) -> Result<(PathBuf, Vec<(String, String)>), String> {
    let pid = crate::pty::session_leaf_pid(state, session_id)
        .ok_or_else(|| format!("the {agent} process has exited"))?;
    let env = crate::agent_session::read_agent_env_overrides(agent, pid)
        .into_iter()
        .collect();
    Ok((agent_program(agent, pid)?, env))
}

/// The binary of the running agent when its argv names an absolute path,
/// otherwise the detected install (release builds have no shell PATH).
fn agent_program(agent: &str, pid: u32) -> Result<PathBuf, String> {
    if let Some(argv0) =
        crate::process_env::read_process_argv(pid).and_then(|a| a.into_iter().next())
    {
        let path = PathBuf::from(argv0);
        if path.is_absolute() && path.is_file() {
            return Ok(path);
        }
    }
    crate::agent::detect_agent_binary_sync(agent.to_owned())
        .path
        .map(PathBuf::from)
        .ok_or_else(|| format!("the {agent} executable was not found"))
}

/// Run one turn. Dropping the future (timeout, cancel) kills the process.
pub(crate) async fn run(
    adapter: &dyn Adapter,
    target: &ForkTarget,
    turn: &Turn,
    question: &str,
    cancel: oneshot::Receiver<()>,
    timeout: Duration,
) -> Result<Answer, String> {
    let outcome = tokio::select! {
        outcome = tokio::time::timeout(timeout, ask(adapter, target, turn, question)) => outcome
            .map_err(|_| format!("the fork did not answer within {} s", timeout.as_secs()))??,
        // A dropped sender is not a cancel: only an explicit send stops the run.
        Ok(()) = cancel => return Err("cancelled".to_owned()),
    };
    let classified = header::classify(&markers::strip(&outcome.text));
    if !classified.header_found {
        tracing::warn!(target: "discussion", fork = %outcome.fork_session_id, "answer has no classification header; filed as answer");
    }
    Ok(Answer {
        outcome,
        classified,
    })
}

/// The fork command of an adapter that needs one, then the question.
async fn ask(
    adapter: &dyn Adapter,
    target: &ForkTarget,
    turn: &Turn,
    question: &str,
) -> Result<ForkOutcome, String> {
    let forked;
    let turn = match (turn, adapter.fork_args(target)) {
        (Turn::NewFork, Some(args)) => {
            let fork_session_id = execute(target, args, |out| adapter.parse_fork(out)).await?;
            forked = Turn::FollowUp {
                fork_session_id,
                prior: Vec::new(),
            };
            &forked
        }
        _ => turn,
    };
    let prompt = match turn {
        Turn::FollowUp { prior, .. } if adapter.replays_thread() => {
            wrap::wrap_follow_up(prior, question)
        }
        _ => wrap::wrap_question(question),
    };
    execute(target, adapter.args(target, turn, &prompt), |out| {
        adapter.parse(out)
    })
    .await
}

/// Run the agent CLI once and parse its complete stdout.
async fn execute<T>(
    target: &ForkTarget,
    args: Vec<String>,
    parse: impl FnOnce(&str) -> Result<T, String>,
) -> Result<T, String> {
    let mut command = tokio::process::Command::new(&target.program);
    command
        .args(args)
        .current_dir(&target.cwd)
        .envs(target.env.iter().map(|(k, v)| (k, v)))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    let output = command
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", target.program.display()))?
        .wait_with_output()
        .await
        .map_err(|e| format!("fork process failed: {e}"))?;
    parse(&String::from_utf8_lossy(&output.stdout)).map_err(|error| {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: String = stderr
            .chars()
            .rev()
            .take(STDERR_TAIL_CHARS)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("{error} (exit {}): {}", output.status, tail.trim())
    })
}
