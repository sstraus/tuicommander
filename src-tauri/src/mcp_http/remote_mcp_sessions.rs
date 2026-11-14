//! MCP session ownership uses the same Rust mirror and configured credential as
//! the HTTP session proxy. Read-only MCP preserves the daemon's output contract;
//! submit uses the existing authenticated HTTP command, never raw PTY input.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};

use crate::AppState;

pub(super) fn resolve(state: &AppState, args: &Value) -> Option<Result<(String, String), Value>> {
    let address = args["session_id"].as_str()?;
    if address.trim().is_empty() {
        return Some(Err(json!({"error":"session_id must not be empty"})));
    }
    if let Some((host, id)) = address.split_once('/') {
        if id.trim().is_empty() || id.contains('/') {
            return Some(Err(
                json!({"error":"Invalid connection-qualified session_id"}),
            ));
        }
        if host == "local" {
            if args["connection_id"]
                .as_str()
                .is_some_and(|explicit| explicit != "local")
            {
                return Some(Err(
                    json!({"error":"connection_id conflicts with qualified session_id"}),
                ));
            }
            return None;
        }
        if let Some(explicit) = args["connection_id"].as_str()
            && explicit != host
        {
            return Some(Err(
                json!({"error":"connection_id conflicts with qualified session_id"}),
            ));
        }
        if id.is_empty() || id.contains('/') {
            return Some(Err(
                json!({"error":"Invalid connection-qualified session_id"}),
            ));
        }
        return Some(Ok((host.to_string(), id.to_string())));
    }
    if let Some(host) = args["connection_id"].as_str() {
        return (host != "local").then(|| Ok((host.to_string(), address.to_string())));
    }
    match state.resolve_session_ref_checked(address) {
        Ok(Some(_)) => return None,
        Err(detail) => return Some(Err(json!({"error":detail}))),
        Ok(None) => {}
    }
    let rows: Vec<_> = crate::remote_mirror::mirrored_rows(state)
        .into_iter()
        .filter(|row| {
            row.session_id == address
                || row.alias.as_deref() == Some(address)
                || row.tuic_session.as_deref() == Some(address)
                || row.display_name.as_deref() == Some(address)
                || row.session_id.starts_with(address)
        })
        .collect();
    if rows.len() > 1 {
        return Some(Err(
            json!({"error":format!("Remote session address '{address}' is ambiguous; use connection_id or connection/id")}),
        ));
    }
    rows.into_iter()
        .next()
        .and_then(|row| row.connection_id.map(|host| Ok((host, row.session_id))))
}

#[cfg(test)]
pub(super) async fn call(state: &Arc<AppState>, host: &str, id: &str, args: &Value) -> Value {
    call_with_caller(state, host, id, args, None).await
}

pub(super) async fn call_with_caller(
    state: &Arc<AppState>,
    host: &str,
    id: &str,
    args: &Value,
    own_pty: Option<&str>,
) -> Value {
    let requested_budget = if args["action"] == "output" {
        match super::response_pages::budget(args) {
            Ok(budget) => budget,
            Err(error) => return error,
        }
    } else {
        None
    };
    if id.trim().is_empty() {
        return failure(host, "session_id must not be empty");
    }
    let Some(base) = state.remote.base_url(host) else {
        return failure(host, "connection is unavailable");
    };
    let mut url = match reqwest::Url::parse(&base) {
        Ok(url) => url,
        Err(_) => return failure(host, "connection has an invalid URL"),
    };
    // Resolve against the daemon's actual session registry. This accepts aliases
    // without guessing which host a desktop-local alias belongs to.
    let direct_id = uuid::Uuid::parse_str(id).is_ok();
    url.set_path("/sessions");
    credential(state, host, &mut url);
    let rows = if direct_id {
        json!([{"session_id":id}])
    } else {
        match request(host, state.remote.http_client().get(url.clone())).await {
            Ok(rows) => rows,
            Err(value) => return value,
        }
    };
    let Some(rows) = rows.as_array() else {
        return failure(host, "invalid session list");
    };
    let matches: Vec<_> = rows
        .iter()
        .filter(|row| {
            row["session_id"] == id
                || row["alias"] == id
                || row["tuic_session"] == id
                || row["display_name"] == id
                || row["session_id"]
                    .as_str()
                    .is_some_and(|value| value.starts_with(id))
        })
        .collect();
    let canonical = match matches.as_slice() {
        [row] => match row["session_id"].as_str() {
            Some(id) => id,
            None => return failure(host, "invalid session identity"),
        },
        [] => return failure(host, format!("session '{id}' was not found")),
        _ => return failure(host, format!("session '{id}' is ambiguous")),
    };
    let action = args["action"].as_str().unwrap_or("");
    if action == "close" && own_pty == Some(canonical) {
        return failure(
            host,
            "Cannot close own session. Use exit to terminate yourself.",
        );
    }
    if !matches!(action, "output" | "submit" | "close") {
        return failure(
            host,
            format!("MCP session action '{action}' is not supported remotely"),
        );
    }
    url.set_path("/sessions");
    match url.path_segments_mut() {
        Ok(mut path) => {
            path.push(canonical);
            if action != "close" {
                path.push(action);
            }
        }
        Err(_) => return failure(host, "invalid session URL"),
    }
    credential(state, host, &mut url);
    let request_builder = if action == "close" {
        state.remote.http_client().delete(url)
    } else if action == "submit" {
        state
            .remote
            .http_client()
            .post(url)
            .json(&json!({"input":args.get("input"),"timeout_ms":args.get("timeout_ms")}))
    } else {
        {
            let mut query = url.query_pairs_mut();
            query.append_pair(
                "format",
                if args["format"] == "raw" {
                    "mcp_raw"
                } else {
                    "mcp"
                },
            );
            for key in [
                "limit",
                "from_line",
                "from_byte",
                "since_cursor",
                "byte_budget",
            ] {
                if let Some(value) = args[key].as_u64() {
                    query.append_pair(key, &value.to_string());
                }
            }
            if let Some(token) = args["continuation"].as_str() {
                query.append_pair("continuation", token);
            }
            // The remote host still requires its own owner grant.
            if args["literal"] == true {
                query.append_pair("literal", "true");
            }
        }
        state.remote.http_client().get(url)
    };
    match request(host, request_builder).await {
        Ok(mut value) => {
            if let Some(budget) = requested_budget {
                if value["encoding"] != "json-text" {
                    return failure(
                        host,
                        "daemon does not support byte-budget output; update the daemon",
                    );
                }
                if serde_json::to_vec(&value)
                    .expect("JSON response serializes")
                    .len()
                    > budget
                {
                    return failure(host, "daemon response exceeds requested byte_budget");
                }
            }
            if action == "output"
                && value.get("exited").is_none()
                && value["encoding"] != "json-text"
            {
                return failure(
                    host,
                    "daemon does not support MCP output parity; update the daemon",
                );
            }
            if value["encoding"] == "json-text" {
                return value;
            }
            value["connection_id"] = json!(host);
            if let Some(note) = value["continuation"].as_str() {
                value["continuation"] = json!(format!(
                    "{note} For this remote page, also pass connection_id={host:?}."
                ));
            }
            value
        }
        Err(value) => {
            if requested_budget.is_some_and(|budget| {
                serde_json::to_vec(&value)
                    .expect("JSON error serializes")
                    .len()
                    > budget
            }) {
                failure(host, "daemon error exceeds requested byte_budget")
            } else {
                value
            }
        }
    }
}

fn failure(host: &str, detail: impl std::fmt::Display) -> Value {
    json!({"error":format!("Remote connection '{host}': {detail}"),"connection_id":host})
}

fn credential(state: &AppState, host: &str, url: &mut reqwest::Url) {
    url.set_query(None);
    if let Some(token) = state.remote.token(host) {
        url.query_pairs_mut().append_pair("token", &token);
    }
}

async fn request(host: &str, builder: reqwest::RequestBuilder) -> Result<Value, Value> {
    let response = builder
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|_| {
            failure(
                host,
                "session transport failed; submit delivery may be uncertain, do not resend blindly",
            )
        })?;
    let status = response.status();
    let mut value: Value = response
        .json()
        .await
        .map_err(|_| failure(host, format!("invalid session response ({status})")))?;
    if value.get("error").is_some() {
        let detail = value["error"].as_str().unwrap_or("session request failed");
        value["error"] = json!(format!("Remote connection '{host}': {detail}"));
        value["connection_id"] = json!(host);
        return Err(value);
    }
    if !status.is_success() {
        // Native submit rejects safe-to-retry preconditions with structured
        // submitted/reason/detail fields, deliberately without an error string.
        // Preserve that consumer contract instead of flattening it to HTTP 409.
        if value["submitted"] == false && value["reason"].is_string() {
            value["connection_id"] = json!(host);
            return Err(value);
        }
        return Err(failure(host, format!("session request returned {status}")));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::super::tests::test_state;
    use super::*;

    // Catches: close rejected remotely, alias targeting the wrong daemon, loss
    // of tombstone output, or an explicit connection bypassing self-close.
    #[cfg(unix)]
    #[tokio::test]
    async fn remote_mcp_close_uses_owner_tombstone_and_preserves_self_guard() {
        let hub = test_state();
        let remote = test_state();
        let id = uuid::Uuid::new_v4().to_string();
        crate::state::tests_support::insert_dummy_session(&remote, &id);
        remote
            .session_maps
            .term_aliases
            .insert(id.clone(), "remote-1".into());
        let mut log = crate::state::VtLogBuffer::new(24, 80, 100);
        log.process(b"final remote output\r\n");
        remote
            .grid
            .vt_log_buffers
            .insert(id.clone(), parking_lot::Mutex::new(log));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let token = remote.session_token.read().clone();
        hub.remote
            .force_connected_for_test("mint", &url, Some(&token));
        let owner = Arc::clone(&remote);
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                super::super::build_remote_router(owner)
                    .into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap();
        });
        let own = call_with_caller(
            &hub,
            "mint",
            "remote-1",
            &json!({"action":"close"}),
            Some(&id),
        )
        .await;
        let preserved = remote.session_maps.sessions.contains_key(&id);
        let reply = call(&hub, "mint", "remote-1", &json!({"action":"close"})).await;
        let closed = !remote.session_maps.sessions.contains_key(&id);
        let tail = call(&hub, "mint", &id, &json!({"action":"output"})).await;
        let again = call(&hub, "mint", &id, &json!({"action":"close"})).await;
        crate::pty::close_pty_core(&remote, &id, false);
        server.abort();
        assert!(
            own["error"]
                .as_str()
                .is_some_and(|s| s.contains("Cannot close own session")),
            "{own}"
        );
        assert!(preserved);
        assert_eq!(reply, json!({"ok":true,"connection_id":"mint"}));
        assert!(closed, "owning daemon must close the live PTY");
        assert_eq!(tail["exited"], true, "{tail}");
        assert!(
            tail["data"]
                .as_str()
                .unwrap()
                .contains("final remote output"),
            "{tail}"
        );
        assert_eq!(again["ok"], true, "{again}");
    }

    // Catches: remote output silently becomes raw HTTP output, loses tombstone
    // state/cursors, or bypasses the native absolute-window rules.
    #[tokio::test]
    async fn remote_mcp_output_preserves_clean_raw_and_tombstone_contracts() {
        let hub = test_state();
        let remote = test_state();
        let id = uuid::Uuid::new_v4().to_string();
        let mut log = crate::state::VtLogBuffer::new(2, 80, 100);
        log.process(b"remote first\r\nremote second\r\nremote tail\r\n");
        remote
            .grid
            .vt_log_buffers
            .insert(id.clone(), parking_lot::Mutex::new(log));
        let mut ring = crate::OutputRingBuffer::new(4096);
        ring.write(b"\x1b[31mremote tail\x1b[0m\r\n");
        remote
            .session_maps
            .output_buffers
            .insert(id.clone(), parking_lot::Mutex::new(ring));
        remote.session_maps.exit_codes.insert(id.clone(), 42);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let token = remote.session_token.read().clone();
        hub.remote
            .force_connected_for_test("mint", &url, Some(&token));
        let remote_server = Arc::clone(&remote);
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                super::super::build_remote_router(remote_server)
                    .into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap();
        });
        let clean = call(&hub, "mint", &id, &json!({"action":"output","limit":20})).await;
        assert!(
            clean["data"].as_str().unwrap().contains("remote tail"),
            "{clean}"
        );
        assert_eq!(clean["exited"], true);
        assert_eq!(clean["exit_code"], 42);
        assert!(clean["cursor"].is_number());
        assert_eq!(clean["connection_id"], "mint");
        let window = call(
            &hub,
            "mint",
            &id,
            &json!({"action":"output","from_line":0,"limit":1}),
        )
        .await;
        assert_eq!(window["data"], "remote first");
        let raw = call(
            &hub,
            "mint",
            &id,
            &json!({"action":"output","format":"raw"}),
        )
        .await;
        assert_eq!(raw["data"], "\u{1b}[31mremote tail\u{1b}[0m\r\n");
        assert_eq!(raw["exited"], true);
        assert_eq!(raw["exit_code"], 42);
        // Catches: the proxy drops the byte offset or the HTTP query ignores it.
        for args in [
            json!({"action":"output","format":"raw","from_byte":7,"limit":3}),
            json!({"action":"output","format":"raw","since_cursor":7,"limit":3}),
            json!({"action":"output","from_line":0,"limit":1}),
        ] {
            let mut native_args = args.clone();
            native_args["session_id"] = json!(id);
            let mut expected = super::super::mcp_transport::session_output(&remote, &native_args);
            expected["connection_id"] = json!("mint");
            let mut page = call(&hub, "mint", &id, &args).await;
            assert!(
                page["continuation"]
                    .as_str()
                    .unwrap()
                    .contains("connection_id=\"mint\"")
            );
            page.as_object_mut().unwrap().remove("continuation");
            expected.as_object_mut().unwrap().remove("continuation");
            assert_eq!(page, expected);
            assert_eq!(page["has_more"], true);
        }
        server.abort();
    }

    // Catches: HTTP rejection status hides the native semantic submit reason.
    #[tokio::test]
    async fn remote_mcp_submit_rejection_keeps_the_native_reason_and_detail() {
        let hub = test_state();
        let remote = test_state();
        let id = uuid::Uuid::new_v4().to_string();
        let args = json!({"action":"submit", "session_id":id, "input":"must never execute"});
        let expected =
            super::super::mcp_transport::handle_session_submit(&remote, &args, true).await;
        assert_eq!(expected["submitted"], false, "{expected}");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let token = remote.session_token.read().clone();
        hub.remote
            .force_connected_for_test("mint", &url, Some(&token));
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                super::super::build_remote_router(remote)
                    .into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap();
        });
        let result = call(&hub, "mint", &id, &args).await;
        assert_eq!(result["submitted"], false, "{result}");
        assert_eq!(result["reason"], expected["reason"], "{result}");
        assert_eq!(result["detail"], expected["detail"], "{result}");
        assert_eq!(result["connection_id"], "mint");
        server.abort();
    }

    // Catches: direct owner calls bypass resolve's empty-prefix rejection.
    #[tokio::test]
    async fn empty_remote_target_never_reaches_alias_prefix_lookup() {
        let state = test_state();
        for id in ["", " ", "\t"] {
            let result = call(
                &state,
                "mint",
                id,
                &json!({"action":"submit","input":"must never execute"}),
            )
            .await;
            assert!(
                result["error"]
                    .as_str()
                    .unwrap()
                    .contains("must not be empty"),
                "{result}"
            );
        }
        for address in ["", " ", "mint/", "mint/ ", "local/"] {
            let result = resolve(&state, &json!({"action":"submit","session_id":address})).unwrap();
            assert!(result.is_err(), "{address:?} selected a target");
        }
    }

    // Catches: qualified remote failures fall through to the local registry and
    // report not-found rather than naming the unavailable configured connection.
    #[tokio::test]
    async fn remote_mcp_unavailable_connection_is_explicit_and_never_local_fallback() {
        let hub = test_state();
        let result = call(&hub, "mint", "pe-3", &json!({"action":"output"})).await;
        assert_eq!(result["connection_id"], "mint");
        assert!(result["error"].as_str().unwrap().contains("mint"));
        assert!(result["error"].as_str().unwrap().contains("unavailable"));
        let ambiguity = resolve(
            &hub,
            &json!({"session_id":"mint/pe-3","connection_id":"other"}),
        )
        .unwrap()
        .unwrap_err();
        assert!(ambiguity["error"].as_str().unwrap().contains("conflicts"));
    }
    // Catches: an older daemon ignores byte_budget and its full exited-bearing body bypasses the proxy bound.
    #[tokio::test]
    async fn remote_output_rejects_daemon_ignoring_requested_byte_budget() {
        let hub = test_state();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        hub.remote
            .force_connected_for_test("old-daemon", &url, None);
        let server = tokio::spawn(async move {
            // Recorded actual daemon response from the audit, not a fabricated external payload.
            let app = axum::Router::new().route(
                "/sessions/{id}/output",
                axum::routing::get(|| async {
                    axum::Json(
                        serde_json::from_str::<Value>(include_str!(
                            "../fixtures/mcp-output-2005.txt"
                        ))
                        .unwrap(),
                    )
                }),
            );
            axum::serve(listener, app).await.unwrap();
        });
        let id = uuid::Uuid::new_v4().to_string();
        let result = call(
            &hub,
            "old-daemon",
            &id,
            &json!({"action":"output","byte_budget":1024}),
        )
        .await;
        server.abort();
        assert!(
            serde_json::to_vec(&result).unwrap().len() <= 1024,
            "legacy daemon bypassed byte budget"
        );
        assert!(
            result.get("error").is_some(),
            "capability mismatch must fail explicitly: {result}"
        );
    }
}
