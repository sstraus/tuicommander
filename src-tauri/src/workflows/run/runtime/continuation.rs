//! Answer delivery is a durable, generation-bound handoff, not node completion.
use super::super::{ContinuationDelivery, NodeAttempt};
use super::*;

pub(super) async fn deliver(
    state: &Arc<AppState>,
    store: &RunStore,
    run: &RunSnapshot,
    attempt: &NodeAttempt,
) -> Result<bool, String> {
    let Some(continuation) = &attempt.continuation else {
        return Ok(false);
    };
    if continuation.delivery == ContinuationDelivery::Delivered {
        return Ok(false);
    }
    let binding = attempt
        .agent
        .as_ref()
        .ok_or("continuation binding missing")?;
    let reference = format!("workflow-input:{}:{}", attempt.id, continuation.generation);
    if continuation.delivery == ContinuationDelivery::HandoffStarted {
        // Only the exact retained inbox entry proves a PTY handoff. An absent
        // entry after restart could have been consumed; never send it twice.
        if binding.acp.is_some()
            || !state
                .agent_inbox
                .get(&binding.session_id)
                .is_some_and(|mail| mail.iter().any(|m| m.id == reference))
        {
            return Err("continuation handoff outcome unknown; inspect the retained launch/conversation, do not resend".into());
        }
        return mark(
            store,
            run,
            attempt,
            ContinuationDelivery::Delivered,
            reference,
        );
    }
    if let Some(acp) = &binding.acp {
        let snapshot = state
            .acp
            .snapshot(acp.connection_id)
            .map_err(|_| "continuation remains pending: ACP inventory unknown")?;
        if snapshot.generation != acp.generation
            || snapshot.settlement.is_some()
            || !snapshot
                .attachments
                .iter()
                .any(|a| a.session_id.to_string() == acp.conversation_id && a.is_quiescent())
        {
            return Err(
                "continuation remains pending: matching ACP conversation is not idle".into(),
            );
        }
    } else if !state
        .session_maps
        .sessions
        .contains_key(&binding.session_id)
        || !state.peer_agents.contains_key(&binding.session_id)
    {
        return Err("continuation remains pending: managed worker inventory unknown".into());
    }
    let prompt = serde_json::json!({
        "type":"workflow_input_answer", "runId":run.id, "storyId":attempt.story_id,
        "attemptId":attempt.id, "generation":continuation.generation, "answer":continuation.answer,
        "instruction":"Continue the bound work with this answer. Report with workflow action=report using this generation; answering input does not complete the attempt."
    }).to_string();
    mark(
        store,
        run,
        attempt,
        ContinuationDelivery::HandoffStarted,
        reference.clone(),
    )?;
    let receipt = if let Some(acp) = &binding.acp {
        use agent_client_protocol::schema::v1;
        let turn = state
            .acp
            .prompt(
                acp.connection_id,
                acp.conversation_id.clone().into(),
                vec![v1::ContentBlock::Text(v1::TextContent::new(prompt.clone()))],
            )
            .await
            .map_err(|e| e.message)?;
        if !state.acp.has_sent_prompt(acp.connection_id, turn, &prompt) {
            return Err("continuation handoff unobserved; payload was not resent".into());
        }
        format!(
            "acp:{}",
            serde_json::to_string(&turn).map_err(|e| e.to_string())?
        )
    } else {
        let timestamp = super::super::store::now_ms().max(0) as u64;
        let handed = crate::mcp_http::mcp_transport::queue_peer_notice(
            state,
            &binding.session_id,
            crate::state::AgentMessage {
                id: reference.clone(),
                from_tuic_session: format!("workflow:{}", run.id),
                from_name: "workflow".into(),
                content: prompt,
                timestamp,
                delivered_via_channel: false,
            },
        );
        if !handed {
            return Err(
                "continuation handoff outcome unknown; retained answer was not resent".into(),
            );
        }
        reference
    };
    let current = store.snapshot(&run.id)?;
    mark(
        store,
        &current,
        attempt,
        ContinuationDelivery::Delivered,
        receipt,
    )
}

fn mark(
    store: &RunStore,
    run: &RunSnapshot,
    attempt: &NodeAttempt,
    delivery: ContinuationDelivery,
    reference: String,
) -> Result<bool, String> {
    store.command_expected(
        &run.id,
        &format!(
            "continuation:{}:{}:{delivery:?}",
            attempt.id, attempt.generation
        ),
        run.sequence,
        RunCommand::ContinuationDeliveryChanged {
            attempt_id: attempt.id.clone(),
            generation: attempt.generation,
            delivery,
            reference: Some(reference),
        },
    )?;
    Ok(true)
}
