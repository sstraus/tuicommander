//! An AI review of one pull request, run by ego.
//!
//! What came back from story 784 and what deliberately did not.
//!
//! **Back:** the findings, the confidence gate, and the posting flow. A finding
//! is still `{path, line, severity, message, confidence}`, still filtered by
//! [`finding_confidence_threshold`] before anything renders, and still posted
//! through `github::post_pr_review` by the popover. Those three are what
//! criterion 1 of story 795-320b names, and they are the contract.
//!
//! **Not back:** the multi-turn engine. The deleted version held a session per
//! repo for 300 seconds, spent one LLM turn on an overview and one per file, and
//! let the model call `read_file` between them. That was the embedded agent, and
//! it is the thing 784 deleted. This is one unattended ego turn over the whole
//! diff (`acp::oneshot`), which is why there is no session cache, no per-file
//! phase and no tool dispatch here.
//!
//! **Also not back:** `llm_used` and `llm_model`. Which model ran is ego's
//! configuration, not a property of this call, and TUICommander is not told. A
//! field that could only ever be reported as `null` is worse than no field.
//!
//! The diff goes in whole rather than being fetched by the model: the turn has
//! no host tools, so what ego sees is what this module sends, which is the same
//! reason `improvement_scan` assembles its own snapshot.

use serde::Deserialize;

use crate::acp::oneshot;

pub(crate) use tuic_git::pr_review::*;

/// The envelope ego is asked for.
#[derive(Debug, Deserialize)]
struct ReviewEnvelope {
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    files: Vec<ReviewedFile>,
}

/// Read ego's answer, then apply the gate.
///
/// The gate runs here and not in the UI on purpose: a finding below the
/// threshold must never reach a client, because the posting flow posts what it
/// was given.
pub(crate) fn parse_review_output(raw: &str, threshold: f32) -> Result<ReviewEnvelopeOut, String> {
    let candidate = oneshot::extract_json(raw);
    let envelope: ReviewEnvelope = serde_json::from_str(candidate)
        .map_err(|e| oneshot::unparseable("review", e, candidate))?;

    let files = envelope
        .files
        .into_iter()
        .map(|file| ReviewedFile {
            findings: filter_findings_by_confidence(&file.findings, threshold),
            ..file
        })
        .collect();

    Ok(ReviewEnvelopeOut {
        summary: envelope.summary.filter(|s| !s.trim().is_empty()),
        files,
    })
}

/// A parsed, gated review, before it is given its PR identity.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ReviewEnvelopeOut {
    pub summary: Option<String>,
    pub files: Vec<ReviewedFile>,
}

/// Dual-emit, in the shape `remote_runtime::publish` uses: the window emit is
/// the desktop's half and is gated, the bus send is everyone's and is not.
///
/// `POST /repo/pr-review` is mounted unconditionally, so the headless build
/// answers it as well — and its clients are browsers reading `/events`. Gating
/// the whole function left them with a review column stuck on "running".
fn emit_review_progress(state: &crate::AppState, repo_path: &str, payload: serde_json::Value) {
    #[cfg(feature = "desktop")]
    if let Some(app) = state.app_handle.read().clone() {
        use tauri::Emitter;
        let _ = app.emit(
            "review-progress",
            serde_json::json!({ "repo_path": repo_path, "payload": payload }),
        );
    }
    let _ = state
        .event_bus
        .send(crate::state::AppEvent::ReviewProgress {
            repo_path: repo_path.to_string(),
            payload,
        });
}

pub(crate) async fn run_pr_review_impl(
    repo_path: String,
    pr_number: i64,
    state: &std::sync::Arc<crate::AppState>,
) -> Result<PrReviewResult, String> {
    let diff = crate::github::get_pr_diff_impl(&repo_path, pr_number, state).await?;
    let head_sha = format!("{:016x}", hash_diff(&diff));
    let files = split_unified_diff(&diff);

    // An empty diff is an answer, and it is not one ego has to be started for.
    if files.is_empty() {
        return Ok(PrReviewResult {
            repo_path,
            pr_number,
            head_sha,
            summary: Some("This pull request changes no files.".to_string()),
            files: Vec::new(),
        });
    }

    emit_review_progress(
        state,
        &repo_path,
        serde_json::json!({ "pr_number": pr_number, "done": false, "findings_count": 0 }),
    );

    let answer = oneshot::ask(
        state,
        std::path::PathBuf::from(&repo_path),
        PR_REVIEW_SYSTEM_PROMPT,
        build_review_prompt(&files),
    )
    .await;

    let parsed =
        match answer.and_then(|raw| parse_review_output(&raw, finding_confidence_threshold())) {
            Ok(parsed) => parsed,
            Err(error) => {
                // The column must not be left saying "running" forever because the
                // turn failed; the reason travels with the end of the review.
                emit_review_progress(
                    state,
                    &repo_path,
                    serde_json::json!({ "pr_number": pr_number, "done": true, "error": error }),
                );
                return Err(error);
            }
        };

    let findings_count: usize = parsed.files.iter().map(|f| f.findings.len()).sum();
    emit_review_progress(
        state,
        &repo_path,
        serde_json::json!({
            "pr_number": pr_number,
            "done": true,
            "findings_count": findings_count,
        }),
    );

    Ok(PrReviewResult {
        repo_path,
        pr_number,
        head_sha,
        summary: parsed.summary,
        files: parsed.files,
    })
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn run_pr_review(
    repo_path: String,
    pr_number: i64,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<PrReviewResult, String> {
    let state = state.inner().clone();
    run_pr_review_impl(repo_path, pr_number, &state).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `POST /repo/pr-review` is mounted unconditionally, so the headless build
    /// answers it too — and every client of that build is a browser reading
    /// `/events`. The window emit is the desktop's half of the dual emit; the
    /// bus send is everybody's. Gating the bus send along with it left a review
    /// started from a browser with a column stuck on "running" and no way to
    /// learn it had finished.
    #[test]
    fn review_progress_reaches_the_bus_on_every_build() {
        let state = crate::state::tests_support::make_test_app_state();
        let mut events = state.event_bus.subscribe();

        emit_review_progress(
            &state,
            "/repo",
            serde_json::json!({ "pr_number": 7, "done": false }),
        );

        match events
            .try_recv()
            .expect("the review's progress reaches the event bus")
        {
            crate::state::AppEvent::ReviewProgress { repo_path, payload } => {
                assert_eq!(repo_path, "/repo");
                assert_eq!(payload["pr_number"], 7);
            }
            other => panic!("expected ReviewProgress, got {other:?}"),
        }
    }

    #[test]
    fn the_gate_runs_before_anything_leaves_this_module() {
        let raw = r#"{"summary":"s","files":[{"path":"src/a.rs","summary":"f",
            "findings":[
              {"path":"src/a.rs","line":1,"severity":"bug","message":"real","confidence":0.95},
              {"path":"src/a.rs","line":2,"severity":"nit","message":"guess","confidence":0.2}
            ]}]}"#;
        let parsed = parse_review_output(raw, 0.7).unwrap();

        assert_eq!(parsed.files[0].findings.len(), 1);
        assert_eq!(parsed.files[0].findings[0].message, "real");
    }

    #[test]
    fn a_review_that_found_nothing_is_a_result_and_not_a_failure() {
        let parsed = parse_review_output(r#"{"summary":"all fine","files":[]}"#, 0.7).unwrap();

        assert_eq!(parsed.summary.as_deref(), Some("all fine"));
        assert_eq!(parsed.files, [] as [tuic_git::pr_review::ReviewedFile; 0]);
    }

    #[test]
    fn a_blank_summary_is_no_summary() {
        let parsed = parse_review_output(r#"{"summary":"   ","files":[]}"#, 0.7).unwrap();
        assert!(parsed.summary.is_none());
    }

    #[test]
    fn an_unparseable_review_says_what_ego_actually_said() {
        let err = parse_review_output("I am not going to do that", 0.7).unwrap_err();

        assert!(err.contains("review"), "{err}");
        assert!(err.contains("I am not going to do that"), "{err}");
    }

    #[test]
    fn a_fenced_answer_is_read_like_any_other() {
        let parsed =
            parse_review_output("```json\n{\"summary\":\"s\",\"files\":[]}\n```", 0.7).unwrap();
        assert_eq!(parsed.summary.as_deref(), Some("s"));
    }
}
