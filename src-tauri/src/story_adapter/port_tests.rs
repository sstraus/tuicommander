//! A project bound to the reference adapter through the TUIC settings file:
//! story actions, capabilities and watch paths reach the adapter, never SQLite.

use super::binding::{SETTINGS_FILE, paths_changed};
use crate::state::AppEvent;
use crate::stories::{StoryStore, story_action, story_capabilities_for};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

struct Bound {
    _config: TempDir,
    _guard: Box<dyn std::any::Any>,
    dir: TempDir,
}

impl Bound {
    fn project(&self) -> PathBuf {
        std::fs::canonicalize(self.dir.path()).unwrap()
    }
    fn store_file(&self) -> PathBuf {
        self.project().join(".tuic-reference-stories.json")
    }
}

fn scratch() -> TempDir {
    TempDir::new_in(crate::test_support::test_temp_root()).unwrap()
}

/// Binds a fresh project to `command` in the settings file of a fresh config dir.
fn bind(command: &str, args: &[String]) -> Bound {
    let config = scratch();
    let guard = Box::new(crate::config::set_config_dir_override(
        config.path().to_path_buf(),
    ));
    let dir = scratch();
    let project = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::write(
        config.path().join(SETTINGS_FILE),
        json!({ project.to_string_lossy(): { "command": command, "args": args } }).to_string(),
    )
    .unwrap();
    Bound {
        _config: config,
        _guard: guard,
        dir,
    }
}

fn bind_reference() -> Bound {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../scripts/story-adapter-reference/adapter.mjs");
    bind("node", &[script.to_string_lossy().into_owned()])
}

fn act(project: &Path, action: Value) -> Result<Value, String> {
    let action = serde_json::from_value(action).expect("story action");
    story_action(project.to_str().unwrap(), action, None)
        .map(|reply| serde_json::to_value(reply).unwrap())
}

#[test]
fn a_bound_project_reads_and_writes_its_adapter_store_not_sqlite() {
    // Catches: story actions still opening the native SQLite store for a bound
    // project (a silent fallback), a stale revision accepted, or a note not sent
    // to the adapter. Transitions: execution_tests.rs.
    let bound = bind_reference();
    let project = bound.project();

    let plans = act(&project, json!({"action": "list_plans"})).unwrap();
    assert_eq!(plans["value"][0]["id"], "default", "{plans}");

    let created = act(
        &project,
        json!({"action": "create_story", "input": {
            "planId": "default", "title": "Fix login", "criteria": ["works"],
            "priority": 1, "origin": {"type": "native"}, "fileScope": []
        }}),
    )
    .unwrap();
    let id = created["value"]["id"].as_str().unwrap().to_string();
    let revision = created["value"]["revision"].as_i64().unwrap();
    let file = std::fs::read_to_string(bound.store_file()).unwrap();
    assert!(file.contains("Fix login"), "{file}");
    assert!(StoryStore::open().unwrap().get_story(&id).is_err());

    let stale = act(
        &project,
        json!({"action": "update_story", "story_id": id, "expected_revision": revision + 1,
               "patch": {"title": "Other"}}),
    )
    .unwrap_err();
    assert!(stale.starts_with("stale story revision"), "{stale}");

    let updated = act(
        &project,
        json!({"action": "update_story", "story_id": id, "expected_revision": revision,
               "patch": {"title": "Fix sign-in"}}),
    )
    .unwrap();
    assert_eq!(updated["value"]["title"], "Fix sign-in");
    let revision = updated["value"]["revision"].as_i64().unwrap();

    act(
        &project,
        json!({"action": "add_note", "story_id": id, "expected_revision": revision,
               "text": "checked by hand"}),
    )
    .unwrap();
    let notes = act(&project, json!({"action": "list_notes", "story_id": id})).unwrap();
    assert_eq!(notes["value"][0]["text"], "checked by hand", "{notes}");
}

#[test]
fn a_bound_adapter_that_cannot_start_is_unavailable_not_native() {
    // Catches: a broken adapter making TUIC show the project's native plans.
    // The native plan is archived: an open one would refuse the binding itself.
    let bound = bind("/nonexistent/story-adapter", &[]);
    let project = bound.project();
    let store = StoryStore::open().unwrap();
    let native = store
        .create_plan(crate::stories::NewPlan {
            project: project.to_string_lossy().into_owned(),
            title: "Native".into(),
            source: "plan.md".into(),
        })
        .unwrap();
    store
        .set_plan_archived(
            &native.id,
            native.revision,
            true,
            crate::stories::StoryTransitionActor::Human,
        )
        .unwrap();

    let error = act(&project, json!({"action": "list_plans"})).unwrap_err();
    assert!(error.contains("unavailable: cannot start"), "{error}");
    let error = story_capabilities_for(project.to_str().unwrap()).unwrap_err();
    assert!(error.contains("unavailable: cannot start"), "{error}");
    let status = act(&project, json!({"action":"backend_status"})).unwrap();
    assert_eq!(status["type"], "backend_status");
    assert_eq!(status["value"]["provenance"], "explicit");
    assert_eq!(status["value"]["availability"], "unavailable");
    assert_ne!(status["value"]["backend"], "native");
}

#[test]
fn capabilities_of_a_bound_project_come_from_its_adapter() {
    // Catches: the native descriptor answered for an external project, so the
    // UI and MCP offer plan creation, archive and transitions it cannot do.
    let bound = bind_reference();
    let caps = story_capabilities_for(bound.project().to_str().unwrap()).unwrap();
    let caps = serde_json::to_value(caps).unwrap();
    assert_eq!(caps["backend"], "external:tuic-reference");
    let ops = &caps["operations"];
    for supported in [
        "create_story",
        "update_story",
        "dependencies",
        "notes",
        "transitions",
    ] {
        assert_eq!(ops[supported], true, "{supported}: {caps}");
    }
    for refused in [
        "create_plan",
        "update_plan",
        "archive_plan",
        "archive_story",
    ] {
        assert_eq!(ops[refused], false, "{refused}: {caps}");
    }
}

#[test]
fn a_change_under_a_watch_path_publishes_the_changed_story_ids() {
    // Catches: the watcher hook ignoring the adapter's watch paths, forgetting
    // the cursor (every change a rescan), or calling the adapter for paths it
    // does not watch.
    let bound = bind_reference();
    let project = bound.project();
    let (bus, mut rx) = tokio::sync::broadcast::channel(8);
    let create = |title: &str| {
        act(
            &project,
            json!({"action": "create_story", "input": {
                "planId": "default", "title": title, "criteria": ["x"],
                "priority": 2, "origin": {"type": "native"}, "fileScope": []
            }}),
        )
        .unwrap()["value"]["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    create("first");
    let payload = |rx: &mut tokio::sync::broadcast::Receiver<AppEvent>| match rx.try_recv() {
        Ok(AppEvent::StoriesChanged { repo_path, payload }) => {
            assert_eq!(Path::new(&repo_path), project);
            Some(payload)
        }
        Ok(_) => panic!("unexpected event"),
        Err(_) => None,
    };

    paths_changed(&project, &[project.join("src/main.rs")], &bus);
    assert!(payload(&mut rx).is_none(), "a path outside the watch paths");

    paths_changed(&project, &[bound.store_file()], &bus);
    assert_eq!(
        payload(&mut rx).unwrap()["rescan"],
        true,
        "first call has no cursor"
    );

    let second = create("second");
    paths_changed(&project, &[bound.store_file()], &bus);
    let changed = payload(&mut rx).unwrap();
    assert_eq!(changed["rescan"], false, "{changed}");
    assert_eq!(changed["ids"], json!([second]));
}

#[test]
fn explicit_native_overrides_detection_and_invalid_override_never_falls_back() {
    let bound = bind("unused", &[]);
    let project = bound.project();
    std::fs::create_dir(project.join(".wiz")).unwrap();
    std::fs::write(
        bound._config.path().join(SETTINGS_FILE),
        json!({project.to_string_lossy(): {"backend":"native"}}).to_string(),
    )
    .unwrap();
    let status = act(&project, json!({"action":"backend_status"})).unwrap();
    assert_eq!(status["value"]["backend"], "native");
    assert_eq!(status["value"]["provenance"], "explicit");
    assert_eq!(
        act(&project, json!({"action":"list_plans"})).unwrap()["value"],
        json!([])
    );
    std::fs::write(
        bound._config.path().join(SETTINGS_FILE),
        json!({project.to_string_lossy(): {"backend":"invalid"}}).to_string(),
    )
    .unwrap();
    assert!(
        act(&project, json!({"action":"list_plans"}))
            .unwrap_err()
            .contains("invalid explicit")
    );
    assert_eq!(
        act(&project, json!({"action":"backend_status"})).unwrap()["value"]["availability"],
        "unavailable"
    );
}

#[test]
fn external_selection_refuses_open_native_plan_without_changing_its_records() {
    let bound = bind("unused", &[]);
    let project = bound.project();
    let store = StoryStore::open().unwrap();
    let plan = store
        .create_plan(crate::stories::NewPlan {
            project: project.to_string_lossy().into_owned(),
            title: "Still open".into(),
            source: "plans/open.md".into(),
        })
        .unwrap();
    let status = act(&project, json!({"action":"backend_status"})).unwrap();
    assert_eq!(status["value"]["availability"], "unavailable");
    assert!(
        status["value"]["error"]
            .as_str()
            .unwrap()
            .contains("still open")
    );
    assert_eq!(store.get_plan(&plan.id).unwrap(), plan);
    let error = story_action(
        project.to_str().unwrap(),
        serde_json::from_value(
            json!({"action":"approve_connector", "installation":"/repo-controlled", "expected_identity":"stale"}),
        )
        .unwrap(),
        Some("managed-agent"),
    )
    .unwrap_err();
    assert!(error.contains("authenticated human UI"));
}

/// Catches: detecting Wiz before approval stranding native plans and archive actions.
#[test]
fn automatic_wiz_marker_retains_open_native_plan_and_native_actions() {
    let bound = bind("unused", &[]);
    let project = bound.project();
    std::fs::remove_file(bound._config.path().join(SETTINGS_FILE)).unwrap();
    let store = StoryStore::open().unwrap();
    let plan = store
        .create_plan(crate::stories::NewPlan {
            project: project.to_string_lossy().into_owned(),
            title: "Native continuity".into(),
            source: "plans/open.md".into(),
        })
        .unwrap();
    std::fs::create_dir(project.join(".wiz")).unwrap();
    let status = act(&project, json!({"action":"backend_status"})).unwrap();
    assert_eq!(status["value"]["backend"], "native");
    assert_eq!(status["value"]["availability"], "available");
    assert!(
        status["value"]["reason"]
            .as_str()
            .unwrap()
            .contains("Native retained while")
    );
    assert_eq!(
        act(&project, json!({"action":"list_plans"})).unwrap()["value"][0]["id"],
        plan.id
    );
    assert_eq!(
        act(&project, json!({"action":"get_plan","plan_id":plan.id})).unwrap()["value"]["title"],
        "Native continuity"
    );
    // Explicit Wiz remains unsafe, but the shared human API can choose native.
    assert!(
        act(
            &project,
            json!({"action":"set_backend","backend":"wiz-stories"})
        )
        .unwrap_err()
        .contains("still open")
    );
    assert_eq!(
        act(&project, json!({"action":"set_backend","backend":"native"})).unwrap()["value"]["provenance"],
        "explicit"
    );
}

/// Catches: newly strict parsing disabling a legacy binding (even via unrelated extra metadata).
#[test]
fn legacy_command_map_tolerates_extra_keys() {
    let bound = bind_reference();
    let project = bound.project();
    let path = bound._config.path().join(SETTINGS_FILE);
    let mut settings: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    settings[project.to_str().unwrap()]["comment"] = json!("previously tolerated");
    settings["/unrelated"] = json!({"command":"unused", "futureKey":true});
    std::fs::write(path, settings.to_string()).unwrap();
    assert_eq!(
        act(&project, json!({"action":"list_plans"})).unwrap()["value"][0]["id"],
        "default"
    );
}

/// Our own protocol reference, narrowed to single-field apply and instrumented
/// at the RPC boundary. This is not a fake/claim about the external Wiz CLI.
fn bind_restricted_reference(flags: &str) -> Bound {
    let script_dir = scratch();
    let source = include_str!("../../../scripts/story-adapter-reference/adapter.mjs")
        .replace(
            "import { createHash }",
            "import { appendFileSync } from 'node:fs';\nimport { createHash }",
        )
        .replace(
            "capabilities, watch_paths:",
            &format!(
                "capabilities: {{ ...capabilities, set_fields: false, {flags} }}, watch_paths:"
            ),
        )
        .replace(
            "get_story({ id }) {",
            "get_story({ id }) { appendFileSync('.rpc-log', 'read\\n');",
        )
        .replace(
            "apply({ id, expected_version, op }) {",
            "apply({ id, expected_version, op }) { appendFileSync('.rpc-log', 'apply\\n');",
        );
    let script = script_dir.path().join("adapter.mjs");
    std::fs::write(&script, source).unwrap();
    // The reference adapter imports its sibling `./store-lock.mjs`.
    std::fs::write(
        script_dir.path().join("store-lock.mjs"),
        include_str!("../../../scripts/story-adapter-reference/store-lock.mjs"),
    )
    .unwrap();
    let bound = bind("node", &[script.to_string_lossy().into_owned()]);
    // initialize before the temporary source is removed; process remains alive.
    story_capabilities_for(bound.project().to_str().unwrap()).unwrap();
    bound
}

fn port_update(bound: &Bound, id: &str, rev: i64, patch: Value) -> Result<(), String> {
    use crate::stories::StoryBackend;
    let backend = super::binding::external_for(&bound.project())?.unwrap();
    let port = super::ExternalStories::new(backend, bound.project());
    port.update_story(id, rev, serde_json::from_value(patch).unwrap())
        .map(|_| ())
}

fn create_edit_fixture(bound: &Bound) -> (String, i64) {
    let created = act(
        &bound.project(),
        json!({"action":"create_story", "input":{
            "planId":"default", "title":"Original", "criteria":["first", "second"],
            "priority":1, "origin":{"type":"native"}, "fileScope":[]
        }}),
    )
    .unwrap();
    (
        created["value"]["id"].as_str().unwrap().into(),
        created["value"]["revision"].as_i64().unwrap(),
    )
}

#[test]
fn restricted_editor_rejects_unsupported_and_multi_op_saves_without_writing_bytes() {
    // Catches: accepting a restricted save as a non-atomic apply_all sequence,
    // mutating a criterion in place, or sending unsupported fields to the backend.
    let bound = bind_restricted_reference("set_title:true, set_priority:true");
    let project = bound.project();
    let caps =
        serde_json::to_value(story_capabilities_for(project.to_str().unwrap()).unwrap()).unwrap();
    assert_eq!(caps["operations"]["update_story"], false);
    assert_eq!(
        caps["operations"]["story_edits"],
        json!({"title":true,"priority":true,"problem":false,"criteria_add":true,"single_change":true})
    );
    let (id, rev) = create_edit_fixture(&bound);
    let bytes = std::fs::read(bound.store_file()).unwrap();
    let log = project.join(".rpc-log");
    for patch in [
        json!({"problem":"unsupported"}),
        json!({"storyType":"fix"}),
        json!({"risk":"high"}),
        json!({"fileScope":["src/x"]}),
        json!({"origin":{"type":"native"}}),
        json!({"dependencies":[]}),
        json!({"title":"Changed", "priority":2}),
        json!({"title":"Changed", "criteria":["first","second","third"]}),
        json!({}),
    ] {
        let reads_before = std::fs::read(&log).unwrap_or_default();
        let error = port_update(&bound, &id, rev, patch.clone()).unwrap_err();
        assert!(
            error.starts_with("story backend does not support "),
            "{patch}: {error}"
        );
        assert_eq!(
            std::fs::read(&log).unwrap_or_default(),
            reads_before,
            "unsupported patch must refuse before read: {patch}"
        );
        assert_eq!(std::fs::read(bound.store_file()).unwrap(), bytes, "{patch}");
    }
    for criteria in [
        json!(["first", "second", "third", "fourth"]),
        json!(["second", "first"]),
        json!(["first"]),
        json!(["edited", "second"]),
        json!(["first", "second"]),
    ] {
        let error = act(&project,json!({"action":"update_story","story_id":id,"expected_revision":rev,"patch":{"criteria":criteria}})).unwrap_err();
        assert!(
            error.starts_with("story backend does not support "),
            "{criteria}: {error}"
        );
        assert_eq!(
            std::fs::read(bound.store_file()).unwrap(),
            bytes,
            "{criteria}"
        );
    }
    assert!(
        !std::fs::read_to_string(&log)
            .unwrap_or_default()
            .contains("apply")
    );
    let mut rev = rev;
    for patch in [
        json!({"title":"New title"}),
        json!({"priority":2}),
        json!({"criteria":["first","second","third"]}),
    ] {
        let before = std::fs::read_to_string(&log)
            .unwrap_or_default()
            .matches("apply")
            .count();
        let reply = act(
            &project,
            json!({"action":"update_story","story_id":id,"expected_revision":rev,"patch":patch}),
        )
        .unwrap();
        rev = reply["value"]["revision"].as_i64().unwrap();
        assert_eq!(
            std::fs::read_to_string(&log)
                .unwrap()
                .matches("apply")
                .count(),
            before + 1,
            "exactly one apply: {patch}"
        );
    }
    let stored: Value =
        serde_json::from_slice(&std::fs::read(bound.store_file()).unwrap()).unwrap();
    assert_eq!(stored["stories"][0]["title"], "New title");
    assert_eq!(stored["stories"][0]["priority"], 2);
    assert_eq!(
        stored["stories"][0]["criteria"].as_array().unwrap().len(),
        3
    );
}

#[test]
fn absent_individual_flags_do_not_grant_external_edits_and_legacy_keeps_multi_op_saves() {
    // Catches: absent optional flags granting writes, or a compatibility rewrite
    // accidentally limiting the existing aggregate adapter to a single change.
    {
        let bound = bind_restricted_reference("");
        let caps = serde_json::to_value(
            story_capabilities_for(bound.project().to_str().unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(caps["operations"]["story_edits"]["title"], false);
        assert_eq!(caps["operations"]["story_edits"]["priority"], false);
        let (id, rev) = create_edit_fixture(&bound);
        let bytes = std::fs::read(bound.store_file()).unwrap();
        for patch in [json!({"title":"No"}), json!({"priority":2})] {
            assert!(act(&bound.project(),json!({"action":"update_story","story_id":id,"expected_revision":rev,"patch":patch})).is_err());
            assert_eq!(std::fs::read(bound.store_file()).unwrap(), bytes);
        }
    }
    let bound = bind_reference();
    let caps =
        serde_json::to_value(story_capabilities_for(bound.project().to_str().unwrap()).unwrap())
            .unwrap();
    assert_eq!(
        caps["operations"]["story_edits"],
        json!({"title":true,"priority":true,"problem":true,"criteria_add":true,"single_change":false})
    );
    let (id, rev) = create_edit_fixture(&bound);
    let result = act(&bound.project(),json!({"action":"update_story","story_id":id,"expected_revision":rev,"patch":{
        "title":"Combined", "priority":3,"problem":"Full contract", "criteria":["first","second","third","fourth"]
    }})).unwrap();
    assert_eq!(result["value"]["title"], "Combined");
    assert_eq!(result["value"]["priority"], 3);
    assert_eq!(result["value"]["criteria"].as_array().unwrap().len(), 4);
}

#[test]
#[ignore = "requires explicit TUIC_TEST_WIZ_CONNECTOR path to the approved checkout; no live data"]
fn actual_wiz_process_preserves_individual_edit_capability_mapping() {
    // Catches: TUIC decoding/mapping the real connector's individual fields as
    // an aggregate full editor, or dropping its supported title/priority edits.
    let entry = std::env::var("TUIC_TEST_WIZ_CONNECTOR").expect("explicit approved connector path");
    let cli = Path::new(&entry).parent().unwrap().join("stories-cli.js");
    assert!(
        cli.is_file(),
        "approved connector must have its real adjacent CLI"
    );
    let bound = bind("node", &[entry]);
    let project = bound.project();
    let run = |program: &str, args: &[&str]| {
        let output = std::process::Command::new(program)
            .args(args)
            .current_dir(&project)
            .env("HOME", &project)
            .env("CLAUDE_CONFIG_DIR", project.join(".claude"))
            .env("SESSION_ID", "")
            .env("NODE_OPTIONS", "")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{program} {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    };
    run("git", &["init", "-q"]);
    std::fs::create_dir(project.join(".wiz")).unwrap();
    std::fs::write(
        project.join(".wiz/features.json"),
        r#"{"proof":false,"tdd":false}"#,
    )
    .unwrap();
    std::fs::create_dir(project.join(".claude")).unwrap();
    std::fs::write(
        project.join(".claude/settings.local.json"),
        r#"{"wiz":{"storiesDirectory":"stories","plansDirectory":"plans"}}"#,
    )
    .unwrap();
    // Same real CLI fixture seam as the recorded compatibility probe; never seed Wiz state by hand.
    let id = run(
        "node",
        &[
            cli.to_str().unwrap(),
            "create",
            r#"{"title":"Disposable Rust boundary story","priority":"P2","type":"task","problem":"Isolated Rust connector test","criteria":["An unmet acceptance criterion"]}"#,
            "--silent",
        ],
    );
    let caps =
        serde_json::to_value(story_capabilities_for(bound.project().to_str().unwrap()).unwrap())
            .unwrap();
    assert_eq!(caps["backend"], "external:wiz-stories");
    assert_eq!(caps["operations"]["update_story"], false);
    assert_eq!(
        caps["operations"]["story_edits"],
        json!({"title":true,"priority":true,"problem":false,"criteria_add":true,"single_change":true})
    );
    let backend = super::binding::external_for(&project).unwrap().unwrap();
    let raw_before = backend.get_story(&id).unwrap();
    assert_eq!(raw_before.title, "Disposable Rust boundary story");
    assert_eq!(raw_before.version.len(), 64);
    let before = act(&project, json!({"action":"get_story", "story_id":id})).unwrap();
    let revision = before["value"]["revision"].as_i64().unwrap();
    let updated = act(
        &project,
        json!({"action":"update_story", "story_id":id,
        "expected_revision":revision, "patch":{"title":"Updated through Rust"}}),
    )
    .unwrap();
    assert_eq!(updated["value"]["title"], "Updated through Rust");
    let raw_after = backend.get_story(&id).unwrap();
    assert_eq!(raw_after.title, "Updated through Rust");
    assert_ne!(raw_after.version, raw_before.version);
    let read_back = act(&project, json!({"action":"get_story", "story_id":id})).unwrap();
    assert_eq!(read_back["value"], updated["value"]);
    let bytes = || -> std::collections::BTreeMap<_, _> {
        let root = project.join("stories");
        let mut pending = vec![root.clone()];
        let mut files = std::collections::BTreeMap::new();
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    pending.push(path);
                } else {
                    files.insert(
                        path.strip_prefix(&root).unwrap().to_path_buf(),
                        std::fs::read(path).unwrap(),
                    );
                }
            }
        }
        files
    };
    let after_update = bytes();
    let stale = backend
        .apply(
            &id,
            &raw_before.version,
            &super::ApplyOp::SetFields {
                title: Some("Stale raw write".into()),
                priority: None,
                problem: None,
            },
        )
        .unwrap_err();
    assert!(matches!(stale, super::AdapterError::Conflict));
    assert_eq!(bytes(), after_update);
    let stale = act(
        &project,
        json!({"action":"update_story", "story_id":id,
        "expected_revision":revision, "patch":{"title":"Stale port write"}}),
    )
    .unwrap_err();
    assert!(stale.starts_with("stale story revision"), "{stale}");
    assert_eq!(bytes(), after_update);
    let refused = act(
        &project,
        json!({"action":"update_story", "story_id":id,
        "expected_revision":updated["value"]["revision"],
        "patch":{"title":"Must not partially save", "priority":1}}),
    )
    .unwrap_err();
    assert!(refused.contains("does not support"), "{refused}");
    assert_eq!(bytes(), after_update);
    assert_eq!(backend.get_story(&id).unwrap().version, raw_after.version);
    backend.shutdown();
}
