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
                    "get_settings",
                    "set_settings",
                ],
                &["path"],
            );
            required(&["set_settings"], &["settings"]);
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
                "result_offset",
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
                "{tab_url} For design_start/design_navigate: absolute HTTP(S) page URL; remote hosts require sidebar host consent."
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
    if tool["name"] == "workflow" {
        // The private schema still validates every typed action before dispatch.
        tool["inputSchema"]["properties"]["input"] = json!({
            "type": "object",
            "description": "Action payload. The server returns typed field errors for invalid input."
        });
    }
    prune_unreferenced_defs(&mut tool["inputSchema"]);
    if tool["name"]
        .as_str()
        .is_some_and(|name| TYPE_NOTES_IN_HELP.contains(&name))
        && let Some(defs) = tool["inputSchema"]
            .get_mut("$defs")
            .and_then(Value::as_object_mut)
    {
        for def in defs.values_mut() {
            walk_type_notes(def, "", &mut |_, _| {}, true);
        }
    }
}

/// Tools whose advertised schema leaves its `$defs` notes to `help`: the
/// `workflow` schema sits at the 65,536-byte `search_tools` budget, and the
/// notes are prose, not contract. The private validation schema keeps them.
pub(super) const TYPE_NOTES_IN_HELP: &[&str] = &["workflow"];

/// Visit every `description` keyword of a schema, keyed by its JSON path, and
/// remove it when `strip`. Keys of a `properties` map are field names, so a
/// field named `description` is never touched.
fn walk_type_notes(schema: &mut Value, path: &str, note: &mut dyn FnMut(&str, &str), strip: bool) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    let description = if strip {
        object.remove("description")
    } else {
        object.get("description").cloned()
    };
    if let Some(text) = description.as_ref().and_then(Value::as_str) {
        note(path, text);
    }
    for (keyword, child) in object.iter_mut() {
        match keyword.as_str() {
            "properties" | "$defs" | "definitions" | "patternProperties" => {
                if let Some(map) = child.as_object_mut() {
                    for (name, field) in map.iter_mut() {
                        walk_type_notes(field, &format!("{path}/{name}"), note, strip);
                    }
                }
            }
            "anyOf" | "oneOf" | "allOf" | "prefixItems" => {
                if let Some(items) = child.as_array_mut() {
                    for (index, item) in items.iter_mut().enumerate() {
                        walk_type_notes(item, &format!("{path}/{index}"), note, strip);
                    }
                }
            }
            "items" | "additionalProperties" | "not" => {
                walk_type_notes(child, path, note, strip);
            }
            _ => {}
        }
    }
}

/// The `$defs` notes a tool's advertised schema leaves out, keyed `Type/field`.
pub(super) fn type_notes(tool: &Value) -> serde_json::Map<String, Value> {
    let mut notes = serde_json::Map::new();
    let mut schema = tool["inputSchema"]["$defs"].clone();
    if let Some(defs) = schema.as_object_mut() {
        for (name, def) in defs.iter_mut() {
            walk_type_notes(
                def,
                name,
                &mut |path, text| {
                    notes.insert(path.to_owned(), json!(text));
                },
                false,
            );
        }
    }
    notes
}

/// Keep only the root `$defs` that the advertised schema still reaches. A capability denial
/// removes action variants, and their subtypes must not stay advertised through `$defs`.
fn prune_unreferenced_defs(schema: &mut Value) {
    let Some(Value::Object(mut defs)) = schema.as_object_mut().and_then(|o| o.remove("$defs"))
    else {
        return;
    };
    fn collect_refs(value: &Value, refs: &mut Vec<String>) {
        match value {
            Value::Object(object) => {
                if let Some(name) = object
                    .get("$ref")
                    .and_then(Value::as_str)
                    .and_then(|target| target.strip_prefix("#/$defs/"))
                {
                    refs.push(name.to_owned());
                }
                object.values().for_each(|child| collect_refs(child, refs));
            }
            Value::Array(array) => array.iter().for_each(|child| collect_refs(child, refs)),
            _ => {}
        }
    }
    let mut pending = Vec::new();
    collect_refs(schema, &mut pending);
    let mut reached = std::collections::BTreeSet::new();
    while let Some(name) = pending.pop() {
        if reached.insert(name.clone())
            && let Some(definition) = defs.get(&name)
        {
            collect_refs(definition, &mut pending);
        }
    }
    defs.retain(|name, _| reached.contains(name));
    if !defs.is_empty() {
        schema["$defs"] = Value::Object(defs);
    }
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

type ValidationError = jsonschema::ValidationError<'static>;

fn union_branches(error: &ValidationError) -> Option<&Vec<Vec<ValidationError>>> {
    match error.kind() {
        jsonschema::error::ValidationErrorKind::OneOfNotValid { context }
        | jsonschema::error::ValidationErrorKind::AnyOf { context } => Some(context),
        _ => None,
    }
}

/// Deepest instance location this error or a nested union branch reaches.
fn reach(error: &ValidationError) -> usize {
    let own = error.instance_path().as_str().matches('/').count();
    union_branches(error)
        .into_iter()
        .flatten()
        .flatten()
        .map(reach)
        .fold(own, usize::max)
}

/// A failed union only says that no branch matched. Rank each branch by whether its tag matched
/// (no `const` failure of its own), then by how deep it got into the instance. When exactly one
/// branch leads, report its errors, so a malformed or missing field of the matched action keeps
/// its path. Mismatched branches fail on their tag, and untagged branches at equal depth tie.
fn most_specific(error: jsonschema::ValidationError<'_>) -> Vec<ValidationError> {
    let error = error.to_owned();
    let Some(branches) = union_branches(&error) else {
        return vec![error];
    };
    let ranks: Vec<(bool, usize)> = branches
        .iter()
        .map(|branch| {
            let tag_matched = !branch.iter().any(|error| {
                matches!(
                    error.kind(),
                    jsonschema::error::ValidationErrorKind::Constant { .. }
                )
            });
            (tag_matched, branch.iter().map(reach).max().unwrap_or(0))
        })
        .collect();
    let best = ranks.iter().copied().max().unwrap_or_default();
    let mut leaders = (0..ranks.len()).filter(|index| ranks[*index] == best);
    let (Some(index), None) = (leaders.next(), leaders.next()) else {
        return vec![error];
    };
    match error.into_parts().kind {
        jsonschema::error::ValidationErrorKind::OneOfNotValid { mut context }
        | jsonschema::error::ValidationErrorKind::AnyOf { mut context } => context
            .swap_remove(index)
            .into_iter()
            .flat_map(most_specific)
            .collect(),
        _ => unreachable!("union error kind checked above"),
    }
}

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
        .flat_map(most_specific)
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
    if violations.is_empty() {
        return semantic_violation(name, args);
    }
    Some(rejection(name, violations))
}

/// The JSON schema is one flat object for every action, so a field legal for another action
/// (`count` on `list`) passes it and only the typed parser rejects it. That is still the caller's
/// malformed arguments, not a server fault: answer it with the same contract error.
/// `help` is a catalogue action answered after the role gates, not an automation action.
fn semantic_violation(name: &str, args: &Value) -> Option<Value> {
    if name != "automations" || args["action"] == "help" {
        return None;
    }
    let message = crate::automations::api::parse(args.clone()).err()?;
    Some(rejection(
        name,
        vec![json!({"path":"","schema_path":"","constraint":null,"message":message})],
    ))
}

fn rejection(name: &str, violations: Vec<Value>) -> Value {
    json!({
        "error": format!("{name}: {}: {}", violations[0]["path"].as_str().unwrap_or(""), violations[0]["message"].as_str().unwrap_or("invalid arguments")),
        "code":"invalid_arguments", "effect":"not_started", "outcome":"rejected",
        "retryable":false, "repair":{"retry_after_correction":true}, "violations":violations
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Catches: union expansion naming an arbitrary branch when no action tag matched, so an
    /// unknown action is reported as a broken field of an unrelated action.
    #[test]
    fn unknown_nested_action_keeps_the_union_violation() {
        let error = validate(
            "workflow",
            &json!({"action":"neutral","input":{"action":"no_such_action"}}),
        )
        .unwrap();
        assert_eq!(error["code"], "invalid_arguments");
        let paths: Vec<&str> = error["violations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|violation| violation["path"].as_str().unwrap())
            .collect();
        assert!(
            paths.iter().all(|path| path.len() <= "/input".len()),
            "{error}"
        );
    }

    /// Catches: a matched action whose own field is missing being reported as the generic union
    /// failure (with the whole union as its constraint) instead of the missing field.
    #[test]
    fn matched_action_with_a_missing_field_names_that_field() {
        let error = validate(
            "workflow",
            &json!({"action":"neutral","input":{"action":"publish","id":"d"}}),
        )
        .unwrap();
        assert!(
            error["violations"]
                .as_array()
                .unwrap()
                .iter()
                .any(|violation| {
                    let message = violation["message"].as_str().unwrap();
                    violation["path"] == "/input"
                        && message.contains("expected_revision")
                        && message.contains("required")
                }),
            "{error}"
        );
        let bytes = serde_json::to_vec(&error).unwrap().len();
        assert!(bytes < 4096, "{bytes} bytes: {error}");
    }

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
