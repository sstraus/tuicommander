use super::neutral_trigger_fixture::fixture;
use std::path::Path;

fn repository(root: &Path, name: &str, remote: &str) -> String {
    let path = root.join(name);
    std::fs::create_dir_all(&path).unwrap();
    for args in [
        vec!["init", "--quiet"],
        vec!["remote", "add", "origin", remote],
    ] {
        let result = std::process::Command::new("git")
            .arg("-C")
            .arg(&path)
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    path.to_string_lossy().into_owned()
}
// Catches: a user-enabled neutral issue binding receiving no intake callback;
// also guards repository scope.
#[tokio::test]
async fn enabled_neutral_issue_binding_receives_issue() {
    let f = fixture();
    let binding = f.enable_without_baseline(f.draft.clone(), 0);
    rusqlite::Connection::open(f._dir.path().join("definitions.sqlite3"))
        .unwrap()
        .execute(
            "VACUUM INTO ?1",
            [f._dir.path().join("workflows.sqlite3").to_str().unwrap()],
        )
        .unwrap();
    let _config = crate::config::set_config_dir_override(f._dir.path().to_path_buf());
    let workflows = crate::workflows::WorkflowStore::open().unwrap();
    assert_eq!(
        crate::github::issue_intake_poll_sources()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["octo-org/example"]
    );
    assert!(
        workflows.neutral_trigger_bindings("/project").unwrap()[0]
            .refusal_reason
            .is_none()
    );
    let wanted = repository(
        f._dir.path(),
        "wanted",
        "https://github.com/octo-org/example.git",
    );
    let unrequested = repository(
        f._dir.path(),
        "unrequested",
        "https://github.com/octo-org/example-docs.git",
    );
    let mut http = crate::github::partial_tests::PollHttpFixture::start(true).await;
    let result = crate::github::get_all_batch_with_intake_impl(
        &[wanted.clone(), unrequested.clone()],
        false,
        "disabled",
        false,
        &http.state,
        true,
    )
    .await
    .unwrap();
    assert_eq!(
        result.intake_issues[&wanted].len(),
        1,
        "enabled neutral binding received no issue"
    );
    assert!(!result.intake_issues.contains_key(&unrequested));
    let mut intake_queries = Vec::new();
    while let Ok(request) = http.requests.try_recv() {
        let query = request["query"].as_str().unwrap();
        if query.contains("intakeIssues:") {
            intake_queries.push(query.to_owned());
        }
    }
    assert_eq!(intake_queries.len(), 1);
    assert!(
        intake_queries[0].contains("name: \"example\""),
        "{}",
        intake_queries[0]
    );
    assert!(!intake_queries[0].contains("example-docs"));
    assert_eq!(binding.draft.resource_prefix, "github:octo-org/example/");
    http.close().await;
}
// Catches: enabling the neutral source predicate globally fetching issues with
// no explicit binding/setting request. A disabled binding must not subscribe.
#[tokio::test]
async fn no_enabled_issue_source_does_not_fetch_or_start_intake() {
    let f = fixture();
    f.workflows
        .save_neutral_trigger_binding("/project", 0, f.draft.clone(), &f.catalog, &f.handlers)
        .unwrap();
    rusqlite::Connection::open(f._dir.path().join("definitions.sqlite3"))
        .unwrap()
        .execute(
            "VACUUM INTO ?1",
            [f._dir.path().join("workflows.sqlite3").to_str().unwrap()],
        )
        .unwrap();
    let _config = crate::config::set_config_dir_override(f._dir.path().to_path_buf());
    assert!(
        crate::github::issue_intake_poll_sources()
            .unwrap()
            .is_empty()
    );
    let wanted = repository(
        f._dir.path(),
        "wanted",
        "https://github.com/octo-org/example.git",
    );
    let mut http = crate::github::partial_tests::PollHttpFixture::start(true).await;
    crate::workflows::github_poll::refresh(&http.state).unwrap();
    assert!(
        http.state.github.poller.lock().is_none(),
        "no enabled source may start a poller"
    );
    assert!(http.requests.try_recv().is_err());
    // An independent PR/sidebar request may still poll, but cannot fetch issues.
    let result = crate::github::get_all_batch_with_intake_impl(
        &[wanted],
        false,
        "disabled",
        false,
        &http.state,
        true,
    )
    .await
    .unwrap();
    assert!(result.intake_issues.is_empty());
    while let Ok(request) = http.requests.try_recv() {
        assert!(!request["query"].as_str().unwrap().contains("intakeIssues:"));
    }
    http.close().await;
}
