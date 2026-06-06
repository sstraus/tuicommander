//! Account-centric GitHub model (multi-account support, #61 + #62).
//!
//! Step 1 introduces the canonical host type and host-aware remote parsing —
//! the building blocks the later steps (account registry, repo bindings,
//! resolution) sit on top of. github.com keeps behaving exactly as before;
//! these are additive primitives, not yet wired into the hot paths.

use serde::{Deserialize, Serialize};

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
}
