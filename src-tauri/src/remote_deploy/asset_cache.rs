//! Rotation of the `remote-bin/<version>/` release asset cache.

use std::path::{Path, PathBuf};

/// Runs before every asset lookup: keep the version in use plus the newest
/// other one, delete the rest. Each version holds 23–60 MB per target, and
/// every release added one that was never removed. The extra version covers a
/// second install of another version (a dev build next to a release) that
/// shares this config directory.
pub(super) async fn prune_other_versions(root: &Path, current: &str) {
    let root = root.to_path_buf();
    let current = current.to_string();
    let pruned = tokio::task::spawn_blocking(move || {
        let Ok(entries) = std::fs::read_dir(&root) else {
            return;
        };
        let mut others: Vec<(std::time::SystemTime, PathBuf)> = entries
            .flatten()
            .filter(|entry| entry.file_name() != current.as_str())
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| {
                let modified = entry
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);
                (modified, entry.path())
            })
            .collect();
        others.sort_by_key(|entry| std::cmp::Reverse(entry.0));
        for (_, directory) in others.into_iter().skip(1) {
            if let Err(error) = std::fs::remove_dir_all(&directory) {
                tracing::warn!(
                    source = "remote",
                    %error,
                    "Cannot remove old tuic-remote cache {}",
                    directory.display()
                );
            }
        }
    })
    .await;
    if let Err(error) = pruned {
        tracing::warn!(source = "remote", %error, "tuic-remote cache rotation failed");
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn serving_a_version_keeps_it_and_the_newest_other_version_only() {
        let config = tempfile::tempdir().expect("temporary config directory");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let root = config.path().join("remote-bin");
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000);
        for (version, age) in [("0.9.0", 3), ("1.0.0", 2), ("1.1.0", 1)] {
            let directory = root.join(version);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join("tuic-remote-test-target"), b"old").unwrap();
            std::fs::File::open(&directory)
                .and_then(|file| file.set_modified(old + std::time::Duration::from_secs(100 - age)))
                .ok();
        }
        let current = root.join("1.2.3");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::write(current.join("tuic-remote-test-target"), b"remote binary").unwrap();

        super::super::assets::ensure_local_from_url(
            "1.2.3",
            "test-target",
            "http://127.0.0.1:1",
            std::time::Duration::from_secs(300),
        )
        .await
        .expect("cached file needs no network");

        let mut kept: Vec<String> = std::fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        kept.sort();
        assert_eq!(kept, ["1.1.0", "1.2.3"]);
    }
}
