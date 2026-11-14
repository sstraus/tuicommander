//! The full text of one message the stream capped at `MAX_TEXT_CHARS`.
//!
//! The cap exists for the live log only; the agent's transcript file still holds
//! the whole message. This reads that file back for ONE message id through the
//! same Claude adapter the stream uses (uncapped, redaction kept), so there is
//! no second parser to drift.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use super::claude::ClaudeAdapter;
use super::{Agent, bound_transcript};
use crate::state::AppState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatFullText {
    pub text: String,
}

/// Prompt or reply text of `message_id`, uncapped. `None` when no row carries it.
fn read_message(path: &Path, message_id: &str) -> std::io::Result<Option<String>> {
    let mut adapter = ClaudeAdapter::uncapped();
    let mut text: Option<String> = None;
    for line in BufReader::new(std::fs::File::open(path)?).lines() {
        let line = line?;
        // A row of another message never needs parsing; the id is quoted in its own row.
        if !line.contains(message_id) {
            continue;
        }
        for update in adapter.absorb(&line) {
            let kind = update.get("sessionUpdate").and_then(Value::as_str);
            let ours = update.get("messageId").and_then(Value::as_str) == Some(message_id);
            if ours
                && matches!(kind, Some("user_message_chunk" | "agent_message_chunk"))
                && let Some(chunk) = update.pointer("/content/text").and_then(Value::as_str)
            {
                text.get_or_insert_with(String::new).push_str(chunk);
            }
        }
    }
    Ok(text)
}

pub(crate) fn chat_view_full_text(
    state: &Arc<AppState>,
    session_id: &str,
    message_id: &str,
) -> Result<ChatFullText, String> {
    if message_id.is_empty() {
        return Err("message id is empty".to_owned());
    }
    let bound = bound_transcript(state, session_id)?;
    if bound.agent != Agent::Claude {
        return Err("not_supported: full text is read from a Claude transcript only".to_owned());
    }
    match read_message(&bound.path, message_id) {
        Ok(Some(text)) => Ok(ChatFullText { text }),
        Ok(None) => Err("message not found in the transcript".to_owned()),
        Err(e) => Err(format!("transcript unreadable: {e}")),
    }
}

/// `blocking` pool wrapper: the read walks the transcript file.
pub(crate) async fn chat_view_full_text_blocking(
    state: Arc<AppState>,
    session_id: String,
    message_id: String,
) -> Result<ChatFullText, String> {
    tokio::task::spawn_blocking(move || chat_view_full_text(&state, &session_id, &message_id))
        .await
        .map_err(|e| format!("chat full text task failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(kind: &str, uuid: &str, text: &str) -> String {
        serde_json::json!({
            "type": kind, "uuid": uuid, "promptId": "p", "origin": {"kind": "human"},
            "message": {"role": kind, "content": text},
        })
        .to_string()
    }

    /// Catches: full text read from the capped copy (the stream's 20k cut).
    #[test]
    fn capped_prompt_comes_back_whole() {
        let big = "ab".repeat(40_000);
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let path = dir.path().join("t.jsonl");
        std::fs::write(
            &path,
            format!(
                "{}\n{}\n",
                row("user", "u-1", "short"),
                row("user", "u-2", &big)
            ),
        )
        .unwrap();
        // The stream view of the same row is capped.
        let mut stream = ClaudeAdapter::default();
        let capped = stream.absorb(&row("user", "u-2", &big));
        assert!(
            capped[0]["content"]["text"]
                .as_str()
                .unwrap()
                .contains("[truncated, 80000 chars total]")
        );
        let full = read_message(&path, "u-2").unwrap().unwrap();
        assert_eq!(full.chars().count(), 80_000);
        assert_eq!(full, big);
        assert!(read_message(&path, "u-9").unwrap().is_none());
    }

    /// Catches: only the first text block of a split assistant message returned.
    #[test]
    fn assistant_blocks_of_one_message_join() {
        let a = |text: &str, uuid: &str| {
            serde_json::json!({
                "type": "assistant", "uuid": uuid,
                "message": {"id": "m-1", "content": [{"type": "text", "text": text}]},
            })
            .to_string()
        };
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let path = dir.path().join("t.jsonl");
        let first = "x".repeat(30_000);
        std::fs::write(&path, format!("{}\n{}\n", a(&first, "r1"), a("tail", "r2"))).unwrap();
        let full = read_message(&path, "m-1").unwrap().unwrap();
        assert_eq!(full, format!("{first}\n\ntail"));
    }
}
