//! Real file/SQLite recovery and ACP fixture transport; no paid provider proof.
use super::super::super::neutral_model_adapter_tests as adapter;
use super::super::super::neutral_model_request::ModelRequest;
use super::*;
use crate::acp::AcpClientManager;
use crate::workflows::catalog::{FieldSchema, ObjectSchema, ValueType, WorkflowCatalog};

fn configured(
    request: &ModelRequest,
) -> (tempfile::TempDir, Arc<RuntimeOwner>, NeutralExecution, i64) {
    super::super::super::neutral_execution_tests::fixture_with(
        30,
        false,
        |definition, catalog, _| {
            let mut descriptor = catalog.descriptors().next().unwrap().clone();
            let config: BTreeMap<String, serde_json::Value> =
                serde_json::from_value(json!(request)).unwrap();
            descriptor.config = ObjectSchema {
                fields: config
                    .iter()
                    .map(|(name, value)| {
                        let value_type = match value {
                            serde_json::Value::String(_) => ValueType::String,
                            serde_json::Value::Array(_) => ValueType::Array,
                            _ => ValueType::Object,
                        };
                        (
                            name.clone(),
                            FieldSchema {
                                value_type,
                                required: true,
                                default: None,
                            },
                        )
                    })
                    .collect(),
            };
            *catalog = WorkflowCatalog::new();
            catalog.register(descriptor).unwrap();
            definition.instances[0].config = config;
        },
    )
}

struct RecoveryHost {
    writer: Arc<FileHost>,
    owner: Arc<RuntimeOwner>,
    runtime: crate::acp::EgoAcpConfig,
    root: PathBuf,
}
impl NeutralHost for RecoveryHost {
    fn context(&self, inv: &NeutralInvocation) -> Result<Box<dyn HostContext>, String> {
        self.writer.context(inv)
    }
    fn registry(&self) -> HostRegistry<'_> {
        self.writer.registry()
    }
    fn prepare(&self, inv: &NeutralInvocation) -> Result<EffectPreparation, String> {
        self.writer.prepare(inv)
    }
    fn invoke_effect(&self, _: NeutralEffect) -> ObservationFuture {
        Box::pin(async { Err("recovery test refuses any new write dispatch".into()) })
    }
    fn reconcile(&self, effect: NeutralEffect) -> ObservationFuture {
        if effect.preparation.model_tools.is_none() {
            return self.writer.reconcile(effect);
        }
        let owner = self.owner.clone();
        let runtime = self.runtime.clone();
        let root = self.root.clone();
        Box::pin(async move {
            let outputs = owner
                .resume_model_activity(&AcpClientManager::new(), &effect, &runtime, root)
                .await?;
            Ok(EffectObservation {
                stopped: true,
                outcome: EffectOutcome::Completed {
                    result: NeutralResult::Completed {
                        outcome: "ok".into(),
                        outputs,
                    },
                },
            })
        })
    }
}

// Catches recovering the model before its write settles, losing a result at
// reopen, repeating the write, or silently failing to issue the new ACP prompt.
#[tokio::test]
async fn restart_reconciles_original_write_and_delivers_new_model_prompt() {
    let mut request = adapter::request();
    request.limits.max_context_bytes = 65_536;
    let (process, runtime) = adapter::fixture("{\"next\":7}", false);
    let (dir, owner, run, at) = configured(&request);
    let (run, call) = parent(&owner, &run, at);
    let file = dir.path().join("original-write");
    let writer = FileHost::new(&owner, file.clone(), Reply::Lost, false);
    owner.neutral_host(writer.clone());
    let child = owner
        .start_model_tool(call.clone(), BTreeMap::from([("value".into(), json!(30))]))
        .unwrap();
    tokio::task::yield_now().await;
    let stored = owner.store.neutral_execution(&run.run.id).unwrap();
    assert!(stored.model_recovery_context(&call.parent).is_err());
    let db = owner.store.database_path().to_path_buf();
    drop(writer);
    drop(owner);
    let reopened = RuntimeOwner::acquire(&db).unwrap();
    let writer = FileHost::new(&reopened, file.clone(), Reply::Lost, false);
    // Construct the expected prompt from the persisted, independently checked
    // original result. The fixture must observe it over the real ACP pipe.
    let retained = reopened.store.neutral_execution(&run.run.id).unwrap();
    let observation = writer
        .reconcile(retained.effect(&child.key).unwrap().clone())
        .await
        .unwrap();
    let mut expected = retained.clone();
    let decision = expected
        .decide_effect(
            &EffectCommand::Observe {
                key: child.key.clone(),
                observation,
            },
            at,
        )
        .unwrap()
        .unwrap();
    expected.apply_effect(&decision).unwrap();
    let context = expected.model_recovery_context(&call.parent).unwrap();
    let value: serde_json::Value = serde_json::from_str(&context).unwrap();
    assert_eq!(value["results"][0]["call"]["id"], "original-provider-call");
    assert_eq!(value["results"][0]["arguments"]["value"], 30);
    assert_eq!(value["results"][0]["result"]["outputs"]["next"], 31);
    let mut expected_request = request.clone();
    expected_request.context.push(context);
    // The mediated launch pre-approves every tool of the retained policy
    // (`--allow tool(tuicommander/<name>)`); the fixture checks that exact argv.
    let policy = retained
        .effect(&call.parent)
        .unwrap()
        .preparation
        .model_tools
        .as_ref();
    let allowed = policy
        .unwrap()
        .tools
        .keys()
        .fold(String::new(), |lines, name| {
            format!("{lines}--allow\ntool(tuicommander/{name})\n")
        });
    std::fs::write(process.path().join("expected-mediated"), allowed).unwrap();
    let scenario = process.path().join("scenario.jsonl");
    let steps = std::fs::read_to_string(&scenario)
        .unwrap()
        .lines()
        .map(|line| {
            let mut step: serde_json::Value = serde_json::from_str(line).unwrap();
            if step["method"] == "session/prompt" {
                step["params"]["prompt"] =
                    json!([{"type":"text","text":expected_request.prompt().unwrap()}]);
            }
            step.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(scenario, steps).unwrap();
    let host = Arc::new(RecoveryHost {
        writer: writer.clone(),
        owner: reopened.clone(),
        runtime,
        root: process.path().into(),
    });
    reopened.neutral_host(host.clone());
    super::super::super::runtime::run_actor(
        Arc::downgrade(&reopened),
        run.run.id.clone(),
        Arc::new(Notify::new()),
    )
    .await;
    let recovered = reopened.store.neutral_execution(&run.run.id).unwrap();
    assert!(
        matches!(recovered.status, NeutralStatus::Completed { .. }),
        "{:?}",
        recovered.status
    );
    assert_eq!(writer.calls.load(Ordering::Acquire), 0);
    assert_eq!(writer.reconciles.load(Ordering::Acquire), 2); // oracle read + actual actor reconciliation
    assert_eq!(std::fs::read_to_string(file).unwrap().lines().count(), 1);
    assert_eq!(
        recovered.effect(&child.key).unwrap().tool.as_ref(),
        Some(&call)
    );
    assert_ne!(
        recovered
            .effect(&call.parent)
            .unwrap()
            .model_session
            .as_ref(),
        Some(&call.session)
    );
    assert!(
        reopened
            .start_model_tool(call, BTreeMap::from([("value".into(), json!(30))]))
            .is_err()
    );
    reopened.neutral_host(writer);
    drop(host);
    drop(reopened);
    let persisted = RuntimeOwner::acquire(&db).unwrap();
    assert_eq!(
        persisted.store.neutral_execution(&run.run.id).unwrap(),
        recovered
    );
}

// Catches generation theft, an invented result digest, and repeating a resume
// command to replace the already bound continuation session.
#[tokio::test]
async fn recovery_session_fences_reject_missing_results_and_stale_replacement() {
    let (dir, owner, run, at) = fixture(30, false);
    let (run, call) = parent(&owner, &run, at);
    let mut session = call.session.clone();
    session.generation += 1;
    assert!(
        effect_command_at(
            &owner,
            &run,
            "resume",
            EffectCommand::ResumeModel {
                key: call.parent.clone(),
                previous: call.session.clone(),
                session: session.clone(),
                results_digest: "invented".into()
            },
            at
        )
        .is_err()
    );
    let writer = FileHost::new(&owner, dir.path().join("write"), Reply::Complete, false);
    owner.neutral_host(writer);
    owner
        .start_model_tool(call.clone(), BTreeMap::from([("value".into(), json!(30))]))
        .unwrap();
    tokio::task::yield_now().await;
    let run = owner.store.neutral_execution(&run.run.id).unwrap();
    let digest = crate::workflows::catalog::content_digest(
        &run.model_recovery_context(&call.parent).unwrap(),
    )
    .unwrap();
    let command = EffectCommand::ResumeModel {
        key: call.parent.clone(),
        previous: call.session.clone(),
        session,
        results_digest: digest,
    };
    let resumed = effect_command_at(&owner, &run, "resume-valid", command.clone(), at).unwrap();
    assert!(effect_command_at(&owner, &resumed, "resume-stale", command, at).is_err());
    let mut cancelled = resumed.clone();
    cancelled
        .effects
        .get_mut(&call.parent.digest)
        .unwrap()
        .cancel_started = true;
    assert!(cancelled.model_recovery_context(&call.parent).is_err());
}
