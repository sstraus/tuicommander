use super::*;
use crate::output_parser::{ChoiceOption, ChoicePromptPayload};
use serde_json::json;

/// The recorded PreToolUse `tool_input` of `tool` in a native hook fixture.
fn recorded_input(jsonl: &str, tool: &str) -> (String, Value) {
    jsonl
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|receipt| receipt["hook_event_name"] == "PreToolUse" && receipt["tool_name"] == tool)
        .map(|receipt| {
            (
                receipt["tool_use_id"].as_str().unwrap().to_owned(),
                receipt["tool_input"].clone(),
            )
        })
        .expect("recorded PreToolUse")
}

fn claude_ask() -> (String, Value) {
    recorded_input(
        include_str!("../fixtures/agent_hook_payload/claude-ask-multiselect-2.1.316.jsonl"),
        "AskUserQuestion",
    )
}

fn screen_option(key: &str, label: &str) -> ChoiceOption {
    ChoiceOption {
        key: key.into(),
        label: label.into(),
        highlighted: key == "1",
        destructive: false,
        hint: None,
    }
}

fn screen_prompt(title: &str, labels: &[&str]) -> ChoicePromptPayload {
    ChoicePromptPayload {
        title: title.into(),
        options: labels
            .iter()
            .enumerate()
            .map(|(i, label)| screen_option(&(i + 1).to_string(), label))
            .collect(),
        selection_mode: None,
        dismiss_key: Some("cancel".into()),
        amend_key: None,
        header: None,
        question_index: None,
        question_count: None,
    }
}

// Catches: the card drops the per-question header or the multiSelect flag of
// a recorded 2.1.316 payload, so a client shows a single-choice dialog.
#[test]
fn recorded_claude_payload_keeps_every_question() {
    let (id, input) = claude_ask();
    let card = claude_card("AskUserQuestion", &id, &input).expect("card");
    assert_eq!(card.request_id, id);
    assert_eq!(card.kind, CardKind::Questions);
    let shape: Vec<_> = card
        .questions
        .iter()
        .map(|q| {
            (
                q.header.as_deref(),
                q.question.as_str(),
                q.multi_select,
                q.options.len(),
            )
        })
        .collect();
    assert_eq!(
        shape,
        [
            (Some("Color"), "Which color should the probe use?", false, 2),
            (Some("Sizes"), "Which sizes should the probe use?", true, 2),
        ]
    );
    assert_eq!(
        card.questions[1].options[0].description.as_deref(),
        Some("Use small")
    );
}

// Catches: a plan approval card invents plan text that Claude 2.1.316 never
// sends (`tool_input` is `{}`), or a plan payload is dropped.
#[test]
fn recorded_plan_payload_is_an_approval_without_text() {
    let (id, input) = recorded_input(
        include_str!("../fixtures/agent_hook_payload/claude-plan-approve-2.1.316.jsonl"),
        "ExitPlanMode",
    );
    let card = claude_card("ExitPlanMode", &id, &input).expect("card");
    assert_eq!(card.kind, CardKind::PlanApproval);
    assert_eq!(card.questions.len(), 0);
    assert_eq!(card.text(), "");
}

// Catches: any tool whose input happens to carry `questions` becomes a human
// question card.
#[test]
fn arbitrary_tool_name_gives_no_card() {
    let (id, input) = claude_ask();
    assert!(claude_card("Bash", &id, &input).is_none());
    assert!(claude_card("mcp__x__AskUserQuestion", &id, &input).is_none());
}

// Catches: an over-cap or malformed payload yields a partial card that names
// the wrong option, instead of no card.
#[test]
fn capped_and_malformed_payloads_give_no_card() {
    let (id, input) = claude_ask();
    let with = |edit: &dyn Fn(&mut Value)| {
        let mut input = input.clone();
        edit(&mut input);
        claude_card("AskUserQuestion", &id, &input)
    };
    let many: Vec<Value> = (0..=MAX_OPTIONS)
        .map(|i| json!({"label": format!("o{i}")}))
        .collect();
    assert!(with(&|i| i["questions"][0]["options"] = json!(many)).is_none());
    let question = input["questions"][0].clone();
    assert!(with(&|i| i["questions"] = json!(vec![question.clone(); MAX_QUESTIONS + 1])).is_none());
    assert!(with(&|i| i["questions"] = json!([])).is_none());
    assert!(with(&|i| i["questions"][1]["options"][0]["label"] = json!(7)).is_none());
    assert!(with(&|i| i["questions"][1]["multiSelect"] = json!("yes")).is_none());
    assert!(with(&|i| i["questions"][0]["question"] = json!("   ")).is_none());
    assert!(
        with(&|i| {
            i["questions"][0].as_object_mut().unwrap().remove("options");
        })
        .is_none()
    );
    assert!(claude_card("AskUserQuestion", "", &input).is_none());
    assert!(claude_card("AskUserQuestion", &"x".repeat(257), &input).is_none());

    let long =
        with(&|i| i["questions"][0]["options"][0]["label"] = json!("é".repeat(400))).unwrap();
    assert_eq!(
        long.questions[0].options[0].label.chars().count(),
        MAX_LABEL_CHARS
    );
}

// Catches: a recorded Codex 0.166.0 `request_user_input` loses its options, or
// a question without options is not offered as a typed answer.
#[test]
fn recorded_codex_payload_and_typed_question() {
    let (id, input) = recorded_input(
        include_str!("../fixtures/codex_tools_2626/hooks.jsonl"),
        "request_user_input",
    );
    let card = codex_card(&id, &input).expect("card");
    assert_eq!(card.text(), "Pick a color");
    assert_eq!(card.questions[0].header.as_deref(), Some("Choice"));
    let labels: Vec<_> = card.questions[0]
        .options
        .iter()
        .map(|o| o.label.as_str())
        .collect();
    assert_eq!(labels, ["Blue", "Green"]);
    assert!(!card.questions[0].free_text);

    let typed = json!({"questions": [{"id": "why", "question": "Why?"}]});
    let card = codex_card("call-1", &typed).expect("typed card");
    assert!(card.questions[0].free_text && card.questions[0].options.is_empty());
}

// Catches: the ego permission modal, or an unknown modal kind, becomes a
// question card; or the recorded ego 0.4.3 question loses its options.
#[test]
fn recorded_ego_modal_and_permission() {
    let data = json!({"kind": "question", "question": {"prompt": "Choose readiness", "mode": "single",
        "options": [{"id": "ready", "label": "Ready"}, {"id": "wait", "label": "Wait"}], "free_text": false}});
    let card = ego_card("01a19726-8a84-7331-a08f-596cdde18f79", &data).expect("card");
    assert_eq!(card.text(), "Choose readiness");
    assert!(!card.questions[0].multi_select && !card.questions[0].free_text);
    assert_eq!(card.questions[0].options.len(), 2);

    let mut multiple = data.clone();
    multiple["question"]["mode"] = json!("multiple");
    assert!(ego_card("m", &multiple).unwrap().questions[0].multi_select);
    assert!(ego_card("p", &json!({"kind": "permission", "label": "exec ls"})).is_none());
    let mut unknown = data;
    unknown["question"]["mode"] = json!("dance");
    assert!(ego_card("u", &unknown).is_none());
}

// Catches: a multi-question dialog repaint names the first question (or no
// position) while the second is on screen, or the card rewrites native keys.
#[test]
fn repaint_of_second_question_takes_its_header_and_position() {
    let (id, input) = claude_ask();
    let card = claude_card("AskUserQuestion", &id, &input).unwrap();
    let mut prompt = screen_prompt(
        "Which sizes should the probe use?",
        &["Small", "Large", "Type something."],
    );
    let keys: Vec<_> = prompt.options.iter().map(|o| o.key.clone()).collect();
    enrich_choice_prompt(&card, &mut prompt);
    assert_eq!(prompt.header.as_deref(), Some("Sizes"));
    assert_eq!(
        (prompt.question_index, prompt.question_count),
        (Some(2), Some(2))
    );
    assert_eq!(
        prompt
            .options
            .iter()
            .map(|o| o.key.clone())
            .collect::<Vec<_>>(),
        keys
    );
    assert_eq!(prompt.options[0].hint.as_deref(), Some("Use small"));
    assert_eq!(
        prompt.options[2].hint, None,
        "a screen-only option gains nothing"
    );

    // The recorded multi-select row draws a checkbox before the label.
    let mut checkbox = screen_prompt("Sizes", &["[ ] Small", "[✔] Large"]);
    enrich_choice_prompt(&card, &mut checkbox);
    assert_eq!(checkbox.question_index, Some(2));
    assert_eq!(checkbox.options[1].hint.as_deref(), Some("Use large"));

    // A screen title that is not a payload question: the labels decide.
    let mut by_labels = screen_prompt("Color", &["Red", "Blue"]);
    enrich_choice_prompt(&card, &mut by_labels);
    assert_eq!(by_labels.question_index, Some(1));
}

// Catches: a screen dialog that matches no payload question (or two equally)
// takes a header from the wrong question.
#[test]
fn unmatched_dialog_is_left_alone() {
    let (id, input) = claude_ask();
    let card = claude_card("AskUserQuestion", &id, &input).unwrap();
    let mut prompt = screen_prompt("Do you want to proceed?", &["Yes", "No"]);
    let before = prompt.clone();
    enrich_choice_prompt(&card, &mut prompt);
    assert_eq!(prompt, before);
}

fn awaiting_snapshot(request: &str, card: QuestionCard) -> crate::state::SessionState {
    crate::state::SessionState {
        awaiting_input: true,
        question_text: Some("screen text".into()),
        wait_request: Some(request.into()),
        wait_card: Some(std::sync::Arc::new(card)),
        ..Default::default()
    }
}

// Catches: two requests with the same question text share one card, so the
// card of a settled request names the next wait.
#[test]
fn card_of_another_request_with_same_text_is_not_shown() {
    let (id, input) = claude_ask();
    let card = claude_card("AskUserQuestion", &id, &input).unwrap();
    let mut other = awaiting_snapshot("toolu_other", card.clone());
    apply(&mut other);
    assert!(other.question_card.is_none());
    assert_eq!(other.question_text.as_deref(), Some("screen text"));

    let mut same = awaiting_snapshot(&id, card.clone());
    apply(&mut same);
    assert_eq!(same.question_card.as_ref(), Some(&card));
    assert_eq!(
        same.question_text,
        Some(card.text()),
        "the structured text wins"
    );

    let mut settled = awaiting_snapshot(&id, card);
    settled.awaiting_input = false;
    apply(&mut settled);
    assert!(
        settled.question_card.is_none(),
        "no card without a live wait"
    );
}

// Catches: the snapshot field name or shape drifts from the client contract.
#[test]
fn card_wire_shape() {
    let (id, input) = claude_ask();
    let card = claude_card("AskUserQuestion", &id, &input).unwrap();
    let wire = serde_json::to_value(&card).unwrap();
    assert_eq!(wire["kind"], "questions");
    assert_eq!(wire["questions"][1]["multi_select"], true);
    assert_eq!(wire["questions"][1]["free_text"], true);
    assert_eq!(
        wire["questions"][0]["options"][1]["description"],
        "Use blue"
    );
    let back: QuestionCard = serde_json::from_value(wire).unwrap();
    assert_eq!(back, card);
}
