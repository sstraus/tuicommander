mod api;
mod definition;
mod prompt;
mod run;
mod store;

pub use api::*;
pub use definition::*;
pub use prompt::*;
pub use run::*;
pub use store::*;

#[cfg(test)]
mod critic_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn story_graph() -> WorkflowGraph {
        WorkflowGraph {
            nodes: vec![
                Node {
                    id: "start".into(),
                    kind: NodeKind::Start,
                },
                Node {
                    id: "work".into(),
                    kind: NodeKind::Agent {
                        role: AgentRole::Implementer,
                        capabilities: vec!["story_read".into(), "story_report".into()],
                        prompt_template: "Implement {{story.title}}".into(),
                    },
                },
                Node {
                    id: "end".into(),
                    kind: NodeKind::End,
                },
            ],
            edges: vec![
                Edge {
                    from: "start".into(),
                    to: "work".into(),
                    outcome: None,
                },
                Edge {
                    from: "work".into(),
                    to: "end".into(),
                    outcome: None,
                },
            ],
        }
    }

    #[test]
    fn validation_rejects_unreachable_outcomes_joins_capabilities_and_cycles() {
        let mut graph = story_graph();
        graph.nodes.push(Node {
            id: "lost".into(),
            kind: NodeKind::Notify,
        });
        assert!(validate_graph(&graph, WorkflowKind::Story).is_err());

        let mut graph = story_graph();
        graph.nodes[1].kind = NodeKind::Judge;
        assert!(validate_graph(&graph, WorkflowKind::Story).is_err());

        let mut graph = story_graph();
        graph.nodes[1].kind = NodeKind::Join {};
        assert!(validate_graph(&graph, WorkflowKind::Plan).is_err());

        let mut graph = story_graph();
        if let NodeKind::Agent { capabilities, .. } = &mut graph.nodes[1].kind {
            capabilities.push("shell_exec".into());
        }
        assert!(validate_graph(&graph, WorkflowKind::Story).is_err());

        let mut graph = story_graph();
        graph.edges.push(Edge {
            from: "work".into(),
            to: "start".into(),
            outcome: None,
        });
        assert!(validate_graph(&graph, WorkflowKind::Story).is_err());

        let mut graph = story_graph();
        graph.nodes[1].kind = NodeKind::Loop { max_iterations: 0 };
        assert!(validate_graph(&graph, WorkflowKind::Story).is_err());
    }

    fn unit_check() -> CheckDefinition {
        CheckDefinition {
            id: "unit".into(),
            argv: vec!["git".into(), "status".into()],
            timeout_secs: 30,
        }
    }

    #[test]
    fn story_publish_requires_a_check_but_plan_publish_does_not() {
        let dir = tempfile::tempdir().expect("db dir");
        let store = WorkflowStore::open_at(&dir.path().join("workflow.sqlite3")).expect("store");
        let story = store
            .create_draft("/project", "Delivery", WorkflowKind::Story, story_graph())
            .expect("draft");
        // catches: a story workflow with zero checks publishing and later vacuously releasing dependents.
        let error = store
            .publish(&story.id, story.draft_revision)
            .expect_err("empty checks must not publish");
        assert!(error.contains("required check"));
        let checked = store
            .update_checks(&story.id, story.draft_revision, vec![unit_check()])
            .expect("checks");
        assert_eq!(
            store
                .publish(&story.id, checked.draft_revision)
                .expect("publish")
                .required_checks,
            vec![unit_check()]
        );
        // GREEN: plan workflows carry no checks and still publish.
        let plan = store
            .seed_templates("/project")
            .expect("seed")
            .into_iter()
            .find(|draft| draft.kind == WorkflowKind::Plan)
            .expect("plan template");
        let edited = store
            .update_draft(&plan.id, plan.draft_revision, plan.graph.clone())
            .expect("edit plan");
        store
            .publish(&plan.id, edited.draft_revision)
            .expect("plan publishes without checks");
    }

    #[test]
    fn seeded_story_delivery_ships_a_check_policy() {
        let dir = tempfile::tempdir().expect("db dir");
        let store = WorkflowStore::open_at(&dir.path().join("workflow.sqlite3")).expect("store");
        let templates = store.seed_templates("/project").expect("seed");
        let story = templates
            .iter()
            .find(|draft| draft.kind == WorkflowKind::Story)
            .expect("story template");
        // catches: seeds inserted with an empty policy, so every seeded run integrates unchecked.
        assert_ne!(
            story.required_checks,
            [] as [crate::workflows::store::CheckDefinition; 0]
        );
        let published = store.get_published(&story.id, 1).expect("published");
        assert_eq!(published.required_checks, story.required_checks);
        let plan = templates
            .iter()
            .find(|draft| draft.kind == WorkflowKind::Plan)
            .expect("plan template");
        assert_ne!(
            plan.required_checks,
            [] as [crate::workflows::store::CheckDefinition; 0]
        );
    }

    #[test]
    fn published_revisions_are_immutable_when_a_draft_changes() {
        let dir = tempfile::tempdir().expect("db dir");
        let store = WorkflowStore::open_at(&dir.path().join("workflow.sqlite3")).expect("store");
        let draft = store
            .create_draft("/project", "Delivery", WorkflowKind::Story, story_graph())
            .expect("draft");
        let draft = store
            .update_checks(&draft.id, draft.draft_revision, vec![unit_check()])
            .expect("checks");
        let first = store
            .publish(&draft.id, draft.draft_revision)
            .expect("publish");
        assert_eq!(first.revision, 1);
        let mut changed = story_graph();
        if let NodeKind::Agent {
            prompt_template, ..
        } = &mut changed.nodes[1].kind
        {
            *prompt_template = "Revised prompt".into();
        }
        let draft = store
            .update_draft(&draft.id, draft.draft_revision, changed)
            .expect("edit draft");
        let second = store
            .publish(&draft.id, draft.draft_revision)
            .expect("publish second");
        assert_eq!(second.revision, 2);
        assert_ne!(
            store.get_published(&draft.id, 1).expect("first").graph,
            store.get_published(&draft.id, 2).expect("second").graph
        );
        assert!(store.publish(&draft.id, draft.draft_revision).is_err());
    }

    #[test]
    fn closure_defaults_to_human_and_automatic_cannot_be_published() {
        let dir = tempfile::tempdir().expect("db dir");
        let db = dir.path().join("workflow.sqlite3");
        let store = WorkflowStore::open_at(&db).expect("store");
        let draft = store
            .create_draft("/project", "Delivery", WorkflowKind::Story, story_graph())
            .expect("draft");
        assert_eq!(draft.closure, WorkflowClosure::Human);
        let draft = store
            .update_checks(&draft.id, draft.draft_revision, vec![unit_check()])
            .expect("checks");
        let first = store
            .publish(&draft.id, draft.draft_revision)
            .expect("human publish");
        assert_eq!(first.closure, WorkflowClosure::Human);
        let automatic = store
            .update_closure(&draft.id, draft.draft_revision, WorkflowClosure::Automatic)
            .expect("edit closure");
        assert!(
            store
                .update_closure(&draft.id, draft.draft_revision, WorkflowClosure::Human)
                .is_err()
        );
        assert!(
            store
                .publish(&draft.id, automatic.draft_revision)
                .expect_err("automatic is unavailable")
                .contains("automatic closure")
        );
        drop(store);
        let reopened = WorkflowStore::open_at(&db).expect("reopen");
        assert_eq!(
            reopened.get_draft(&draft.id).expect("draft").closure,
            WorkflowClosure::Automatic
        );
        assert_eq!(
            reopened
                .get_published(&draft.id, 1)
                .expect("first revision")
                .closure,
            WorkflowClosure::Human
        );
    }

    #[test]
    fn existing_workflow_database_migrates_to_human_closure() {
        let dir = tempfile::tempdir().expect("db dir");
        let db = dir.path().join("workflow.sqlite3");
        let conn = rusqlite::Connection::open(&db).expect("legacy db");
        conn.execute_batch("CREATE TABLE workflow_definitions (
            id TEXT PRIMARY KEY, project TEXT NOT NULL, name TEXT NOT NULL, kind TEXT NOT NULL,
            graph_json TEXT NOT NULL, draft_revision INTEGER NOT NULL,
            latest_published_revision INTEGER NOT NULL, last_published_draft_revision INTEGER NOT NULL,
            builtin_key TEXT);
            CREATE TABLE workflow_published (
            id TEXT NOT NULL, revision INTEGER NOT NULL, project TEXT NOT NULL,
            name TEXT NOT NULL, kind TEXT NOT NULL, graph_json TEXT NOT NULL,
            PRIMARY KEY(id,revision));").expect("legacy schema");
        conn.execute(
            "INSERT INTO workflow_definitions VALUES (?1,?2,?3,?4,?5,1,1,1,NULL)",
            rusqlite::params![
                "legacy",
                "/project",
                "Delivery",
                "story",
                serde_json::to_string(&story_graph()).expect("graph")
            ],
        )
        .expect("legacy draft");
        conn.execute(
            "INSERT INTO workflow_published VALUES (?1,1,?2,?3,?4,?5)",
            rusqlite::params![
                "legacy",
                "/project",
                "Delivery",
                "story",
                serde_json::to_string(&story_graph()).expect("graph")
            ],
        )
        .expect("legacy publication");
        drop(conn);
        let store = WorkflowStore::open_at(&db).expect("migrate");
        assert_eq!(
            store.get_draft("legacy").expect("draft").closure,
            WorkflowClosure::Human
        );
        assert_eq!(
            store.get_published("legacy", 1).expect("published").closure,
            WorkflowClosure::Human
        );
    }

    #[test]
    fn published_checks_are_pinned_and_invalid_commands_are_rejected() {
        let dir = tempfile::tempdir().expect("db dir");
        let store = WorkflowStore::open_at(&dir.path().join("workflow.sqlite3")).expect("store");
        let draft = store
            .create_draft("/project", "Delivery", WorkflowKind::Story, story_graph())
            .expect("draft");
        let checks = vec![CheckDefinition {
            id: "unit".into(),
            argv: vec!["git".into(), "status".into()],
            timeout_secs: 30,
        }];
        let edited = store
            .update_checks(&draft.id, draft.draft_revision, checks.clone())
            .expect("edit checks");
        let published = store
            .publish(&draft.id, edited.draft_revision)
            .expect("publish");
        assert_eq!(published.required_checks, checks);
        assert!(
            store
                .update_checks(&draft.id, draft.draft_revision, vec![])
                .is_err()
        );
        assert!(
            store
                .update_checks(
                    &draft.id,
                    edited.draft_revision,
                    vec![CheckDefinition {
                        id: "bad".into(),
                        argv: vec!["sh".into(), "-c".into(), "true".into()],
                        timeout_secs: 30,
                    }]
                )
                .is_err()
        );
        let changed = store
            .update_checks(&draft.id, edited.draft_revision, vec![])
            .expect("edit again");
        assert_eq!(
            changed.required_checks,
            [] as [crate::workflows::store::CheckDefinition; 0]
        );
        assert_eq!(
            store
                .get_published(&draft.id, 1)
                .expect("pinned")
                .required_checks,
            checks
        );
    }

    fn seeds(store: &WorkflowStore) -> (WorkflowDraft, WorkflowDraft) {
        let all = store.seed_templates("/project").expect("seed");
        let pick = |kind| all.iter().find(|d| d.kind == kind).expect("seed").clone();
        (pick(WorkflowKind::Story), pick(WorkflowKind::Plan))
    }

    #[test]
    fn unmodified_pre_policy_seed_is_migrated_once() {
        let dir = tempfile::tempdir().expect("db dir");
        let store = WorkflowStore::open_at(&dir.path().join("workflow.sqlite3")).expect("store");
        store
            .seed_pre_policy_templates("/project")
            .expect("legacy seed");
        let (story, plan) = seeds(&store);
        // catches: seeded projects keep an empty-check story rev 1 and every integration fails.
        assert_eq!(story.latest_published_revision, 2);
        assert_ne!(
            story.required_checks,
            [] as [crate::workflows::store::CheckDefinition; 0]
        );
        assert_eq!(
            store
                .get_published(&story.id, 2)
                .expect("rev 2")
                .required_checks,
            story.required_checks
        );
        assert_eq!(
            store
                .get_published(&story.id, 1)
                .expect("rev 1")
                .required_checks,
            [] as [crate::workflows::store::CheckDefinition; 0]
        );
        // the plan must dispatch the migrated revision or new runs still pin rev 1.
        assert_eq!(plan.latest_published_revision, 2);
        assert!(
            store
                .get_published(&plan.id, 2)
                .expect("plan rev 2")
                .graph
                .nodes
                .iter()
                .any(|node| matches!(&node.kind,
            NodeKind::StoryDispatch { story_template_id, story_revision }
            if story_template_id == &story.id && *story_revision == 2))
        );
        // GREEN: second load is a no-op.
        let (story_again, plan_again) = seeds(&store);
        assert_eq!((&story_again, &plan_again), (&story, &plan));
        assert_eq!(
            store.get_published(&plan.id, 3).unwrap_err(),
            "published workflow revision not found"
        );
    }

    #[test]
    fn edited_pre_policy_seed_is_untouched() {
        let dir = tempfile::tempdir().expect("db dir");
        let store = WorkflowStore::open_at(&dir.path().join("workflow.sqlite3")).expect("store");
        store
            .seed_pre_policy_templates("/project")
            .expect("legacy seed");
        let story = store
            .list_drafts("/project")
            .expect("list")
            .into_iter()
            .find(|d| d.kind == WorkflowKind::Story)
            .expect("story");
        let mut graph = story.graph.clone();
        if let NodeKind::Agent {
            prompt_template, ..
        } = &mut graph.nodes[1].kind
        {
            *prompt_template = "User edit".into();
        }
        let edited = store
            .update_draft(&story.id, story.draft_revision, graph)
            .expect("edit");
        let (after, plan) = seeds(&store);
        // catches: the migration overwriting a user-edited definition.
        assert_eq!(after, edited);
        assert_eq!(plan.latest_published_revision, 1);
        assert_eq!(
            after.required_checks,
            [] as [crate::workflows::store::CheckDefinition; 0]
        );
    }

    #[test]
    fn seeded_resolve_plan_pins_the_story_delivery_template() {
        let dir = tempfile::tempdir().expect("db dir");
        let store = WorkflowStore::open_at(&dir.path().join("workflow.sqlite3")).expect("store");
        let templates = store.seed_templates("/project").expect("seed");
        let again = store.seed_templates("/project").expect("seed again");
        assert_eq!(templates, again);
        let plan = templates
            .iter()
            .find(|template| template.kind == WorkflowKind::Plan)
            .expect("plan");
        let story = templates
            .iter()
            .find(|template| template.kind == WorkflowKind::Story)
            .expect("story");
        let published = store.get_published(&plan.id, 1).expect("published plan");
        assert!(published.graph.nodes.iter().any(|node| matches!(&node.kind,
            NodeKind::StoryDispatch { story_template_id, story_revision }
            if story_template_id == &story.id && *story_revision == 1)));
        assert!(
            published
                .graph
                .nodes
                .iter()
                .any(|node| matches!(node.kind, NodeKind::CreateStories))
        );
        let mut invalid = plan.graph.clone();
        for node in &mut invalid.nodes {
            if let NodeKind::StoryDispatch {
                story_template_id, ..
            } = &mut node.kind
            {
                *story_template_id = "missing-template".into();
            }
        }
        let edited = store
            .update_draft(&plan.id, plan.draft_revision, invalid)
            .expect("edit plan");
        assert!(store.publish(&plan.id, edited.draft_revision).is_err());
        assert_eq!(
            store
                .get_published(&plan.id, 1)
                .expect("original revision")
                .revision,
            1
        );
    }
}
