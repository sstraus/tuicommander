//! ego's documented local transcript v1, never its private session log.
//! Completed messages are authoritative; incremental previews are omitted so
//! completion cannot append the answer a second time.

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};

use super::claude::{AdapterStats, MAX_TEXT_CHARS, cap_strings, clean, notice};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ExportSource {
    pub program: PathBuf,
    pub home: PathBuf,
    pub session: String,
}

impl ExportSource {
    /// The shared ticker owns follow/rebind/idle cleanup. Each read asks ego
    /// for committed events after the last completely parsed sequence.
    pub fn read(
        &self,
        after: u64,
        mut absorb: impl FnMut(&str) -> io::Result<()>,
    ) -> io::Result<()> {
        use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| io::Error::other("ego transcript export requires the runtime"))?;
        runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(10), async {
                let mut child = tokio::process::Command::new(&self.program)
                    .args(["export", &self.session, "--format", "transcript", "--after", &after.to_string()])
                    .env("EGO_HOME", &self.home)
                    .env_remove("EGO_PROFILE")
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .kill_on_drop(true)
                    .spawn()?;
                let result = async {
                    let mut output = BufReader::new(child.stdout.take().expect("piped stdout"));
                    let mut header = false;
                    loop {
                        let mut line = String::new();
                        let n = (&mut output).take(1_048_577).read_line(&mut line).await?;
                        if n == 0 { break; }
                        if n > 1_048_576 {
                            return Err(io::Error::other("ego transcript record exceeds one MiB"));
                        }
                        // An interrupted final record is not a committed cursor.
                        if !line.ends_with('\n') { break; }
                        if !header {
                            let row: Value = serde_json::from_str(&line).map_err(io::Error::other)?;
                            if row["type"] != "transcript.header" || row["v"] != 1 || row["session"] != self.session {
                                return Err(io::Error::other("ego transcript v1 header/session mismatch"));
                            }
                            header = true;
                        }
                        absorb(&line)?;
                    }
                    Ok(header)
                }.await;
                let header = match result {
                    Ok(header) => header,
                    Err(error) => {
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                        return Err(error);
                    }
                };
                let status = child.wait().await?;
                if !status.success() {
                    return Err(io::Error::other(format!(
                        "ego export failed ({status}); the configured executable {} must support local transcript v1 and read this session; check the ego executable in Settings",
                        self.program.display(),
                    )));
                }
                if !header { return Err(io::Error::other("ego transcript v1 header missing")); }
                Ok(())
            }).await.map_err(|_| io::Error::other("ego transcript export timed out after ten seconds"))?
        })
    }
}

#[derive(Default)]
pub(crate) struct EgoAdapter {
    pub stats: AdapterStats,
    session: Option<String>,
    pub after: u64,
}

impl EgoAdapter {
    pub(crate) fn absorb(&mut self, line: &str) -> Vec<Value> {
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            self.stats.malformed_rows += 1;
            return Vec::new();
        };
        if row["type"] == "transcript.header" {
            // A caller must reset the View before crossing a session boundary.
            if row["v"] == 1 {
                self.session = row["session"].as_str().map(str::to_owned);
            } else {
                self.session = None;
            }
            return Vec::new();
        }
        if row["type"] != "transcript.event" {
            self.stats.unknown_rows += 1;
            return Vec::new();
        }
        if self.session.as_deref() != row["session"].as_str() || self.session.is_none() {
            self.stats.malformed_rows += 1;
            return Vec::new();
        }
        let Some(seq) = row["seq"].as_u64() else {
            self.stats.malformed_rows += 1;
            return Vec::new();
        };
        if seq <= self.after {
            return Vec::new();
        }
        self.after = seq;
        let event = &row["event"];
        let text = event["text"].as_str().unwrap_or_default();
        let id = row["item"].as_str().unwrap_or_default();
        match (event["type"].as_str(), event["kind"].as_str()) {
            (Some("item_completed"), Some("user_message" | "agent_message" | "reasoning")) => {
                if text.is_empty() || id.is_empty() {
                    return Vec::new();
                }
                vec![json!({"sessionUpdate": match event["kind"].as_str() {
                    Some("user_message") => "user_message_chunk",
                    Some("reasoning") => "agent_thought_chunk",
                    _ => "agent_message_chunk",
                }, "messageId": id, "content": {"type": "text", "text": clean(text, MAX_TEXT_CHARS)}})]
            }
            (Some("item_started"), Some("tool")) if !id.is_empty() => {
                let name = clean(event["name"].as_str().unwrap_or("tool"), 120);
                let input = serde_json::from_str::<Value>(text)
                    .unwrap_or_else(|_| json!({"arguments": text}));
                vec![json!({"sessionUpdate": "tool_call", "toolCallId": id,
                    "title": name, "kind": "other", "status": "in_progress", "rawInput": cap_strings(&input)})]
            }
            (Some("item_completed"), Some("tool")) if !id.is_empty() => {
                vec![
                    json!({"sessionUpdate": "tool_call_update", "toolCallId": id,
                    "status": if event["ok"] == true { "completed" } else { "failed" },
                    "content": [{"type": "content", "content": {"type": "text", "text": clean(text, 4_000)}}]}),
                ]
            }
            (Some("item_completed"), Some("context_checkpoint")) => vec![notice(&format!(
                "Conversation compacted\n\n{}",
                clean(text, MAX_TEXT_CHARS)
            ))],
            (Some("goal_continuation"), _) => vec![notice(&format!(
                "Goal continuation\n\n{}",
                clean(text, MAX_TEXT_CHARS)
            ))],
            (
                Some(
                    "item_delta" | "turn_completed" | "usage_updated" | "goal_set" | "goal_updated"
                    | "goal_transitioned",
                ),
                _,
            ) => Vec::new(),
            _ => {
                self.stats.unknown_rows += 1;
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests;
