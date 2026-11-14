//! Progress Flow — the journal drawn as a delegation sequence.
//!
//! One column per participant: every terminal that wrote to or was named by
//! the project's journal, the Claude in-process subagents of each live
//! terminal, and the native children (Codex, ego) it reports live. The arrows
//! are the content: a `delegated` entry is a hand-off from a parent to the
//! child it spawned, a `message` entry is a peer-to-peer
//! `agent action=send`, a child's `done` or `blocked` is its return to the
//! parent, and a subagent's spawn and final report are the same two arrows
//! read from its transcript. Time is order, not an axis.
//!
//! Hand-offs are journaled rather than read from `session_parent` or the agent
//! inbox: both are in-memory, the first is removed when the child closes and
//! the second is capped and lost on restart. The journal is the only source
//! that still knows who delegated what an hour later.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::model::{LIST_LIMIT, ProgressEntry, ProgressKind};
use crate::subagent_map::{SubagentFlow, TextPart, prompt_summary};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressFlowInput {
    /// None draws every terminal in the project. A terminal draws itself, its
    /// direct parent and its direct children.
    #[serde(default)]
    pub pty_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressFlowDetailInput {
    pub pty_id: String,
    pub agent_id: String,
    pub part: TextPart,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressFlowDetail {
    pub text: String,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantKind {
    Terminal,
    Subagent,
}

/// `busy`/`idle`/`awaiting`/`closed` for a terminal, `running`/`done` for a
/// subagent. A closed terminal keeps its column: the journal outlives it.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FlowState {
    Busy,
    Idle,
    Awaiting,
    Closed,
    Running,
    Done,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FlowParticipant {
    /// The PTY id for a terminal, `<ptyId>/<agentId>` for a subagent.
    pub id: String,
    pub kind: ParticipantKind,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    pub state: FlowState,
    /// The participant that handed this one its work.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// A terminal's newest `intent:`, redacted and shortened.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    /// Tool calls a subagent made. Zero for a terminal.
    pub tool_calls: u32,
    /// The terminal a participant belongs to — itself, for a terminal.
    pub pty_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FlowEventKind {
    Intent,
    Done,
    Blocked,
    Delegated,
    Message,
    SubagentSpawn,
    SubagentReturn,
}

/// Where the full text of a subagent arrow is fetched from.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FlowDetailRef {
    pub pty_id: String,
    pub agent_id: String,
    pub part: TextPart,
}

/// One row of the sequence. `to` is `None` for a note on the `from` column.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FlowEvent {
    /// Stable across refreshes: `entry:<journal id>` for a journal row,
    /// `<participant id>:spawn` or `:return` for a subagent arrow. The view
    /// keys its expanded rows by it, because the position of an event moves
    /// whenever an earlier one is added or the oldest falls out of the window.
    pub id: String,
    pub kind: FlowEventKind,
    pub from: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// Redacted, whitespace-collapsed, at most 200 characters.
    pub summary: String,
    /// The whole journal text, redacted — present only when `summary` is
    /// shorter. A journal entry is capped at 500 characters, so it travels
    /// with the row instead of costing a request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// A subagent arrow whose full prompt or report must be fetched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<FlowDetailRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    pub at_ms: i64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressFlow {
    pub project: String,
    /// Parents before their children, in the order they first appear.
    pub participants: Vec<FlowParticipant>,
    /// Oldest first.
    pub events: Vec<FlowEvent>,
    /// The journal held more than `LIST_LIMIT` entries; only the newest were
    /// drawn, so the first delegations may be missing.
    pub truncated: bool,
}

/// What the handler knows about a terminal that is still open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LiveTerminal {
    pub title: Option<String>,
    pub agent_type: Option<String>,
    pub state: Option<FlowState>,
}

fn summarize(text: &str) -> (String, bool) {
    let (summary, more) = prompt_summary(text);
    (summary.unwrap_or_default(), more)
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

/// Build the flow from the journal (oldest first), the open terminals and
/// the subagents of each. Pure, so every rule below is testable without a PTY.
pub(crate) fn build_flow(
    project: &str,
    entries: &[ProgressEntry],
    live: &HashMap<String, LiveTerminal>,
    subagents: &HashMap<String, Vec<SubagentFlow>>,
    scope: Option<&str>,
    truncated: bool,
) -> ProgressFlow {
    // Terminals in the order they first appear, then the ones known only for
    // their subagents or because they were asked for.
    let mut terminals: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut note = |id: &str, terminals: &mut Vec<String>| {
        if seen.insert(id.to_string()) {
            terminals.push(id.to_string());
        }
    };
    for entry in entries {
        if let Some(pty) = entry.pty_id.as_deref() {
            note(pty, &mut terminals);
        }
        if let Some(target) = entry.target_pty_id.as_deref() {
            note(target, &mut terminals);
        }
    }
    let mut with_subagents: Vec<&String> = subagents.keys().collect();
    with_subagents.sort();
    for pty in with_subagents {
        note(pty, &mut terminals);
    }
    if let Some(pty) = scope {
        note(pty, &mut terminals);
    }

    // The first delegation to a terminal names its parent. A later one would be
    // a second spawn into an id that is a fresh UUID per spawn — not a case.
    let mut parent_of: HashMap<String, String> = HashMap::new();
    let mut remembered_name: HashMap<String, String> = HashMap::new();
    let mut latest_intent: HashMap<String, String> = HashMap::new();
    for entry in entries {
        let Some(pty) = entry.pty_id.as_deref() else {
            continue;
        };
        if let Some(name) = entry.agent_name.as_deref() {
            remembered_name.insert(pty.to_string(), name.to_string());
        }
        if let (Some(target), Some(name)) =
            (entry.target_pty_id.as_deref(), entry.target_name.as_deref())
        {
            remembered_name
                .entry(target.to_string())
                .or_insert_with(|| name.to_string());
        }
        match entry.kind {
            ProgressKind::Delegated => {
                if let Some(target) = entry.target_pty_id.as_deref()
                    && target != pty
                {
                    parent_of
                        .entry(target.to_string())
                        .or_insert_with(|| pty.to_string());
                }
            }
            ProgressKind::Intent => {
                latest_intent.insert(pty.to_string(), summarize(&entry.text).0);
            }
            _ => {}
        }
    }

    let mut participants: Vec<FlowParticipant> = Vec::new();
    for pty in &terminals {
        let now = live.get(pty);
        let title = now
            .and_then(|t| t.title.clone())
            .filter(|t| !t.trim().is_empty())
            .or_else(|| remembered_name.get(pty).cloned())
            .unwrap_or_else(|| format!("Terminal {}", short_id(pty)));
        let agent_type = now
            .and_then(|t| t.agent_type.clone())
            .filter(|a| *a != title);
        participants.push(FlowParticipant {
            id: pty.clone(),
            kind: ParticipantKind::Terminal,
            title,
            agent_type,
            state: now.and_then(|t| t.state).unwrap_or(FlowState::Closed),
            parent: parent_of.get(pty).cloned(),
            intent: latest_intent.get(pty).cloned(),
            tool_calls: 0,
            pty_id: pty.clone(),
            agent_id: None,
        });
    }

    let mut events: Vec<FlowEvent> = Vec::new();
    for entry in entries {
        let Some(pty) = entry.pty_id.as_deref() else {
            // DEFERRED (2026-09-23) — entries from before PTY attribution have
            // no column. They stay visible in the List view.
            continue;
        };
        let (kind, to) = match entry.kind {
            ProgressKind::Intent => (FlowEventKind::Intent, None),
            // A child's outcome is its return to whoever handed it the work.
            ProgressKind::Done => (FlowEventKind::Done, parent_of.get(pty).cloned()),
            ProgressKind::Blocked => (FlowEventKind::Blocked, parent_of.get(pty).cloned()),
            ProgressKind::Delegated => (FlowEventKind::Delegated, entry.target_pty_id.clone()),
            ProgressKind::Message => (FlowEventKind::Message, entry.target_pty_id.clone()),
        };
        let (summary, more) = summarize(&entry.text);
        events.push(FlowEvent {
            id: format!("entry:{}", entry.id),
            kind,
            from: pty.to_string(),
            to,
            summary,
            text: more.then(|| crate::redaction::redact_secrets(entry.text.trim())),
            detail: None,
            step: entry.step.clone(),
            at_ms: i64::try_from(entry.created_at_ms).unwrap_or(i64::MAX),
        });
    }

    let mut owners: Vec<&String> = subagents.keys().collect();
    owners.sort();
    for pty in owners {
        let lanes = &subagents[pty];
        let known: HashSet<&str> = lanes.iter().map(|l| l.agent_id.as_str()).collect();
        let id_of = |agent: &str| format!("{pty}/{agent}");
        for lane in lanes {
            let id = id_of(&lane.agent_id);
            let parent = lane
                .parent_agent_id
                .as_deref()
                .filter(|p| known.contains(p) && *p != lane.agent_id)
                .map(id_of)
                .unwrap_or_else(|| pty.clone());
            participants.push(FlowParticipant {
                id: id.clone(),
                kind: ParticipantKind::Subagent,
                title: lane.title.clone(),
                agent_type: lane.agent_type.clone(),
                state: if lane.running {
                    FlowState::Running
                } else {
                    FlowState::Done
                },
                parent: Some(parent.clone()),
                intent: None,
                tool_calls: lane.tool_calls,
                pty_id: pty.clone(),
                agent_id: Some(lane.agent_id.clone()),
            });
            let detail = |part| {
                Some(FlowDetailRef {
                    pty_id: pty.clone(),
                    agent_id: lane.agent_id.clone(),
                    part,
                })
            };
            if let Some(at_ms) = lane.started_at_ms {
                let (summary, more) = summarize(&lane.prompt);
                events.push(FlowEvent {
                    id: format!("{id}:spawn"),
                    kind: FlowEventKind::SubagentSpawn,
                    from: parent.clone(),
                    to: Some(id.clone()),
                    summary,
                    text: None,
                    detail: more.then(|| detail(TextPart::Prompt)).flatten(),
                    step: None,
                    at_ms,
                });
            }
            if !lane.running
                && let Some(at_ms) = lane.ended_at_ms
            {
                let (summary, more) = summarize(&lane.report);
                events.push(FlowEvent {
                    id: format!("{id}:return"),
                    kind: FlowEventKind::SubagentReturn,
                    from: id.clone(),
                    to: Some(parent.clone()),
                    summary,
                    text: None,
                    detail: more.then(|| detail(TextPart::Report)).flatten(),
                    step: None,
                    at_ms,
                });
            }
        }
    }
    // Stable: journal rows keep their rowid order within one millisecond.
    events.sort_by_key(|e| e.at_ms);

    if let Some(pty) = scope {
        let mut keep: HashSet<String> = HashSet::from([pty.to_string()]);
        if let Some(parent) = parent_of.get(pty) {
            keep.insert(parent.clone());
        }
        for p in &participants {
            let is_child = p.kind == ParticipantKind::Terminal && p.parent.as_deref() == Some(pty);
            let is_own_subagent = p.kind == ParticipantKind::Subagent && p.pty_id == pty;
            if is_child || is_own_subagent {
                keep.insert(p.id.clone());
            }
        }
        participants.retain(|p| keep.contains(&p.id));
        events
            .retain(|e| keep.contains(&e.from) && e.to.as_ref().is_none_or(|to| keep.contains(to)));
    }

    ProgressFlow {
        project: project.to_string(),
        participants: parents_first(participants),
        events,
        truncated,
    }
}

/// Order columns so a parent always sits left of its children, keeping first
/// appearance among siblings. A parent that is not drawn, or a cycle, leaves
/// the participant a root rather than dropping it.
fn parents_first(participants: Vec<FlowParticipant>) -> Vec<FlowParticipant> {
    let ids: HashSet<&str> = participants.iter().map(|p| p.id.as_str()).collect();
    let mut kids: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut roots: Vec<usize> = Vec::new();
    for (i, p) in participants.iter().enumerate() {
        match p
            .parent
            .as_deref()
            .filter(|parent| ids.contains(parent) && *parent != p.id)
        {
            Some(parent) => kids.entry(parent).or_default().push(i),
            None => roots.push(i),
        }
    }
    let mut order: Vec<usize> = Vec::with_capacity(participants.len());
    let mut placed = vec![false; participants.len()];
    fn visit(
        i: usize,
        participants: &[FlowParticipant],
        kids: &HashMap<&str, Vec<usize>>,
        placed: &mut [bool],
        order: &mut Vec<usize>,
    ) {
        if placed[i] {
            return;
        }
        placed[i] = true;
        order.push(i);
        for &k in kids.get(participants[i].id.as_str()).into_iter().flatten() {
            visit(k, participants, kids, placed, order);
        }
    }
    for &r in &roots {
        visit(r, &participants, &kids, &mut placed, &mut order);
    }
    // Whatever a cycle kept unreachable from any root.
    for i in 0..participants.len() {
        visit(i, &participants, &kids, &mut placed, &mut order);
    }
    let mut slots: Vec<Option<FlowParticipant>> = participants.into_iter().map(Some).collect();
    order
        .into_iter()
        .map(|i| slots[i].take().expect("each participant is placed once"))
        .collect()
}

/// Running rows for the native children a terminal reports. The roster keeps
/// labels in start order without ids, so a row is keyed by its position.
fn native_lanes(children: &[String]) -> Vec<SubagentFlow> {
    children
        .iter()
        .enumerate()
        .map(|(index, label)| SubagentFlow {
            agent_id: format!("native-{index}"),
            parent_agent_id: None,
            title: label.clone(),
            agent_type: None,
            running: true,
            started_at_ms: None,
            ended_at_ms: None,
            tool_calls: 0,
            prompt: String::new(),
            report: String::new(),
        })
        .collect()
}

/// The flow for one project, as the dialog asks for it.
pub fn progress_flow(
    state: &crate::state::AppState,
    project: &str,
    input: ProgressFlowInput,
) -> Result<ProgressFlow, String> {
    let project = super::service::project_of(project)?;
    let (mut entries, truncated) = newest_entries(&project)?;
    entries.reverse();

    let mut ids: HashSet<String> = entries
        .iter()
        .flat_map(|e| [e.pty_id.clone(), e.target_pty_id.clone()])
        .flatten()
        .collect();
    // An open terminal of this project with no journal entry still has
    // subagents worth drawing.
    for session in state.session_maps.sessions.iter() {
        let id = session.key();
        let owned = super::service::project_for_session(state, id)
            .and_then(|p| super::service::project_of(&p).ok())
            .is_some_and(|p| p == project);
        if owned {
            ids.insert(id.clone());
        }
    }
    if let Some(pty) = input.pty_id.as_deref() {
        ids.insert(pty.to_string());
    }

    let mut live: HashMap<String, LiveTerminal> = HashMap::new();
    let mut subagents: HashMap<String, Vec<SubagentFlow>> = HashMap::new();
    for id in &ids {
        if !state.session_maps.sessions.contains_key(id) {
            continue;
        }
        let snapshot = state.session_state_with_shell(id);
        let (agent_type, title) = super::service::session_identity(state, id);
        live.insert(
            id.clone(),
            LiveTerminal {
                title,
                agent_type,
                state: Some(match &snapshot {
                    Some(s) if s.awaiting_input => FlowState::Awaiting,
                    Some(s) if s.shell_state.as_deref() == Some("busy") => FlowState::Busy,
                    _ => FlowState::Idle,
                }),
            },
        );
        // Native children (Codex hooks, ego frames) are live rows only: the
        // roster drops a child on its stop or when the lead's turn is cleared,
        // so a child is never drawn running from memory (#2635-8052).
        let native = snapshot
            .as_ref()
            .map(|s| native_lanes(&s.native_children))
            .unwrap_or_default();
        if !native.is_empty() {
            subagents.entry(id.clone()).or_default().extend(native);
        }
        // DEFERRED (2026-09-23) — subagents are read only for open terminals:
        // resolving a Claude transcript needs the agent's pid. Keeping them
        // after close needs the session uuid journaled at discovery time.
        if let Some(source) = crate::subagent_map::transcript_source(state, id) {
            let lanes = crate::subagent_map::subagent_flows(
                &mut state.subagent_map_cache.lock(),
                &source.subagents_dir,
                &source.parent_transcript,
            );
            if !lanes.is_empty() {
                subagents.entry(id.clone()).or_default().extend(lanes);
            }
        }
    }

    if let Some(cutoff) = std::time::Instant::now().checked_sub(crate::subagent_map::CACHE_IDLE) {
        state.subagent_map_cache.lock().evict_idle_before(cutoff);
    }

    Ok(build_flow(
        &project,
        &entries,
        &live,
        &subagents,
        input.pty_id.as_deref(),
        truncated,
    ))
}

/// The newest `LIST_LIMIT` journal entries, newest first, and whether older
/// ones exist. One extra row is read: a journal of exactly `LIST_LIMIT`
/// entries was not cut and must not say so.
fn newest_entries(project: &str) -> Result<(Vec<ProgressEntry>, bool), String> {
    let mut entries = super::store::ProgressStore::open()?
        .list_limited(project, &Default::default(), LIST_LIMIT + 1)?
        .entries;
    let truncated = entries.len() > LIST_LIMIT;
    entries.truncate(LIST_LIMIT);
    Ok((entries, truncated))
}

/// `progress_flow` on the blocking pool. It reads SQLite and parses Claude
/// transcripts (the largest here is 31 MB) under the shared transcript-cache
/// mutex, so neither the Tauri main thread nor a tokio worker may run it.
pub async fn progress_flow_blocking(
    state: std::sync::Arc<crate::state::AppState>,
    project: String,
    input: ProgressFlowInput,
) -> Result<ProgressFlow, String> {
    tokio::task::spawn_blocking(move || progress_flow(&state, &project, input))
        .await
        .map_err(|e| format!("progress flow task failed: {e}"))?
}

/// `progress_flow_detail` on the blocking pool, for the same reason.
pub async fn progress_flow_detail_blocking(
    state: std::sync::Arc<crate::state::AppState>,
    input: ProgressFlowDetailInput,
) -> Result<ProgressFlowDetail, String> {
    tokio::task::spawn_blocking(move || progress_flow_detail(&state, input))
        .await
        .map_err(|e| format!("progress flow task failed: {e}"))?
}

/// The full, redacted prompt or report behind one subagent arrow.
///
/// `ptyId` selects a terminal in `AppState` and `agentId` is compared against
/// the subagents found on disk for it; neither becomes part of a path.
pub fn progress_flow_detail(
    state: &crate::state::AppState,
    input: ProgressFlowDetailInput,
) -> Result<ProgressFlowDetail, String> {
    let source = crate::subagent_map::transcript_source(state, &input.pty_id)
        .ok_or_else(|| "not_found: no open Claude terminal with that id".to_string())?;
    crate::subagent_map::subagent_text(
        &mut state.subagent_map_cache.lock(),
        &source.subagents_dir,
        &source.parent_transcript,
        &input.agent_id,
        input.part,
    )
    .map(|text| ProgressFlowDetail { text })
    .ok_or_else(|| "not_found: no subagent with that id in that terminal".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
        id: i64,
        kind: ProgressKind,
        pty: &str,
        text: &str,
        target: Option<(&str, &str)>,
    ) -> ProgressEntry {
        ProgressEntry {
            id,
            project: "/repo".into(),
            pty_id: Some(pty.into()),
            created_at_ms: 1_000 + id as u64,
            kind,
            text: text.into(),
            step: None,
            agent_name: Some(format!("name-{pty}")),
            target_pty_id: target.map(|(t, _)| t.into()),
            target_name: target.map(|(_, n)| n.into()),
        }
    }

    fn flow(entries: &[ProgressEntry], scope: Option<&str>) -> ProgressFlow {
        build_flow(
            "/repo",
            entries,
            &HashMap::new(),
            &HashMap::new(),
            scope,
            false,
        )
    }

    fn arrows(flow: &ProgressFlow) -> Vec<(FlowEventKind, &str, Option<&str>)> {
        flow.events
            .iter()
            .map(|e| (e.kind, e.from.as_str(), e.to.as_deref()))
            .collect()
    }

    fn orchestration() -> Vec<ProgressEntry> {
        vec![
            entry(
                1,
                ProgressKind::Intent,
                "lead",
                "Split the parser work",
                None,
            ),
            entry(
                2,
                ProgressKind::Delegated,
                "lead",
                "Write the lexer",
                Some(("w1", "lexer")),
            ),
            entry(
                3,
                ProgressKind::Delegated,
                "lead",
                "Write the tests",
                Some(("w2", "tests")),
            ),
            entry(
                4,
                ProgressKind::Message,
                "w2",
                "Which token set?",
                Some(("w1", "lexer")),
            ),
            entry(5, ProgressKind::Done, "w1", "Lexer shipped", None),
            entry(6, ProgressKind::Blocked, "w2", "Needs the grammar", None),
        ]
    }

    /// The arrows are the content: a delegation goes parent → child, a peer
    /// message goes sender → recipient, and a child's own outcome is its
    /// return to the parent that delegated to it.
    #[test]
    fn flow_turns_the_journal_into_hand_offs_and_returns() {
        let f = flow(&orchestration(), None);
        assert_eq!(
            arrows(&f),
            vec![
                (FlowEventKind::Intent, "lead", None),
                (FlowEventKind::Delegated, "lead", Some("w1")),
                (FlowEventKind::Delegated, "lead", Some("w2")),
                (FlowEventKind::Message, "w2", Some("w1")),
                (FlowEventKind::Done, "w1", Some("lead")),
                (FlowEventKind::Blocked, "w2", Some("lead")),
            ]
        );
        let ids: Vec<&str> = f.participants.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["lead", "w1", "w2"],
            "the parent's column comes first"
        );
        assert_eq!(f.participants[1].parent.as_deref(), Some("lead"));
        assert_eq!(
            f.participants[0].intent.as_deref(),
            Some("Split the parser work")
        );
    }

    /// A terminal that has closed keeps its column and its name: the journal
    /// outlives the PTY, which is the reason hand-offs are journaled at all.
    #[test]
    fn flow_names_a_closed_terminal_from_the_journal() {
        let f = flow(&orchestration(), None);
        let w1 = f.participants.iter().find(|p| p.id == "w1").unwrap();
        assert_eq!(w1.state, FlowState::Closed);
        assert_eq!(w1.title, "name-w1", "its own entries name it");

        let only_delegated = vec![entry(
            1,
            ProgressKind::Delegated,
            "lead",
            "go",
            Some(("w9", "scout")),
        )];
        let f = flow(&only_delegated, None);
        let w9 = f.participants.iter().find(|p| p.id == "w9").unwrap();
        assert_eq!(
            w9.title, "scout",
            "a child that never wrote is named by the delegation"
        );
    }

    /// An open terminal is described by what TUIC sees now, not by the name it
    /// had when it last wrote.
    #[test]
    fn flow_prefers_the_live_terminal() {
        let live = HashMap::from([(
            "lead".to_string(),
            LiveTerminal {
                title: Some("orchestrator".into()),
                agent_type: Some("claude".into()),
                state: Some(FlowState::Busy),
            },
        )]);
        let f = build_flow(
            "/repo",
            &orchestration(),
            &live,
            &HashMap::new(),
            None,
            false,
        );
        let lead = &f.participants[0];
        assert_eq!(lead.title, "orchestrator");
        assert_eq!(lead.agent_type.as_deref(), Some("claude"));
        assert_eq!(lead.state, FlowState::Busy);
    }

    /// One terminal's view is that terminal, its direct parent and its direct
    /// children. A sibling's traffic is not part of it.
    #[test]
    fn flow_scoped_to_a_terminal_keeps_its_parent_and_children_only() {
        let mut entries = orchestration();
        entries.push(entry(
            7,
            ProgressKind::Delegated,
            "w1",
            "Profile it",
            Some(("g1", "profiler")),
        ));
        let f = flow(&entries, Some("w1"));
        let ids: Vec<&str> = f.participants.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["lead", "w1", "g1"]);
        assert_eq!(
            arrows(&f),
            vec![
                (FlowEventKind::Intent, "lead", None),
                (FlowEventKind::Delegated, "lead", Some("w1")),
                (FlowEventKind::Done, "w1", Some("lead")),
                (FlowEventKind::Delegated, "w1", Some("g1")),
            ],
            "the lead's delegation to w2 and w2's message are a sibling's business"
        );
    }

    /// A root has no parent to return to: its outcome is a note on its own
    /// column, not an arrow into nothing.
    #[test]
    fn flow_draws_a_roots_outcome_as_a_note() {
        let f = flow(
            &[entry(1, ProgressKind::Done, "solo", "Shipped", None)],
            None,
        );
        assert_eq!(arrows(&f), vec![(FlowEventKind::Done, "solo", None)]);
    }

    /// Delegation links come from the journal, not from a structure that
    /// guarantees a tree. A cycle costs the edge, never a column.
    #[test]
    fn flow_survives_a_delegation_cycle() {
        let entries = vec![
            entry(1, ProgressKind::Delegated, "a", "x", Some(("b", "b"))),
            entry(2, ProgressKind::Delegated, "b", "y", Some(("a", "a"))),
        ];
        let f = flow(&entries, None);
        assert_eq!(f.participants.len(), 2);
    }

    fn lane(
        agent: &str,
        parent: Option<&str>,
        running: bool,
        prompt: &str,
        report: &str,
    ) -> SubagentFlow {
        SubagentFlow {
            agent_id: agent.into(),
            parent_agent_id: parent.map(Into::into),
            title: agent.into(),
            agent_type: Some("Explore".into()),
            running,
            started_at_ms: Some(1_010),
            ended_at_ms: (!running).then_some(1_020),
            tool_calls: 7,
            prompt: prompt.into(),
            report: report.into(),
        }
    }

    /// A Claude subagent is a column of its own under its terminal: the spawn
    /// arrow carries what it was asked, the return arrow what it reported. A
    /// running one has not returned.
    #[test]
    fn flow_draws_subagents_under_their_terminal() {
        let subs = HashMap::from([(
            "lead".to_string(),
            vec![
                lane("a1", None, false, "Survey the lexer", "Two issues found"),
                lane("a2", Some("a1"), true, "Check the first issue", ""),
            ],
        )]);
        let f = build_flow(
            "/repo",
            &orchestration(),
            &HashMap::new(),
            &subs,
            None,
            false,
        );
        let ids: Vec<&str> = f.participants.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["lead", "w1", "w2", "lead/a1", "lead/a2"]);
        let a2 = f.participants.iter().find(|p| p.id == "lead/a2").unwrap();
        assert_eq!(
            a2.parent.as_deref(),
            Some("lead/a1"),
            "nested by parentAgentId"
        );
        assert_eq!(a2.state, FlowState::Running);
        assert_eq!(a2.tool_calls, 7);

        let sub_arrows: Vec<_> = arrows(&f)
            .into_iter()
            .filter(|(k, _, _)| {
                matches!(
                    k,
                    FlowEventKind::SubagentSpawn | FlowEventKind::SubagentReturn
                )
            })
            .collect();
        assert_eq!(
            sub_arrows,
            vec![
                (FlowEventKind::SubagentSpawn, "lead", Some("lead/a1")),
                (FlowEventKind::SubagentSpawn, "lead/a1", Some("lead/a2")),
                (FlowEventKind::SubagentReturn, "lead/a1", Some("lead")),
            ]
        );
        let ret = f
            .events
            .iter()
            .find(|e| e.kind == FlowEventKind::SubagentReturn)
            .unwrap();
        assert_eq!(ret.summary, "Two issues found");
    }

    /// Revised 2026-09-23 from the `/agents/map` rule that no result body ever
    /// reaches the wire. The Flow view carries a subagent's final report: that
    /// is the return arrow. It arrives redacted and cut to 200 characters,
    /// with a fetch reference for the rest. What a tool returned to the
    /// subagent is still never kept (`LaneSummary::last_reply`).
    #[test]
    fn flow_carries_a_redacted_report_but_never_a_tool_result() {
        let secret = format!("ghp_{}", "S".repeat(40));
        let report = format!(
            "Deployed with {secret}. {}",
            "All checks passed. ".repeat(20)
        );
        let subs = HashMap::from([(
            "lead".to_string(),
            vec![lane("a1", None, false, &format!("Use {secret}"), &report)],
        )]);
        let f = build_flow("/repo", &[], &HashMap::new(), &subs, None, false);
        let wire = serde_json::to_string(&f).unwrap();
        assert!(
            !wire.contains("ghp_"),
            "a secret reached the payload: {wire}"
        );
        let ret = f
            .events
            .iter()
            .find(|e| e.kind == FlowEventKind::SubagentReturn)
            .unwrap();
        assert!(ret.summary.contains("[REDACTED]"));
        assert!(ret.summary.chars().count() <= 200);
        assert_eq!(
            ret.detail,
            Some(FlowDetailRef {
                pty_id: "lead".into(),
                agent_id: "a1".into(),
                part: TextPart::Report
            }),
            "the rest is fetched on demand"
        );
        let spawn = f
            .events
            .iter()
            .find(|e| e.kind == FlowEventKind::SubagentSpawn)
            .unwrap();
        assert_eq!(
            spawn.detail, None,
            "a short prompt has nothing more to fetch"
        );
    }

    /// Journal text is at most 500 characters, so the whole of it travels with
    /// the row — redacted, since an agent's own report is not redacted when
    /// it is written.
    #[test]
    fn flow_ships_a_long_journal_text_inline_and_redacted() {
        let text = format!(
            "Used ghp_{} then {}",
            "T".repeat(40),
            "more work. ".repeat(30)
        );
        let f = flow(&[entry(1, ProgressKind::Done, "solo", &text, None)], None);
        let row = &f.events[0];
        let full = row.text.as_deref().expect("longer than the summary");
        assert!(full.contains("[REDACTED]") && !full.contains("ghp_"));
        assert!(row.summary.chars().count() <= 200);

        let short = flow(
            &[entry(1, ProgressKind::Done, "solo", "Shipped", None)],
            None,
        );
        assert_eq!(short.events[0].text, None);
    }

    #[test]
    fn flow_inputs_refuse_an_unknown_field() {
        serde_json::from_value::<ProgressFlowInput>(serde_json::json!({"ptyId": "a"})).unwrap();
        assert!(
            serde_json::from_value::<ProgressFlowInput>(serde_json::json!({"nope": 1})).is_err()
        );
        serde_json::from_value::<ProgressFlowDetailInput>(
            serde_json::json!({"ptyId": "a", "agentId": "b", "part": "report"}),
        )
        .unwrap();
        assert!(
            serde_json::from_value::<ProgressFlowDetailInput>(
                serde_json::json!({"ptyId": "a", "agentId": "b", "part": "path"}),
            )
            .is_err()
        );
    }

    /// The detail lookup never reaches the filesystem for an id `AppState`
    /// does not know — the same answer as any unknown terminal.
    #[test]
    fn flow_detail_for_an_unknown_terminal_is_not_found() {
        let state = crate::state::tests_support::make_test_app_state();
        let err = progress_flow_detail(
            &state,
            ProgressFlowDetailInput {
                pty_id: "../../etc".into(),
                agent_id: "../passwd".into(),
                part: TextPart::Prompt,
            },
        )
        .unwrap_err();
        assert!(err.starts_with("not_found"), "{err}");
    }

    /// The view keys its expanded rows by event id. An event's id must not
    /// move when an earlier event arrives or the oldest leaves the window —
    /// its position does both.
    #[test]
    fn flow_event_ids_survive_an_earlier_event_arriving() {
        let subs = HashMap::from([(
            "lead".to_string(),
            vec![lane("a1", None, false, "Survey", "Found two")],
        )]);
        let ids = |entries: &[ProgressEntry]| -> HashMap<String, FlowEventKind> {
            build_flow("/repo", entries, &HashMap::new(), &subs, None, false)
                .events
                .into_iter()
                .map(|e| (e.id, e.kind))
                .collect()
        };
        let before = ids(&orchestration());
        assert_eq!(before.len(), orchestration().len() + 2, "ids are unique");
        assert_eq!(before.get("entry:5"), Some(&FlowEventKind::Done));
        assert_eq!(
            before.get("lead/a1:spawn"),
            Some(&FlowEventKind::SubagentSpawn)
        );
        assert_eq!(
            before.get("lead/a1:return"),
            Some(&FlowEventKind::SubagentReturn)
        );

        let mut earlier = vec![entry(0, ProgressKind::Intent, "lead", "Plan", None)];
        earlier[0].created_at_ms = 0;
        earlier.extend(orchestration());
        let after = ids(&earlier);
        for (id, kind) in &before {
            assert_eq!(after.get(id), Some(kind), "{id} kept its identity");
        }
    }

    /// "The earliest hand-offs may be missing" is a claim about the journal.
    /// A journal of exactly `LIST_LIMIT` entries lost nothing.
    #[test]
    fn flow_is_truncated_only_past_the_list_limit() {
        let config = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir().unwrap();
        let hint = repo.path().to_string_lossy().to_string();
        let project = super::super::service::project_of(&hint).unwrap();
        let store = super::super::store::ProgressStore::open().unwrap();
        let insert = |n: usize| {
            let mut conn = rusqlite::Connection::open(store.database_path()).unwrap();
            let tx = conn.transaction().unwrap();
            for i in 0..n {
                tx.execute(
                    "INSERT INTO entries (project, created_at_ms, kind, text, pty_id)
                     VALUES (?1, ?2, 'done', 'step', 'pty')",
                    rusqlite::params![project, i as i64],
                )
                .unwrap();
            }
            tx.commit().unwrap();
        };
        let state = crate::state::tests_support::make_test_app_state();
        let read = || progress_flow(&state, &hint, ProgressFlowInput::default()).unwrap();

        insert(LIST_LIMIT);
        let full = read();
        assert!(!full.truncated, "exactly LIST_LIMIT entries were not cut");
        assert_eq!(full.events.len(), LIST_LIMIT);

        insert(1);
        let cut = read();
        assert!(cut.truncated);
        assert_eq!(cut.events.len(), LIST_LIMIT, "only the newest are drawn");
    }

    /// Catches: a native child that a Codex or ego terminal reports live never
    /// appears in Flow, or a child whose stop went missing stays drawn as
    /// running after the roster dropped it (#2635-8052).
    #[cfg(unix)]
    #[test]
    fn native_children_draw_running_rows_only_while_live() {
        let config = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir().unwrap();
        let hint = repo.path().to_string_lossy().to_string();
        let project = super::super::service::project_of(&hint).unwrap();
        let store = super::super::store::ProgressStore::open().unwrap();
        rusqlite::Connection::open(store.database_path())
            .unwrap()
            .execute(
                "INSERT INTO entries (project, created_at_ms, kind, text, pty_id)
                 VALUES (?1, 1, 'intent', 'spawn a child', 'lead')",
                rusqlite::params![project],
            )
            .unwrap();
        let state = crate::state::tests_support::make_test_app_state();
        crate::test_support::insert_recording_session(&state, "lead");
        state
            .session_maps
            .session_states
            .entry("lead".into())
            .or_default()
            .native_children = vec!["default".into()];
        let read = || progress_flow(&state, &hint, ProgressFlowInput::default()).unwrap();

        let live = read();
        let child = live
            .participants
            .iter()
            .find(|p| p.kind == ParticipantKind::Subagent)
            .expect("the live native child has a row");
        assert_eq!(child.state, FlowState::Running);
        assert_eq!(child.parent.as_deref(), Some("lead"));
        assert_eq!(child.pty_id, "lead");
        assert_eq!(child.title, "default");

        state
            .session_maps
            .session_states
            .get_mut("lead")
            .unwrap()
            .native_children
            .clear();
        assert!(
            read()
                .participants
                .iter()
                .all(|p| p.kind != ParticipantKind::Subagent),
            "a dropped child must not stay drawn"
        );
    }
}
