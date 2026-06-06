//! Account-centric GitHub model (multi-account support, #61 + #62).
//!
//! Step 1 introduces the canonical host type and host-aware remote parsing —
//! the building blocks the later steps (account registry, repo bindings,
//! resolution) sit on top of. github.com keeps behaving exactly as before;
//! these are additive primitives, not yet wired into the hot paths.

use serde::{Deserialize, Serialize};

use crate::credentials::Credential;

/// A canonical, validated GitHub host (e.g. `github.com`, `ghe.acme.com`).
///
/// Construction goes through [`GitHubHost::new`], which lowercases, trims, and
/// rejects anything that isn't a bare hostname. Using the type everywhere (vs a
/// raw `String`) prevents URL/path injection AND duplicate-key drift from casing
/// or trailing dots.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct GitHubHost {
    host: String,
}

impl GitHubHost {
    /// Canonicalize + validate a raw host string.
    ///
    /// Lowercases, trims surrounding whitespace, strips a single trailing dot.
    /// Rejects: empty, anything containing `:` (scheme or port), and any byte
    /// outside `[a-z0-9.-]` (which also rejects `/`, `@`, and userinfo).
    pub(crate) fn new(raw: &str) -> Option<Self> {
        let h = raw.trim().trim_end_matches('.').to_ascii_lowercase();
        let ok = !h.is_empty()
            && !h.contains(':')
            && h.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
        ok.then_some(Self { host: h })
    }

    /// The canonical host string.
    pub(crate) fn as_str(&self) -> &str {
        &self.host
    }

    /// Whether this is github.com (cloud) vs a GitHub Enterprise Server.
    pub(crate) fn is_cloud(&self) -> bool {
        self.host == "github.com"
    }

    /// GraphQL endpoint URL: cloud uses `api.github.com/graphql`; GHE Server uses
    /// `https://{host}/api/graphql`.
    pub(crate) fn graphql_url(&self) -> String {
        if self.is_cloud() {
            "https://api.github.com/graphql".to_string()
        } else {
            format!("https://{}/api/graphql", self.host)
        }
    }

    /// REST API base URL: cloud uses `api.github.com`; GHE Server uses
    /// `https://{host}/api/v3`.
    pub(crate) fn rest_base(&self) -> String {
        if self.is_cloud() {
            "https://api.github.com".to_string()
        } else {
            format!("https://{}/api/v3", self.host)
        }
    }
}

/// Split a URL path tail into `(owner, repo)`, stripping a leading `/` and a
/// trailing `.git`. Returns `None` if either component is empty.
fn split_owner_repo(path: &str) -> Option<(String, String)> {
    let path = path.trim_start_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.splitn(3, '/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

/// Parse a git remote URL into `(host, owner, repo)` for any GitHub host.
///
/// Supports, for both github.com and GitHub Enterprise Server:
/// - scp-like SSH: `git@host:owner/repo.git`
/// - SSH URL: `ssh://git@host[:port]/owner/repo.git`
/// - HTTPS/HTTP: `https://host[:port]/owner/repo.git`
///
/// The host is canonicalized + validated via [`GitHubHost::new`]; an invalid
/// host yields `None`. This is the host-aware successor to
/// `github::parse_remote_url` (which is github.com-only and stays untouched
/// until later steps route through here).
pub(crate) fn parse_remote_url(url: &str) -> Option<(GitHubHost, String, String)> {
    let url = url.trim();

    // scp-like SSH (no scheme): user@host:owner/repo.git
    if !url.contains("://") {
        let at = url.find('@')?;
        let after_at = &url[at + 1..];
        let colon = after_at.find(':')?;
        let host = &after_at[..colon];
        let path = &after_at[colon + 1..];
        let host = GitHubHost::new(host)?;
        let (owner, repo) = split_owner_repo(path)?;
        return Some((host, owner, repo));
    }

    // URL forms: scheme://[userinfo@]host[:port]/owner/repo[.git]
    let scheme_end = url.find("://")?;
    let rest = &url[scheme_end + 3..];
    // Strip optional userinfo (`git@`).
    let rest = rest.rsplit('@').next().unwrap_or(rest);
    let (authority, path) = rest.split_once('/')?;
    // Strip optional port from the authority.
    let host = authority.split(':').next()?;
    let host = GitHubHost::new(host)?;
    let (owner, repo) = split_owner_repo(path)?;
    Some((host, owner, repo))
}

// ---------------------------------------------------------------------------
// Account model
// ---------------------------------------------------------------------------

/// Stable, persisted identifier for a GitHub account.
///
/// For v1 it equals the canonical host (one account per host), but it is stored
/// explicitly so future same-host multi-account is a data change, not a schema
/// change. The github.com default account uses [`GitHubAccount::GITHUB_COM_ID`].
pub(crate) type GitHubAccountId = String;

/// How an account authenticates. github.com mirrors the existing token-source
/// chain; GitHub Enterprise Server uses a pasted Personal Access Token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AccountKind {
    /// github.com via OAuth Device Flow (keyring `GithubOauthToken` slot).
    GithubComOauth,
    /// github.com via `GH_TOKEN`/`GITHUB_TOKEN` environment variable.
    GithubComEnv,
    /// github.com via `gh` CLI.
    GithubComGhCli,
    /// GitHub Enterprise Server via pasted Personal Access Token.
    GhePat,
}

impl AccountKind {
    /// Whether this kind authenticates with a per-account PAT (vs the github.com
    /// env→OAuth→gh chain).
    pub(crate) fn is_pat(&self) -> bool {
        matches!(self, AccountKind::GhePat)
    }
}

/// A configured GitHub account: a host + how it authenticates + (once validated)
/// the resolved viewer login.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GitHubAccount {
    pub(crate) id: GitHubAccountId,
    pub(crate) host: GitHubHost,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) login: Option<String>,
    pub(crate) kind: AccountKind,
}

impl GitHubAccount {
    /// Stable id of the implicit github.com default account.
    pub(crate) const GITHUB_COM_ID: &'static str = "github.com";

    /// Build the github.com default account for the given resolved auth kind.
    pub(crate) fn github_com(kind: AccountKind, login: Option<String>) -> Self {
        Self {
            id: Self::GITHUB_COM_ID.to_string(),
            host: GitHubHost::new("github.com").expect("github.com is a valid host"),
            login,
            kind,
        }
    }

    /// Build a GitHub Enterprise Server account authenticated by PAT. The id
    /// defaults to the canonical host (one account per host in v1).
    pub(crate) fn ghe_pat(host: GitHubHost, login: Option<String>) -> Self {
        Self {
            id: host.as_str().to_string(),
            host,
            login,
            kind: AccountKind::GhePat,
        }
    }

    pub(crate) fn is_cloud(&self) -> bool {
        self.host.is_cloud()
    }
}

// ---------------------------------------------------------------------------
// Account registry (persisted)
// ---------------------------------------------------------------------------

/// Config filename for the persisted account registry.
const GITHUB_ACCOUNTS_FILE: &str = "github_accounts.json";

/// The set of configured GitHub accounts, persisted to `github_accounts.json`.
///
/// Does NOT include the implicit github.com default account — that is
/// synthesized from the existing token-source chain so a github.com-only user
/// has an empty registry and unchanged behavior.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct GitHubAccountRegistry {
    #[serde(default)]
    accounts: Vec<GitHubAccount>,
}

impl GitHubAccountRegistry {
    /// Load the registry from disk (returns an empty registry if absent/corrupt).
    pub(crate) fn load() -> Self {
        crate::config::load_json_config(GITHUB_ACCOUNTS_FILE)
    }

    /// Persist the registry to disk atomically.
    pub(crate) fn save(&self) -> Result<(), String> {
        crate::config::save_json_config(GITHUB_ACCOUNTS_FILE, self)
    }

    pub(crate) fn list(&self) -> &[GitHubAccount] {
        &self.accounts
    }

    pub(crate) fn get(&self, id: &str) -> Option<&GitHubAccount> {
        self.accounts.iter().find(|a| a.id == id)
    }

    /// Insert a new account or replace the existing one with the same id.
    pub(crate) fn upsert(&mut self, account: GitHubAccount) {
        if let Some(slot) = self.accounts.iter_mut().find(|a| a.id == account.id) {
            *slot = account;
        } else {
            self.accounts.push(account);
        }
    }

    /// Remove the account with `id`; returns whether one was removed.
    pub(crate) fn remove(&mut self, id: &str) -> bool {
        let before = self.accounts.len();
        self.accounts.retain(|a| a.id != id);
        before != self.accounts.len()
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RepoBinding {
    pub(crate) account_id: GitHubAccountId,
    pub(crate) owner: String,
    pub(crate) repo: String,
    /// Which git remote the owner/repo came from (origin, upstream, …).
    pub(crate) remote_name: String,
}

/// Map of canonical repo root → binding, persisted to `github_bindings.json`.
///
/// Worktrees of the same repo resolve to the same canonical root and therefore
/// share one binding.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct RepoBindingStore {
    #[serde(default)]
    bindings: std::collections::HashMap<String, RepoBinding>,
}

impl RepoBindingStore {
    /// Load bindings from disk (empty store if absent/corrupt).
    pub(crate) fn load() -> Self {
        crate::config::load_json_config(GITHUB_BINDINGS_FILE)
    }

    /// Persist bindings to disk atomically.
    pub(crate) fn save(&self) -> Result<(), String> {
        crate::config::save_json_config(GITHUB_BINDINGS_FILE, self)
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Resolved to an account (explicit persisted binding).
    Bound {
        account: GitHubAccount,
        owner: String,
        repo: String,
    },
    /// One or more candidate bindings. `len == 1` → the caller auto-confirms +
    /// persists the binding and proceeds (the single obvious choice). `len > 1`
    /// → the caller must prompt the user (no silent origin pick).
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
    let match_host = |host: &GitHubHost| -> Option<&GitHubAccount> {
        if let Some(d) = github_com_default
            && d.host == *host
        {
            return Some(d);
        }
        registry.list().iter().find(|a| a.host == *host)
    };

    let mut candidates: Vec<BindCandidate> = Vec::new();
    let mut saw_cloud_without_account = false;

    for (remote_name, url) in crate::git::list_remotes(repo_path) {
        let Some((host, owner, repo)) = parse_remote_url(&url) else {
            continue;
        };
        if let Some(account) = match_host(&host) {
            let cand = BindCandidate {
                account_id: account.id.clone(),
                host,
                owner,
                repo,
                remote_name,
            };
            if !candidates.contains(&cand) {
                candidates.push(cand);
            }
        } else if host.is_cloud() {
            // github.com is a known GitHub host; an unregistered GHE host is not
            // assumed to be GitHub (we never auto-probe).
            saw_cloud_without_account = true;
        }
    }

    match candidates.len() {
        0 if saw_cloud_without_account => RepoResolution::NeedsAccount,
        0 => RepoResolution::Unmonitored,
        _ => RepoResolution::NeedsBind(candidates),
    }
}

// ---------------------------------------------------------------------------
// Persistence operations (testable, no network / no State)
// ---------------------------------------------------------------------------

/// Add or update an account record, storing its PAT (GHE accounts) in the vault.
pub(crate) fn add_account_record(account: GitHubAccount, pat: Option<&str>) -> Result<(), String> {
    if let Some(pat) = pat {
        crate::credentials::set(Credential::GithubToken(&account.id), pat)?;
    }
    let mut registry = GitHubAccountRegistry::load();
    registry.upsert(account);
    registry.save()
}

/// Remove an account everywhere: its PAT, its registry record, and every binding
/// that referenced it. (Per-account in-memory cache invalidation is layered on
/// in Step 9, once those caches become account-scoped.)
pub(crate) fn remove_account_everywhere(account_id: &str) -> Result<(), String> {
    // Best-effort token delete — absence is not an error.
    let _ = crate::credentials::delete(Credential::GithubToken(account_id));
    let mut registry = GitHubAccountRegistry::load();
    registry.remove(account_id);
    registry.save()?;
    let mut bindings = RepoBindingStore::load();
    bindings.remove_account_bindings(account_id);
    bindings.save()
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
    let mut store = RepoBindingStore::load();
    store.set_binding(repo_path, binding.clone());
    store.save()?;
    Ok(binding)
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Add a GitHub Enterprise Server account: validate the PAT against
/// `{rest_base}/user`, store it under the new account id, and persist the record.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_add_account(
    state: tauri::State<'_, std::sync::Arc<crate::state::AppState>>,
    host: String,
    pat: String,
) -> Result<GitHubAccount, String> {
    let host = GitHubHost::new(&host).ok_or_else(|| format!("Invalid host: {host}"))?;
    if host.is_cloud() {
        return Err("github.com uses device-flow login, not a PAT.".to_string());
    }
    let pat = pat.trim().to_string();
    if pat.is_empty() {
        return Err("Personal Access Token is required".to_string());
    }

    // Validate the PAT and resolve the viewer login.
    let url = format!("{}/user", host.rest_base());
    let resp = state
        .http_client
        .get(&url)
        .header("Authorization", format!("Bearer {pat}"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "TUICommander")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| format!("Could not reach {}: {e}", host.as_str()))?;
    if !resp.status().is_success() {
        let status = resp.status();
        return Err(format!(
            "Token rejected by {} (HTTP {status})",
            host.as_str()
        ));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse /user response: {e}"))?;
    let login = body["login"].as_str().map(String::from);

    let account = GitHubAccount::ghe_pat(host, login);
    add_account_record(account.clone(), Some(&pat))?;
    Ok(account)
}

/// Remove an account: its PAT, its record, and all of its repo bindings.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_remove_account(id: String) -> Result<(), String> {
    remove_account_everywhere(&id)
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

#[cfg(test)]
mod tests {
    use super::*;

    // --- GitHubHost::new canonicalization + validation ---

    #[test]
    fn host_lowercases_and_trims() {
        assert_eq!(GitHubHost::new("  GitHub.COM  ").unwrap().as_str(), "github.com");
    }

    #[test]
    fn host_strips_trailing_dot() {
        assert_eq!(GitHubHost::new("github.com.").unwrap().as_str(), "github.com");
    }

    #[test]
    fn host_rejects_scheme() {
        assert!(GitHubHost::new("https://github.com").is_none());
    }

    #[test]
    fn host_rejects_slash_path() {
        assert!(GitHubHost::new("github.com/owner").is_none());
    }

    #[test]
    fn host_rejects_userinfo() {
        assert!(GitHubHost::new("git@github.com").is_none());
    }

    #[test]
    fn host_rejects_port() {
        assert!(GitHubHost::new("github.com:443").is_none());
    }

    #[test]
    fn host_rejects_empty() {
        assert!(GitHubHost::new("").is_none());
        assert!(GitHubHost::new("   ").is_none());
        assert!(GitHubHost::new(".").is_none());
    }

    #[test]
    fn host_accepts_ghe() {
        assert_eq!(GitHubHost::new("ghe.acme.com").unwrap().as_str(), "ghe.acme.com");
    }

    // --- endpoint URLs ---

    #[test]
    fn cloud_endpoints() {
        let h = GitHubHost::new("github.com").unwrap();
        assert!(h.is_cloud());
        assert_eq!(h.graphql_url(), "https://api.github.com/graphql");
        assert_eq!(h.rest_base(), "https://api.github.com");
    }

    #[test]
    fn ghe_endpoints() {
        let h = GitHubHost::new("ghe.acme.com").unwrap();
        assert!(!h.is_cloud());
        assert_eq!(h.graphql_url(), "https://ghe.acme.com/api/graphql");
        assert_eq!(h.rest_base(), "https://ghe.acme.com/api/v3");
    }

    // --- parse_remote_url: github.com ---

    #[test]
    fn parse_github_ssh_scp_like() {
        let (h, o, r) = parse_remote_url("git@github.com:octocat/hello.git").unwrap();
        assert_eq!(h.as_str(), "github.com");
        assert_eq!(o, "octocat");
        assert_eq!(r, "hello");
    }

    #[test]
    fn parse_github_https() {
        let (h, o, r) = parse_remote_url("https://github.com/octocat/hello.git").unwrap();
        assert_eq!(h.as_str(), "github.com");
        assert_eq!(o, "octocat");
        assert_eq!(r, "hello");
    }

    #[test]
    fn parse_github_https_no_dot_git() {
        let (h, o, r) = parse_remote_url("https://github.com/octocat/hello").unwrap();
        assert_eq!(h.as_str(), "github.com");
        assert_eq!((o.as_str(), r.as_str()), ("octocat", "hello"));
    }

    // --- parse_remote_url: GitHub Enterprise Server ---

    #[test]
    fn parse_ghe_ssh_scp_like() {
        let (h, o, r) = parse_remote_url("git@ghe.acme.com:team/project.git").unwrap();
        assert_eq!(h.as_str(), "ghe.acme.com");
        assert_eq!((o.as_str(), r.as_str()), ("team", "project"));
    }

    #[test]
    fn parse_ghe_https() {
        let (h, o, r) = parse_remote_url("https://ghe.acme.com/team/project.git").unwrap();
        assert_eq!(h.as_str(), "ghe.acme.com");
        assert_eq!((o.as_str(), r.as_str()), ("team", "project"));
    }

    #[test]
    fn parse_ssh_url_with_port() {
        let (h, o, r) = parse_remote_url("ssh://git@ghe.acme.com:22/team/project.git").unwrap();
        assert_eq!(h.as_str(), "ghe.acme.com");
        assert_eq!((o.as_str(), r.as_str()), ("team", "project"));
    }

    #[test]
    fn parse_rejects_non_owner_repo() {
        assert!(parse_remote_url("https://github.com/onlyowner").is_none());
        assert!(parse_remote_url("not a url").is_none());
    }

    // --- account model ---

    #[test]
    fn github_com_default_account() {
        let acc = GitHubAccount::github_com(AccountKind::GithubComOauth, Some("octocat".into()));
        assert_eq!(acc.id, "github.com");
        assert!(acc.is_cloud());
        assert_eq!(acc.host.as_str(), "github.com");
        assert!(!acc.kind.is_pat());
    }

    #[test]
    fn ghe_pat_account_id_defaults_to_host() {
        let host = GitHubHost::new("ghe.acme.com").unwrap();
        let acc = GitHubAccount::ghe_pat(host, None);
        assert_eq!(acc.id, "ghe.acme.com");
        assert!(!acc.is_cloud());
        assert_eq!(acc.kind, AccountKind::GhePat);
        assert!(acc.kind.is_pat());
    }

    #[test]
    fn account_serde_round_trip() {
        let acc = GitHubAccount::ghe_pat(
            GitHubHost::new("ghe.acme.com").unwrap(),
            Some("octocat".into()),
        );
        let json = serde_json::to_string(&acc).unwrap();
        let back: GitHubAccount = serde_json::from_str(&json).unwrap();
        assert_eq!(acc, back);
    }

    // --- registry CRUD ---

    #[test]
    fn registry_upsert_get_remove() {
        let mut reg = GitHubAccountRegistry::default();
        assert!(reg.list().is_empty());

        let acc = GitHubAccount::ghe_pat(GitHubHost::new("ghe.acme.com").unwrap(), None);
        reg.upsert(acc.clone());
        assert_eq!(reg.get("ghe.acme.com"), Some(&acc));
        assert_eq!(reg.list().len(), 1);

        // Upsert with same id replaces, does not duplicate.
        let updated = GitHubAccount::ghe_pat(
            GitHubHost::new("ghe.acme.com").unwrap(),
            Some("octocat".into()),
        );
        reg.upsert(updated.clone());
        assert_eq!(reg.list().len(), 1);
        assert_eq!(reg.get("ghe.acme.com"), Some(&updated));

        assert!(reg.remove("ghe.acme.com"));
        assert!(reg.get("ghe.acme.com").is_none());
        assert!(!reg.remove("ghe.acme.com"));
    }

    #[test]
    fn registry_persists_to_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        // Empty registry when nothing on disk.
        assert!(GitHubAccountRegistry::load().list().is_empty());

        let mut reg = GitHubAccountRegistry::default();
        reg.upsert(GitHubAccount::ghe_pat(
            GitHubHost::new("ghe.acme.com").unwrap(),
            Some("octocat".into()),
        ));
        reg.save().expect("save");

        let loaded = GitHubAccountRegistry::load();
        assert_eq!(loaded, reg);
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
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", wt_gitdir.display()))
            .expect("write .git file");

        let mut store = RepoBindingStore::default();
        store.set_binding(&main, sample_binding("github.com"));
        // The linked worktree resolves to the same binding.
        assert_eq!(
            store.get_binding(&wt),
            Some(&sample_binding("github.com"))
        );
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
        store.save().expect("save");

        let loaded = RepoBindingStore::load();
        assert_eq!(loaded, store);
        assert_eq!(loaded.get_binding(&repo), Some(&sample_binding("github.com")));
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
        let mut reg = GitHubAccountRegistry::default();
        reg.upsert(acc.clone());
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
    fn resolve_unbound_single_github_com_needs_bind_one() {
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
        match res {
            RepoResolution::NeedsBind(c) => {
                assert_eq!(c.len(), 1);
                assert_eq!(c[0].account_id, "github.com");
                assert_eq!((c[0].owner.as_str(), c[0].repo.as_str()), ("octocat", "hello"));
            }
            other => panic!("expected NeedsBind([1]), got {other:?}"),
        }
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
        // A stale binding to an account that no longer exists must not resolve to
        // Bound — it re-resolves from the remotes instead.
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
        assert!(matches!(res, RepoResolution::NeedsBind(_)));
    }

    // --- persistence operations (Step 6) ---

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
        bindings.save().unwrap();

        remove_account_everywhere("ghe.remove-test.example").unwrap();

        assert!(GitHubAccountRegistry::load().get("ghe.remove-test.example").is_none());
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
