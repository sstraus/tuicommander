//! Explicit server-owned repository registration. The CLI reaches this through
//! MCP, so it works without a desktop URL-scheme handler or a connected WebView.
use super::{ConfigFile, repository_file};
use crate::git::RepoInfo;
use serde_json::{Value, json};

pub(crate) fn register_repository(info: &RepoInfo) -> Result<bool, String> {
    ConfigFile::<Value>::at_path(repository_file()).update_with_strict(|document| {
        if document.is_null() {
            *document = json!({});
        }
        let root = document
            .as_object_mut()
            .ok_or("repositories root must be an object")?;
        let repos = root
            .entry("repos")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or("repos must be an object")?;
        let mut changed = false;
        if !repos.contains_key(&info.path) {
            let branch = if info.is_git_repo {
                info.branch.as_str()
            } else {
                "shell"
            };
            let is_main = !info.is_git_repo
                || matches!(
                    branch,
                    "main" | "master" | "develop" | "development" | "dev"
                );
            let workspace = json!({
                "workspaceId": branch, "branchName": branch,
                "kind": if is_main { "main" } else { "worktree" },
                "parentRepoPath": null, "isMain": is_main, "isShell": !info.is_git_repo,
                "worktreePath": info.path, "terminals": [], "hadTerminals": false,
                "lastActiveTerminal": null, "additions": 0, "deletions": 0,
                "isMerged": false, "lastCommitTs": null
            });
            repos.insert(
                info.path.clone(),
                json!({
                    "path": info.path, "displayName": info.name, "initials": info.initials,
                    "isGitRepo": info.is_git_repo, "expanded": true, "collapsed": false,
                    "parked": false, "workspaces": {branch: workspace}, "activeWorkspaceId": branch
                }),
            );
            let order = root
                .entry("repoOrder")
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or("repoOrder must be an array")?;
            if !order.iter().any(|path| path.as_str() == Some(&info.path)) {
                order.push(json!(info.path));
            }
            changed = true;
        } else {
            let repo = repos
                .get_mut(&info.path)
                .and_then(Value::as_object_mut)
                .ok_or("repository must be an object")?;
            // Reopening selects and reveals the existing row without resetting
            // workspaces, terminals, custom names, group membership or settings.
            if repo.get("parked") == Some(&Value::Bool(true)) {
                repo.insert("parked".into(), json!(false));
                changed = true;
            }
        }
        if root.get("activeRepoPath").and_then(Value::as_str) != Some(&info.path) {
            root.insert("activeRepoPath".into(), json!(info.path));
            changed = true;
        }
        Ok((changed, changed))
    })
}
