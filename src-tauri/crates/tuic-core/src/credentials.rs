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

/// The default instance vault tuple. Named instances derive their own in
/// `app_instance`; these constants only remain to address the default vault in
/// tests.
#[cfg(any(test, feature = "test-support"))]
pub const KEYRING_SERVICE: &str = "tuicommander";
#[cfg(test)]
const KEYRING_USER: &str = "vault";

pub const LEGACY_ENTRIES: &[(&str, &str)] = &[
    ("tuicommander-ai-chat", "api-key"),
    ("tuicommander-llm-api", "api-key"),
    ("tuicommander-github", "oauth-token"),
];

/// Legacy keyring *service* shared by every `Credential::McpUpstream` entry,
/// independent of upstream name — unlike `LEGACY_ENTRIES`, whose entries are
/// per-service, one MCP upstream's legacy account (`user`) sits under this one
/// service. `security find-generic-password -s <service> -w` matches on
/// service alone, ignoring account, so a plugin must be denied this service
/// string outright regardless of which upstream it claims to want. Kept as the
/// single source of truth `legacy_entry()` reads from below, so
/// `plugin_credentials::plugin_read_credential_inner`'s deny check can derive
/// from it instead of hardcoding the string a second time.
pub const MCP_UPSTREAM_LEGACY_SERVICE: &str = "tuicommander-mcp";

// ---------------------------------------------------------------------------
// Credential keys
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Credential<'a> {
    AiChatApiKey,
    LlmApiKey,
    GithubOauthToken,
    CircleCiToken,
    RemoteSessionToken,
    RelayToken,
    PushVapidPrivateKey,
    /// Per-account GitHub token (PAT) keyed by stable account id. The github.com
    /// default account keeps using `GithubOauthToken` instead — this slot is for
    /// additional accounts (GitHub Enterprise Server) only.
    GithubToken(&'a str),
    /// Core account token; no plugin/model read transport exposes this slot.
    AccountToken(&'a str),
    McpUpstream(&'a str),
    /// Basic Auth password for a remote `tuic-remote` daemon, keyed by the
    /// connection's UUID. `connections.json` holds the username and never this.
    RemoteConnection(&'a str),
    /// Pairing token used by a TUIC-deployed daemon. It shares the connection
    /// UUID namespace but never enters the plain-text connection store.
    RemotePairingToken(&'a str),
    /// Relay pairing bundle (relay bearer and end-to-end secret), keyed by the
    /// desktop connection UUID, or by a fixed id on the daemon (story 2085-721d).
    RelayPairing(&'a str),
}

impl Credential<'_> {
    fn vault_key(&self) -> String {
        match self {
            Self::AiChatApiKey => "ai-chat/api-key".into(),
            Self::LlmApiKey => "llm-api/api-key".into(),
            Self::GithubOauthToken => "account/github.com".into(),
            Self::CircleCiToken => "circleci/token".into(),
            Self::RemoteSessionToken => "remote/session-token".into(),
            Self::RelayToken => "remote/relay-token".into(),
            Self::PushVapidPrivateKey => "remote/push-vapid-private-key".into(),
            Self::GithubToken(id) | Self::AccountToken(id) => format!("account/{id}"),
            Self::McpUpstream(name) => format!("mcp/{name}"),
            Self::RemoteConnection(id) => format!("remote/connection/{id}/password"),
            Self::RemotePairingToken(id) => {
                format!("remote/connection/{id}/pairing-token")
            }
            Self::RelayPairing(id) => format!("remote/relay-pairing/{id}"),
        }
    }

    fn legacy_entry(&self) -> Option<(&str, &str)> {
        match self {
            Self::AiChatApiKey => Some(("tuicommander-ai-chat", "api-key")),
            Self::LlmApiKey => Some(("tuicommander-llm-api", "api-key")),
            Self::GithubOauthToken => Some(("tuicommander-github", "oauth-token")),
            Self::McpUpstream(name) => Some((MCP_UPSTREAM_LEGACY_SERVICE, name)),
            Self::RemoteSessionToken
            | Self::RelayToken
            | Self::PushVapidPrivateKey
            | Self::GithubToken(_)
            | Self::AccountToken(_)
            | Self::CircleCiToken
            | Self::RemoteConnection(_)
            | Self::RemotePairingToken(_)
            | Self::RelayPairing(_) => None,
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
    let instance = crate::app_instance::current_app_instance();
    let entry =
        keyring::Entry::new(instance.vault_service(), instance.vault_user()).map_err(|e| {
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
    // Read-fault injection means "the vault is unreachable", so a cache some
    // other test happened to populate first must not answer in its place —
    // otherwise the fault never reaches the mock and the assertion becomes
    // order-dependent. Dropping it under the same guard leaves no window for a
    // concurrent test to repopulate before the forced failure is observed.
    #[cfg(any(test, feature = "test-support"))]
    if MOCK_FAIL_READS.load(std::sync::atomic::Ordering::SeqCst) {
        guard.take();
    }

    if guard.is_some() {
        return Ok(());
    }

    #[cfg(all(debug_assertions, not(test)))]
    dev_store::init();

    #[cfg(any(test, feature = "test-support"))]
    ensure_test_keyring();

    let instance = crate::app_instance::current_app_instance();
    let mut vault: Vault = match read_keyring_entry(
        instance.vault_service(),
        instance.vault_user(),
    )? {
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

    // Migrate in one vault commit. Canonical slots win; retain every legacy copy
    // until persistence succeeds, including the ambient github.com OAuth token.
    let old_tokens: Vec<_> = vault
        .iter()
        .filter_map(|(key, value)| {
            let id = if key == "github/oauth-token" {
                Some("github.com")
            } else {
                key.strip_prefix("github/account/")
                    .and_then(|key| key.strip_suffix("/token"))
            }?;
            Some((key.clone(), format!("account/{id}"), value.clone()))
        })
        .collect();
    let migrated_tokens = !old_tokens.is_empty();
    for (old_key, new_key, value) in old_tokens {
        vault.entry(new_key).or_insert(value);
        vault.remove(&old_key);
    }

    // The default instance always sweeps legacy entries — handles stragglers when
    // its vault was created before all legacy keys were migrated. Persist the
    // merged vault BEFORE deleting any legacy copy, so a persist failure (locked
    // keychain, circuit breaker open) can never lose a secret from both locations
    // (#116-1cb4). Named instances never inspect that global namespace.
    let mut to_delete: Vec<(&str, &str)> = Vec::new();
    if instance.is_default() {
        for &(service, user) in LEGACY_ENTRIES {
            if let Ok(Some(value)) = read_keyring_entry(service, user) {
                let cred = match (service, user) {
                    ("tuicommander-ai-chat", "api-key") => Credential::AiChatApiKey,
                    ("tuicommander-llm-api", "api-key") => Credential::LlmApiKey,
                    ("tuicommander-github", "oauth-token") => Credential::GithubOauthToken,
                    _ => unreachable!(),
                };
                vault.entry(cred.vault_key()).or_insert(value);
                to_delete.push((service, user));
            }
        }
    }
    if migrated_tokens || !to_delete.is_empty() {
        // vault is necessarily non-empty here (we or_insert'd for each swept
        // entry). Persist first; only then remove the legacy copies.
        persist(&vault)?;
        for (service, user) in &to_delete {
            delete_keyring_entry(service, user);
        }
    }
    **guard = Some(vault);
    Ok(())
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn get(cred: Credential<'_>) -> Result<Option<String>, String> {
    let key = cred.vault_key();
    let mut guard = lock();
    load(&mut guard)?;

    if let Some(value) = guard.as_ref().unwrap().get(&key) {
        return Ok(Some(value.clone()));
    }
    drop(guard);

    // Lazy migration for dynamic keys only (MCP upstreams aren't in LEGACY_ENTRIES).
    // Static credentials are swept in load() — no extra keychain prompts.
    if crate::app_instance::current_app_instance().is_default()
        && matches!(cred, Credential::McpUpstream(_))
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

pub fn set(cred: Credential<'_>, value: &str) -> Result<(), String> {
    let key = cred.vault_key();
    let trimmed = value.trim().to_string();
    let mut guard = lock();
    load(&mut guard)?;
    let mut next = guard.as_ref().unwrap().clone();
    next.insert(key, trimmed);
    persist(&next)?;
    *guard = Some(next);
    Ok(())
}

pub fn delete(cred: Credential<'_>) -> Result<(), String> {
    let key = cred.vault_key();
    let mut guard = lock();
    load(&mut guard)?;
    let mut next = guard.as_ref().unwrap().clone();
    next.remove(&key);
    persist(&next)?;
    *guard = Some(next);
    Ok(())
}

/// Fail a named instance before it binds if its vault namespace is unreadable.
/// The default instance has nothing to prove here: `load()` already sweeps it.
pub fn probe_named_vault_read() -> Result<(), String> {
    let instance = crate::app_instance::current_app_instance();
    if instance.is_default() {
        return Ok(());
    }

    #[cfg(all(debug_assertions, not(test)))]
    dev_store::init();

    #[cfg(any(test, feature = "test-support"))]
    ensure_test_keyring();

    read_keyring_entry(instance.vault_service(), instance.vault_user()).map(|_| ())
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
        let instance = crate::app_instance::current_app_instance();
        let mut dir = home.join(".tuicommander-dev");
        if let Some(id) = instance.named_id() {
            dir = dir.join("instances").join(id);
        }
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

/// Test-only fault injection: when set, the mock keyring rejects `set_password`
/// (simulating a locked keychain on write) while reads still succeed. Used to
/// prove the legacy sweep never deletes a legacy entry before the merged vault
/// is durably persisted (#116-1cb4).
#[cfg(any(test, feature = "test-support"))]
pub static MOCK_FAIL_WRITES: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Test-only fault injection for READS: the mock rejects `get_password` with a real
/// error rather than `NoEntry`. That distinction is the whole point — a vault that
/// cannot be consulted must never be mistaken for a vault holding nothing, or the
/// config layer deletes a perfectly good secret (#488-5576).
#[cfg(any(test, feature = "test-support"))]
pub static MOCK_FAIL_READS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(any(test, feature = "test-support"))]
pub fn reset_test_faults() {
    use std::sync::atomic::Ordering;
    MOCK_FAIL_READS.store(false, Ordering::SeqCst);
    MOCK_FAIL_WRITES.store(false, Ordering::SeqCst);
    let mut cb = CIRCUIT.lock().unwrap_or_else(|e| e.into_inner());
    cb.failures = 0;
    cb.last_failure = None;
}

#[cfg(any(test, feature = "test-support"))]
fn ensure_test_keyring() {
    // Integration tests enable test-support for the daemon binary too. Let the
    // named-vault test exercise the debug file store in its isolated HOME.
    #[cfg(all(debug_assertions, not(test)))]
    if std::env::var("TUIC_TEST_USE_FILE_VAULT").as_deref() == Ok("1") {
        return;
    }
    ensure_mock_keyring();
}

#[cfg(any(test, feature = "test-support"))]
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
            if MOCK_FAIL_WRITES.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(Error::Invalid("mock".into(), "forced write failure".into()));
            }
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
            if MOCK_FAIL_READS.load(std::sync::atomic::Ordering::SeqCst) {
                // NOT NoEntry: this is "the vault is unreachable", which callers must
                // treat differently from "the secret is not there".
                return Err(Error::Invalid("mock".into(), "forced read failure".into()));
            }
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
        // Install the mock BEFORE any Entry::new — otherwise this delete hits
        // the user's real OS keychain (load() installs the mock too late).
        ensure_mock_keyring();
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
            "account/github.com"
        );
        assert_eq!(Credential::McpUpstream("foo").vault_key(), "mcp/foo");
        assert_eq!(Credential::CircleCiToken.vault_key(), "circleci/token");
    }

    // The key is the connection's UUID, which is what makes `delete_remote_connection`
    // able to clean up after itself: a secret keyed by anything else would outlive
    // the only name that could ever reach it.
    #[test]
    fn remote_connection_vault_key_is_uuid_scoped() {
        assert_eq!(
            Credential::RemoteConnection("2f1c8a3e-0000-4000-8000-000000000001").vault_key(),
            "remote/connection/2f1c8a3e-0000-4000-8000-000000000001/password"
        );
        // A new feature: there is no legacy keyring slot to migrate from.
        assert!(Credential::RemoteConnection("any").legacy_entry().is_none());
    }

    #[test]
    fn remote_connection_pairing_token_crud() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_vault();
        let credential = Credential::RemotePairingToken("connection-id");
        assert_eq!(
            credential.vault_key(),
            "remote/connection/connection-id/pairing-token"
        );
        assert!(credential.legacy_entry().is_none());

        set(credential.clone(), "pair-secret").unwrap();
        assert_eq!(
            get(credential.clone()).unwrap().as_deref(),
            Some("pair-secret")
        );
        delete(credential.clone()).unwrap();
        assert_eq!(get(credential).unwrap(), None);
    }

    #[test]
    fn github_token_vault_key_is_account_scoped() {
        assert_eq!(
            Credential::GithubToken("ghe.acme.com").vault_key(),
            "account/ghe.acme.com"
        );
    }

    #[test]
    fn github_token_has_no_legacy_entry() {
        // Per-account PATs are a new feature — there is no legacy keyring slot to
        // migrate from (only github.com's OAuth token has one, unchanged).
        assert!(Credential::GithubToken("acc1").legacy_entry().is_none());
        assert_eq!(
            Credential::GithubOauthToken.vault_key(),
            "account/github.com"
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
    fn failed_set_does_not_mutate_the_cached_vault() {
        use std::sync::atomic::Ordering;

        let _guard = TEST_LOCK.lock().unwrap();
        reset_vault();
        reset_test_faults();
        set(Credential::AiChatApiKey, "old").unwrap();
        MOCK_FAIL_WRITES.store(true, Ordering::SeqCst);
        assert!(set(Credential::AiChatApiKey, "new").is_err());
        MOCK_FAIL_WRITES.store(false, Ordering::SeqCst);
        assert_eq!(
            get(Credential::AiChatApiKey).unwrap(),
            Some("old".to_string())
        );
        reset_test_faults();
    }

    #[test]
    fn failed_delete_does_not_mutate_the_cached_vault() {
        use std::sync::atomic::Ordering;

        let _guard = TEST_LOCK.lock().unwrap();
        reset_vault();
        reset_test_faults();
        set(Credential::AiChatApiKey, "keep").unwrap();
        MOCK_FAIL_WRITES.store(true, Ordering::SeqCst);
        assert!(delete(Credential::AiChatApiKey).is_err());
        MOCK_FAIL_WRITES.store(false, Ordering::SeqCst);
        assert_eq!(
            get(Credential::AiChatApiKey).unwrap(),
            Some("keep".to_string())
        );
        reset_test_faults();
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
    fn legacy_sweep_persist_failure_keeps_legacy_entries() {
        use std::sync::atomic::Ordering;
        let _guard = TEST_LOCK.lock().unwrap();
        reset_circuit_breaker();
        reset_vault();

        // Plant two legacy entries the sweep would migrate.
        let legacy_chat = keyring::Entry::new("tuicommander-ai-chat", "api-key").unwrap();
        legacy_chat.set_password("legacy-chat").unwrap();
        let legacy_llm = keyring::Entry::new("tuicommander-llm-api", "api-key").unwrap();
        legacy_llm.set_password("legacy-llm").unwrap();

        // Force the vault persist() to fail — simulates a locked keychain /
        // circuit breaker opening between reading the legacy entries and writing
        // the merged vault.
        MOCK_FAIL_WRITES.store(true, Ordering::SeqCst);
        simulate_restart();
        let result = get(Credential::AiChatApiKey);
        MOCK_FAIL_WRITES.store(false, Ordering::SeqCst);

        // The persist failure must propagate…
        assert!(result.is_err(), "expected persist failure to propagate");
        // …and NO legacy entry may have been deleted: both copies survive, so the
        // secret is never lost from both locations.
        assert_eq!(legacy_chat.get_password().unwrap(), "legacy-chat");
        assert_eq!(legacy_llm.get_password().unwrap(), "legacy-llm");

        reset_circuit_breaker();
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
    // Catches: account-vault migration dropping legacy tokens or overwriting canonical credentials.
    #[test]
    fn account_token_migration_preserves_every_legacy_slot_and_canonical_precedence() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_vault();
        let mut legacy = Vault::new();
        legacy.insert("github/oauth-token".into(), "legacy-default".into());
        legacy.insert("github/account/alice/token".into(), "legacy-alice".into());
        legacy.insert(
            "github/account/enterprise/token".into(),
            "legacy-enterprise".into(),
        );
        legacy.insert("account/alice".into(), "canonical-alice".into());
        persist(&legacy).unwrap();
        simulate_restart();
        assert_eq!(
            get(Credential::AccountToken("github.com"))
                .unwrap()
                .as_deref(),
            Some("legacy-default")
        );
        assert_eq!(
            get(Credential::GithubToken("alice")).unwrap().as_deref(),
            Some("canonical-alice")
        );
        assert_eq!(
            get(Credential::AccountToken("enterprise"))
                .unwrap()
                .as_deref(),
            Some("legacy-enterprise")
        );
        assert!(
            lock()
                .as_ref()
                .unwrap()
                .keys()
                .all(|key| !key.starts_with("github/"))
        );
        simulate_restart();
        assert_eq!(
            get(Credential::GithubOauthToken).unwrap().as_deref(),
            Some("legacy-default")
        );
        assert_eq!(
            get(Credential::GithubToken("enterprise"))
                .unwrap()
                .as_deref(),
            Some("legacy-enterprise")
        );
        reset_vault();
    }

    // Catches: a failed vault commit deleting the only durable copy of a GitHub token.
    #[test]
    fn account_token_migration_write_failure_leaves_legacy_copy_recoverable() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_vault();
        let mut legacy = Vault::new();
        legacy.insert(
            "github/account/recoverable/token".into(),
            "recoverable-token".into(),
        );
        persist(&legacy).unwrap();
        simulate_restart();
        MOCK_FAIL_WRITES.store(true, std::sync::atomic::Ordering::SeqCst);
        let failed = get(Credential::AccountToken("recoverable"));
        MOCK_FAIL_WRITES.store(false, std::sync::atomic::Ordering::SeqCst);
        assert!(failed.is_err());
        simulate_restart();
        assert_eq!(
            get(Credential::AccountToken("recoverable"))
                .unwrap()
                .as_deref(),
            Some("recoverable-token")
        );
        reset_vault();
    }
}
