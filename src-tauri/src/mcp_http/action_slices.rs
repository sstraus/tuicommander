//! Action-scoped discovery for the multiplexed native tools (Adaptive Speakeasy).
//!
//! `agent`, `session`, `repo` and `ui` document many actions in one definition, so
//! discovering one action used to load all of them. Discovery now serves one action
//! at a time: the tool's always-on directives, that action's own guidance and only
//! the fields that action reads. Dispatch does not change: `call_tool` with
//! `tool_name=<tool>` and `arguments={action, ...}` reaches the same handler and the
//! same private validator, so a slice never grants or narrows anything.

use std::{collections::HashMap, sync::LazyLock};

use serde_json::{Value, json};

/// Description segments: the actions a segment documents, then its exact text.
/// An empty action list marks a directive that every action keeps. The segments
/// concatenate to the advertised description, byte for byte.
pub(super) type Guide = &'static [(&'static [&'static str], &'static str)];

/// The fields each action reads. Every advertised field belongs to at least one action.
type Fields = &'static [(&'static str, &'static [&'static str])];

/// The tools discovery serves per action.
pub(super) const SLICED_TOOLS: &[&str] = &["agent", "session", "repo", "ui"];

pub(super) const SESSION_GUIDE: Guide = &[
    (
        &[],
        "PTY multiplexer (tmux): input=send-keys, output=capture-pane. help: full usage and response guide. All session_id actions accept PTY id, tuic_session, alias, unique short PTY prefix or unique display name; ambiguity errors.\n",
    ),
    (
        &["list"],
        "list: local and connected remote inventory; optional session_id/compact. All active sessions and states in one call; never fan out per-session status calls. Optional fields are omitted, not null; background_work/standby only when true. completed requires a suggest marker.\n",
    ),
    (&["create"], "create: new PTY. "),
    (
        &["submit"],
        "submit: one non-empty command to a confirmed-idle managed agent; bounded receipt. Use one call; never split text and Enter; never poll after it. Never queues; partial composers, dialogs, busy agents, and older queued commands reject before writing. Acknowledgement means child terminal movement after Enter, not semantic application acceptance. composer_state (tracked InputLineBuffer, not application state). ",
    ),
    (
        &["force_retry"],
        "force_retry: submit /retry once only during ego provider recovery; never queues or retries automatically. ",
    ),
    (
        &["input"],
        "input: type into any terminal (send-keys): text and/or special_key, e.g. run a shell command with input=<command> and special_key=enter. Raw surface: ok confirms the PTY write only, not that the command ran; use submit for an agent's composer.\n",
    ),
    (
        &["output"],
        "output: bounded retained terminal output; anomaly fallback only when a child failed to send its result, not normal orchestration. Tail hides empty composer/HUD; raw preserves it. Byte-budget chunks bound compact envelope bytes, not MCP framing; concatenate chunk values then parse JSON. Continue with same action/session and opaque token; expires after 300s/eviction. Follow has_more/next_cursor without rerunning commands; raw cursors are source bytes. Evicted output is unrecoverable.\n",
    ),
    (&["status"], "- status: state. "),
    (
        &["wait"],
        "wait: server-side idle/exited wait, not a polling loop. ",
    ),
    (&["resize"], "resize: dimensions. "),
    (
        &["rename"],
        "rename: non-empty display name; sticky unless is_custom=false. \n",
    ),
    (
        &["keep_open"],
        "- keep_open: managed child enabled=true disables idle closure; enabled=false restores it. ",
    ),
    (
        &["suspend"],
        "suspend: end PTY/agent, keep restorable tab; refused while working, awaiting answer or running a command; not SIGSTOP standby. ",
    ),
    (&["close"], "close: graceful shutdown. "),
    (&["kill"], "kill: SIGKILL only if close fails. "),
    (&["pause"], "pause: stop buffering. "),
    (&["resume"], "resume: restart buffering. "),
    (
        &["declare_worktree"],
        "declare_worktree: authenticated live caller only, existing linked worktree in its repository, no foreign session; persists placement without changing cwd; MCP-only via POST /mcp. ",
    ),
    (
        &[],
        "Executing/destructive session actions require a loopback or authenticated caller.",
    ),
];

pub(super) const REPO_GUIDE: Guide = &[
    (
        &[],
        "Repositories and version control. help: full response and navigation guide.\n",
    ),
    (
        &["add"],
        "add: absolute existing local directory; register/activate Git or shell workspace, preserve settings/terminals on repeats, no terminal spawn. ",
    ),
    (&["list"], "list: open repos. "),
    (&["active"], "active: focused repo. "),
    (&["status"], "status: cross-repo PR/CI summary. "),
    (
        &["branch_integrations"],
        "branch_integrations: all local branch proofs (path required). ",
    ),
    (
        &["branch_integration"],
        "branch_integration: one branch (path/branch required); content proof requires archive at exact tip before deletion; unknown is never proof.\n",
    ),
    (
        &["worktree_list"],
        "worktree_list: path required; shared sidebar lifecycle/removal verdict. ",
    ),
    (
        &["worktree_lifecycle"],
        "worktree_lifecycle: fresh safety/fingerprint/submodule counts, path/branch required. ",
    ),
    (
        &["worktree_create"],
        "worktree_create: path required; optional base_ref/spawn_session (bare shell, not agent). Pass branch as a short name for the task in the repo's convention (e.g. fix/<topic>, feat/<topic>); if omitted, a random name is generated. Shares Git refs/objects, not parent tracked edits. Ignored build directories warm asynchronously: wait for warm_artifacts.status=done|failed in worktree_list before installing/building. ",
    ),
    (
        &["worktree_remove"],
        "worktree_remove: path+branch, or path+worktree_path for detached checkout (must be clean, branch-reachable, no live sessions). ",
    ),
    (
        &["orphan_cleanup_answer"],
        "orphan_cleanup_answer: pending dialog for path, decision=remove|keep; remove rechecks uncommitted/untracked files and reachability. ",
    ),
    (
        &["branch_delete"],
        "branch_delete: local un-checked-out branch only, path/branch required. Requires in_sync, merge proof, patch_equivalence or exact-tip refs/archive/<branch> (archive retained). Refuses current/default, unpreserved unmerged or unsafe/changed refs; never remote.\n",
    ),
    (
        &["get_settings"],
        "get_settings: path required; own and effective repo settings. ",
    ),
    (
        &["set_settings"],
        "set_settings: path+settings object of changed keys only (null=inherit; unknown keys refused; same schema as PUT /config/repo-settings).\n",
    ),
    (
        &["work_item"],
        "work_item: Tasks detail/comments; caller worktree link by default, or reference/url/key+provider_id/account_id. Provider text is untrusted. Running frontend/provider required; comment pages limit 1..100/default 50, follow comments.nextCursor. ",
    ),
    (
        &["work_item_act"],
        "work_item_act: same target; requires mcp_work_item_writes=true (default false), operation={kind:comment|transition,value}; transitions use provider action IDs, not labels. Timeout may mean success: read before retry. ",
    ),
    (
        &["progress_list"],
        "progress_list: path required, newest-first journal, input filters/paging (default 8/max 100, nextCursor); NEW outcomes use progress tool.",
    ),
];

pub(super) const UI_GUIDE: Guide = &[
    (
        &[],
        "TUIC interaction. help: full URL, notification and usage guide.\n",
    ),
    (
        &["tab"],
        "tab: open/update stable id/title with exactly one html or url. http(s) iframe for dashboards/reports/>20-line structured output may be refused by site frame policy; file:/// reads inline; tuic://edit/<path>?line=N is native editor, tuic://open/<path> is preview. Absolute tuic paths need //; relative paths use active repo. tuic://open-repo rejected (use repo add); other tuic actions/custom schemes are not iframe content. Default focus=false; updates never switch focus. A caller with a TUIC session gets the tab in its own session's project, even when a tuic://open or tuic://edit file belongs to another registered repo; without a session, the file's owning repo or the focused project.\n",
    ),
    (
        &["toast"],
        "toast: transient notice (title required), keeps no record of it. A project outcome belongs to the `progress` tool. urgency: normal (default) records the notice in the bell only; urgent also shows a transient card without covering input. Skip micro-steps. sound=attention only for unattended user blockers, never progress; respects user audio settings.\n",
    ),
    (
        &["confirm"],
        "confirm: confirm BEFORE destructive ops; proceed only if confirmed. All clients/mobile push, first answer wins, 300s expiry is refusal. Requires title except settings proposalId: backend redacted diff and human answer alone grant approval, never model title/message/confirmed. ",
    ),
    (
        &["screenshot"],
        "screenshot: panel WebP, requires id; screenshot to visually verify your rendered HTML, read returned path.\n",
    ),
    (
        &["design_start"],
        "design_start: bind a live terminal agent (session_id), optional browser/url; desktop host required, the browser extension loads automatically. ",
    ),
    (
        &["design_status"],
        "design_status: list bindings for an armed caller. ",
    ),
    (
        &["design_stop"],
        "design_stop: stop the caller-bound repository (repo_path). ",
    ),
    (
        &["design_reload"],
        "design_reload: reload the reviewed tab (repo_path). ",
    ),
    (
        &["design_navigate"],
        "design_navigate: open HTTP(S) url (repo_path); remote hosts require sidebar host consent. ",
    ),
    (
        &["design_screenshot"],
        "design_screenshot: reviewed-tab capture (repo_path). WebP (default, quality 25) for text and layout; quality 50+ for small glyphs and icons; PNG for pixel-exact detail. Optional format=webp|png, quality=1..100 (WebP only), max_long_edge pixels downsizes before encoding. Returns image, path, format, quality (null for PNG), bytes, mediaType and downscaled (over 512 KiB). Token cost follows pixel dimensions, not format or bytes (Claude: about 1,296 tokens per megapixel, capped at 1,568 standard / 4,784 high-res). ",
    ),
    (
        &["design_errors"],
        "design_errors: captured page errors (repo_path). ",
    ),
    (
        &["design_eval"],
        "design_eval: run script (repo_path), sidebar host consent required; next page via result_offset=next_offset. ",
    ),
    (
        &[
            "design_start",
            "design_status",
            "design_stop",
            "design_reload",
            "design_navigate",
            "design_screenshot",
            "design_errors",
            "design_eval",
        ],
        "Design actions retain armed caller/repository checks and Design Mode capability denials.",
    ),
];

const SESSION_ACTION_FIELDS: Fields = &[
    ("list", &["session_id", "compact", "connection_id"]),
    ("create", &["cwd", "shell", "rows", "cols"]),
    (
        "submit",
        &[
            "session_id",
            "input",
            "pty_description",
            "timeout_ms",
            "connection_id",
        ],
    ),
    ("force_retry", &["session_id"]),
    (
        "input",
        &[
            "session_id",
            "input",
            "special_key",
            "pty_description",
            "wait_ms",
        ],
    ),
    (
        "output",
        &[
            "session_id",
            "limit",
            "from_line",
            "from_byte",
            "format",
            "literal",
            "since_cursor",
            "byte_budget",
            "continuation",
            "connection_id",
        ],
    ),
    ("resize", &["session_id", "rows", "cols"]),
    ("rename", &["session_id", "name", "is_custom"]),
    ("keep_open", &["session_id", "enabled"]),
    ("suspend", &["session_id"]),
    ("close", &["session_id"]),
    ("kill", &["session_id"]),
    ("pause", &["session_id"]),
    ("resume", &["session_id"]),
    ("status", &["session_id"]),
    ("wait", &["session_id", "until", "timeout_ms"]),
    ("declare_worktree", &["worktree_path"]),
];

const REPO_ACTION_FIELDS: Fields = &[
    ("add", &["path"]),
    ("branch_integrations", &["path"]),
    ("branch_integration", &["path", "branch"]),
    ("worktree_list", &["path"]),
    ("worktree_lifecycle", &["path", "branch"]),
    (
        "worktree_create",
        &["path", "branch", "base_ref", "spawn_session"],
    ),
    (
        "worktree_remove",
        &[
            "path",
            "branch",
            "worktree_path",
            "force",
            "delete_branch",
            "override_lock",
            "expected_fingerprint",
        ],
    ),
    ("orphan_cleanup_answer", &["path", "decision"]),
    ("branch_delete", &["path", "branch"]),
    ("progress_list", &["path", "input"]),
    ("get_settings", &["path"]),
    ("set_settings", &["path", "settings"]),
    (
        "work_item",
        &[
            "reference",
            "url",
            "key",
            "provider_id",
            "account_id",
            "cursor",
            "limit",
        ],
    ),
    (
        "work_item_act",
        &[
            "reference",
            "url",
            "key",
            "provider_id",
            "account_id",
            "operation",
        ],
    ),
];

const UI_ACTION_FIELDS: Fields = &[
    ("tab", &["id", "title", "html", "url", "pinned", "focus"]),
    ("toast", &["title", "message", "level", "urgency", "sound"]),
    ("confirm", &["title", "message", "proposalId"]),
    ("screenshot", &["id"]),
    ("design_start", &["session_id", "browser", "url"]),
    ("design_stop", &["repo_path"]),
    ("design_reload", &["repo_path"]),
    ("design_navigate", &["repo_path", "url"]),
    (
        "design_screenshot",
        &["repo_path", "format", "quality", "max_long_edge"],
    ),
    ("design_errors", &["repo_path"]),
    ("design_eval", &["repo_path", "script", "result_offset"]),
];

/// `agent` keeps its role-aware guide in `caller_role`; its preamble key is "".
static AGENT_SEGMENTS: LazyLock<Vec<(&'static [&'static str], &'static str)>> =
    LazyLock::new(|| {
        super::caller_role::AGENT_GUIDE
            .iter()
            .map(|(action, text)| {
                let actions: &'static [&'static str] = if action.is_empty() {
                    &[]
                } else {
                    std::slice::from_ref(action)
                };
                (actions, *text)
            })
            .collect()
    });

fn guide(tool: &str) -> Option<&'static [(&'static [&'static str], &'static str)]> {
    match tool {
        "agent" => Some(AGENT_SEGMENTS.as_slice()),
        "session" => Some(SESSION_GUIDE),
        "repo" => Some(REPO_GUIDE),
        "ui" => Some(UI_GUIDE),
        _ => None,
    }
}

pub(super) fn action_fields(tool: &str) -> Option<Fields> {
    match tool {
        "agent" => Some(super::caller_role::AGENT_ACTION_FIELDS),
        "session" => Some(SESSION_ACTION_FIELDS),
        "repo" => Some(REPO_ACTION_FIELDS),
        "ui" => Some(UI_ACTION_FIELDS),
        _ => None,
    }
}

fn concatenated(guide: Guide) -> String {
    guide.iter().map(|(_, text)| *text).collect()
}

static SESSION_DESCRIPTION: LazyLock<String> = LazyLock::new(|| concatenated(SESSION_GUIDE));
static REPO_DESCRIPTION: LazyLock<String> = LazyLock::new(|| concatenated(REPO_GUIDE));
static UI_DESCRIPTION: LazyLock<String> = LazyLock::new(|| concatenated(UI_GUIDE));

/// The complete advertised description of a segmented tool other than `agent`.
pub(super) fn full_description(tool: &str) -> Option<&'static str> {
    match tool {
        "session" => Some(SESSION_DESCRIPTION.as_str()),
        "repo" => Some(REPO_DESCRIPTION.as_str()),
        "ui" => Some(UI_DESCRIPTION.as_str()),
        _ => None,
    }
}

/// Fields each action requires, from the unconditional `then.required` rules of
/// the private contract. Alternatives (`anyOf`/`oneOf`) stay with the validator.
static REQUIRED: LazyLock<HashMap<(String, String), Vec<String>>> = LazyLock::new(|| {
    let mut required: HashMap<(String, String), Vec<String>> = HashMap::new();
    let tools = super::mcp_transport_catalogue::native_validation_tool_definitions();
    for tool in tools.as_array().into_iter().flatten() {
        let name = tool["name"].as_str().unwrap_or_default();
        if !SLICED_TOOLS.contains(&name) {
            continue;
        }
        for condition in tool["inputSchema"]["allOf"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let selector = &condition["if"]["properties"]["action"];
            let actions = selector["enum"]
                .as_array()
                .cloned()
                .unwrap_or_else(|| selector.get("const").cloned().into_iter().collect());
            let Some(fields) = condition["then"]["required"].as_array() else {
                continue;
            };
            for action in actions.iter().filter_map(Value::as_str) {
                let entry = required
                    .entry((name.to_owned(), action.to_owned()))
                    .or_default();
                for field in fields.iter().filter_map(Value::as_str) {
                    if !entry.iter().any(|known| known == field) {
                        entry.push(field.to_owned());
                    }
                }
            }
        }
    }
    required
});

/// The discovery name of one action: `<tool>_<action>`, e.g. `agent_send`. Clients
/// cache admitted schemas by tool name, so every action needs a name of its own.
pub(super) fn action_tool_name(tool: &str, action: &str) -> String {
    format!("{tool}_{action}")
}

/// Split an action's discovery name into its tool and action. The action is not
/// checked here: the caller's filtered definition decides whether it exists.
pub(super) fn split_action_tool_name(name: &str) -> Option<(&'static str, &str)> {
    SLICED_TOOLS.iter().find_map(|tool| {
        let action = name.strip_prefix(tool)?.strip_prefix('_')?;
        (!action.is_empty()).then_some((*tool, action))
    })
}

/// One action of an advertised (already role- and state-filtered) definition, or
/// `None` when the tool is not segmented or the caller may not use the action.
pub(super) fn action_slice(definition: &Value, action: &str) -> Option<Value> {
    let tool = definition["name"].as_str()?;
    let guide = guide(tool)?;
    let fields = action_fields(tool)?;
    let schema = &definition["inputSchema"];
    if !schema["properties"]["action"]["enum"]
        .as_array()?
        .iter()
        .any(|known| known == action)
    {
        return None;
    }
    let description = guide
        .iter()
        .filter(|(actions, _)| actions.is_empty() || actions.contains(&action))
        .map(|(_, text)| text.trim())
        .collect::<Vec<_>>()
        .join("\n");
    // The name carries the action; `call_tool` supplies it to the handler.
    let mut properties = serde_json::Map::new();

    let reads = fields
        .iter()
        .find(|(name, _)| *name == action)
        .map_or(&[][..], |(_, reads)| *reads);
    for field in reads {
        if let Some(property) = schema["properties"].get(*field) {
            properties.insert((*field).to_owned(), property.clone());
        }
    }
    let mut required: Vec<Value> = Vec::new();
    if let Some(fields) = REQUIRED.get(&(tool.to_owned(), action.to_owned())) {
        required.extend(
            fields
                .iter()
                .filter(|field| properties.contains_key(field.as_str()))
                .map(|field| json!(field)),
        );
    }
    Some(json!({
        "name": action_tool_name(tool, action),
        "description": description,
        "inputSchema": {"type": "object", "properties": properties, "required": required},
        // Strict clients validate a discovered schema as an MCP Tool and refuse
        // unknown top-level fields, so the action travels in `_meta`.
        "_meta": {"tuic/tool": tool, "tuic/action": action},
    }))
}

/// Index documents: a segmented tool contributes one ranked document per action
/// and keeps its whole definition for exact selection only.
pub(super) fn index_documents(tools: &[Value]) -> Vec<crate::tool_search::IndexDoc> {
    let mut docs = Vec::new();
    for tool in tools {
        let name = tool["name"].as_str().unwrap_or_default();
        if !SLICED_TOOLS.contains(&name) {
            docs.push(crate::tool_search::IndexDoc::whole(tool.clone()));
            continue;
        }
        docs.push(crate::tool_search::IndexDoc::unranked(tool.clone()));
        for action in tool["inputSchema"]["properties"]["action"]["enum"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if let Some(slice) = action_slice(tool, action) {
                let text = ranking_text(name, action, &slice);
                let summary = action_summary(name, action);
                docs.push(crate::tool_search::IndexDoc::action(
                    slice, action, text, summary,
                ));
            }
        }
    }
    docs
}

/// What tells one action from its siblings: its own guidance and its fields.
/// The tool's identity sentence stays in: it anchors tool-level queries.
fn ranking_text(tool: &str, action: &str, slice: &Value) -> String {
    let segments = guide(tool).unwrap_or_default();
    let identity = segments
        .first()
        .filter(|(actions, _)| actions.is_empty())
        .map(|(_, text)| text.split(". ").next().unwrap_or(text).trim().to_owned());
    let mut text: Vec<String> = identity.into_iter().collect();
    text.extend(
        segments
            .iter()
            .filter(|(actions, _)| actions.contains(&action))
            .map(|(_, text)| text.trim().to_owned()),
    );
    for (field, property) in slice["inputSchema"]["properties"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(field, _)| *field != "action")
    {
        text.push(field.replace('_', " "));
        if let Some(description) = property["description"].as_str() {
            text.push(description.to_owned());
        }
    }
    text.join(" ")
}

/// The first sentence of the action's own guidance, so results tell actions apart.
fn action_summary(tool: &str, action: &str) -> String {
    let own = guide(tool)
        .unwrap_or_default()
        .iter()
        .find(|(actions, _)| actions.contains(&action))
        .map(|(_, text)| text.trim().trim_start_matches("- "));
    match own {
        Some(text) => match text.find(". ") {
            Some(end) => text[..=end].trim().to_owned(),
            None => text.to_owned(),
        },
        None => format!("{tool} {action}"),
    }
}

/// Actions an action needs first. An external caller must register before mail;
/// shipping `agent_register` with `agent_send` saves the discovery round trip.
const PREREQUISITES: &[(&str, &str, &[&str])] = &[
    ("agent", "send", &["register"]),
    ("agent", "inbox", &["register"]),
    ("agent", "wait", &["register"]),
];

pub(super) fn prerequisites(tool: &str, action: &str) -> &'static [&'static str] {
    PREREQUISITES
        .iter()
        .find(|(owner, needing, _)| *owner == tool && *needing == action)
        .map_or(&[], |(_, _, needed)| *needed)
}
