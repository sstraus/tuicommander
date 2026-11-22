//! ego adapter: `ego fork`, then `ego resume` on the persisted fork.
//!
//! Measured on an ego build of 8782c15de (2026-11-21, story 3114-1ba7):
//! - ego has no command that forks and asks at once (ego story 1181-f262), so
//!   a new thread runs `ego fork ID --output json` first. The fork is a stored
//!   session and a follow-up resumes it.
//! - A fork reads the main session's prefix from the cache only when ego keys
//!   the cache by model, developer item and tools (ego 4564cbb9b). ego 0.4.18
//!   keys it by session lineage and the first fork reads nothing.
//! - `--mode plan` keeps the cache and refuses a mutating tool call
//!   (`blocked: mode floor`): the turn then ends with `PermissionRequired` and
//!   no answer. The user configuration can make `yolo` the default mode, so
//!   the flag is not optional.
//! - `--no-hooks` keeps the cache. With hooks on, plan mode refuses each hook
//!   and reports it as an `error` event that does not end the turn.
//! - `--output json` reports usage summed over every turn of the fork, so the
//!   parser reads `--output jsonl` and adds the `usage_delta` of this run.
//! - `ego resume` reads the prompt from stdin when stdin is not a terminal and
//!   no data arrives: the runner must give it a null stdin.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{Adapter, ForkOutcome, ForkTarget, Turn, Usage};

pub(crate) struct Ego;

impl Adapter for Ego {
    fn args(&self, target: &ForkTarget, turn: &Turn, prompt: &str) -> Vec<String> {
        let Turn::FollowUp {
            fork_session_id, ..
        } = turn
        else {
            // Not reached: `fork_args` makes the runner fork first. The fork
            // command alone answers nothing, and never writes to the main session.
            return fork_command(target);
        };
        // The prompt starts with `<side-question>`, never with a dash.
        let mut args = vec![
            "resume".to_owned(),
            fork_session_id.clone(),
            prompt.to_owned(),
        ];
        if let Some(model) = &target.model {
            args.extend(["-m".to_owned(), model.clone()]);
        }
        args.extend(["--mode", "plan", "--no-hooks", "--output", "jsonl"].map(str::to_owned));
        args
    }

    fn parse(&self, stdout: &str) -> Result<ForkOutcome, String> {
        parse_events(stdout)
    }

    fn fork_args(&self, target: &ForkTarget) -> Option<Vec<String>> {
        Some(fork_command(target))
    }

    fn parse_fork(&self, stdout: &str) -> Result<String, String> {
        parse_fork(stdout)
    }
}

fn fork_command(target: &ForkTarget) -> Vec<String> {
    ["fork", &target.main_session_id, "--output", "json"]
        .map(str::to_owned)
        .into()
}

/// The new session id in the output of `ego fork --output json`.
pub(crate) fn parse_fork(stdout: &str) -> Result<String, String> {
    let result = stdout
        .lines()
        .rev()
        .find_map(|line| {
            serde_json::from_str::<Value>(line)
                .ok()
                .filter(Value::is_object)
        })
        .ok_or("ego fork printed no result")?;
    if result["ok"].as_bool() != Some(true) {
        return Err(format!("ego fork failed: {}", result["exit_reason"]));
    }
    result["session"]
        .as_str()
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "ego fork result has no session id".to_owned())
}

/// Parse `ego resume --output jsonl` output.
pub(crate) fn parse_events(stdout: &str) -> Result<ForkOutcome, String> {
    let mut session = None;
    let mut text = None;
    let mut usage = Usage::default();
    let mut completed = None;
    for line in stdout.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue; // CLI warnings can share stdout with the events
        };
        match event["type"].as_str() {
            Some("session_started") => session = event["session"].as_str().map(str::to_owned),
            // One event per model request of this run.
            Some("usage_updated") => {
                let n = |key: &str| event["usage_delta"][key].as_u64().unwrap_or(0);
                usage.input_tokens += n("uncached_input_tokens");
                usage.output_tokens += n("output_tokens");
                usage.cache_read_tokens += n("cache_read_tokens");
                usage.cache_creation_tokens +=
                    n("cache_write_tokens") + n("cache_write_extended_tokens");
            }
            // The last agent message is the answer; earlier ones are commentary.
            Some("item_completed") if event["kind"] == "agent_message" => {
                text = event["text"].as_str().map(str::to_owned);
            }
            Some("turn_completed") => completed = Some(event),
            // An `error` event (a refused hook) does not end the turn.
            _ => {}
        }
    }
    let completed = completed.ok_or("ego ended without a turn_completed event")?;
    if completed["ok"].as_bool() != Some(true) {
        return Err(format!("ego stopped: {}", completed["exit_reason"]));
    }
    let fork_session_id = session
        .filter(|id| !id.is_empty())
        .ok_or("ego output has no session id")?;
    Ok(ForkOutcome {
        fork_session_id,
        text: text.unwrap_or_default(),
        usage,
    })
}

/// Model (`provider/model`) and workspace of a stored ego session.
///
/// The fork reuses the cache only with the main session's model, so the model
/// is passed to `ego resume` and never left to the configured default.
pub(crate) fn manifest_model_and_cwd(manifest: &Path) -> Result<(Option<String>, PathBuf), String> {
    let text = std::fs::read_to_string(manifest)
        .map_err(|e| format!("cannot read the ego manifest: {e}"))?;
    let manifest: Value =
        serde_json::from_str(&text).map_err(|e| format!("the ego manifest is invalid: {e}"))?;
    let model = manifest["model"]
        .as_str()
        .filter(|m| !m.is_empty())
        .map(str::to_owned);
    let cwd = manifest["workspace_root"]
        .as_str()
        .filter(|path| !path.is_empty())
        .ok_or("the ego manifest records no workspace")?;
    Ok((model, PathBuf::from(cwd)))
}
