use super::*;
use serde_json::json;

struct Fixture {
    _dir: tempfile::TempDir,
    _guard: Box<dyn std::any::Any>,
    project: PathBuf,
    config: PathBuf,
    settings: PathBuf,
    install: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
        let base = std::fs::canonicalize(dir.path()).unwrap();
        let (project, config, settings, install) = (
            base.join("project"),
            base.join("claude"),
            base.join("tuic"),
            base.join("installed-wiz"),
        );
        for path in [
            &project,
            &config.join("plugins"),
            &settings,
            &install.join("scripts"),
        ] {
            std::fs::create_dir_all(path).unwrap();
        }
        // Recorded from installed_plugins.json on 2026-11-11; only installPath is relocated.
        let mut registry: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/wiz-installed.json")).unwrap();
        registry["plugins"]["wiz@lansweeper-tools"][0]["installPath"] = json!(install);
        std::fs::write(
            config.join("plugins/installed_plugins.json"),
            registry.to_string(),
        )
        .unwrap();
        // Selection tests never execute this content or simulate the Wiz protocol.
        std::fs::write(
            install.join(ENTRY),
            "connector approval fingerprint fixture",
        )
        .unwrap();
        let guard = Box::new(crate::config::set_config_dir_override(settings.clone()));
        Self {
            _dir: dir,
            _guard: guard,
            project,
            config,
            settings,
            install,
        }
    }
    fn resolve(&self) -> (BackendStatus, Option<AdapterCommand>) {
        resolve(&self.project, &self.config, &self.settings, false).unwrap()
    }
    fn marker(&self) {
        std::fs::create_dir(self.project.join(".wiz")).unwrap();
    }
    fn approve(&self) {
        approve_at(
            &self.project,
            &self.install,
            self.resolve().0.generation.as_deref().unwrap(),
            &self.config,
            &self.settings,
        )
        .unwrap();
    }
}

#[test]
fn generic_stories_folder_and_repository_commands_do_not_select_or_execute_a_connector() {
    let f = Fixture::new();
    std::fs::create_dir(f.project.join("stories")).unwrap();
    std::fs::write(
        f.project.join("stories/README.md"),
        "ordinary documentation",
    )
    .unwrap();
    std::fs::write(
        f.project.join("story-backends.json"),
        json!({"command":"malicious-repo-code"}).to_string(),
    )
    .unwrap();
    let (status, command) = f.resolve();
    assert_eq!(status.backend, "native");
    assert!(command.is_none());
    f.marker();
    let (status, command) = f.resolve();
    assert_eq!(status.backend, "external:wiz-stories");
    assert_eq!(status.availability, "approval_required");
    assert!(command.is_none());
    assert!(!f.settings.join(APPROVAL_FILE).exists());
}

#[test]
fn detected_missing_broken_or_invalid_connector_never_becomes_native() {
    let f = Fixture::new();
    f.marker();
    std::fs::remove_file(f.install.join(ENTRY)).unwrap();
    let (status, command) = f.resolve();
    assert_eq!(status.label, "Wiz");
    assert_eq!(status.availability, "unavailable");
    assert!(command.is_none());
    assert!(status.error.unwrap().contains("connector missing"));
    std::fs::write(f.install.join(ENTRY), "fixture").unwrap();
    std::fs::write(f.settings.join(APPROVAL_FILE), "broken JSON").unwrap();
    let (status, command) = f.resolve();
    assert_eq!(status.backend, "external:wiz-stories");
    assert_eq!(status.availability, "unavailable");
    assert!(command.is_none());
}

#[test]
fn approval_is_global_but_script_and_installation_drift_require_new_approval() {
    let f = Fixture::new();
    f.marker();
    f.approve();
    let (status, command) = f.resolve();
    assert_eq!(status.availability, "available");
    let command = command.unwrap();
    assert!(Path::new(&command.program).is_absolute());
    assert_eq!(command.args, vec![f.install.join(ENTRY).to_string_lossy()]);
    let second = f.project.with_file_name("second-project");
    std::fs::create_dir_all(second.join(".wiz")).unwrap();
    assert!(
        resolve(&second, &f.config, &f.settings, false)
            .unwrap()
            .1
            .is_some()
    );
    std::fs::write(f.install.join(ENTRY), "changed entry bytes").unwrap();
    let (status, command) = f.resolve();
    assert_eq!(status.availability, "approval_required");
    assert!(command.is_none());
    assert!(approve_at(&f.project, &f.project, "stale", &f.config, &f.settings).is_err());
}

#[test]
fn linked_worktree_uses_main_store_and_validates_story_directory_setting() {
    let f = Fixture::new();
    let main = f.project.with_file_name("main");
    let metadata = main.join(".git/worktrees/feature");
    std::fs::create_dir_all(&metadata).unwrap();
    std::fs::write(
        f.project.join(".git"),
        format!("gitdir: {}\n", metadata.display()),
    )
    .unwrap();
    std::fs::write(metadata.join("commondir"), "../..\n").unwrap();
    std::fs::create_dir_all(main.join(".claude")).unwrap();
    std::fs::write(
        main.join(".claude/settings.local.json"),
        json!({"wiz":{"storiesDirectory":"work/stories"}}).to_string(),
    )
    .unwrap();
    std::fs::create_dir_all(main.join("work/stories")).unwrap();
    std::fs::write(
        main.join("work/stories/146-204a-in_progress-p2-versioned-writes.md"),
        "data",
    )
    .unwrap();
    assert_eq!(f.resolve().0.backend, "external:wiz-stories");
    std::fs::write(
        main.join(".claude/settings.local.json"),
        json!({"wiz":{"storiesDirectory":"../outside"}}).to_string(),
    )
    .unwrap();
    assert_eq!(f.resolve().0.backend, "native");
}

/// Catches: an empty Wiz main store becoming native when viewed through a worktree.
#[test]
fn linked_worktree_detects_main_marker_even_without_story_files() {
    let f = Fixture::new();
    let main = f.project.with_file_name("main-empty");
    let metadata = main.join(".git/worktrees/feature");
    std::fs::create_dir_all(&metadata).unwrap();
    std::fs::create_dir_all(main.join(".wiz")).unwrap();
    std::fs::write(
        f.project.join(".git"),
        format!("gitdir: {}\n", metadata.display()),
    )
    .unwrap();
    std::fs::write(metadata.join("commondir"), "../..\n").unwrap();
    assert_eq!(f.resolve().0.backend, "external:wiz-stories");
    assert!(f.resolve().1.is_none());
    // The main checkout is repository-controlled too, not an installed-code authority.
    std::fs::create_dir_all(main.join("scripts")).unwrap();
    std::fs::write(main.join(ENTRY), "repository-owned connector").unwrap();
    let registry_path = f.config.join("plugins/installed_plugins.json");
    let mut registry = json(&registry_path).unwrap();
    registry["plugins"]["wiz@lansweeper-tools"][0]["installPath"] = json!(main);
    std::fs::write(registry_path, registry.to_string()).unwrap();
    let (status, command) = f.resolve();
    assert_eq!(status.availability, "unavailable");
    assert!(command.is_none());
    assert!(
        approve_at(&f.project, &main, "stale", &f.config, &f.settings)
            .unwrap_err()
            .contains("repository-owned")
    );
}

/// Catches: registry version drift reusing approval of an older installation.
#[test]
fn plugin_version_drift_requires_approval_even_when_entry_bytes_are_unchanged() {
    let f = Fixture::new();
    f.marker();
    f.approve();
    let path = f.config.join("plugins/installed_plugins.json");
    let mut registry = json(&path).unwrap();
    registry["plugins"]["wiz@lansweeper-tools"][0]["version"] = json!("changed-version");
    std::fs::write(path, registry.to_string()).unwrap();
    let (status, command) = f.resolve();
    assert_eq!(status.availability, "approval_required");
    assert!(command.is_none());
}

/// Catches: a newly approved update reusing a process running old bytes at the same path.
#[test]
fn reapproved_entry_changes_binding_generation_without_changing_argv() {
    let f = Fixture::new();
    f.marker();
    f.approve();
    let (before, old_command) = f.resolve();
    std::fs::write(f.install.join(ENTRY), "new connector bytes at same path").unwrap();
    f.approve();
    let (after, new_command) = f.resolve();
    assert_eq!(old_command, new_command);
    assert_ne!(before.generation, after.generation);
    assert!(after.generation.is_some());
}

/// Catches: approving new bytes/version that were never displayed to the human.
#[test]
fn approval_rejects_changed_displayed_entry_or_version() {
    for change_version in [false, true] {
        let f = Fixture::new();
        f.marker();
        let displayed = f.resolve().0.generation.unwrap();
        if change_version {
            let path = f.config.join("plugins/installed_plugins.json");
            let mut registry = json(&path).unwrap();
            registry["plugins"]["wiz@lansweeper-tools"][0]["version"] = json!("new-version");
            std::fs::write(path, registry.to_string()).unwrap();
        } else {
            std::fs::write(f.install.join(ENTRY), "unseen bytes").unwrap();
        }
        assert!(
            approve_at(&f.project, &f.install, &displayed, &f.config, &f.settings)
                .unwrap_err()
                .contains("refresh")
        );
        assert!(!f.settings.join(APPROVAL_FILE).exists());
        f.approve(); // Freshly displayed identity is accepted; never a scope autoapproval.
        assert_eq!(f.resolve().0.availability, "available");
    }
}

/// Catches: a legitimate user+project registry array being permanently ambiguous,
/// or an unrelated project's local install shadowing the applicable installation.
#[test]
fn registry_scope_precedence_uses_canonical_owner_without_approving_code() {
    let f = Fixture::new();
    f.marker();
    let path = f.config.join("plugins/installed_plugins.json");
    let mut registry = json(&path).unwrap();
    let user = registry["plugins"]["wiz@lansweeper-tools"][0].clone();
    let mut scoped = user.clone();
    scoped["scope"] = json!("project");
    scoped["projectPath"] = json!(f.project.join("."));
    scoped["version"] = json!("project-version");
    let mut unrelated = scoped.clone();
    unrelated["scope"] = json!("local");
    unrelated["projectPath"] = json!(f.install);
    registry["plugins"]["wiz@lansweeper-tools"] = json!([unrelated, user, scoped]);
    std::fs::write(&path, registry.to_string()).unwrap();
    let (status, command) = f.resolve();
    assert_eq!(status.version.as_deref(), Some("project-version"));
    assert_eq!(status.availability, "approval_required");
    assert!(command.is_none());
    f.approve();
    assert_eq!(f.resolve().0.availability, "available");
    let duplicate = registry["plugins"]["wiz@lansweeper-tools"][2].clone();
    registry["plugins"]["wiz@lansweeper-tools"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    std::fs::write(path, registry.to_string()).unwrap();
    assert!(f.resolve().0.error.unwrap().contains("plugin manager"));
}

/// Catches: managed/anonymous approval writes, or rejecting a real human's pinned entry/version.
#[test]
fn authenticated_human_approves_displayed_entry_but_other_actors_cannot_write() {
    let f = Fixture::new();
    f.marker();
    let displayed = f.resolve().0.generation.unwrap();
    for (session, anonymous) in [
        (Some("managed"), false),
        (None, true),
        (Some("managed"), true),
        (None, false),
    ] {
        let result = crate::stories::human_backend_write(session, anonymous, || {
            approve_at(&f.project, &f.install, &displayed, &f.config, &f.settings)
        });
        if session.is_none() && !anonymous {
            result.unwrap();
            let approval = json(&f.settings.join(APPROVAL_FILE)).unwrap();
            assert_eq!(
                approval[f.install.to_str().unwrap()]["installation"],
                json!(f.install)
            );
            assert_eq!(approval[f.install.to_str().unwrap()]["version"], "5.12.1");
            assert_eq!(f.resolve().0.availability, "available");
        } else {
            assert!(result.unwrap_err().contains("authenticated human"));
            assert!(!f.settings.join(APPROVAL_FILE).exists());
        }
    }
}

/// Catches: approving B removing A, or one installation's approval authorizing another.
#[test]
fn independent_installation_approvals_survive_other_approvals_and_drift() {
    let f = Fixture::new();
    f.marker();
    let project_b = f.project.with_file_name("project-b");
    let install_b = f.install.with_file_name("installed-wiz-b");
    std::fs::create_dir_all(project_b.join(".wiz")).unwrap();
    std::fs::create_dir_all(install_b.join("scripts")).unwrap();
    // Identical bytes/version still require independent installation approval.
    std::fs::copy(f.install.join(ENTRY), install_b.join(ENTRY)).unwrap();
    let path = f.config.join("plugins/installed_plugins.json");
    let mut registry = json(&path).unwrap();
    let mut entry_b = registry["plugins"]["wiz@lansweeper-tools"][0].clone();
    entry_b["scope"] = json!("project");
    entry_b["projectPath"] = json!(project_b);
    entry_b["installPath"] = json!(install_b);
    registry["plugins"]["wiz@lansweeper-tools"]
        .as_array_mut()
        .unwrap()
        .push(entry_b);
    std::fs::write(&path, registry.to_string()).unwrap();
    let resolve_b = || {
        resolve(&project_b, &f.config, &f.settings, false)
            .unwrap()
            .0
    };
    f.approve();
    assert_eq!(resolve_b().availability, "approval_required");
    approve_at(
        &project_b,
        &install_b,
        resolve_b().generation.as_deref().unwrap(),
        &f.config,
        &f.settings,
    )
    .unwrap();
    assert_eq!(f.resolve().0.availability, "available");
    assert_eq!(resolve_b().availability, "available");
    std::fs::write(f.install.join(ENTRY), "changed A bytes").unwrap();
    assert_eq!(f.resolve().0.availability, "approval_required");
    assert_eq!(resolve_b().availability, "available");
    f.approve();
    assert_eq!(resolve_b().availability, "available");
    registry["plugins"]["wiz@lansweeper-tools"][1]["version"] = json!("changed B version");
    std::fs::write(&path, registry.to_string()).unwrap();
    assert_eq!(resolve_b().availability, "approval_required");
    assert_eq!(f.resolve().0.availability, "available");
    let approvals = json(&f.settings.join(APPROVAL_FILE)).unwrap();
    assert_eq!(approvals.as_object().unwrap().len(), 2);
}

/// Catches: dropping runtime trust checks when excluding Node from durable pins.
#[test]
fn repository_owned_node_is_rejected_even_though_runtime_is_not_approval_identity() {
    let f = Fixture::new();
    let node = f.project.join("node");
    std::fs::write(&node, "not executed").unwrap();
    assert!(
        trusted_node(&node, &f.project)
            .unwrap_err()
            .contains("Node runtime must not be repository-owned")
    );
    let external_node = f.install.join("node");
    std::fs::write(&external_node, "not executed").unwrap();
    assert_eq!(
        trusted_node(&external_node, &f.project).unwrap(),
        std::fs::canonicalize(external_node).unwrap()
    );
}
