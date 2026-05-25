use anyhow::{Result, bail};
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::Mutex;

use crate::mdkb_client::MdkbClient;
use crate::plugin_exec::resolve_binary;

const DAEMON_SPAWN_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

pub struct MdkbDaemon {
    client: Option<MdkbClient>,
    binary_path: Option<PathBuf>,
    cached_version: Option<String>,
}

impl MdkbDaemon {
    pub fn new() -> Self {
        let binary_path = resolve_binary("mdkb").map(PathBuf::from);
        let cached_version = binary_path.as_ref().and_then(|bin| {
            let output = std::process::Command::new(bin)
                .arg("--version")
                .output()
                .ok()?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let version = stdout.trim().strip_prefix("mdkb ").unwrap_or(stdout.trim());
            Some(version.to_string())
        });
        Self {
            client: None,
            binary_path,
            cached_version,
        }
    }

    pub fn is_available(&self) -> bool {
        self.binary_path.as_ref().is_some_and(|p| p.exists())
    }

    pub fn is_connected(&self) -> bool {
        self.client.is_some()
    }

    pub fn binary_path(&self) -> Option<&std::path::Path> {
        self.binary_path.as_deref()
    }

    pub fn version(&self) -> Option<String> {
        self.cached_version.clone()
    }

    pub async fn ensure_running(&mut self) -> Result<&mut MdkbClient> {
        if let Some(ref mut c) = self.client {
            if c.ping().await.is_ok() {
                return Ok(self.client.as_mut().unwrap());
            }
            self.client = None;
        }

        if let Ok(c) = MdkbClient::connect().await {
            self.client = Some(c);
            if self.client.as_mut().unwrap().ping().await.is_ok() {
                return Ok(self.client.as_mut().unwrap());
            }
            self.client = None;
        }

        self.spawn_daemon().await?;

        let client = MdkbClient::connect().await?;
        self.client = Some(client);
        Ok(self.client.as_mut().unwrap())
    }

    async fn spawn_daemon(&self) -> Result<()> {
        let bin = self
            .binary_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("mdkb binary not found in trusted directories"))?;

        Command::new(bin)
            .args(["serve", "--daemon", "--detach"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        let deadline = tokio::time::Instant::now() + DAEMON_SPAWN_TIMEOUT;

        while tokio::time::Instant::now() < deadline {
            if let Ok(mut c) = MdkbClient::connect().await
                && c.ping().await.is_ok()
            {
                return Ok(());
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }

        bail!(
            "mdkb daemon did not start within {}s",
            DAEMON_SPAWN_TIMEOUT.as_secs()
        );
    }
}

pub type SharedMdkbDaemon = Mutex<MdkbDaemon>;

pub fn create_shared_daemon() -> SharedMdkbDaemon {
    Mutex::new(MdkbDaemon::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_without_binary_is_not_available() {
        // In test env, mdkb may or may not be installed
        let daemon = MdkbDaemon {
            client: None,
            binary_path: None,
            cached_version: None,
        };
        assert!(!daemon.is_available());
    }

    #[test]
    fn new_with_binary_is_available() {
        // Use a path guaranteed to exist
        let daemon = MdkbDaemon {
            client: None,
            binary_path: Some(PathBuf::from(env!("CARGO_MANIFEST_DIR"))),
            cached_version: None,
        };
        assert!(daemon.is_available());
    }

    #[test]
    fn stale_cached_path_not_available() {
        let daemon = MdkbDaemon {
            client: None,
            binary_path: Some(PathBuf::from("/nonexistent/mdkb")),
            cached_version: None,
        };
        assert!(!daemon.is_available());
    }

    #[tokio::test]
    async fn ensure_running_without_binary_uses_existing_daemon() {
        let mut daemon = MdkbDaemon {
            client: None,
            binary_path: None,
            cached_version: None,
        };
        let result = daemon.ensure_running().await;
        if MdkbClient::socket_path().exists() {
            assert!(result.is_ok(), "should connect to running daemon");
        } else {
            assert!(result.unwrap_err().to_string().contains("not found"));
        }
    }

    #[tokio::test]
    async fn spawn_daemon_fails_when_no_binary() {
        let daemon = MdkbDaemon {
            client: None,
            binary_path: None,
            cached_version: None,
        };
        let err = daemon.spawn_daemon().await.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }
}
