use super::neutral_model_adapter::run_request;
use super::neutral_model_request::{ModelLimits, ModelRequest, UnavailablePolicy};
use crate::acp::{
    AcpClientManager, EgoAcpConfig, McpNotify, McpOverAcpError, McpOverAcpHost, McpReply,
};
use crate::workflows::catalog::{FieldSchema, ObjectSchema, ValueType};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

struct NoTools;
/// A host whose ledger admits one tool, like the converted implementer's stage_files.
struct StageTool;
impl McpOverAcpHost for StageTool {
    fn has_inbox_subscriber(&self, _: &str) -> bool {
        false
    }
    fn connect(&self, _: Option<&str>, _: McpNotify) -> Result<String, McpOverAcpError> {
        Ok("fixture".into())
    }
    fn message(&self, _: &str, _: String, _: Option<Map<String, Value>>) -> McpReply {
        Box::pin(async { Err(McpOverAcpError::new(-32601, "no tools")) })
    }
    fn notification(&self, _: &str, _: String, _: Option<Map<String, Value>>) {}
    fn disconnect(&self, _: &str) {}
    fn preapproved_tools(&self) -> Vec<String> {
        vec!["stage_files".into()]
    }
}
impl McpOverAcpHost for NoTools {
    fn has_inbox_subscriber(&self, _: &str) -> bool {
        false
    }
    fn connect(&self, _: Option<&str>, _: McpNotify) -> Result<String, McpOverAcpError> {
        Ok("fixture".into())
    }
    fn message(&self, _: &str, _: String, _: Option<Map<String, Value>>) -> McpReply {
        Box::pin(async { Err(McpOverAcpError::new(-32601, "no tools")) })
    }
    fn notification(&self, _: &str, _: String, _: Option<Map<String, Value>>) {}
    fn disconnect(&self, _: &str) {}
}
pub(super) fn request() -> ModelRequest {
    ModelRequest {
        profile: "exact-profile".into(),
        model: "fixture-model".into(),
        effort: "high".into(),
        instructions: "Transform literal context".into(),
        context: vec!["$(must remain literal)".into()],
        output_schema: ObjectSchema {
            fields: BTreeMap::from([(
                "next".into(),
                FieldSchema {
                    value_type: ValueType::Integer,
                    required: true,
                    default: None,
                },
            )]),
        },
        limits: ModelLimits {
            timeout_ms: 60_000,
            max_context_bytes: 1024,
            max_output_bytes: 32,
        },
        unavailable_policy: UnavailablePolicy::Fail,
    }
}
pub(super) fn fixture(output: &str, drift: bool) -> (tempfile::TempDir, EgoAcpConfig) {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    std::fs::copy(
        "tests/fixtures/acp/ego-initialize.json",
        dir.path().join("ego-initialize.json"),
    )
    .unwrap();
    std::fs::write(dir.path().join("expected-profile.txt"), "exact-profile").unwrap();
    std::fs::write(dir.path().join("expected-mediated"), "").unwrap();
    let session = "01932d5e-0000-7000-8000-0000000000aa";
    let options = |model: &str| {
        json!([
            {"id":"model","name":"Model","type":"select","currentValue":model,
             "options":[{"value":"fixture-model","name":"Fixture"}]},
            {"id":"effort","name":"Effort","type":"select","currentValue":"high",
             "options":[{"value":"high","name":"High"}]}
        ])
    };
    let mut steps = vec![
        json!({"step":"expect","method":"initialize","capture":"init"}),
        json!({"step":"respond","to":"init","result_file":"ego-initialize.json"}),
        json!({"step":"expect","method":"session/new","capture":"new",
               "params":{"mcpServers":[{"name":"tuicommander","type":"acp","serverId":"tuicommander"}]}}),
        json!({"step":"respond","to":"new","result":{"sessionId":session,"configOptions":options("fixture-model")}}),
    ];
    for (id, value) in [("model", "fixture-model"), ("effort", "high")] {
        steps.push(
            json!({"step":"expect","method":"session/set_config_option","capture":"set",
                          "params":{"configId":id,"value":value}}),
        );
        steps.push(
            json!({"step":"respond","to":"set","result":{"configOptions":options(
                if drift { "substituted" } else { "fixture-model" }
            )}}),
        );
    }
    if !drift {
        steps.extend([
            json!({"step":"expect","method":"session/prompt","capture":"prompt","params":{"sessionId":session}}),
            json!({"step":"notify","method":"session/update","params":{"sessionId":session,
                "update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":output}}}}),
            json!({"step":"respond","to":"prompt","result":{"stopReason":"end_turn"}}),
        ]);
    }
    steps.push(json!({"step":"await_stdin_eof"}));
    std::fs::write(
        dir.path().join("scenario.jsonl"),
        steps
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let executable = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(format!(
            "tuic-acp-fixture-agent{}",
            std::env::consts::EXE_SUFFIX
        ));
    assert!(executable.is_file(), "nextest setup builds ACP fixture");
    (
        dir,
        EgoAcpConfig {
            executable,
            profile: "exact-profile".into(),
            master_prompt: String::new(),
            ai_chat: false,
        },
    )
}

// Catches accidental broad launch flags, settings drift and session binding after prompting.
#[tokio::test]
async fn exact_model_request_uses_mediated_process_and_binds_before_result() {
    let (dir, runtime) = fixture("{\"next\":7}", false);
    let bound = Mutex::new(None);
    let result = run_request(
        &AcpClientManager::new(),
        &request(),
        &runtime,
        dir.path().into(),
        Arc::new(NoTools),
        |session| {
            assert_eq!(session.session, "01932d5e-0000-7000-8000-0000000000aa");
            *bound.lock().unwrap() = Some(session);
            Ok(())
        },
    )
    .await
    .unwrap();
    assert!(bound.lock().unwrap().is_some());
    assert_eq!(result, BTreeMap::from([("next".into(), json!(7))]));
}

// Catches a mediated launch that does not pre-approve the host's ledger-owned tools:
// ego then asks a person no unattended turn has, the host cancels, and every
// mediated call is refused ("stage_files may not opaque tuicommander/stage_files:
// permission was refused", live run 2026-11-13).
#[tokio::test]
async fn mediated_launch_preapproves_each_host_tool_in_ego() {
    let (dir, runtime) = fixture("{\"next\":7}", false);
    std::fs::write(
        dir.path().join("expected-mediated"),
        "--allow\ntool(tuicommander/stage_files)\n",
    )
    .unwrap();
    let result = run_request(
        &AcpClientManager::new(),
        &request(),
        &runtime,
        dir.path().into(),
        Arc::new(StageTool),
        |_| Ok(()),
    )
    .await;
    assert_eq!(result, Ok(BTreeMap::from([("next".into(), json!(7))])));
}

// Catches silent model substitution even when set-config returns a successful response.
#[tokio::test]
async fn mismatched_acknowledgment_never_binds_or_prompts() {
    let (dir, runtime) = fixture("unused", true);
    let error = run_request(
        &AcpClientManager::new(),
        &request(),
        &runtime,
        dir.path().into(),
        Arc::new(NoTools),
        |_| panic!("substituted model must not bind"),
    )
    .await
    .unwrap_err();
    assert!(error.contains("not acknowledged"), "{error}");
}

// Catches unbounded buffering and permissive extraction of fenced/non-object responses.
#[tokio::test]
async fn malformed_and_oversized_model_responses_fail_closed() {
    for (output, expected) in [
        ("```json\n{\"next\":7}\n```", "complete JSON"),
        ("xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx", "byte limit"),
        ("{\"next\":\"wrong\"}", "wrong type"),
    ] {
        let (dir, runtime) = fixture(output, false);
        let error = run_request(
            &AcpClientManager::new(),
            &request(),
            &runtime,
            dir.path().into(),
            Arc::new(NoTools),
            |_| Ok(()),
        )
        .await
        .unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
}

// Catches profile fallback, unsupported setting acceptance and ignored literal context limits.
#[test]
fn exact_request_rejects_unknown_settings_profile_fallback_and_oversized_context() {
    let mut request = request();
    let runtime = EgoAcpConfig {
        executable: std::env::current_exe().unwrap(),
        profile: "other".into(),
        master_prompt: String::new(),
        ai_chat: false,
    };
    assert!(
        request
            .validate(&runtime)
            .unwrap_err()
            .contains("exact isolated")
    );
    let mut value = serde_json::to_value(&request).unwrap();
    value["temperature"] = json!(0.5);
    assert!(serde_json::from_value::<ModelRequest>(value).is_err());
    let runtime = EgoAcpConfig {
        profile: request.profile.clone(),
        ..runtime
    };
    request.limits.max_context_bytes = 1;
    assert!(
        request
            .validate(&runtime)
            .unwrap_err()
            .contains("context exceeds")
    );
    request.unavailable_policy = UnavailablePolicy::Fail;
    let mut value = serde_json::to_value(&request).unwrap();
    value["unavailable_policy"] = json!("substitute");
    assert!(serde_json::from_value::<ModelRequest>(value).is_err());
}

// Catches the adapter waiting its whole declared timeout when ego reports that
// session setup failed, then hiding ego's reason behind a generic timeout.
#[tokio::test]
async fn failed_ego_session_configuration_fails_at_once_with_ego_error() {
    // Refusals carry the reason in `message`; internal errors carry it in `data`.
    for (error, reason) in [
        (
            json!({"code":-32602,"message":"adaptive source crosses a hard write floor"}),
            "adaptive source crosses a hard write floor",
        ),
        (
            json!({"code":-32603,"message":"Internal error","data":"control store unreadable"}),
            "control store unreadable",
        ),
    ] {
        failed_configuration_case(error, reason).await;
    }
}

async fn failed_configuration_case(ego_error: Value, reason: &str) {
    let (dir, runtime) = fixture("unused", true);
    let session = "01932d5e-0000-7000-8000-0000000000aa";
    // Ego's deferred readiness shape (ego-acp serve.rs failed_config_options).
    let steps = [
        json!({"step":"expect","method":"initialize","capture":"init"}),
        json!({"step":"respond","to":"init","result_file":"ego-initialize.json"}),
        json!({"step":"expect","method":"session/new","capture":"new"}),
        json!({"step":"respond","to":"new","result":{"sessionId":session,
            "_meta":{"ego":{"configOptions":{"state":"pending"}}}}}),
        json!({"step":"notify","method":"session/update","params":{"sessionId":session,
            "update":{"sessionUpdate":"config_option_update","configOptions":[],
                "_meta":{"ego":{"configOptions":{"state":"failed"},
                    "error":ego_error}}}}}),
        json!({"step":"await_stdin_eof"}),
    ];
    std::fs::write(
        dir.path().join("scenario.jsonl"),
        steps
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let error = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        run_request(
            &AcpClientManager::new(),
            &request(),
            &runtime,
            dir.path().into(),
            Arc::new(NoTools),
            |_| panic!("a failed session must not bind"),
        ),
    )
    .await
    .expect("a failed ego session must not wait for the 60 s request limit")
    .unwrap_err();
    assert!(error.contains(reason), "{error}");
}

// Catches setting effort before the model: ego offers only the efforts of the
// current model (`ego-cli acp_controls.rs` `efforts(model, ..)`), so a valid
// model/effort pair the profile default lacks waits for an offer that never
// arrives and fails as a timeout instead of reaching the prompt.
#[tokio::test]
async fn effort_offered_only_by_requested_model_is_set_after_the_model() {
    let (dir, runtime) = fixture("{\"next\":7}", false);
    let session = "01932d5e-0000-7000-8000-0000000000aa";
    let options = |model: &str, effort: &str, efforts: &[&str]| {
        json!([
            {"id":"model","name":"Model","type":"select","currentValue":model,
             "options":[{"value":"default-model","name":"Default"},
                        {"value":"fixture-model","name":"Fixture"}]},
            {"id":"effort","name":"Effort","type":"select","currentValue":effort,
             "options":efforts.iter().map(|e| json!({"value":e,"name":e})).collect::<Vec<_>>()}
        ])
    };
    let steps = [
        json!({"step":"expect","method":"initialize","capture":"init"}),
        json!({"step":"respond","to":"init","result_file":"ego-initialize.json"}),
        json!({"step":"expect","method":"session/new","capture":"new"}),
        json!({"step":"respond","to":"new","result":{"sessionId":session,
            "configOptions":options("default-model","off",&["off"])}}),
        json!({"step":"expect","method":"session/set_config_option","capture":"model",
               "params":{"configId":"model","value":"fixture-model"}}),
        json!({"step":"respond","to":"model","result":{
            "configOptions":options("fixture-model","off",&["off","high"])}}),
        json!({"step":"expect","method":"session/set_config_option","capture":"effort",
               "params":{"configId":"effort","value":"high"}}),
        json!({"step":"respond","to":"effort","result":{
            "configOptions":options("fixture-model","high",&["off","high"])}}),
        json!({"step":"expect","method":"session/prompt","capture":"prompt","params":{"sessionId":session}}),
        json!({"step":"notify","method":"session/update","params":{"sessionId":session,
            "update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"{\"next\":7}"}}}}),
        json!({"step":"respond","to":"prompt","result":{"stopReason":"end_turn"}}),
        json!({"step":"await_stdin_eof"}),
    ];
    std::fs::write(
        dir.path().join("scenario.jsonl"),
        steps
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let mut request = request();
    request.limits.timeout_ms = 5_000;
    let result = run_request(
        &AcpClientManager::new(),
        &request,
        &runtime,
        dir.path().into(),
        Arc::new(NoTools),
        |_| Ok(()),
    )
    .await;
    assert_eq!(result, Ok(BTreeMap::from([("next".into(), json!(7))])));
}
