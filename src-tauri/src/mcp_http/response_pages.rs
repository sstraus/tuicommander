//! Immutable, owner-bound byte pages shared by native MCP dispatch entry points.
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const TTL: Duration = Duration::from_secs(300);
const CACHE_BYTES: usize = 32 * 1024 * 1024;
const CACHE_ENTRIES: usize = 128;

struct Snapshot {
    owner: String,
    operation: String,
    text: String,
    created: Instant,
    offsets: HashSet<usize>,
    acknowledgement: Option<Acknowledgement>,
}

#[derive(Clone)]
pub(super) struct Acknowledgement {
    pub cursor: u64,
    pub message_ids: Vec<String>,
    pub eviction_watermark: u64,
}

#[derive(Default)]
pub(crate) struct ResponsePages {
    snapshots: HashMap<String, Snapshot>,
}

pub(super) fn budget(args: &Value) -> Result<Option<usize>, Value> {
    if args
        .get("continuation")
        .is_some_and(|value| !value.is_string())
    {
        return Err(json!({"error":"invalid_continuation"}));
    }
    match args.get("byte_budget") {
        None if args.get("continuation").is_none() => Ok(None),
        None => Ok(Some(4096)),
        Some(value) => value
            .as_u64()
            .filter(|n| (1024..=65536).contains(n))
            .map(|n| Some(n as usize))
            .ok_or_else(|| json!({"error":"byte_budget must be an integer in 1024..65536"})),
    }
}

impl ResponsePages {
    /// The bound includes this compact JSON envelope, not JSON-RPC/content framing.
    /// Return a cursor only once a complete snapshot has been delivered.
    pub(super) fn page(
        &mut self,
        owner: &str,
        operation: &str,
        args: &Value,
        budget: usize,
        capture: impl FnOnce() -> Value,
    ) -> (Value, Option<Acknowledgement>) {
        self.page_with_ack(owner, operation, args, budget, || {
            let value = capture();
            let acknowledgement = value["next_since"].as_u64().map(|cursor| Acknowledgement {
                cursor,
                message_ids: value["messages"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|message| message["id"].as_str().map(str::to_owned))
                    .collect(),
                eviction_watermark: 0,
            });
            (value, acknowledgement)
        })
    }

    pub(super) fn page_with_ack(
        &mut self,
        owner: &str,
        operation: &str,
        args: &Value,
        budget: usize,
        capture: impl FnOnce() -> (Value, Option<Acknowledgement>),
    ) -> (Value, Option<Acknowledgement>) {
        self.snapshots.retain(|_, s| s.created.elapsed() < TTL);
        let (id, offset) = if let Some(token) = args.get("continuation") {
            let parsed = token
                .as_str()
                .and_then(|s| s.split_once(':'))
                .and_then(|(id, offset)| offset.parse::<usize>().ok().map(|n| (id.to_string(), n)));
            let Some((id, offset)) = parsed else {
                return (json!({"error":"invalid_continuation"}), None);
            };
            let valid = self.snapshots.get(&id).is_some_and(|s| {
                s.owner == owner && s.operation == operation && s.offsets.contains(&offset)
            });
            if !valid {
                return (
                    json!({"error":"invalid_or_expired_continuation", "restart_required":true}),
                    None,
                );
            }
            (id, offset)
        } else {
            let (value, acknowledgement) = capture();
            // Errors are errors, never a successful empty snapshot.
            if value.get("error").is_some() {
                if serde_json::to_vec(&value)
                    .expect("JSON value serializes")
                    .len()
                    > budget
                {
                    return (
                        json!({"error":"response_error_exceeds_byte_budget", "omitted_fields":["error_detail"]}),
                        None,
                    );
                }
                return (value, None);
            }
            let text = serde_json::to_string(&value).expect("JSON value serializes");
            if text.len() > CACHE_BYTES {
                return (
                    json!({"error":"response_snapshot_too_large", "max_bytes":CACHE_BYTES}),
                    None,
                );
            }
            while self.snapshots.len() >= CACHE_ENTRIES
                || self.snapshots.values().map(|s| s.text.len()).sum::<usize>() + text.len()
                    > CACHE_BYTES
            {
                let oldest = self
                    .snapshots
                    .iter()
                    .min_by_key(|(_, s)| s.created)
                    .map(|(id, _)| id.clone());
                if let Some(id) = oldest {
                    self.snapshots.remove(&id);
                } else {
                    break;
                }
            }
            let id = uuid::Uuid::new_v4().to_string();
            self.snapshots.insert(
                id.clone(),
                Snapshot {
                    owner: owner.into(),
                    operation: operation.into(),
                    text,
                    created: Instant::now(),
                    offsets: HashSet::from([0]),
                    acknowledgement,
                },
            );
            (id, 0)
        };
        let snapshot = self.snapshots.get_mut(&id).expect("validated snapshot");
        let remaining = &snapshot.text[offset..];
        let boundaries: Vec<usize> = remaining
            .char_indices()
            .map(|(i, _)| i)
            .take(budget + 1)
            .chain(std::iter::once(remaining.len().min(budget)))
            .filter(|i| remaining.is_char_boundary(*i))
            .collect();
        let render = |end: usize| {
            let end = offset + end;
            json!({"encoding":"json-text", "chunk":&snapshot.text[offset..end],
                "offset":offset, "total_bytes":snapshot.text.len(),
                "continuation": if end < snapshot.text.len() { Some(format!("{id}:{end}")) } else { None },
                "omitted_fields":[], "expires_after_seconds":300})
        };
        // The boundary list must be sorted: the whole suffix can exceed budget.
        let mut boundaries = boundaries;
        boundaries.sort_unstable();
        boundaries.dedup();
        let mut low = 0;
        let mut high = boundaries.len();
        while low + 1 < high {
            let mid = low.midpoint(high);
            if serde_json::to_vec(&render(boundaries[mid]))
                .expect("JSON value serializes")
                .len()
                <= budget
            {
                low = mid;
            } else {
                high = mid;
            }
        }
        let end = offset + boundaries[low];
        let response = render(boundaries[low]);
        if end == offset {
            return (json!({"error":"byte_budget_too_small"}), None);
        }
        snapshot.offsets.insert(end);
        // Replaying a final page must not acknowledge later eviction counts twice.
        let acknowledgement = if end == snapshot.text.len() {
            snapshot.acknowledgement.take()
        } else {
            None
        };
        (response, acknowledgement)
    }
}

/// Compact inventories explicitly distinguish unavailable state from idle.
pub(super) fn compact(row: &Value) -> Value {
    let keys = [
        "session_id",
        "tuic_session",
        "name",
        "display_name",
        "alias",
        "address",
        "connection_id",
        "is_caller",
        "state",
        "shell_state",
        "agent_state",
        "activity_age_ms",
        "stage",
        "stage_reason",
        "report",
        "report_at",
        "report_message_id",
    ];
    let mut result = serde_json::Map::new();
    for key in keys {
        if let Some(value) = row.get(key) {
            result.insert(key.into(), value.clone());
        }
    }
    if !result.get("state").is_some_and(Value::is_string) {
        result.remove("state");
    }
    result.entry("state").or_insert_with(|| {
        row.get("agent_state")
            .or_else(|| row.get("shell_state"))
            .cloned()
            .unwrap_or_else(|| json!("unknown"))
    });
    result.entry("activity_age_ms").or_insert_with(|| {
        row["last_activity_ms"]
            .as_u64()
            .filter(|timestamp| *timestamp > 0)
            .map(|timestamp| {
                json!(super::mcp_transport_peer::now_unix_ms().saturating_sub(timestamp))
            })
            .unwrap_or(Value::Null)
    });
    result.insert(
        "omitted_fields".into(),
        json!(
            row.as_object()
                .into_iter()
                .flat_map(|o| o.keys())
                .filter(|key| !keys.contains(&key.as_str()))
                .collect::<Vec<_>>()
        ),
    );
    Value::Object(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: an audited oversized response or UTF-8/escaped text bypasses the envelope bound.
    #[test]
    fn captured_mcp_response_reconstructs_inside_byte_budget() {
        let captured = include_str!("../fixtures/mcp-output-2005.txt").trim_end();
        assert_eq!(captured.len(), 49162);
        let original: Value = serde_json::from_str(captured).unwrap();
        let inbox = include_str!("../fixtures/mcp-inbox-2005.txt").trim_end();
        assert_eq!(inbox.len(), 49609);
        let inbox: Value = serde_json::from_str(inbox).unwrap();
        for value in [
            original,
            inbox,
            json!({"messages":[{"content":"界\"\n".repeat(9000)}],"next_since":1000}),
        ] {
            let expected = serde_json::to_string(&value).unwrap();
            let mut pages = ResponsePages::default();
            let mut args = json!({"byte_budget":4096});
            let mut reconstructed = String::new();
            let mut max_bytes = 0;
            let mut calls = 0;
            loop {
                let (page, _) = pages.page("owner", "output", &args, 4096, || value.clone());
                let bytes = serde_json::to_vec(&page).unwrap().len();
                assert!(bytes <= 4096, "{bytes}");
                max_bytes = max_bytes.max(bytes);
                calls += 1;
                reconstructed.push_str(page["chunk"].as_str().unwrap());
                if page["continuation"].is_null() {
                    break;
                }
                args["continuation"] = page["continuation"].clone();
            }
            assert_eq!(reconstructed, expected);
            eprintln!(
                "byte-budget measurement: before={} after_max={} calls={} reconstructed={}",
                expected.len(),
                max_bytes,
                calls,
                reconstructed.len()
            );
        }
    }

    // Catches: malformed, foreign, expired or fabricated-offset continuations read another snapshot.
    #[test]
    fn response_continuations_fail_closed_and_expire() {
        let mut pages = ResponsePages::default();
        let (first, _) = pages.page(
            "owner",
            "output:a",
            &json!({}),
            1024,
            || json!({"data":"界".repeat(3000)}),
        );
        let token = first["continuation"].as_str().unwrap();
        for (owner, operation, token) in [
            ("other", "output:a", token.to_owned()),
            ("owner", "output:b", token.to_owned()),
            ("owner", "output:a", "invalid".into()),
            (
                "owner",
                "output:a",
                format!("{}:1", token.split_once(':').unwrap().0),
            ),
        ] {
            let (error, _) = pages.page(
                owner,
                operation,
                &json!({"continuation":token}),
                1024,
                || panic!("must not recapture"),
            );
            assert!(error.get("error").is_some(), "{error}");
        }
        for snapshot in pages.snapshots.values_mut() {
            snapshot.created = Instant::now() - TTL;
        }
        let (expired, _) = pages.page(
            "owner",
            "output:a",
            &json!({"continuation":token}),
            1024,
            || panic!("must not recapture"),
        );
        assert_eq!(expired["error"], "invalid_or_expired_continuation");
        assert_eq!(expired["restart_required"], true);
    }

    // Catches: replaying a completed snapshot acknowledges newly lost mail twice.
    #[test]
    fn empty_snapshot_acknowledges_once_and_bad_budgets_do_not_capture() {
        for value in [
            json!(0),
            json!(1023),
            json!(65537),
            json!(1.5),
            json!("4096"),
            Value::Null,
        ] {
            assert!(budget(&json!({"byte_budget":value})).is_err());
        }
        let mut pages = ResponsePages::default();
        let (page, ack) = pages.page(
            "owner",
            "inbox",
            &json!({}),
            1024,
            || json!({"messages":[],"next_since":42,"missed_count":2}),
        );
        assert!(page["continuation"].is_null());
        assert_eq!(ack.unwrap().cursor, 42);
        let id = pages.snapshots.keys().next().unwrap().clone();
        let (replayed, ack) = pages.page(
            "owner",
            "inbox",
            &json!({"continuation":format!("{id}:0")}),
            1024,
            || panic!("immutable snapshot"),
        );
        assert_eq!(replayed, page);
        assert!(ack.is_none());
    }
    // Catches: a compact projection discards the backend activity timestamp instead of deriving age.
    #[test]
    fn compact_inventory_derives_activity_from_last_activity_timestamp() {
        let row = json!({"session_id":"live", "last_activity_ms":1, "shell_state":"busy"});
        let projected = compact(&row);
        assert!(
            projected["activity_age_ms"]
                .as_u64()
                .is_some_and(|age| age > 0),
            "{projected}"
        );
        assert_eq!(projected["state"], "busy");
    }
}
