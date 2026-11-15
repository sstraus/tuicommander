//! The `agent` surface per caller role, in the catalogue and at dispatch (story 2805).
//!
//! The role comes only from identity TUIC wrote at spawn: the session parent
//! link and the fixed capability set. No argument, message or client name is read.
//!
//! Three roles. A managed child keeps mail only. An agent adds `spawn`. The
//! orchestrator keeps every action. The orchestrator is a registered peer in
//! `orchestrator_peers` (Boss, 2026-11-07: the coordinator registers with
//! `orchestrator=true`). A caller with no registered identity keeps the full
//! surface: there is no identity to narrow it by. A managed child that registers
//! `orchestrator=true` stays a managed child: the parent link is checked first.

use std::sync::{Arc, LazyLock};

use serde_json::{Value, json};

use crate::agent_capabilities::Capability;
use crate::state::AppState;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum CallerRole {
    /// Every action: a registered orchestrator, or a caller with no registered identity.
    #[default]
    Orchestrator,
    /// May `spawn`: a root session or a child granted `children`, not registered as orchestrator.
    Agent,
    /// A managed child without the `children` capability.
    ManagedChild,
}

impl CallerRole {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Orchestrator => "orchestrator",
            Self::Agent => "agent",
            Self::ManagedChild => "managed_child",
        }
    }
}

/// `agent` actions a managed child keeps. `register` stays: it is the binding repair path.
const CHILD_AGENT_ACTIONS: &[&str] = &["register", "list_peers", "send", "inbox", "wait", "help"];

/// `agent` actions an agent keeps: the child set plus `spawn` and the task actions of its spawn.
const AGENT_ROLE_ACTIONS: &[&str] = &[
    "register",
    "list_peers",
    "send",
    "inbox",
    "wait",
    "help",
    "spawn",
    "task_get",
    "task_cancel",
];

/// The actions a role has; `None` means every action.
fn role_actions(role: CallerRole) -> Option<&'static [&'static str]> {
    match role {
        CallerRole::Orchestrator => None,
        CallerRole::Agent => Some(AGENT_ROLE_ACTIONS),
        CallerRole::ManagedChild => Some(CHILD_AGENT_ACTIONS),
    }
}

/// The one line the instructions keep for an agent.
pub(super) const AGENT_MISSING_LINE: &str = "- Agent: `agent` has no launch or subscribe/ack/unsubscribe; they belong to the orchestrator. When your task needs one, report BLOCKED to your parent or orchestrator with `agent action=send`.\n";

/// The one line the instructions keep for a managed child.
pub(super) const CHILD_MISSING_LINE: &str = "- Managed child: `agent` has no spawn, launch, task_get/task_cancel or subscribe/ack/unsubscribe. When your task needs one, report BLOCKED to your parent with `agent action=send`.\n";

pub(super) fn resolve(state: &Arc<AppState>, mcp_session_id: Option<&str>) -> CallerRole {
    let Some(caller) =
        super::mcp_transport_ancillary::resolve_mcp_origin_session(state, mcp_session_id)
    else {
        return CallerRole::Orchestrator;
    };
    if state.session_maps.session_parent.contains_key(&caller) {
        let may_spawn = state
            .agent_capabilities
            .get(&caller)
            .is_some_and(|set| set.contains(Capability::Children));
        return if may_spawn {
            CallerRole::Agent
        } else {
            CallerRole::ManagedChild
        };
    }
    if state.orchestrator_peers.contains(&caller) {
        CallerRole::Orchestrator
    } else {
        CallerRole::Agent
    }
}

/// The action the role gate checks: a subscription read belongs to `subscribe`.
fn gated_action(args: &Value) -> &str {
    let action = args["action"].as_str().unwrap_or("");
    if matches!(action, "inbox" | "wait") && args.get("subscription_id").is_some() {
        "subscribe"
    } else {
        action
    }
}

/// Refuse an `agent` action the caller's role does not have.
pub(super) fn refuse_call(role: CallerRole, tool: &str, args: &Value) -> Option<Value> {
    let allowed = role_actions(role)?;
    if tool != "agent" {
        return None;
    }
    let action = gated_action(args);
    if allowed.contains(&action) {
        return None;
    }
    Some(json!({
        "error": format!(
            "Role {} has no agent action '{action}'. Report BLOCKED to your parent or orchestrator with agent action=send when your task needs it.",
            role.as_str()
        ),
        "code": "role_denied",
        "role": role.as_str(),
        "effect": "not_started",
        "retryable": false,
    }))
}

/// Name the role on a refusal another gate produced for a managed child.
pub(super) fn name_role(refusal: &mut Value, role: CallerRole) {
    if role != CallerRole::Orchestrator && refusal.is_object() {
        refusal["role"] = json!(role.as_str());
    }
}

/// Which `agent` fields each action reads. A role keeps the fields of its actions.
pub(super) const AGENT_ACTION_FIELDS: &[(&str, &[&str])] = &[
    (
        "launch",
        &[
            "idempotency_key",
            "launch_id",
            "parent_session_id",
            "workspace",
            "spawn",
            "name",
            "keep_open",
        ],
    ),
    (
        "spawn",
        &[
            "prompt",
            "pty_description",
            "cwd",
            "model",
            "scope",
            "env",
            "print_mode",
            "output_format",
            "agent_type",
            "binary_path",
            "args",
            "rows",
            "cols",
            "name",
            "keep_open",
            "capabilities",
        ],
    ),
    (
        "register",
        &["tuic_session", "replaces", "name", "path", "orchestrator"],
    ),
    ("list_peers", &["to", "path", "connection_id", "compact"]),
    (
        "send",
        &[
            "to",
            "message",
            "report",
            "urgency",
            "keep_open",
            "connection_id",
        ],
    ),
    ("inbox", &["byte_budget", "continuation", "limit", "since"]),
    ("wait", &["timeout_ms", "since"]),
    ("subscribe", &["subscription_id", "filter", "limit"]),
    ("ack", &["subscription_id", "cursor"]),
    ("unsubscribe", &["subscription_id"]),
    ("task_get", &["task_id"]),
    ("task_cancel", &["task_id"]),
];

/// The compact `agent` description, one segment per action ("" = preamble).
const AGENT_GUIDE: &[(&str, &str)] = &[
    (
        "",
        "Managed peers and mail. send to=orchestrator message=<text> reaches your registered coordinator; no list_peers needed. help: full guide. Managed PTYs auto-bind from TUIC_SESSION; a headerless external caller calls register without tuic_session. There is no separate swarm action. Every worker must report task output or blockers with send. Lifecycle notifications carry state only. Read session output only for missing results. Wait: do NOT poll in a loop; waits cap at 300s.",
    ),
    (
        "launch",
        "launch: loopback or authenticated, durable/idempotent; registered parent_session_id, stable idempotency_key/name, explicit keep_open, workspace={existing_path}|{repo_path,branch,base_ref?}, spawn={prompt,...}. Read by launch_id; identical retry reuses resources, conflicting key errors. Uncertain creation: inspect handle, never respawn. Delivered=argv/PTY handoff, not acceptance; ready needs native Ready-screen evidence. Model/account/brief policy: caller-owned.",
    ),
    (
        "spawn",
        "spawn: loopback or authenticated PTY, named before prompt; returns task_id.",
    ),
    (
        "task_get",
        "task_get: caller-owned outcome retained 24h in-process across reconnect; working|input_required|completed|failed|cancelled, last three immutable. Poll no faster than poll_interval_ms.",
    ),
    (
        "task_cancel",
        "task_cancel: loopback or authenticated, caller-owned; final status unchanged, does NOT kill PTY (session kill does).",
    ),
    (
        "register",
        "register: optional stable UUID (default MCP-scoped); reconnect with a NEW UUID must pass replaces or strand mail. Live-PTY identity cannot be superseded: check mail_stranded/identity_warning. Without a terminal use wait/inbox (subscribed ACP can wake idle ego). orchestrator=true/false sets/removes role; omission preserves it, spawning a child never infers it. mail_wake=managed_pty_lifecycle identifies reachable PTY wake.",
    ),
    (
        "list_peers",
        "list_peers: local/remote peers; optional to/path/connection_id/compact, absent fields omitted; is_caller=true marks your own row.",
    ),
    (
        "send",
        "send: optional report={state:ready|waiting|blocked|failed|dropped|progress,wait_for?:boss|review|dependency,question?:{id,text,options?:[{id,label}],recommended_option_id?}}. waiting requires wait_for; question only waiting/boss. ready means waiting_on_review; progress alone has no stage override. Session rows expose stage, stage_reason, report, report_at, report_message_id. to/message required; orchestrator (parent chain, else unique role in your path; missing/ambiguous errors), UUID, PTY, alias/name or connection/peer_id. Normal is the default; urgent when the recipient must change course before its next step. Normal mail keeps the payload in the inbox and may submit a payload-free idle wake. urgency=urgent and urgency=interrupt share forced steering: send Esc to a safe working Claude/Codex/ego, waits up to 5 s for idle, then submits the full message as one input; an idle recipient skips Esc. Dialogs/approvals and drafts block it; unsupported agents or timeout stay inbox-only without a queued fallback. Inspect urgent_delivered, urgent_fallback_reason, interrupt_delivered, interrupt_fallback_reason and delivery_path; interrupt_payload_and_inbox proves input written, not model action. Interrupted tools may leave partial work or running children. Active waiters own delivery.",
    ),
    (
        "inbox",
        "inbox: FIFO up to 100, omitted since resumes server cursor, since=0 deliberately replays; follow has_more. Byte-budget chunks (envelope included): concatenate/parse, same action/token; expire after 300s/eviction. Cursor advances only on final chunk; fresh read after error; missed_count reports unread eviction.",
    ),
    (
        "wait",
        "wait: Block for fresh mail inline, next_since even on timeout; active wait suppresses terminal wake. byte_budget/continuation rejected before consumption.",
    ),
    (
        "subscribe",
        "subscribe: independent durable subscription_id, immutable actionable|all|exit|failure filter; actionable excludes routine lifecycle, never peer mail. Subscription inbox/wait: limit 1..100, no byte_budget/continuation or since; reads do NOT acknowledge; an unacked batch replays unchanged; ACK before following has_more. Matching mail/retention loss alone wakes wait.",
    ),
    (
        "ack",
        "ack: processed batch cursor; same identity/id survives reconnect; deduplicate side effects by message id before ack. Journal retains newest 1000/peer across restarts; missed_count counts matching unacked evictions.",
    ),
    (
        "unsubscribe",
        "unsubscribe: remove reader only, not mail/other readers.",
    ),
];

fn guide_for(keep: impl Fn(&str) -> bool) -> String {
    AGENT_GUIDE
        .iter()
        .filter(|(action, _)| action.is_empty() || keep(action))
        .map(|(_, text)| *text)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The full compact description every spawner receives.
pub(super) static AGENT_DESCRIPTION: LazyLock<String> = LazyLock::new(|| guide_for(|_| true));

/// Narrow the generated `agent` definition to a role. Runs on the private
/// validation definition, before the advertised schema is derived from it.
pub(super) fn restrict_agent_tool(tool: &mut Value, role: CallerRole) {
    let Some(actions) = role_actions(role) else {
        return;
    };
    if tool["name"] != "agent" {
        return;
    }
    let allowed = |action: &str| actions.contains(&action);
    tool["description"] = json!(guide_for(allowed));
    let schema = &mut tool["inputSchema"];
    let action = &mut schema["properties"]["action"];
    if let Some(actions) = action["enum"].as_array_mut() {
        actions.retain(|name| name.as_str().is_some_and(allowed));
        let names: Vec<&str> = actions.iter().filter_map(Value::as_str).collect();
        action["description"] = json!(format!("One of: {}", names.join(", ")));
    }
    let fields: Vec<&str> = AGENT_ACTION_FIELDS
        .iter()
        .filter(|(name, _)| allowed(name))
        .flat_map(|(_, fields)| fields.iter().copied())
        .collect();
    if let Some(properties) = schema["properties"].as_object_mut() {
        properties.retain(|name, _| name == "action" || fields.contains(&name.as_str()));
    }
    if let Some(conditions) = schema["allOf"].as_array_mut() {
        conditions.retain_mut(|condition| {
            let selector = &mut condition["if"]["properties"]["action"];
            if let Some(single) = selector["const"].as_str() {
                return allowed(single);
            }
            let Some(actions) = selector["enum"].as_array_mut() else {
                return true;
            };
            actions.retain(|name| name.as_str().is_some_and(allowed));
            !actions.is_empty()
        });
    }
}
