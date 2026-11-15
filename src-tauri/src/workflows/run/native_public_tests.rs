use super::super::check::git_output;
use super::super::native_test_support::Fixture;
use super::super::{NativeCaller, NativeContext};
use crate::agent_capabilities::CapabilitySet;
use crate::workflows::neutral_api::model::NeutralAction;
use serde_json::json;

use super::native_public_support::{answer, attach, call, commit_request, ready, request};
// Catches: public start never binds its authenticated owner, or Answer bypasses
// exact question ownership before the real approved commit service executes.
#[tokio::test]
async fn native_public_owner_start_and_exact_answer_execute_reviewed_commit() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let mut f = Fixture::new();
    attach(&f);
    f.context = NativeContext::capture(
        &f.state,
        NativeCaller::Owner,
        f.context.workspace_path(),
        vec![],
    )
    .unwrap();
    let path = f.context.workspace_path().to_owned();
    let base = git_output(&path, &["rev-parse", "HEAD"]).unwrap();
    std::fs::write(path.join("artifact.txt"), "public candidate").unwrap();
    git_output(&path, &["add", "."]).unwrap();
    let tree = git_output(&path, &["write-tree"]).unwrap();
    let review = super::super::native_review_tests::reviewed(
        &f,
        crate::workflows::review_verdict::CodeVerdict::Pass,
    )
    .await;
    let request = request(
        &f,
        "commit.create",
        json!({"parent":base,"tree":tree,"message":"public candidate","review":review}),
    );
    assert!(
        call(
            &f,
            NeutralAction::Start {
                request: request.clone()
            },
            None
        )
        .unwrap_err()
        .contains("authenticated backend caller")
    );
    let run = call(
        &f,
        NeutralAction::Start { request },
        Some(NativeCaller::Owner),
    )
    .unwrap();
    let run = ready(&f, run);
    let waiting = f.dispatch(&run).await;
    assert!(
        call(&f, answer(&waiting, 1), None)
            .unwrap_err()
            .contains("authenticated owner")
    );
    assert!(call(&f, answer(&waiting, 2), Some(NativeCaller::Owner)).is_err());
    let approved = call(&f, answer(&waiting, 1), Some(NativeCaller::Owner)).unwrap();
    let finished = f.dispatch(&approved).await;
    assert!(
        finished.effects.values().next().unwrap().settled(),
        "{finished:?}"
    );
    assert_ne!(git_output(&path, &["rev-parse", "HEAD"]).unwrap(), base);
    assert_eq!(
        git_output(&path, &["show", "HEAD:artifact.txt"]).unwrap(),
        "public candidate"
    );
}

// Catches: exact retries replacing caller/ceiling, a later capability upgrade
// widening captured rights, revoked Operator answers, or Owner losing recovery.
#[tokio::test]
async fn native_public_retry_preserves_caller_ceiling_and_answer_rechecks_operator() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    attach(&f);
    let request = request(&f, "commit.create", commit_request());
    let run = call(
        &f,
        NeutralAction::Start {
            request: request.clone(),
        },
        Some(NativeCaller::Agent("trusted".into())),
    )
    .unwrap();
    let run = ready(&f, run);
    let waiting = f.dispatch(&run).await;
    f.state.agent_capabilities.insert(
        "trusted".into(),
        CapabilitySet::from_spawn_arg(&json!(["operator"])).unwrap(),
    );
    call(
        &f,
        NeutralAction::Start {
            request: request.clone(),
        },
        Some(NativeCaller::Agent("trusted".into())),
    )
    .unwrap();
    assert!(
        call(
            &f,
            answer(&waiting, 1),
            Some(NativeCaller::Agent("trusted".into()))
        )
        .unwrap_err()
        .contains("operator")
    );
    for caller in [NativeCaller::Owner, NativeCaller::Agent("other".into())] {
        assert!(
            call(
                &f,
                NeutralAction::Start {
                    request: request.clone()
                },
                Some(caller)
            )
            .unwrap_err()
            .contains("different trusted binding")
        );
    }
    let mut operator_request = request.clone();
    operator_request.request_id = "operator-start".into();
    let operator_run = call(
        &f,
        NeutralAction::Start {
            request: operator_request,
        },
        Some(NativeCaller::Agent("trusted".into())),
    )
    .unwrap();
    let operator_run = ready(&f, operator_run);
    let operator_waiting = f.dispatch(&operator_run).await;
    call(
        &f,
        answer(&operator_waiting, 1),
        Some(NativeCaller::Agent("trusted".into())),
    )
    .unwrap();
    let mut missing_request = request.clone();
    missing_request.request_id = "missing-agent".into();
    assert!(
        call(
            &f,
            NeutralAction::Start {
                request: missing_request
            },
            Some(NativeCaller::Agent("missing".into()))
        )
        .unwrap_err()
        .contains("no backend capability binding")
    );
    f.state.agent_capabilities.remove("trusted");
    assert!(
        call(
            &f,
            NeutralAction::Start { request },
            Some(NativeCaller::Agent("trusted".into()))
        )
        .unwrap_err()
        .contains("revoked")
    );
    assert!(
        call(
            &f,
            answer(&waiting, 1),
            Some(NativeCaller::Agent("trusted".into()))
        )
        .is_err()
    );
    call(&f, answer(&waiting, 1), Some(NativeCaller::Owner)).unwrap();
}
