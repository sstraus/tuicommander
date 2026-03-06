use anyhow::Result;
use tokio_rusqlite::Connection;
use tracing::info;

/// Initialize the SQLite database and create tables if needed.
pub async fn init(path: &str) -> Result<Connection> {
    let conn = Connection::open(path).await?;

    conn.call(|conn| {
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA busy_timeout = 5000;

            CREATE TABLE IF NOT EXISTS tokens (
                token_hash  TEXT PRIMARY KEY,
                created_at  INTEGER NOT NULL,
                last_seen   INTEGER NOT NULL,
                total_sessions INTEGER DEFAULT 0,
                total_bytes    INTEGER DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS sessions (
                id            TEXT PRIMARY KEY,
                token_hash    TEXT NOT NULL,
                started_at    INTEGER NOT NULL,
                ended_at      INTEGER,
                bytes_relayed INTEGER DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS idx_sessions_token
                ON sessions(token_hash);

            CREATE TABLE IF NOT EXISTS push_subscriptions (
                token_hash  TEXT NOT NULL,
                endpoint    TEXT NOT NULL,
                p256dh      TEXT NOT NULL,
                auth        TEXT NOT NULL,
                created_at  INTEGER NOT NULL,
                PRIMARY KEY (token_hash, endpoint)
            );
            ",
        )?;
        Ok(())
    })
    .await?;

    info!(path, "database initialized");
    Ok(conn)
}

/// Record a new token hash in the database.
pub async fn insert_token(conn: &Connection, token_hash: &str) -> Result<()> {
    let hash = token_hash.to_owned();
    let now = now_epoch();
    conn.call(move |c| {
        c.execute(
            "INSERT INTO tokens (token_hash, created_at, last_seen) VALUES (?1, ?2, ?3)",
            rusqlite::params![hash, now, now],
        )?;
        Ok(())
    })
    .await?;
    Ok(())
}

/// List all stored token hashes (for DB-fallback auth verification on cache miss).
pub async fn list_token_hashes(conn: &Connection) -> Result<Vec<String>> {
    let hashes = conn
        .call(|c| {
            let mut stmt = c.prepare("SELECT token_hash FROM tokens")?;
            let rows = stmt
                .query_map([], |row| row.get(0))?
                .collect::<std::result::Result<Vec<String>, _>>()?;
            Ok(rows)
        })
        .await?;
    Ok(hashes)
}

/// Check if a token hash exists and update last_seen.
pub async fn validate_token(conn: &Connection, token_hash: &str) -> Result<bool> {
    let hash = token_hash.to_owned();
    let now = now_epoch();
    let exists = conn
        .call(move |c| {
            let updated = c.execute(
                "UPDATE tokens SET last_seen = ?1 WHERE token_hash = ?2",
                rusqlite::params![now, hash],
            )?;
            Ok(updated > 0)
        })
        .await?;
    Ok(exists)
}

/// Record a session start.
pub async fn start_session(conn: &Connection, session_id: &str, token_hash: &str) -> Result<()> {
    let sid = session_id.to_owned();
    let hash = token_hash.to_owned();
    let now = now_epoch();
    conn.call(move |c| {
        c.execute(
            "INSERT OR IGNORE INTO sessions (id, token_hash, started_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![sid, hash, now],
        )?;
        c.execute(
            "UPDATE tokens SET total_sessions = total_sessions + 1 WHERE token_hash = ?1",
            rusqlite::params![hash],
        )?;
        Ok(())
    })
    .await?;
    Ok(())
}

/// Record a session end and bytes relayed.
pub async fn end_session(conn: &Connection, session_id: &str, bytes: u64) -> Result<()> {
    let sid = session_id.to_owned();
    let now = now_epoch();
    conn.call(move |c| {
        c.execute(
            "UPDATE sessions SET ended_at = ?1, bytes_relayed = ?2 WHERE id = ?3",
            rusqlite::params![now, bytes as i64, sid],
        )?;
        Ok(())
    })
    .await?;
    Ok(())
}

/// Get per-token stats.
pub async fn token_stats(conn: &Connection, token_hash: &str) -> Result<Option<TokenStats>> {
    let hash = token_hash.to_owned();
    let stats = conn
        .call(move |c| {
            let mut stmt = c.prepare(
                "SELECT total_sessions, total_bytes, created_at, last_seen
                 FROM tokens WHERE token_hash = ?1",
            )?;
            let row = stmt.query_row(rusqlite::params![hash], |row| {
                Ok(TokenStats {
                    total_sessions: row.get(0)?,
                    total_bytes: row.get(1)?,
                    created_at: row.get(2)?,
                    last_seen: row.get(3)?,
                })
            });
            match row {
                Ok(s) => Ok(Some(s)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(tokio_rusqlite::Error::Rusqlite(e)),
            }
        })
        .await?;
    Ok(stats)
}

/// Per-token stats returned by the /stats endpoint.
#[derive(Debug, serde::Serialize)]
pub struct TokenStats {
    pub total_sessions: i64,
    pub total_bytes: i64,
    pub created_at: i64,
    pub last_seen: i64,
}

// --- Push subscription CRUD ---

/// Store or update a push subscription for a token.
pub async fn insert_push_sub(
    conn: &Connection,
    token_hash: &str,
    endpoint: &str,
    p256dh: &str,
    auth: &str,
) -> Result<()> {
    let hash = token_hash.to_owned();
    let ep = endpoint.to_owned();
    let key = p256dh.to_owned();
    let a = auth.to_owned();
    let now = now_epoch();
    conn.call(move |c| {
        c.execute(
            "INSERT INTO push_subscriptions (token_hash, endpoint, p256dh, auth, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (token_hash, endpoint) DO UPDATE SET p256dh = ?3, auth = ?4",
            rusqlite::params![hash, ep, key, a, now],
        )?;
        Ok(())
    })
    .await?;
    Ok(())
}

/// Remove a push subscription. Returns true if a row was deleted.
pub async fn delete_push_sub(
    conn: &Connection,
    token_hash: &str,
    endpoint: &str,
) -> Result<bool> {
    let hash = token_hash.to_owned();
    let ep = endpoint.to_owned();
    let deleted = conn
        .call(move |c| {
            let count = c.execute(
                "DELETE FROM push_subscriptions WHERE token_hash = ?1 AND endpoint = ?2",
                rusqlite::params![hash, ep],
            )?;
            Ok(count > 0)
        })
        .await?;
    Ok(deleted)
}

/// List all push subscriptions for a token.
pub async fn list_push_subs(
    conn: &Connection,
    token_hash: &str,
) -> Result<Vec<crate::types::PushSubscription>> {
    let hash = token_hash.to_owned();
    let subs = conn
        .call(move |c| {
            let mut stmt = c.prepare(
                "SELECT endpoint, p256dh, auth FROM push_subscriptions WHERE token_hash = ?1",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![hash], |row| {
                    Ok(crate::types::PushSubscription {
                        endpoint: row.get(0)?,
                        p256dh: row.get(1)?,
                        auth: row.get(2)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?;
    Ok(subs)
}

/// Add relayed bytes to a token's total.
pub async fn add_bytes(conn: &Connection, token_hash: &str, bytes: u64) -> Result<()> {
    let hash = token_hash.to_owned();
    conn.call(move |c| {
        c.execute(
            "UPDATE tokens SET total_bytes = total_bytes + ?1 WHERE token_hash = ?2",
            rusqlite::params![bytes as i64, hash],
        )?;
        Ok(())
    })
    .await?;
    Ok(())
}

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_secs() as i64
}
