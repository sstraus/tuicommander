//! Story 2642-203b: exact-command consent, not lexical option bans, decides
//! which argv may receive secrets. Structural validation stays.
use super::*;

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|p| (*p).to_string()).collect()
}

fn put_secret(store: &SecretStore, name: &str, value: &str) {
    let form = store
        .open(
            Form::request(
                vec![Field {
                    name: name.into(),
                    kind: FieldKind::Password,
                    display: None,
                }],
                "test".into(),
            )
            .unwrap(),
        )
        .unwrap();
    store
        .submit(
            &form.nonce,
            Submission::stored(BTreeMap::from([(name.into(), value.into())])),
        )
        .unwrap();
}

fn approve(store: &SecretStore, name: &str, argv: &[String], cwd: &str) {
    let mut approval = Form::request(
        vec![Field {
            name: name.into(),
            kind: FieldKind::Password,
            display: None,
        }],
        "Approve".into(),
    )
    .unwrap();
    approval.argv = Some(argv.to_vec());
    approval.cwd = Some(cwd.into());
    let approval = store.open(approval).unwrap();
    let mut submission = Submission::declined();
    submission.status = "approved".into();
    submission.template = Some(argv.to_vec());
    store.submit(&approval.nonce, submission).unwrap();
}

// Catches: owner `git -c` and ordinary -e/-p flags (and env/printenv) refused
// before the exact-argv consent form, while malformed argv still fails.
#[test]
fn normal_short_options_reach_secret_consent() {
    for ok in [
        argv(&["git", "-c", "color.ui=false", "status"]),
        argv(&["grep", "-e", "pattern", "file"]),
        argv(&["ssh", "-p", "2222", "host"]),
        argv(&["mysql", "-psecret-from-env"]),
        argv(&["node", "-e", "0"]),
        argv(&["env"]),
        argv(&["printenv"]),
    ] {
        assert!(policy::validate_argv(&ok).is_ok(), "refused {ok:?}");
        assert!(policy::Template::new(ok.clone()).is_ok(), "template {ok:?}");
    }
    let too_many = vec!["x".to_string(); 129];
    let oversized = vec!["x".to_string(), "a".repeat(8193)];
    for bad in [
        vec![],
        argv(&[""]),
        argv(&["git", "-c\0x"]),
        argv(&["git\0", "status"]),
        too_many,
        oversized,
        argv(&["run.bat"]),
        argv(&["C:\\tools\\RUN.CMD", "x"]),
    ] {
        assert!(policy::validate_argv(&bad).is_err(), "accepted {bad:?}");
    }
}

// Catches: the run path still refusing `-c` argv lexically instead of asking
// for consent (the test host is headless with Remote Access off, so the consent
// step stops at the setup guidance).
#[tokio::test]
async fn run_with_short_option_reaches_consent_not_a_lexical_refusal() {
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    put_secret(&state.secrets, "PASS", "v");
    let exe = std::fs::canonicalize(std::env::current_exe().unwrap()).unwrap();
    let cwd = std::fs::canonicalize(crate::test_support::test_temp_root()).unwrap();
    let result = run::handle_secret(
        &state,
        &serde_json::json!({"action": "run", "names": ["PASS"], "argv": [exe, "-c", "color.ui=false"], "cwd": cwd}),
    )
    .await;
    let error = result["error"].as_str().unwrap_or_default();
    assert!(error.contains("Remote Access is off"), "{result}");
}

// Catches: prompt-injected argv reusing consent granted for a different
// command (changed executable, option value, extra or missing argument).
#[test]
fn changed_argv_cannot_reuse_secret_consent() {
    let store = SecretStore::default();
    put_secret(&store, "PASS", "hunter22");
    let approved = argv(&["/usr/bin/git", "-c", "color.ui=false", "status"]);
    approve(&store, "PASS", &approved, "/repo");
    let names = vec!["PASS".to_string()];
    assert!(store.allowed(&approved, &names, "/repo"));
    for changed in [
        argv(&["/tmp/git", "-c", "color.ui=false", "status"]),
        argv(&["/usr/bin/git", "-c", "core.pager=sh", "status"]),
        argv(&["/usr/bin/git", "-c", "color.ui=false", "status", "-v"]),
        argv(&["/usr/bin/git", "-c", "color.ui=false"]),
        argv(&["/usr/bin/git", "-ccolor.ui=false", "status"]),
    ] {
        assert!(!store.allowed(&changed, &names, "/repo"), "{changed:?}");
    }
    // A submitted template for a different argv cannot attach to this form.
    let mut form = Form::request(
        vec![Field {
            name: "PASS".into(),
            kind: FieldKind::Password,
            display: None,
        }],
        "Approve".into(),
    )
    .unwrap();
    form.argv = Some(approved.clone());
    form.cwd = Some("/repo".into());
    let form = store.open(form).unwrap();
    let mut submission = Submission::declined();
    submission.status = "approved".into();
    submission.template = Some(argv(&["/usr/bin/env"]));
    assert!(store.submit(&form.nonce, submission).is_err());
    assert!(!store.allowed(&argv(&["/usr/bin/env"]), &names, "/repo"));
}
