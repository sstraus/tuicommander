use super::{config_file::ConfigFileStore, request::CommitReply, transaction::Prepared};
use crate::config::ConfigFile;
use serde_json::json;

// Catches: comparing before normalization writes a canonical no-op and advertises false effects.
#[test]
fn normalized_noop_preserves_file_bytes_and_clears_effects() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let path = dir.path().join("settings.json");
    let original = "{ \"name\": \"kept\" }\n";
    std::fs::write(&path, original).unwrap();
    let store = ConfigFileStore::new(
        "fixture",
        ConfigFile::<serde_json::Value>::at_path(path.clone()),
    );
    let reply = store
        .commit(
            &mut |_| {
                Ok(Prepared {
                    value: json!({"name": "changed"}),
                    reply: CommitReply {
                        revision: "stale".into(),
                        changed_ids: vec!["name".into()],
                        persisted: false,
                        applied: false,
                        effects: vec!["refresh".into()],
                        restart: vec!["owner".into()],
                    },
                })
            },
            |_, next| next["name"] = json!("kept"),
        )
        .unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
    assert!(!reply.persisted);
    assert_eq!(reply.changed_ids, [] as [std::string::String; 0]);
    assert_eq!(reply.effects, [] as [std::string::String; 0]);
    assert_eq!(reply.restart, [] as [std::string::String; 0]);
    assert_ne!(reply.revision, "stale");
}
