//! Which story backend a project uses, and the running adapter for it.
//!
//! Trust rule: TUIC starts only commands in its own legacy settings or an
//! installed Wiz connector pinned by human approval in story-connectors.json.
//! Repository markers select approved code; repository files never name code.
//!
//! ```json
//! {"/abs/project": {"command": "node", "args": ["/abs/adapter.mjs"]}}
//! ```

use super::selection::BackendStatus;
use super::{AdapterCommand, ChangeCursor, Changes, ExternalBackend, lock};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::Duration;

pub(crate) const SETTINGS_FILE: &str = "story-backends.json";

#[derive(Deserialize)]
struct Setting {
    backend: Option<String>,
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
}

struct Bound {
    command: AdapterCommand,
    generation: Option<String>,
    backend: Arc<ExternalBackend>,
    cursor: Option<ChangeCursor>,
}

static BOUND: LazyLock<Mutex<HashMap<PathBuf, Bound>>> = LazyLock::new(Default::default);

/// The adapter command the settings bind to `project`, if any. A settings file
/// that cannot be read or parsed is an error, never "native": a broken binding
/// must not silently show the SQLite stories instead.
fn configured(project: &Path) -> Result<Option<Setting>, String> {
    let path = crate::config::config_dir().join(SETTINGS_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "story backend unavailable: {}: {error}",
                path.display()
            ));
        }
    };
    let settings: HashMap<PathBuf, Setting> = serde_json::from_str(&text)
        .map_err(|error| format!("story backend unavailable: {}: {error}", path.display()))?;
    let mut matching = settings
        .into_iter()
        .filter(|(key, _)| std::fs::canonicalize(key).is_ok_and(|key| key == project));
    let setting = matching.next().map(|(_, setting)| setting);
    if matching.next().is_some() {
        return Err("story backend unavailable: duplicate project overrides".into());
    }
    Ok(setting)
}

fn selected(project: &Path) -> Result<(BackendStatus, Option<AdapterCommand>), String> {
    let Some(setting) = configured(project)? else {
        // Marker detection is passive. Unmarked native projects need no retention reason.
        let Some(config) = super::selection::detection_config(project)? else {
            return Ok((BackendStatus::native(false), None));
        };
        // Continuity precedes connector resolution/approval, never a failure fallback.
        if let Some(reason) = native_continuity(project)? {
            let mut status = BackendStatus::native(false);
            status.reason = format!("Native retained while {reason}");
            return Ok((status, None));
        }
        return super::selection::automatic_detected(project, &config);
    };
    match (setting.backend.as_deref(), setting.command) {
        (Some("native"), None) if setting.args.is_empty() => {
            Ok((BackendStatus::native(true), None))
        }
        (Some("wiz-stories"), None) if setting.args.is_empty() => {
            super::selection::automatic(project, true)
        }
        (None, Some(program)) if !program.trim().is_empty() => {
            Ok(legacy_command(program, setting.args))
        }
        _ => Err("story backend unavailable: invalid explicit backend binding".into()),
    }
}

fn legacy_command(program: String, args: Vec<String>) -> (BackendStatus, Option<AdapterCommand>) {
    (
        BackendStatus {
            backend: "external".into(),
            label: "External connector".into(),
            provenance: "explicit".into(),
            reason: "Legacy command binding".into(),
            availability: "available".into(),
            installation: None,
            version: None,
            error: None,
            generation: Some(serde_json::json!([program, args]).to_string()),
        },
        Some(AdapterCommand { program, args }),
    )
}

/// Status remains readable when approval, the native-plan guard or initialization fails.
pub(crate) fn status(project: &Path) -> BackendStatus {
    let (mut status, command) = match selected(project) {
        Ok(selection) => selection,
        Err(error) => {
            return BackendStatus {
                backend: "unknown".into(),
                label: "Story backend".into(),
                provenance: "explicit".into(),
                reason: "Invalid backend configuration".into(),
                availability: "unavailable".into(),
                installation: None,
                version: None,
                error: Some(error),
                generation: None,
            };
        }
    };
    if status.backend == "native"
        && let Err(error) = external_for(project)
    {
        status.unavailable(error);
    }
    if command.is_some() {
        match external_for(project).and_then(|backend| {
            backend
                .ok_or_else(|| "Story backend changed during status lookup; refresh".to_string())?
                .info()
                .map_err(|e| e.to_string())
        }) {
            Ok(info) => {
                if status.backend == "external:wiz-stories" && info.adapter.id != "wiz-stories" {
                    status.unavailable(
                        "Installed Wiz connector returned a different adapter identity".into(),
                    );
                } else {
                    status.label = if info.adapter.id == "wiz-stories" {
                        "Wiz".into()
                    } else {
                        format!("External: {}", info.adapter.id)
                    };
                    status.backend = format!("external:{}", info.adapter.id);
                    status.version = Some(info.adapter.version);
                }
            }
            Err(error) => status.unavailable(error),
        }
    }
    status
}

/// The external backend of a canonical project path, or `None` for the native store.
pub(crate) fn external_for(project: &Path) -> Result<Option<Arc<ExternalBackend>>, String> {
    let (status, command) = selected(project)?;
    if status.backend != "native" && command.is_none() {
        return Err(format!(
            "story backend unavailable: {}",
            status.error.as_deref().unwrap_or(&status.reason)
        ));
    }
    let Some(command) = command else {
        if let Some(old) = lock(&BOUND).remove(project) {
            old.backend.shutdown();
        }
        return Ok(None);
    };
    let existing = lock(&BOUND).get(project).map(|entry| {
        (
            entry.command.clone(),
            entry.generation.clone(),
            Arc::clone(&entry.backend),
        )
    });
    if let Some((previous, generation, backend)) = existing
        && previous == command
        && generation == status.generation
    {
        return checked_backend(&status, backend);
    }
    // Checked when the binding starts: once bound, no native plan can be created.
    require_no_open_native_plan(project)?;
    let mut bound = lock(&BOUND);
    if let Some(existing) = bound.get(project) {
        if existing.command == command && existing.generation == status.generation {
            let backend = Arc::clone(&existing.backend);
            drop(bound);
            return checked_backend(&status, backend);
        }
        // The settings name a different command: stop the old adapter.
        existing.backend.shutdown();
    }
    let backend = Arc::new(ExternalBackend::new(command.clone(), project));
    bound.insert(
        project.to_path_buf(),
        Bound {
            command,
            generation: status.generation.clone(),
            backend: Arc::clone(&backend),
            cursor: None,
        },
    );
    drop(bound);
    checked_backend(&status, backend)
}

fn checked_backend(
    status: &BackendStatus,
    backend: Arc<ExternalBackend>,
) -> Result<Option<Arc<ExternalBackend>>, String> {
    if status.backend == "external:wiz-stories" {
        let info = backend.info().map_err(|e| e.to_string())?;
        if info.adapter.id != "wiz-stories"
            || status.version.as_deref() != Some(&info.adapter.version)
        {
            return Err("story backend unavailable: Wiz connector identity/version differs from approved installation".into());
        }
    }
    Ok(Some(backend))
}

fn native_continuity(project: &Path) -> Result<Option<String>, String> {
    let store = crate::stories::StoryStore::open()?;
    for plan in store.list_plans(&project.to_string_lossy())? {
        if store.plan_state(&plan.id)? != crate::stories::PlanState::Done {
            return Ok(Some(format!(
                "native plan \"{}\" ({}) is still open",
                plan.title, plan.id
            )));
        }
    }
    Ok(None)
}

fn require_no_open_native_plan(project: &Path) -> Result<(), String> {
    if let Some(reason) = native_continuity(project)? {
        return Err(format!(
            "story backend unavailable: {reason}; complete or archive native work before this project uses an external backend"
        ));
    }
    Ok(())
}

/// Human-only shared action calls this after authenticating the caller.
pub(crate) fn set_override(project: &Path, backend: &str) -> Result<(), String> {
    if !matches!(backend, "automatic" | "native" | "wiz-stories") {
        return Err("Invalid backend override".into());
    }
    if backend == "wiz-stories" {
        require_no_open_native_plan(project)?;
    }
    let path = crate::config::config_dir().join(SETTINGS_FILE);
    let mut settings: serde_json::Map<String, serde_json::Value> = match std::fs::read(&path) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|e| format!("Invalid backend settings: {e}"))?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Default::default(),
        Err(e) => return Err(e.to_string()),
    };
    settings.retain(|key, _| !std::fs::canonicalize(key).is_ok_and(|key| key == project));
    if backend != "automatic" {
        settings.insert(
            project.to_string_lossy().into_owned(),
            serde_json::json!({"backend":backend}),
        );
    }
    crate::fs::atomic_write(
        &path,
        &serde_json::to_vec_pretty(&settings).map_err(|e| e.to_string())?,
    )
}

/// The repository watcher's hook. Costs one lock when no adapter is bound; the
/// adapter call runs off the watcher thread because it can wait for the timeout.
pub(crate) fn watcher_event(
    rt: &tokio::runtime::Handle,
    repo: &Path,
    paths: &[PathBuf],
    bus: &tokio::sync::broadcast::Sender<crate::state::AppEvent>,
) {
    if lock(&BOUND).is_empty() {
        return;
    }
    let (repo, paths, bus) = (repo.to_path_buf(), paths.to_vec(), bus.clone());
    rt.spawn_blocking(move || {
        if let Ok(project) = std::fs::canonicalize(&repo) {
            paths_changed(&project, &paths, &bus);
        }
    });
}

/// Called with every changed path of a canonical project. When a path is under
/// a watch path of the project's running adapter, asks the adapter for `changes`
/// and publishes the answer. Never starts an adapter: one that is not running has
/// no watch paths yet.
pub(crate) fn paths_changed(
    project: &Path,
    paths: &[PathBuf],
    bus: &tokio::sync::broadcast::Sender<crate::state::AppEvent>,
) {
    let (backend, cursor) = {
        let bound = lock(&BOUND);
        let Some(entry) = bound.get(project) else {
            return;
        };
        let Some(watch) = entry.backend.running_watch_paths() else {
            return;
        };
        if !paths
            .iter()
            .any(|path| watch.iter().any(|root| path.starts_with(root)))
        {
            return;
        }
        (Arc::clone(&entry.backend), entry.cursor.clone())
    };
    let payload = match poll(project, &backend, cursor) {
        Ok(payload) => payload,
        Err(error) => serde_json::json!({ "rescan": true, "ids": [], "error": error.to_string() }),
    };
    publish(bus, project, payload);
}

/// Asks the adapter for `changes` after `cursor` and remembers the new cursor.
fn poll(
    project: &Path,
    backend: &Arc<ExternalBackend>,
    cursor: Option<ChangeCursor>,
) -> Result<serde_json::Value, super::AdapterError> {
    Ok(match backend.changes(cursor.as_ref())? {
        Changes::Changed { ids, cursor } => {
            remember(project, backend, cursor);
            serde_json::json!({ "rescan": false, "ids": ids })
        }
        Changes::RescanRequired { cursor } => {
            remember(project, backend, cursor);
            serde_json::json!({ "rescan": true, "ids": [] })
        }
    })
}

fn publish(
    bus: &tokio::sync::broadcast::Sender<crate::state::AppEvent>,
    project: &Path,
    payload: serde_json::Value,
) {
    let _ = bus.send(crate::state::AppEvent::StoriesChanged {
        repo_path: project.to_string_lossy().into_owned(),
        payload,
    });
}

/// A poll answer worth an event: a rescan or at least one changed id.
fn has_news(payload: &serde_json::Value) -> bool {
    payload["rescan"] == true || payload["ids"].as_array().is_some_and(|ids| !ids.is_empty())
}

/// The bus of the running daemon, set once by [`spawn_poller`].
static BUS: OnceLock<tokio::sync::broadcast::Sender<crate::state::AppEvent>> = OnceLock::new();

const POLL_INTERVAL: Duration = Duration::from_secs(30);

/// Polls `changes` of every running adapter on a timer, for backends that send
/// no notification and declare no watch path. Never starts an adapter.
pub(crate) fn spawn_poller(bus: tokio::sync::broadcast::Sender<crate::state::AppEvent>) {
    if BUS.set(bus.clone()).is_err() {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("story-adapter-poll".into())
        .spawn(move || {
            loop {
                std::thread::sleep(POLL_INTERVAL);
                poll_running(&bus);
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(target: "story_adapter", "story adapter poller not started: {error}");
    }
}

fn poll_running(bus: &tokio::sync::broadcast::Sender<crate::state::AppEvent>) {
    let running: Vec<_> = lock(&BOUND)
        .iter()
        .filter(|(_, entry)| entry.backend.running_watch_paths().is_some())
        .map(|(project, entry)| {
            (
                project.clone(),
                Arc::clone(&entry.backend),
                entry.cursor.clone(),
            )
        })
        .collect();
    for (project, backend, cursor) in running {
        match poll(&project, &backend, cursor) {
            Ok(payload) if has_news(&payload) => publish(bus, &project, payload),
            Ok(_) => {}
            Err(error) => publish(
                bus,
                &project,
                serde_json::json!({ "rescan": true, "ids": [], "error": error.to_string() }),
            ),
        }
    }
}

fn remember(project: &Path, backend: &Arc<ExternalBackend>, cursor: ChangeCursor) {
    if let Some(entry) = lock(&BOUND)
        .get_mut(project)
        .filter(|entry| Arc::ptr_eq(&entry.backend, backend))
    {
        entry.cursor = Some(cursor);
    }
}

#[cfg(test)]
mod selection_regressions {
    use super::*;

    /// Catches: claiming native was retained for Wiz on an ordinary unmarked project.
    #[test]
    fn plain_open_native_plan_has_no_markers_reason_until_markers_are_detected() {
        let config = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
        let project_dir = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
        let project = std::fs::canonicalize(project_dir.path()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let plan = crate::stories::StoryStore::open()
            .unwrap()
            .create_plan(crate::stories::NewPlan {
                project: project.to_string_lossy().into_owned(),
                title: "Open native".into(),
                source: "plan.md".into(),
            })
            .unwrap();
        let plain = status(&project);
        assert_eq!(plain.backend, "native");
        assert_eq!(plain.reason, "No Wiz project markers");
        std::fs::create_dir(project.join(".wiz")).unwrap();
        let marked = status(&project);
        assert_eq!(marked.backend, "native");
        assert_eq!(marked.availability, "available");
        assert!(
            marked
                .reason
                .starts_with("Native retained while native plan"),
            "{}",
            marked.reason
        );
        assert!(marked.reason.contains(&plan.id));
        // A missing/invalid connector installation never needs to execute to
        // preserve access to the original native work.
        assert!(external_for(&project).unwrap().is_none());
    }
}
