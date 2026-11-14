use super::*;

fn recorded_v5() -> Value {
    serde_json::from_str(include_str!("../../fixtures/ego-usage-v5.json")).unwrap()
}

// Catches: a new report version silently removing the model badge.
#[test]
fn ego_usage_v5_recorded_single_session_shows_model() {
    let report = recorded_v5();
    let usage = project(&report, "01a1c787-d41f-7067-9cb5-db90deb9ca8d").unwrap();
    assert_eq!(
        usage.text.as_deref(),
        Some("gpt-6.1-sol · ctx: 102135/272000")
    );
    assert!(usage.tooltip.unwrap().contains("openai-codex/gpt-6.1-sol"));
    assert_eq!(usage.usage_agent, Some("codex"));
}

// Catches: borrowing another session/model's quota or rendering ego ledger limits.
#[test]
fn ego_usage_routes_only_supported_model_providers_and_ignores_ledger_limits() {
    let mut report = recorded_v5();
    let id = "01a1c787-d41f-7067-9cb5-db90deb9ca8d";
    for (model, agent) in [
        ("anthropic/claude-sonnet-5-5", Some("claude")),
        ("openai-codex/gpt-6.1-sol", Some("codex")),
        ("openrouter/anthropic/claude-sonnet-5-5", None),
    ] {
        report["sessions"][0]["model"] = model.into();
        let usage = project(&report, id).unwrap();
        assert_eq!(usage.usage_agent, agent, "{model}");
        assert!(!usage.text.unwrap().contains("USD"));
    }
    assert!(project(&report, "another-session").unwrap().text.is_none());
}

// Catches: unsupported reports failing silently, or missing context hiding the model.
#[test]
fn ego_usage_unknown_version_errors_and_missing_context_keeps_model() {
    let mut report = recorded_v5();
    let id = "01a1c787-d41f-7067-9cb5-db90deb9ca8d";
    report["v"] = 999.into();
    assert!(
        project(&report, id)
            .unwrap_err()
            .contains("Failed to parse ego usage version")
    );
    report["v"] = 5.into();
    report["sessions"][0]["context"] = Value::Null;
    assert_eq!(
        project(&report, id).unwrap().text.as_deref(),
        Some("gpt-6.1-sol")
    );
}
