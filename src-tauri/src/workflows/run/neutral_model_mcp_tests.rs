//! Owned MCP/SQLite/file boundary tests; no provider continuation claim.
use super::super::super::neutral_model_mcp::ModelMcpHost;
use super::*;
use crate::acp::{AcpMcpOrigin, McpOverAcpHost};
use std::task::{Context, Poll};

fn channel(host: &ModelMcpHost, call: &ModelToolCall) -> (String, AcpMcpOrigin) {
    let origin = AcpMcpOrigin {
        peer_id: Some("owned-peer".into()),
        connection_id: serde_json::from_value(json!(call.session.connection)).unwrap(),
        generation: call.session.generation,
    };
    let id = host
        .connect_scoped(Some("owned-peer"), Arc::new(|_, _| {}), Some(&origin))
        .unwrap();
    (id, origin)
}
fn params(id: &str, name: &str, value: i64) -> serde_json::Map<String, serde_json::Value> {
    json!({"name":name,"arguments":{"value":value},"_meta":{"tuic/call_id":id}})
        .as_object()
        .unwrap()
        .clone()
}

// Catches offering/dispatching ordinary MCP tools and malformed or stolen original-call identities.
#[tokio::test]
async fn model_mcp_denies_unknown_tools_methods_ids_and_stale_origins() {
    let (dir, owner, run, at) = fixture(30, false);
    let (_, call) = parent(&owner, &run, at);
    let writer = FileHost::new(&owner, dir.path().join("forbidden"), Reply::Complete, false);
    owner.neutral_host(writer.clone());
    let host = ModelMcpHost::new(&owner, call.parent.clone()).unwrap();
    let (connection, mut origin) = channel(&host, &call);
    let list = host
        .message(&connection, "tools/list".into(), None)
        .await
        .unwrap();
    assert_eq!(list["tools"].as_array().unwrap().len(), 1);
    assert_eq!(list["tools"][0]["name"], "increment");
    assert_eq!(
        list["tools"][0]["inputSchema"]["additionalProperties"],
        false
    );
    for (method, request) in [
        ("session", Some(params(&"a".repeat(64), "increment", 30))),
        ("tools/call", Some(params(&"a".repeat(64), "shell", 30))),
        (
            "tools/call",
            Some(params("missing-original-id", "increment", 30)),
        ),
        ("tools/call", Some(params(&"A".repeat(64), "increment", 30))),
    ] {
        assert!(
            host.message(&connection, method.into(), request)
                .await
                .is_err()
        );
    }
    assert!(host.connect(None, Arc::new(|_, _| {})).is_err());
    origin.generation += 1;
    assert!(
        host.connect_scoped(Some("owned-peer"), Arc::new(|_, _| {}), Some(&origin))
            .is_err()
    );
    host.disconnect(&connection);
    let stale = host
        .connect_scoped(Some("owned-peer"), Arc::new(|_, _| {}), Some(&origin))
        .unwrap();
    assert!(
        host.message(
            &stale,
            "tools/call".into(),
            Some(params(&"a".repeat(64), "increment", 30))
        )
        .await
        .is_err()
    );
    assert_eq!(writer.calls.load(Ordering::Acquire), 0);
    assert!(!dir.path().join("forbidden").exists());
}

// Catches the mediated model MCP refusing the 2026-07-28 handshake or omitting the list
// envelope: installed ego then refuses the server and every workflow role fails at
// session/new ("the server does not answer 'server/discover'", live run 2026-11-13).
#[tokio::test]
async fn model_mcp_answers_the_discovery_and_list_envelope_that_ego_requires() {
    let (_dir, owner, run, at) = fixture(30, false);
    let (_, call) = parent(&owner, &run, at);
    let host = ModelMcpHost::new(&owner, call.parent.clone()).unwrap();
    let (connection, _) = channel(&host, &call);
    // The request ego sends, recorded from ego by the ordinary MCP channel.
    let recorded: serde_json::Value = serde_json::from_str(include_str!(
        "../../mcp_http/fixtures/ego_server_discover.json"
    ))
    .unwrap();
    let discover = host
        .message(
            &connection,
            recorded["method"].as_str().unwrap().into(),
            recorded["params"].as_object().cloned(),
        )
        .await
        .unwrap();
    assert_eq!(discover["resultType"], "complete");
    assert!(
        discover["supportedVersions"]
            .as_array()
            .unwrap()
            .contains(&json!("2026-07-28"))
    );
    assert!(discover["capabilities"]["tools"].is_object());
    assert!(discover["capabilities"].get("resources").is_none());
    let list = host
        .message(&connection, "tools/list".into(), None)
        .await
        .unwrap();
    for result in [&discover, &list] {
        assert_eq!(result["ttlMs"], 0, "{result}");
        assert_eq!(result["cacheScope"], "private", "{result}");
    }
    assert_eq!(list["resultType"], "complete");
    assert_eq!(list["tools"][0]["name"], "increment");
}

// Catches a transport returning a pending receipt instead of the validated committed child result.
#[tokio::test]
async fn model_mcp_waits_for_committed_result_and_duplicate_calls_write_once() {
    let (dir, owner, run, at) = fixture(30, false);
    let (_, mut call) = parent(&owner, &run, at);
    call.id = "a".repeat(64);
    let writer = FileHost::new(&owner, dir.path().join("mcp-write"), Reply::Pending, false);
    owner.neutral_host(writer.clone());
    let host = ModelMcpHost::new(&owner, call.parent.clone()).unwrap();
    let (connection, _) = channel(&host, &call);
    let request = params(&call.id, "increment", 30);
    let mut reply = host.message(&connection, "tools/call".into(), Some(request.clone()));
    let waker = futures_util::task::noop_waker();
    let mut cx = Context::from_waker(&waker);
    assert!(matches!(reply.as_mut().poll(&mut cx), Poll::Pending));
    assert_eq!(writer.calls.load(Ordering::Acquire), 1);
    let snapshot = owner.store.neutral_execution(&run.run.id).unwrap();
    let child = snapshot.model_tool_call(&call).unwrap().unwrap().clone();
    let snapshot = parent_result(&owner, &snapshot, &call, at);
    assert!(snapshot.attempt.as_ref().unwrap().result.is_none());
    owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Observe {
                key: child.key.clone(),
                observation: FileHost::result(&child),
            },
        )
        .unwrap();
    let result = reply.await.unwrap();
    assert_eq!(result["isError"], false);
    // A 2026-07-28 tool result carries its discriminator.
    assert_eq!(result["resultType"], "complete");
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("31"));
    let duplicate = host
        .message(&connection, "tools/call".into(), Some(request))
        .await
        .unwrap();
    assert_eq!(duplicate, result);
    assert_eq!(writer.calls.load(Ordering::Acquire), 1);
    assert!(
        owner
            .store
            .neutral_execution(&run.run.id)
            .unwrap()
            .attempt
            .as_ref()
            .unwrap()
            .result
            .is_some()
    );
    assert!(
        host.message(
            &connection,
            "tools/call".into(),
            Some(params(&call.id, "increment", 31))
        )
        .await
        .is_err()
    );
    tokio::task::yield_now().await;
}

// Catches turning an uncertain write into success or dispatching it again to obtain a response.
#[tokio::test]
async fn model_mcp_uncertain_original_call_returns_explicit_error_without_retry() {
    let (dir, owner, run, at) = fixture(30, false);
    let (_, mut call) = parent(&owner, &run, at);
    call.id = "b".repeat(64);
    let writer = FileHost::new(&owner, dir.path().join("uncertain"), Reply::Lost, false);
    owner.neutral_host(writer.clone());
    let host = ModelMcpHost::new(&owner, call.parent.clone()).unwrap();
    let (connection, _) = channel(&host, &call);
    let error = host
        .message(
            &connection,
            "tools/call".into(),
            Some(params(&call.id, "increment", 30)),
        )
        .await
        .unwrap_err();
    assert!(error.message.contains("uncertain"));
    let duplicate = host
        .message(
            &connection,
            "tools/call".into(),
            Some(params(&call.id, "increment", 30)),
        )
        .await
        .unwrap_err();
    assert!(duplicate.message.contains("uncertain"));
    assert_eq!(writer.calls.load(Ordering::Acquire), 1);
}
