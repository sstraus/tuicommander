use super::*;
use crate::automations::{
    model::{AutomationDefinition, EventIntake},
    store::RunOwner,
};
use tuic_git::github::parse_issue_node;

const SANDBOX: &str = "sstraus/tuic-test-sandbox";

/// Poller response shape: the GraphQL issue node `parse_issue_node` consumes
/// (same keys as `github.rs::test_parse_issue_node_full`).
fn issue(number: i64, repository: &str, labels: &[&str], state: &str) -> GitHubIssue {
    parse_issue_node(&serde_json::json!({
        "number": number, "title": "Add a greeting", "state": state,
        "url": format!("https://github.com/{repository}/issues/{number}"),
        "createdAt": "2026-10-23T08:00:00Z", "updatedAt": "2026-10-23T08:00:00Z",
        "author": {"login": "sstraus"},
        "labels": {"nodes": labels.iter().map(|l| serde_json::json!({"name": l, "color": "d73a49"})).collect::<Vec<_>>()},
        "assignees": {"nodes": []}, "milestone": null, "comments": {"totalCount": 0}
    }))
    .unwrap()
}

fn config(repositories: &[&str]) -> AutomationsConfig {
    let definition: AutomationDefinition = serde_json::from_value(serde_json::json!({
        "id":"pilot","name":"Issue to draft PR","prompt":"Resolve the issue","run_config":"claude",
        "repository":"/project","workspace":{"mode":"new_per_run","base_branch":"main"},"cron":"0 * * * *",
        "timezone":"UTC","enabled":true,"grace_secs":1800,"overlap":"skip",
        "max_duration_secs":3600,"precheck":null
    }))
    .unwrap();
    AutomationsConfig {
        definitions: vec![definition],
        event_intake: EventIntake {
            repositories: repositories.iter().map(|r| (*r).into()).collect(),
            label: "tuic-run".into(),
            definition_id: "pilot".into(),
        },
        ..Default::default()
    }
}

fn owner() -> (tempfile::TempDir, RunOwner) {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let owner = RunOwner::acquire_at(&dir.path().join("runs.sqlite3"), 0).unwrap();
    (dir, owner)
}

// Catches an intake that ignores the label or never starts a run for a matching issue.
#[test]
fn a_labelled_issue_starts_exactly_one_run() {
    let (_dir, owner) = owner();
    let issues = [
        issue(3, SANDBOX, &["tuic-run"], "OPEN"),
        issue(4, SANDBOX, &["bug"], "OPEN"),
    ];
    let runs = admit_issues(owner.store(), &config(&[SANDBOX]), &issues, 10).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(
        runs[0].trigger,
        RunTrigger::Event {
            repository: SANDBOX.into(),
            issue_number: 3,
            label: "tuic-run".into()
        }
    );
}

// Catches a poll loop that re-admits the same issue on every poll, or after a restart.
#[test]
fn a_repeat_poll_and_a_restart_start_no_second_run() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let path = dir.path().join("runs.sqlite3");
    let issues = [issue(3, SANDBOX, &["tuic-run"], "OPEN")];
    let first = RunOwner::acquire_at(&path, 0).unwrap();
    let config = config(&[SANDBOX]);
    let run = admit_issues(first.store(), &config, &issues, 10).unwrap();
    assert_eq!(run.len(), 1);
    first
        .store()
        .transition(
            &run[0].id,
            crate::automations::run::RunStatus::Failed,
            Default::default(),
            20,
        )
        .unwrap();
    assert_eq!(
        admit_issues(first.store(), &config, &issues, 30).unwrap(),
        Vec::new()
    );
    drop(first);
    let restarted = RunOwner::acquire_at(&path, 40).unwrap();
    assert_eq!(
        admit_issues(restarted.store(), &config, &issues, 50).unwrap(),
        Vec::new()
    );
}

// Catches an allowlist bypass: another repository, a case variant lookalike host, or an empty list.
#[test]
fn a_repository_outside_the_allowlist_starts_nothing() {
    let (_dir, owner) = owner();
    let mut other_host = issue(3, SANDBOX, &["tuic-run"], "OPEN");
    other_host.url = format!("https://evil.example/{SANDBOX}/issues/3");
    let issues = [
        issue(3, "sstraus/other-repo", &["tuic-run"], "OPEN"),
        issue(5, "evil/tuic-test-sandbox", &["tuic-run"], "OPEN"),
        other_host,
    ];
    assert_eq!(
        admit_issues(owner.store(), &config(&[SANDBOX]), &issues, 10).unwrap(),
        Vec::new()
    );
    let allowed = [issue(3, SANDBOX, &["tuic-run"], "OPEN")];
    assert_eq!(
        admit_issues(owner.store(), &config(&[]), &allowed, 10).unwrap(),
        Vec::new()
    );
}

// Catches a closed issue or a differently-cased repository name changing eligibility.
#[test]
fn closed_issues_never_start_and_repository_case_is_ignored() {
    let (_dir, owner) = owner();
    let closed = [issue(3, SANDBOX, &["tuic-run"], "CLOSED")];
    assert_eq!(
        admit_issues(owner.store(), &config(&[SANDBOX]), &closed, 10).unwrap(),
        Vec::new()
    );
    let cased = [issue(6, "SStraus/TUIC-test-sandbox", &["TUIC-RUN"], "OPEN")];
    assert_eq!(
        admit_issues(owner.store(), &config(&[SANDBOX]), &cased, 10)
            .unwrap()
            .len(),
        1
    );
}

// Catches an overlap refusal that consumes the event key, losing the issue forever.
#[test]
fn an_issue_refused_for_overlap_is_admitted_by_a_later_poll() {
    let (_dir, owner) = owner();
    let config = config(&[SANDBOX]);
    let first = admit_issues(
        owner.store(),
        &config,
        &[issue(3, SANDBOX, &["tuic-run"], "OPEN")],
        10,
    )
    .unwrap();
    let both = [
        issue(3, SANDBOX, &["tuic-run"], "OPEN"),
        issue(4, SANDBOX, &["tuic-run"], "OPEN"),
    ];
    assert_eq!(
        admit_issues(owner.store(), &config, &both, 20).unwrap(),
        Vec::new()
    );
    owner
        .store()
        .transition(
            &first[0].id,
            crate::automations::run::RunStatus::Completed,
            Default::default(),
            30,
        )
        .unwrap();
    let later = admit_issues(owner.store(), &config, &both, 40).unwrap();
    assert_eq!(later.len(), 1);
    assert!(matches!(
        later[0].trigger,
        RunTrigger::Event {
            issue_number: 4,
            ..
        }
    ));
}

// Catches a retry over a stale snapshot after the bus lost the update that withdrew the label.
#[test]
fn a_snapshot_emptied_by_lost_updates_admits_nothing_until_refilled() {
    let (_dir, owner) = owner();
    let config = config(&[SANDBOX]);
    let mut snapshot = IssueSnapshot::default();
    snapshot.replace(
        "/repo".into(),
        vec![issue(3, SANDBOX, &["tuic-run"], "OPEN")],
    );
    snapshot.lost_updates();
    assert_eq!(
        admit_issues(owner.store(), &config, &snapshot.issues(), 10).unwrap(),
        Vec::new()
    );
    snapshot.replace(
        "/repo".into(),
        vec![issue(3, SANDBOX, &["tuic-run"], "OPEN")],
    );
    assert_eq!(
        admit_issues(owner.store(), &config, &snapshot.issues(), 20)
            .unwrap()
            .len(),
        1
    );
}
