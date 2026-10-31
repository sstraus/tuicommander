//! Machine-owned Once scheduling, independent of the WebView lifecycle.
use super::{
    definitions::DefinitionStore,
    dispatcher::{Dispatcher, native::NativeEffects},
    run::AutomationRun,
    scheduler::TICK_INTERVAL_SECS,
    store::{RunOwner, RunStore},
};
use crate::state::AppState;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Default)]
pub(crate) struct AutomationRuntime {
    started: AtomicBool,
    owner: parking_lot::Mutex<Option<RunStore>>,
}

impl AutomationRuntime {
    pub(crate) fn spawn(state: &Arc<AppState>) {
        if state
            .automation_runtime
            .started
            .swap(true, Ordering::AcqRel)
        {
            return;
        }
        let weak = Arc::downgrade(state);
        let path = crate::config::config_dir().join("automation_runs.sqlite3");
        let definitions = crate::config::config_dir().join("automations.json");
        tokio::spawn(async move {
            let owner = match tokio::task::spawn_blocking(move || {
                RunOwner::acquire_at(&path, chrono::Utc::now().timestamp_millis())
            })
            .await
            {
                Ok(Ok(owner)) => owner,
                error => {
                    // Do not expose prompts or output in runtime diagnostics.
                    tracing::warn!(source="automations", error=?error.map(|r| r.map(|_| ())), "Automation executor unavailable");
                    return;
                }
            };
            if let Some(state) = weak.upgrade() {
                *state.automation_runtime.owner.lock() = Some(owner.store().clone());
            } else {
                return;
            }
            let Some(state) = weak.upgrade() else { return };
            match owner.store().unsettled_publications() {
                Ok(runs) => {
                    for run in runs {
                        super::completion::publish_event_draft_pr(&state, &run);
                    }
                }
                Err(error) => {
                    tracing::error!(source="automations", %error, "Publication recovery failed")
                }
            }
            let mut events = state.event_bus.subscribe();
            drop(state);
            // An issue refused for capacity is retried on the scheduler tick.
            let mut issues = super::event_intake::IssueSnapshot::default();
            let mut reviews = super::pr_reviews::ReviewSnapshot::default();
            let mut last_intake = None;
            let mut timer = tokio::time::interval(Duration::from_secs(TICK_INTERVAL_SECS));
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                let (scheduled_wake, intake_wake) = tokio::select! {
                    _ = timer.tick() => (true, true),
                    event = events.recv() => {
                        match event {
                            Ok(crate::state::AppEvent::GitHubIntakeIssuesUpdate { repo_path, issues: update }) => {
                                issues.replace(repo_path, update);
                                (false, true)
                            }
                            Ok(crate::state::AppEvent::GitHubPrUpdate { repo_path, statuses }) => {
                                reviews.replace(repo_path, statuses);
                                (false, true)
                            }
                            Ok(crate::state::AppEvent::RepositoriesChanged
                                | crate::state::AppEvent::SettingsChanged(_)) => (false, true),
                            Ok(crate::state::AppEvent::AutomationRunChanged { .. }
                                | crate::state::AppEvent::ProgressRecorded { .. }
                                | crate::state::AppEvent::PtyExit { .. }
                                | crate::state::AppEvent::SessionClosed { .. }
                                | crate::state::AppEvent::SessionStateChanged { .. }) => (false, false),
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                // Issue updates may be among the lost events: drop the
                                // snapshot and ask the poller to emit everything again.
                                issues.lost_updates();
                                reviews.lost_updates();
                                if let Some(state) = weak.upgrade()
                                    && let Err(error) = crate::github_poller::send_poller_cmd(&state, crate::github_poller::PollerCmd::ForceResync)
                                {
                                    tracing::warn!(source="automations", %error, "Issue resync after bus lag not requested");
                                }
                                (false, false)
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                            _ => continue,
                        }
                    }
                };
                let Some(state) = weak.upgrade() else {
                    return;
                };
                let reconcile_state = state.clone();
                let reconcile_store = owner.store().clone();
                match tokio::task::spawn_blocking(move || {
                    super::completion::reconcile(
                        &reconcile_store,
                        &super::completion::NativeCompletion(&reconcile_state),
                        chrono::Utc::now().timestamp_millis(),
                    )
                })
                .await
                {
                    Ok(Ok(())) => {}
                    error => tracing::warn!(
                        source = "automations",
                        ?error,
                        "Automation reconciliation failed"
                    ),
                }
                if let Err(error) = super::triage::publish_pending(&state, owner.store()).await {
                    tracing::warn!(source="automations", %error, "Triage publication queue failed");
                }
                if !scheduled_wake && !intake_wake {
                    continue;
                }
                let polling_config = definitions.clone();
                let polling_state = state.clone();
                let polling = tokio::task::spawn_blocking(move || {
                    let config = DefinitionStore::at_path(polling_config).load()?;
                    let mut combined = config.event_intake.clone();
                    combined.repositories.extend(config.pull_request_reviews.repositories.clone());
                    let (paths, errors) = intake_poll_paths(&polling_state, &combined);
                    for error in errors {
                        tracing::error!(source="automations", %error, "Event intake configuration error");
                    }
                    Ok::<_, String>(((config.event_intake, config.pull_request_reviews), paths))
                })
                .await;
                let (paths, changed) = match polling {
                    Ok(Ok((intake, paths))) => {
                        let changed = last_intake.as_ref() != Some(&intake);
                        last_intake = Some(intake);
                        (paths, changed)
                    }
                    error => {
                        last_intake = None;
                        tracing::error!(source="automations", error=?error, "Event intake configuration error");
                        (Vec::new(), false)
                    }
                };
                let review_paths = paths.clone();
                let resync = changed && !paths.is_empty();
                if let Err(error) = crate::github_poller::set_intake_paths(&state, paths) {
                    tracing::error!(source="automations", %error, "Event intake configuration error");
                    last_intake = None;
                } else if resync
                    && let Err(error) = crate::github_poller::send_poller_cmd(
                        &state,
                        crate::github_poller::PollerCmd::ForceResync,
                    )
                {
                    tracing::error!(source="automations", %error, "Event intake configuration resync failed");
                    last_intake = None;
                }
                let review_state = state.clone();
                let review_store = owner.store().clone();
                let dispatcher = Arc::new(Dispatcher {
                    definitions: DefinitionStore::at_path(definitions.clone()),
                    runs: owner.store().clone(),
                    effects: NativeEffects(state),
                });
                let blocking = dispatcher.clone();
                let snapshot = issues.issues();
                let review_snapshot = reviews.clone();
                let runs = match tokio::task::spawn_blocking(move || {
                    // A failed half must not strand the other half's Reserved runs.
                    let now = chrono::Utc::now();
                    let mut runs = Vec::new();
                    let mut errors = Vec::new();
                    if intake_wake {
                        match blocking.admit_events(&snapshot, now) {
                            Ok(admitted) => runs.extend(admitted),
                            Err(error) => errors.push(error),
                        }
                    }
                    if intake_wake {
                        match blocking.definitions.load().and_then(|config| {
                            review_snapshot.admit(
                                &blocking.runs,
                                &config,
                                &review_paths,
                                now.timestamp_millis(),
                            )
                        }) {
                            Ok(admitted) => runs.extend(admitted),
                            Err(error) => errors.push(error),
                        }
                    }
                    if scheduled_wake {
                        match blocking.reserve_due(now) {
                            Ok(due) => runs.extend(due),
                            Err(error) => errors.push(error),
                        }
                    }
                    (runs, errors)
                })
                .await
                {
                    Ok((runs, errors)) => {
                        for error in errors {
                            tracing::warn!(source="automations", %error, "Automation wake failed");
                        }
                        runs
                    }
                    error => {
                        tracing::warn!(source="automations", error=?error, "Automation wake failed");
                        continue;
                    }
                };
                for run in runs
                    .into_iter()
                    .filter(|run| run.status == super::run::RunStatus::Reserved)
                {
                    if matches!(
                        run.trigger,
                        super::run::RunTrigger::PullRequestReview { .. }
                    ) {
                        tokio::spawn(super::pr_reviews::dispatch(
                            review_state.clone(),
                            review_store.clone(),
                            run,
                        ));
                        continue;
                    }
                    let dispatcher = dispatcher.clone();
                    tokio::spawn(async move {
                        if let Err(error) = dispatcher.dispatch(&run.id).await {
                            tracing::warn!(source="automations", %error, run_id=%run.id, "Automation dispatch persistence failed");
                        }
                    });
                }
            }
        });
    }

    pub(crate) fn notification_store(&self) -> Option<RunStore> {
        self.owner.lock().clone()
    }

    /// Step 8 exposes this same owner-guarded boundary through HTTP and IPC.
    pub(crate) async fn run_now(state: &Arc<AppState>, id: &str) -> Result<AutomationRun, String> {
        let runs = state
            .automation_runtime
            .owner
            .lock()
            .clone()
            .ok_or("Automation executor unavailable: this process is not the owner")?;
        Arc::new(Dispatcher {
            definitions: DefinitionStore::new(),
            runs,
            effects: NativeEffects(state.clone()),
        })
        .run_now(id)
        .await
    }
}

/// Resolve only registered, unambiguous github.com repositories through the same
/// account/binding rules as the poller; an unmatched allowlist must never be silent.
fn intake_poll_paths(
    state: &AppState,
    intake: &super::model::EventIntake,
) -> (Vec<String>, Vec<String>) {
    if !intake.enabled() {
        return (Vec::new(), Vec::new());
    }
    use crate::github_account::{
        GitHubAccountRegistry, RepoBindingStore, RepoResolution, resolve_repo_account,
    };
    let registry = GitHubAccountRegistry::load();
    let bindings = RepoBindingStore::load();
    let default = crate::github::github_com_account(state);
    let mut resolved = Vec::new();
    for path in crate::config::registered_repo_paths() {
        if let RepoResolution::Bound {
            account,
            owner,
            repo,
        } = resolve_repo_account(
            std::path::Path::new(&path),
            &registry,
            &bindings,
            Some(&default),
        ) && account.host.is_cloud()
        {
            resolved.push((format!("{owner}/{repo}"), path));
        }
    }
    let mut paths = Vec::new();
    let mut errors = Vec::new();
    for repository in &intake.repositories {
        let Some((_, path)) = resolved
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(repository))
        else {
            errors.push(format!(
                "Event intake repository {repository} is not registered with an unambiguous GitHub account on this daemon"
            ));
            continue;
        };
        if !paths.contains(path) {
            paths.push(path.clone());
        }
    }
    paths.sort();
    (paths, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: native completion logs a created PR URL but loses it from run details on restart.
    #[tokio::test]
    async fn native_publication_url_survives_owner_restart() {
        use super::super::{
            completion::{CompletionEffects, NativeCompletion},
            model::AutomationDefinition,
            run::{RunDetails, RunStatus, RunTrigger},
        };
        use std::sync::atomic::AtomicUsize;

        let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(scratch.path().to_owned());
        let git = |args: &[&str]| super::super::publish::run_git(scratch.path(), args).unwrap();
        git(&["init", "--initial-branch=tidy-dev-prepare-team-pr-skill"]);
        git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-m",
            "test",
        ]);
        git(&["remote", "add", "origin", "https://github.com/cli/cli.git"]);
        std::fs::write(scratch.path().join("automations.json"), serde_json::json!({
            "event_intake": {"repositories": ["cli/cli"], "label": "run", "definition_id": "event"}
        }).to_string()).unwrap();

        // The recorded provider response is served through the real RepoApi URL,
        // using isolated trust roots/DNS and a dummy token, never live credentials.
        let recorded: serde_json::Value =
            serde_json::from_str(include_str!("publish/fixtures/pulls.json")).unwrap();
        let created = recorded[0].clone();
        let posts = Arc::new(AtomicUsize::new(0));
        let posted = posts.clone();
        let router = axum::Router::new().route(
            "/repos/cli/cli/pulls",
            axum::routing::get(|| async { axum::Json(serde_json::json!([])) }).post(move || {
                posted.fetch_add(1, Ordering::SeqCst);
                let response = created.clone();
                async move { axum::Json(response) }
            }),
        );
        let cert = include_bytes!("../fixtures/github_volume/cert.pem");
        let key = include_bytes!("../fixtures/github_volume/key.pem");
        let tls = axum_server::tls_rustls::RustlsConfig::from_pem(cert.to_vec(), key.to_vec())
            .await
            .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let handle = axum_server::Handle::new();
        let server = tokio::spawn(
            axum_server::from_tcp_rustls(listener, tls)
                .unwrap()
                .handle(handle.clone())
                .serve(router.into_make_service()),
        );
        let mut state = crate::state::tests_support::make_test_app_state_in(scratch.path());
        state.http_client = reqwest::Client::builder()
            .no_proxy()
            .tls_certs_only([reqwest::Certificate::from_pem(cert).unwrap()])
            .resolve("api.github.com", address)
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        *state.github.token.write() = Some("test-only-credential".into());
        let state = Arc::new(state);
        let path = scratch.path().join("automation_runs.sqlite3");
        let now = chrono::Utc::now().timestamp_millis();
        let owner = RunOwner::acquire_at(&path, now).unwrap();
        *state.automation_runtime.owner.lock() = Some(owner.store().clone());
        let definition: AutomationDefinition = serde_json::from_value(serde_json::json!({
            "id":"event", "name":"Event", "prompt":"Resolve issue", "run_config":"codex",
            "repository":scratch.path(), "workspace":{"mode":"new_per_run", "base_branch":"trunk"},
            "cron":"0 9 * * *", "timezone":"UTC", "enabled":true,
            "grace_secs":1800, "overlap":"skip", "max_duration_secs":3600, "precheck":null
        }))
        .unwrap();
        let run = owner
            .store()
            .reserve(
                &definition,
                RunTrigger::Event {
                    repository: "cli/cli".into(),
                    issue_number: 3,
                    label: "run".into(),
                },
                now,
            )
            .unwrap()
            .unwrap();
        let run = owner
            .store()
            .transition(
                &run.id,
                RunStatus::Completed,
                RunDetails {
                    workspace: Some(scratch.path().to_string_lossy().into_owned()),
                    ..Default::default()
                },
                now + 1,
            )
            .unwrap();
        NativeCompletion(&state).publish(&run);
        // The native callback owns an Arc until all publication/settlement work
        // finishes. Wait for that task, not a timing guess after the POST arrives.
        tokio::time::timeout(Duration::from_secs(60), async {
            while Arc::strong_count(&state) > 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("native publication task did not finish");
        handle.shutdown();
        server.await.unwrap().unwrap();
        assert_eq!(
            posts.load(Ordering::SeqCst),
            1,
            "native completion must create the PR"
        );
        drop(state);
        drop(owner);
        let restarted = RunOwner::acquire_at(&path, now + 2).unwrap();
        let details = serde_json::to_value(restarted.store().get(&run.id).unwrap()).unwrap();
        assert_eq!(details["status"], "completed");
        assert_eq!(
            details["outcome"]["pr_url"], "https://github.com/cli/cli/pull/14846",
            "created PR URL must remain in run details after owner restart"
        );
    }

    // Catches: intake waiting for a client to start the shared GitHub poller.
    #[tokio::test]
    async fn headless_intake_polls_without_client() {
        assert_headless_intake(vec!["sstraus/tuicommander".into()]).await;
    }

    // Catches: one unresolved allowlist entry disabling valid registered siblings.
    #[tokio::test]
    async fn headless_intake_keeps_valid_repository_with_missing_entries() {
        assert_headless_intake(vec![
            "missing/first".into(),
            "sstraus/tuicommander".into(),
            "missing/second".into(),
        ])
        .await;
    }

    async fn assert_headless_intake(repositories: Vec<String>) {
        let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let config = scratch.path().join("config");
        std::fs::create_dir_all(&config).unwrap();
        let _config = crate::config::set_config_dir_override(config.clone());
        let repo = scratch.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        for args in [
            vec!["init"],
            vec![
                "remote",
                "add",
                "origin",
                "https://github.com/sstraus/tuicommander.git",
            ],
        ] {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::fs::write(
            config.join("repositories.json"),
            serde_json::to_vec(&serde_json::json!({
                "repos": { repo.to_string_lossy().as_ref(): {} }
            }))
            .unwrap(),
        )
        .unwrap();
        let intake = super::super::model::AutomationsConfig {
            event_intake: super::super::model::EventIntake {
                repositories,
                label: "tuic-run".into(),
                definition_id: "not-dispatched".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        std::fs::write(
            config.join("automations.json"),
            serde_json::to_vec(&intake).unwrap(),
        )
        .unwrap();
        let mut fixture = crate::github::partial_tests::PollHttpFixture::start(true).await;
        AutomationRuntime::spawn(&fixture.state);
        // Owner acquisition is setup, not the deadline under test.
        while fixture
            .state
            .automation_runtime
            .notification_store()
            .is_none()
        {
            tokio::task::yield_now().await;
        }
        let request = tokio::time::timeout(Duration::from_secs(5), fixture.requests.recv())
            .await
            .expect("headless intake did not poll after acquiring ownership")
            .unwrap();
        let (resolved, errors) = intake_poll_paths(&fixture.state, &intake.event_intake);
        assert_eq!(resolved, vec![repo.to_string_lossy().into_owned()]);
        let missing: Vec<_> = intake
            .event_intake
            .repositories
            .iter()
            .filter(|name| name.starts_with("missing/"))
            .collect();
        assert_eq!(
            errors.len(),
            missing.len(),
            "each failed entry needs its own diagnostic"
        );
        for name in missing {
            assert!(
                errors.iter().any(|error| error.contains(name)),
                "missing diagnostic for {name}: {errors:?}"
            );
        }
        let query = request["query"].as_str().unwrap();
        assert!(query.contains("owner: \"sstraus\""), "{query}");
        assert!(
            query.contains("issues("),
            "intake must request issues: {query}"
        );
        assert!(
            !query.contains("assignee:"),
            "intake must not filter by viewer: {query}"
        );
        // Catches: a later client replacing the intake subscription or disabling issues.
        crate::github_poller::ensure_polling(&fixture.state, vec![], "disabled".into(), false);
        let request = tokio::time::timeout(Duration::from_secs(5), fixture.requests.recv())
            .await
            .expect("client configuration stranded intake polling")
            .unwrap();
        let query = request["query"].as_str().unwrap();
        assert!(
            query.contains("issues(") && !query.contains("filterBy:"),
            "{query}"
        );
        crate::github_poller::stop_poller(&fixture.state)
            .await
            .unwrap();
        crate::github_poller::send_poller_cmd(
            &fixture.state,
            crate::github_poller::PollerCmd::ForceResync,
        )
        .unwrap();
        let request = tokio::time::timeout(Duration::from_secs(5), fixture.requests.recv())
            .await
            .expect("client stop stranded intake polling")
            .unwrap();
        assert!(request["query"].as_str().unwrap().contains("issues("));

        // Catches: config changes retaining the old repository rather than starting the new one.
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args([
                "remote",
                "set-url",
                "origin",
                "https://github.com/sstraus/replacement.git",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut changed = intake;
        changed.event_intake.repositories = vec!["SSTRAUS/replacement".into()];
        // The runtime reads concurrently. A truncate/write can be quarantined as
        // corrupt between those operations, leaving no live config to reconcile.
        crate::config::ConfigFile::at_path(config.join("automations.json"))
            .save(&changed)
            .unwrap();
        fixture
            .state
            .event_bus
            .send(crate::state::AppEvent::RepositoriesChanged)
            .unwrap();
        let query = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let request = fixture.requests.recv().await.unwrap();
                let query = request["query"].as_str().unwrap().to_owned();
                if query.contains("name: \"replacement\"") {
                    break query;
                }
            }
        })
        .await
        .expect("changed intake configuration did not reach polling");
        assert!(
            query.contains("issues(") && !query.contains("filterBy:"),
            "{query}"
        );
        crate::github_poller::set_intake_paths(&fixture.state, vec![]).unwrap();
        crate::github_poller::stop_poller(&fixture.state)
            .await
            .unwrap();
        fixture.close().await;
    }

    // Catches: an unknown allowlist entry silently disabling headless intake.
    #[test]
    fn intake_reports_unregistered_repository() {
        let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(scratch.path().into());
        let state = crate::state::tests_support::make_test_app_state();
        let intake = super::super::model::EventIntake {
            repositories: vec!["missing/repository".into()],
            ..Default::default()
        };
        let (paths, errors) = intake_poll_paths(&state, &intake);
        assert_eq!(paths, Vec::<String>::new());
        let error = &errors[0];
        assert!(
            error.contains("missing/repository") && error.contains("not registered"),
            "{error}"
        );
    }

    // Catches: the default-off intake starting a poller without an allowlist.
    #[tokio::test]
    async fn empty_intake_does_not_start_poller() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let (paths, errors) = intake_poll_paths(&state, &Default::default());
        assert_eq!(errors, Vec::<String>::new());
        crate::github_poller::set_intake_paths(&state, paths).unwrap();
        assert!(state.github.poller.lock().is_none());
    }

    // Catches: forcing intake's all-issues filter onto unrelated client repositories.
    #[tokio::test]
    async fn intake_filter_does_not_replace_client_filter() {
        let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(scratch.path().join("config"));
        std::fs::create_dir_all(scratch.path().join("config")).unwrap();
        std::fs::write(scratch.path().join("config/automations.json"), serde_json::to_vec(&serde_json::json!({
            "event_intake": {"repositories": ["sstraus/intake"], "label": "tuic-run", "definition_id": "pilot"}
        })).unwrap()).unwrap();
        let mut paths = Vec::new();
        for name in ["intake", "client"] {
            let repo = scratch.path().join(name);
            std::fs::create_dir_all(&repo).unwrap();
            for args in [
                vec!["init".to_string()],
                vec![
                    "remote".into(),
                    "add".into(),
                    "origin".into(),
                    format!("https://github.com/sstraus/{name}.git"),
                ],
            ] {
                let output = std::process::Command::new("git")
                    .arg("-C")
                    .arg(&repo)
                    .args(args)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            paths.push(repo.to_string_lossy().into_owned());
        }
        let mut fixture = crate::github::partial_tests::PollHttpFixture::start(true).await;
        crate::github_poller::set_intake_paths(&fixture.state, vec![paths[0].clone()]).unwrap();
        crate::github_poller::ensure_polling(
            &fixture.state,
            vec![paths[1].clone()],
            "assigned".into(),
            false,
        );
        tokio::time::timeout(Duration::from_secs(5), async {
            let mut seen_intake = false;
            let mut seen_client = false;
            while !seen_intake || !seen_client {
                let request = fixture.requests.recv().await.unwrap();
                let query = request["query"].as_str().unwrap();
                if query.contains("name: \"intake\"") {
                    let intake_selection = query
                        .split("intakeIssues:")
                        .nth(1)
                        .expect("labelled intake selection")
                        .split("viewerPrs:")
                        .next()
                        .unwrap();
                    assert!(
                        intake_selection.contains("issues(")
                            && !intake_selection.contains("filterBy:"),
                        "{query}"
                    );
                    seen_intake = true;
                }
                if query.contains("name: \"client\"") {
                    assert!(query.contains("assignee: \"sstraus\""), "{query}");
                    seen_client = true;
                }
            }
        })
        .await
        .expect("shared poller failed to poll both subscriptions");
        crate::github_poller::set_intake_paths(&fixture.state, vec![]).unwrap();
        crate::github_poller::stop_poller(&fixture.state)
            .await
            .unwrap();
        fixture.close().await;
    }
}
