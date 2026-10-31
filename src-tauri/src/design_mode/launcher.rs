//! Ordinary Chrome/Edge launch with a dedicated repository profile and native host.
use crate::AppState;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
const EXTENSION_ID: &str = "eefbgjmmpaimpeaflioojmdipmfchefo";
fn browser(choice: Option<&str>) -> Result<(String, PathBuf), String> {
    if let Some(choice) = choice
        && !matches!(choice, "chrome" | "edge")
    {
        return Err("Choose Chrome or Edge".into());
    }
    let candidates = super::browser::chrome_candidates();
    let matches = |path: &Path| {
        let name = path.to_string_lossy().to_lowercase();
        match choice {
            Some("edge") => name.contains("edge"),
            Some("chrome") => !name.contains("edge"),
            _ => true,
        }
    };
    let executable = candidates
        .into_iter()
        .find(|path| path.is_file() && matches(path))
        .ok_or("The chosen Chrome/Edge browser is not installed")?;
    let kind = if executable.to_string_lossy().to_lowercase().contains("edge") {
        "edge"
    } else {
        "chrome"
    };
    Ok((kind.into(), executable))
}
fn onboarding(profile: &Path) -> Result<String, String> {
    let path = profile.join("design-mode.html");
    let html = "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>Design Mode</title><style>body{font:16px system-ui;max-width:780px;margin:8vh auto;background:#1e1e1e;color:#ccc;line-height:1.6}h1{font-size:28px}</style><h1>Design Mode</h1><p>No single dev server was found. Open the TUICommander Design Mode extension icon to choose a server or start your configured dev server, or enter your page URL in the address bar.</p><p>The sidebar collects your picks and comments before you send them to an agent. Ordinary browsing works until you click Pick element.</p></html>";
    std::fs::write(&path, html).map_err(|e| e.to_string())?;
    url::Url::from_file_path(path)
        .map(|url| url.to_string())
        .map_err(|_| "Cannot open Design Mode setup page".into())
}
fn installed(profile: &Path) -> bool {
    ["Default/Secure Preferences", "Default/Preferences"]
        .iter()
        .any(|file| {
            std::fs::read(profile.join(file))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .is_some_and(|prefs| {
                    prefs
                        .pointer(&format!("/extensions/settings/{EXTENSION_ID}"))
                        .is_some()
                })
        })
}

/// Never forward a load flag to a running browser: Chromium ignores it there.
fn ensure_profile_closed(profile: &Path) -> Result<(), String> {
    if profile_running(profile)? {
        let previous = if installed(profile) {
            " The installed extension may still use an older repository binding."
        } else {
            " The extension cannot be loaded into an already running profile."
        };
        return Err(format!(
            "Close the Design Mode browser window for {} and start Design Mode again.{previous}",
            profile.display()
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn profile_running(profile: &Path) -> Result<bool, String> {
    let lock = match std::fs::read_link(profile.join("SingletonLock")) {
        Ok(lock) => lock,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("Cannot inspect Design Mode profile lock: {error}")),
    };
    let pid = lock
        .to_string_lossy()
        .rsplit('-')
        .next()
        .and_then(|pid| pid.parse::<i32>().ok())
        .filter(|pid| *pid > 0)
        .ok_or(
            "Cannot identify the Design Mode profile owner; close its browser window and try again",
        )?;
    // SAFETY: signal zero only observes the lock owner; it never sends a signal.
    Ok(unsafe { libc::kill(pid, 0) } == 0
        || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH))
}

#[cfg(windows)]
fn profile_running(profile: &Path) -> Result<bool, String> {
    use std::os::windows::fs::OpenOptionsExt;
    // Chromium owns lockfile with GENERIC_WRITE, FILE_SHARE_READ, DELETE_ON_CLOSE.
    match std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(profile.join("lockfile"))
    {
        Ok(_) => Ok(false),
        Err(error) if error.raw_os_error() == Some(32) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("Cannot inspect Design Mode profile lock: {error}")),
    }
}

fn ensure_autoload_supported(executable: &Path) -> Result<(), String> {
    let name = executable
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let path = executable
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase();
    if path.contains("/google/chrome/")
        || name == "google chrome"
        || name == "google chrome canary"
        || name.starts_with("google-chrome")
        || name == "chrome.exe"
    {
        return Err("Google Chrome branded builds (137+) do not support automatic Design Mode extension loading. Choose Microsoft Edge or Chromium instead.".into());
    }
    Ok(())
}

pub(crate) async fn launch(
    state: &Arc<AppState>,
    repo: &Path,
    session: &str,
    choice: Option<&str>,
    url: Option<&str>,
) -> Result<Value, String> {
    validate_url(url)?;
    let (kind, executable) = browser(choice)?;
    let profile = super::browser::profile_dir(repo).join(&kind);
    ensure_autoload_supported(&executable)?;
    ensure_profile_closed(&profile)?;
    let bridge = crate::agent_mcp::locate_bridge_binary()
        .ok_or("Design Mode native host is missing; rebuild the bundled tuic-bridge sidecar")?;
    #[cfg(unix)]
    let endpoint = state
        .bound_socket_path
        .read()
        .to_string_lossy()
        .into_owned();
    #[cfg(windows)]
    let endpoint = tuic_ipc::PIPE_NAME.to_owned();
    if endpoint.is_empty() {
        return Err("This instance has no bound local IPC endpoint".into());
    }
    let output = tokio::process::Command::new(bridge)
        .arg("--design-install")
        .arg(&profile)
        .arg(repo)
        .arg(&endpoint)
        .arg(session)
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "Native host installation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let discovery = super::discovery::discover(state, repo, Some(session)).await;
    let selected = discovery
        .as_ref()
        .ok()
        .and_then(|value| value["servers"].as_array())
        .filter(|servers| servers.len() == 1)
        .and_then(|servers| servers[0]["url"].as_str())
        .map(str::to_owned);
    let target = match url.map(str::to_owned).or(selected) {
        Some(target) => target,
        None => onboarding(&profile)?,
    };
    ensure_profile_closed(&profile)?;
    let mut command = std::process::Command::new(executable);
    command
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg(format!(
            "--load-extension={}",
            profile.join("tuic-design-extension").display()
        ))
        .args(["--no-first-run", "--no-default-browser-check"])
        .arg(&target);
    let manager = super::legacy::manager(state).await;
    // Publish the queue before the worker can poll it. An armed return still requires a reply.
    let repo_path = repo.to_string_lossy().into_owned();
    let status = manager
        .arm_extension(repo_path.clone(), session.to_owned())
        .await;
    let process = match super::extension_process::ExtensionProcess::spawn(command).await {
        Ok(process) => process,
        Err(error) => {
            let _ = manager.stop(&repo_path).await;
            return Err(error);
        }
    };
    let channel = manager
        .command_channels
        .lock()
        .unwrap()
        .get(&repo_path)
        .cloned();
    let ready = match channel {
        Some(channel) => {
            channel
                .request("ready", json!({"url":target}), session)
                .await
        }
        None => Err("Design Mode start was cancelled".into()),
    };
    let ready = ready.and_then(|reply| {
        if reply["ok"] == true && reply["result"]["tabId"].as_i64().is_some() {
            Ok(())
        } else {
            Err(reply["error"]
                .as_str()
                .unwrap_or("Extension did not identify the launched page")
                .to_owned())
        }
    });
    if let Err(error) = ready {
        let _ = manager.stop(&repo_path).await;
        process.close().await?;
        return Err(format!(
            "browser_not_ready: Design Mode extension did not acknowledge startup: {error}"
        ));
    }
    manager
        .attach_extension_process(&repo_path, session, process)
        .await?;
    Ok(
        json!({"repoPath":status.repo_path,"sessionId":status.session_id,"status":status.status,"browser":kind,"url":target,"extensionPath":profile.join("tuic-design-extension")}),
    )
}

#[cfg(all(test, unix))]
#[path = "launcher_tests.rs"]
mod tests;

/// Check the navigation contract before any native host or browser side effects.
pub(crate) fn validate_url(url: Option<&str>) -> Result<(), String> {
    let Some(url) = url else {
        return Ok(());
    };
    let parsed = url::Url::parse(url)
        .map_err(|_| "Design Mode URL must be an absolute HTTP or HTTPS URL")?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("Design Mode URL must be an absolute HTTP or HTTPS URL".into());
    }
    Ok(())
}

#[cfg(test)]
mod url_tests {
    use super::*;

    // Catches: Edge stores extensions.settings in Secure Preferences, not Preferences.
    #[test]
    fn installed_finds_edge_extension_in_secure_preferences() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        std::fs::create_dir(dir.path().join("Default")).unwrap();
        std::fs::write(dir.path().join("Default/Preferences"), "{}").unwrap();
        std::fs::write(
            dir.path().join("Default/Secure Preferences"),
            include_str!("fixtures/edge-extension-settings.json"),
        )
        .unwrap();
        assert!(installed(dir.path()));
        // Catches: malformed Secure Preferences hiding a valid legacy Preferences entry.
        std::fs::rename(
            dir.path().join("Default/Secure Preferences"),
            dir.path().join("Default/Preferences"),
        )
        .unwrap();
        std::fs::write(dir.path().join("Default/Secure Preferences"), "invalid").unwrap();
        assert!(installed(dir.path()));
        std::fs::write(dir.path().join("Default/Preferences"), "{}").unwrap();
        assert!(!installed(dir.path()));
    }

    #[test]
    fn url_contract_rejects_non_web_navigation_before_launch() {
        // Catches: file/custom schemes or empty URL reaching a browser launch.
        for url in [
            "",
            "localhost:3000",
            "file:///tmp/page.html",
            "javascript:alert(1)",
            "ftp://example.com",
        ] {
            assert!(validate_url(Some(url)).is_err(), "{url}");
        }
        for url in [
            "http://localhost:3000/page?x=1#target",
            "https://example.com/path",
            "http://127.0.0.1:5173",
        ] {
            assert!(validate_url(Some(url)).is_ok(), "{url}");
        }
        assert!(validate_url(None).is_ok());
    }
}

#[cfg(all(test, unix))]
#[path = "launcher_reaping_tests.rs"]
mod reaping_tests;

#[cfg(all(test, unix))]
fn acknowledge_test_launch(state: Arc<AppState>, repo: PathBuf) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let manager = super::legacy::manager(&state).await;
        let repo = repo.to_string_lossy();
        let channel = loop {
            let channel = manager
                .command_channels
                .lock()
                .unwrap()
                .get(repo.as_ref())
                .cloned();
            if let Some(channel) = channel {
                break channel;
            }
            tokio::task::yield_now().await;
        };
        let command = channel.next().await.unwrap();
        assert_eq!(command["action"], "ready");
        assert!(channel.reply(
            command["requestId"].as_str().unwrap(),
            json!({"ok":true,"result":{"tabId":7}})
        ));
    })
}
