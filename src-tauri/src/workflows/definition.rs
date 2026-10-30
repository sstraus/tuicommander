use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowKind {
    Plan,
    Story,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Coordinator,
    Planner,
    Implementer,
    Reviewer,
    Validator,
}

/// An explicit execution route is part of the immutable published definition.
/// Absence retains the historical saved-run-profile PTY route.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "transport", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentExecution {
    Pty {
        profile: String,
    },
    Acp {
        profile: String,
        model: String,
        effort: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum NodeKind {
    Start,
    Agent {
        role: AgentRole,
        capabilities: Vec<String>,
        prompt_template: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        execution: Option<AgentExecution>,
    },
    CreateStories,
    StoryDispatch {
        story_template_id: String,
        story_revision: i64,
    },
    Judge,
    Gate,
    Pause {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resume_to: Option<String>,
    },
    Loop {
        max_iterations: u16,
    },
    Join {},
    Notify,
    End,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub outcome: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkflowGraph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

fn required_outcomes(kind: &NodeKind) -> &'static [&'static str] {
    match kind {
        NodeKind::Judge => &["yes", "no", "uncertain"],
        NodeKind::Gate => &["pass", "fail"],
        NodeKind::Loop { .. } => &["repeat", "exhausted"],
        NodeKind::StoryDispatch { .. } => &["completed", "blocked"],
        _ => &[],
    }
}

/// Validate a bounded, deterministic graph before publishing or running it.
pub fn validate_graph(graph: &WorkflowGraph, workflow_kind: WorkflowKind) -> Result<(), String> {
    if graph.nodes.is_empty() || graph.nodes.len() > 128 || graph.edges.len() > 512 {
        return Err("workflow graph size is outside supported bounds".into());
    }
    let mut nodes = HashMap::new();
    let mut starts = 0;
    let mut ends = 0;
    for node in &graph.nodes {
        if node.id.is_empty()
            || node.id.len() > 80
            || !node
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(format!("invalid workflow node id: {}", node.id));
        }
        if nodes.insert(node.id.as_str(), &node.kind).is_some() {
            return Err(format!("duplicate workflow node id: {}", node.id));
        }
        match &node.kind {
            NodeKind::Start => starts += 1,
            NodeKind::End => ends += 1,
            NodeKind::CreateStories if workflow_kind != WorkflowKind::Plan => {
                return Err("Create Stories is only valid in a plan workflow".into());
            }
            NodeKind::StoryDispatch {
                story_template_id,
                story_revision,
            } => {
                if workflow_kind != WorkflowKind::Plan
                    || story_template_id.is_empty()
                    || *story_revision < 1
                {
                    return Err(
                        "Story Dispatch needs a published story template in a plan workflow".into(),
                    );
                }
            }
            NodeKind::Join {} if workflow_kind != WorkflowKind::Story => {
                return Err("Join may combine only branches of one story attempt".into());
            }
            NodeKind::Loop { max_iterations } if !(1..=100).contains(max_iterations) => {
                return Err("Loop max_iterations must be between 1 and 100".into());
            }
            NodeKind::Agent {
                capabilities,
                prompt_template,
                execution,
                ..
            } => {
                if let Some(execution) = execution {
                    let fields = match execution {
                        AgentExecution::Pty { profile } => vec![profile],
                        AgentExecution::Acp {
                            profile,
                            model,
                            effort,
                        } => vec![profile, model, effort],
                    };
                    if fields.iter().any(|value| {
                        value.is_empty()
                            || value.len() > 256
                            || value.trim() != value.as_str()
                            || value.contains(['\0', '\r', '\n'])
                    }) {
                        return Err(
                            "agent execution needs explicit bounded profile/model/effort values"
                                .into(),
                        );
                    }
                }
                if prompt_template.trim().is_empty() || prompt_template.len() > 10_000 {
                    return Err("agent prompt template is empty or too large".into());
                }
                if capabilities.len() > 16
                    || capabilities.iter().any(|capability| {
                        !matches!(
                            capability.as_str(),
                            "story_read"
                                | "story_report"
                                | "story_create"
                                | "agent_spawn"
                                | "notify"
                        )
                    })
                {
                    return Err("unsupported agent capability".into());
                }
            }
            _ => {}
        }
    }
    if starts != 1 || ends == 0 {
        return Err("workflow needs exactly one Start and at least one End".into());
    }

    let mut outgoing: HashMap<&str, Vec<&Edge>> = HashMap::new();
    let mut incoming: HashMap<&str, usize> = HashMap::new();
    let mut unique_edges = HashSet::new();
    for edge in &graph.edges {
        if !unique_edges.insert((&edge.from, &edge.to, &edge.outcome)) {
            return Err("duplicate workflow edge".into());
        }
        if !nodes.contains_key(edge.from.as_str()) || !nodes.contains_key(edge.to.as_str()) {
            return Err("workflow edge references a missing node".into());
        }
        outgoing.entry(&edge.from).or_default().push(edge);
        *incoming.entry(&edge.to).or_default() += 1;
    }
    let start = graph
        .nodes
        .iter()
        .find(|node| matches!(node.kind, NodeKind::Start))
        .expect("checked Start");
    if incoming.get(start.id.as_str()).copied().unwrap_or(0) != 0 {
        return Err("Start cannot have an incoming edge".into());
    }
    for node in &graph.nodes {
        let edges = outgoing
            .get(node.id.as_str())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let required = required_outcomes(&node.kind);
        if !required.is_empty() {
            let actual: HashSet<&str> = edges
                .iter()
                .filter_map(|edge| edge.outcome.as_deref())
                .collect();
            if edges.len() != required.len()
                || actual.len() != required.len()
                || !required.iter().all(|outcome| actual.contains(outcome))
            {
                return Err(format!(
                    "node {} has missing or duplicate outcome edges",
                    node.id
                ));
            }
        } else if matches!(node.kind, NodeKind::End | NodeKind::Pause { .. }) {
            if !edges.is_empty() {
                return Err(format!(
                    "terminal node {} cannot have an outgoing edge",
                    node.id
                ));
            }
        } else if edges.len() != 1 || edges[0].outcome.is_some() {
            return Err(format!(
                "node {} needs one unlabeled outgoing edge",
                node.id
            ));
        }
        if matches!(node.kind, NodeKind::Join {})
            && incoming.get(node.id.as_str()).copied().unwrap_or(0) < 2
        {
            return Err(format!(
                "Join {} needs at least two incoming branches",
                node.id
            ));
        }
    }

    let mut reached = HashSet::new();
    let mut queue = VecDeque::from([start.id.as_str()]);
    while let Some(id) = queue.pop_front() {
        if reached.insert(id) {
            for edge in outgoing.get(id).into_iter().flatten() {
                queue.push_back(&edge.to);
            }
        }
    }
    if reached.len() != nodes.len() {
        return Err("workflow contains unreachable nodes".into());
    }

    // A back edge is allowed only through a bounded Loop's repeat outcome.
    let non_repeat: Vec<&Edge> = graph
        .edges
        .iter()
        .filter(|edge| {
            !matches!(nodes.get(edge.from.as_str()), Some(NodeKind::Loop { .. }))
                || edge.outcome.as_deref() != Some("repeat")
        })
        .collect();
    let mut degree: HashMap<&str, usize> = nodes.keys().map(|id| (*id, 0)).collect();
    for edge in &non_repeat {
        *degree.get_mut(edge.to.as_str()).expect("known target") += 1;
    }
    let mut ready: VecDeque<&str> = degree
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(*id))
        .collect();
    let mut processed = 0;
    while let Some(id) = ready.pop_front() {
        processed += 1;
        for edge in non_repeat.iter().filter(|edge| edge.from == id) {
            let count = degree.get_mut(edge.to.as_str()).expect("known target");
            *count -= 1;
            if *count == 0 {
                ready.push_back(&edge.to);
            }
        }
    }
    if processed != nodes.len() {
        return Err("workflow has a cycle outside a bounded Loop repeat edge".into());
    }
    for edge in graph
        .edges
        .iter()
        .filter(|edge| edge.outcome.as_deref() == Some("repeat"))
    {
        if !matches!(nodes.get(edge.from.as_str()), Some(NodeKind::Loop { .. })) {
            continue;
        }
        let mut seen = HashSet::new();
        let mut queue = VecDeque::from([edge.to.as_str()]);
        while let Some(id) = queue.pop_front() {
            if seen.insert(id) {
                for next in non_repeat.iter().filter(|next| next.from == id) {
                    queue.push_back(&next.to);
                }
            }
        }
        if !seen.contains(edge.from.as_str()) {
            return Err("Loop repeat edge must return through its Loop node".into());
        }
    }
    Ok(())
}

/// Plan and story roles use the same bounded runtime; writable story coordinators are unsupported.
pub(crate) fn validate_runtime_nodes(
    graph: &WorkflowGraph,
    kind: WorkflowKind,
) -> Result<(), String> {
    for node in &graph.nodes {
        if let NodeKind::Agent { role, .. } = node.kind {
            let plan_role = matches!(role, AgentRole::Coordinator | AgentRole::Planner);
            if plan_role != (kind == WorkflowKind::Plan) {
                return Err(format!(
                    "workflow node '{}' has a role not executable for this graph; keep it as a draft",
                    node.id
                ));
            }
        }
    }
    Ok(())
}

/// Validate settings required by the versioned execution contract.
/// Legacy definitions remain readable, but need a new revision to execute.
pub fn validate_executable_graph(
    graph: &WorkflowGraph,
    kind: WorkflowKind,
    has_final_checks: bool,
) -> Result<(), String> {
    validate_graph(graph, kind)?;
    if !has_final_checks {
        return Err("executable workflows require deterministic final checks".into());
    }
    for node in &graph.nodes {
        if let NodeKind::Pause { resume_to } = &node.kind {
            let target = resume_to
                .as_ref()
                .ok_or("executable Pause needs resume_to")?;
            if !graph.nodes.iter().any(|candidate| {
                candidate.id == *target
                    && !matches!(candidate.kind, NodeKind::Start | NodeKind::Pause { .. })
            }) {
                return Err("Pause resume_to must name an executable non-Start target".into());
            }
        }
    }
    Ok(())
}
