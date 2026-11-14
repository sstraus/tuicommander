//! Review coordinates are computed from saved editor snapshots, never Git history.

#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn markdown_review_changed_lines(
    path: String,
    baseline: Option<String>,
    expected: String,
) -> Result<Option<String>, String> {
    crate::fs::spawn_blocking_fs(move || {
        let Some(baseline) = baseline else {
            return Ok(None);
        };
        if crate::read_external_file_impl(&path)? != expected {
            return Ok(None);
        }
        Ok(tuic_git::text_diff::changed_line_ranges(
            &baseline, &expected,
        ))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::markdown_review_changed_lines;

    // Catches: checkbox-only edits are missed, offsets use old-file coordinates,
    // or separated changes are collapsed into one large read.
    #[tokio::test]
    async fn markdown_review_ranges_use_current_lines_and_preserve_gaps() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let path = dir
            .path()
            .join("untracked.md")
            .to_string_lossy()
            .into_owned();
        let before = "# Review\n- [ ] Pick\nKeep\nOld\nLast\n";
        let after = "# Review\n- [x] Pick\nKeep\nNew\nExtra\nLast\n";
        std::fs::write(&path, after).unwrap();
        assert_eq!(
            markdown_review_changed_lines(path, Some(before.into()), after.into())
                .await
                .unwrap(),
            Some("2, 4-5".into())
        );
    }

    // Catches: a stale or absent snapshot narrows a read and hides external edits.
    #[tokio::test]
    async fn markdown_review_ranges_fall_back_for_missing_stale_or_unchanged_snapshots() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let path = dir.path().join("review.md").to_string_lossy().into_owned();
        std::fs::write(&path, "external\n").unwrap();
        for (baseline, expected) in [
            (None, "external\n"),
            (Some("old\n"), "saved\n"),
            (Some("external\n"), "external\n"),
        ] {
            assert_eq!(
                markdown_review_changed_lines(
                    path.clone(),
                    baseline.map(str::to_owned),
                    expected.into()
                )
                .await
                .unwrap(),
                None
            );
        }
    }

    // Catches: deletion-only hunks disappear or point beyond the surviving file.
    #[tokio::test]
    async fn markdown_review_ranges_anchor_deletions_and_handle_crlf_unicode() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let path = dir.path().join("review.md").to_string_lossy().into_owned();
        for (before, after, ranges) in [
            ("drop\nkeep\n", "keep\n", "1"),
            ("keep\ndrop\n", "keep\n", "1"),
            ("drop\n", "", "1"),
            ("a\n", "a", "1"),
            ("# Résumé\r\n- [ ] Sì\r\n", "# Résumé\r\n- [x] Sì\r\n", "2"),
            ("a\nb\nc\nd\n", "a\ninsert\nb\nc\nD\n", "2, 5"),
        ] {
            std::fs::write(&path, after).unwrap();
            assert_eq!(
                markdown_review_changed_lines(path.clone(), Some(before.into()), after.into())
                    .await
                    .unwrap(),
                Some(ranges.into())
            );
        }
    }
}
