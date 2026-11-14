//! Install ego from its latest GitHub release and select it in Settings.

use std::path::Path;
use std::sync::Arc;

use tauri::State;

use crate::AppState;

const EGO_REPO: &str = "sstraus/ego";

/// Download the latest ego release for this platform, install it, and save
/// its path as the ego executable. Returns the installed path.
#[tauri::command]
pub async fn install_ego(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    install_ego_from(
        &state,
        &crate::release_install::latest_download_base(EGO_REPO),
        &crate::release_install::install_path("ego"),
    )
    .await
}

async fn install_ego_from(
    state: &AppState,
    base_url: &str,
    install_path: &Path,
) -> Result<String, String> {
    crate::release_install::download_and_install(
        base_url,
        &crate::release_install::asset_name("ego"),
        install_path,
    )
    .await?;
    let path = install_path.display().to_string();
    tracing::info!(source = "ego", path = %path, "ego installed");

    crate::config::commit_config_change(state, |current| {
        let mut next = current.clone();
        next.ego_executable = path.clone();
        Ok(next)
    })
    .map_err(|e| format!("Installed ego at {path} but failed to save the setting: {e}"))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    // RED: install_sets_the_ego_executable_setting — catches: the ego install
    // writes the binary but leaves the ego executable setting empty.
    #[tokio::test]
    #[serial_test::serial]
    async fn install_sets_the_ego_executable_setting() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().join("config"));
        let state = crate::state::tests_support::make_test_app_state();

        let asset = crate::release_install::asset_name("ego");
        let app = axum::Router::new().route(
            &format!("/{asset}"),
            axum::routing::get(|| async { b"ego-binary".to_vec() }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let install_path = dir.path().join("bin").join("ego");
        let installed = install_ego_from(&state, &base_url, &install_path)
            .await
            .unwrap();

        assert_eq!(std::fs::read(&install_path).unwrap(), b"ego-binary");
        assert_eq!(installed, install_path.display().to_string());
        assert_eq!(state.config.read().ego_executable, installed);
        assert_eq!(
            crate::config::load_app_config().ego_executable,
            installed,
            "the setting must survive a restart"
        );
    }
}
