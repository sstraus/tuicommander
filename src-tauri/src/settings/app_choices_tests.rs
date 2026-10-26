//! Choices exercise the shared service without touching app config or the credential vault.
use super::{
    model::{Availability, Choices},
    native::native_catalogue,
    request::*,
    service::{Service, Store},
    transaction::Prepared,
};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Owner {
    state: parking_lot::Mutex<Value>,
    probes: AtomicUsize,
    dir: std::path::PathBuf,
}
impl Store for Owner {
    fn id(&self) -> &'static str {
        "app"
    }
    fn target(&self) -> Target {
        Target::Machine
    }
    fn read(&self) -> Result<Value, Failure> {
        Ok(self.state.lock().clone())
    }
    fn refresh_choices(
        &self,
        resolver: &str,
        path: &str,
    ) -> impl std::future::Future<Output = Result<Vec<Value>, Failure>> + Send {
        self.probes.fetch_add(1, Ordering::SeqCst);
        assert_eq!(resolver, "themes");
        assert_eq!(path, "/theme");
        std::future::ready(super::app_choices::theme_choices(&self.dir))
    }
    fn validate_proposal(&self, _: &Value, value: &Value) -> Result<(), Failure> {
        super::app_choices::validate(value)
    }
    fn commit(
        &self,
        prepare: &mut dyn FnMut(&Value) -> Result<Prepared, Failure>,
    ) -> Result<CommitReply, Failure> {
        let mut state = self.state.lock();
        let Prepared { value, mut reply } = prepare(&state)?;
        *state = value;
        reply.persisted = !reply.changed_ids.is_empty();
        Ok(reply)
    }
    fn apply(
        &self,
        _: &CommitReply,
    ) -> impl std::future::Future<Output = Result<bool, Failure>> + Send {
        std::future::ready(Ok(true))
    }
}
fn fixture(dir: &std::path::Path) -> (Arc<Owner>, Service<Owner>) {
    let owner = Arc::new(Owner {
        state: parking_lot::Mutex::new(
            serde_json::to_value(crate::config::AppConfig::default()).unwrap(),
        ),
        probes: AtomicUsize::new(0),
        dir: dir.into(),
    });
    let service = Service::new(Arc::new(native_catalogue().unwrap()), owner.clone());
    (owner, service)
}
async fn describe(service: &Service<Owner>, refresh: bool) -> Snapshot {
    let paths = vec!["/theme".into()];
    let request = if refresh {
        Request::Refresh {
            target: Target::Machine,
            store: "app".into(),
            paths,
        }
    } else {
        Request::Describe {
            target: Target::Machine,
            store: "app".into(),
            paths,
        }
    };
    let Reply::Snapshot(value) = service.execute(request).await.unwrap() else {
        panic!("expected snapshot")
    };
    value
}

/// Catches: describe implicitly launches discovery, rather than requiring explicit refresh.
#[tokio::test]
async fn app_describe_never_launches_choice_probes() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    std::fs::write(
        dir.path().join("custom.json"),
        include_str!("../themes/nord.json"),
    )
    .unwrap();
    let (owner, service) = fixture(dir.path());
    assert!(matches!(
        native_catalogue().unwrap().fields["app.font_family"].choices,
        Some(Choices::Static { .. })
    ));
    let before = describe(&service, false).await;
    assert_eq!(owner.probes.load(Ordering::SeqCst), 0);
    assert!(matches!(
        before.choices["/theme"],
        Choices::Dynamic { values: None, .. }
    ));
    let after = describe(&service, true).await;
    assert_eq!(owner.probes.load(Ordering::SeqCst), 1);
    let Choices::Dynamic {
        availability: Availability::Available,
        values: Some(values),
        ..
    } = &after.choices["/theme"]
    else {
        panic!("missing refreshed choices")
    };
    assert_eq!(values[0]["value"], "custom");
    describe(&service, false).await;
    assert_eq!(owner.probes.load(Ordering::SeqCst), 1);
    assert_eq!(before.revision, after.revision);
}

/// Catches: choice refresh or a one-field edit clobbers sibling settings or another owner.
#[tokio::test]
async fn app_describe_never_launches_choice_probes_preserves_other_owner_state() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let neighbour = dir.path().join("ui-prefs.json");
    std::fs::write(&neighbour, "sentinel").unwrap();
    let (owner, service) = fixture(dir.path());
    let before = owner.read().unwrap();
    let snapshot = service
        .get(Target::Machine, "app".into(), vec!["/font_family".into()])
        .await
        .unwrap();
    service
        .execute(Request::Set {
            target: Target::Machine,
            store: "app".into(),
            path: "/font_family".into(),
            value: json!("Fira Code"),
            expected_revision: snapshot.revision,
        })
        .await
        .unwrap();
    let mut expected = before;
    expected["font_family"] = json!("Fira Code");
    assert_eq!(owner.read().unwrap(), expected);
    describe(&service, true).await;
    assert_eq!(owner.read().unwrap(), expected);
    assert_eq!(std::fs::read_to_string(neighbour).unwrap(), "sentinel");
}

/// Catches: an absent theme directory being presented as a successful empty discovery.
#[test]
fn app_theme_choices_distinguish_empty_from_unavailable() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    assert_eq!(
        super::app_choices::theme_choices(dir.path()).unwrap(),
        [] as [serde_json::Value; 0]
    );
    assert!(super::app_choices::theme_choices(&dir.path().join("missing")).is_err());
}

/// Catches: supported font and IDE edits bypassing owner validation or rejecting bundled values.
#[test]
fn app_choices_validate_supported_values_without_probing() {
    let mut config = serde_json::to_value(crate::config::AppConfig::default()).unwrap();
    for (key, value) in [
        ("font_family", "not-bundled"),
        ("ide", "not-supported"),
        ("theme", "../escape"),
    ] {
        let before = config.clone();
        config[key] = json!(value);
        assert!(super::app_choices::validate(&config).is_err());
        config = before;
    }
    assert!(super::app_choices::validate(&config).is_ok());
    assert_eq!(
        super::app_choices::options()["fonts"]
            .as_object()
            .unwrap()
            .len(),
        11
    );
}
