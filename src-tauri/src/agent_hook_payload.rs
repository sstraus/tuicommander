//! Native question payloads, normalized into one card for every client.
//!
//! A native owner (the Claude hook receipts, the Codex hook receipts, the ego
//! native `agent-event` frames) holds the human wait; this module only reads
//! the payload that came with it. A card never sets, keeps or clears a wait:
//! the snapshot shows it only while `awaiting_input` is true and the card
//! belongs to the request that the owner holds open (`wait_request`).
//!
//! Only these payloads name a human question:
//!
//! | Agent | Payload | Recorded |
//! |---|---|---|
//! | Claude | `AskUserQuestion` / `ExitPlanMode` PreToolUse `tool_input` | 2.1.316 |
//! | Codex | `request_user_input` PreToolUse `tool_input` | 0.166.0 |
//! | ego | `agent-event` `modal_open` with `data.kind = "question"` | 0.4.3 |
//!
//! Claude 2.1.316 sends `ExitPlanMode` with an empty `tool_input`: the plan is
//! in a plan file that the payload does not name, so a plan approval card has
//! no plan text. A payload that does not match its recorded shape, or that is
//! over a cap, gives no card at all; a partial card could name the wrong
//! option.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::output_parser::ChoicePromptPayload;

/// Questions kept at most. Claude allows four; ego rounds are small.
const MAX_QUESTIONS: usize = 8;
/// Options kept at most per question.
const MAX_OPTIONS: usize = 16;
/// Characters kept of a header, an option label or an option description.
const MAX_LABEL_CHARS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CardKind {
    Questions,
    PlanApproval,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CardOption {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CardQuestion {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    pub question: String,
    pub options: Vec<CardOption>,
    pub multi_select: bool,
    /// The native dialog also accepts typed text.
    pub free_text: bool,
}

/// One normalized question or plan approval of one native request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct QuestionCard {
    /// The native request id: a tool call id or an ego question item.
    pub request_id: String,
    pub kind: CardKind,
    pub questions: Vec<CardQuestion>,
}

impl QuestionCard {
    /// The question text, joined like the Codex hook text.
    pub(crate) fn text(&self) -> String {
        self.questions
            .iter()
            .map(|q| q.question.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

fn label(value: &Value) -> Option<String> {
    let text = crate::chat_view::clean_question(value.as_str()?);
    let text: String = text.chars().take(MAX_LABEL_CHARS).collect();
    (!text.trim().is_empty()).then_some(text)
}

/// A label that is present but not a usable string.
struct InvalidLabel;

/// `Ok(None)` when absent or null; `Err` when present but not a usable string.
fn optional_label(value: Option<&Value>) -> Result<Option<String>, InvalidLabel> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => label(value).map(Some).ok_or(InvalidLabel),
    }
}

fn options(value: &Value, description_key: &str) -> Option<Vec<CardOption>> {
    let options = value.as_array()?;
    if options.len() > MAX_OPTIONS {
        return None;
    }
    options
        .iter()
        .map(|option| {
            Some(CardOption {
                label: label(option.get("label")?)?,
                description: optional_label(option.get(description_key)).ok()?,
            })
        })
        .collect()
}

fn question_text(value: &Value) -> Option<String> {
    let text = crate::chat_view::clean_question(value.as_str()?);
    (!text.trim().is_empty()).then_some(text)
}

/// The `questions` array that Claude `AskUserQuestion` and Codex
/// `request_user_input` share. Claude always offers a typed answer; a Codex
/// question without options is a typed answer.
fn shared_questions(input: &Value, claude: bool) -> Option<Vec<CardQuestion>> {
    let questions = input.get("questions")?.as_array()?;
    if questions.is_empty() || questions.len() > MAX_QUESTIONS {
        return None;
    }
    questions
        .iter()
        .map(|q| {
            let options = match q.get("options") {
                None | Some(Value::Null) if !claude => Vec::new(),
                Some(value) => options(value, "description")?,
                None => return None,
            };
            let multi_select = match q.get("multiSelect") {
                None | Some(Value::Null) => false,
                Some(value) => value.as_bool()?,
            };
            Some(CardQuestion {
                header: optional_label(q.get("header")).ok()?,
                question: question_text(q.get("question")?)?,
                free_text: claude || options.is_empty(),
                options,
                multi_select,
            })
        })
        .collect()
}

fn checked_id(id: &str) -> Option<String> {
    (!id.is_empty() && id.len() <= 256).then(|| id.to_owned())
}

/// A Claude PreToolUse payload. Only the two human-wait tools give a card.
pub(crate) fn claude_card(tool: &str, id: &str, input: &Value) -> Option<QuestionCard> {
    let (kind, questions) = match tool {
        "AskUserQuestion" => (CardKind::Questions, shared_questions(input, true)?),
        "ExitPlanMode" => (CardKind::PlanApproval, Vec::new()),
        _ => return None,
    };
    Some(QuestionCard {
        request_id: checked_id(id)?,
        kind,
        questions,
    })
}

/// A Codex PreToolUse payload. Only `request_user_input` gives a card.
pub(crate) fn codex_card(id: &str, input: &Value) -> Option<QuestionCard> {
    Some(QuestionCard {
        request_id: checked_id(id)?,
        kind: CardKind::Questions,
        questions: shared_questions(input, false)?,
    })
}

/// The `data` of an ego `modal_open` frame. A permission modal gives no card.
pub(crate) fn ego_card(id: &str, data: &Value) -> Option<QuestionCard> {
    if data.get("kind")?.as_str()? != "question" {
        return None;
    }
    let question = data.get("question")?;
    let prompt = question_text(question.get("prompt")?)?;
    let free_text = question.get("free_text")?.as_bool()?;
    let options = options(question.get("options")?, "description")?;
    let mode = question.get("mode")?;
    let questions = match mode.as_str() {
        Some("single") => vec![(prompt, options, false, free_text)],
        Some("multiple") => vec![(prompt, options, true, free_text)],
        Some("free_text") => vec![(prompt, options, false, true)],
        _ => return None,
    };
    Some(QuestionCard {
        request_id: checked_id(id)?,
        kind: CardKind::Questions,
        questions: questions
            .into_iter()
            .map(
                |(question, options, multi_select, free_text)| CardQuestion {
                    header: None,
                    question,
                    options,
                    multi_select,
                    free_text,
                },
            )
            .collect(),
    })
}

fn same_text(a: &str, b: &str) -> bool {
    a.split_whitespace().eq(b.split_whitespace())
}

/// A multi-select row draws its checkbox before the label (`[ ] Small`,
/// recorded 2.1.316); the payload label has none.
fn same_label(payload: &str, screen: &str) -> bool {
    let screen = screen.trim_start();
    let screen = match screen
        .strip_prefix('[')
        .and_then(|rest| rest.split_once(']'))
    {
        Some((mark, rest)) if mark.chars().count() <= 1 => rest,
        _ => screen.trim_start_matches(['☐', '☒', '✓', '✔']),
    };
    same_text(payload, screen)
}

/// Add the card to the native screen dialog of the same wait. The native keys,
/// labels and order stay as the screen drew them; the card adds only the
/// current question's header and position and the description of an option
/// whose screen label equals the payload label.
pub(crate) fn enrich_choice_prompt(card: &QuestionCard, prompt: &mut ChoicePromptPayload) {
    let labels = |q: &CardQuestion| {
        prompt
            .options
            .iter()
            .filter(|option| {
                q.options
                    .iter()
                    .any(|o| same_label(&o.label, &option.label))
            })
            .count()
    };
    let by_title = card
        .questions
        .iter()
        .position(|q| same_text(&q.question, &prompt.title));
    // A multi-question dialog repaints each question; the screen title can be
    // the header instead of the question, so fall back to the one question
    // whose labels the screen shows.
    let index = by_title.or_else(|| {
        let counts: Vec<usize> = card.questions.iter().map(labels).collect();
        let max = *counts.iter().max()?;
        let mut best = (0..counts.len()).filter(|&i| counts[i] == max);
        let first = best.next()?;
        (max > 0 && best.next().is_none()).then_some(first)
    });
    let Some(index) = index else {
        return;
    };
    let question = &card.questions[index];
    prompt.header = question.header.clone();
    if card.questions.len() > 1 {
        prompt.question_index = Some(index as u32 + 1);
        prompt.question_count = Some(card.questions.len() as u32);
    }
    for option in &mut prompt.options {
        if option.hint.is_some() {
            continue;
        }
        option.hint = question
            .options
            .iter()
            .find(|o| same_label(&o.label, &option.label))
            .and_then(|o| o.description.clone());
    }
}

/// Put the card of the live wait into a client snapshot. Only a card of the
/// request that `awaiting_input` belongs to is shown: a card of a settled or
/// a different request never names the current wait. The structured question
/// text wins over transcript and screen text (`agent_question`); a native
/// screen dialog keeps its own title and keys and gains the card's header,
/// position and descriptions.
pub(crate) fn apply(snapshot: &mut crate::state::SessionState) {
    let Some(card) = snapshot.wait_card.clone().filter(|card| {
        snapshot.awaiting_input
            && snapshot.wait_request.as_deref() == Some(card.request_id.as_str())
    }) else {
        return;
    };
    match snapshot.choice_prompt.as_mut() {
        Some(prompt) => enrich_choice_prompt(&card, prompt),
        None => {
            let text = card.text();
            if !text.is_empty() {
                snapshot.question_text = Some(text);
            }
        }
    }
    snapshot.question_card = Some((*card).clone());
}

#[cfg(test)]
mod tests;
