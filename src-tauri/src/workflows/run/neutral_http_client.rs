//! The existing installed HTTP client owns authentication and destination checks.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "009 HTTP awaits host composition in 008")
)]

use super::neutral_effects::{EffectObservation, EffectOutcome, NeutralEffect};
use super::neutral_execution::NeutralResult;
use crate::workflows::connector::{ConnectorContract, Reconciliation};
use crate::workflows::http_operation::HttpOperation;
use serde_json::Value;
use std::{collections::BTreeSet, sync::Arc};

pub(super) async fn execute(
    state: Arc<crate::AppState>,
    effect: NeutralEffect,
    op: HttpOperation,
    contract: ConnectorContract,
    plugin: String,
    base: String,
) -> Result<EffectObservation, String> {
    let result = execute_request(state, &effect, op, contract, plugin, base).await;
    settle_result(&effect, result)
}

/// One settlement policy for installed HTTP-backed activity clients.
pub(super) fn settle_result(
    effect: &NeutralEffect,
    result: Result<EffectObservation, String>,
) -> Result<EffectObservation, String> {
    match result {
        Err(diagnostic) if !effect.preparation.mutation => Ok(EffectObservation {
            stopped: true,
            outcome: EffectOutcome::Completed {
                result: NeutralResult::Failed { diagnostic },
            },
        }),
        result => result,
    }
}

async fn execute_request(
    state: Arc<crate::AppState>,
    effect: &NeutralEffect,
    op: HttpOperation,
    contract: ConnectorContract,
    plugin: String,
    base: String,
) -> Result<EffectObservation, String> {
    let mut resolved = op.resolve(&base, &effect.invocation.inputs)?;
    if let Some(callback) = &contract.callback {
        resolved
            .headers
            .insert(callback.header.clone(), effect.key.digest.clone());
    }
    let original = resolved.url.clone();
    let mut seen = BTreeSet::new();
    let mut items = Vec::new();
    let mut bytes = 0usize;
    let pages = contract.pagination.as_ref().map_or(1, |p| p.max_pages);
    for page in 0..pages {
        let reply = crate::plugin_http::plugin_http_fetch_impl(
            &state,
            resolved.url.to_string(),
            Some(op.method.clone()),
            Some(resolved.headers.clone().into_iter().collect()),
            resolved.body.clone(),
            super::neutral_http::credential(&contract),
            plugin.clone(),
        )
        .await?;
        if reply.status == 202 {
            return Ok(EffectObservation {
                stopped: false,
                outcome: if contract.reconciliation == Reconciliation::OriginalEffectCallback {
                    EffectOutcome::Pending {
                        handle: effect.key.digest.clone(),
                    }
                } else {
                    EffectOutcome::Uncertain {
                        diagnostic:
                            "HTTP accepted asynchronously without a reconciliation contract".into(),
                    }
                },
            });
        }
        if reply.status == 429 {
            let hint = reply
                .headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("retry-after"))
                .and_then(|(_, value)| value.parse::<u64>().ok());
            return Ok(EffectObservation {
                stopped: !effect.preparation.mutation,
                outcome: if effect.preparation.mutation {
                    EffectOutcome::Uncertain {
                        diagnostic: format!(
                            "HTTP rate limited; retry_after_seconds={hint:?}; write not repeated"
                        ),
                    }
                } else {
                    EffectOutcome::Completed {
                        result: NeutralResult::Failed {
                            diagnostic: format!(
                                "HTTP rate limited; retry_after_seconds={hint:?}; no automatic retry"
                            ),
                        },
                    }
                },
            });
        }
        if !(200..300).contains(&reply.status) {
            // A write can commit before an error response. Do not release its
            // ownership on a generic HTTP status without service evidence.
            return Err(format!(
                "HTTP operation returned status {}; effect requires reconciliation",
                reply.status
            ));
        }
        bytes = bytes
            .checked_add(reply.body.len())
            .ok_or("HTTP response byte overflow")?;
        if bytes > op.max_response_bytes {
            return Err("HTTP cumulative response byte bound exceeded".into());
        }
        let json: Value = if reply.body.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&reply.body).map_err(|_| "HTTP response is not JSON")?
        };
        let mut next = None;
        if let Some(p) = &contract.pagination {
            let page_items = json
                .pointer(&p.items_pointer)
                .and_then(Value::as_array)
                .ok_or("HTTP pagination items are not an array")?;
            if items.len().saturating_add(page_items.len()) > p.max_items as usize {
                return Err("HTTP pagination exceeds max-items".into());
            }
            items.extend(page_items.iter().cloned());
            next = match json.pointer(&p.cursor_pointer) {
                None | Some(Value::Null) => None,
                Some(Value::String(value)) if value.is_empty() => None,
                Some(Value::String(value)) if value.len() <= 4096 => Some(value.clone()),
                _ => return Err("HTTP pagination cursor has invalid type or size".into()),
            };
            if let Some(cursor) = &next {
                if page + 1 >= pages {
                    return Err("HTTP pagination exceeds max-pages".into());
                }
                if !seen.insert(cursor.clone()) {
                    return Err("HTTP pagination repeated a cursor".into());
                }
                resolved.url = original.clone();
                resolved
                    .url
                    .query_pairs_mut()
                    .append_pair(&p.query_parameter, cursor);
            }
        }
        if next.is_none() {
            let collection = contract
                .pagination
                .as_ref()
                .map(|p| (p.output.as_str(), items));
            let outputs = op.outputs(&json, &effect.invocation.descriptor.outputs, collection)?;
            return Ok(super::neutral_http::completed(outputs));
        }
    }
    Err("HTTP pagination did not terminate".into())
}
