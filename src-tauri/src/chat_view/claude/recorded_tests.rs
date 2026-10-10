//! Regression cases recorded from real CLI transcripts, with payloads removed.

use super::*;

fn cases() -> Vec<(String, String)> {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/chat_view/recorded");
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("survey.json")).unwrap()).unwrap();
    manifest["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            let name = entry["file"].as_str().unwrap();
            (
                entry["shape"].as_str().unwrap().to_owned(),
                std::fs::read_to_string(root.join(name)).unwrap(),
            )
        })
        .collect()
}

fn run(text: &str) -> Vec<Value> {
    let mut adapter = ClaudeAdapter::default();
    text.lines().flat_map(|line| adapter.absorb(line)).collect()
}

// Catches: older string and array prompts disappear because origin was not yet recorded.
#[test]
fn recorded_legacy_prompts_survive_without_origin_but_harness_echoes_do_not() {
    let cases = cases();
    for signature in ["user|str|absent|", "user|list|absent|text"] {
        let (_, fixture) = cases
            .iter()
            .find(|(s, _)| s == signature)
            .expect("recorded legacy prompt");
        let updates = run(fixture);
        assert_eq!(updates.len(), 1, "{signature}");
        assert_eq!(
            updates[0]["sessionUpdate"], "user_message_chunk",
            "{signature}"
        );
        assert!(!updates[0]["content"]["text"].as_str().unwrap().is_empty());
    }
    let mut checked = 0;
    for (shape, fixture) in &cases {
        if shape.starts_with("user|str|absent|") && shape.contains('<') {
            assert!(
                run(fixture).is_empty(),
                "harness echo became a prompt: {shape}"
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 5,
        "recorded command, stdout and bash echoes must be exercised"
    );
}

// Catches: image-only tool output looks empty, and PDF output loses its attachment marker.
#[test]
fn recorded_tool_results_keep_image_and_document_placeholders() {
    let cases = cases();
    for (signature, expected) in [
        ("user|list|absent|tool_result|list|False|image", "[image]"),
        (
            "user|list|absent|tool_result|list|False|text|document",
            "[document]",
        ),
        (
            "user|list|absent|tool_result|list|False|text|image",
            "[image]",
        ),
    ] {
        let (_, fixture) = cases
            .iter()
            .find(|(s, _)| s == signature)
            .expect("recorded media result with its real call");
        let updates = run(fixture);
        let result = updates
            .iter()
            .find(|u| u["sessionUpdate"] == "tool_call_update")
            .expect("call completed");
        assert_eq!(result["status"], "completed", "{signature}");
        assert!(
            result["content"][0]["content"]["text"]
                .as_str()
                .unwrap()
                .contains(expected),
            "{signature}"
        );
        assert!(
            !updates
                .iter()
                .any(|u| u["sessionUpdate"] == "user_message_chunk"),
            "a tool result is not a human prompt"
        );
    }
}

// Catches: a real fallback block silently drops the model transition from the conversation.
#[test]
fn recorded_model_fallback_is_a_card() {
    let (_, fixture) = cases()
        .into_iter()
        .find(|(s, _)| s == "assistant|list|absent|fallback")
        .expect("recorded model fallback");
    let updates = run(&fixture);
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0]["content"]["text"], "Model changed");
    assert_eq!(updates[0]["_meta"]["ego"]["salience"], "card");
}

// Catches: newer metadata pollutes unknown-row diagnostics; sidechain messages leak into the parent chat.
#[test]
fn recorded_plumbing_and_sidechains_remain_outside_the_conversation() {
    let mut checked = 0;
    for (shape, fixture) in cases() {
        let row: Value = serde_json::from_str(fixture.lines().last().unwrap()).unwrap();
        let kind = row["type"].as_str().unwrap();
        if kind == "attachment"
            && row.pointer("/attachment/type") == Some(&json!("queued_command"))
            && row.pointer("/attachment/origin/kind") == Some(&json!("human"))
        {
            let updates = run(&fixture);
            assert_eq!(updates.len(), 1, "recorded human queued prompt: {shape}");
            assert_eq!(updates[0]["sessionUpdate"], "user_message_chunk");
        } else if !matches!(kind, "user" | "assistant") || row["isSidechain"] == true {
            let mut adapter = ClaudeAdapter::default();
            for line in fixture.lines() {
                assert!(adapter.absorb(line).is_empty(), "{shape}");
            }
            assert_eq!(
                adapter.stats.unknown_rows, 0,
                "recognized real metadata: {shape}"
            );
            assert_eq!(adapter.stats.malformed_rows, 0, "{shape}");
            checked += 1;
        }
    }
    assert!(
        checked > 0,
        "recorded metadata and sidechains must be exercised"
    );
}

// Catches: real array prompts with images vanish, and compact summaries leak their private body.
#[test]
fn recorded_image_prompts_and_compaction_keep_their_conversation_role() {
    let mut images = 0;
    let mut summaries = 0;
    for (shape, fixture) in cases() {
        if shape.starts_with("user|list|human|") && shape.contains("image") {
            let updates = run(&fixture);
            assert_eq!(updates.len(), 1, "{shape}");
            assert_eq!(updates[0]["sessionUpdate"], "user_message_chunk");
            assert!(
                updates[0]["content"]["text"]
                    .as_str()
                    .unwrap()
                    .contains("[image]")
            );
            images += 1;
        } else if shape.starts_with("user|")
            && shape.contains("isCompactSummary")
            && !shape.contains("isSidechain")
        {
            let updates = run(&fixture);
            assert_eq!(updates.len(), 1, "{shape}");
            assert_eq!(updates[0]["content"]["text"], "Conversation compacted");
            assert_eq!(updates[0]["_meta"]["ego"]["salience"], "card");
            summaries += 1;
        }
    }
    assert!(
        images > 0 && summaries > 0,
        "real images and summaries must be exercised"
    );
}

// Catches: queued prompts bypass redaction, text limits, or human-origin filtering.
#[test]
fn recorded_queued_prompts_share_normal_prompt_safety() {
    let fixture = include_str!("../../fixtures/chat_view/recorded/queued-human.jsonl");
    let mut row: Value = fixture
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .find(|r| r.pointer("/attachment/origin/kind") == Some(&json!("human")))
        .unwrap();
    let secret = "sk-ant-api03-abcdefghijklmnopqrstuvwxyz1234567890";
    let raw = format!("{secret} {}", "é".repeat(MAX_TEXT_CHARS));
    row["attachment"]["prompt"] = json!(raw);
    let updates = run(&row.to_string());
    assert_eq!(updates.len(), 1);
    assert!(
        !updates[0]["content"]["text"]
            .as_str()
            .unwrap()
            .contains(secret)
    );
    assert_eq!(
        updates[0]["content"]["text"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        MAX_TEXT_CHARS
    );
    for origin in [
        Value::Null,
        json!({"kind":"task-notification"}),
        json!({"kind":"unknown"}),
    ] {
        row["attachment"]["origin"] = origin;
        assert!(run(&row.to_string()).is_empty());
    }
    row["attachment"]["origin"] = json!({"kind":"human"});
    for prompt in [Value::Null, json!(42), json!("  ")] {
        row["attachment"]["prompt"] = prompt;
        assert!(run(&row.to_string()).is_empty());
    }
}
