use std::sync::Arc;

use serde::Serialize;

use super::profile::TunnelProfile;
use super::storage::ProfileStore;
use crate::AppState;

#[tauri::command]
pub(crate) fn list_tunnel_profiles(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<TunnelProfile>, String> {
    ProfileStore::load_all(&state.data_dir, None).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn save_tunnel_profile(
    state: tauri::State<'_, Arc<AppState>>,
    mut profile: serde_json::Value,
) -> Result<String, String> {
    if profile
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .is_empty()
    {
        profile["id"] = serde_json::Value::String(uuid::Uuid::new_v4().to_string());
    }
    let mut profile: TunnelProfile = serde_json::from_value(profile).map_err(|e| e.to_string())?;
    profile.validate()?;
    let id = profile.id.clone();
    ProfileStore::save(&state.data_dir, &profile).map_err(|e| e.to_string())?;
    Ok(id)
}

#[tauri::command]
pub(crate) fn delete_tunnel_profile(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<bool, String> {
    state.tunnel_manager.stop_if_running(&id);
    ProfileStore::delete(&state.data_dir, None, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn start_tunnel(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<String, String> {
    let profiles = ProfileStore::load_all(&state.data_dir, None).map_err(|e| e.to_string())?;
    let profile = profiles
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| "profile not found".to_string())?;
    state.tunnel_manager.start(profile).await
}

#[tauri::command]
pub(crate) fn stop_tunnel(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    state.tunnel_manager.stop(&id)
}

#[tauri::command]
pub(crate) fn list_active_tunnels(
    state: tauri::State<'_, Arc<AppState>>,
) -> Vec<serde_json::Value> {
    state
        .tunnel_manager
        .list()
        .into_iter()
        .map(|(id, status)| {
            serde_json::json!({
                "id": id,
                "status": status_to_frontend(&status),
                "started_at": chrono::Utc::now().to_rfc3339(),
            })
        })
        .collect()
}

#[tauri::command]
pub(crate) fn get_tunnel_status(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<serde_json::Value, String> {
    state
        .tunnel_manager
        .get_status(&id)
        .map(|status| {
            serde_json::json!({
                "id": id,
                "status": status_to_frontend(&status),
                "started_at": chrono::Utc::now().to_rfc3339(),
            })
        })
        .ok_or_else(|| "tunnel not found".to_string())
}

#[tauri::command]
pub(crate) fn list_ssh_config_hosts() -> Vec<String> {
    let config_path = match dirs::home_dir() {
        Some(h) => h.join(".ssh").join("config"),
        None => return Vec::new(),
    };

    let file = match std::fs::File::open(&config_path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let mut reader = std::io::BufReader::new(file);
    let config = match ssh2_config::SshConfig::default()
        .parse(&mut reader, ssh2_config::ParseRule::ALLOW_UNKNOWN_FIELDS)
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    config
        .get_hosts()
        .iter()
        .flat_map(|host| {
            host.pattern.iter().filter_map(|clause| {
                if clause.negated || clause.pattern == "*" {
                    None
                } else {
                    Some(clause.pattern.clone())
                }
            })
        })
        .collect()
}

#[tauri::command]
pub(crate) fn get_tunnel_audit(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
    limit: Option<usize>,
) -> Result<Vec<serde_json::Value>, String> {
    let limit = limit.unwrap_or(20);
    let events = state
        .tunnel_audit
        .lock()
        .query_by_tunnel(&id, limit)
        .map_err(|e| e.to_string())?;

    Ok(events
        .into_iter()
        .map(|e| {
            let message = extract_audit_message(&e.detail);
            serde_json::json!({
                "tunnel_id": e.tunnel_id,
                "timestamp": e.timestamp.to_rfc3339(),
                "kind": e.kind,
                "message": message,
            })
        })
        .collect())
}

fn extract_audit_message(detail: &serde_json::Value) -> Option<String> {
    // Try structured fields first
    if let Some(msg) = detail.get("message").and_then(|v| v.as_str()) {
        return Some(msg.to_string());
    }
    if let Some(msg) = detail.get("reason").and_then(|v| v.as_str()) {
        return Some(msg.to_string());
    }
    // The status callback stores Debug repr: {"status": "Error { message: \"...\" }"}
    if let Some(status_str) = detail.get("status").and_then(|v| v.as_str()) {
        if let Some(start) = status_str.find("message: \"") {
            let rest = &status_str[start + 10..];
            if let Some(end) = rest.find('"') {
                return Some(rest[..end].to_string());
            }
        }
        if let Some(start) = status_str.find("reason: \"") {
            let rest = &status_str[start + 9..];
            if let Some(end) = rest.find('"') {
                return Some(rest[..end].to_string());
            }
        }
        return Some(status_str.to_string());
    }
    // Fallback: stringify non-empty detail
    if !detail.is_null() && detail != &serde_json::json!({}) {
        return Some(detail.to_string());
    }
    None
}

fn status_to_frontend(status: &super::supervisor::TunnelStatus) -> serde_json::Value {
    use super::supervisor::TunnelStatus;
    match status {
        TunnelStatus::Starting => serde_json::json!({"type": "starting"}),
        TunnelStatus::Connected => serde_json::json!({"type": "connected"}),
        TunnelStatus::Reconnecting { attempt, reason } => {
            serde_json::json!({"type": "reconnecting", "attempt": attempt, "reason": reason})
        }
        TunnelStatus::Stopped { reason } => {
            serde_json::json!({"type": "stopped", "reason": reason})
        }
        TunnelStatus::Error { message } => {
            serde_json::json!({"type": "error", "message": message})
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AgentKey {
    pub fingerprint: String,
    pub comment: String,
    pub key_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SshAgentInfo {
    pub keys: Vec<AgentKey>,
    pub agent_type: String,
}

fn detect_agent_type() -> String {
    let sock = std::env::var("SSH_AUTH_SOCK").unwrap_or_default();

    if sock.contains("1password") || sock.contains("2BUA8C4S2C") {
        return "1Password".to_string();
    }
    if sock.contains("secretive") {
        return "Secretive".to_string();
    }
    if sock.contains("gpg") || sock.contains("gnupg") {
        return "GPG Agent".to_string();
    }

    // 1Password socket exists but isn't the active SSH_AUTH_SOCK
    if let Some(home) = dirs::home_dir() {
        let op_sock = home.join("Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock");
        if op_sock.exists() {
            return "SSH Agent (1Password available)".to_string();
        }
    }

    if sock.is_empty() {
        "Not available".to_string()
    } else {
        "SSH Agent".to_string()
    }
}

#[tauri::command]
pub(crate) async fn list_ssh_agent_keys() -> Result<SshAgentInfo, String> {
    let agent_type = detect_agent_type();

    let output = tokio::process::Command::new("ssh-add")
        .arg("-l")
        .output()
        .await
        .map_err(|e| format!("failed to run ssh-add: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("no identities") || output.status.code() == Some(1) {
            return Ok(SshAgentInfo {
                keys: Vec::new(),
                agent_type,
            });
        }
        return Err(format!("ssh-add failed: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let keys = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(4, ' ').collect();
            if parts.len() >= 3 {
                Some(AgentKey {
                    fingerprint: parts[1].to_string(),
                    comment: parts[2].to_string(),
                    key_type: parts
                        .get(3)
                        .map(|s| s.trim_matches(|c| c == '(' || c == ')').to_string())
                        .unwrap_or_default(),
                })
            } else {
                None
            }
        })
        .collect();

    Ok(SshAgentInfo { keys, agent_type })
}
