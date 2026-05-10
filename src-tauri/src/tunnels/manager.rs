use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use parking_lot::Mutex;

use super::audit::{AuditLog, EventKind};
use super::profile::TunnelProfile;
use super::supervisor::{TunnelStatus, TunnelSupervisor};

pub struct TunnelHandle {
    pub profile: TunnelProfile,
    pub supervisor: TunnelSupervisor,
    pub started_at: DateTime<Utc>,
}

pub struct TunnelManager {
    tunnels: DashMap<String, Arc<Mutex<TunnelHandle>>>,
    /// Wrapped in Mutex so Arc<Mutex<AuditLog>> is Send+Sync and can be
    /// captured in the `Send + 'static` status callback required by TunnelSupervisor.
    audit: Arc<Mutex<AuditLog>>,
}

impl TunnelManager {
    pub fn new(audit: Arc<Mutex<AuditLog>>) -> Self {
        Self {
            tunnels: DashMap::new(),
            audit,
        }
    }

    /// Start a tunnel for `profile`, store its handle, and log the Started event.
    /// Returns the profile id on success.
    pub async fn start(&self, profile: TunnelProfile) -> Result<String, String> {
        let id = profile.id.clone();
        let audit = Arc::clone(&self.audit);
        let audit_cb = Arc::clone(&self.audit);
        let cb_id = id.clone();

        let status_callback = move |status: TunnelStatus| {
            let kind = match &status {
                TunnelStatus::Connected => EventKind::Connected,
                TunnelStatus::Reconnecting { .. } => EventKind::Retry,
                TunnelStatus::Stopped { .. } => EventKind::Stopped,
                TunnelStatus::Error { .. } => EventKind::Error,
                TunnelStatus::Starting => return, // Starting is logged via Started below
            };
            let detail = match &status {
                TunnelStatus::Error { message } => serde_json::json!({ "message": message }),
                TunnelStatus::Stopped { reason } => serde_json::json!({ "reason": reason }),
                TunnelStatus::Reconnecting { attempt, reason } => {
                    serde_json::json!({ "attempt": attempt, "reason": reason })
                }
                _ => serde_json::json!({}),
            };
            let _ = audit_cb.lock().insert(&cb_id, kind, detail);
        };

        let supervisor = TunnelSupervisor::start(profile.clone(), status_callback).await;

        // Log Started event.
        let _ = audit
            .lock()
            .insert(&id, EventKind::Started, serde_json::json!({}));

        let handle = Arc::new(Mutex::new(TunnelHandle {
            profile,
            supervisor,
            started_at: Utc::now(),
        }));

        self.tunnels.insert(id.clone(), handle);
        Ok(id)
    }

    /// Stop the tunnel with `id`, remove it from the map, and log a Stopped event.
    pub fn stop(&self, id: &str) -> Result<(), String> {
        let handle = self
            .tunnels
            .remove(id)
            .map(|(_, v)| v)
            .ok_or_else(|| format!("tunnel '{id}' not found"))?;

        handle.lock().supervisor.stop();
        let _ = self.audit.lock().insert(
            id,
            EventKind::Stopped,
            serde_json::json!({"reason": "stop requested"}),
        );
        Ok(())
    }

    /// Stop the tunnel if it exists, ignoring "not found".
    pub fn stop_if_running(&self, id: &str) {
        if let Some((_, handle)) = self.tunnels.remove(id) {
            handle.lock().supervisor.stop();
            let _ = self.audit.lock().insert(
                id,
                EventKind::Stopped,
                serde_json::json!({"reason": "stop requested"}),
            );
        }
    }

    /// Return all tunnel ids with their current status.
    pub fn list(&self) -> Vec<(String, TunnelStatus)> {
        self.tunnels
            .iter()
            .map(|entry| {
                let id = entry.key().clone();
                let status = entry.value().lock().supervisor.status();
                (id, status)
            })
            .collect()
    }

    /// Return the current status of a single tunnel, or `None` if not found.
    pub fn get_status(&self, id: &str) -> Option<TunnelStatus> {
        self.tunnels
            .get(id)
            .map(|entry| entry.value().lock().supervisor.status())
    }

    /// Stop all running tunnels and clear the map. Used on app exit.
    pub fn shutdown_all(&self) {
        for entry in self.tunnels.iter() {
            entry.value().lock().supervisor.stop();
        }
        self.tunnels.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn fake_ssh_script(behavior: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "#!/bin/sh\n{behavior}").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(f.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        f
    }

    fn test_profile(name: &str) -> TunnelProfile {
        TunnelProfile {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            host: "example.com".to_string(),
            port: 22,
            user: "alice".to_string(),
            identity_file: None,
            forwards: Vec::new(),
            options: super::super::profile::ProfileOptions::default(),
            auto_connect: false,
        }
    }

    fn temp_audit() -> (Arc<Mutex<AuditLog>>, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let audit = Arc::new(Mutex::new(
            AuditLog::open(&dir.path().join("audit.db")).unwrap(),
        ));
        (audit, dir)
    }

    /// Start a tunnel using the fake ssh binary via `start_with_binary`.
    async fn start_with_fake_ssh(
        manager: &TunnelManager,
        profile: TunnelProfile,
        ssh_path: PathBuf,
    ) -> Result<String, String> {
        let id = profile.id.clone();
        let audit = Arc::clone(&manager.audit);
        let audit_cb = Arc::clone(&manager.audit);
        let cb_id = id.clone();

        let status_callback = move |status: TunnelStatus| {
            let kind = match &status {
                TunnelStatus::Connected => EventKind::Connected,
                TunnelStatus::Reconnecting { .. } => EventKind::Retry,
                TunnelStatus::Stopped { .. } => EventKind::Stopped,
                TunnelStatus::Error { .. } => EventKind::Error,
                TunnelStatus::Starting => return,
            };
            let detail = match &status {
                TunnelStatus::Error { message } => serde_json::json!({ "message": message }),
                TunnelStatus::Stopped { reason } => serde_json::json!({ "reason": reason }),
                TunnelStatus::Reconnecting { attempt, reason } => {
                    serde_json::json!({ "attempt": attempt, "reason": reason })
                }
                _ => serde_json::json!({}),
            };
            let _ = audit_cb.lock().insert(&cb_id, kind, detail);
        };

        let supervisor =
            TunnelSupervisor::start_with_binary(profile.clone(), ssh_path, status_callback).await;

        let _ = audit
            .lock()
            .insert(&id, EventKind::Started, serde_json::json!({}));

        let handle = Arc::new(Mutex::new(TunnelHandle {
            profile,
            supervisor,
            started_at: Utc::now(),
        }));

        manager.tunnels.insert(id.clone(), handle);
        Ok(id)
    }

    #[tokio::test]
    async fn create_and_list() {
        let (audit, _dir) = temp_audit();
        let manager = TunnelManager::new(audit);
        // Sleep-forever script so the tunnel stays alive for the assertion.
        let script = fake_ssh_script("sleep 3600");

        let id = start_with_fake_ssh(&manager, test_profile("t1"), script.path().to_path_buf())
            .await
            .unwrap();

        let list = manager.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, id);

        manager.stop(&id).unwrap();
    }

    #[tokio::test]
    async fn stop_removes_from_map() {
        let (audit, _dir) = temp_audit();
        let manager = TunnelManager::new(audit);
        let script = fake_ssh_script("sleep 3600");

        let id = start_with_fake_ssh(&manager, test_profile("t2"), script.path().to_path_buf())
            .await
            .unwrap();

        manager.stop(&id).unwrap();

        assert!(manager.list().is_empty(), "map should be empty after stop");
    }

    #[tokio::test]
    async fn get_status_returns_correct_status() {
        let (audit, _dir) = temp_audit();
        let manager = TunnelManager::new(audit);
        let script = fake_ssh_script("sleep 3600");

        let id = start_with_fake_ssh(&manager, test_profile("t3"), script.path().to_path_buf())
            .await
            .unwrap();

        let status = manager.get_status(&id);
        assert!(
            status.is_some(),
            "status should be Some for a running tunnel"
        );
        match status.unwrap() {
            TunnelStatus::Starting | TunnelStatus::Connected => {} // either is valid right after start
            other => panic!("unexpected status: {other:?}"),
        }

        manager.stop(&id).unwrap();
    }

    #[tokio::test]
    async fn shutdown_all_clears_everything() {
        let (audit, _dir) = temp_audit();
        let manager = TunnelManager::new(audit);
        let script = fake_ssh_script("sleep 3600");

        for i in 0..3 {
            start_with_fake_ssh(
                &manager,
                test_profile(&format!("t{i}")),
                script.path().to_path_buf(),
            )
            .await
            .unwrap();
        }

        assert_eq!(manager.list().len(), 3);

        manager.shutdown_all();

        assert!(
            manager.list().is_empty(),
            "all tunnels should be removed after shutdown_all"
        );
    }

    #[tokio::test]
    async fn concurrent_starts_no_panic() {
        let (audit, _dir) = temp_audit();
        let manager = Arc::new(TunnelManager::new(audit));
        let script = fake_ssh_script("sleep 3600");
        let ssh_path = script.path().to_path_buf();

        // Spawn 10 tasks concurrently via tokio::spawn. Arc<Mutex<AuditLog>> and
        // Arc<TunnelManager> are Send+Sync so this is safe.
        let tasks: Vec<_> = (0..10_usize)
            .map(|i| {
                let manager = Arc::clone(&manager);
                let path = ssh_path.clone();
                tokio::spawn(async move {
                    start_with_fake_ssh(&manager, test_profile(&format!("concurrent-{i}")), path)
                        .await
                        .unwrap()
                })
            })
            .collect();

        let mut ids: Vec<String> = Vec::new();
        for t in tasks {
            ids.push(t.await.unwrap());
        }

        let list = manager.list();
        assert_eq!(list.len(), 10, "all 10 tunnels should be present");

        for id in &ids {
            assert!(
                list.iter().any(|(k, _)| k == id),
                "tunnel {id} missing from list"
            );
        }

        manager.shutdown_all();
    }
}
