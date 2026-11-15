//! Review and triage verdict types shared by native workflow operations.
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CodeVerdict {
    Pass,
    /// Actionable findings on the reviewed head.
    Findings,
    /// Conflicting or unresolved reviewer evidence.
    Uncertain,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TriageOutcome {
    Accepted,
    Rejected,
    NeedsInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TriageDecision {
    pub outcome: TriageOutcome,
    pub reason: String,
}
impl TriageDecision {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.reason.trim().is_empty() || self.reason.len() > 4000 {
            return Err("Triage reason must contain 1 to 4000 bytes".into());
        }
        Ok(())
    }
}

/// The outer review deadline drops its future; cancel the detached ACP request
/// so it still shuts down its owned connection instead of spending the full turn budget.
struct ReviewCancellation<'a> {
    state: &'a AppState,
    request_id: String,
}

impl Drop for ReviewCancellation<'_> {
    fn drop(&mut self) {
        crate::acp::oneshot::cancel(self.state, &self.request_id);
    }
}

/// The no-tool ACP code review of one immutable diff. `note` carries the
/// advisor reason of a recheck.
pub(crate) async fn acp_code_review(
    state: &Arc<AppState>,
    path: &str,
    diff: &str,
    note: Option<&str>,
) -> Result<(CodeVerdict, String, Option<String>), String> {
    let files = crate::pr_review::split_unified_diff(diff);
    if files.is_empty() {
        return Ok((
            CodeVerdict::Uncertain,
            "Empty diff: no change was reviewed or approved.".into(),
            None,
        ));
    }
    let cancellation = ReviewCancellation {
        state,
        request_id: crate::acp::oneshot::prepare(state),
    };
    let turn = crate::acp::oneshot::run_prepared(
        state,
        path.into(),
        format!(
            "{}\n\n{}{}",
            crate::pr_review::PR_REVIEW_SYSTEM_PROMPT,
            crate::pr_review::build_review_prompt(&files),
            note.map(|note| format!("\n\n{note}")).unwrap_or_default()
        ),
        cancellation.request_id.clone(),
    )
    .await
    .map_err(|error| error.message)?;
    let session_id = turn.session_id.clone();
    let raw = crate::acp::oneshot::answer_text(turn)?;
    let review = crate::pr_review::parse_review_output(
        &raw,
        crate::pr_review::finding_confidence_threshold(),
    )?;
    let code = if review.files.iter().any(|file| !file.findings.is_empty()) {
        CodeVerdict::Findings
    } else {
        CodeVerdict::Pass
    };
    let mut body = review
        .summary
        .unwrap_or_else(|| "Automated PR review".into());
    for file in review.files {
        for finding in file.findings {
            body.push_str(&format!(
                "\n\n- `{}`{}: {}",
                finding.path,
                finding
                    .line
                    .map(|line| format!(":{line}"))
                    .unwrap_or_default(),
                finding.message
            ));
        }
    }
    Ok((code, body, session_id))
}
