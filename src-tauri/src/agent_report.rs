//! Typed peer reports. Text mail remains independent of this contract.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportState {
    Ready,
    Waiting,
    Blocked,
    Failed,
    Dropped,
    Progress,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitFor {
    Boss,
    Review,
    Dependency,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionOption {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportQuestion {
    pub id: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<QuestionOption>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended_option_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentReport {
    pub state: ReportState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_for: Option<WaitFor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<ReportQuestion>,
}

pub(crate) fn parse(args: &Value) -> Result<Option<AgentReport>, String> {
    let Some(value) = args.get("report") else {
        return Ok(None);
    };
    if serde_json::to_vec(value).map_err(|e| e.to_string())?.len()
        + args["message"].as_str().map(str::len).unwrap_or(0)
        > 65536
    {
        return Err("message plus report exceeds 64 KB".into());
    }
    let report: AgentReport =
        serde_json::from_value(value.clone()).map_err(|e| format!("Invalid report: {e}"))?;
    if (report.state == ReportState::Waiting) != report.wait_for.is_some() {
        return Err("report.wait_for is required only for waiting".into());
    }
    if let Some(question) = &report.question {
        if report.wait_for != Some(WaitFor::Boss) {
            return Err("report.question requires waiting on boss".into());
        }
        if question.id.trim().is_empty() || question.text.trim().is_empty() {
            return Err("report.question requires non-empty id and text".into());
        }
        let mut ids = std::collections::HashSet::new();
        if let Some(options) = &question.options {
            if options.is_empty() {
                return Err("question.options must not be empty".into());
            }
            for option in options {
                if option.id.trim().is_empty()
                    || option.label.trim().is_empty()
                    || !ids.insert(option.id.as_str())
                {
                    return Err("question options require unique non-empty ids and labels".into());
                }
            }
        }
        if question
            .recommended_option_id
            .as_deref()
            .is_some_and(|id| !ids.contains(id))
        {
            return Err("recommended_option_id must name an option".into());
        }
    }
    Ok(Some(report))
}

pub(crate) fn schema() -> Value {
    serde_json::json!({"type":"object","additionalProperties":false,"required":["state"],
        "description":"Optional typed report on send. message remains required. waiting requires wait_for. A question is allowed only when waiting on boss. ready means waiting_on_review; progress alone never changes stage.",
        "properties":{
            "state":{"type":"string","enum":["ready","waiting","blocked","failed","dropped","progress"]},
            "wait_for":{"type":"string","enum":["boss","review","dependency"]},
            "question":{"type":"object","additionalProperties":false,"required":["id","text"],"properties":{
                "id":{"type":"string","minLength":1},"text":{"type":"string","minLength":1},
                "options":{"type":"array","minItems":1,"items":{"type":"object","additionalProperties":false,
                    "required":["id","label"],"properties":{"id":{"type":"string","minLength":1},"label":{"type":"string","minLength":1}}}},
                "recommended_option_id":{"type":"string"}}}}})
}
