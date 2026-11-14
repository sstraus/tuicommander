use crate::state::AppState;
use crate::workflows::catalog::*;
use crate::workflows::command::Invocation;
use serde_json::json;
use std::path::Path;

pub(super) fn configure(root: &Path, state: &AppState) {
    let mut accounts = serde_json::Map::new();
    for (provider, host) in [
        ("jira", "https://jira.example.invalid"),
        ("gitlab", "https://gitlab.example.invalid"),
    ] {
        let id = format!("fixture-{provider}");
        accounts.insert(
            id.clone(),
            json!({"id":id,"provider":provider,"site_id":provider,"host":host,
            "kind":"bearer","label":"Fixture account","login":null,"created_at":0}),
        );
        let directory = root.join("plugins").join(provider);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../plugins")
                .join(provider)
                .join("manifest.json"),
            directory.join("manifest.json"),
        )
        .unwrap();
        crate::plugins::write_plugin_data(provider.into(), "work-items.json".into(),
            json!({"accounts":[{"id":id,"host":host,"projectId":"FIXTURE","project":"fixture/project",
                "resolvedProject":{"path":"fixture/project","id":42}}]}).to_string()).unwrap();
    }
    std::fs::write(
        root.join("accounts.json"),
        json!({"accounts":accounts,"sites":{},"github_migrated":true}).to_string(),
    )
    .unwrap();
    let telegram = root.join("telegram");
    std::fs::create_dir_all(&telegram).unwrap();
    let file = telegram.join("config.json");
    std::fs::write(
        &file,
        json!({"enabled":true,"bot_alias":"fixture-bot"}).to_string(),
    )
    .unwrap();
    let allowlist = telegram.join("allowed_chat_ids");
    std::fs::write(&allowlist, "12345\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&telegram, std::fs::Permissions::from_mode(0o700)).unwrap();
        for private in [&file, &allowlist] {
            std::fs::set_permissions(private, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
    }
    let mut settings = state.config.write();
    // Configured executable metadata is never launched by catalog/preview.
    settings.ego_executable = "//workflow-fixture/bin/model".into();
    settings.ego_profile = "fixture-profile".into();
}

pub(super) fn script() -> ActivityDescriptor {
    let contract: ActivityDescriptor = serde_json::from_value(json!({
        "id":{"namespace":"custom","name":"calculate","version":1},"purpose":"Calculate a structured value",
        "handler":{"name":"host.script","version":1},"target":null,"config":{"fields":{}},
        "inputs":{"fields":{"value":{"value_type":"integer","required":true,"default":null}}},"subject":null,
        "outputs":{"fields":{"next":{"value_type":"integer","required":true,"default":null}}},
        "outcomes":["ok"],"required_capabilities":[],"execution_policies":["local"]
    })).unwrap();
    let policy = serde_json::from_value(json!({"version":1,"timeout_secs":10,"input_bytes":65536,
        "stdout_bytes":16384,"stderr_bytes":16384,"artifact_limit":4,"resources":[],"environment_refs":{},
        "secret_refs":{},"mutation":false,"retry":"never","reconciliation":{"type":"read_only"},
        "output":{"type":"structured"}})).unwrap();
    crate::workflows::run::neutral_script::script_descriptor(
        contract,
        Invocation::Shell {
            program: "python3".into(),
            script: "import json,sys; q=json.load(sys.stdin)".into(),
            version: 1,
        },
        &policy,
    )
    .unwrap()
}
