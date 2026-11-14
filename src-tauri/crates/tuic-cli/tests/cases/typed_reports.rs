// Catches: CLI accepts report flags but drops them before the MCP request.
#[test]
fn agent_send_forwards_typed_report_and_keeps_legacy_payload() {
    for (flags, expected) in [
        (vec![], None),
        (
            vec!["--report-state", "ready"],
            Some(serde_json::json!({"state":"ready"})),
        ),
        (
            vec![
                "--report-state",
                "waiting",
                "--wait-for",
                "boss",
                "--question-json",
                r#"{"id":"scope","text":"Scope?"}"#,
            ],
            Some(
                serde_json::json!({"state":"waiting","wait_for":"boss","question":{"id":"scope","text":"Scope?"}}),
            ),
        ),
    ] {
        let mut args = vec!["agent", "send", "coordinator", "Summary", "--json"];
        args.extend(flags);
        let (output, requests) = run_with_stub(
            &args,
            None,
            Some("reporter"),
            r#"{"message_id":"m","delivered":false,"delivery_path":"inbox_only"}"#,
            false,
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let call = requests
            .iter()
            .map(|r| &r.1)
            .find(|r| r["method"] == "tools/call")
            .unwrap();
        assert_eq!(call["params"]["name"], "agent");
        let payload = &call["params"]["arguments"];
        assert_eq!(payload["action"], "send");
        assert_eq!(payload["message"], "Summary");
        assert_eq!(payload["to"], "coordinator");
        assert_eq!(payload.get("report"), expected.as_ref());
    }
}
