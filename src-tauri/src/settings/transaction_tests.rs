//! Consumer regressions at the actual AppConfig owner boundary.
use super::{native::native_catalogue, request::*, service::Service};
use crate::{
    config::settings_app::AppSettingsStore,
    config::{self, AppConfig},
};
use serde_json::{Value, json};
use std::sync::Arc;

// Catches: a disconnected MCP connection still writes an approved exposure change
// after its commit waited for the owner, because approval was consumed too early.
#[tokio::test]
#[serial_test::serial]
async fn disconnect_while_approved_commit_waits_must_leave_config_unchanged() {
    queued_approval_revoked_before_commit(false).await;
}

// Catches: an approved grant expires while queued but still persists a change.
#[tokio::test(start_paused = true)]
#[serial_test::serial]
async fn expiry_while_approved_commit_waits_must_leave_config_unchanged() {
    queued_approval_revoked_before_commit(true).await;
}

async fn queued_approval_revoked_before_commit(expire: bool) {
    use super::policy::Caller;
    use super::service::Store;
    use super::transaction::Prepared;
    use std::sync::{Mutex, mpsc};

    struct QueuedOwner {
        inner: AppSettingsStore,
        entered: tokio::sync::Notify,
        release: Mutex<mpsc::Receiver<()>>,
    }
    impl Store for QueuedOwner {
        fn id(&self) -> &'static str {
            "app"
        }
        fn target(&self) -> Target {
            Target::Machine
        }
        fn read(&self) -> Result<Value, Failure> {
            self.inner.read()
        }
        fn validate_proposal(&self, latest: &Value, value: &Value) -> Result<(), Failure> {
            self.inner.validate_proposal(latest, value)
        }
        fn commit(
            &self,
            prepare: &mut dyn FnMut(&Value) -> Result<Prepared, Failure>,
        ) -> Result<CommitReply, Failure> {
            self.entered.notify_one();
            self.release.lock().unwrap().recv().unwrap();
            self.inner.commit(prepare)
        }
        async fn apply(&self, reply: &CommitReply) -> Result<bool, Failure> {
            self.inner.apply(reply).await
        }
    }

    crate::credentials::reset_test_faults();
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = config::set_config_dir_override(dir.path().to_path_buf());
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let (release, waiting) = mpsc::channel();
    let owner = Arc::new(QueuedOwner {
        inner: AppSettingsStore::new(state.clone()),
        entered: tokio::sync::Notify::new(),
        release: Mutex::new(waiting),
    });
    let policy = state.settings_policy.clone();
    let service = Arc::new(
        Service::new(Arc::new(native_catalogue().unwrap()), owner.clone())
            .with_policy(policy.clone()),
    );
    let path = "/services/auth/username";
    let revision = service
        .get(Target::Machine, "app".into(), vec![path.into()])
        .await
        .unwrap()
        .revision;
    let caller = Caller::mcp(
        "127.0.0.1:12345".parse().unwrap(),
        "queued-connection".into(),
        true,
    );
    let request = Request::Set {
        target: Target::Machine,
        store: "app".into(),
        path: path.into(),
        value: json!("approved-user"),
        expected_revision: revision,
    };
    let proposal = service
        .execute_as(request.clone(), &caller, None)
        .await
        .unwrap_err()
        .proposal_id
        .unwrap();
    assert_eq!(
        super::policy_tests::answer(state.clone(), caller.clone(), proposal.clone(), true).await["confirmed"],
        json!(true)
    );
    let task =
        tokio::spawn(async move { service.execute_as(request, &caller, Some(&proposal)).await });
    owner.entered.notified().await;
    if expire {
        tokio::time::advance(std::time::Duration::from_secs(300)).await;
    } else {
        policy.disconnect_mcp("queued-connection");
    }
    release.send(()).unwrap();
    let outcome = task.await.unwrap();
    assert_eq!(
        state.config.read().services.auth.username,
        "",
        "a revoked approval committed an exposure change: {outcome:?}"
    );
    assert!(
        !dir.path().join("config.json").exists(),
        "revocation must prevent persistence"
    );
    assert_eq!(outcome.unwrap_err().code, "confirmation_denied");
}

fn set(path: &str, value: Value) -> Edit {
    Edit::Set {
        path: path.into(),
        value,
    }
}

fn service() -> (Arc<crate::AppState>, Service<AppSettingsStore>) {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let service = Service::new(
        Arc::new(native_catalogue().unwrap()),
        Arc::new(AppSettingsStore::new(state.clone())),
    )
    .with_policy(state.settings_policy.clone());
    (state, service)
}

async fn read(service: &Service<AppSettingsStore>) -> Snapshot {
    service
        .get(
            Target::Machine,
            "app".into(),
            vec!["/font_size".into(), "/shell".into()],
        )
        .await
        .unwrap()
}

#[tokio::test]
#[serial_test::serial]
async fn patch_typo_rejected() {
    crate::credentials::reset_test_faults();
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = config::set_config_dir_override(dir.path().to_path_buf());
    let (_state, service) = service();
    let snapshot = read(&service).await;
    let error = service
        .patch(
            Target::Machine,
            "app".into(),
            snapshot.revision,
            vec![set("/font_siz", json!(21))],
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "unknown_path");
    assert!(!dir.path().join("config.json").exists());
    let malformed = json!({"action":"patch", "target":{"kind":"machine"}, "store":"app", "expectedRevsion":"r1", "operations":[]});
    assert!(serde_json::from_value::<Request>(malformed).is_err());
    let extra = json!({"action":"patch", "target":{"kind":"machine"}, "store":"app", "expectedRevision":"r1", "operations":[], "confirmed":true});
    assert!(serde_json::from_value::<Request>(extra).is_err());
}

#[tokio::test]
#[serial_test::serial]
async fn stale_revision_rejected() {
    crate::credentials::reset_test_faults();
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = config::set_config_dir_override(dir.path().to_path_buf());
    let (state, service) = service();
    let snapshot = read(&service).await;
    let first = service
        .patch(
            Target::Machine,
            "app".into(),
            snapshot.revision.clone(),
            vec![set("/font_size", json!(21))],
        )
        .await
        .unwrap();
    let error = service
        .patch(
            Target::Machine,
            "app".into(),
            snapshot.revision,
            vec![set("/font_size", json!(23))],
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "conflict");
    assert_eq!(error.revision, Some(first.revision.clone()));
    assert_eq!(state.config.read().font_size, 21);
    assert_eq!(
        read(&service).await.revision,
        first.revision,
        "reply revision must equal persisted readback"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn invalid_batch_writes_nothing() {
    crate::credentials::reset_test_faults();
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = config::set_config_dir_override(dir.path().to_path_buf());
    let (state, service) = service();
    let before = state.config.read().font_size;
    let revision = read(&service).await.revision;
    let error = service
        .patch(
            Target::Machine,
            "app".into(),
            revision.clone(),
            vec![
                set("/font_size", json!(21)),
                set("/font_weight", json!(950)),
            ],
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "out_of_range");
    assert!(!error.persisted);
    assert!(!dir.path().join("config.json").exists());
    assert_eq!(state.config.read().font_size, before);
    assert_eq!(read(&service).await.revision, revision);
}

#[tokio::test]
#[serial_test::serial]
async fn omitted_optional_set_reset_and_other_fields_survive() {
    crate::credentials::reset_test_faults();
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = config::set_config_dir_override(dir.path().to_path_buf());
    let (state, service) = service();
    let snapshot = read(&service).await;
    assert_eq!(snapshot.values["/shell"].stored, Value::Null);
    let reply = service
        .patch(
            Target::Machine,
            "app".into(),
            snapshot.revision,
            vec![
                set("/shell", json!("coordinator")),
                set("/font_size", json!(21)),
            ],
        )
        .await
        .unwrap();
    assert!(reply.persisted && reply.applied);
    assert_eq!(reply.changed_ids, vec!["app.font_size", "app.shell"]);
    assert!(reply.restart.contains(&"next_session".into()));
    let reply = service
        .patch(
            Target::Machine,
            "app".into(),
            reply.revision,
            vec![Edit::Reset {
                path: "/shell".into(),
            }],
        )
        .await
        .unwrap();
    assert!(state.config.read().shell.is_none());
    assert_eq!(state.config.read().font_size, 21);
    let same = service
        .patch(
            Target::Machine,
            "app".into(),
            reply.revision.clone(),
            vec![set("/font_size", json!(21))],
        )
        .await
        .unwrap();
    assert!(!same.persisted);
    assert_eq!(same.changed_ids, [] as [std::string::String; 0]);
    assert_eq!(same.revision, reply.revision);
}

#[tokio::test]
#[serial_test::serial]
async fn wrong_owner_malformed_paths_and_secret_objects_never_commit() {
    crate::credentials::reset_test_faults();
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = config::set_config_dir_override(dir.path().to_path_buf());
    let (_state, service) = service();
    let revision = read(&service).await.revision;
    let error = service
        .patch(
            Target::Instance,
            "app".into(),
            revision.clone(),
            vec![set("/font_size", json!(21))],
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "not_found");
    for path in [
        "",
        "font_size",
        "/",
        "/services//port",
        "/__proto__/font_size",
        "/../font_size",
        "/font~2size",
    ] {
        let error = service
            .patch(
                Target::Machine,
                "app".into(),
                revision.clone(),
                vec![set(path, json!(21))],
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, "unknown_path", "{path}");
    }
    let canary = "private-value-must-not-appear";
    let error = service
        .patch(
            Target::Machine,
            "app".into(),
            revision,
            vec![set("/services/relay/token", json!({"status":canary}))],
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "unavailable");
    assert!(!serde_json::to_string(&error).unwrap().contains(canary));
    assert!(!dir.path().join("config.json").exists());
}

#[path = "failure_tests.rs"]
mod failures;
