//! Reuses the installed plugin HTTP/account client, never a separate credential store.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "012 awaits native host composition")
)]
use super::neutral_effects::{EffectObservation, EffectOutcome, NeutralEffect};
use super::neutral_execution::NeutralResult;
use crate::workflows::gitlab::{GitLabInstall, Operation, project_response};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

pub(super) async fn execute(
    state: Arc<crate::AppState>,
    install: &GitLabInstall,
    op: Operation,
    effect: &NeutralEffect,
) -> Result<EffectObservation, String> {
    let result = execute_request(state, install, op, effect).await;
    super::neutral_http_client::settle_result(effect, result)
}

async fn execute_request(
    state: Arc<crate::AppState>,
    install: &GitLabInstall,
    op: Operation,
    effect: &NeutralEffect,
) -> Result<EffectObservation, String> {
    let inputs = &effect.invocation.inputs;
    let (url, method, body) = install.request(op, inputs, &effect.key.digest)?;
    if matches!(op, Operation::ReviewNote | Operation::Pipeline) {
        let mr = inputs
            .get("mr")
            .and_then(Value::as_u64)
            .ok_or("GitLab MR missing")?;
        let current_url = format!(
            "{}projects/{}/merge_requests/{mr}",
            install.base, install.project
        );
        let current = fetch(&state, install, &current_url, "GET", None).await?;
        let Some(raw) = decode(current, false)? else {
            return Err("GitLab preflight unavailable".into());
        };
        project_response(
            install,
            Operation::MergeRequest,
            &BTreeMap::from([("mr".into(), json!(mr))]),
            &raw,
            &effect.key.digest,
        )?;
        let stale = raw.get("sha") != inputs.get("head")
            || raw.pointer("/diff_refs/base_sha") != inputs.get("base")
            || raw.pointer("/diff_refs/head_sha") != inputs.get("head");
        if stale {
            return result(
                effect,
                "stale",
                json!({"project":install.project,"mr":mr,"head":inputs.get("head"),"base":inputs.get("base")}),
            );
        }
        if op == Operation::Pipeline && raw.pointer("/head_pipeline/id") != inputs.get("pipeline") {
            return result(
                effect,
                "stale",
                json!({"reason":"pipeline is not the selected MR head pipeline"}),
            );
        }
    }
    let reply = fetch(&state, install, &url, method, body).await?;
    if reply.status == 429 {
        let hint = reply
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("retry-after"))
            .map(|(_, v)| v.clone());
        return if op.mutation() {
            Ok(EffectObservation {
                stopped: false,
                outcome: EffectOutcome::Uncertain {
                    diagnostic: format!(
                        "GitLab rate limited; retry_after={hint:?}; no write repeated"
                    ),
                },
            })
        } else {
            result(effect, "pending", json!({"retry_after":hint}))
        };
    }
    let raw = decode(reply, op.mutation())?.ok_or("GitLab response missing")?;
    let (outcome, receipt) = project_response(install, op, inputs, &raw, &effect.key.digest)?;
    result(effect, outcome, receipt)
}
async fn fetch(
    state: &Arc<crate::AppState>,
    install: &GitLabInstall,
    url: &str,
    method: &str,
    body: Option<String>,
) -> Result<crate::plugin_http::HttpResponse, String> {
    crate::plugin_http::plugin_http_fetch_impl(
        state,
        url.into(),
        Some(method.into()),
        Some(HashMap::from([
            ("Accept".into(), "application/json".into()),
            ("Content-Type".into(), "application/json".into()),
        ])),
        body,
        super::neutral_http::credential(&install.connector),
        "gitlab".into(),
    )
    .await
}
fn decode(
    reply: crate::plugin_http::HttpResponse,
    mutation: bool,
) -> Result<Option<Value>, String> {
    if reply.status == 202 || !(200..300).contains(&reply.status) {
        return Err(format!(
            "GitLab status {}; mutation={mutation}; retain original effect for reconciliation",
            reply.status
        ));
    }
    if reply.body.len() > 10 * 1024 * 1024 {
        return Err("GitLab response byte bound exceeded".into());
    }
    serde_json::from_str(&reply.body)
        .map(Some)
        .map_err(|_| "GitLab response is not JSON".into())
}
fn result(
    effect: &NeutralEffect,
    outcome: &str,
    receipt: Value,
) -> Result<EffectObservation, String> {
    let mut outputs = BTreeMap::from([("receipt".into(), receipt.clone())]);
    for name in effect.invocation.descriptor.outputs.fields.keys() {
        if let Some(value) = receipt.get(name) {
            outputs.insert(name.clone(), value.clone());
        }
    }
    Ok(EffectObservation {
        stopped: true,
        outcome: EffectOutcome::Completed {
            result: NeutralResult::Completed {
                outcome: outcome.into(),
                outputs,
            },
        },
    })
}
