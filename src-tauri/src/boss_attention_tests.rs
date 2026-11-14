use super::*;
use crate::stories::{NewPlan, NewStory, StoryAction, StoryCommand, StoryOrigin};

// Catches: a missing live stage creates a warning solely from a managed identity.
#[test]
fn boss_attention_health_absent_stage_is_not_a_problem() {
    let rows = [None, Some("managed")]
        .into_iter()
        .map(|identity| SessionInfo {
            session_id: "worker".into(),
            tuic_session: identity.map(str::to_owned),
            ..Default::default()
        })
        .collect();
    let mut result = BossAttention::default();
    add_health(&mut result, rows);
    assert!(result.health.is_empty(), "{:?}", result.health);
}

// Catches: removing the absent-stage warning also hides explicit failures or problem stages.
#[test]
fn boss_attention_health_keeps_real_problems_visible() {
    let base = SessionInfo {
        tuic_session: Some("managed".into()),
        ..Default::default()
    };
    let mut failed_report = base.clone();
    failed_report.agent_report.report = Some(crate::agent_report::AgentReport {
        state: ReportState::Failed,
        wait_for: None,
        question: None,
    });
    let mut stalled = base.clone();
    stalled.agent_report.stalled = true;
    let mut failed_state = base.clone();
    failed_state.state = Some(crate::state::SessionState {
        last_error: Some("Process failed".into()),
        ..Default::default()
    });
    let mut input_stalled = base.clone();
    input_stalled.agent_report.stage = Some("undelivered_input".into());
    let mut blocked = base;
    blocked.agent_report.stage = Some("blocked".into());
    let cases = [
        (
            "report",
            failed_report,
            "Failed",
            "The agent reported a failure.",
        ),
        (
            "stall",
            stalled,
            "Stalled",
            "The owning host flagged no report, input or output within its configured silence window.",
        ),
        ("error", failed_state, "Failed", "Process failed"),
        (
            "input",
            input_stalled,
            "Input stalled",
            "Input is pending, failed or delivery is uncertain.",
        ),
        (
            "blocked",
            blocked,
            "Blocked",
            "The agent is waiting for a dependency or recovery.",
        ),
    ];
    let mut result = BossAttention::default();
    let rows = cases
        .iter()
        .map(|(id, row, _, _)| {
            let mut row = row.clone();
            row.session_id = (*id).into();
            row
        })
        .collect();
    add_health(&mut result, rows);
    let actual: Vec<_> = result
        .health
        .iter()
        .map(|health| {
            (
                health.session_id.as_str(),
                health.kind.as_str(),
                health.reason.as_str(),
            )
        })
        .collect();
    let expected: Vec<_> = cases
        .iter()
        .map(|(id, _, kind, reason)| (*id, *kind, *reason))
        .collect();
    assert_eq!(actual, expected);
}

// Catches: the consumer ignores the host's explicit stalled flag or invents a
// stall from a working stage with no such flag.
#[test]
fn boss_attention_health_uses_the_host_stalled_flag() {
    let mut row = SessionInfo {
        session_id: "worker".into(),
        ..Default::default()
    };
    row.agent_report.stage = Some("working".into());
    row.agent_report.stalled = true;
    let mut result = BossAttention::default();
    add_health(&mut result, vec![row.clone()]);
    assert_eq!(result.health[0].kind, "Stalled");
    row.agent_report.stalled = false;
    let mut current = BossAttention::default();
    add_health(&mut current, vec![row]);
    assert!(current.health.is_empty());
}

// Catches: native manual checks never reach the decision list, or a cancelled
// story remains there because the projection retained a previous snapshot.
#[test]
fn boss_attention_native_story_qa_is_current_not_cached() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().join("config"));
    let project = dir
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let store = StoryStore::open().unwrap();
    let plan = store
        .create_plan(NewPlan {
            project: project.clone(),
            title: "Release".into(),
            source: "release.md".into(),
        })
        .unwrap();
    let plan_id = plan.id;
    let story = store
        .create_story(NewStory {
            story_type: None,
            plan_id: plan_id.clone(),
            title: "Check the native view".into(),
            criteria: vec!["[MANUAL] Verify rendering".into()],
            priority: 2,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap();
    let change = |command| {
        let current = store.get_story(&story.id).unwrap();
        crate::stories::story_action(
            &project,
            StoryAction::Transition {
                story_id: story.id.clone(),
                expected_revision: current.revision,
                command,
            },
            None,
        )
        .unwrap();
    };
    change(StoryCommand::StartManual);
    assert_eq!(store.get_story(&story.id).unwrap().status, StoryStatus::Qa);
    let read = || {
        let mut result = BossAttention::default();
        add_project(&mut result, &store, &project).unwrap();
        result
    };
    let result = read();
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].story.as_ref().unwrap().id, story.id);
    assert_eq!(result.items[0].title, "Check the native view");
    assert_eq!(result.items[0].kind, AttentionKind::ManualCheck);
    assert_eq!(result.items[0].plan_id.as_deref(), Some(plan_id.as_str()));
    change(StoryCommand::WontFix);
    assert_eq!(read().items, [] as [AttentionItem; 0]);
}
