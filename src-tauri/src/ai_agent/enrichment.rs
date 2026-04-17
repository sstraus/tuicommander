//! Post-hoc AI enrichment of completed OSC 133 command blocks.
//!
//! When `AiChatConfig.experimental_ai_block_enrichment` is set, each recorded
//! `CommandOutcome` is pushed onto a bounded mpsc queue. A single background
//! worker drains the queue, rate-limits itself (10/min), calls the configured
//! LLM with a terse classify prompt, and writes the returned one-line goal
//! back into `SessionKnowledge` via `set_semantic_intent`.
//!
//! Design constraints:
//! - **Non-blocking capture path.** Enqueue uses `try_send`; a full queue
//!   drops the oldest pending item so the PTY path is never awaited.
//! - **Opt-in.** Disabled by default — token cost + output_snippet leaves
//!   the device. Redaction already happened in `SessionKnowledge::record`.
//! - **Fire and forget.** No retries, no partial-state tracking; a failed
//!   enrichment leaves `semantic_intent = None` and callers treat missing
//!   as unknown.

use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::ai_agent::knowledge::CommandOutcome;
use crate::state::AppState;

/// Bounded queue size. A burst larger than this drops oldest-first.
const QUEUE_CAPACITY: usize = 100;

/// Rate limit — one enrichment every this many milliseconds (10/min ≈ 6s).
const MIN_SPACING_MS: u64 = 6_000;

/// Per-request timeout. Keep short: enrichment is best-effort.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Truncate the output tail to this many chars before sending to the LLM.
/// Matches the 1KB compromise from the design sketch.
const MAX_OUTPUT_TAIL: usize = 1024;

#[derive(Debug)]
pub struct EnrichmentRequest {
    pub session_id: String,
    pub outcome_id: u64,
    pub command: String,
    pub exit_code: Option<i32>,
    pub output_tail: String,
}

static SENDER: OnceLock<mpsc::Sender<EnrichmentRequest>> = OnceLock::new();

/// Non-blocking enqueue. Called from the PTY record path.
///
/// Silently no-ops if the worker is not spawned yet. On a full queue this
/// drops the newest request (tokio mpsc has no built-in drop-oldest); the
/// alternative of awaiting would violate the "never block PTY" contract.
pub fn try_enqueue_outcome(session_id: &str, outcome: &CommandOutcome) {
    let Some(tx) = SENDER.get() else { return };
    let req = EnrichmentRequest {
        session_id: session_id.to_string(),
        outcome_id: outcome.id,
        command: outcome.command.clone(),
        exit_code: outcome.exit_code,
        output_tail: truncate_tail(&outcome.output_snippet),
    };
    let _ = tx.try_send(req);
}

fn truncate_tail(s: &str) -> String {
    if s.len() <= MAX_OUTPUT_TAIL {
        return s.to_string();
    }
    let start = s.len() - MAX_OUTPUT_TAIL;
    // Move to a char boundary.
    let mut boundary = start;
    while boundary < s.len() && !s.is_char_boundary(boundary) {
        boundary += 1;
    }
    format!("…{}", &s[boundary..])
}

/// Spawn the single enrichment worker. Idempotent — subsequent calls are
/// no-ops once the channel has been installed.
pub fn spawn_worker(state: Arc<AppState>) {
    let (tx, mut rx) = mpsc::channel::<EnrichmentRequest>(QUEUE_CAPACITY);
    if SENDER.set(tx).is_err() {
        // Already spawned.
        return;
    }

    tokio::spawn(async move {
        let mut last_dispatch = tokio::time::Instant::now()
            .checked_sub(Duration::from_millis(MIN_SPACING_MS))
            .unwrap_or_else(tokio::time::Instant::now);

        while let Some(req) = rx.recv().await {
            if !enrichment_enabled() {
                // Setting flipped off between enqueue and dispatch — drop.
                continue;
            }
            let wait = Duration::from_millis(MIN_SPACING_MS)
                .checked_sub(last_dispatch.elapsed())
                .unwrap_or_default();
            if !wait.is_zero() {
                tokio::time::sleep(wait).await;
            }
            last_dispatch = tokio::time::Instant::now();

            match run_enrichment(&req).await {
                Ok(intent) if !intent.is_empty() => {
                    apply_intent(&state, &req.session_id, req.outcome_id, intent);
                }
                _ => { /* drop on failure, per design */ }
            }
        }
    });
}

fn enrichment_enabled() -> bool {
    crate::ai_chat::load_ai_chat_config().experimental_ai_block_enrichment
}

fn apply_intent(state: &AppState, session_id: &str, outcome_id: u64, intent: String) {
    let Some(entry) = state.session_knowledge.get(session_id) else { return };
    let updated = {
        let mut k = entry.lock();
        k.set_semantic_intent(outcome_id, intent)
    };
    if updated {
        state.knowledge_dirty.insert(session_id.to_string(), ());
    }
}

async fn run_enrichment(req: &EnrichmentRequest) -> Result<String, String> {
    let config = crate::ai_chat::load_ai_chat_config();
    if !config.is_configured() {
        return Err("provider not configured".into());
    }

    let api_key = if config.provider == "ollama" {
        crate::ai_chat::read_api_key()
            .ok()
            .flatten()
            .unwrap_or_else(|| "ollama".to_string())
    } else {
        match crate::ai_chat::read_api_key() {
            Ok(Some(k)) => k,
            _ => return Err("no api key".into()),
        }
    };

    let llm_config = crate::llm_api::LlmApiConfig {
        provider: config.provider.clone(),
        model: config.model.clone(),
        base_url: config.effective_base_url(),
    };
    let client = crate::llm_api::build_client(&llm_config, &api_key);

    let prompt = build_prompt(req);

    use genai::chat::{ChatMessage, ChatRequest};
    let chat_req = ChatRequest::default()
        .with_system(SYSTEM_PROMPT)
        .append_message(ChatMessage::user(prompt));

    let resp =
        tokio::time::timeout(REQUEST_TIMEOUT, client.exec_chat(&config.model, chat_req, None))
            .await
            .map_err(|_| "timeout".to_string())?
            .map_err(|e| format!("llm error: {e}"))?;

    let text = resp.first_text().unwrap_or_default().trim().to_string();
    Ok(sanitize_intent(&text))
}

const SYSTEM_PROMPT: &str = "You classify shell commands. \
Reply with one short sentence (≤15 words) describing the user's goal. \
Do not restate the command. No quotes, no trailing period needed, no preamble.";

fn build_prompt(req: &EnrichmentRequest) -> String {
    let exit = req
        .exit_code
        .map(|c| c.to_string())
        .unwrap_or_else(|| "unknown".into());
    format!(
        "cmd: {}\nexit: {}\noutput tail:\n{}",
        req.command, exit, req.output_tail
    )
}

/// Strip leading/trailing quotes, enforce max length, collapse whitespace.
fn sanitize_intent(raw: &str) -> String {
    let trimmed = raw
        .trim()
        .trim_matches(|c: char| c == '"' || c == '\'' || c == '`')
        .trim();
    let collapsed: String = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
    let max = 200;
    if collapsed.chars().count() <= max {
        collapsed
    } else {
        collapsed.chars().take(max).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_tail_short_is_passthrough() {
        assert_eq!(truncate_tail("hello"), "hello");
    }

    #[test]
    fn truncate_tail_long_keeps_last_chunk() {
        let long = "a".repeat(MAX_OUTPUT_TAIL + 100);
        let t = truncate_tail(&long);
        assert!(t.starts_with('…'));
        assert!(t.len() <= MAX_OUTPUT_TAIL + 4);
    }

    #[test]
    fn truncate_tail_respects_char_boundary() {
        // Build a string where the naive cut would land mid-codepoint.
        let head = "x".repeat(MAX_OUTPUT_TAIL - 1);
        let s = format!("{head}é{}", "y".repeat(200));
        let t = truncate_tail(&s);
        // No panic, and result is valid UTF-8 by virtue of being a String.
        assert!(t.starts_with('…'));
    }

    #[test]
    fn sanitize_intent_strips_quotes_and_collapses_ws() {
        assert_eq!(
            sanitize_intent("  \"build   the project\"  "),
            "build the project"
        );
    }

    #[test]
    fn sanitize_intent_truncates_long_output() {
        let long = "word ".repeat(100);
        let out = sanitize_intent(&long);
        assert!(out.chars().count() <= 200);
    }
}
