//! The last visit of an issue or pull request panel, so the next visit can say
//! how many comments are new. One row per work item URL in `tuic.sqlite3`;
//! rows not visited within `retention.work_item_visit_days` are deleted.

use std::path::{Path, PathBuf};

use axum::{Json, response::Response};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

/// Last visits of work item panels in `tuic.sqlite3`.
pub(crate) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS work_item_visits (
   item_url      TEXT PRIMARY KEY,
   comment_count INTEGER NOT NULL,
   visited_ms    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS work_item_visits_visited ON work_item_visits (visited_ms);";

/// Comments added since the previous visit. The first visit has no previous
/// visit and reports none, so a panel never calls every existing comment new.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NewComments {
    pub count: i64,
    /// Time of the previous visit; a comment created after it is new.
    pub since_ms: Option<i64>,
}

/// Records this visit and returns what is new since the previous one.
pub(crate) fn visit(
    path: &Path,
    item_url: &str,
    comment_count: i64,
    now_ms: i64,
) -> Result<NewComments, String> {
    if item_url.is_empty() {
        return Err("work item visit: empty item URL".to_owned());
    }
    let mut conn = crate::db::open_with(path, &crate::db::TUIC_SCHEMA)
        .map_err(|e| format!("work item visits: {e}"))?;
    let tx = conn
        .transaction()
        .map_err(|e| format!("work item visits: {e}"))?;
    let previous = tx
        .query_row(
            "SELECT comment_count, visited_ms FROM work_item_visits WHERE item_url = ?1",
            [item_url],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(|e| format!("read work item visit: {e}"))?;
    tx.execute(
        "INSERT INTO work_item_visits (item_url, comment_count, visited_ms) VALUES (?1, ?2, ?3)
         ON CONFLICT(item_url) DO UPDATE SET comment_count = ?2, visited_ms = ?3",
        params![item_url, comment_count, now_ms],
    )
    .map_err(|e| format!("record work item visit: {e}"))?;
    tx.commit()
        .map_err(|e| format!("record work item visit: {e}"))?;
    Ok(match previous {
        // A deleted comment lowers the count; it never makes a negative number new.
        Some((count, visited_ms)) => NewComments {
            count: (comment_count - count).max(0),
            since_ms: Some(visited_ms),
        },
        None => NewComments {
            count: 0,
            since_ms: None,
        },
    })
}

fn db_path() -> PathBuf {
    crate::db::path(crate::db::TUIC_DB)
}

async fn visit_now(item_url: String, comment_count: i64) -> Result<NewComments, String> {
    tokio::task::spawn_blocking(move || {
        visit(
            &db_path(),
            &item_url,
            comment_count,
            crate::db::retention::now_ms(),
        )
    })
    .await
    .map_err(|e| format!("work item visit task: {e}"))?
}

/// Records a visit of an issue or pull request panel; returns what is new since the previous one.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn visit_work_item(
    item_url: String,
    comment_count: i64,
) -> Result<NewComments, String> {
    visit_now(item_url, comment_count).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VisitBody {
    item_url: String,
    comment_count: i64,
}

/// HTTP twin of [`visit_work_item`].
pub(crate) async fn endpoint(Json(body): Json<VisitBody>) -> Response {
    crate::mcp_http::json_result(visit_now(body.item_url, body.comment_count).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://github.com/acme/api/issues/7";

    fn store() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::Builder::new()
            .tempdir_in(crate::test_support::test_temp_root())
            .unwrap();
        let path = dir.path().join(crate::db::TUIC_DB);
        (dir, path)
    }

    fn new(count: i64, since_ms: Option<i64>) -> NewComments {
        NewComments { count, since_ms }
    }

    // Catches: a first visit reporting every existing comment as new.
    #[test]
    fn first_visit_reports_nothing_new() {
        let (_dir, path) = store();
        assert_eq!(visit(&path, URL, 4, 1_000).unwrap(), new(0, None));
    }

    // Catches: comparing with the visit just recorded (always 0 new), or never updating it.
    #[test]
    fn each_visit_compares_with_the_one_before() {
        let (_dir, path) = store();
        visit(&path, URL, 2, 1_000).unwrap();
        assert_eq!(visit(&path, URL, 4, 2_000).unwrap(), new(2, Some(1_000)));
        assert_eq!(visit(&path, URL, 4, 3_000).unwrap(), new(0, Some(2_000)));
    }

    // Catches: one item's visit answering for another item.
    #[test]
    fn visits_are_kept_per_item() {
        let (_dir, path) = store();
        visit(&path, URL, 2, 1_000).unwrap();
        let other = "https://github.com/acme/api/pull/8";
        assert_eq!(visit(&path, other, 5, 2_000).unwrap(), new(0, None));
    }

    // Catches: a deleted comment showing as a negative new count.
    #[test]
    fn a_lower_count_reports_nothing_new() {
        let (_dir, path) = store();
        visit(&path, URL, 5, 1_000).unwrap();
        assert_eq!(visit(&path, URL, 3, 2_000).unwrap(), new(0, Some(1_000)));
    }

    #[test]
    fn rejects_an_empty_url() {
        let (_dir, path) = store();
        assert!(visit(&path, "", 1, 1_000).is_err());
    }
}
