//! Claude Code adapter: `claude -p` on a fork of the bound session.
//!
//! Measured on claude 2.1.322 (2026-11-21, `ideas/discussion.md`):
//! `--permission-mode plan` keeps the cache and blocks writes; whole-tool deny
//! rules change the tool list and lose the cache. `total_cost_usd` and
//! `modelUsage` in the result are cumulative over the resumed chain, so usage
//! comes from `model_usage_since_last_result`, or from the per-call usage of
//! the assistant events deduplicated by message id.

use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{Adapter, ForkOutcome, ForkTarget, Turn, Usage};

/// How much of the transcript tail is read to find the current model and cwd.
const TRANSCRIPT_TAIL_BYTES: u64 = 512 * 1024;

pub(crate) struct Claude;

impl Adapter for Claude {
    fn args(&self, target: &ForkTarget, turn: &Turn, prompt: &str) -> Vec<String> {
        let mut args = vec!["-p".to_owned(), prompt.to_owned()];
        match turn {
            Turn::NewFork => {
                args.extend(["--resume".to_owned(), target.main_session_id.clone()]);
                args.push("--fork-session".to_owned());
            }
            Turn::FollowUp {
                fork_session_id, ..
            } => {
                args.extend(["--resume".to_owned(), fork_session_id.clone()]);
            }
        }
        if let Some(model) = &target.model {
            args.extend(["--model".to_owned(), model.clone()]);
        }
        args.extend(
            [
                "--permission-mode",
                "plan",
                "--output-format",
                "stream-json",
                "--verbose",
            ]
            .map(str::to_owned),
        );
        args
    }

    fn parse(&self, stdout: &str) -> Result<ForkOutcome, String> {
        parse_stream(stdout)
    }
}

/// Parse `--output-format stream-json --verbose` output.
pub(crate) fn parse_stream(stdout: &str) -> Result<ForkOutcome, String> {
    let mut seen = HashSet::new();
    let mut per_call = Usage::default();
    let mut result: Option<Value> = None;
    for line in stdout.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue; // CLI warnings can share stdout with the events
        };
        match event["type"].as_str() {
            // One event per content block; every block of a call repeats the usage.
            Some("assistant") => {
                let message = &event["message"];
                let id = message["id"].as_str().unwrap_or_default().to_owned();
                if seen.insert(id) {
                    per_call.add_call(&message["usage"]);
                }
            }
            Some("result") => result = Some(event),
            _ => {}
        }
    }
    let result = result.ok_or("claude ended without a result event")?;
    let text = result["result"].as_str().unwrap_or_default().to_owned();
    if result["is_error"].as_bool() == Some(true) {
        let detail = if text.is_empty() {
            result["subtype"].to_string()
        } else {
            text
        };
        return Err(format!("claude reported an error: {detail}"));
    }
    let fork_session_id = result["session_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or("claude result has no session id")?
        .to_owned();
    let usage =
        Usage::from_model_usage(&result["model_usage_since_last_result"]).unwrap_or(per_call);
    Ok(ForkOutcome {
        fork_session_id,
        text,
        usage,
    })
}

/// Model and cwd of the latest assistant record in a Claude transcript.
///
/// The model can change mid-session (`/model`), and the fork reuses the cache
/// only with the same model, so it is read from the transcript and never from
/// the launch flags. `<synthetic>` records are not API calls and are skipped.
pub(crate) fn transcript_model_and_cwd(
    path: &Path,
) -> std::io::Result<(Option<String>, Option<PathBuf>)> {
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(TRANSCRIPT_TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail)?;
    let tail = String::from_utf8_lossy(&tail);
    let mut lines: Vec<&str> = tail.lines().collect();
    if start > 0 && !lines.is_empty() {
        lines.remove(0); // the first line is cut by the seek
    }
    let mut model = None;
    let mut cwd = None;
    for line in lines.iter().rev() {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if cwd.is_none() {
            cwd = record["cwd"].as_str().map(PathBuf::from);
        }
        if model.is_none() && record["type"] == "assistant" {
            model = record["message"]["model"]
                .as_str()
                .filter(|m| !m.is_empty() && *m != "<synthetic>")
                .map(str::to_owned);
        }
        if model.is_some() && cwd.is_some() {
            break;
        }
    }
    Ok((model, cwd))
}
