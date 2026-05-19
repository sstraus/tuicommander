//! Remote connection config model.
//!
//! Persists named connections (SSH or Direct) to `connections.json` in the
//! app config directory. Each connection has a UUID, a human-readable name,
//! a transport, auth info, and an enabled flag.

use std::path::Path;

use serde::{Deserialize, Serialize};

const CONNECTIONS_FILE: &str = "connections.json";

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A saved remote connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RemoteConnection {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) transport: RemoteTransport,
    pub(crate) auth_username: String,
    pub(crate) enabled: bool,
}

/// Transport layer for a remote connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum RemoteTransport {
    Ssh {
        ssh_host: String,
        ssh_port: u16,
        ssh_user: String,
        identity_file: Option<String>,
        remote_daemon_port: u16,
    },
    Direct {
        url: String,
    },
}

impl RemoteConnection {
    /// Create a new SSH connection with default port (22) and daemon port (9877).
    pub(crate) fn new_ssh(
        name: impl Into<String>,
        host: impl Into<String>,
        user: impl Into<String>,
    ) -> Self {
        let ssh_user = user.into();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            transport: RemoteTransport::Ssh {
                ssh_host: host.into(),
                ssh_port: 22,
                ssh_user: ssh_user.clone(),
                identity_file: None,
                remote_daemon_port: 9877,
            },
            auth_username: ssh_user,
            enabled: true,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if uuid::Uuid::parse_str(&self.id).is_err() {
            return Err("id must be a valid UUID".to_string());
        }
        if self.name.trim().is_empty() {
            return Err("name must not be empty".to_string());
        }
        if self.auth_username.trim().is_empty() {
            return Err("auth_username must not be empty".to_string());
        }
        match &self.transport {
            RemoteTransport::Ssh {
                ssh_host,
                ssh_user,
                ssh_port,
                ..
            } => {
                if ssh_host.trim().is_empty() {
                    return Err("ssh_host must not be empty".to_string());
                }
                if ssh_user.trim().is_empty() {
                    return Err("ssh_user must not be empty".to_string());
                }
                if *ssh_port == 0 {
                    return Err("ssh_port must be in range 1-65535".to_string());
                }
            }
            RemoteTransport::Direct { url } => {
                if url.trim().is_empty() {
                    return Err("url must not be empty".to_string());
                }
            }
        }
        Ok(())
    }

    /// Create a new Direct connection.
    pub(crate) fn new_direct(
        name: impl Into<String>,
        url: impl Into<String>,
        auth_username: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            transport: RemoteTransport::Direct { url: url.into() },
            auth_username: auth_username.into(),
            enabled: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

pub(crate) struct RemoteConnectionStore;

impl RemoteConnectionStore {
    /// Load connections from `<config_dir>/connections.json`.
    /// Returns an empty vec if the file does not exist.
    pub(crate) fn load(config_dir: &Path) -> anyhow::Result<Vec<RemoteConnection>> {
        let path = config_dir.join(CONNECTIONS_FILE);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {e}", path.display()))?;
        let connections = serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse {}: {e}", path.display()))?;
        Ok(connections)
    }

    /// Save connections to `<config_dir>/connections.json` atomically.
    pub(crate) fn save(config_dir: &Path, connections: &[RemoteConnection]) -> anyhow::Result<()> {
        std::fs::create_dir_all(config_dir)
            .map_err(|e| anyhow::anyhow!("Failed to create config dir: {e}"))?;
        let json = serde_json::to_string_pretty(connections)
            .map_err(|e| anyhow::anyhow!("Failed to serialize connections: {e}"))?;
        let target = config_dir.join(CONNECTIONS_FILE);
        let temp = target.with_extension(format!("tmp.{}", std::process::id()));
        std::fs::write(&temp, json.as_bytes())
            .map_err(|e| anyhow::anyhow!("Failed to write temp file: {e}"))?;
        std::fs::rename(&temp, &target).map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            anyhow::anyhow!("Failed to commit connections file: {e}")
        })?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn list_remote_connections(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<Vec<RemoteConnection>, String> {
    RemoteConnectionStore::load(&state.data_dir).map_err(|e| e.to_string())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn save_remote_connection(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
    connection: RemoteConnection,
) -> Result<(), String> {
    let _guard = state.connections_lock.lock().await;
    let mut connections =
        RemoteConnectionStore::load(&state.data_dir).map_err(|e| e.to_string())?;
    if let Some(existing) = connections.iter_mut().find(|c| c.id == connection.id) {
        *existing = connection;
    } else {
        connections.push(connection);
    }
    RemoteConnectionStore::save(&state.data_dir, &connections).map_err(|e| e.to_string())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn delete_remote_connection(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
    id: String,
) -> Result<(), String> {
    let _guard = state.connections_lock.lock().await;
    let mut connections =
        RemoteConnectionStore::load(&state.data_dir).map_err(|e| e.to_string())?;
    connections.retain(|c| c.id != id);
    RemoteConnectionStore::save(&state.data_dir, &connections).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_json_round_trip() {
        let conn = RemoteConnection::new_ssh("my-server", "example.com", "alice");
        let json = serde_json::to_string(&conn).unwrap();
        let decoded: RemoteConnection = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, conn.name);
        assert_eq!(decoded.auth_username, conn.auth_username);
        assert!(decoded.enabled);
        match decoded.transport {
            RemoteTransport::Ssh {
                ssh_host,
                ssh_port,
                ssh_user,
                identity_file,
                remote_daemon_port,
            } => {
                assert_eq!(ssh_host, "example.com");
                assert_eq!(ssh_port, 22);
                assert_eq!(ssh_user, "alice");
                assert!(identity_file.is_none());
                assert_eq!(remote_daemon_port, 9877);
            }
            other => panic!("expected Ssh, got {other:?}"),
        }
    }

    #[test]
    fn direct_json_round_trip() {
        let conn = RemoteConnection::new_direct("office", "http://office:9877", "bob");
        let json = serde_json::to_string(&conn).unwrap();
        let decoded: RemoteConnection = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "office");
        assert_eq!(decoded.auth_username, "bob");
        assert!(decoded.enabled);
        match decoded.transport {
            RemoteTransport::Direct { url } => assert_eq!(url, "http://office:9877"),
            other => panic!("expected Direct, got {other:?}"),
        }
    }

    #[test]
    fn serde_tag_produces_correct_type_field() {
        let ssh = RemoteConnection::new_ssh("s", "h", "u");
        let ssh_json = serde_json::to_string(&ssh).unwrap();
        let ssh_val: serde_json::Value = serde_json::from_str(&ssh_json).unwrap();
        assert_eq!(ssh_val["transport"]["type"], "Ssh");

        let direct = RemoteConnection::new_direct("d", "http://x", "u");
        let direct_json = serde_json::to_string(&direct).unwrap();
        let direct_val: serde_json::Value = serde_json::from_str(&direct_json).unwrap();
        assert_eq!(direct_val["transport"]["type"], "Direct");
    }

    #[test]
    fn store_save_then_load_returns_same_data() {
        let dir = tempfile::tempdir().unwrap();
        let conn1 = RemoteConnection::new_ssh("server1", "host1.example.com", "alice");
        let conn2 = RemoteConnection::new_direct("direct1", "http://10.0.0.1:9877", "bob");
        let connections = vec![conn1, conn2];

        RemoteConnectionStore::save(dir.path(), &connections).unwrap();
        let loaded = RemoteConnectionStore::load(dir.path()).unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].name, "server1");
        assert_eq!(loaded[1].name, "direct1");
        assert_eq!(loaded[0].id, connections[0].id);
        assert_eq!(loaded[1].id, connections[1].id);
    }

    #[test]
    fn store_load_nonexistent_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = RemoteConnectionStore::load(dir.path()).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn new_ssh_defaults() {
        let conn = RemoteConnection::new_ssh("test", "myhost", "myuser");
        assert!(conn.enabled);
        match conn.transport {
            RemoteTransport::Ssh {
                ssh_port,
                remote_daemon_port,
                ssh_user,
                ..
            } => {
                assert_eq!(ssh_port, 22);
                assert_eq!(remote_daemon_port, 9877);
                assert_eq!(ssh_user, "myuser");
            }
            other => panic!("expected Ssh, got {other:?}"),
        }
        assert_eq!(conn.auth_username, "myuser");
    }

    #[test]
    fn new_direct_enabled() {
        let conn = RemoteConnection::new_direct("d", "http://x", "u");
        assert!(conn.enabled);
    }

    #[test]
    fn validate_valid_ssh_connection() {
        let conn = RemoteConnection::new_ssh("server", "host.example.com", "alice");
        assert!(conn.validate().is_ok());
    }

    #[test]
    fn validate_invalid_uuid_rejected() {
        let mut conn = RemoteConnection::new_ssh("server", "host", "alice");
        conn.id = "../../malicious".to_string();
        let err = conn.validate().unwrap_err();
        assert!(
            err.contains("valid UUID"),
            "expected UUID error, got: {err}"
        );
    }

    #[test]
    fn validate_whitespace_name_rejected() {
        let conn = RemoteConnection::new_ssh("  ", "host", "alice");
        assert!(conn.validate().is_err());
    }

    #[test]
    fn validate_empty_ssh_host_rejected() {
        let conn = RemoteConnection::new_ssh("s", "", "alice");
        assert!(conn.validate().is_err());
    }

    #[test]
    fn validate_empty_url_rejected() {
        let conn = RemoteConnection::new_direct("d", "  ", "u");
        assert!(conn.validate().is_err());
    }
}
