//! Improvement proposals for GitHub Ops, written by ego.
//!
//! One unattended ego turn over a deterministic local snapshot — the working
//! tree plus the last twelve commits, assembled here rather than left for the
//! model to go and find. Two properties that are the whole point:
//!
//! * **The snapshot is ours.** The turn has no host tools (`acp::oneshot`), so
//!   what ego sees is exactly what this module put in the prompt. That makes a
//!   scan reproducible and keeps "do not invent facts outside the supplied
//!   snapshot" a rule the prompt can actually hold the model to.
//! * **It never files an issue.** Creating one is a separate command a person
//!   invokes per proposal. A scan that could open issues by itself would be an
//!   agent, which is the thing story 784 deleted.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::acp::oneshot;

const IMPROVEMENT_SCAN_SYSTEM_PROMPT: &str = "\
You are a senior software maintainer. Given a repository snapshot, propose a small \
set of high-leverage follow-up improvements. Favor concrete, reviewable work over \
speculative rewrites. Do not invent facts outside the supplied snapshot.

Respond with ONLY a JSON object, no prose and no code fences, of the form:
{\"proposals\":[{\"title\":\"...\",\"summary\":\"...\",\"rationale\":\"...\",\
\"issue_title\":\"...\",\"issue_body\":\"...\",\"labels\":[\"...\"],\"impact\":\"low|medium|high\",\
\"effort\":\"small|medium|large\"}]}

Return at most 5 proposals. `issue_body` must be ready to paste into a GitHub issue \
and include acceptance criteria.";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ImprovementFocus {
    Refactor,
    Testing,
    Perf,
}

impl ImprovementFocus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Refactor => "refactor",
            Self::Testing => "testing",
            Self::Perf => "perf",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ImprovementProposal {
    pub title: String,
    pub summary: String,
    pub rationale: String,
    pub issue_title: String,
    pub issue_body: String,
    #[serde(default)]
    pub labels: Vec<String>,
    pub impact: String,
    pub effort: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ImprovementScanResult {
    pub repo_path: String,
    pub focus: ImprovementFocus,
    pub proposals: Vec<ImprovementProposal>,
}

#[derive(Debug, Deserialize)]
struct ProposalEnvelope {
    #[serde(default)]
    proposals: Vec<ImprovementProposal>,
}

/// Dual-emit, the same shape as `emit_review_progress` and
/// `remote_runtime::publish`: only the window emit is gated on the desktop
/// feature. `POST /repo/improvement-scan` is mounted unconditionally, and
/// nothing but this event tells a browser client the scan is over.
fn emit_proposals_ready(state: &crate::AppState, repo_path: &str, result: &ImprovementScanResult) {
    let payload = serde_json::to_value(result).unwrap_or_else(|_| serde_json::json!({}));
    #[cfg(feature = "desktop")]
    if let Some(app) = state.app_handle.read().clone() {
        use tauri::Emitter;
        let _ = app.emit(
            "proposals-ready",
            serde_json::json!({ "repo_path": repo_path, "payload": payload }),
        );
    }
    let _ = state
        .event_bus
        .send(crate::state::AppEvent::ProposalsReady {
            repo_path: repo_path.to_string(),
            payload,
        });
}

/// Read ego's answer into proposals worth showing.
///
/// Strict where the changelog is lenient, and for a reason: a changelog that
/// came back as prose is still a changelog, while a proposal that lost its
/// `issue_body` is a card with a button that would file an empty issue. An
/// incomplete proposal is dropped rather than repaired.
pub(crate) fn parse_improvement_output(raw: &str) -> Result<Vec<ImprovementProposal>, String> {
    let candidate = oneshot::extract_json(raw);
    let envelope: ProposalEnvelope = serde_json::from_str(candidate)
        .map_err(|e| oneshot::unparseable("proposals", e, candidate))?;
    Ok(envelope
        .proposals
        .into_iter()
        .filter(|p| {
            !p.title.trim().is_empty()
                && !p.issue_title.trim().is_empty()
                && !p.issue_body.trim().is_empty()
        })
        .take(5)
        .collect())
}

fn trim_line(s: &str, max_chars: usize) -> String {
    let trimmed = s.trim();
    let mut out: String = trimmed.chars().take(max_chars).collect();
    if trimmed.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

pub(crate) fn build_improvement_prompt(
    repo_path: &str,
    focus: ImprovementFocus,
    status: &crate::git::WorkingTreeStatus,
    commits: &[crate::git::CommitLogEntry],
) -> String {
    let repo_name = Path::new(repo_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(repo_path);
    let mut out = format!(
        "Repository: {repo_name}\nPath: {repo_path}\nFocus: {}\n\nWorking tree:\n",
        focus.as_str()
    );
    out.push_str(&format!(
        "- branch: {}\n- upstream: {}\n- ahead/behind: {}/{}\n- stash_count: {}\n",
        status.branch.as_deref().unwrap_or("(detached/unknown)"),
        status.upstream.as_deref().unwrap_or("(none)"),
        status.ahead,
        status.behind,
        status.stash_count,
    ));
    out.push_str(&format!(
        "- staged files: {}\n- unstaged files: {}\n- untracked files: {}\n",
        status.staged.len(),
        status.unstaged.len(),
        status.untracked.len()
    ));

    let changed: Vec<String> = status
        .staged
        .iter()
        .chain(status.unstaged.iter())
        .take(25)
        .map(|e| {
            format!(
                "{} {} (+{} -{})",
                e.status, e.path, e.additions, e.deletions
            )
        })
        .collect();
    if !changed.is_empty() {
        out.push_str("\nChanged files (first 25):\n");
        for line in changed {
            out.push_str("- ");
            out.push_str(&trim_line(&line, 180));
            out.push('\n');
        }
    }

    if !status.untracked.is_empty() {
        out.push_str("\nUntracked files (first 15):\n");
        for path in status.untracked.iter().take(15) {
            out.push_str("- ");
            out.push_str(&trim_line(path, 180));
            out.push('\n');
        }
    }

    out.push_str("\nRecent commits:\n");
    for commit in commits.iter().take(12) {
        let short_hash: String = commit.hash.chars().take(8).collect();
        out.push_str(&format!(
            "- {} {} ({})\n",
            short_hash,
            trim_line(&commit.subject, 180),
            commit.author_date
        ));
    }
    out
}

pub(crate) fn proposal_issue_text(proposal: &ImprovementProposal) -> (String, String) {
    let mut body = proposal.issue_body.trim().to_string();
    if !proposal.summary.trim().is_empty() && !body.contains(proposal.summary.trim()) {
        body.push_str("\n\nSummary:\n");
        body.push_str(proposal.summary.trim());
    }
    if !proposal.rationale.trim().is_empty() && !body.contains(proposal.rationale.trim()) {
        body.push_str("\n\nRationale:\n");
        body.push_str(proposal.rationale.trim());
    }
    (proposal.issue_title.trim().to_string(), body)
}

pub(crate) async fn run_improvement_scan_impl(
    repo_path: String,
    focus: ImprovementFocus,
    state: &std::sync::Arc<crate::AppState>,
) -> Result<ImprovementScanResult, String> {
    let status = crate::git::get_working_tree_status(repo_path.clone()).await?;
    let commits = tokio::task::spawn_blocking({
        let repo_path = repo_path.clone();
        move || crate::git::get_commit_log_impl(repo_path, Some(12), None)
    })
    .await
    .map_err(|e| format!("commit log task failed: {e}"))??;
    let prompt = build_improvement_prompt(&repo_path, focus, &status, &commits);
    let raw = oneshot::ask(
        state,
        std::path::PathBuf::from(&repo_path),
        IMPROVEMENT_SCAN_SYSTEM_PROMPT,
        prompt,
    )
    .await?;
    let proposals = parse_improvement_output(&raw)?;
    let result = ImprovementScanResult {
        repo_path: repo_path.clone(),
        focus,
        proposals,
    };
    emit_proposals_ready(state, &repo_path, &result);
    Ok(result)
}

pub(crate) async fn create_issue_from_proposal_impl(
    repo_path: &str,
    proposal: &ImprovementProposal,
    state: &crate::AppState,
) -> Result<crate::github::CreatedIssue, String> {
    let (title, body) = proposal_issue_text(proposal);
    crate::github::create_issue_impl(repo_path, &title, &body, state).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn run_improvement_scan(
    repo_path: String,
    focus: ImprovementFocus,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<ImprovementScanResult, String> {
    let state = state.inner().clone();
    run_improvement_scan_impl(repo_path, focus, &state).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn create_issue_from_proposal(
    repo_path: String,
    proposal: ImprovementProposal,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<crate::github::CreatedIssue, String> {
    let state = state.inner().clone();
    create_issue_from_proposal_impl(&repo_path, &proposal, &state).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same rule as `emit_review_progress`: `POST /repo/improvement-scan`
    /// is mounted unconditionally, so the scan a browser starts against a
    /// headless build has to announce its proposals on the bus. Nothing else
    /// tells that client the scan is over.
    #[test]
    fn proposals_ready_reaches_the_bus_on_every_build() {
        let state = crate::state::tests_support::make_test_app_state();
        let mut events = state.event_bus.subscribe();
        let result = ImprovementScanResult {
            repo_path: "/repo".to_string(),
            focus: ImprovementFocus::Refactor,
            proposals: Vec::new(),
        };

        emit_proposals_ready(&state, "/repo", &result);

        match events
            .try_recv()
            .expect("the finished scan reaches the event bus")
        {
            crate::state::AppEvent::ProposalsReady { repo_path, payload } => {
                assert_eq!(repo_path, "/repo");
                assert_eq!(payload["repo_path"], "/repo");
            }
            other => panic!("expected ProposalsReady, got {other:?}"),
        }
    }

    fn sample_status() -> crate::git::WorkingTreeStatus {
        crate::git::WorkingTreeStatus {
            branch: Some("main".into()),
            upstream: Some("origin/main".into()),
            ahead: 1,
            behind: 0,
            stash_count: 2,
            staged: vec![crate::git::StatusEntry {
                path: "src/lib.rs".into(),
                status: "M".into(),
                original_path: None,
                additions: 10,
                deletions: 2,
            }],
            unstaged: vec![],
            untracked: vec!["notes/spike.md".into()],
            conflicted: vec![],
        }
    }

    fn sample_commit() -> crate::git::CommitLogEntry {
        crate::git::CommitLogEntry {
            hash: "abcdef1234567890".into(),
            parents: vec![],
            refs: vec![],
            author_name: "Dev".into(),
            author_date: "2026-07-06T10:00:00Z".into(),
            subject: "Improve tests".into(),
            body: None,
        }
    }

    #[test]
    fn prompt_includes_focus_status_and_commits() {
        let prompt = build_improvement_prompt(
            "/tmp/repo",
            ImprovementFocus::Testing,
            &sample_status(),
            &[sample_commit()],
        );
        assert!(prompt.contains("Focus: testing"));
        assert!(prompt.contains("branch: main"));
        assert!(prompt.contains("M src/lib.rs (+10 -2)"));
        assert!(prompt.contains("Improve tests"));
    }

    #[test]
    fn parse_output_strips_fences_and_filters_incomplete_items() {
        let raw = "```json\n{\"proposals\":[{\"title\":\"T\",\"summary\":\"S\",\"rationale\":\"R\",\"issue_title\":\"I\",\"issue_body\":\"B\",\"labels\":[\"tech-debt\"],\"impact\":\"medium\",\"effort\":\"small\"},{\"title\":\"drop\",\"summary\":\"\",\"rationale\":\"\",\"issue_title\":\"\",\"issue_body\":\"\",\"impact\":\"low\",\"effort\":\"small\"}]}\n```";
        let proposals = parse_improvement_output(raw).unwrap();
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].issue_title, "I");
        assert_eq!(proposals[0].labels, vec!["tech-debt"]);
    }

    // An unusable answer must carry what ego actually said. "Failed to parse"
    // on its own sends a person to the logs for something the toast already had.
    #[test]
    fn parse_output_errors_on_unparseable_json_and_quotes_the_answer() {
        let err = parse_improvement_output("not json at all").unwrap_err();
        assert!(err.contains("proposals"), "{err}");
        assert!(err.contains("not json at all"), "{err}");
    }

    // Five is the prompt's limit; a model that ignores it must not be able to
    // push an unbounded list into the dashboard.
    #[test]
    fn no_more_than_five_proposals_survive_however_many_arrive() {
        let one = r#"{"title":"T","summary":"S","rationale":"R","issue_title":"I","issue_body":"B","impact":"low","effort":"small"}"#;
        let raw = format!("{{\"proposals\":[{}]}}", [one; 9].join(","));

        assert_eq!(parse_improvement_output(&raw).unwrap().len(), 5);
    }

    // An envelope with no proposals at all is an answer, not a failure: the
    // model looked and found nothing worth proposing.
    #[test]
    fn an_empty_proposal_list_is_not_an_error() {
        assert_eq!(
            parse_improvement_output(r#"{"proposals":[]}"#).unwrap(),
            [] as [crate::improvement_scan::ImprovementProposal; 0]
        );
    }

    #[test]
    fn proposal_issue_text_appends_summary_and_rationale() {
        let proposal = ImprovementProposal {
            title: "T".into(),
            summary: "Short summary".into(),
            rationale: "Why it matters".into(),
            issue_title: "Issue title".into(),
            issue_body: "Acceptance:\n- done".into(),
            labels: vec![],
            impact: "high".into(),
            effort: "medium".into(),
        };
        let (title, body) = proposal_issue_text(&proposal);
        assert_eq!(title, "Issue title");
        assert!(body.contains("Acceptance:"));
        assert!(body.contains("Short summary"));
        assert!(body.contains("Why it matters"));
    }
}
