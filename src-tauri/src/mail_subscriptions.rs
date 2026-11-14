//! Durable, independent readers of the common peer-mail insertion stream.
//! Legacy inbox reads and terminal delivery never acknowledge this journal.
use std::path::Path;

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::state::AgentMessage;

pub(crate) const RETAINED_MESSAGES: i64 = 1000;
const MAX_SUBSCRIPTIONS: i64 = 32;
const MATCH_FILTER: &str = "(?3='all' OR (?3='actionable' AND kind!='routine') OR kind=?3 OR (?3 IN ('exit','failure') AND kind='exit_failure'))";

#[derive(Default)]
pub(crate) struct MailSubscriptions {
    journal: Mutex<Option<Connection>>,
    // A failed append must not be disguised as an empty, successful read.
    append_error: Mutex<Option<String>>,
}

fn open(path: &Path) -> Result<Connection, String> {
    let connection =
        Connection::open(path.join("mail-subscriptions.db")).map_err(|e| e.to_string())?;
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    connection.execute_batch("
        PRAGMA journal_mode=WAL;
        PRAGMA synchronous=FULL;
        CREATE TABLE IF NOT EXISTS mail_heads(recipient TEXT PRIMARY KEY, head INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS mail_events(
            recipient TEXT NOT NULL, seq INTEGER NOT NULL, id TEXT NOT NULL,
            kind TEXT NOT NULL, body TEXT NOT NULL,
            PRIMARY KEY(recipient,seq), UNIQUE(recipient,id));
        CREATE TABLE IF NOT EXISTS mail_subscriptions(
            recipient TEXT NOT NULL, id TEXT NOT NULL, filter TEXT NOT NULL,
            ack INTEGER NOT NULL DEFAULT 0, offered INTEGER NOT NULL DEFAULT 0, batch_end INTEGER NOT NULL DEFAULT 0, missed INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY(recipient,id));
    ").map_err(|e| e.to_string())?;
    Ok(connection)
}

fn event_kind(message: &AgentMessage) -> &'static str {
    if !message
        .id
        .starts_with(crate::state::LIFECYCLE_MSG_ID_PREFIX)
        || message.from_name != "tuic"
    {
        return "mail";
    }
    let Ok(payload) = serde_json::from_str::<Value>(&message.content) else {
        return "mail";
    };
    match payload["type"].as_str() {
        Some("state_change") => match payload["state"].as_str() {
            Some("exited") if payload["exit_code"].as_i64().is_some_and(|code| code != 0) => {
                "exit_failure"
            }
            Some("exited") => "exit",
            Some("failed" | "error" | "stalled") => "failure",
            Some("awaiting_input") => "awaiting",
            _ => "routine",
        },
        Some("exit" | "exited") => "exit",
        Some("child_report") => "report",
        Some(
            "failure"
            | "failed"
            | "initial_prompt_timeout"
            | "prompt_delivery_failed"
            | "delivery_failed",
        ) => "failure",
        // Unknown system notices are actionable rather than silently suppressed.
        _ => "mail",
    }
}

impl MailSubscriptions {
    fn with_connection<T>(
        &self,
        path: &Path,
        f: impl FnOnce(&mut Connection) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut guard = self.journal.lock();
        if guard.is_none() {
            *guard = Some(open(path)?);
        }
        f(guard.as_mut().expect("journal opened"))
    }

    pub(crate) fn append(&self, path: &Path, recipient: &str, message: &AgentMessage) {
        let result = self.with_connection(path, |connection| {
            let tx = connection.transaction().map_err(|e| e.to_string())?;
            let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM mail_events WHERE recipient=?1 AND id=?2)", params![recipient, message.id], |r| r.get(0)).map_err(|e| e.to_string())?;
            if !exists {
                tx.execute("INSERT INTO mail_heads VALUES(?1,1) ON CONFLICT(recipient) DO UPDATE SET head=head+1", [recipient]).map_err(|e| e.to_string())?;
                let head: i64 = tx.query_row("SELECT head FROM mail_heads WHERE recipient=?1", [recipient], |r| r.get(0)).map_err(|e| e.to_string())?;
                let body = serde_json::to_string(message).map_err(|e| e.to_string())?;
                tx.execute("INSERT INTO mail_events VALUES(?1,?2,?3,?4,?5)", params![recipient, head, message.id, event_kind(message), body]).map_err(|e| e.to_string())?;
                let eviction_filter = MATCH_FILTER.replace("?3", "mail_subscriptions.filter");
                tx.execute(&format!("UPDATE mail_subscriptions SET missed=missed+(SELECT COUNT(*) FROM mail_events WHERE recipient=?1 AND seq<=?2 AND seq>mail_subscriptions.ack AND {eviction_filter}) WHERE recipient=?1"), params![recipient,head.saturating_sub(RETAINED_MESSAGES).max(0)]).map_err(|e| e.to_string())?;
                tx.execute("DELETE FROM mail_events WHERE recipient=?1 AND seq<=?2", params![recipient, head.saturating_sub(RETAINED_MESSAGES).max(0)]).map_err(|e| e.to_string())?;
            }
            tx.commit().map_err(|e| e.to_string())
        });
        if let Err(error) = result {
            tracing::error!(source="agent_msg", %error, "Durable mail journal append failed");
            *self.append_error.lock() = Some(error);
        }
    }

    /// One validation and transaction boundary for every transport.
    #[cfg(test)]
    pub(crate) fn action(&self, path: &Path, recipient: &str, args: &Value) -> Value {
        self.action_with_observation(path, recipient, args).0
    }

    pub(crate) fn action_with_observation(
        &self,
        path: &Path,
        recipient: &str,
        args: &Value,
    ) -> (Value, Vec<String>) {
        if let Some(error) = paging_error(args) {
            return (error, Vec::new());
        }
        let mut observed = Vec::new();
        match self.action_result(path, recipient, args, &mut observed) {
            Ok(reply) => (reply, observed),
            Err(error) => (json!({"error":error}), Vec::new()),
        }
    }

    fn action_result(
        &self,
        path: &Path,
        recipient: &str,
        args: &Value,
        observed: &mut Vec<String>,
    ) -> Result<Value, String> {
        let id = args["subscription_id"]
            .as_str()
            .filter(|id| {
                !id.is_empty()
                    && id.len() <= 64
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            })
            .ok_or("subscription_id must be 1..64 ASCII letters, digits, underscores or hyphens")?;
        if let Some(error) = self.append_error.lock().as_ref() {
            return Err(format!(
                "Durable mail journal has a failed append; retained mail may be incomplete: {error}"
            ));
        }
        self.with_connection(path, |connection| {
            let tx = connection.transaction().map_err(|e| e.to_string())?;
            let action = args["action"].as_str().unwrap_or("");
            if action == "subscribe" {
                let filter = args.get("filter").map(|v| v.as_str().ok_or("filter must be a string")).transpose()?.unwrap_or("actionable");
                if !["actionable","all","exit","failure","report"].contains(&filter) { return Err("filter must be actionable, all, exit, failure or report".into()); }
                let existing: Option<String> = tx.query_row("SELECT filter FROM mail_subscriptions WHERE recipient=?1 AND id=?2", params![recipient,id], |r| r.get(0)).optional().map_err(|e| e.to_string())?;
                if let Some(existing) = existing {
                    if existing != filter { return Err("Subscription filter is immutable; use a new subscription_id".into()); }
                } else {
                    let count: i64 = tx.query_row("SELECT COUNT(*) FROM mail_subscriptions WHERE recipient=?1", [recipient], |r| r.get(0)).map_err(|e| e.to_string())?;
                    if count >= MAX_SUBSCRIPTIONS { return Err("Maximum 32 subscriptions per peer; unsubscribe an unused reader".into()); }
                    tx.execute("INSERT INTO mail_subscriptions(recipient,id,filter,ack) VALUES(?1,?2,?3,MAX(0,COALESCE((SELECT head FROM mail_heads WHERE recipient=?1),0)-?4))", params![recipient,id,filter,RETAINED_MESSAGES]).map_err(|e| e.to_string())?;
                }
                tx.commit().map_err(|e| e.to_string())?;
                return Ok(json!({"subscription_id":id,"filter":filter,"retained_messages":RETAINED_MESSAGES,"acknowledgement":"explicit"}));
            }
            let (filter, ack, offered, batch_end, missed): (String,i64,i64,i64,i64) = tx.query_row("SELECT filter,ack,offered,batch_end,missed FROM mail_subscriptions WHERE recipient=?1 AND id=?2", params![recipient,id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(|e| e.to_string())?.ok_or("Unknown subscription_id; call subscribe first")?;
            if action == "unsubscribe" {
                tx.execute("DELETE FROM mail_subscriptions WHERE recipient=?1 AND id=?2", params![recipient,id]).map_err(|e| e.to_string())?;
                tx.commit().map_err(|e| e.to_string())?;
                return Ok(json!({"subscription_id":id,"removed":true}));
            }
            if action == "ack" {
                let cursor = args["cursor"].as_i64().filter(|cursor| *cursor >= 0).ok_or("ack requires a nonnegative integer cursor")?;
                if cursor > offered || (cursor > ack && cursor != offered) { return Err("cursor must equal a returned subscription batch cursor".into()); }
                let ids = {
                    let mut statement = tx.prepare(&format!("SELECT id FROM mail_events WHERE recipient=?1 AND seq>?2 AND seq<=?4 AND {MATCH_FILTER} ORDER BY seq")).map_err(|e| e.to_string())?;
                    statement.query_map(params![recipient,ack,filter,cursor], |row| row.get::<_,String>(0))
                        .map_err(|e| e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e| e.to_string())?
                };
                // DEFERRED (2026-10-18) — a delayed ACK below the retention floor
                // leaves conservative loss accounting until a later gap ACK.
                // Exact subtraction needs retained per-eviction filter metadata;
                // the accepted contract permits overcounting in this window.
                tx.execute("UPDATE mail_subscriptions SET ack=MAX(ack,?3), batch_end=CASE WHEN ?3>=batch_end THEN 0 ELSE batch_end END, missed=CASE WHEN ?3>=MAX(0,COALESCE((SELECT head FROM mail_heads WHERE recipient=?1),0)-?4) THEN 0 ELSE missed END WHERE recipient=?1 AND id=?2", params![recipient,id,cursor,RETAINED_MESSAGES]).map_err(|e| e.to_string())?;
                tx.commit().map_err(|e| e.to_string())?;
                *observed = ids;
                return Ok(json!({"subscription_id":id,"cursor":cursor.max(ack),"acknowledged":true}));
            }
            if !["inbox","wait"].contains(&action) { return Err("Unsupported subscription action".into()); }
            if args.get("since").is_some() { return Err("Subscriptions use explicit ack, not the legacy since cursor".into()); }
            let limit = match args.get("limit") {
                None => 100,
                Some(v) => v.as_i64().filter(|n| (1..=100).contains(n)).ok_or("limit must be an integer in 1..100")?,
            };
            // A pending batch keeps its original range even if a retry changes limit.
            let limit = if batch_end > ack { 100 } else { limit };
            let head: i64 = tx.query_row("SELECT head FROM mail_heads WHERE recipient=?1", [recipient], |r| r.get(0)).optional().map_err(|e| e.to_string())?.unwrap_or(0);
            let floor = head.saturating_sub(RETAINED_MESSAGES).max(0);
            // Freeze an unacknowledged batch, including across new arrivals and reconnects.
            let read_head = if batch_end > ack { batch_end.max(floor).min(head) } else { head };
            let mut rows: Vec<(i64,Value)> = {
                let mut statement = tx.prepare(&format!("SELECT seq,body FROM mail_events WHERE recipient=?1 AND seq>?2 AND seq<=?5 AND {MATCH_FILTER} ORDER BY seq LIMIT ?4")).map_err(|e| e.to_string())?;
                let mapped = statement.query_map(params![recipient, ack.max(floor),filter,limit+1,read_head], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?))).map_err(|e| e.to_string())?;
                let mut rows=Vec::new();
                for row in mapped { let (seq,body)=row.map_err(|e| e.to_string())?; rows.push((seq,serde_json::from_str(&body).map_err(|e| e.to_string())?)); }
                rows
            };
            let page_more = rows.len() as i64 > limit;
            if page_more { rows.pop(); }
            let cursor = if page_more { rows.last().map_or(ack.max(floor), |row| row.0) } else { read_head.max(ack) };
            let has_more: bool = tx.query_row(&format!("SELECT EXISTS(SELECT 1 FROM mail_events WHERE recipient=?1 AND seq>?2 AND {MATCH_FILTER})"), params![recipient,cursor,filter], |r| r.get(0)).map_err(|e| e.to_string())?;
            tx.execute("UPDATE mail_subscriptions SET offered=MAX(offered,?3), batch_end=CASE WHEN ?4 THEN ?3 ELSE batch_end END WHERE recipient=?1 AND id=?2", params![recipient,id,cursor,!rows.is_empty() || missed>0]).map_err(|e| e.to_string())?;
            let messages: Vec<_> = rows.into_iter().map(|(_,message)| message).collect();
            tx.commit().map_err(|e| e.to_string())?;
            Ok(json!({"subscription_id":id,"messages":messages,"count":messages.len(),"cursor":cursor,"acknowledged_cursor":ack,"missed_count":missed,"has_more":has_more}))
        })
    }
}

/// Subscription cursors are not tokens for the legacy byte pager.
pub(crate) fn paging_error(args: &Value) -> Option<Value> {
    (args.get("subscription_id").is_some()
        && (args.get("byte_budget").is_some() || args.get("continuation").is_some()))
    .then(|| json!({"error":"subscription_byte_budget_unsupported", "effect":"not_started",
        "alternative":"Use the bounded subscription batch and explicit ack; byte paging is legacy inbox only."}))
}

/// Transport adapters share validation, acknowledgement and notification.
pub(crate) fn action(state: &crate::AppState, peer: &str, args: &Value) -> Value {
    let reply = state.apply_mail_subscription_action(peer, args);
    if reply.get("error").is_none()
        && matches!(args["action"].as_str(), Some("ack" | "unsubscribe"))
    {
        state.notify_mail_subscription_change(peer);
    }
    reply
}

pub(crate) async fn read(
    state: &std::sync::Arc<crate::AppState>,
    peer: &str,
    args: &Value,
) -> Value {
    let state = state.clone();
    let peer = peer.to_owned();
    let args = args.clone();
    tokio::task::spawn_blocking(move || action(&state, &peer, &args))
        .await
        .unwrap_or_else(|error| json!({"error":format!("Mail subscription read failed: {error}")}))
}

pub(crate) fn pending(batch: &Value) -> bool {
    batch["count"].as_u64().unwrap_or(0) > 0 || batch["missed_count"].as_u64().unwrap_or(0) > 0
}

pub(crate) async fn wait(
    state: &std::sync::Arc<crate::AppState>,
    peer: &str,
    args: &Value,
    timeout_ms: u64,
) -> Value {
    // Subscribe before the snapshot: an insert between the read and sleep cannot be lost.
    let mut changes = state.subscribe_agent_inbox(peer);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        let mut batch = read(state, peer, args).await;
        if batch.get("error").is_some() {
            return batch;
        }
        if pending(&batch) {
            batch["met"] = json!(true);
            batch["timed_out"] = json!(false);
            return batch;
        }
        if !matches!(
            tokio::time::timeout_at(deadline, changes.changed()).await,
            Ok(Ok(()))
        ) {
            // Read once more so an insertion at the deadline is not misreported.
            let mut batch = read(state, peer, args).await;
            if batch.get("error").is_some() {
                return batch;
            }
            let met = pending(&batch);
            batch["met"] = json!(met);
            batch["timed_out"] = json!(!met);
            return batch;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mail(id: &str, body: &str) -> AgentMessage {
        AgentMessage {
            id: id.into(),
            from_tuic_session: "child".into(),
            from_name: "child".into(),
            content: body.into(),
            timestamp: 1,
            report: None,
            delivered_via_channel: false,
        }
    }
    fn request(store: &MailSubscriptions, path: &Path, args: Value) -> Value {
        let reply = store.action(path, "parent", &args);
        assert!(reply.get("error").is_none(), "{reply}");
        reply
    }

    // Catches: reconnect forgets acknowledgement, or a new arrival expands an
    // unacknowledged batch and redelivers a previously offered RESULT.
    #[test]
    fn actionable_subscription_restart_preserves_ack_and_frozen_batches() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let store = MailSubscriptions::default();
        request(
            &store,
            dir.path(),
            json!({"action":"subscribe","subscription_id":"pump"}),
        );
        store.append(dir.path(), "parent", &mail("one", "RESULT one"));
        let first = request(
            &store,
            dir.path(),
            json!({"action":"inbox","subscription_id":"pump","limit":1}),
        );
        store.append(dir.path(), "parent", &mail("two", "BLOCKED two"));
        drop(store);
        let resumed = MailSubscriptions::default();
        let retry = request(
            &resumed,
            dir.path(),
            json!({"action":"inbox","subscription_id":"pump"}),
        );
        assert_eq!(retry["messages"], first["messages"]);
        request(
            &resumed,
            dir.path(),
            json!({"action":"ack","subscription_id":"pump","cursor":first["cursor"]}),
        );
        drop(resumed);
        let resumed = MailSubscriptions::default();
        let second = request(
            &resumed,
            dir.path(),
            json!({"action":"inbox","subscription_id":"pump","limit":1}),
        );
        assert_eq!(second["messages"][0]["content"], "BLOCKED two");
        assert_eq!(second["count"], 1);
        request(
            &resumed,
            dir.path(),
            json!({"action":"ack","subscription_id":"pump","cursor":second["cursor"]}),
        );
        let empty = request(
            &resumed,
            dir.path(),
            json!({"action":"inbox","subscription_id":"pump"}),
        );
        assert_eq!(empty["count"], 0);
    }

    // Catches: filtered routine evictions wake actionable clients, or overflow
    // silently loses a matching RESULT and does not report its loss.
    #[test]
    fn actionable_subscription_overflow_counts_only_matching_unacknowledged_mail() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let store = MailSubscriptions::default();
        // This checks filtering at the real retention boundary, not power-loss
        // durability. Keep every append and transaction, but avoid a full disk
        // flush per fixture row (over 2000) on shared coverage runners.
        store
            .with_connection(dir.path(), |connection| {
                connection
                    .pragma_update(None, "synchronous", "NORMAL")
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        for (id, filter) in [("pump", "actionable"), ("all", "all")] {
            request(
                &store,
                dir.path(),
                json!({"action":"subscribe","subscription_id":id,"filter":filter}),
            );
        }
        for index in 0..=RETAINED_MESSAGES {
            let mut m = mail(
                &format!("tuic-auto-{index}"),
                r#"{"type":"state_change","state":"idle"}"#,
            );
            m.from_name = "tuic".into();
            store.append(dir.path(), "parent", &m);
        }
        let quiet = request(
            &store,
            dir.path(),
            json!({"action":"inbox","subscription_id":"pump"}),
        );
        assert!(
            !pending(&quiet),
            "routine evictions are not actionable: {quiet}"
        );
        let all = request(
            &store,
            dir.path(),
            json!({"action":"inbox","subscription_id":"all","limit":2}),
        );
        assert_eq!(all["count"], 2);
        assert_eq!(all["has_more"], true);
        assert_eq!(all["missed_count"], 1);
        store.append(dir.path(), "parent", &mail("result", "RESULT retained"));
        for index in 0..RETAINED_MESSAGES {
            store.append(
                dir.path(),
                "parent",
                &mail(&format!("later-{index}"), "later"),
            );
        }
        let loss = request(
            &store,
            dir.path(),
            json!({"action":"inbox","subscription_id":"pump","limit":1}),
        );
        assert_eq!(loss["missed_count"], 1);
        assert_eq!(loss["messages"][0]["id"], "later-0");
    }

    // Catches: a malformed ack skips unseen mail, a peer can change a filter
    // after consuming part of a stream, or failed exits disappear from filters.
    #[test]
    fn actionable_subscription_validates_ack_scope_and_failure_filters() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let store = MailSubscriptions::default();
        for filter in ["exit", "failure"] {
            request(
                &store,
                dir.path(),
                json!({"action":"subscribe","subscription_id":filter,"filter":filter}),
            );
        }
        let mut failed = mail(
            "tuic-auto-exit",
            r#"{"type":"state_change","state":"exited","exit_code":1}"#,
        );
        failed.from_name = "tuic".into();
        store.append(dir.path(), "parent", &failed);
        for filter in ["exit", "failure"] {
            let batch = request(
                &store,
                dir.path(),
                json!({"action":"inbox","subscription_id":filter}),
            );
            assert_eq!(batch["messages"][0]["id"], failed.id);
        }
        for bad in [
            json!({"action":"ack","subscription_id":"exit","cursor":999}),
            json!({"action":"inbox","subscription_id":"exit","limit":0}),
            json!({"action":"inbox","subscription_id":"exit","since":0}),
            json!({"action":"subscribe","subscription_id":"exit","filter":"all"}),
        ] {
            assert!(
                store
                    .action(dir.path(), "parent", &bad)
                    .get("error")
                    .is_some()
            );
        }
        let foreign = store.action(
            dir.path(),
            "other-peer",
            &json!({"action":"inbox","subscription_id":"exit"}),
        );
        assert!(foreign.get("error").is_some());
        assert_eq!(
            request(
                &store,
                dir.path(),
                json!({"action":"inbox","subscription_id":"exit"})
            )["count"],
            1
        );
    }
}
