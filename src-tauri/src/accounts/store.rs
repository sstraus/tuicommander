//! One locked registry owns metadata; the instance vault owns every token.
use super::model::{Account, AuthKind, Registry, Site};
use crate::credentials::Credential;
use crate::github_account::{AccountKind, GitHubAccount, GitHubHost};
use serde::de::DeserializeOwned;

pub(crate) const ACCOUNTS_FILE: &str = "accounts.json";

fn read_existing<T: DeserializeOwned + Default>(name: &str) -> Result<T, String> {
    match std::fs::read(crate::config::config_dir().join(name)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| {
            format!("Account metadata in {name} is invalid; restore it before retrying")
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(_) => Err(format!("Account metadata in {name} is unavailable")),
    }
}

pub(crate) fn with_registry<R>(
    mutate: impl FnOnce(&mut Registry) -> Result<(R, bool), String>,
) -> Result<R, String> {
    // Do not let ConfigFile's general corrupt-file recovery turn a later retry into
    // a new empty registry. Account corruption needs an explicit operator repair.
    let _: Registry = read_existing(ACCOUNTS_FILE)?;
    crate::config::ConfigFile::<Registry>::new(ACCOUNTS_FILE).update_with_strict(|registry| {
        let migrated = migrate_github(registry)?;
        let (result, changed) = mutate(registry)?;
        Ok((result, changed || migrated))
    })
}

fn migrate_github(registry: &mut Registry) -> Result<bool, String> {
    if registry.github_migrated {
        return Ok(false);
    }
    #[derive(Default, serde::Deserialize)]
    struct Legacy {
        #[serde(default)]
        accounts: Vec<GitHubAccount>,
    }
    let legacy: Legacy = read_existing("github_accounts.json")?;
    for account in legacy.accounts {
        upsert_github_metadata(registry, account)?;
    }
    // The default is a durable reference to the existing env/vault chain,
    // not a copied environment secret. Its id also preserves legacy repo bindings.
    if !registry.accounts.contains_key(GitHubAccount::GITHUB_COM_ID) {
        upsert_github_metadata(
            registry,
            GitHubAccount::github_com(AccountKind::GithubComOauth, None),
        )?;
    }
    registry.github_migrated = true;
    Ok(true)
}

pub(crate) fn snapshot() -> Result<Registry, String> {
    with_registry(|registry| Ok((registry.clone(), false)))
}

/// Local discovery reads metadata only, without vault access or migration writes.
pub(crate) fn installed_accounts() -> Result<Vec<Account>, String> {
    Ok(read_existing::<Registry>(ACCOUNTS_FILE)?
        .accounts
        .into_values()
        .collect())
}

pub(crate) fn ensure_site(
    registry: &mut Registry,
    provider: &str,
    host: &str,
    label: &str,
) -> Site {
    if let Some(site) = registry
        .sites
        .values()
        .find(|site| site.provider == provider && site.host == host)
    {
        return site.clone();
    }
    let site = Site {
        id: uuid::Uuid::new_v4().to_string(),
        provider: provider.into(),
        host: host.into(),
        label: label.into(),
    };
    registry.sites.insert(site.id.clone(), site.clone());
    site
}

fn upsert_github_metadata(registry: &mut Registry, github: GitHubAccount) -> Result<(), String> {
    let host = format!("https://{}", github.host.as_str());
    if let Some(existing) = registry.accounts.get(&github.id)
        && (existing.provider != "github" || existing.host != host)
    {
        return Err("Account id is already used by a different host or provider".into());
    }
    let site = ensure_site(registry, "github", &host, github.host.as_str());
    let kind = if github.is_ambient_default() {
        AuthKind::Ambient
    } else if github.kind == AccountKind::GhePat {
        AuthKind::Bearer
    } else {
        AuthKind::OauthDevice
    };
    let source = match github.kind {
        AccountKind::GithubComOauth => "oauth",
        AccountKind::GithubComEnv => "env",
        AccountKind::GhePat => "pat",
    };
    let existing = registry.accounts.get(&github.id);
    let record = Account {
        id: github.id.clone(),
        provider: "github".into(),
        site_id: site.id,
        host,
        kind,
        label: existing
            .map(|a| a.label.clone())
            .unwrap_or_else(|| github.login.clone().unwrap_or_else(|| github.id.clone())),
        login: github.login,
        username: None,
        created_at: existing.map_or(0, |a| a.created_at),
        auth_source: Some(source.into()),
    };
    registry.accounts.insert(record.id.clone(), record);
    Ok(())
}

pub(crate) fn github_account(record: &Account) -> Option<GitHubAccount> {
    if record.provider != "github" {
        return None;
    }
    let host = GitHubHost::new(record.host.strip_prefix("https://")?)?;
    let kind = match record.auth_source.as_deref() {
        Some("env") => AccountKind::GithubComEnv,
        Some("pat") => AccountKind::GhePat,
        _ if record.kind == AuthKind::Bearer => AccountKind::GhePat,
        _ => AccountKind::GithubComOauth,
    };
    Some(GitHubAccount {
        id: record.id.clone(),
        host,
        login: record.login.clone(),
        kind,
    })
}

pub(crate) fn github_accounts() -> Result<Vec<GitHubAccount>, String> {
    Ok(snapshot()?
        .accounts
        .values()
        .filter(|account| account.id != GitHubAccount::GITHUB_COM_ID)
        .filter_map(github_account)
        .collect())
}

pub(crate) fn upsert_github(account: GitHubAccount, token: Option<&str>) -> Result<(), String> {
    with_registry(|registry| {
        upsert_github_metadata(registry, account.clone())?;
        if let Some(token) = token {
            crate::credentials::set(Credential::AccountToken(&account.id), token)
                .map_err(|_| "Account credential storage failed")?;
        }
        Ok(((), true))
    })
}

pub(crate) fn read_token(id: &str) -> Result<Option<String>, String> {
    crate::credentials::get(Credential::AccountToken(id))
        .map_err(|_| "Account credential storage is unavailable".into())
}

pub(crate) fn remove(id: &str) -> Result<Option<Account>, String> {
    // Every removal path reaches this point, the legacy GitHub command included.
    if id == GitHubAccount::GITHUB_COM_ID {
        return Err("The github.com default cannot be removed; use clear_secret to log out".into());
    }
    with_registry(|registry| {
        if !registry.accounts.contains_key(id) {
            return Ok((None, false));
        }
        crate::credentials::delete(Credential::AccountToken(id))
            .map_err(|_| "Account credential removal failed; account was not removed")?;
        Ok((registry.accounts.remove(id), true))
    })
}

/// Apply a display-only change to the latest locked record.
pub(crate) fn set_label(id: &str, label: &str, site: bool) -> Result<(), String> {
    if label.trim().is_empty() || label.len() > 256 {
        return Err("Label must contain 1–256 bytes".into());
    }
    with_registry(|registry| {
        let target = if site {
            registry.sites.get_mut(id).map(|row| &mut row.label)
        } else {
            registry.accounts.get_mut(id).map(|row| &mut row.label)
        }
        .ok_or("Account or site not found")?;
        let changed = target.as_str() != label.trim();
        *target = label.trim().into();
        Ok(((), changed))
    })
}
