use super::neutral_transport_fixture::{call, source, state};
use serde_json::json;

// Catches: the public trigger action reshapes mappings/revisions, loses exact
// overlap acknowledgements or trusts caller schemas.
#[tokio::test]
async fn neutral_transport_trigger_mapping_and_overlap_revisions_share_the_authoring_contract() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let (project, source, definition) = source();
    let path = project
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let state = state();
    state.workflow_runtime.install_neutral_catalog_host(source);
    let schema = call(
        &state,
        &path,
        json!({"action":"trigger_schema","source":"github","kind":"issue"}),
    )
    .unwrap();
    assert_eq!(schema["type"], "schema");
    assert_eq!(schema["value"]["fields"]["number"]["value_type"], "integer");
    assert!(
        call(
            &state,
            &path,
            json!({"action":"trigger_schema","source":"github","kind":"invented"})
        )
        .is_err()
    );
    let workflow = call(
        &state,
        &path,
        json!({"action":"create_draft","name":"trigger target","definition":definition}),
    )
    .unwrap();
    let id = workflow["value"]["id"].as_str().unwrap();
    call(
        &state,
        &path,
        json!({"action":"publish","id":id,"expected_revision":1}),
    )
    .unwrap();
    let draft = json!({"id":"first","publication":{"project":path,"id":id,"revision":1},"source":"github","kind":"issue","resource_prefix":"github:org/repo/","events":["opened"],"filters":{},"ignored_actors":[],"mapping":{"inputs":{"value":"number"},"subject":{}},"limits":{"max_activations":20,"max_concurrency":1,"max_duration_ms":60000}});
    let saved = call(
        &state,
        &path,
        json!({"action":"save_trigger_binding","expected_revision":0,"draft":draft}),
    )
    .unwrap();
    assert_eq!(saved["type"], "trigger_binding");
    assert_eq!(saved["value"]["revision"], 1);
    assert_eq!(saved["value"]["enabled"], false);
    assert!(
        call(
            &state,
            &path,
            json!({"action":"save_trigger_binding","expected_revision":0,"draft":draft})
        )
        .is_err()
    );
    let list = call(&state, &path, json!({"action":"trigger_bindings"})).unwrap();
    assert_eq!(list["value"], json!([saved["value"]]));
    let snapshot = json!({"source":"github","kind":"issue","resource":"github:org/repo/issue/7","generation":"stable","event":"opened","delivery_id":null,"schema":schema["value"],"values":{"repository":"org/repo","number":7,"action":"opened","title":"Example","material_digest":"digest","labels":[]},"actor":null});
    let preview = call(
        &state,
        &path,
        json!({"action":"preview_trigger_mapping","draft":draft,"snapshot":snapshot}),
    )
    .unwrap();
    assert_eq!(preview["type"], "trigger_preview");
    assert_eq!(preview["value"]["inputs"], json!({"value":7}));
    assert_eq!(preview["value"]["overlaps"], json!({}));
    let mut second = draft.clone();
    second["id"] = json!("second");
    call(
        &state,
        &path,
        json!({"action":"save_trigger_binding","expected_revision":0,"draft":second}),
    )
    .unwrap();
    let enabled=call(&state,&path,json!({"action":"enable_trigger_binding","id":"first","expected_revision":1,"enabled":true,"acknowledged_overlaps":{}})).unwrap();
    assert_eq!(enabled["value"]["revision"], 2);
    let overlap = call(
        &state,
        &path,
        json!({"action":"preview_trigger_mapping","draft":second,"snapshot":snapshot}),
    )
    .unwrap();
    assert_eq!(overlap["value"]["overlaps"], json!({"first":2}));
    assert!(call(&state,&path,json!({"action":"enable_trigger_binding","id":"second","expected_revision":1,"enabled":true,"acknowledged_overlaps":{}})).unwrap_err().contains("overlapping"));
    let accepted=call(&state,&path,json!({"action":"enable_trigger_binding","id":"second","expected_revision":1,"enabled":true,"acknowledged_overlaps":{"first":2}})).unwrap();
    assert_eq!(accepted["value"]["enabled"], true);
}

fn intake_paths(state: &crate::state::AppState) -> Option<Vec<String>> {
    state
        .github
        .poller
        .lock()
        .as_ref()
        .map(|poller| poller.neutral_intake_paths.clone())
}

// Catches: enabling or disabling a GitHub binding while the app runs leaves the
// intake subscription as it was at runtime start, so a new issue binding gets
// no intake poll and a PR binding on a repository that no sidebar client polls
// is never fetched until restart (#3056-4ff3).
#[tokio::test]
async fn enabling_a_github_binding_subscribes_its_repository_for_intake_at_once() {
    for (kind, marker) in [
        ("issue", "intakeIssues:"),
        ("pull_request", "pullRequests("),
    ] {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = config.path().join("intake-repo");
        std::fs::create_dir_all(&repo).unwrap();
        for args in [
            vec!["init", "--quiet"],
            vec!["remote", "add", "origin", "https://github.com/org/repo.git"],
        ] {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        let repo = repo.to_string_lossy().into_owned();
        // Registered, but no sidebar client subscribes it: only intake may poll it.
        std::fs::write(
            crate::config::repository_file(),
            serde_json::to_string(&json!({"repos":{&repo:{"path":&repo}}})).unwrap(),
        )
        .unwrap();
        let (project, source, definition) = source();
        let path = project
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut http = crate::github::partial_tests::PollHttpFixture::start(true).await;
        let state = http.state.clone();
        state.workflow_runtime.install_neutral_catalog_host(source);
        let workflow = call(
            &state,
            &path,
            json!({"action":"create_draft","name":"intake target","definition":definition}),
        )
        .unwrap();
        let id = workflow["value"]["id"].as_str().unwrap();
        call(
            &state,
            &path,
            json!({"action":"publish","id":id,"expected_revision":1}),
        )
        .unwrap();
        let draft = json!({"id":"live","publication":{"project":path,"id":id,"revision":1},"source":"github","kind":kind,"resource_prefix":"github:org/repo/","events":["opened"],"filters":{},"ignored_actors":[],"mapping":{"inputs":{"value":"number"},"subject":{}},"limits":{"max_activations":20,"max_concurrency":1,"max_duration_ms":60000}});
        call(
            &state,
            &path,
            json!({"action":"save_trigger_binding","expected_revision":0,"draft":draft}),
        )
        .unwrap();
        assert_eq!(
            intake_paths(&state),
            None,
            "{kind}: a disabled binding subscribed"
        );

        call(&state,&path,json!({"action":"enable_trigger_binding","id":"live","expected_revision":1,"enabled":true,"acknowledged_overlaps":{}})).unwrap();
        assert_eq!(
            intake_paths(&state),
            Some(vec![repo.clone()]),
            "{kind}: enabling did not subscribe the repository for intake"
        );
        // The next poll reaches GitHub for that repository with no restart.
        let query = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let request = http.requests.recv().await.unwrap();
                let query = request["query"].as_str().unwrap().to_owned();
                if query.contains(marker) {
                    break query;
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("{kind}: no intake poll after enabling"));
        assert!(query.contains("name: \"repo\""), "{kind}: {query}");

        call(&state,&path,json!({"action":"enable_trigger_binding","id":"live","expected_revision":2,"enabled":false,"acknowledged_overlaps":{}})).unwrap();
        assert_eq!(
            intake_paths(&state),
            Some(Vec::new()),
            "{kind}: disabling kept the intake subscription"
        );
        crate::github_poller::stop_poller(&state).await.unwrap();
        http.close().await;
    }
}

// Catches: one enabled binding whose repository has no registered path fails
// the whole intake subscription, so every other binding loses intake, and the
// stale binding is dropped with no status the user can see (#3059-ffa3).
#[tokio::test]
async fn a_binding_on_an_unregistered_repository_does_not_block_intake_for_the_others() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let repo = config.path().join("intake-repo");
    std::fs::create_dir_all(&repo).unwrap();
    for args in [
        vec!["init", "--quiet"],
        vec!["remote", "add", "origin", "https://github.com/org/repo.git"],
    ] {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let repo = repo.to_string_lossy().into_owned();
    std::fs::write(
        crate::config::repository_file(),
        serde_json::to_string(&json!({"repos":{&repo:{"path":&repo}}})).unwrap(),
    )
    .unwrap();
    let (project, source, definition) = source();
    let path = project
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mut http = crate::github::partial_tests::PollHttpFixture::start(true).await;
    let state = http.state.clone();
    state.workflow_runtime.install_neutral_catalog_host(source);
    let workflow = call(
        &state,
        &path,
        json!({"action":"create_draft","name":"intake target","definition":definition}),
    )
    .unwrap();
    let id = workflow["value"]["id"].as_str().unwrap();
    call(
        &state,
        &path,
        json!({"action":"publish","id":id,"expected_revision":1}),
    )
    .unwrap();
    for (binding, prefix) in [("stale", "github:org/gone/"), ("live", "github:org/repo/")] {
        let draft = json!({"id":binding,"publication":{"project":path,"id":id,"revision":1},"source":"github","kind":"issue","resource_prefix":prefix,"events":["opened"],"filters":{},"ignored_actors":[],"mapping":{"inputs":{"value":"number"},"subject":{}},"limits":{"max_activations":20,"max_concurrency":1,"max_duration_ms":60000}});
        call(
            &state,
            &path,
            json!({"action":"save_trigger_binding","expected_revision":0,"draft":draft}),
        )
        .unwrap();
    }
    let stale = call(&state,&path,json!({"action":"enable_trigger_binding","id":"stale","expected_revision":1,"enabled":true,"acknowledged_overlaps":{}})).unwrap();
    let reason = stale["value"]["refusal_reason"]
        .as_str()
        .unwrap_or_default();
    assert!(
        reason.contains("org/gone"),
        "enabling the stale binding reported no refusal: {stale}"
    );
    let live = call(&state,&path,json!({"action":"enable_trigger_binding","id":"live","expected_revision":1,"enabled":true,"acknowledged_overlaps":{}})).unwrap();
    assert_eq!(live["value"]["refusal_reason"], serde_json::Value::Null);
    assert_eq!(
        intake_paths(&state),
        Some(vec![repo.clone()]),
        "the stale binding blocked the intake subscription of the registered one"
    );
    // The registered repository reaches GitHub on the next poll.
    let query = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let request = http.requests.recv().await.unwrap();
            let query = request["query"].as_str().unwrap().to_owned();
            if query.contains("intakeIssues:") {
                break query;
            }
        }
    })
    .await
    .expect("no intake poll for the registered binding");
    assert!(query.contains("name: \"repo\""), "{query}");
    // The listed status keeps naming the stale binding, and only it.
    let list = call(&state, &path, json!({"action":"trigger_bindings"})).unwrap();
    let reasons = list["value"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (
                b["draft"]["id"].as_str().unwrap().to_owned(),
                b["refusal_reason"].as_str().map(str::to_owned),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(reasons[0].1, None, "{reasons:?}");
    assert_eq!(reasons[0].0, "live");
    assert_eq!(reasons[1].0, "stale");
    assert!(
        reasons[1]
            .1
            .as_deref()
            .is_some_and(|r| r.contains("org/gone")),
        "{reasons:?}"
    );
    crate::github_poller::stop_poller(&state).await.unwrap();
    http.close().await;
}
