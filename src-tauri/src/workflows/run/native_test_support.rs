use super::check::git_output;
use super::native_host::NativeHost;
use super::native_operations::NativeContext;
use super::neutral::{NeutralLimits, NeutralStartRequest};
use super::neutral_effects::EffectCommand;
use super::neutral_execution::NeutralExecution;
use super::neutral_execution_tests::step;
use super::runtime::RuntimeOwner;
use crate::agent_capabilities::CapabilitySet;
use crate::state::AppState;
use crate::workflows::catalog::{ActivityInstance, DescriptorId, ObjectSchema};
use crate::workflows::data_binding::DataSource;
use crate::workflows::neutral_control::{ControlGraph, ControlNode};
use crate::workflows::neutral_publication::PublicationRef;
use crate::workflows::{CheckDefinition, WorkflowStore};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tokio::sync::Notify;

pub(super) struct Fixture {
    pub dir: tempfile::TempDir,
    pub state: Arc<AppState>,
    pub host: Arc<NativeHost>,
    pub context: NativeContext,
    pub owner: Arc<RuntimeOwner>,
}
impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let repo = dir.path().join("checkout");
        std::fs::create_dir(&repo).unwrap();
        for args in [
            vec!["init", "-b", "fixture"],
            vec!["config", "user.name", "Native fixture"],
            vec!["config", "user.email", "native@example.invalid"],
            vec!["config", "commit.gpgsign", "false"],
            vec!["config", "core.hooksPath", ""],
        ] {
            git_output(&repo, &args).unwrap();
        }
        std::fs::write(repo.join("artifact.txt"), "seed").unwrap();
        git_output(&repo, &["add", "."]).unwrap();
        tuic_git::git::git_commit_blocking(
            repo.to_string_lossy().into_owned(),
            "seed".into(),
            Some(false),
        )
        .unwrap();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.agent_capabilities.insert(
            "trusted".into(),
            CapabilitySet::from_spawn_arg(&json!(["push"])).unwrap(),
        );
        let host = NativeHost::install(&state).unwrap();
        host.install_project_checks(
            &repo,
            vec![CheckDefinition {
                id: "exact-tree".into(),
                argv: vec!["git".into(), "diff".into(), "--exit-code".into()],
                timeout_secs: 30,
            }],
        )
        .unwrap();
        let context = NativeContext::capture(
            &state,
            super::NativeCaller::Agent("trusted".into()),
            &repo,
            vec!["exact-tree".into()],
        )
        .unwrap();
        let owner = RuntimeOwner::acquire(&dir.path().join("runs.sqlite3")).unwrap();
        owner.neutral_host(host.clone());
        Self {
            dir,
            state,
            host,
            context,
            owner,
        }
    }
    pub fn start(&self, operation: &str, request: Value) -> NeutralExecution {
        self.start_custom(operation, request, |_| {})
    }
    pub fn start_custom(
        &self,
        operation: &str,
        request: Value,
        customize: impl FnOnce(&mut crate::workflows::neutral_publication::NeutralDefinition),
    ) -> NeutralExecution {
        self.start_identified(
            operation,
            request,
            uuid::Uuid::new_v4().to_string(),
            customize,
        )
    }
    pub fn start_triggered(
        &self,
        operation: &str,
        request: Value,
        receipt: &str,
    ) -> NeutralExecution {
        self.start_identified(operation, request, format!("trigger:{receipt}"), |_| {})
    }
    fn start_identified(
        &self,
        operation: &str,
        request: Value,
        request_id: String,
        customize: impl FnOnce(&mut crate::workflows::neutral_publication::NeutralDefinition),
    ) -> NeutralExecution {
        let (_dir, _owner, template, _) = super::neutral_execution_tests::fixture(20, false);
        let catalog = crate::workflows::native_operation_catalog().unwrap();
        let id = DescriptorId {
            namespace: "native".into(),
            name: operation.into(),
            version: 1,
        };
        let descriptor = catalog.get(&id).unwrap();
        let mut definition = template.publication.definition;
        definition.inputs = descriptor.inputs.clone();
        definition.subject = descriptor.subject.clone();
        definition.outputs = ObjectSchema::default();
        definition.output_bindings.clear();
        definition.outcomes = BTreeSet::from(["done".into()]);
        definition.instances = vec![ActivityInstance {
            descriptor: id,
            config: BTreeMap::new(),
            execution_policy: "native".into(),
        }];
        definition.input_bindings = BTreeMap::from([(
            0,
            BTreeMap::from([(
                "request".into(),
                DataSource::WorkflowInput {
                    field: "request".into(),
                },
            )]),
        )]);
        definition.conditions.clear();
        definition.collections.clear();
        definition.forks.clear();
        definition.loops.clear();
        definition.control = ControlGraph {
            nodes: BTreeMap::from([
                ("start".into(), ControlNode::Start),
                ("work".into(), ControlNode::Activity { activity: 0 }),
                (
                    "done".into(),
                    ControlNode::End {
                        outcome: "done".into(),
                    },
                ),
            ]),
            edges: BTreeMap::from([
                (
                    "start".into(),
                    BTreeMap::from([("next".into(), "work".into())]),
                ),
                (
                    "work".into(),
                    BTreeMap::from([("ok".into(), "done".into())]),
                ),
            ]),
        };
        customize(&mut definition);
        let project = self.context.workspace_path().to_string_lossy().into_owned();
        let workflows = WorkflowStore::open_at(
            &self
                .dir
                .path()
                .join(format!("{}.sqlite3", uuid::Uuid::new_v4())),
        )
        .unwrap();
        let handlers = definition
            .instances
            .iter()
            .map(|instance| catalog.get(&instance.descriptor).unwrap().handler.clone())
            .collect();
        let publication = workflows
            .save_neutral_publication(&project, "native", 0, definition, &catalog, &handlers)
            .unwrap();
        let run = self
            .owner
            .start_neutral(
                &workflows,
                &NeutralStartRequest {
                    publication: PublicationRef {
                        project: project.clone(),
                        id: publication.id,
                        revision: publication.revision,
                    },
                    request_id,
                    inputs: BTreeMap::from([("request".into(), request)]),
                    subject: Some(BTreeMap::from([(
                        "workspace".into(),
                        json!({"reference":"workspace","id":project}),
                    )])),
                    limits: NeutralLimits {
                        max_activations: 20,
                        max_concurrency: 1,
                        max_duration_ms: 600_000,
                    },
                },
                &catalog,
                &handlers,
            )
            .unwrap();
        self.host
            .bind_run(&self.owner.store, &run.id, self.context.clone())
            .unwrap();
        let run = self.owner.store.neutral_execution(&run.id).unwrap();
        let at = super::store::now_ms();
        let run = step(&self.owner, &run, "enter", at);
        step(&self.owner, &run, "prepare", at)
    }
    pub async fn dispatch(&self, run: &NeutralExecution) -> NeutralExecution {
        let notify = Arc::new(Notify::new());
        if self.owner.dispatch_neutral(run, &notify).unwrap() {
            let after = self.owner.store.neutral_execution(&run.run.id).unwrap();
            if after
                .effects
                .values()
                .any(|effect| effect.dispatched && effect.observation.is_none())
            {
                notify.notified().await;
            }
        }
        self.owner.store.neutral_execution(&run.run.id).unwrap()
    }
    pub fn approve(&self, run: &NeutralExecution) -> NeutralExecution {
        let effect = run.effects.values().next().unwrap();
        self.owner
            .neutral_effect_command(
                &run.run.id,
                EffectCommand::Answer {
                    key: effect.key.clone(),
                    revision: effect.question.as_ref().unwrap().revision,
                    approved: true,
                },
            )
            .unwrap()
    }
}
