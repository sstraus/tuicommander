use super::*;
use crate::workflows::catalog::{
    ActivityDescriptor, DescriptorId, FieldSchema, ObjectSchema, ValueType,
};
use crate::workflows::command::Invocation;
use crate::workflows::neutral_admission::{HostAdmission, HostContext, HostRegistry};
use crate::workflows::neutral_publication::NeutralPublication;
use crate::workflows::run::neutral_execution::{NeutralExecution, NeutralExecutionCommand};
use crate::workflows::run::neutral_execution_tests::{fixture_with, step};
use crate::workflows::run::neutral_runtime::{NeutralHost, ObservationFuture};
use crate::workflows::run::runtime::RuntimeOwner;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub(super) const ECHO: &str = r#"
import json,sys
q=json.load(sys.stdin)
print('diagnostic only',file=sys.stderr)
print(json.dumps(dict(version=1,identity=q['identity'],outcome='ok',data=dict(next=q['inputs']['value']+1,payload=dict(empty=[],nothing=None,text='7')),artifacts=dict(report=dict(reference='artifact',id='report',revision='r1')))))
"#;

pub(super) fn policy(mutation: bool) -> ScriptPolicy {
    ScriptPolicy {
        version: 1,
        timeout_secs: 10,
        input_bytes: 65_536,
        stdout_bytes: 16_384,
        stderr_bytes: 16_384,
        artifact_limit: 4,
        resources: if mutation {
            BTreeSet::from(["workspace:throwaway".into()])
        } else {
            BTreeSet::new()
        },
        environment_refs: BTreeMap::new(),
        secret_refs: BTreeMap::new(),
        mutation,
        retry: contract::RetryPolicy::Never,
        reconciliation: if mutation {
            Reconciliation::Manual {
                procedure: "Inspect the retained effect and original workspace".into(),
            }
        } else {
            Reconciliation::ReadOnly
        },
        output: OutputMode::Structured,
    }
}

fn field(value_type: ValueType, required: bool, default: Option<Value>) -> FieldSchema {
    FieldSchema {
        value_type,
        required,
        default,
    }
}

pub(super) fn descriptor(script: &str, policy: &ScriptPolicy) -> ActivityDescriptor {
    let contract = ActivityDescriptor {
        id: DescriptorId {
            namespace: "custom".into(),
            name: "calculate".into(),
            version: 1,
        },
        purpose: "Calculate a structured value".into(),
        handler: handler(),
        target: None,
        config: ObjectSchema::default(),
        inputs: ObjectSchema {
            fields: BTreeMap::from([("value".into(), field(ValueType::Integer, true, None))]),
        },
        subject: None,
        outputs: ObjectSchema {
            fields: BTreeMap::from([
                ("next".into(), field(ValueType::Integer, true, None)),
                ("note".into(), field(ValueType::String, false, None)),
                ("payload".into(), field(ValueType::Object, false, None)),
                (
                    "report".into(),
                    field(ValueType::ArtifactReference, false, None),
                ),
            ]),
        },
        outcomes: BTreeSet::from(["ok".into()]),
        required_capabilities: BTreeSet::new(),
        execution_policies: BTreeSet::from(["local".into()]),
    };
    script_descriptor(
        contract,
        Invocation::Shell {
            program: if cfg!(windows) { "python" } else { "python3" }.into(),
            script: script.into(),
            version: 1,
        },
        policy,
    )
    .unwrap()
}

pub(super) fn fixture(
    script: &str,
    policy: &ScriptPolicy,
) -> (tempfile::TempDir, Arc<RuntimeOwner>, NeutralExecution, i64) {
    fixture_with(30, false, |definition, catalog, _| {
        let mut descriptor = descriptor(script, policy);
        if matches!(policy.output, OutputMode::ExitStatus { .. }) {
            descriptor.outputs.fields.get_mut("next").unwrap().required = false;
            definition
                .outputs
                .fields
                .get_mut("result")
                .unwrap()
                .required = false;
        }
        definition.instances[0].descriptor = descriptor.id.clone();
        catalog.register(descriptor).unwrap();
    })
}

pub(super) fn prepared(
    script: &str,
    policy: &ScriptPolicy,
) -> (
    tempfile::TempDir,
    Arc<RuntimeOwner>,
    NeutralEffect,
    ScriptAdapter,
) {
    let (dir, owner, run, at) = fixture(script, policy);
    let run = step(&owner, &run, "enter", at);
    let run = step(&owner, &run, "prepare", at);
    let attempt = run.attempt.as_ref().unwrap();
    let adapter = ScriptAdapter {
        runs_db: dir.path().join("runs.sqlite3"),
        input_directory: dir.path().join("inputs"),
    };
    let host = Host {
        adapter: adapter.clone(),
        workspace: dir.path().into(),
    };
    let mut invocation = NeutralInvocation {
        fence: attempt.fence.clone(),
        publication: run.publication.clone(),
        descriptor: run.descriptor(0).unwrap().clone(),
        instance: run.publication.definition.instances[0].clone(),
        inputs: attempt.inputs.clone(),
        subject: attempt.subject.clone(),
    };
    super::super::neutral_runtime::admitted(&host, &run, &mut invocation, None).unwrap();
    let preparation = ScriptAdapter::prepare(&invocation).unwrap();
    let effect = NeutralEffect::prepare(invocation, preparation).unwrap();
    let run = owner
        .neutral_effect_command(
            &run.run.id,
            super::super::neutral_effects::EffectCommand::Prepare {
                effect: Box::new(effect.clone()),
            },
        )
        .unwrap();
    let run = owner
        .neutral_command_at(
            &run.run.id,
            "dispatch",
            run.sequence,
            NeutralExecutionCommand::Effect {
                command: super::super::neutral_effects::EffectCommand::Dispatch {
                    key: effect.key.clone(),
                },
            },
            at,
        )
        .unwrap();
    let effect = run.effect(&effect.key).unwrap().clone();
    (dir, owner, effect, adapter)
}

struct Context;
impl HostContext for Context {}

pub(super) struct Host {
    pub adapter: ScriptAdapter,
    pub workspace: PathBuf,
}
impl HostAdmission for Host {
    fn publication(
        &self,
        _: &dyn HostContext,
        _: &NeutralPublication,
        descriptor: &ActivityDescriptor,
        instance: &crate::workflows::catalog::ActivityInstance,
    ) -> Result<(), String> {
        ScriptPolicy::resolve(descriptor, instance).map(|_| ())
    }
    fn dispatch(
        &self,
        context: &dyn HostContext,
        publication: &NeutralPublication,
        descriptor: &ActivityDescriptor,
        instance: &crate::workflows::catalog::ActivityInstance,
        _: &BTreeMap<String, Value>,
        _: Option<&BTreeMap<String, Value>>,
    ) -> Result<(), String> {
        // The fixture host binds only its throwaway workspace, never graph paths.
        if !(context as &dyn std::any::Any).is::<Context>() || !self.workspace.is_dir() {
            return Err("owned workspace unavailable".into());
        }
        self.publication(context, publication, descriptor, instance)
    }
}
impl NeutralHost for Host {
    fn context(&self, _: &NeutralInvocation) -> Result<Box<dyn HostContext>, String> {
        Ok(Box::new(Context))
    }
    fn registry(&self) -> HostRegistry<'_> {
        let mut registry = HostRegistry::default();
        register(&mut registry, self).unwrap();
        registry
    }
    fn prepare(&self, invocation: &NeutralInvocation) -> Result<EffectPreparation, String> {
        ScriptAdapter::prepare(invocation)
    }
    fn invoke_effect(&self, effect: NeutralEffect) -> ObservationFuture {
        let adapter = self.adapter.clone();
        let workspace = self.workspace.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || adapter.invoke(&effect, &workspace).effect)
                .await
                .map_err(|e| e.to_string())
        })
    }
    fn reconcile(&self, effect: NeutralEffect) -> ObservationFuture {
        let observation = self.adapter.reconcile(&effect);
        Box::pin(async move { Ok(observation) })
    }
    fn cancel(&self, effect: NeutralEffect) -> ObservationFuture {
        let observation = self.adapter.cancel(&effect);
        Box::pin(async move { Ok(observation) })
    }
}
