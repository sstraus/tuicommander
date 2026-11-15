use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Resolve a registered project/workspace path to the project that owns its
/// Progress database. The caller must already have an authoritative registered
/// project; this function deliberately has no focused-repository or CWD fallback.
pub fn resolve_owning_project(registered_project: Option<&str>) -> Result<PathBuf, String> {
    let registered = registered_project
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            "project_required: register an authoritative project before reporting progress"
                .to_string()
        })?;
    let start = canonical_existing_dir(Path::new(registered))?;
    follow_workspace_parents(start, &cached_workspace_parents())
}

#[cfg(test)]
pub(crate) fn resolve_owning_project_in(
    registered_project: &str,
    repositories: &serde_json::Value,
) -> Result<PathBuf, String> {
    let start = canonical_existing_dir(Path::new(registered_project))?;
    follow_workspace_parents(start, &workspace_parents_of(repositories))
}

/// How long a built ownership map may serve calls while `repositories.json` is
/// unchanged. Building it stats every registered repository and workspace (about
/// 150 ms for 58 repositories and 145 workspaces), and the automation reconcile
/// asks on every session state change. The bound covers a checkout that appears
/// or disappears on disk without a registry write.
const OWNERSHIP_MAP_TTL: std::time::Duration = std::time::Duration::from_secs(30);

type WorkspaceParents = std::sync::Arc<HashMap<PathBuf, PathBuf>>;
/// (registry path, mtime, length) of the document the map was built from.
type RegistryStamp = (PathBuf, Option<std::time::SystemTime>, u64);

static OWNERSHIP_MAP: std::sync::Mutex<
    Option<(RegistryStamp, std::time::Instant, WorkspaceParents)>,
> = std::sync::Mutex::new(None);

fn cached_workspace_parents() -> WorkspaceParents {
    let path = crate::config::repository_file();
    let meta = std::fs::metadata(&path).ok();
    let stamp = (
        path,
        meta.as_ref().and_then(|m| m.modified().ok()),
        meta.as_ref().map_or(0, std::fs::Metadata::len),
    );
    let mut cache = OWNERSHIP_MAP
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((cached, built, parents)) = cache.as_ref()
        && *cached == stamp
        && built.elapsed() < OWNERSHIP_MAP_TTL
    {
        return parents.clone();
    }
    let parents = WorkspaceParents::new(workspace_parents_of(&crate::config::load_repositories()));
    *cache = Some((stamp, std::time::Instant::now(), parents.clone()));
    parents
}

fn follow_workspace_parents(
    start: PathBuf,
    workspace_parents: &HashMap<PathBuf, PathBuf>,
) -> Result<PathBuf, String> {
    let mut current = start;
    let mut visited = HashSet::new();
    while let Some(parent) = workspace_parents.get(&current) {
        if !visited.insert(current.clone()) {
            return Err(format!(
                "project_unavailable: managed workspace ownership cycle at '{}'",
                current.display()
            ));
        }
        current = parent.clone();
    }

    canonical_existing_dir(&current)
}

fn workspace_parents_of(repositories: &serde_json::Value) -> HashMap<PathBuf, PathBuf> {
    let mut workspace_parents = HashMap::<PathBuf, PathBuf>::new();
    let Some(repos) = repositories
        .get("repos")
        .and_then(serde_json::Value::as_object)
    else {
        return workspace_parents;
    };
    for (repo_path, repo) in repos {
        // A repository of a remote connection is a path on another host. It owns
        // no local journal, and resolving it here can stall on a local automount
        // (`/home` on macOS costs about 40 ms per lookup).
        if repo.get("connectionId").is_some_and(|id| !id.is_null()) {
            continue;
        }
        // Registrations can repeat the same shared Git inventory from a linked
        // checkout. Infer one main-checkout owner rather than opposite edges.
        let repo_path = Path::new(repo_path);
        let repo_root = tuic_git::git::resolve_git_dir(repo_path)
            .filter(|git_dir| git_dir.join("commondir").is_file())
            .map(|_| tuic_git::git::canonical_repo_root(repo_path))
            .filter(|main| main.join(".git").is_dir())
            // An absorbed submodule's gitdir is administration storage, not a
            // checkout parent. Only infer a main checkout for linked worktrees.
            .unwrap_or_else(|| canonical_if_present(repo_path));
        let Some(workspaces) = repo
            .get("workspaces")
            .and_then(serde_json::Value::as_object)
        else {
            continue;
        };
        for workspace in workspaces.values() {
            let Some(workspace_path) = workspace
                .get("worktreePath")
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            let workspace_path = canonical_if_present(Path::new(workspace_path));
            let parent = workspace
                .get("parentRepoPath")
                .and_then(serde_json::Value::as_str)
                .map(Path::new)
                .map(canonical_if_present)
                .unwrap_or_else(|| repo_root.clone());
            // The main workspace of a repository records the repository itself
            // as its worktree, so it maps the project root onto the project
            // root. That self-edge carries no ownership information, and
            // following it below would look exactly like a two-node cycle —
            // which made every registered project report
            // `project_unavailable` and left the Progress panel showing
            // nothing but errors. Drop it here so the cycle check stays strict
            // for real cycles.
            if workspace_path != parent {
                workspace_parents.insert(workspace_path, parent);
            }
        }
    }
    workspace_parents
}

/// The registered managed workspace (worktree) a path lies in, deepest first.
///
/// A worktree usually sits outside its repository's root, so a terminal
/// running there matches no registered repository by prefix. Its workspace is
/// the authoritative answer: `resolve_owning_project` then maps it to the
/// parent project, which is how a worktree and its repository share one journal.
pub(crate) fn registered_workspace_for_path(path: &Path) -> Option<PathBuf> {
    registered_workspace_for_path_in(path, &crate::config::load_repositories())
}

pub(crate) fn registered_workspace_for_path_in(
    path: &Path,
    repositories: &serde_json::Value,
) -> Option<PathBuf> {
    let path = canonical_if_present(path);
    repositories
        .get("repos")?
        .as_object()?
        .values()
        .filter_map(|repo| repo.get("workspaces")?.as_object())
        .flat_map(|workspaces| workspaces.values())
        .filter_map(|workspace| workspace.get("worktreePath")?.as_str())
        .map(|worktree| canonical_if_present(Path::new(worktree)))
        // `Path::starts_with` compares whole components, so a sibling that
        // merely shares a prefix is not inside the workspace.
        .filter(|worktree| path.starts_with(worktree))
        .max_by_key(|worktree| worktree.components().count())
}

fn canonical_existing_dir(path: &Path) -> Result<PathBuf, String> {
    let canonical = std::fs::canonicalize(path).map_err(|error| {
        format!(
            "project_unavailable: cannot access project root '{}': {error}",
            path.display()
        )
    })?;
    if !canonical.is_dir() {
        return Err(format!(
            "project_unavailable: project root '{}' is not a directory",
            canonical.display()
        ));
    }
    Ok(canonical)
}

fn canonical_if_present(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A worker spawned into a managed worktree runs outside its repository's
    /// root. Its terminal must still resolve to that workspace — and so to the
    /// parent project — or its intents and messages never reach the journal.
    #[test]
    fn a_path_inside_a_managed_workspace_resolves_to_that_workspace() {
        let root = tempfile::tempdir().unwrap();
        let worktree = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(worktree.path().join("src")).unwrap();
        let doc = json!({
            "repos": {
                root.path().to_string_lossy(): {
                    "workspaces": {
                        "w": { "worktreePath": worktree.path(), "parentRepoPath": root.path() }
                    }
                }
            }
        });
        let canonical = std::fs::canonicalize(worktree.path()).unwrap();
        assert_eq!(
            registered_workspace_for_path_in(&worktree.path().join("src"), &doc),
            Some(canonical.clone())
        );
        assert_eq!(
            registered_workspace_for_path_in(worktree.path(), &doc),
            Some(canonical)
        );
        let sibling = format!("{}-other", worktree.path().display());
        assert_eq!(
            registered_workspace_for_path_in(Path::new(&sibling), &doc),
            None
        );
        assert_eq!(
            registered_workspace_for_path_in(Path::new("/tmp"), &json!({})),
            None
        );
    }

    /// Recorded duplicate inventories must not redirect the main checkout to a sibling.
    #[test]
    fn recorded_ego_inventory_does_not_make_main_ownership_cyclic() {
        let (root, document) = recorded_ego_inventory();
        let main = root.path().join("ego");
        assert_eq!(
            resolve_owning_project_in(main.to_str().unwrap(), &document).unwrap(),
            main.canonicalize().unwrap()
        );
    }

    /// A separately registered linked checkout still shares the primary journal.
    #[test]
    fn recorded_ego_linked_registration_does_not_own_a_second_journal() {
        let (root, document) = recorded_ego_inventory();
        let linked = root.path().join("ego__wt/feat-history-anchors");
        assert_eq!(
            resolve_owning_project_in(linked.to_str().unwrap(), &document).unwrap(),
            root.path().join("ego").canonicalize().unwrap()
        );
    }

    /// A sibling listed by two registrations must not inherit the last inventory's owner.
    #[test]
    fn recorded_ego_sibling_inventory_does_not_redirect_progress() {
        let (root, document) = recorded_ego_inventory();
        let sibling = root.path().join("ego__wt/fix-public-candidate-737");
        assert_eq!(
            resolve_owning_project_in(sibling.to_str().unwrap(), &document).unwrap(),
            root.path().join("ego").canonicalize().unwrap()
        );
    }

    fn recorded_ego_inventory() -> (tempfile::TempDir, serde_json::Value) {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let text = include_str!("ownership-ego-2583.json")
            .replace("$ROOT", &root.path().to_string_lossy().replace('\\', "/"));
        let fixture: serde_json::Value = serde_json::from_str(&text).unwrap();
        let document = fixture["repositories"].clone();
        for repo in document["repos"].as_object().unwrap().values() {
            for workspace in repo["workspaces"].as_object().unwrap().values() {
                std::fs::create_dir_all(workspace["worktreePath"].as_str().unwrap()).unwrap();
            }
        }
        std::fs::create_dir_all(root.path().join("ego/.git")).unwrap();
        for (path, contents) in fixture["gitFiles"].as_object().unwrap() {
            let path = Path::new(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents.as_str().unwrap()).unwrap();
        }
        (root, document)
    }

    /// Catches: a registered submodule's journal moving into .git/modules.
    #[test]
    fn a_registered_submodule_keeps_its_checkout_as_journal_owner() {
        use tuic_git::git_cli::git_cmd;

        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let source = root.path().join("source");
        let project = root.path().join("project");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        git_cmd(&source).args(["init"]).run().unwrap();
        git_cmd(&source)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "--allow-empty",
                "-m",
                "submodule fixture",
            ])
            .run()
            .unwrap();
        git_cmd(&project).args(["init"]).run().unwrap();
        git_cmd(&project)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &source.to_string_lossy(),
                "deps/library",
            ])
            .run()
            .unwrap();
        let submodule = project.join("deps/library");
        let document = json!({
            "repos": {
                submodule.to_string_lossy(): {
                    "workspaces": {
                        "main": { "worktreePath": submodule, "parentRepoPath": null }
                    }
                }
            }
        });
        assert_eq!(
            resolve_owning_project_in(submodule.to_str().unwrap(), &document).unwrap(),
            submodule.canonicalize().unwrap(),
            "an absorbed submodule gitdir is not its journal's owning checkout"
        );
    }

    #[test]
    fn unbound_callers_fail_explicitly() {
        assert!(
            resolve_owning_project(None)
                .unwrap_err()
                .starts_with("project_required:")
        );
    }

    #[test]
    fn managed_and_nested_workspaces_resolve_to_the_primary_project() {
        let root = tempfile::tempdir().unwrap();
        let nested = tempfile::tempdir().unwrap();
        let leaf = tempfile::tempdir().unwrap();
        let doc = json!({
            "repos": {
                root.path().to_string_lossy(): {
                    "workspaces": {
                        "nested": {
                            "worktreePath": nested.path(),
                            "parentRepoPath": root.path()
                        }
                    }
                },
                nested.path().to_string_lossy(): {
                    "workspaces": {
                        "leaf": {
                            "worktreePath": leaf.path(),
                            "parentRepoPath": nested.path()
                        }
                    }
                }
            }
        });

        // Ownership canonicalizes, so the owner is the resolved path — on macOS
        // `/var/…` is a symlink to `/private/var/…`.
        let canonical_root = root.path().canonicalize().unwrap();
        let owner = resolve_owning_project_in(&leaf.path().to_string_lossy(), &doc).unwrap();
        assert_eq!(owner, canonical_root);
    }

    #[test]
    fn a_main_workspace_owns_itself() {
        // Every registered repository has a main workspace whose `worktreePath`
        // is the repository root and whose `parentRepoPath` is absent, so the
        // parent falls back to that same root.
        let root = tempfile::tempdir().unwrap();
        let doc = json!({
            "repos": {
                root.path().to_string_lossy(): {
                    "workspaces": {
                        "master": { "worktreePath": root.path(), "isMain": true }
                    }
                }
            }
        });

        assert_eq!(
            resolve_owning_project_in(&root.path().to_string_lossy(), &doc).unwrap(),
            root.path().canonicalize().unwrap()
        );
    }

    /// A COW workspace's journal belongs to the parent project, and reading it
    /// back through the workspace path finds the parent's entries — one journal,
    /// reached from either address.
    ///
    /// The database this used to guard against — a `.tuic/progress.sqlite3`
    /// copied into the workspace by the COW clone — cannot exist any more: the
    /// store writes one file in the config directory and nothing inside a
    /// repository. What is still worth proving is the resolution itself.
    #[test]
    fn a_cow_workspace_reads_and_writes_the_parent_journal() {
        use crate::progress::{NewProgressEntry, ProgressKind, ProgressStore};

        let config = tempfile::tempdir().unwrap();
        let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let root = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let doc = json!({
            "repos": {
                root.path().to_string_lossy(): {
                    "workspaces": {
                        "cow": {
                            "kind": "cow",
                            "worktreePath": workspace.path(),
                            "parentRepoPath": root.path()
                        }
                    }
                }
            }
        });

        let owner = resolve_owning_project_in(&workspace.path().to_string_lossy(), &doc).unwrap();
        assert_eq!(owner, root.path().canonicalize().unwrap());

        let store = ProgressStore::open().unwrap();
        let recorded = store
            .record(
                &owner.to_string_lossy(),
                &NewProgressEntry {
                    kind: ProgressKind::Done,
                    text: "The owning project retains its history.".to_string(),
                    step: None,
                    agent_name: None,
                },
            )
            .unwrap();
        let listed = store
            .list(&owner.to_string_lossy(), &Default::default())
            .unwrap();
        assert_eq!(listed.entries, vec![recorded]);
        assert!(
            !workspace.path().join(".tuic").exists(),
            "nothing is written inside the workspace"
        );
    }

    #[test]
    fn independent_projects_remain_separate() {
        let project = tempfile::tempdir().unwrap();
        assert_eq!(
            resolve_owning_project_in(&project.path().to_string_lossy(), &serde_json::json!({}))
                .unwrap(),
            project.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn ownership_cycles_fail_closed() {
        let one = tempfile::tempdir().unwrap();
        let two = tempfile::tempdir().unwrap();
        let doc = json!({
            "repos": {
                one.path().to_string_lossy(): { "workspaces": { "two": {
                    "worktreePath": two.path(), "parentRepoPath": one.path()
                }}},
                two.path().to_string_lossy(): { "workspaces": { "one": {
                    "worktreePath": one.path(), "parentRepoPath": two.path()
                }}}
            }
        });
        assert!(
            resolve_owning_project_in(&one.path().to_string_lossy(), &doc)
                .unwrap_err()
                .contains("ownership cycle")
        );
    }

    // Catches: a repository of a remote connection taking part in local ownership,
    // which both redirects a local workspace and stats a path on another host.
    #[test]
    fn a_remote_connection_repository_does_not_own_a_local_workspace() {
        let root = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let doc = json!({
            "repos": {
                "/home/remote/project": {
                    "connectionId": "c1",
                    "workspaces": {
                        "w": { "worktreePath": workspace.path(), "parentRepoPath": root.path() }
                    }
                }
            }
        });
        assert_eq!(
            resolve_owning_project_in(&workspace.path().to_string_lossy(), &doc).unwrap(),
            workspace.path().canonicalize().unwrap()
        );
    }

    // Catches: the cached ownership map serving a registry that has since been
    // written, so a newly registered workspace keeps its own journal.
    #[test]
    fn a_registry_write_rebuilds_the_cached_ownership_map() {
        let config = tempfile::tempdir().unwrap();
        let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let root = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let registry = crate::config::repository_file();
        let workspace_path = workspace.path().to_string_lossy().to_string();

        std::fs::write(&registry, json!({ "repos": {} }).to_string()).unwrap();
        assert_eq!(
            resolve_owning_project(Some(&workspace_path)).unwrap(),
            workspace.path().canonicalize().unwrap()
        );

        let doc = json!({
            "repos": {
                root.path().to_string_lossy(): {
                    "workspaces": {
                        "w": { "worktreePath": workspace.path(), "parentRepoPath": root.path() }
                    }
                }
            }
        });
        std::fs::write(&registry, doc.to_string()).unwrap();
        assert_eq!(
            resolve_owning_project(Some(&workspace_path)).unwrap(),
            root.path().canonicalize().unwrap()
        );
    }
}
