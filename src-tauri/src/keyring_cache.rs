//! Cached keyring access — reads the OS keyring once, caches in-memory.
//!
//! Every `get` returns the cached value (or populates from the OS keyring on
//! first access). `set` and `delete` write-through to the keyring AND update
//! the cache atomically. Callers never hit the OS keyring twice for the same
//! (service, user) pair unless the cache is explicitly invalidated.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

type CacheMap = HashMap<(String, String), Option<String>>;
type CacheGuard<'a> = MutexGuard<'a, Option<CacheMap>>;

static CACHE: Mutex<Option<CacheMap>> = Mutex::new(None);

const KNOWN_KEYS: &[(&str, &str)] = &[
    ("tuicommander-ai-chat", "api-key"),
    ("tuicommander-llm-api", "api-key"),
    ("tuicommander-github", "oauth-token"),
];

fn cache_map() -> CacheGuard<'static> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner())
}

fn read_from_keyring(service: &str, user: &str) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(service, user)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;
    match entry.get_password() {
        Ok(v) => Ok(Some(v.trim().to_string())),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Failed to read keyring ({service}/{user}): {e}")),
    }
}

fn warm_up(guard: &mut CacheGuard<'_>) {
    if guard.is_some() {
        return;
    }
    let mut map = HashMap::new();
    for &(service, user) in KNOWN_KEYS {
        if let Ok(value) = read_from_keyring(service, user) {
            map.insert((service.to_string(), user.to_string()), value);
        }
    }
    **guard = Some(map);
}

pub(crate) fn get(service: &str, user: &str) -> Result<Option<String>, String> {
    let key = (service.to_string(), user.to_string());
    let mut guard = cache_map();
    warm_up(&mut guard);
    let map = guard.as_ref().unwrap();

    if let Some(cached) = map.get(&key) {
        return Ok(cached.clone());
    }
    drop(guard);

    let value = read_from_keyring(service, user)?;

    let mut guard = cache_map();
    // Map is guaranteed to exist after warm_up
    guard.as_mut().unwrap().insert(key, value.clone());
    Ok(value)
}

pub(crate) fn set(service: &str, user: &str, value: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(service, user)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;
    entry
        .set_password(value)
        .map_err(|e| format!("Failed to save keyring ({service}/{user}): {e}"))?;

    let key = (service.to_string(), user.to_string());
    let mut guard = cache_map();
    warm_up(&mut guard);
    guard.as_mut().unwrap().insert(key, Some(value.trim().to_string()));
    Ok(())
}

pub(crate) fn delete(service: &str, user: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(service, user)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(e) => return Err(format!("Failed to delete keyring ({service}/{user}): {e}")),
    }

    let key = (service.to_string(), user.to_string());
    let mut guard = cache_map();
    warm_up(&mut guard);
    guard.as_mut().unwrap().insert(key, None);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_returns_same_value_without_keyring() {
        let mut guard = cache_map();
        warm_up(&mut guard);
        let key = ("test-service".to_string(), "test-user".to_string());
        guard.as_mut().unwrap().insert(key, Some("cached-value".to_string()));
        drop(guard);

        let result = get("test-service", "test-user").unwrap();
        assert_eq!(result, Some("cached-value".to_string()));
    }

    #[test]
    fn cache_returns_none_for_deleted() {
        let mut guard = cache_map();
        warm_up(&mut guard);
        let key = ("test-del".to_string(), "test-user".to_string());
        guard.as_mut().unwrap().insert(key, None);
        drop(guard);

        let result = get("test-del", "test-user").unwrap();
        assert_eq!(result, None);
    }
}
