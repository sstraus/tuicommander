//! SQLite adapter for the gateway's L2 cache port: `provider_cache.sqlite3`,
//! a disposable cache of provider response bodies. Its retention is the size
//! cap and idle limit that `tuic_gateway::l2` passes to `evict_to`; a file of
//! another layout is deleted, never migrated or refused. Rules:
//! `docs/backend/persistence.md`.

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use futures::future::{BoxFuture, FutureExt};
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use tuic_gateway::cache::CacheKey;
use tuic_gateway::l2::{ColdStore, StoredEntry};

use crate::db::{self, Schema};

const KEY_COLUMNS: &str = "account_id = ?1 AND api_base = ?2 AND method = ?3 AND url = ?4 \
                           AND accept = ?5 AND gql_hash = ?6 AND gql_vars = ?7";

const SCHEMA: Schema = Schema {
    name: "provider_cache",
    migrations: &[&[
        "CREATE TABLE l2_entries (
            id INTEGER PRIMARY KEY,
            account_id TEXT NOT NULL, api_base TEXT NOT NULL, method TEXT NOT NULL,
            url TEXT NOT NULL, accept TEXT NOT NULL, gql_hash TEXT NOT NULL, gql_vars TEXT NOT NULL,
            status INTEGER NOT NULL, headers TEXT NOT NULL, body BLOB NOT NULL,
            etag TEXT, last_modified TEXT,
            fetched_ms INTEGER NOT NULL, ttl_base_ms INTEGER NOT NULL, ttl_mult REAL NOT NULL,
            last_used_ms INTEGER NOT NULL, size INTEGER NOT NULL,
            UNIQUE (account_id, api_base, method, url, accept, gql_hash, gql_vars)
        )",
        "CREATE INDEX l2_entries_last_used ON l2_entries (last_used_ms)",
    ]],
};

pub(crate) struct SqliteColdStore {
    conn: Arc<Mutex<Connection>>,
}

fn to_ms(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn from_ms(ms: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(ms.max(0) as u64)
}

fn key_params(k: &CacheKey) -> [String; 7] {
    let (hash, vars) = k
        .graphql
        .as_ref()
        .map_or((String::new(), String::new()), |g| {
            (g.query_hash.clone(), g.variables.clone())
        });
    [
        k.account.account_id.clone(),
        k.account.api_base.to_string(),
        k.method.clone(),
        k.url.clone(),
        k.accept.clone(),
        hash,
        vars,
    ]
}

/// The cache is disposable: a file of another layout (or an unreadable one) is
/// deleted so the open below starts clean.
fn discard_if_unusable(path: &Path) {
    if !path.exists() {
        return;
    }
    let expected = SCHEMA.migrations.len() as i64;
    let usable = db::open_read_only(path)
        .and_then(|c| {
            c.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .map_err(|e| e.to_string())
        })
        .is_ok_and(|v| v == 0 || v == expected);
    if usable {
        return;
    }
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        let _ = std::fs::remove_file(name);
    }
}

impl SqliteColdStore {
    pub(crate) fn open(path: &Path) -> Result<Self, String> {
        discard_if_unusable(path);
        let conn = db::open_with(path, &SCHEMA)?;
        restrict_to_owner(path);
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Runs `f` on a blocking thread; a failure is logged and yields `None`,
    /// because a cache that cannot be read or written is only a miss.
    fn run<T: Send + 'static>(
        &self,
        what: &'static str,
        f: impl FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
    ) -> BoxFuture<'static, Option<T>> {
        let conn = Arc::clone(&self.conn);
        async move {
            let result = tokio::task::spawn_blocking(move || {
                let mut conn = conn.lock().unwrap_or_else(PoisonError::into_inner);
                f(&mut conn)
            })
            .await;
            match result {
                Ok(Ok(v)) => Some(v),
                Ok(Err(error)) => {
                    tracing::warn!(source = "gateway", %error, what, "L2 cache operation failed");
                    None
                }
                Err(error) => {
                    tracing::warn!(source = "gateway", %error, what, "L2 cache task failed");
                    None
                }
            }
        }
        .boxed()
    }
}

#[cfg(unix)]
fn restrict_to_owner(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_to_owner(_path: &Path) {}

impl ColdStore for SqliteColdStore {
    fn get(&self, key: &CacheKey, now: SystemTime) -> BoxFuture<'static, Option<StoredEntry>> {
        let key = key.clone();
        let fut = self.run("get", move |conn| {
            let tx = conn.transaction()?;
            let row = tx
                .query_row(
                    &format!(
                        "SELECT status, headers, body, etag, last_modified, fetched_ms,
                                ttl_base_ms, ttl_mult FROM l2_entries WHERE {KEY_COLUMNS}"
                    ),
                    params_from_iter(key_params(&key)),
                    |r| {
                        Ok(StoredEntry {
                            key: key.clone(),
                            status: r.get(0)?,
                            headers: r.get(1)?,
                            body_zstd: Bytes::from(r.get::<_, Vec<u8>>(2)?),
                            etag: r.get(3)?,
                            last_modified: r.get(4)?,
                            fetched_at: from_ms(r.get(5)?),
                            ttl_base: Duration::from_millis(r.get::<_, i64>(6)?.max(0) as u64),
                            ttl_mult: r.get(7)?,
                            last_used: now,
                        })
                    },
                )
                .optional()?;
            if row.is_some() {
                tx.execute(
                    &format!("UPDATE l2_entries SET last_used_ms = ?8 WHERE {KEY_COLUMNS}"),
                    params_from_iter(key_params(&key).into_iter().chain([to_ms(now).to_string()])),
                )?;
            }
            tx.commit()?;
            Ok(row)
        });
        async move { fut.await.flatten() }.boxed()
    }

    fn put_batch(&self, rows: Vec<StoredEntry>) -> BoxFuture<'static, ()> {
        let fut = self.run("put_batch", move |conn| {
            let tx = conn.transaction()?;
            for r in &rows {
                let size = r.body_zstd.len() + r.headers.len() + r.key.url.len();
                let mut values: Vec<rusqlite::types::Value> =
                    key_params(&r.key).into_iter().map(Into::into).collect();
                values.extend([
                    i64::from(r.status).into(),
                    r.headers.clone().into(),
                    r.body_zstd.to_vec().into(),
                    r.etag.clone().into(),
                    r.last_modified.clone().into(),
                    to_ms(r.fetched_at).into(),
                    (r.ttl_base.as_millis().min(i64::MAX as u128) as i64).into(),
                    r.ttl_mult.into(),
                    to_ms(r.last_used).into(),
                    (size as i64).into(),
                ]);
                tx.execute(
                    "INSERT INTO l2_entries (account_id, api_base, method, url, accept, gql_hash,
                        gql_vars, status, headers, body, etag, last_modified, fetched_ms,
                        ttl_base_ms, ttl_mult, last_used_ms, size)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)
                     ON CONFLICT (account_id, api_base, method, url, accept, gql_hash, gql_vars)
                     DO UPDATE SET status=?8, headers=?9, body=?10, etag=?11, last_modified=?12,
                        fetched_ms=?13, ttl_base_ms=?14, ttl_mult=?15, last_used_ms=?16, size=?17",
                    params_from_iter(values),
                )?;
            }
            tx.commit()
        });
        async move {
            fut.await;
        }
        .boxed()
    }

    fn delete_batch(&self, keys: Vec<CacheKey>) -> BoxFuture<'static, ()> {
        let fut = self.run("delete_batch", move |conn| {
            let tx = conn.transaction()?;
            for k in &keys {
                tx.execute(
                    &format!("DELETE FROM l2_entries WHERE {KEY_COLUMNS}"),
                    params_from_iter(key_params(k)),
                )?;
            }
            tx.commit()
        });
        async move {
            fut.await;
        }
        .boxed()
    }

    fn purge_account(&self, account_id: &str) -> BoxFuture<'static, ()> {
        let id = account_id.to_owned();
        let fut = self.run("purge_account", move |conn| {
            conn.execute("DELETE FROM l2_entries WHERE account_id = ?1", params![id])?;
            conn.execute_batch("PRAGMA incremental_vacuum")
        });
        async move {
            fut.await;
        }
        .boxed()
    }

    fn evict_to(
        &self,
        cap_bytes: u64,
        max_idle: Duration,
        now: SystemTime,
    ) -> BoxFuture<'static, ()> {
        let cutoff = to_ms(now) - max_idle.as_millis().min(i64::MAX as u128) as i64;
        let fut = self.run("evict_to", move |conn| {
            let tx = conn.transaction()?;
            let mut removed = tx.execute(
                "DELETE FROM l2_entries WHERE last_used_ms < ?1",
                params![cutoff],
            )?;
            let mut total: i64 =
                tx.query_row("SELECT COALESCE(SUM(size), 0) FROM l2_entries", [], |r| {
                    r.get(0)
                })?;
            let cap = i64::try_from(cap_bytes).unwrap_or(i64::MAX);
            if total > cap {
                let victims: Vec<(i64, i64)> = {
                    let mut stmt =
                        tx.prepare("SELECT id, size FROM l2_entries ORDER BY last_used_ms, id")?;
                    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                        .collect::<rusqlite::Result<_>>()?
                };
                for (id, size) in victims {
                    if total <= cap {
                        break;
                    }
                    removed += tx.execute("DELETE FROM l2_entries WHERE id = ?1", params![id])?;
                    total -= size;
                }
            }
            tx.commit()?;
            if removed > 0 {
                conn.execute_batch("PRAGMA incremental_vacuum")?;
            }
            Ok(())
        });
        async move {
            fut.await;
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests;
