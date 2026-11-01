//! Private native validation contracts and client-compatible advertised schemas.
use std::{collections::HashMap, sync::LazyLock};

use serde_json::{Value, json};

use super::mcp_transport_catalogue::*;

/// Add the action-dependent requirements that prose alone cannot enforce.
pub(super) fn complete_schema(tool: &mut Value) {
    let name = tool["name"].as_str().unwrap_or("").to_owned();
    let schema = &mut tool["inputSchema"];
    let actions = match name.as_str() {
        "session" => Some(SESSION_ACTIONS),
        "agent" => Some(AGENT_ACTIONS),
        "workflow" => Some(WORKFLOW_ACTIONS),
        "repo" => Some(REPO_ACTIONS),
        "ui" => Some(UI_ACTIONS),
        "voice" => Some(VOICE_ACTIONS),
        "debug" => Some(DEBUG_ACTIONS),
        _ => None,
    };
    if let Some(actions) = actions {
        schema["properties"]["action"]["enum"] = json!(actions.split(", ").collect::<Vec<_>>());
    }
    let mut conditions = Vec::new();
    let mut required = |actions: &[&str], fields: &[&str]| {
        conditions.push(json!({
            "if": {"properties":{"action":{"enum":actions}}, "required":["action"]},
            "then": {"required":fields}
        }));
    };
    match name.as_str() {
        "session" => {
            required(
                &[
                    "submit",
                    "force_retry",
                    "input",
                    "output",
                    "status",
                    "wait",
                    "resize",
                    "rename",
                    "keep_open",
                    "suspend",
                    "close",
                    "kill",
                    "pause",
                    "resume",
                ],
                &["session_id"],
            );
            required(&["submit"], &["input"]);
            required(&["rename"], &["name"]);
            required(&["keep_open"], &["enabled"]);
            required(&["declare_worktree"], &["worktree_path"]);
            conditions.push(json!({"if":{"properties":{"action":{"const":"input"}},"required":["action"]},"then":{"anyOf":[{"required":["input"]},{"required":["special_key"]},{"required":["pty_description"]}]}}));
        }
        "agent" => {
            required(&["send"], &["to", "message"]);
            required(&["task_get", "task_cancel"], &["task_id"]);
            required(&["subscribe", "ack", "unsubscribe"], &["subscription_id"]);
            required(&["ack"], &["cursor"]);
        }
        "repo" => {
            required(
                &[
                    "add",
                    "branch_integrations",
                    "branch_integration",
                    "worktree_list",
                    "worktree_lifecycle",
                    "worktree_create",
                    "worktree_remove",
                    "orphan_cleanup_answer",
                    "branch_delete",
                    "progress_list",
                ],
                &["path"],
            );
            required(
                &["branch_integration", "worktree_lifecycle", "branch_delete"],
                &["branch"],
            );
            required(&["orphan_cleanup_answer"], &["decision"]);
            required(&["work_item_act"], &["operation"]);
            conditions.push(json!({"if":{"properties":{"action":{"const":"worktree_remove"}},"required":["action"]},"then":{"anyOf":[{"required":["branch"]},{"required":["worktree_path"]}]}}));
        }
        "ui" => {
            let design = crate::design_mode::mcp::tool_definition();
            for field in [
                "session_id",
                "repo_path",
                "browser",
                "script",
                "format",
                "quality",
                "max_long_edge",
            ] {
                schema["properties"][field] = design["inputSchema"]["properties"][field].clone();
            }
            let tab_url = schema["properties"]["url"]["description"]
                .as_str()
                .unwrap_or_default();
            schema["properties"]["url"]["description"] = json!(format!(
                "{tab_url} For design_start/design_navigate: absolute HTTP(S) page URL; remote hosts require an extension host grant."
            ));
            required(&["design_start"], &["session_id"]);
            required(
                &[
                    "design_stop",
                    "design_reload",
                    "design_navigate",
                    "design_screenshot",
                    "design_errors",
                    "design_eval",
                ],
                &["repo_path"],
            );
            required(&["design_navigate"], &["url"]);
            required(&["design_eval"], &["script"]);
            required(&["tab"], &["id", "title"]);
            required(&["toast"], &["title"]);
            required(&["screenshot"], &["id"]);
            conditions.push(json!({"if":{"properties":{"action":{"const":"tab"}},"required":["action"]},"then":{"oneOf":[{"required":["html"]},{"required":["url"]}]}}));
            conditions.push(json!({"if":{"properties":{"action":{"const":"confirm"}},"required":["action"]},"then":{"anyOf":[{"required":["title"]},{"required":["proposalId"]}]}}));
        }
        "voice" => required(&["speak"], &["text"]),
        "debug" => required(&["invoke_js"], &["script"]),
        "remote" => required(&["update"], &["confirmed_sessions", "expected_sha256"]),
        _ => {}
    }
    if !conditions.is_empty() {
        schema["allOf"] = json!(conditions);
    }
    if matches!(name.as_str(), "session" | "agent") {
        for field in ["rows", "cols"] {
            schema["properties"][field]["minimum"] = json!(1);
            schema["properties"][field]["maximum"] = json!(super::MAX_TERMINAL_DIMENSION);
        }
    }
    if name == "agent" {
        // Launch has two disjoint forms. Validate both before the journal or PTY
        // handler runs, and publish the same contract through lazy discovery.
        let spawn_properties: serde_json::Map<String, Value> = [
            "prompt",
            "pty_description",
            "model",
            "env",
            "print_mode",
            "output_format",
            "agent_type",
            "binary_path",
            "args",
            "rows",
            "cols",
        ]
        .into_iter()
        .map(|field| (field.to_owned(), schema["properties"][field].clone()))
        .collect();
        schema["properties"]["spawn"]["properties"] = json!(spawn_properties);
        schema["properties"]["spawn"]["required"] = json!(["prompt"]);
        schema["properties"]["spawn"]["not"] = json!({"anyOf": [
            {"required":["action"]}, {"required":["name"]}, {"required":["keep_open"]},
            {"required":["cwd"]}, {"required":["parent_session_id"]}, {"required":["idempotency_key"]}
        ]});
        schema["properties"]["workspace"]["oneOf"] = json!([
            {"type":"object", "properties":{"existing_path":{"type":"string"}},
             "required":["existing_path"], "additionalProperties":false},
            {"type":"object", "properties":{"repo_path":{"type":"string"}, "branch":{"type":"string"}, "base_ref":{"type":["string","null"]}},
             "required":["repo_path","branch"], "additionalProperties":false}
        ]);
        schema["allOf"].as_array_mut().unwrap().push(json!({
            "if":{"properties":{"action":{"const":"launch"}},"required":["action"]},
            "then":{"oneOf":[
                {"properties":{"action":{},"launch_id":{}}, "required":["launch_id"], "additionalProperties":false},
                {"properties":{"action":{},"idempotency_key":{"pattern":"\\S"},"parent_session_id":{},"name":{"pattern":"^\\S(?:[\\s\\S]*\\S)?$"},"keep_open":{},"workspace":{},"spawn":{}},
                 "required":["idempotency_key","parent_session_id","name","keep_open","workspace","spawn"], "additionalProperties":false}
            ]}
        }));
    }
    if name == "progress" {
        schema["additionalProperties"] = json!(false);
        schema["properties"]["text"]["maxLength"] = json!(crate::progress::MAX_TEXT_CHARS);
        schema["properties"]["text"]["minLength"] = json!(1);
        schema["properties"]["text"]["pattern"] = json!("\\S");
        schema["properties"]["step"]["maxLength"] = json!(crate::progress::MAX_STEP_CHARS);
        schema["properties"]["text"]["description"] = json!(
            "Outcome, not implementation. At most 500 Unicode scalar values, before trimming; not UTF-8 bytes, UTF-16 units or grapheme clusters."
        );
    }
}

/// Anthropic rejects root combinators. Keep the full contract private and publish
/// the union of action fields, with conditional requirements in description text.
/// Nested anyOf parameters are supported by the Anthropic JSON Schema API.
pub(super) fn advertise_schema(tool: &mut Value) {
    let schema = &mut tool["inputSchema"];
    let object = schema.as_object_mut().expect("native input schema object");
    if let Some(conditions) = object.remove("allOf") {
        let description = object.entry("description").or_insert(json!(""));
        *description = json!(format!(
            "{} Action-specific requirements (checked by the server before dispatch): {conditions}",
            description.as_str().unwrap_or_default()
        ));
    }
    for keyword in ["oneOf", "anyOf"] {
        let Some(variants) = object.remove(keyword) else {
            continue;
        };
        fn collect_object_alternatives<'a>(schema: &'a Value, objects: &mut Vec<&'a Value>) {
            if schema.get("properties").is_some() {
                objects.push(schema);
            }
            // Untagged envelopes and flattened owner enums can wrap tagged
            // actions. Traverse their object-level unions, not field schemas.
            for keyword in ["oneOf", "anyOf", "allOf"] {
                if let Some(variants) = schema[keyword].as_array() {
                    for variant in variants {
                        collect_object_alternatives(variant, objects);
                    }
                }
            }
        }
        let mut objects = Vec::new();
        for variant in variants.as_array().expect("native action variants") {
            collect_object_alternatives(variant, &mut objects);
        }
        let mut fields: std::collections::BTreeMap<String, Vec<Value>> = Default::default();
        let mut actions = Vec::new();
        let mut required: Option<Vec<Value>> = None;
        for variant in objects {
            let names = variant["required"].as_array().cloned().unwrap_or_default();
            required = Some(match required {
                None => names,
                Some(previous) => previous
                    .into_iter()
                    .filter(|name| names.contains(name))
                    .collect(),
            });
            if let Some(properties) = variant["properties"].as_object() {
                for (name, property) in properties {
                    if name == "action" {
                        for action in property
                            .get("const")
                            .into_iter()
                            .chain(property["enum"].as_array().into_iter().flatten())
                        {
                            if !actions.contains(action) {
                                actions.push(action.clone());
                            }
                        }
                    } else {
                        let options = fields.entry(name.clone()).or_default();
                        if !options.contains(property) {
                            options.push(property.clone());
                        }
                    }
                }
            }
        }
        let properties = object.entry("properties").or_insert(json!({}));
        for (name, mut options) in fields {
            properties[&name] = if options.len() == 1 {
                options.pop().unwrap()
            } else {
                json!({"anyOf":options})
            };
        }
        if !actions.is_empty() {
            let required = required.get_or_insert_default();
            if !required.iter().any(|field| field == "action") {
                required.push(json!("action"));
            }
            properties["action"] = json!({"type":"string", "enum":actions,
                "description":"Select the action. Its required fields are described by the tool; the server validates the complete action contract before dispatch."});
        }
        object.insert("type".into(), json!("object"));
        object.insert("required".into(), json!(required.unwrap_or_default()));
    }
    // Use the documented nested union form, not exclusive-union enforcement:
    // exclusivity remains in the unchanged private validator.
    fn nested_unions(value: &mut Value) {
        match value {
            Value::Object(object) => {
                if let Some(variants) = object.remove("oneOf") {
                    object.insert("anyOf".into(), variants);
                }
                for child in object.values_mut() {
                    nested_unions(child);
                }
            }
            Value::Array(array) => array.iter_mut().for_each(nested_unions),
            _ => {}
        }
    }
    nested_unions(schema);
}

struct Contract {
    schema: Value,
    validator: Result<jsonschema::Validator, String>,
}

static CONTRACTS: LazyLock<HashMap<String, Contract>> = LazyLock::new(|| {
    native_validation_tool_definitions()
        .as_array()
        .expect("native tool array")
        .iter()
        .map(|tool| {
            let schema = tool["inputSchema"].clone();
            let validator = jsonschema::validator_for(&schema)
                .map_err(|_| "invalid native input schema".to_owned());
            (
                tool["name"].as_str().expect("native name").to_owned(),
                Contract { schema, validator },
            )
        })
        .collect()
});

pub(super) fn validate(name: &str, args: &Value) -> Option<Value> {
    let contract = CONTRACTS.get(name)?;
    let validator = match &contract.validator {
        Ok(validator) => validator,
        Err(error) => {
            return Some(
                json!({"error":error,"code":"invalid_contract","effect":"not_started","retryable":false}),
            );
        }
    };
    let violations: Vec<Value> = validator
        .iter_errors(args)
        .take(16)
        .map(|error| {
            let schema_path = error.schema_path().to_string();
            json!({
                "path":error.instance_path().to_string(),
                "schema_path":schema_path,
                "constraint":contract.schema.pointer(&schema_path),
                "message":error.masked().to_string()
            })
        })
        .collect();
    (!violations.is_empty()).then(|| json!({
        "error": format!("{name}: {}: {}", violations[0]["path"].as_str().unwrap_or(""), violations[0]["message"].as_str().unwrap_or("invalid arguments")),
        "code":"invalid_arguments", "effect":"not_started", "outcome":"rejected",
        "retryable":false, "repair":{"retry_after_correction":true}, "violations":violations
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Catches: agent task actions accept a missing or non-string task handle before dispatch.
    #[test]
    fn agent_task_actions_require_a_string_task_id() {
        for action in ["task_get", "task_cancel"] {
            for args in [
                json!({"action":action}),
                json!({"action":action,"task_id":1}),
            ] {
                assert_eq!(
                    validate("agent", &args).unwrap()["code"],
                    "invalid_arguments"
                );
            }
            assert!(validate("agent", &json!({"action":action,"task_id":"handle"})).is_none());
        }
    }

    /// Catches: newly advertised subscription actions omit their repairable required fields.
    #[test]
    fn actionable_subscription_schema_requires_identity_and_ack_cursor() {
        for action in ["subscribe", "ack", "unsubscribe"] {
            let error = validate("agent", &json!({"action":action})).unwrap();
            assert_eq!(error["code"], "invalid_arguments");
            let args = json!({"action":action,"subscription_id":"pump"});
            assert_eq!(validate("agent", &args).is_some(), action == "ack");
        }
        assert!(
            validate(
                "agent",
                &json!({"action":"ack","subscription_id":"pump","cursor":1})
            )
            .is_none()
        );
    }

    /// Catches: a client and server disagree on historical bad inputs or Unicode boundaries.
    #[test]
    fn action_validation_consumer_fixture_matches_advertised_contract() {
        let cases: Value =
            serde_json::from_str(include_str!("fixtures/action-input-contract-2007.json")).unwrap();
        for case in cases.as_array().unwrap() {
            let tool = case["tool"].as_str().unwrap();
            let valid = case["valid"].as_bool().unwrap();
            assert_eq!(
                validate(tool, &case["arguments"]).is_none(),
                valid,
                "{}",
                case["name"]
            );
            if tool == "progress" && case["arguments"].get("text").is_some() {
                let input: crate::progress::ProgressReportInput =
                    serde_json::from_value(case["arguments"].clone()).unwrap();
                assert_eq!(
                    input.into_entry(None).unwrap().validate().is_ok(),
                    valid,
                    "shared HTTP/IPC core: {}",
                    case["name"]
                );
            }
        }
    }

    /// Catches: repair errors echo secret input values or recommend an unchanged retry.
    #[test]
    fn action_validation_repairs_exclude_argument_values() {
        let error = validate("agent", &json!({"action":"secret-token-not-an-action"})).unwrap();
        assert!(!error.to_string().contains("secret-token-not-an-action"));
        assert_eq!(error["violations"][0]["path"], "/action");
        assert!(
            error["violations"][0]["constraint"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item == "list_peers")
        );
        assert_eq!(error["effect"], "not_started");
        assert_eq!(error["retryable"], false);
    }
}
