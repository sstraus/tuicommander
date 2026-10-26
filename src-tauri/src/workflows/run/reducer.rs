use super::model::*;

/// Pure projection of a committed event. Replaying the event stream must rebuild
/// the exact snapshot stored by the writer transaction.
pub fn apply_event(previous: Option<RunSnapshot>, event: &RunEvent) -> Result<RunSnapshot, String> {
    if let RunEventKind::Started { initial } = &event.kind {
        if previous.is_some()
            || event.sequence != 1
            || initial.sequence != 0
            || initial.event_contract_version > super::graph::RUN_EVENT_CONTRACT_VERSION
            || !initial.graph_executions.is_empty()
        {
            return Err("invalid workflow start event".into());
        }
        let mut snapshot = (**initial).clone();
        snapshot.sequence = event.sequence;
        return Ok(snapshot);
    }
    let mut snapshot = previous.ok_or("workflow event has no start")?;
    if event.sequence != snapshot.sequence + 1 {
        return Err("workflow event sequence gap".into());
    }
    if matches!(snapshot.status, RunStatus::Completed | RunStatus::Cancelled)
        && !matches!(&event.kind, RunEventKind::CanonicalRecertified { .. })
    {
        return Err("terminal workflow cannot advance".into());
    }
    if matches!(
        event.kind,
        RunEventKind::Graph { .. } | RunEventKind::GraphResumed { .. }
    ) && snapshot.event_contract_version != super::graph::RUN_EVENT_CONTRACT_VERSION
    {
        return Err("unsupported workflow graph event contract".into());
    }
    let previous_status = snapshot.status;
    match &event.kind {
        RunEventKind::Started { .. } => unreachable!(),
        RunEventKind::Graph { event } => match event {
            super::graph::GraphEvent::Started { execution } => {
                if snapshot.graph_executions.iter().any(|existing| {
                    existing.id == execution.id || existing.target_id == execution.target_id
                }) {
                    return Err("graph target already has an execution".into());
                }
                let (id, revision) = if execution.target_id == snapshot.plan_id {
                    (&snapshot.definition_id, snapshot.definition_revision)
                } else {
                    (
                        &snapshot.story_definition_id,
                        snapshot.story_definition_revision,
                    )
                };
                if execution.definition.id != *id
                    || execution.definition.revision != revision
                    || execution.definition.project != snapshot.project
                {
                    return Err("graph execution has a foreign definition".into());
                }
                let initial = super::graph::GraphExecution::pristine(
                    execution.id.clone(),
                    execution.target_id.clone(),
                    execution.definition.clone(),
                )?;
                if **execution != initial {
                    return Err("graph start state is not pristine".into());
                }
                snapshot.graph_executions.push(initial);
            }
            super::graph::GraphEvent::Transition { transition } => {
                if matches!(
                    transition,
                    super::graph::GraphTransition::ResolvePause { .. }
                ) {
                    validate_resume(&snapshot)?;
                }
                let execution = snapshot
                    .graph_executions
                    .iter_mut()
                    .find(|execution| execution.id == transition.execution_id())
                    .ok_or("graph execution not found")?;
                let remaining = snapshot
                    .limits
                    .max_loops
                    .checked_sub(snapshot.loops)
                    .ok_or("workflow loop budget exhausted")?;
                snapshot.loops = snapshot
                    .loops
                    .checked_add(execution.apply(transition, remaining)?)
                    .ok_or("loop counter overflow")?;
                if snapshot
                    .graph_executions
                    .iter()
                    .any(|graph| graph.pauses.iter().any(|pause| pause.resolution.is_none()))
                {
                    snapshot.status = RunStatus::Paused;
                } else if matches!(
                    transition,
                    super::graph::GraphTransition::ResolvePause { .. }
                ) {
                    snapshot.status = RunStatus::Running;
                }
            }
        },
        RunEventKind::PlanningClosed { fingerprint } => {
            snapshot.planning_fingerprint = Some(fingerprint.clone());
            snapshot.verification_fingerprint = None;
        }
        RunEventKind::PlanningReopened => {
            snapshot.planning_fingerprint = None;
            snapshot.verification_fingerprint = None;
        }
        RunEventKind::AttemptStarted { attempt } => {
            if snapshot
                .attempts
                .iter()
                .any(|existing| existing.id == attempt.id)
            {
                return Err("duplicate node attempt".into());
            }
            if attempt.story_id != snapshot.plan_id {
                let story = snapshot
                    .stories
                    .iter_mut()
                    .find(|story| story.story_id == attempt.story_id);
                if let Some(story) = story {
                    story.attempt_ids.push(attempt.id.clone());
                } else {
                    snapshot.stories.push(StoryExecution {
                        story_id: attempt.story_id.clone(),
                        accepted: false,
                        accepted_revision: None,
                        worktree_path: None,
                        attempt_ids: vec![attempt.id.clone()],
                        check_receipts: vec![],
                        integration_receipt: None,
                    });
                }
            }
            snapshot.attempts.push((**attempt).clone());
        }
        RunEventKind::WorktreeAssigned { story_id, path } => {
            let story = snapshot
                .stories
                .iter_mut()
                .find(|story| story.story_id == *story_id)
                .ok_or("story execution not found")?;
            if story.worktree_path.is_some() {
                return Err("story worktree already assigned".into());
            }
            story.worktree_path = Some(path.clone());
        }
        RunEventKind::AttemptReported {
            attempt_id,
            generation,
            outcome,
            report,
        } => {
            let attempt = snapshot
                .attempts
                .iter_mut()
                .find(|attempt| attempt.id == *attempt_id)
                .ok_or("node attempt not found")?;
            if attempt.generation != *generation || attempt.state != AttemptState::Running {
                return Err("attempt report is no longer current".into());
            }
            attempt.state = AttemptState::Reported;
            attempt.outcome = Some(*outcome);
            attempt.report = report.clone();
            if *outcome == AttemptOutcome::NeedsInput {
                snapshot.status = RunStatus::Paused;
            }
        }
        RunEventKind::InputAnswered { attempt_id, answer } => {
            let attempt = snapshot
                .attempts
                .iter_mut()
                .find(|attempt| attempt.id == *attempt_id)
                .ok_or("node attempt not found")?;
            if attempt.outcome != Some(AttemptOutcome::NeedsInput) || attempt.input_answer.is_some()
            {
                return Err("attempt has no pending input request".into());
            }
            attempt.input_answer = Some(answer.clone());
        }
        RunEventKind::AgentBound {
            attempt_id,
            binding,
        } => {
            let attempt = snapshot
                .attempts
                .iter_mut()
                .find(|attempt| attempt.id == *attempt_id)
                .ok_or("node attempt not found")?;
            if attempt.state != AttemptState::Running || attempt.agent.is_some() {
                return Err("node attempt cannot bind an agent".into());
            }
            let effect = snapshot
                .effects
                .iter_mut()
                .find(|effect| effect.id == binding.effect_id)
                .ok_or("spawn effect not found")?;
            if effect.kind != EffectKind::SpawnAgent
                || effect.state != EffectState::Intended
                || effect.key != format!("spawn:{attempt_id}")
            {
                return Err("spawn effect is not intended".into());
            }
            effect.state = EffectState::Succeeded;
            attempt.agent = Some(binding.clone());
        }
        RunEventKind::LateReportIgnored { .. } => {}
        RunEventKind::AttemptInterrupted { attempt_id } => {
            let attempt = snapshot
                .attempts
                .iter_mut()
                .find(|attempt| attempt.id == *attempt_id)
                .ok_or("node attempt not found")?;
            if attempt.state != AttemptState::Running {
                return Err("attempt is no longer running".into());
            }
            attempt.state = AttemptState::Interrupted;
            attempt.outcome = Some(AttemptOutcome::Interrupted);
            snapshot.status = RunStatus::Paused;
        }
        RunEventKind::EffectReserved { effect } => {
            if snapshot
                .effects
                .iter()
                .any(|existing| existing.id == effect.id || existing.key == effect.key)
            {
                return Err("effect key already reserved".into());
            }
            match effect.kind {
                EffectKind::SpawnAgent => {
                    snapshot.spawns = snapshot
                        .spawns
                        .checked_add(1)
                        .ok_or("spawn counter overflow")?
                }
                EffectKind::CreateStory => {
                    snapshot.story_creations = snapshot
                        .story_creations
                        .checked_add(1)
                        .ok_or("story creation counter overflow")?
                }
                _ => {}
            }
            if snapshot.spawns > snapshot.limits.max_spawns
                || snapshot.story_creations > snapshot.limits.max_story_creations
            {
                return Err("workflow effect budget exhausted".into());
            }
            snapshot.effects.push(effect.clone());
        }
        RunEventKind::EffectChanged { effect_id, state } => {
            let effect = snapshot
                .effects
                .iter_mut()
                .find(|effect| effect.id == *effect_id)
                .ok_or("effect intent not found")?;
            if !matches!(
                (effect.state, *state),
                (
                    EffectState::Intended,
                    EffectState::Succeeded | EffectState::Failed | EffectState::Uncertain
                ) | (
                    EffectState::Uncertain,
                    EffectState::Succeeded | EffectState::Failed
                )
            ) {
                return Err("effect state cannot change this way".into());
            }
            effect.state = *state;
            if *state == EffectState::Uncertain {
                snapshot.status = RunStatus::Paused;
            }
        }
        RunEventKind::LoopAdvanced => {
            snapshot.loops = snapshot
                .loops
                .checked_add(1)
                .ok_or("loop counter overflow")?;
            if snapshot.loops > snapshot.limits.max_loops {
                return Err("workflow loop budget exhausted".into());
            }
        }
        RunEventKind::StoryAccepted { story_id, revision } => {
            let story = snapshot
                .stories
                .iter_mut()
                .find(|story| story.story_id == *story_id);
            if let Some(story) = story {
                story.accepted = true;
                story.accepted_revision = Some(*revision);
            } else {
                snapshot.stories.push(StoryExecution {
                    story_id: story_id.clone(),
                    accepted: true,
                    accepted_revision: Some(*revision),
                    worktree_path: None,
                    attempt_ids: vec![],
                    check_receipts: vec![],
                    integration_receipt: None,
                });
            }
        }
        RunEventKind::CheckRecorded { story_id, receipt } => {
            let story = snapshot
                .stories
                .iter_mut()
                .find(|story| story.story_id == *story_id)
                .ok_or("story execution not found")?;
            if !story.accepted
                && !snapshot
                    .graph_executions
                    .iter()
                    .any(|g| g.target_id == *story_id && !g.completed)
            {
                return Err("story has not been accepted".into());
            }
            // Graph checks run before independent approval; their artifact is fenced by the service.
            story.check_receipts.push(receipt.clone());
        }
        RunEventKind::StoryIntegrated { story_id, receipt } => {
            let story = snapshot
                .stories
                .iter_mut()
                .find(|story| story.story_id == *story_id)
                .ok_or("story execution not found")?;
            if !story.accepted || story.accepted_revision != Some(receipt.story_revision) {
                return Err("integration receipt has a stale story revision".into());
            }
            if story.integration_receipt.is_some() {
                return Err("story has already been integrated".into());
            }
            story.integration_receipt = Some(receipt.clone());
        }
        RunEventKind::CanonicalRecertified { receipt } => {
            snapshot.canonical_recertification = Some(receipt.clone());
            snapshot.verification_fingerprint = None;
        }
        RunEventKind::VerificationPassed { fingerprint } => {
            snapshot.verification_fingerprint = Some(fingerprint.clone())
        }
        RunEventKind::Paused => snapshot.status = RunStatus::Paused,
        RunEventKind::PausedFor { cause } => {
            snapshot.status = RunStatus::Paused;
            snapshot.pause_cause = Some(cause.clone());
        }
        RunEventKind::DeadlineExpired { deadline_ms } => {
            if snapshot.status != RunStatus::Running
                || (*deadline_ms != super::runtime::deadline_ms(&snapshot)
                    && *deadline_ms
                        != snapshot
                            .started_ms
                            .saturating_add(i64::from(snapshot.limits.max_duration_secs) * 1000))
                || event.at_ms < *deadline_ms
            {
                return Err("invalid workflow deadline expiry".into());
            }
            snapshot.status = RunStatus::Paused;
        }
        RunEventKind::GraphResumed {
            execution_id,
            activation_id,
            resolution,
        } => {
            if resolution.trim().is_empty() || resolution.len() > 4096 {
                return Err("graph resume needs a bounded explicit resolution".into());
            }
            super::store::validate_graph_recovery(&snapshot, execution_id, activation_id)?;
            snapshot.status = RunStatus::Running;
        }
        RunEventKind::Resumed => {
            validate_resume(&snapshot)?;
            snapshot.status = RunStatus::Running;
        }
        RunEventKind::Cancelled => {
            // A spawn may already be in flight outside this transaction. Keep
            // its outcome explicit even though terminal runs cannot accept a
            // later binding or reconciliation event.
            for effect in &mut snapshot.effects {
                if effect.state == EffectState::Intended {
                    effect.state = EffectState::Uncertain;
                }
            }
            for attempt in &mut snapshot.attempts {
                if attempt.state == AttemptState::Running {
                    attempt.state = AttemptState::Interrupted;
                    attempt.outcome = Some(AttemptOutcome::Interrupted);
                }
            }
            snapshot.status = RunStatus::Cancelled;
        }
        RunEventKind::Completed => snapshot.status = RunStatus::Completed,
    }
    if snapshot.status != RunStatus::Paused {
        snapshot.pause_cause = None;
    }
    if previous_status != RunStatus::Paused && snapshot.status == RunStatus::Paused {
        snapshot.paused_since_ms = Some(event.at_ms);
    } else if previous_status == RunStatus::Paused
        && snapshot.status != RunStatus::Paused
        && let Some(since) = snapshot.paused_since_ms.take()
    {
        snapshot.paused_duration_ms = snapshot
            .paused_duration_ms
            .saturating_add(event.at_ms.saturating_sub(since).max(0));
    }
    snapshot.sequence = event.sequence;
    Ok(snapshot)
}

/// Shared resume fence for commands and graph event replay.
pub(super) fn validate_resume(snapshot: &RunSnapshot) -> Result<(), String> {
    if snapshot.status != RunStatus::Paused {
        return Err("workflow is not paused".into());
    }
    if snapshot
        .effects
        .iter()
        .any(|effect| effect.state == EffectState::Uncertain)
    {
        return Err("uncertain effects need an explicit resolution".into());
    }
    if snapshot.attempts.iter().any(|attempt| {
        attempt.outcome == Some(AttemptOutcome::NeedsInput) && attempt.input_answer.is_none()
    }) {
        return Err("human input is still pending".into());
    }
    Ok(())
}
