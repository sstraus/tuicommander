//! Codex adapter: `codex exec fork --ephemeral` on the bound session.
//!
//! Measured on codex-cli 0.169.0 (2026-11-21, `ideas/discussion.md`):
//! - Only an ephemeral fork is routed onto the main session's prompt cache; a
//!   persistent fork gets its own cache key and reads the generic prefix only.
//! - An ephemeral fork writes no rollout, so it cannot be resumed: a follow-up
//!   forks the main session again and its prompt carries the thread.
//! - The fork replays the recorded initial context (developer instructions,
//!   hook output), so it needs no `developer_instructions` override. With
//!   hooks on, `SessionStart` runs again and its output lands after the
//!   history; `--disable hooks` removes that tail and the hook side effects.
//! - `turn.completed` usage is cumulative over the thread and a fork inherits
//!   the main session's totals, so usage is the difference from those totals.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{Adapter, ForkOutcome, ForkTarget, Turn, Usage};

/// Cumulative token totals of a Codex thread.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Totals {
    pub input: u64,
    pub cached_input: u64,
    pub output: u64,
}

impl Totals {
    fn from_usage(usage: &Value) -> Self {
        let n = |key: &str| usage[key].as_u64().unwrap_or(0);
        Totals {
            input: n("input_tokens"),
            cached_input: n("cached_input_tokens"),
            output: n("output_tokens"),
        }
    }
}

pub(crate) struct Codex {
    /// The main session's totals when the fork target was resolved.
    pub inherited: Totals,
}

impl Adapter for Codex {
    fn args(&self, target: &ForkTarget, _turn: &Turn, prompt: &str) -> Vec<String> {
        let mut args: Vec<String> = [
            "exec",
            "fork",
            "--ephemeral",
            "--disable",
            "hooks",
            "-c",
            "sandbox_mode=\"read-only\"",
        ]
        .map(str::to_owned)
        .into();
        if let Some(model) = &target.model {
            args.extend(["-m".to_owned(), model.clone()]);
        }
        // The main session may run outside a Git repository; `exec` refuses that by default.
        args.extend(["--skip-git-repo-check", "--json"].map(str::to_owned));
        // The prompt starts with `<side-question>`, never with a dash.
        args.extend([target.main_session_id.clone(), prompt.to_owned()]);
        args
    }

    fn parse(&self, stdout: &str) -> Result<ForkOutcome, String> {
        parse_events(stdout, self.inherited)
    }

    fn replays_thread(&self) -> bool {
        true
    }
}

/// Parse `codex exec --json` output. `inherited` holds the totals the fork
/// started from.
pub(crate) fn parse_events(stdout: &str, inherited: Totals) -> Result<ForkOutcome, String> {
    let mut thread_id = None;
    let mut text = None;
    let mut totals = None;
    for line in stdout.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue; // CLI warnings can share stdout with the events
        };
        match event["type"].as_str() {
            Some("thread.started") => thread_id = event["thread_id"].as_str().map(str::to_owned),
            // The last agent message is the answer; earlier ones are commentary.
            Some("item.completed") if event["item"]["type"] == "agent_message" => {
                text = event["item"]["text"].as_str().map(str::to_owned);
            }
            Some("turn.completed") => totals = Some(Totals::from_usage(&event["usage"])),
            Some("turn.failed") => {
                return Err(format!(
                    "codex reported an error: {}",
                    event["error"]["message"]
                ));
            }
            Some("error") => return Err(format!("codex reported an error: {}", event["message"])),
            _ => {}
        }
    }
    let totals = totals.ok_or("codex ended without a turn.completed event")?;
    let fork_session_id = thread_id
        .filter(|id| !id.is_empty())
        .ok_or("codex output has no thread id")?;
    let cached = totals.cached_input.saturating_sub(inherited.cached_input);
    let usage = Usage {
        // Codex counts cached tokens inside the input; the journal keeps them apart.
        input_tokens: totals
            .input
            .saturating_sub(inherited.input)
            .saturating_sub(cached),
        output_tokens: totals.output.saturating_sub(inherited.output),
        cache_read_tokens: cached,
        cache_creation_tokens: 0,
        cost_usd: None,
    };
    Ok(ForkOutcome {
        fork_session_id,
        text: text.unwrap_or_default(),
        usage,
    })
}

/// What a fork needs from the main session's rollout.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct RolloutState {
    pub model: Option<String>,
    pub cwd: Option<PathBuf>,
    pub totals: Totals,
}

/// The newest model, cwd and token totals of a rollout.
///
/// The model can change mid-session and the fork reuses the cache only with
/// the same model, so it comes from the last `turn_context`. One record can be
/// megabytes of tool output, so a tail of fixed size could miss both records:
/// the file is read once, and only the two record types are decoded.
pub(crate) fn rollout_state(path: &Path) -> std::io::Result<RolloutState> {
    let mut state = RolloutState::default();
    for line in BufReader::new(std::fs::File::open(path)?).lines() {
        let line = line?;
        let turn_context = line.contains("\"turn_context\"");
        if !turn_context && !line.contains("\"token_count\"") {
            continue;
        }
        let Ok(record) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let payload = &record["payload"];
        if turn_context && record["type"] == "turn_context" {
            if let Some(model) = payload["model"].as_str().filter(|m| !m.is_empty()) {
                state.model = Some(model.to_owned());
            }
            if let Some(cwd) = payload["cwd"].as_str() {
                state.cwd = Some(PathBuf::from(cwd));
            }
        } else if record["type"] == "event_msg" && payload["type"] == "token_count" {
            let total = &payload["info"]["total_token_usage"];
            if total.is_object() {
                state.totals = Totals::from_usage(total);
            }
        }
    }
    Ok(state)
}
