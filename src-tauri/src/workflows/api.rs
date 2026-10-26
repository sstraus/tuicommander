use super::{
    CheckDefinition, PublishedWorkflow, WorkflowClosure, WorkflowDraft, WorkflowGraph,
    WorkflowKind, WorkflowStore,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkflowAction {
    SeedTemplates,
    CreateDraft {
        name: String,
        kind: WorkflowKind,
        graph: WorkflowGraph,
    },
    ListDrafts,
    GetDraft {
        id: String,
    },
    UpdateDraft {
        id: String,
        expected_revision: i64,
        graph: WorkflowGraph,
    },
    UpdateClosure {
        id: String,
        expected_revision: i64,
        closure: WorkflowClosure,
    },
    UpdateChecks {
        id: String,
        expected_revision: i64,
        checks: Vec<CheckDefinition>,
    },
    Publish {
        id: String,
        expected_revision: i64,
    },
    GetPublished {
        id: String,
        revision: i64,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum WorkflowReply {
    Draft(WorkflowDraft),
    Drafts(Vec<WorkflowDraft>),
    Published(PublishedWorkflow),
}

fn draft_in_project(
    store: &WorkflowStore,
    id: &str,
    project: &str,
) -> Result<WorkflowDraft, String> {
    let draft = store.get_draft(id)?;
    if draft.project != project {
        return Err("workflow does not belong to project".into());
    }
    Ok(draft)
}

fn check_draft_size(graph: &WorkflowGraph) -> Result<(), String> {
    if graph.nodes.len() > 128
        || graph.edges.len() > 512
        || serde_json::to_vec(graph)
            .map_err(|e| format!("encode workflow draft: {e}"))?
            .len()
            > 256_000
    {
        return Err("workflow draft is too large".into());
    }
    Ok(())
}

pub const WORKFLOW_VALIDATION_ERROR_PREFIX: &str = "invalid workflow definition: ";

/// True for an error the user can fix: an invalid graph or a publish with no changes.
pub fn is_user_error(message: &str) -> bool {
    message.starts_with(WORKFLOW_VALIDATION_ERROR_PREFIX)
        || message == super::PUBLISH_UNCHANGED_ERROR
}

fn validation_error(error: String) -> String {
    format!("{WORKFLOW_VALIDATION_ERROR_PREFIX}{error}")
}

/// Project-scoped definition operations shared by desktop and HTTP transports.
pub fn definition_action(project: &str, action: WorkflowAction) -> Result<WorkflowReply, String> {
    if !crate::fs::is_absolute_on_any_platform(project) {
        return Err("project must be an absolute path".into());
    }
    let project = crate::progress::resolve_owning_project(Some(project))?
        .to_string_lossy()
        .to_string();
    let store = WorkflowStore::open()?;
    match action {
        WorkflowAction::SeedTemplates => Ok(WorkflowReply::Drafts(store.seed_templates(&project)?)),
        WorkflowAction::CreateDraft { name, kind, graph } => {
            check_draft_size(&graph)?;
            Ok(WorkflowReply::Draft(
                store.create_draft(&project, &name, kind, graph)?,
            ))
        }
        WorkflowAction::ListDrafts => {
            store.seed_templates(&project)?;
            Ok(WorkflowReply::Drafts(store.list_drafts(&project)?))
        }
        WorkflowAction::GetDraft { id } => Ok(WorkflowReply::Draft(draft_in_project(
            &store, &id, &project,
        )?)),
        WorkflowAction::UpdateDraft {
            id,
            expected_revision,
            graph,
        } => {
            draft_in_project(&store, &id, &project)?;
            check_draft_size(&graph)?;
            Ok(WorkflowReply::Draft(store.update_draft(
                &id,
                expected_revision,
                graph,
            )?))
        }
        WorkflowAction::UpdateClosure {
            id,
            expected_revision,
            closure,
        } => {
            draft_in_project(&store, &id, &project)?;
            Ok(WorkflowReply::Draft(store.update_closure(
                &id,
                expected_revision,
                closure,
            )?))
        }
        WorkflowAction::UpdateChecks {
            id,
            expected_revision,
            checks,
        } => {
            draft_in_project(&store, &id, &project)?;
            Ok(WorkflowReply::Draft(store.update_checks(
                &id,
                expected_revision,
                checks,
            )?))
        }
        WorkflowAction::Publish {
            id,
            expected_revision,
        } => {
            let draft = draft_in_project(&store, &id, &project)?;
            super::definition::validate_runtime_nodes(&draft.graph, draft.kind)
                .map_err(validation_error)?;
            super::validate_executable_graph(
                &draft.graph,
                draft.kind,
                !draft.required_checks.is_empty(),
            )
            .map_err(validation_error)?;
            Ok(WorkflowReply::Published(
                store.publish(&id, expected_revision)?,
            ))
        }
        WorkflowAction::GetPublished { id, revision } => {
            let published = store.get_published(&id, revision)?;
            if published.project != project {
                return Err("workflow does not belong to project".into());
            }
            Ok(WorkflowReply::Published(published))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definition_actions_are_scoped_to_the_canonical_owner() {
        let config = tempfile::tempdir().expect("config");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let first = tempfile::tempdir().expect("first");
        let second = tempfile::tempdir().expect("second");
        let first = first.path().to_str().expect("first path");
        let second = second.path().to_str().expect("second path");
        let WorkflowReply::Drafts(templates) =
            definition_action(first, WorkflowAction::SeedTemplates).expect("seed")
        else {
            panic!("draft list");
        };
        let plan = templates
            .iter()
            .find(|draft| draft.kind == WorkflowKind::Plan)
            .expect("plan");
        assert!(
            definition_action(
                second,
                WorkflowAction::GetDraft {
                    id: plan.id.clone()
                }
            )
            .is_err()
        );
        assert!(matches!(
            definition_action(
                &format!("{first}/."),
                WorkflowAction::GetDraft {
                    id: plan.id.clone()
                }
            ),
            Ok(WorkflowReply::Draft(_))
        ));
    }
}

#[cfg(test)]
mod validation_tests {
    use super::*;

    /// Catches: policy mutations accepting a relative project path.
    #[test]
    fn policy_actions_reject_relative_projects() {
        for action in [
            WorkflowAction::CreateDraft {
                name: "draft".into(),
                kind: WorkflowKind::Plan,
                graph: WorkflowGraph {
                    nodes: vec![],
                    edges: vec![],
                },
            },
            WorkflowAction::UpdateChecks {
                id: "draft".into(),
                expected_revision: 1,
                checks: vec![],
            },
            WorkflowAction::Publish {
                id: "draft".into(),
                expected_revision: 1,
            },
        ] {
            assert_eq!(
                definition_action("relative", action).unwrap_err(),
                "project must be an absolute path"
            );
        }
    }
}
