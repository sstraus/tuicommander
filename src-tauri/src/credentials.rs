//! Credential vault — all secrets in a single OS keyring entry.
//!
//! Stores a JSON object in one keyring entry (`tuicommander/vault`).
//! First access loads the blob; writes persist atomically. Legacy
//! per-service entries are lazily migrated on read and deleted after.
//!
//! Platform backends (all native, no JS bridge):
//! - macOS: Keychain (Security framework)
//! - Windows: Credential Manager (wincred)
//! - Linux: kernel keyutils / Secret Service (GNOME Keyring / KWallet)

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

type Vault = HashMap<String, String>;
type VaultGuard<'a> = MutexGuard<'a, Option<Vault>>;

static VAULT: Mutex<Option<Vault>> = Mutex::new(None);

const KEYRING_SERVICE: &str = "tuicommander";
const KEYRING_USER: &str = "vault";

const LEGACY_ENTRIES: &[(&str, &str)] = &[
    ("tuicommander-ai-chat", "api-key"),
    ("tuicommander-llm-api", "api-key"),
    ("tuicommander-github", "oauth-token"),
];

// ---------------------------------------------------------------------------
// Credential keys
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) enum Credential<'a> {
    AiChatApiKey,
    LlmApiKey,
    GithubOauthToken,
    McpUpstream(&'a str),
}

impl Credential<'_> {
    fn vault_key(&self) -> String {
        match self {
            Self::AiChatApiKey => "ai-chat/api-key".into(),
            Self::LlmApiKey => "llm-api/api-key".into(),
            Self::GithubOauthToken => "github/oauth-token".into(),
            Self::McpUpstream(name) => format!("mcp/{name}"),
        }
    }

    fn legacy_entry(&self) -> Option<(&str, &str)> {
        match self {
            Self::AiChatApiKey => Some(("tuicommander-ai-chat", "api-key")),
            Self::LlmApiKey => Some(("tuicommander-llm-api", "api-key")),
            Self::GithubOauthToken => Some(("tuicommander-github", "oauth-token")),
            Self::McpUpstream(name) => Some(("tuicommander-mcp", name)),
        }
    }
}

// ---------------------------------------------------------------------------
// Internal keyring helpers
// ---------------------------------------------------------------------------

fn lock() -> VaultGuard<'static> {
    VAULT.lock().unwrap_or_else(|e| e.into_inner())
}

fn read_keyring_entry(service: &str, user: &str) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(service, user)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;
    match entry.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Failed to read keyring ({service}/{user}): {e}")),
    }
}

fn delete_keyring_entry(service: &str, user: &str) {
    if let Ok(entry) = keyring::Entry::new(service, user) {
        let _ = entry.delete_credential();
    }
}

fn persist(vault: &Vault) -> Result<(), String> {
    let json =
        serde_json::to_string(vault).map_err(|e| format!("Failed to serialize vault: {e}"))?;
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;
    entry
        .set_password(&json)
        .map_err(|e| format!("Failed to save vault: {e}"))?;
    Ok(())
}

fn load(guard: &mut VaultGuard<'_>) -> Result<(), String> {
    if guard.is_some() {
        return Ok(());
    }

    #[cfg(test)]
    ensure_mock_keyring();

    if let Some(json) = read_keyring_entry(KEYRING_SERVICE, KEYRING_USER)? {
        let vault: Vault =
            serde_json::from_str(&json).map_err(|e| format!("Failed to parse vault: {e}"))?;
        **guard = Some(vault);
        return Ok(());
    }

    // Migrate known legacy entries
    let mut vault = HashMap::new();
    for &(service, user) in LEGACY_ENTRIES {
        if let Ok(Some(value)) = read_keyring_entry(service, user) {
            let cred = match (service, user) {
                ("tuicommander-ai-chat", "api-key") => Credential::AiChatApiKey,
                ("tuicommander-llm-api", "api-key") => Credential::LlmApiKey,
                ("tuicommander-github", "oauth-token") => Credential::GithubOauthToken,
                _ => unreachable!(),
            };
            vault.insert(cred.vault_key(), value);
            delete_keyring_entry(service, user);
        }
    }
    if !vault.is_empty() {
        persist(&vault)?;
    }
    **guard = Some(vault);
    Ok(())
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub(crate) fn get(cred: Credential<'_>) -> Result<Option<String>, String> {
    let key = cred.vault_key();
    let mut guard = lock();
    load(&mut guard)?;

    if let Some(value) = guard.as_ref().unwrap().get(&key) {
        return Ok(Some(value.clone()));
    }
    drop(guard);

    // Lazy migration: try legacy individual entry (handles dynamic MCP keys)
    if let Some((service, user)) = cred.legacy_entry() {
        if let Some(value) = read_keyring_entry(service, user)? {
            let mut guard = lock();
            let vault = guard.as_mut().unwrap();
            vault.insert(key, value.clone());
            persist(vault)?;
            delete_keyring_entry(service, user);
            return Ok(Some(value));
        }
    }
    Ok(None)
}

pub(crate) fn set(cred: Credential<'_>, value: &str) -> Result<(), String> {
    let key = cred.vault_key();
    let trimmed = value.trim().to_string();
    let mut guard = lock();
    load(&mut guard)?;
    let vault = guard.as_mut().unwrap();
    vault.insert(key, trimmed);
    persist(vault)
}

pub(crate) fn delete(cred: Credential<'_>) -> Result<(), String> {
    let key = cred.vault_key();
    let mut guard = lock();
    load(&mut guard)?;
    let vault = guard.as_mut().unwrap();
    vault.remove(&key);
    persist(vault)
}

// ---------------------------------------------------------------------------
// Test mock keyring
// ---------------------------------------------------------------------------

#[cfg(test)]
fn ensure_mock_keyring() {
    use keyring::{
        credential::{CredentialApi, CredentialBuilder, CredentialBuilderApi, CredentialPersistence},
        Error,
    };
    use std::sync::{Once, OnceLock};

    type Store = Mutex<HashMap<(String, String), String>>;
    fn store() -> &'static Store {
        static STORE: OnceLock<Store> = OnceLock::new();
        STORE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    #[derive(Debug)]
    struct InMemCredential {
        key: (String, String),
    }

    impl CredentialApi for InMemCredential {
        fn set_password(&self, password: &str) -> keyring::Result<()> {
            store()
                .lock()
                .unwrap()
                .insert(self.key.clone(), password.to_string());
            Ok(())
        }
        fn set_secret(&self, secret: &[u8]) -> keyring::Result<()> {
            let s = std::str::from_utf8(secret)
                .map_err(|e| Error::BadEncoding(e.to_string().into_bytes()))?;
            self.set_password(s)
        }
        fn get_password(&self) -> keyring::Result<String> {
            store()
                .lock()
                .unwrap()
                .get(&self.key)
                .cloned()
                .ok_or(Error::NoEntry)
        }
        fn get_secret(&self) -> keyring::Result<Vec<u8>> {
            self.get_password().map(|s| s.into_bytes())
        }
        fn delete_credential(&self) -> keyring::Result<()> {
            store()
                .lock()
                .unwrap()
                .remove(&self.key)
                .map(|_| ())
                .ok_or(Error::NoEntry)
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[derive(Debug)]
    struct InMemBuilder;

    impl CredentialBuilderApi for InMemBuilder {
        fn build(
            &self,
            _target: Option<&str>,
            service: &str,
            user: &str,
        ) -> keyring::Result<Box<keyring::credential::Credential>> {
            Ok(Box::new(InMemCredential {
                key: (service.to_string(), user.to_string()),
            }))
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn persistence(&self) -> CredentialPersistence {
            CredentialPersistence::ProcessOnly
        }
    }

    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let builder: Box<CredentialBuilder> = Box::new(InMemBuilder);
        keyring::set_default_credential_builder(builder);
    });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn reset_vault() {
        *lock() = None;
    }

    #[test]
    fn vault_key_format() {
        assert_eq!(Credential::AiChatApiKey.vault_key(), "ai-chat/api-key");
        assert_eq!(Credential::LlmApiKey.vault_key(), "llm-api/api-key");
        assert_eq!(
            Credential::GithubOauthToken.vault_key(),
            "github/oauth-token"
        );
        assert_eq!(Credential::McpUpstream("foo").vault_key(), "mcp/foo");
    }

    #[test]
    fn get_returns_none_when_empty() {
        reset_vault();
        let result = get(Credential::AiChatApiKey).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn set_then_get_roundtrips() {
        reset_vault();
        set(Credential::AiChatApiKey, "sk-test-123").unwrap();
        let result = get(Credential::AiChatApiKey).unwrap();
        assert_eq!(result, Some("sk-test-123".to_string()));
    }

    #[test]
    fn delete_removes_credential() {
        reset_vault();
        set(Credential::LlmApiKey, "key").unwrap();
        delete(Credential::LlmApiKey).unwrap();
        let result = get(Credential::LlmApiKey).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn set_trims_whitespace() {
        reset_vault();
        set(Credential::GithubOauthToken, "  spaced  ").unwrap();
        let result = get(Credential::GithubOauthToken).unwrap();
        assert_eq!(result, Some("spaced".to_string()));
    }

    #[test]
    fn mcp_upstream_crud() {
        reset_vault();
        set(Credential::McpUpstream("slack"), "xoxb-tok").unwrap();
        assert_eq!(
            get(Credential::McpUpstream("slack")).unwrap(),
            Some("xoxb-tok".to_string())
        );
        delete(Credential::McpUpstream("slack")).unwrap();
        assert_eq!(get(Credential::McpUpstream("slack")).unwrap(), None);
    }

    #[test]
    fn multiple_credentials_coexist() {
        reset_vault();
        set(Credential::AiChatApiKey, "chat-key").unwrap();
        set(Credential::LlmApiKey, "llm-key").unwrap();
        set(Credential::McpUpstream("a"), "mcp-a").unwrap();
        assert_eq!(
            get(Credential::AiChatApiKey).unwrap(),
            Some("chat-key".to_string())
        );
        assert_eq!(
            get(Credential::LlmApiKey).unwrap(),
            Some("llm-key".to_string())
        );
        assert_eq!(
            get(Credential::McpUpstream("a")).unwrap(),
            Some("mcp-a".to_string())
        );
    }
}
