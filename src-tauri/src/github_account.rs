//! Account-centric GitHub model (multi-account support, #61 + #62).
//!
//! Step 1 introduces the canonical host type and host-aware remote parsing —
//! the building blocks the later steps (account registry, repo bindings,
//! resolution) sit on top of. github.com keeps behaving exactly as before;
//! these are additive primitives, not yet wired into the hot paths.

use serde::{Deserialize, Serialize};

pub(crate) use tuic_git::github_account::*;

#[path = "github_account_settings.rs"]
mod settings;
pub(crate) use settings::update_settings_binding;

// ---------------------------------------------------------------------------
// Account registry (persisted)
// ---------------------------------------------------------------------------

/// GitHub projection of the shared `accounts.json` registry.
///
/// Does NOT include the implicit github.com default account — that is
/// synthesized from the existing token-source chain so a github.com-only user
/// has an empty registry and unchanged behavior.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub(crate) struct GitHubAccountRegistry {
    #[serde(default)]
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates accounts for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}, "x-collection-key" = "id"))]
    accounts: Vec<GitHubAccount>,
}

impl GitHubAccountRegistry {
    /// Load the registry from disk (returns an empty registry if absent/corrupt).
    pub(crate) fn load() -> Self {
        Self {
            accounts: crate::accounts::github_accounts().unwrap_or_else(|_| {
                tracing::error!(
                    source = "accounts",
                    "GitHub account metadata is unavailable; repair the account registry"
                );
                Vec::new()
            }),
        }
    }

    pub(crate) fn list(&self) -> &[GitHubAccount] {
        &self.accounts
    }

    pub(crate) fn get(&self, id: &str) -> Option<&GitHubAccount> {
        self.accounts.iter().find(|a| a.id == id)
    }
}

// ---------------------------------------------------------------------------
// Repo → account binding (persisted)
// ---------------------------------------------------------------------------

/// Config filename for the persisted repo→account bindings.
const GITHUB_BINDINGS_FILE: &str = "github_bindings.json";

/// An explicit association of a repo to a GitHub account.
///
/// Persisted (not derived live from the origin remote) so a repo's account is
/// stable even if its remotes change — drift is then surfaced to the user
/// rather than silently re-routing API calls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub(crate) struct RepoBinding {
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates account id for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) account_id: GitHubAccountId,
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates owner for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) owner: String,
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates repo for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) repo: String,
    /// Which git remote the owner/repo came from (origin, upstream, …).
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates remote name for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) remote_name: String,
}

/// Map of canonical repo root → binding, persisted to `github_bindings.json`.
///
/// Worktrees of the same repo resolve to the same canonical root and therefore
/// share one binding.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub(crate) struct RepoBindingStore {
    #[serde(default)]
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates bindings for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    bindings: std::collections::HashMap<String, RepoBinding>,
}

impl RepoBindingStore {
    /// Load bindings from disk (empty store if absent/corrupt).
    pub(crate) fn load() -> Self {
        crate::config::load_json_config(GITHUB_BINDINGS_FILE)
    }

    /// Persist bindings to disk atomically.
    pub(crate) fn save(&self, base: &Self) -> Result<(), String> {
        crate::config::ConfigFile::<Self>::new(GITHUB_BINDINGS_FILE).save_delta(base, self)
    }

    /// Canonical-root key for a repo path (worktree-aware).
    fn key(repo_path: &std::path::Path) -> String {
        crate::git::canonical_repo_root(repo_path)
            .to_string_lossy()
            .into_owned()
    }

    /// The binding for a repo path, resolved via its canonical root.
    pub(crate) fn get_binding(&self, repo_path: &std::path::Path) -> Option<&RepoBinding> {
        self.bindings.get(&Self::key(repo_path))
    }

    /// Insert or replace the binding for a repo path.
    pub(crate) fn set_binding(&mut self, repo_path: &std::path::Path, binding: RepoBinding) {
        self.bindings.insert(Self::key(repo_path), binding);
    }

    /// Remove the binding for a repo path; returns whether one was removed.
    pub(crate) fn remove_binding(&mut self, repo_path: &std::path::Path) -> bool {
        self.bindings.remove(&Self::key(repo_path)).is_some()
    }

    /// Drop every binding pointing at `account_id` (called when an account is
    /// removed). Returns the number of bindings dropped.
    pub(crate) fn remove_account_bindings(&mut self, account_id: &str) -> usize {
        let before = self.bindings.len();
        self.bindings.retain(|_, b| b.account_id != account_id);
        before - self.bindings.len()
    }

    /// Flatten to a stable, sorted list of entries (for the list command / UI).
    pub(crate) fn entries(&self) -> Vec<RepoBindingEntry> {
        let mut out: Vec<RepoBindingEntry> = self
            .bindings
            .iter()
            .map(|(root, binding)| RepoBindingEntry {
                repo_root: root.clone(),
                binding: binding.clone(),
            })
            .collect();
        out.sort_by(|a, b| a.repo_root.cmp(&b.repo_root));
        out
    }
}

/// A binding paired with the canonical repo root it applies to (for the UI).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RepoBindingEntry {
    pub(crate) repo_root: String,
    #[serde(flatten)]
    pub(crate) binding: RepoBinding,
}

// ---------------------------------------------------------------------------
// Repo → account resolution (binding-first, ambiguity-aware)
// ---------------------------------------------------------------------------

/// A candidate (account, owner/repo, remote) the user could bind a repo to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct BindCandidate {
    pub(crate) account_id: GitHubAccountId,
    pub(crate) host: GitHubHost,
    pub(crate) owner: String,
    pub(crate) repo: String,
    pub(crate) remote_name: String,
}

/// Outcome of resolving which account (if any) a repo belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RepoResolution {
    /// Resolved to an account (persisted binding or one unambiguous candidate).
    Bound {
        account: GitHubAccount,
        owner: String,
        repo: String,
    },
    /// Multiple candidate account/repository bindings require a user choice.
    /// Different repositories on the same account still require a remote choice.
    NeedsBind(Vec<BindCandidate>),
    /// A known GitHub host (github.com) is present but no account is configured
    /// for it → the user must add/authenticate an account.
    NeedsAccount,
    /// No remote that resolves to a configured GitHub account → not monitored.
    Unmonitored,
}

/// Resolve which account a repo belongs to.
///
/// Binding-first: a persisted binding wins outright. Otherwise every remote is
/// mapped to `(host, owner, repo)` and matched against the configured accounts
/// (the registry plus the implicit github.com default, when authenticated).
/// This replaces the github.com-only `get_github_remote_url` + `parse_remote_url`
/// + global-token trio.
///
/// `github_com_default` is the synthesized github.com account (from the existing
/// token chain) when authenticated, or `None` otherwise — so a github.com repo
/// keeps resolving with zero user action.
pub(crate) fn resolve_repo_account(
    repo_path: &std::path::Path,
    registry: &GitHubAccountRegistry,
    bindings: &RepoBindingStore,
    github_com_default: Option<&GitHubAccount>,
) -> RepoResolution {
    let find_account = |id: &str| -> Option<&GitHubAccount> {
        registry
            .get(id)
            .or_else(|| github_com_default.filter(|d| d.id == id))
    };

    // 1) An explicit binding to a still-existing account wins outright.
    if let Some(binding) = bindings.get_binding(repo_path)
        && let Some(account) = find_account(&binding.account_id)
    {
        return RepoResolution::Bound {
            account: account.clone(),
            owner: binding.owner.clone(),
            repo: binding.repo.clone(),
        };
    }

    // 2) Map every remote to (host, owner, repo) and match against accounts.
    //
    // Collect ALL accounts configured for a host, not just the first: two
    // github.com accounts share host `github.com`, so each must surface as its
    // own bind candidate (the persisted binding's account_id is the
    // disambiguator). The implicit github.com default is listed first, then any
    // registry accounts for the same host, deduped by account id.
    let match_hosts = |host: &GitHubHost| -> Vec<&GitHubAccount> {
        let mut out: Vec<&GitHubAccount> = Vec::new();
        if let Some(d) = github_com_default
            && d.host == *host
        {
            out.push(d);
        }
        for a in registry.list().iter().filter(|a| a.host == *host) {
            if !out.iter().any(|e| e.id == a.id) {
                out.push(a);
            }
        }
        out
    };

    let mut candidates: Vec<BindCandidate> = Vec::new();
    let mut saw_cloud_without_account = false;

    for (remote_name, url) in crate::git::list_remotes(repo_path) {
        let Some((host, owner, repo)) = parse_remote_url(&url) else {
            continue;
        };
        let accounts = match_hosts(&host);
        if accounts.is_empty() {
            if host.is_cloud() {
                // github.com is a known GitHub host; an unregistered GHE host is
                // not assumed to be GitHub (we never auto-probe).
                saw_cloud_without_account = true;
            }
            continue;
        }
        for account in accounts {
            let cand = BindCandidate {
                account_id: account.id.clone(),
                host: host.clone(),
                owner: owner.clone(),
                repo: repo.clone(),
                remote_name: remote_name.clone(),
            };
            if !candidates.contains(&cand) {
                candidates.push(cand);
            }
        }
    }

    match candidates.len() {
        0 if saw_cloud_without_account => RepoResolution::NeedsAccount,
        0 => RepoResolution::Unmonitored,
        1 => {
            let candidate = candidates.remove(0);
            RepoResolution::Bound {
                account: find_account(&candidate.account_id)
                    .expect("candidate came from a configured account")
                    .clone(),
                owner: candidate.owner,
                repo: candidate.repo,
            }
        }
        _ => RepoResolution::NeedsBind(candidates),
    }
}

// ---------------------------------------------------------------------------
// Persistence operations (testable, no network / no State)
// ---------------------------------------------------------------------------

/// Serializes the load-modify-save cycles of the account registry and binding
/// stores so two concurrent mutations can't lose an update. Both stores share
/// one lock — account removal touches them together. Poison is recovered (a
/// panic mid-mutation must not wedge all later account edits).
static STORE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn store_guard() -> std::sync::MutexGuard<'static, ()> {
    STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Add or update a GitHub account through the shared registry and vault.
pub(crate) fn add_account_record(account: GitHubAccount, pat: Option<&str>) -> Result<(), String> {
    let _guard = store_guard();
    crate::accounts::upsert_github(account, pat)
}

/// Remove an account everywhere: its PAT, its registry record, and every binding
/// that referenced it. (Per-account in-memory cache invalidation is layered on
/// in Step 9, once those caches become account-scoped.)
pub(crate) fn remove_account_everywhere(account_id: &str) -> Result<(), String> {
    let _guard = store_guard();
    // Refuse corrupt/unreadable bindings before deleting the account or its credential.
    let mut bindings = RepoBindingStore::read_strict()?;
    if account_id == GitHubAccount::GITHUB_COM_ID {
        return Err("The github.com default cannot be removed; use clear_secret to log out".into());
    }
    let bindings_base = bindings.clone();
    bindings.remove_account_bindings(account_id);
    // Persist the references first: a failed save must leave the account and credential
    // available to retry. A later account-removal failure leaves an unbound, valid account.
    bindings.save(&bindings_base)?;
    crate::accounts::remove(account_id)?;
    Ok(())
}

/// Persist a repo→account binding derived from the chosen remote (its owner/repo
/// come from parsing that remote's URL).
pub(crate) fn bind_repo_to_account(
    repo_path: &std::path::Path,
    account_id: &str,
    remote_name: &str,
) -> Result<RepoBinding, String> {
    let (_, url) = crate::git::list_remotes(repo_path)
        .into_iter()
        .find(|(name, _)| name == remote_name)
        .ok_or_else(|| format!("Remote '{remote_name}' not found"))?;
    let (_, owner, repo) = parse_remote_url(&url)
        .ok_or_else(|| format!("Remote '{remote_name}' is not a GitHub URL: {url}"))?;
    let binding = RepoBinding {
        account_id: account_id.to_string(),
        owner,
        repo,
        remote_name: remote_name.to_string(),
    };
    let _guard = store_guard();
    let mut store = RepoBindingStore::load();
    let base = store.clone();
    store.set_binding(repo_path, binding.clone());
    store.save(&base)?;
    Ok(binding)
}

/// Remove a repo→account binding; returns whether one existed. Serialized with
/// the other binding mutations.
pub(crate) fn unbind_repo(repo_path: &std::path::Path) -> Result<bool, String> {
    let _guard = store_guard();
    let mut store = RepoBindingStore::load();
    let base = store.clone();
    let removed = store.remove_binding(repo_path);
    if removed {
        store.save(&base)?;
    }
    Ok(removed)
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Validate a token against `{host}/user` and return the resolved login.
///
/// Used by additional github.com device-flow login.
pub(crate) async fn fetch_account_login(
    client: &reqwest::Client,
    host: &GitHubHost,
    token: &str,
) -> Result<Option<String>, String> {
    let resp = crate::github::github_rest_request(
        client,
        reqwest::Method::GET,
        host,
        "/user",
        token,
        crate::github::GITHUB_JSON,
    )
    .send()
    .await
    .map_err(|e| format!("Could not reach {}: {e}", host.as_str()))?;
    crate::accounts::read_verified_identity(resp, "/login", token)
        .await
        .map(Some)
}

/// Remove an account: its PAT, its record, all of its repo bindings, and its
/// in-memory caches (per-account breaker/viewer/rate + its cooldown entries).
/// Only the removed account's state is touched — others are untouched.
///
/// Shared by the Tauri command and the HTTP route (IPC/HTTP parity).
#[cfg(feature = "desktop")]
pub(crate) fn github_remove_account_impl(
    state: &std::sync::Arc<crate::state::AppState>,
    id: String,
) -> Result<(), String> {
    remove_account_everywhere(&id)?;
    // Drop the account's per-account runtime state and its cooldown keys
    // ("{id}:owner/repo"). github.com cooldowns ("owner/repo") are untouched.
    state.github.ghe_state.remove(&id);
    state
        .git_cache
        .github_repo_access_errors
        .retain(|_, failure| failure.account_id != id);
    let prefix = format!("{id}:");
    state
        .git_cache
        .github_repo_cooldown
        .retain(|key, _| !key.starts_with(&prefix));
    Ok(())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_remove_account(
    state: tauri::State<'_, std::sync::Arc<crate::state::AppState>>,
    id: String,
) -> Result<(), String> {
    github_remove_account_impl(state.inner(), id)
}

/// Persist a repo→account binding for the chosen remote.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_bind_repo(
    repo_path: String,
    account_id: String,
    remote_name: String,
) -> Result<RepoBinding, String> {
    bind_repo_to_account(std::path::Path::new(&repo_path), &account_id, &remote_name)
}

/// List configured (non-github.com) accounts.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_list_accounts() -> Result<Vec<GitHubAccount>, String> {
    Ok(GitHubAccountRegistry::load().list().to_vec())
}

/// List persisted repo→account bindings.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_list_bindings() -> Result<Vec<RepoBindingEntry>, String> {
    Ok(RepoBindingStore::load().entries())
}

/// Serializable view of a repo's resolution for the binding chooser UI.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct RepoResolutionDto {
    /// "bound" | "needs-bind" | "needs-account" | "unmonitored"
    pub(crate) status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) account: Option<GitHubAccount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) repo: Option<String>,
    #[serde(default)]
    pub(crate) candidates: Vec<BindCandidate>,
}

impl From<RepoResolution> for RepoResolutionDto {
    fn from(res: RepoResolution) -> Self {
        match res {
            RepoResolution::Bound {
                account,
                owner,
                repo,
            } => RepoResolutionDto {
                status: "bound".into(),
                account: Some(account),
                owner: Some(owner),
                repo: Some(repo),
                candidates: Vec::new(),
            },
            RepoResolution::NeedsBind(candidates) => RepoResolutionDto {
                status: "needs-bind".into(),
                account: None,
                owner: None,
                repo: None,
                candidates,
            },
            RepoResolution::NeedsAccount => RepoResolutionDto {
                status: "needs-account".into(),
                account: None,
                owner: None,
                repo: None,
                candidates: Vec::new(),
            },
            RepoResolution::Unmonitored => RepoResolutionDto {
                status: "unmonitored".into(),
                account: None,
                owner: None,
                repo: None,
                candidates: Vec::new(),
            },
        }
    }
}

/// Resolve a repo's account binding status (for the binding chooser UI).
///
/// Shared by the Tauri command and the HTTP route (IPC/HTTP parity).
#[cfg(feature = "desktop")]
pub(crate) fn github_resolve_repo_impl(
    state: &std::sync::Arc<crate::state::AppState>,
    repo_path: String,
) -> Result<RepoResolutionDto, String> {
    let registry = GitHubAccountRegistry::load();
    let bindings = RepoBindingStore::load();
    let default = crate::github::github_com_account(state);
    let resolution = resolve_repo_account(
        std::path::Path::new(&repo_path),
        &registry,
        &bindings,
        Some(&default),
    );
    Ok(RepoResolutionDto::from(resolution))
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_resolve_repo(
    state: tauri::State<'_, std::sync::Arc<crate::state::AppState>>,
    repo_path: String,
) -> Result<RepoResolutionDto, String> {
    github_resolve_repo_impl(state.inner(), repo_path)
}

/// Batch variant of [`github_resolve_repo_impl`]: loads the registry, bindings,
/// and github.com default ONCE and resolves every path against them, keyed by
/// the input repo path. Avoids the N-round-trip / 2N-disk-read cost of resolving
/// each repo with its own command call.
///
/// Shared by the Tauri command and the HTTP route (IPC/HTTP parity).
#[cfg(feature = "desktop")]
pub(crate) fn github_resolve_repos_impl(
    state: &std::sync::Arc<crate::state::AppState>,
    repo_paths: Vec<String>,
) -> std::collections::HashMap<String, RepoResolutionDto> {
    let registry = GitHubAccountRegistry::load();
    let bindings = RepoBindingStore::load();
    let default = crate::github::github_com_account(state);
    repo_paths
        .into_iter()
        .map(|p| {
            let resolution = resolve_repo_account(
                std::path::Path::new(&p),
                &registry,
                &bindings,
                Some(&default),
            );
            (p, RepoResolutionDto::from(resolution))
        })
        .collect()
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_resolve_repos(
    state: tauri::State<'_, std::sync::Arc<crate::state::AppState>>,
    repo_paths: Vec<String>,
) -> Result<std::collections::HashMap<String, RepoResolutionDto>, String> {
    Ok(github_resolve_repos_impl(state.inner(), repo_paths))
}

/// Remove a repo→account binding (re-resolves on the next poll).
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_unbind_repo(repo_path: String) -> Result<bool, String> {
    unbind_repo(std::path::Path::new(&repo_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::Credential;

    #[test]
    fn registry_reads_shared_account_records() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        assert_eq!(GitHubAccountRegistry::load().list(), []);

        let account = GitHubAccount::ghe_pat(
            GitHubHost::new("ghe.acme.com").unwrap(),
            Some("octocat".into()),
        );
        add_account_record(account.clone(), None).expect("save shared account");

        let loaded = GitHubAccountRegistry::load();
        assert_eq!(loaded.list(), std::slice::from_ref(&account));
        assert_eq!(loaded.get(&account.id), Some(&account));
    }

    // --- repo → account bindings ---

    fn sample_binding(account_id: &str) -> RepoBinding {
        RepoBinding {
            account_id: account_id.to_string(),
            owner: "octocat".to_string(),
            repo: "hello".to_string(),
            remote_name: "origin".to_string(),
        }
    }

    /// Create a minimal normal repo (just a `.git` dir) at `<base>/<name>`.
    fn make_repo(base: &std::path::Path, name: &str) -> std::path::PathBuf {
        let root = base.join(name);
        std::fs::create_dir_all(root.join(".git")).expect("mkdir .git");
        root
    }

    #[test]
    fn binding_set_get_remove_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = make_repo(dir.path(), "main");

        let mut store = RepoBindingStore::default();
        assert!(store.get_binding(&repo).is_none());

        let binding = sample_binding("github.com");
        store.set_binding(&repo, binding.clone());
        assert_eq!(store.get_binding(&repo), Some(&binding));

        assert!(store.remove_binding(&repo));
        assert!(store.get_binding(&repo).is_none());
        assert!(!store.remove_binding(&repo));
    }

    #[test]
    fn binding_is_shared_across_worktrees() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Main repo with a linked-worktree gitdir.
        let main = dir.path().join("main");
        let wt_gitdir = main.join(".git").join("worktrees").join("feat");
        std::fs::create_dir_all(&wt_gitdir).expect("mkdir worktree gitdir");
        std::fs::write(wt_gitdir.join("commondir"), "../..\n").expect("write commondir");
        let wt = dir.path().join("feat");
        std::fs::create_dir_all(&wt).expect("mkdir worktree");
        std::fs::write(
            wt.join(".git"),
            format!("gitdir: {}\n", wt_gitdir.display()),
        )
        .expect("write .git file");

        let mut store = RepoBindingStore::default();
        store.set_binding(&main, sample_binding("github.com"));
        // The linked worktree resolves to the same binding.
        assert_eq!(store.get_binding(&wt), Some(&sample_binding("github.com")));
    }

    #[test]
    fn removing_account_drops_only_its_bindings() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_a = make_repo(dir.path(), "a");
        let repo_b = make_repo(dir.path(), "b");

        let mut store = RepoBindingStore::default();
        store.set_binding(&repo_a, sample_binding("github.com"));
        store.set_binding(&repo_b, sample_binding("ghe.acme.com"));

        assert_eq!(store.remove_account_bindings("github.com"), 1);
        assert!(store.get_binding(&repo_a).is_none());
        assert_eq!(
            store.get_binding(&repo_b),
            Some(&sample_binding("ghe.acme.com"))
        );
    }

    #[test]
    fn bindings_persist_to_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = make_repo(dir.path(), "main");
        let _guard = crate::config::set_config_dir_override(dir.path().join("config"));

        assert!(RepoBindingStore::load().get_binding(&repo).is_none());

        let mut store = RepoBindingStore::default();
        store.set_binding(&repo, sample_binding("github.com"));
        store.save(&RepoBindingStore::default()).expect("save");

        let loaded = RepoBindingStore::load();
        assert_eq!(loaded, store);
        assert_eq!(
            loaded.get_binding(&repo),
            Some(&sample_binding("github.com"))
        );
    }

    #[test]
    #[serial_test::serial]
    fn stale_binding_saves_preserve_distinct_repositories() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let base = RepoBindingStore::load();
        let mut first = base.clone();
        first
            .bindings
            .insert("/alpha".into(), sample_binding("one"));
        let mut second = base.clone();
        second
            .bindings
            .insert("/beta".into(), sample_binding("two"));

        first.save(&base).unwrap();
        second.save(&base).unwrap();

        let saved = RepoBindingStore::load();
        assert!(saved.bindings.contains_key("/alpha"));
        assert!(saved.bindings.contains_key("/beta"));
    }

    // --- resolve_repo_account ---

    /// Create a repo with a `.git/config` listing the given `(remote, url)` pairs.
    fn make_repo_with_remotes(
        base: &std::path::Path,
        name: &str,
        remotes: &[(&str, &str)],
    ) -> std::path::PathBuf {
        let root = base.join(name);
        let git = root.join(".git");
        std::fs::create_dir_all(&git).expect("mkdir .git");
        let mut config = String::new();
        for (remote, url) in remotes {
            config.push_str(&format!("[remote \"{remote}\"]\n\turl = {url}\n"));
        }
        std::fs::write(git.join("config"), config).expect("write config");
        root
    }

    fn github_com_default() -> GitHubAccount {
        GitHubAccount::github_com(AccountKind::GithubComOauth, Some("octocat".into()))
    }

    #[test]
    fn resolve_bound_repo_returns_its_account() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = make_repo_with_remotes(
            dir.path(),
            "main",
            &[("origin", "git@ghe.acme.com:team/project.git")],
        );
        let acc = GitHubAccount::ghe_pat(GitHubHost::new("ghe.acme.com").unwrap(), None);
        let reg = GitHubAccountRegistry {
            accounts: vec![acc.clone()],
        };
        let mut bindings = RepoBindingStore::default();
        bindings.set_binding(
            &repo,
            RepoBinding {
                account_id: "ghe.acme.com".into(),
                owner: "team".into(),
                repo: "project".into(),
                remote_name: "origin".into(),
            },
        );

        let res = resolve_repo_account(&repo, &reg, &bindings, None);
        assert_eq!(
            res,
            RepoResolution::Bound {
                account: acc,
                owner: "team".into(),
                repo: "project".into()
            }
        );
    }

    #[test]
    fn resolve_single_candidate_does_not_require_a_binding_choice() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = make_repo_with_remotes(
            dir.path(),
            "main",
            &[("origin", "git@github.com:octocat/hello.git")],
        );
        let default = github_com_default();
        let res = resolve_repo_account(
            &repo,
            &GitHubAccountRegistry::default(),
            &RepoBindingStore::default(),
            Some(&default),
        );
        assert_eq!(
            res,
            RepoResolution::Bound {
                account: default,
                owner: "octocat".into(),
                repo: "hello".into(),
            }
        );
    }

    // Catches: resolving a unique named account incorrectly requires a binding,
    // or silently substitutes the ambient account's identity.
    #[test]
    fn resolve_single_named_account_without_ambient_credentials() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let repo = make_repo_with_remotes(
            dir.path(),
            "main",
            &[("origin", "git@ghe.acme.com:team/project.git")],
        );
        let account = GitHubAccount::ghe_pat(GitHubHost::new("ghe.acme.com").unwrap(), None);
        let registry = GitHubAccountRegistry {
            accounts: vec![account.clone()],
        };
        assert_eq!(
            resolve_repo_account(&repo, &registry, &RepoBindingStore::default(), None),
            RepoResolution::Bound {
                account,
                owner: "team".into(),
                repo: "project".into()
            }
        );
    }

    #[test]
    fn resolve_github_com_without_default_needs_account() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = make_repo_with_remotes(
            dir.path(),
            "main",
            &[("origin", "https://github.com/octocat/hello.git")],
        );
        let res = resolve_repo_account(
            &repo,
            &GitHubAccountRegistry::default(),
            &RepoBindingStore::default(),
            None,
        );
        assert_eq!(res, RepoResolution::NeedsAccount);
    }

    #[test]
    fn resolve_no_remotes_is_unmonitored() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = make_repo_with_remotes(dir.path(), "main", &[]);
        let default = github_com_default();
        let res = resolve_repo_account(
            &repo,
            &GitHubAccountRegistry::default(),
            &RepoBindingStore::default(),
            Some(&default),
        );
        assert_eq!(res, RepoResolution::Unmonitored);
    }

    #[test]
    fn resolve_unregistered_ghe_host_is_unmonitored() {
        // An unregistered non-cloud host is not assumed to be GitHub.
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = make_repo_with_remotes(
            dir.path(),
            "main",
            &[("origin", "git@ghe.acme.com:team/project.git")],
        );
        let default = github_com_default();
        let res = resolve_repo_account(
            &repo,
            &GitHubAccountRegistry::default(),
            &RepoBindingStore::default(),
            Some(&default),
        );
        assert_eq!(res, RepoResolution::Unmonitored);
    }

    #[test]
    fn resolve_multiple_remotes_needs_bind_choice() {
        // origin + upstream both on github.com (different owners) → ambiguous,
        // the user must choose (no silent origin pick).
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = make_repo_with_remotes(
            dir.path(),
            "main",
            &[
                ("origin", "git@github.com:octocat/hello.git"),
                ("upstream", "git@github.com:upstream/hello.git"),
            ],
        );
        let default = github_com_default();
        let res = resolve_repo_account(
            &repo,
            &GitHubAccountRegistry::default(),
            &RepoBindingStore::default(),
            Some(&default),
        );
        match res {
            RepoResolution::NeedsBind(c) => {
                assert_eq!(c.len(), 2);
                let owners: Vec<&str> = c.iter().map(|b| b.owner.as_str()).collect();
                assert!(owners.contains(&"octocat") && owners.contains(&"upstream"));
            }
            other => panic!("expected NeedsBind([2]), got {other:?}"),
        }
    }

    #[test]
    fn resolve_binding_to_removed_account_falls_through() {
        // A stale binding must not keep the removed identity: the single current
        // candidate resolves from the remotes instead.
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = make_repo_with_remotes(
            dir.path(),
            "main",
            &[("origin", "git@github.com:octocat/hello.git")],
        );
        let mut bindings = RepoBindingStore::default();
        bindings.set_binding(
            &repo,
            RepoBinding {
                account_id: "ghe.gone.example".into(),
                owner: "team".into(),
                repo: "project".into(),
                remote_name: "origin".into(),
            },
        );
        let default = github_com_default();
        let res = resolve_repo_account(
            &repo,
            &GitHubAccountRegistry::default(),
            &bindings,
            Some(&default),
        );
        assert_eq!(
            res,
            RepoResolution::Bound {
                account: default,
                owner: "octocat".into(),
                repo: "hello".into(),
            }
        );
    }

    /// A second github.com account (same host as the implicit default), kept
    /// distinct by a unique id. Models the multi-account-on-github.com case.
    fn github_com_named(id: &str, login: &str) -> GitHubAccount {
        GitHubAccount {
            id: id.to_string(),
            host: GitHubHost::new("github.com").unwrap(),
            login: Some(login.to_string()),
            kind: AccountKind::GithubComOauth,
        }
    }

    #[test]
    fn resolve_two_github_com_accounts_surfaces_both() {
        // A single github.com remote with two configured github.com accounts must
        // surface BOTH as candidates (host-only matching can't pick one) so the
        // user disambiguates by account.
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = make_repo_with_remotes(
            dir.path(),
            "main",
            &[("origin", "git@github.com:octocat/hello.git")],
        );
        let default = github_com_default(); // id "github.com"
        let reg = GitHubAccountRegistry {
            accounts: vec![github_com_named("github.com#work", "work-login")],
        };

        let res = resolve_repo_account(&repo, &reg, &RepoBindingStore::default(), Some(&default));
        match res {
            RepoResolution::NeedsBind(c) => {
                assert_eq!(c.len(), 2, "both github.com accounts should be candidates");
                let ids: Vec<&str> = c.iter().map(|b| b.account_id.as_str()).collect();
                assert!(ids.contains(&"github.com") && ids.contains(&"github.com#work"));
                // Same remote/owner/repo for both candidates.
                assert!(c.iter().all(|b| b.owner == "octocat" && b.repo == "hello"));
            }
            other => panic!("expected NeedsBind([2]), got {other:?}"),
        }
    }

    #[test]
    fn resolve_binding_disambiguates_same_host_by_account_id() {
        // With two github.com accounts, an explicit binding to one resolves to
        // exactly that account (disambiguation is by account_id, not host).
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = make_repo_with_remotes(
            dir.path(),
            "main",
            &[("origin", "git@github.com:octocat/hello.git")],
        );
        let default = github_com_default();
        let work = github_com_named("github.com#work", "work-login");
        let reg = GitHubAccountRegistry {
            accounts: vec![work.clone()],
        };

        let mut bindings = RepoBindingStore::default();
        bindings.set_binding(
            &repo,
            RepoBinding {
                account_id: "github.com#work".into(),
                owner: "octocat".into(),
                repo: "hello".into(),
                remote_name: "origin".into(),
            },
        );

        let res = resolve_repo_account(&repo, &reg, &bindings, Some(&default));
        assert_eq!(
            res,
            RepoResolution::Bound {
                account: work,
                owner: "octocat".into(),
                repo: "hello".into()
            }
        );
    }

    #[test]
    fn resolve_dedups_identical_candidates_across_remotes() {
        // Two remotes pointing at the same owner/repo on the same host must not
        // produce duplicate candidates.
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = make_repo_with_remotes(
            dir.path(),
            "main",
            &[
                ("origin", "git@github.com:octocat/hello.git"),
                ("mirror", "git@github.com:octocat/hello.git"),
            ],
        );
        let default = github_com_default();
        let res = resolve_repo_account(
            &repo,
            &GitHubAccountRegistry::default(),
            &RepoBindingStore::default(),
            Some(&default),
        );
        match res {
            // Same account_id + owner/repo; the two remotes differ only by name,
            // so they remain distinct candidates but neither is duplicated.
            RepoResolution::NeedsBind(c) => {
                let names: Vec<&str> = c.iter().map(|b| b.remote_name.as_str()).collect();
                assert_eq!(c.len(), 2);
                assert!(names.contains(&"origin") && names.contains(&"mirror"));
                // No exact duplicate entry.
                let mut seen = std::collections::HashSet::new();
                for cand in &c {
                    assert!(
                        seen.insert((
                            cand.account_id.clone(),
                            cand.owner.clone(),
                            cand.repo.clone(),
                            cand.remote_name.clone()
                        )),
                        "duplicate candidate: {cand:?}"
                    );
                }
            }
            other => panic!("expected NeedsBind, got {other:?}"),
        }
    }

    #[test]
    fn ambient_default_vs_named_routing_predicate() {
        // The state/token routing predicate: only the ambient default (id
        // "github.com") is global; a named github.com account is cloud-for-
        // endpoints but per-account-for-state.
        let primary =
            GitHubAccount::github_com(AccountKind::GithubComOauth, Some("octocat".into()));
        assert!(primary.is_ambient_default());
        assert!(primary.is_cloud());

        let named = GitHubAccount::github_com_named("octocat");
        assert!(
            !named.is_ambient_default(),
            "named cloud account is not the ambient default"
        );
        assert!(
            named.is_cloud(),
            "named cloud account is still cloud for endpoint URLs"
        );
        assert_eq!(named.id, "octocat");

        let ghe = GitHubAccount::ghe_pat(GitHubHost::new("ghe.acme.com").unwrap(), None);
        assert!(!ghe.is_ambient_default());
        assert!(!ghe.is_cloud());
    }

    #[test]
    fn legacy_github_com_binding_resolves_before_viewer_login() {
        // Boot-window / criterion 4: a persisted binding to the ambient default
        // id "github.com" must still resolve to Bound even when the viewer login
        // is not yet known (login = None at boot, before /user).
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = make_repo_with_remotes(
            dir.path(),
            "main",
            &[("origin", "git@github.com:octocat/hello.git")],
        );
        let default = GitHubAccount::github_com(AccountKind::GithubComOauth, None);
        let mut bindings = RepoBindingStore::default();
        bindings.set_binding(
            &repo,
            RepoBinding {
                account_id: "github.com".into(),
                owner: "octocat".into(),
                repo: "hello".into(),
                remote_name: "origin".into(),
            },
        );
        let res = resolve_repo_account(
            &repo,
            &GitHubAccountRegistry::default(),
            &bindings,
            Some(&default),
        );
        assert!(
            matches!(res, RepoResolution::Bound { .. }),
            "legacy github.com binding must resolve at boot, got {res:?}"
        );
    }

    // --- persistence operations (Step 6) ---

    #[test]
    fn two_github_com_accounts_persist_independent_tokens_and_ids() {
        // Story 001 criterion 5: two additional github.com accounts persist
        // independent ids and independent per-account tokens.
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let alice = GitHubAccount::github_com_named("alice");
        let bob = GitHubAccount::github_com_named("bob");
        assert_ne!(alice.id, bob.id);

        add_account_record(alice.clone(), Some("gho_alice")).unwrap();
        add_account_record(bob.clone(), Some("gho_bob")).unwrap();

        let reg = GitHubAccountRegistry::load();
        assert_eq!(reg.get("alice"), Some(&alice));
        assert_eq!(reg.get("bob"), Some(&bob));

        assert_eq!(
            crate::credentials::get(Credential::GithubToken("alice")).unwrap(),
            Some("gho_alice".to_string())
        );
        assert_eq!(
            crate::credentials::get(Credential::GithubToken("bob")).unwrap(),
            Some("gho_bob".to_string())
        );

        crate::credentials::delete(Credential::GithubToken("alice")).unwrap();
        crate::credentials::delete(Credential::GithubToken("bob")).unwrap();
    }

    #[test]
    fn removing_one_named_github_com_account_keeps_the_other() {
        // Story 003: with two additional github.com accounts, removing one leaves
        // the other's record AND its per-account token intact.
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        add_account_record(GitHubAccount::github_com_named("keep"), Some("gho_keep")).unwrap();
        add_account_record(GitHubAccount::github_com_named("drop"), Some("gho_drop")).unwrap();
        assert_eq!(GitHubAccountRegistry::load().list().len(), 2);

        remove_account_everywhere("drop").unwrap();

        let reg = GitHubAccountRegistry::load();
        assert!(reg.get("drop").is_none());
        assert_eq!(
            reg.get("keep"),
            Some(&GitHubAccount::github_com_named("keep"))
        );
        assert_eq!(
            crate::credentials::get(Credential::GithubToken("keep")).unwrap(),
            Some("gho_keep".to_string())
        );
        assert_eq!(
            crate::credentials::get(Credential::GithubToken("drop")).unwrap(),
            None
        );

        crate::credentials::delete(Credential::GithubToken("keep")).unwrap();
    }

    #[test]
    fn add_account_record_persists_record_and_pat() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let host = GitHubHost::new("ghe.add-test.example").unwrap();
        let account = GitHubAccount::ghe_pat(host, Some("octocat".into()));
        add_account_record(account.clone(), Some("ghp_add_test")).unwrap();

        let registry = GitHubAccountRegistry::load();
        assert_eq!(registry.get("ghe.add-test.example"), Some(&account));
        assert_eq!(
            crate::credentials::get(Credential::GithubToken("ghe.add-test.example")).unwrap(),
            Some("ghp_add_test".to_string())
        );

        crate::credentials::delete(Credential::GithubToken("ghe.add-test.example")).unwrap();
    }

    #[test]
    fn add_account_rejects_id_collision_across_hosts() {
        // A bare-hostname GHE id ("acme") and a github.com login id ("acme") must
        // not silently clobber each other's record + vault token.
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let ghe = GitHubAccount::ghe_pat(GitHubHost::new("acme").unwrap(), None);
        assert_eq!(ghe.id, "acme");
        add_account_record(ghe, Some("ghp_acme")).unwrap();

        // Same id, different host (github.com login) → rejected.
        let named = GitHubAccount::github_com_named("acme");
        assert_eq!(named.id, "acme");
        let err = add_account_record(named, Some("gho_acme")).unwrap_err();
        assert!(err.contains("different host"), "got: {err}");

        // The GHE account's token survived (no clobber).
        assert_eq!(
            crate::credentials::get(Credential::GithubToken("acme")).unwrap(),
            Some("ghp_acme".to_string())
        );
        crate::credentials::delete(Credential::GithubToken("acme")).unwrap();
    }

    #[test]
    fn remove_account_everywhere_cascades() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let repo = make_repo(dir.path(), "main");

        let host = GitHubHost::new("ghe.remove-test.example").unwrap();
        let account = GitHubAccount::ghe_pat(host, None);
        add_account_record(account.clone(), Some("ghp_remove_test")).unwrap();
        let mut bindings = RepoBindingStore::load();
        bindings.set_binding(
            &repo,
            RepoBinding {
                account_id: "ghe.remove-test.example".into(),
                owner: "team".into(),
                repo: "project".into(),
                remote_name: "origin".into(),
            },
        );
        bindings.save(&RepoBindingStore::default()).unwrap();

        remove_account_everywhere("ghe.remove-test.example").unwrap();

        assert!(
            GitHubAccountRegistry::load()
                .get("ghe.remove-test.example")
                .is_none()
        );
        assert!(RepoBindingStore::load().get_binding(&repo).is_none());
        assert_eq!(
            crate::credentials::get(Credential::GithubToken("ghe.remove-test.example")).unwrap(),
            None
        );
    }

    #[test]
    fn bind_repo_to_account_derives_owner_repo_from_remote() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().join("config"));
        let repo = make_repo_with_remotes(
            dir.path(),
            "main",
            &[("origin", "git@ghe.acme.com:team/project.git")],
        );

        let binding = bind_repo_to_account(&repo, "ghe.acme.com", "origin").unwrap();
        assert_eq!(binding.owner, "team");
        assert_eq!(binding.repo, "project");
        assert_eq!(binding.account_id, "ghe.acme.com");

        assert_eq!(RepoBindingStore::load().get_binding(&repo), Some(&binding));
    }

    #[test]
    fn resolution_dto_maps_each_variant() {
        let acc = GitHubAccount::ghe_pat(GitHubHost::new("ghe.acme.com").unwrap(), None);
        let bound = RepoResolutionDto::from(RepoResolution::Bound {
            account: acc.clone(),
            owner: "team".into(),
            repo: "project".into(),
        });
        assert_eq!(bound.status, "bound");
        assert_eq!(
            bound.account.as_ref().map(|a| a.id.as_str()),
            Some("ghe.acme.com")
        );

        let needs = RepoResolutionDto::from(RepoResolution::NeedsBind(vec![BindCandidate {
            account_id: "github.com".into(),
            host: GitHubHost::new("github.com").unwrap(),
            owner: "octocat".into(),
            repo: "hello".into(),
            remote_name: "origin".into(),
        }]));
        assert_eq!(needs.status, "needs-bind");
        assert_eq!(needs.candidates.len(), 1);

        assert_eq!(
            RepoResolutionDto::from(RepoResolution::NeedsAccount).status,
            "needs-account"
        );
        assert_eq!(
            RepoResolutionDto::from(RepoResolution::Unmonitored).status,
            "unmonitored"
        );
    }

    #[test]
    fn bind_repo_to_account_rejects_unknown_remote() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().join("config"));
        let repo = make_repo_with_remotes(
            dir.path(),
            "main",
            &[("origin", "git@github.com:octocat/hello.git")],
        );
        assert!(bind_repo_to_account(&repo, "github.com", "upstream").is_err());
    }
}
