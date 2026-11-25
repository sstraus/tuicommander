//! Isolated process replay of recorded Ego producer output; see fixture provenance.
use super::{registry::Registry, request::*};
use crate::{AppState, state::tests_support::make_test_app_state};
use serde_json::{Value, json};
use std::{os::unix::fs::PermissionsExt, sync::Arc};

pub(super) struct Fixture {
    pub dir: tempfile::TempDir,
    pub state: Arc<AppState>,
    pub registry: Registry,
}

pub(super) fn fixture() -> Fixture {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let script = dir.path().join("ego");
    std::fs::write(&script, include_str!("fixtures/ego/replay.py")).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    for (name, bytes) in [
        ("alpha.json", include_str!("fixtures/ego/stored-alpha.json")),
        ("base.json", include_str!("fixtures/ego/stored-alpha.json")),
        ("beta.json", include_str!("fixtures/ego/stored-alpha.json")),
        ("models.json", include_str!("fixtures/ego/models.json")),
        ("doctor.json", include_str!("fixtures/ego/doctor.json")),
        (
            "auth.json",
            include_str!("fixtures/ego/auth-inventory.json"),
        ),
        (
            "capabilities.json",
            include_str!("fixtures/ego/capabilities.json"),
        ),
        ("conflict.json", include_str!("fixtures/ego/conflict.json")),
    ] {
        let bytes = match name {
            "base.json" | "beta.json" => {
                // Captured values stay unchanged; isolated replay profiles need their own
                // transport identity instead of the captured alpha profile's identity.
                let mut listing: Value = serde_json::from_str(bytes).unwrap();
                listing["profile"] = if name == "base.json" {
                    Value::Null
                } else {
                    json!("beta")
                };
                listing.to_string()
            }
            _ => bytes.to_owned(),
        };
        std::fs::write(dir.path().join(name), bytes).unwrap();
    }
    let state = Arc::new(make_test_app_state());
    {
        let mut config = state.config.write();
        config.ego_executable = script.to_string_lossy().into_owned();
        config.ego_profile = "alpha".into();
        config.ai_chat_workspace = dir.path().to_string_lossy().into_owned();
    }
    let mut registry = Registry::default();
    registry
        .register(super::service::Service::new(
            Arc::new(super::native::native_catalogue().unwrap()),
            Arc::new(super::ego::EgoSettingsStore::new(state.clone()).unwrap()),
        ))
        .unwrap();
    Fixture {
        dir,
        state,
        registry,
    }
}

pub(super) async fn get(f: &Fixture, paths: &[&str]) -> Result<Snapshot, Failure> {
    match f
        .registry
        .execute(Request::Get {
            target: Target::Profile { id: "alpha".into() },
            store: "ego".into(),
            paths: paths.iter().map(|s| (*s).into()).collect(),
        })
        .await?
    {
        Reply::Snapshot(s) => Ok(s),
        _ => panic!("expected snapshot"),
    }
}

pub(super) async fn set(
    f: &Fixture,
    revision: String,
    path: &str,
    value: Value,
) -> Result<CommitReply, Failure> {
    match f
        .registry
        .execute(Request::Set {
            target: Target::Profile { id: "alpha".into() },
            store: "ego".into(),
            path: path.into(),
            value,
            expected_revision: revision,
        })
        .await?
    {
        Reply::Commit(c) => Ok(c),
        _ => panic!("expected commit"),
    }
}

pub(super) fn disk(f: &Fixture, profile: &str) -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(f.dir.path().join(format!("{profile}.json"))).unwrap(),
    )
    .unwrap()
}

pub(super) fn calls(f: &Fixture) -> Vec<Value> {
    std::fs::read_to_string(f.dir.path().join("calls.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}

pub(super) fn expected_model() -> Value {
    json!("ollama/changed")
}
