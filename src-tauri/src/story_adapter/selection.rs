//! Passive Wiz discovery and machine-owned approval. Repository data never names code.
use super::AdapterCommand;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// Serialize approval read/modify/write so independent human approvals are not lost.
static APPROVAL_WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());
type Approvals = BTreeMap<PathBuf, Approval>;

const APPROVAL_FILE: &str = "story-connectors.json";
const ENTRY: &str = "scripts/tuic-story-connector.js";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Approval {
    installation: PathBuf,
    script: PathBuf,
    digest: String,
    version: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackendStatus {
    pub backend: String,
    pub label: String,
    pub provenance: String,
    pub reason: String,
    pub availability: String,
    pub installation: Option<PathBuf>,
    pub version: Option<String>,
    pub error: Option<String>,
    // Cache identity must change even when an approved update keeps the same argv.
    pub(crate) generation: Option<String>,
}

impl BackendStatus {
    pub(crate) fn native(explicit: bool) -> Self {
        Self {
            backend: "native".into(),
            label: "TUIC native".into(),
            provenance: if explicit { "explicit" } else { "automatic" }.into(),
            reason: if explicit {
                "Native override"
            } else {
                "No Wiz project markers"
            }
            .into(),
            availability: "available".into(),
            installation: None,
            version: None,
            error: None,
            generation: None,
        }
    }
    pub(crate) fn unavailable(&mut self, error: String) {
        self.availability = "unavailable".into();
        self.error = Some(error);
    }
}

fn claude_dir() -> Result<PathBuf, String> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".claude")))
        .ok_or_else(|| "Cannot locate installed Wiz plugins".into())
}

fn json(path: &Path) -> Result<serde_json::Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn installed(config: &Path, project: &Path) -> Result<(PathBuf, String), String> {
    let value = json(&config.join("plugins/installed_plugins.json"))?;
    let entries = value["plugins"]["wiz@lansweeper-tools"]
        .as_array()
        .ok_or("Wiz is not installed")?;
    // Scope chooses data, never executable authority. Claude's supported precedence:
    // local > project > user (docs/en/discover-plugins, Choose an install scope).
    let owner = crate::progress::resolve_owning_project(Some(&project.to_string_lossy()))?;
    let mut candidates = entries
        .iter()
        .filter_map(|entry| {
            let rank = match entry["scope"].as_str()? {
                "user" => 0,
                "project" | "local" => {
                    let path = entry["projectPath"].as_str()?;
                    let candidate = crate::progress::resolve_owning_project(Some(path)).ok()?;
                    if candidate != owner {
                        return None;
                    }
                    if entry["scope"] == "local" { 2 } else { 1 }
                }
                _ => return None,
            };
            Some((rank, entry))
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(rank, _)| std::cmp::Reverse(*rank));
    let Some((rank, entry)) = candidates.first() else {
        return Err("No supported Wiz installation applies to this project; install Wiz at user scope in Claude's plugin manager".into());
    };
    if candidates.get(1).is_some_and(|(next, _)| next == rank) {
        return Err("Wiz has multiple installations at the same scope; select one installation in Claude's plugin manager (uninstall the duplicate), then refresh".into());
    }
    let path = entry["installPath"]
        .as_str()
        .ok_or("Wiz installPath is missing")?;
    if !Path::new(path).is_absolute() {
        return Err("Wiz installPath must be absolute".into());
    }
    let root = std::fs::canonicalize(path).map_err(|e| format!("Wiz installation: {e}"))?;
    let version = entry["version"].as_str().ok_or("Wiz version is missing")?;
    Ok((root, version.into()))
}

fn script(root: &Path) -> Result<(PathBuf, String), String> {
    let path = std::fs::canonicalize(root.join(ENTRY))
        .map_err(|e| format!("Wiz connector missing: {e}"))?;
    if !path.starts_with(root) {
        return Err("Wiz connector escapes installation".into());
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("Wiz connector: {e}"))?;
    Ok((path, hex::encode(Sha256::digest(bytes))))
}

/// Linked worktree metadata is data, not an executable. Wiz stores stories in main.
fn store_root(project: &Path) -> PathBuf {
    let git = project.join(".git");
    let Some(gitdir) = std::fs::read_to_string(git).ok().and_then(|text| {
        text.trim()
            .strip_prefix("gitdir:")
            .map(str::trim)
            .map(str::to_owned)
    }) else {
        return project.into();
    };
    let gitdir = project.join(gitdir);
    let Some(common) = std::fs::read_to_string(gitdir.join("commondir")).ok() else {
        return project.into();
    };
    std::fs::canonicalize(gitdir.join(common.trim()))
        .ok()
        .and_then(|common| common.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| project.into())
}

// A linked checkout and its shared main checkout are the same repository trust boundary.
fn repository_owned(path: &Path, project: &Path) -> bool {
    path.starts_with(project) || path.starts_with(store_root(project))
}

fn stories_directory(project: &Path, config: &Path) -> PathBuf {
    let setting = json(&project.join(".claude/settings.local.json"))
        .ok()
        .and_then(|v| v["wiz"]["storiesDirectory"].as_str().map(str::to_owned))
        .or_else(|| {
            json(&config.join("settings.json"))
                .ok()
                .and_then(|v| v["wiz"]["storiesDirectory"].as_str().map(str::to_owned))
        })
        .unwrap_or_else(|| "stories".into());
    if crate::fs::is_absolute_on_any_platform(&setting)
        || setting.split(['/', '\\']).any(|part| part == "..")
    {
        return "stories".into();
    }
    setting.into()
}

fn story_filename(name: &str) -> bool {
    static NAME: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(
        r"(?i)^[0-9]+-[a-z0-9]{4,8}-(pending|ready|in_progress|blocked|completed|complete|wontfix)-P[0123]-.+\.md$"
    ).expect("fixed Wiz filename pattern")
    });
    NAME.is_match(name)
}

fn detected(project: &Path, config: &Path) -> bool {
    let root = store_root(project);
    if project.join(".wiz").is_dir() || root.join(".wiz").is_dir() {
        return true;
    }
    let directory = root.join(stories_directory(&root, config));
    std::fs::read_dir(directory).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_file())
                && story_filename(&entry.file_name().to_string_lossy())
        })
    })
}

pub(crate) fn automatic(
    project: &Path,
    force: bool,
) -> Result<(BackendStatus, Option<AdapterCommand>), String> {
    resolve(project, &claude_dir()?, &crate::config::config_dir(), force)
}

fn resolve(
    project: &Path,
    config: &Path,
    settings: &Path,
    force: bool,
) -> Result<(BackendStatus, Option<AdapterCommand>), String> {
    if !force && !detected(project, config) {
        return Ok((BackendStatus::native(false), None));
    }
    resolve_detected(project, config, settings, force)
}

/// Passive discovery result reused by binding before applying native continuity.
pub(super) fn detection_config(project: &Path) -> Result<Option<PathBuf>, String> {
    let config = claude_dir()?;
    Ok(detected(project, &config).then_some(config))
}

pub(super) fn automatic_detected(
    project: &Path,
    config: &Path,
) -> Result<(BackendStatus, Option<AdapterCommand>), String> {
    resolve_detected(project, config, &crate::config::config_dir(), false)
}

fn resolve_detected(
    project: &Path,
    config: &Path,
    settings: &Path,
    force: bool,
) -> Result<(BackendStatus, Option<AdapterCommand>), String> {
    let mut status = BackendStatus {
        backend: "external:wiz-stories".into(),
        label: "Wiz".into(),
        provenance: if force { "explicit" } else { "automatic" }.into(),
        reason: if force {
            "Wiz connector override"
        } else {
            "Wiz project markers detected"
        }
        .into(),
        availability: "approval_required".into(),
        installation: None,
        version: None,
        error: None,
        generation: None,
    };
    let result = (|| {
        let (root, version) = installed(config, project)?;
        status.installation = Some(root.clone());
        status.version = Some(version.clone());
        if repository_owned(&root, project) {
            return Err("Installed connector must not be repository-owned".into());
        }
        let (entry, digest) = script(&root)?;
        status.generation = Some(format!("{version}:{digest}"));
        let approval_path = settings.join(APPROVAL_FILE);
        if !approval_path.exists() {
            return Ok(None);
        }
        let approvals: Approvals = serde_json::from_value(json(&approval_path)?)
            .map_err(|e| format!("Invalid connector approvals: {e}"))?;
        let Some(approval) = approvals.get(&root) else {
            return Ok(None);
        };
        if approval.installation != root
            || approval.script != entry
            || approval.digest != digest
            || approval.version != version
        {
            status.reason = "Installed Wiz connector changed; approval required".into();
            return Ok(None);
        }
        let node =
            crate::plugin_exec::resolve_binary("node").ok_or("Node runtime is not installed")?;
        let node = trusted_node(Path::new(&node), project)?;
        status.generation = Some(format!("{version}:{digest}"));
        status.availability = "available".into();
        Ok(Some(AdapterCommand {
            program: node.to_string_lossy().into_owned(),
            args: vec![entry.to_string_lossy().into_owned()],
        }))
    })();
    match result {
        Ok(command) => Ok((status, command)),
        Err(error) => {
            status.unavailable(error);
            Ok((status, None))
        }
    }
}

// Runtime location is process identity, not durable approval identity. It is
// nevertheless always canonicalized and checked against the repository boundary.
fn trusted_node(node: &Path, project: &Path) -> Result<PathBuf, String> {
    let node = std::fs::canonicalize(node).map_err(|e| format!("Node runtime: {e}"))?;
    if repository_owned(&node, project) {
        return Err("Node runtime must not be repository-owned".into());
    }
    Ok(node)
}

/// The UI must display this exact resolved installation before submitting approval.
/// Re-read discovery to reject a stale UI suggestion. Never starts a process.
pub(crate) fn approve(
    project: &Path,
    installation: &Path,
    expected_identity: &str,
) -> Result<(), String> {
    approve_at(
        project,
        installation,
        expected_identity,
        &claude_dir()?,
        &crate::config::config_dir(),
    )
}

fn approve_at(
    project: &Path,
    installation: &Path,
    expected_identity: &str,
    config: &Path,
    settings: &Path,
) -> Result<(), String> {
    let (root, version) = installed(config, project)?;
    if root != installation || repository_owned(&root, project) {
        return Err(
            "Wiz installation changed or is repository-owned; refresh before approval".into(),
        );
    }
    let (script, digest) = script(&root)?;
    if expected_identity != format!("{version}:{digest}") {
        return Err("Wiz entry/version changed; refresh before approval".into());
    }
    let approval = Approval {
        installation: root,
        script,
        digest,
        version,
    };
    let _guard = super::lock(&APPROVAL_WRITE);
    let path = settings.join(APPROVAL_FILE);
    let mut approvals: Approvals = if path.exists() {
        serde_json::from_value(json(&path)?)
            .map_err(|e| format!("Invalid connector approvals: {e}"))?
    } else {
        BTreeMap::new()
    };
    approvals.insert(approval.installation.clone(), approval);
    let bytes = serde_json::to_vec_pretty(&approvals).map_err(|e| e.to_string())?;
    crate::fs::atomic_write(&path, &bytes)
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
