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

use std::time::{Duration, Instant};

const CIRCUIT_BREAKER_COOLDOWN: Duration = Duration::from_secs(60);
const CIRCUIT_BREAKER_THRESHOLD: u32 = 3;

struct CircuitBreaker {
    failures: u32,
    last_failure: Option<Instant>,
}

static CIRCUIT: Mutex<CircuitBreaker> = Mutex::new(CircuitBreaker {
    failures: 0,
    last_failure: None,
});

fn circuit_check() -> Result<(), String> {
    let cb = CIRCUIT.lock().unwrap_or_else(|e| e.into_inner());
    if cb.failures >= CIRCUIT_BREAKER_THRESHOLD
        && let Some(last) = cb.last_failure
        && last.elapsed() < CIRCUIT_BREAKER_COOLDOWN
    {
        return Err(format!(
            "Keyring unavailable (failed {} times). Retrying in {}s.",
            cb.failures,
            (CIRCUIT_BREAKER_COOLDOWN - last.elapsed()).as_secs()
        ));
    }
    Ok(())
}

fn circuit_record_success() {
    let mut cb = CIRCUIT.lock().unwrap_or_else(|e| e.into_inner());
    cb.failures = 0;
    cb.last_failure = None;
}

fn circuit_record_failure() {
    let mut cb = CIRCUIT.lock().unwrap_or_else(|e| e.into_inner());
    cb.failures += 1;
    cb.last_failure = Some(Instant::now());
}

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
    /// Per-account GitHub token (PAT) keyed by stable account id. The github.com
    /// default account keeps using `GithubOauthToken` instead — this slot is for
    /// additional accounts (GitHub Enterprise Server) only.
    GithubToken(&'a str),
    McpUpstream(&'a str),
    Provider(&'a str),
}

impl Credential<'_> {
    fn vault_key(&self) -> String {
        match self {
            Self::AiChatApiKey => "ai-chat/api-key".into(),
            Self::LlmApiKey => "llm-api/api-key".into(),
            Self::GithubOauthToken => "github/oauth-token".into(),
            Self::GithubToken(id) => format!("github/account/{id}/token"),
            Self::McpUpstream(name) => format!("mcp/{name}"),
            Self::Provider(id) => format!("provider/{id}"),
        }
    }

    fn legacy_entry(&self) -> Option<(&str, &str)> {
        match self {
            Self::AiChatApiKey => Some(("tuicommander-ai-chat", "api-key")),
            Self::LlmApiKey => Some(("tuicommander-llm-api", "api-key")),
            Self::GithubOauthToken => Some(("tuicommander-github", "oauth-token")),
            Self::McpUpstream(name) => Some(("tuicommander-mcp", name)),
            Self::GithubToken(_) | Self::Provider(_) => None,
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
    circuit_check()?;
    let entry = keyring::Entry::new(service, user).map_err(|e| {
        circuit_record_failure();
        format!("Failed to create keyring entry: {e}")
    })?;
    match entry.get_password() {
        Ok(v) => {
            circuit_record_success();
            Ok(Some(v))
        }
        Err(keyring::Error::NoEntry) => {
            circuit_record_success();
            Ok(None)
        }
        Err(e) => {
            circuit_record_failure();
            Err(format!("Failed to read keyring ({service}/{user}): {e}"))
        }
    }
}

fn delete_keyring_entry(service: &str, user: &str) {
    if let Ok(entry) = keyring::Entry::new(service, user) {
        let _ = entry.delete_credential();
    }
}

fn persist(vault: &Vault) -> Result<(), String> {
    circuit_check()?;
    let json =
        serde_json::to_string(vault).map_err(|e| format!("Failed to serialize vault: {e}"))?;
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|e| {
        circuit_record_failure();
        format!("Failed to create keyring entry: {e}")
    })?;
    entry.set_password(&json).map_err(|e| {
        circuit_record_failure();
        format!("Failed to save vault: {e}")
    })?;
    circuit_record_success();
    Ok(())
}

fn load(guard: &mut VaultGuard<'_>) -> Result<(), String> {
    if guard.is_some() {
        return Ok(());
    }

    #[cfg(all(debug_assertions, not(test)))]
    dev_store::init();

    #[cfg(test)]
    ensure_mock_keyring();

    let mut vault: Vault = match read_keyring_entry(KEYRING_SERVICE, KEYRING_USER)? {
        Some(json) => match serde_json::from_str(&json) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(
                    source = "credentials",
                    "Vault JSON corrupt ({e}), starting fresh. Corrupt data ({} bytes) discarded.",
                    json.len()
                );
                HashMap::new()
            }
        },
        None => HashMap::new(),
    };

    // Always sweep legacy entries — handles stragglers when vault was created
    // before all legacy keys were migrated.
    let mut migrated = false;
    for &(service, user) in LEGACY_ENTRIES {
        if let Ok(Some(value)) = read_keyring_entry(service, user) {
            let cred = match (service, user) {
                ("tuicommander-ai-chat", "api-key") => Credential::AiChatApiKey,
                ("tuicommander-llm-api", "api-key") => Credential::LlmApiKey,
                ("tuicommander-github", "oauth-token") => Credential::GithubOauthToken,
                _ => unreachable!(),
            };
            vault.entry(cred.vault_key()).or_insert(value);
            delete_keyring_entry(service, user);
            migrated = true;
        }
    }
    if migrated || vault.is_empty() {
        // Persist if we migrated anything, or create empty vault so next load
        // skips the legacy sweep entirely (entries already deleted).
        if !vault.is_empty() {
            persist(&vault)?;
        }
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

    // Lazy migration for dynamic keys only (MCP upstreams aren't in LEGACY_ENTRIES).
    // Static credentials are swept in load() — no extra keychain prompts.
    if matches!(cred, Credential::McpUpstream(_))
        && let Some((service, user)) = cred.legacy_entry()
        && let Some(value) = read_keyring_entry(service, user)?
    {
        let mut guard = lock();
        let vault = guard.as_mut().unwrap();
        vault.insert(key, value.clone());
        persist(vault)?;
        delete_keyring_entry(service, user);
        return Ok(Some(value));
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
// Debug file-backed keyring (avoids OS keychain prompts during development)
// ---------------------------------------------------------------------------

#[cfg(all(debug_assertions, not(test)))]
mod dev_store {
    use keyring::{
        Error,
        credential::{
            CredentialApi, CredentialBuilder, CredentialBuilderApi, CredentialPersistence,
        },
    };
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Mutex, Once, OnceLock};

    type Store = Mutex<HashMap<String, String>>;

    fn file_path() -> PathBuf {
        let home = dirs::home_dir().expect("Cannot determine home directory");
        let dir = home.join(".tuicommander-dev");
        std::fs::create_dir_all(&dir).ok();
        dir.join("credentials.json")
    }

    fn store() -> &'static Store {
        static STORE: OnceLock<Store> = OnceLock::new();
        STORE.get_or_init(|| {
            let map = std::fs::read_to_string(file_path())
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();
            Mutex::new(map)
        })
    }

    fn sync(guard: &HashMap<String, String>) {
        if let Ok(json) = serde_json::to_string_pretty(guard)
            && let Err(e) = std::fs::write(file_path(), json)
        {
            tracing::warn!(error = %e, "dev_store: failed to persist credentials to disk");
        }
    }

    #[derive(Debug)]
    struct FileCredential {
        key: String,
    }

    impl CredentialApi for FileCredential {
        fn set_password(&self, password: &str) -> keyring::Result<()> {
            let mut guard = store().lock().unwrap();
            guard.insert(self.key.clone(), password.to_string());
            sync(&guard);
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
            let mut guard = store().lock().unwrap();
            guard.remove(&self.key).ok_or(Error::NoEntry)?;
            sync(&guard);
            Ok(())
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[derive(Debug)]
    struct FileBuilder;

    impl CredentialBuilderApi for FileBuilder {
        fn build(
            &self,
            _target: Option<&str>,
            service: &str,
            user: &str,
        ) -> keyring::Result<Box<keyring::credential::Credential>> {
            Ok(Box::new(FileCredential {
                key: format!("{service}/{user}"),
            }))
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn persistence(&self) -> CredentialPersistence {
            CredentialPersistence::UntilDelete
        }
    }

    pub(super) fn init() {
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            tracing::info!(
                source = "credentials",
                path = %file_path().display(),
                "Debug build: using file-backed credential store"
            );
            let builder: Box<CredentialBuilder> = Box::new(FileBuilder);
            keyring::set_default_credential_builder(builder);
        });
    }
}

// ---------------------------------------------------------------------------
// Test mock keyring
// ---------------------------------------------------------------------------

#[cfg(test)]
fn ensure_mock_keyring() {
    use keyring::{
        Error,
        credential::{
            CredentialApi, CredentialBuilder, CredentialBuilderApi, CredentialPersistence,
        },
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

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn reset_vault() {
        *lock() = None;
        if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
            let _ = entry.delete_credential();
        }
    }

    fn simulate_restart() {
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
        assert_eq!(Credential::Provider("my-id").vault_key(), "provider/my-id");
    }

    #[test]
    fn provider_credential_has_no_legacy_entry() {
        assert!(Credential::Provider("test").legacy_entry().is_none());
    }

    #[test]
    fn github_token_vault_key_is_account_scoped() {
        assert_eq!(
            Credential::GithubToken("ghe.acme.com").vault_key(),
            "github/account/ghe.acme.com/token"
        );
    }

    #[test]
    fn github_token_has_no_legacy_entry() {
        // Per-account PATs are a new feature — there is no legacy keyring slot to
        // migrate from (only github.com's OAuth token has one, unchanged).
        assert!(Credential::GithubToken("acc1").legacy_entry().is_none());
        assert_eq!(
            Credential::GithubOauthToken.vault_key(),
            "github/oauth-token"
        );
    }

    #[test]
    fn github_token_crud() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_vault();
        set(Credential::GithubToken("ghe.acme.com"), "ghp_test_pat").unwrap();
        assert_eq!(
            get(Credential::GithubToken("ghe.acme.com")).unwrap(),
            Some("ghp_test_pat".to_string())
        );
        delete(Credential::GithubToken("ghe.acme.com")).unwrap();
        assert_eq!(get(Credential::GithubToken("ghe.acme.com")).unwrap(), None);
    }

    #[test]
    fn provider_credential_crud() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_vault();
        set(Credential::Provider("anthropic-main"), "sk-ant-123").unwrap();
        assert_eq!(
            get(Credential::Provider("anthropic-main")).unwrap(),
            Some("sk-ant-123".to_string())
        );
        delete(Credential::Provider("anthropic-main")).unwrap();
        assert_eq!(get(Credential::Provider("anthropic-main")).unwrap(), None);
    }

    #[test]
    fn get_returns_none_when_empty() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_vault();
        let result = get(Credential::AiChatApiKey).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn set_then_get_roundtrips() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_vault();
        set(Credential::AiChatApiKey, "sk-test-123").unwrap();
        let result = get(Credential::AiChatApiKey).unwrap();
        assert_eq!(result, Some("sk-test-123".to_string()));
    }

    #[test]
    fn delete_removes_credential() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_vault();
        set(Credential::LlmApiKey, "key").unwrap();
        delete(Credential::LlmApiKey).unwrap();
        let result = get(Credential::LlmApiKey).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn set_trims_whitespace() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_vault();
        set(Credential::GithubOauthToken, "  spaced  ").unwrap();
        let result = get(Credential::GithubOauthToken).unwrap();
        assert_eq!(result, Some("spaced".to_string()));
    }

    #[test]
    fn mcp_upstream_crud() {
        let _guard = TEST_LOCK.lock().unwrap();
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
    fn legacy_straggler_migrated_when_vault_exists() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_vault();
        // Simulate: vault already exists with one key…
        set(Credential::LlmApiKey, "llm-key").unwrap();
        // …but a legacy entry was never migrated (written by older app version).
        let legacy = keyring::Entry::new("tuicommander-ai-chat", "api-key").unwrap();
        legacy.set_password("legacy-chat-key").unwrap();

        // Simulate app restart (keep keyring state, clear in-memory cache)
        simulate_restart();
        let result = get(Credential::AiChatApiKey).unwrap();
        assert_eq!(result, Some("legacy-chat-key".to_string()));

        // Legacy entry must be deleted after migration
        assert!(matches!(
            legacy.get_password(),
            Err(keyring::Error::NoEntry)
        ));
    }

    #[test]
    fn legacy_sweep_does_not_overwrite_vault_value() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_vault();
        set(Credential::AiChatApiKey, "vault-key").unwrap();
        // Plant a stale legacy entry
        let legacy = keyring::Entry::new("tuicommander-ai-chat", "api-key").unwrap();
        legacy.set_password("stale-legacy").unwrap();

        simulate_restart();
        let result = get(Credential::AiChatApiKey).unwrap();
        // Vault value wins (or_insert, not insert)
        assert_eq!(result, Some("vault-key".to_string()));
        // Legacy still cleaned up
        assert!(matches!(
            legacy.get_password(),
            Err(keyring::Error::NoEntry)
        ));
    }

    #[test]
    fn multiple_credentials_coexist() {
        let _guard = TEST_LOCK.lock().unwrap();
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

    fn reset_circuit_breaker() {
        let mut cb = CIRCUIT.lock().unwrap();
        cb.failures = 0;
        cb.last_failure = None;
    }

    #[test]
    fn circuit_breaker_trips_after_threshold() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_circuit_breaker();

        for _ in 0..CIRCUIT_BREAKER_THRESHOLD {
            circuit_record_failure();
        }
        let result = circuit_check();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Keyring unavailable"));

        reset_circuit_breaker();
    }

    #[test]
    fn circuit_breaker_resets_on_success() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_circuit_breaker();

        for _ in 0..CIRCUIT_BREAKER_THRESHOLD - 1 {
            circuit_record_failure();
        }
        circuit_record_success();
        assert!(circuit_check().is_ok());

        reset_circuit_breaker();
    }

    #[test]
    fn vault_corruption_recovers_gracefully() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_circuit_breaker();
        reset_vault();

        // Plant corrupt JSON directly in keyring
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).unwrap();
        entry.set_password("not-valid-json{{{").unwrap();

        // Clear in-memory vault so load() reads from keyring
        *lock() = None;

        // get() should recover instead of failing
        let result = get(Credential::AiChatApiKey);
        assert!(
            result.is_ok(),
            "corrupt vault should recover, got: {result:?}"
        );
        assert_eq!(result.unwrap(), None);

        // Setting a new credential should work (fresh vault)
        set(Credential::AiChatApiKey, "fresh-key").unwrap();
        assert_eq!(
            get(Credential::AiChatApiKey).unwrap(),
            Some("fresh-key".to_string())
        );

        reset_circuit_breaker();
    }
}
