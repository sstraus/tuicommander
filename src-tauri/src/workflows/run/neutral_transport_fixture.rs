use super::super::{
    neutral_effects::{EffectPreparation, NeutralEffect},
    neutral_runtime::{NeutralHost, NeutralInvocation, ObservationFuture},
    neutral_runtime_tests::Host,
};
use super::*;
use crate::workflows::{
    catalog::*,
    neutral_admission::*,
    neutral_api::{model::*, neutral_action},
    neutral_host::NeutralCatalogHost,
    neutral_publication::*,
};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, AtomicUsize};

pub(super) struct Source {
    inner: Arc<Host>,
    catalog: WorkflowCatalog,
    pub(super) template: NeutralInvocation,
    pub(super) calls: AtomicUsize,
    pub(super) denied: AtomicBool,
}
impl HostAdmission for Source {
    fn publication(
        &self,
        c: &dyn HostContext,
        p: &NeutralPublication,
        d: &ActivityDescriptor,
        i: &ActivityInstance,
    ) -> Result<(), String> {
        if self.denied.load(Ordering::Acquire) {
            return Err("native authority refused".into());
        }
        self.inner.publication(c, p, d, i)
    }
    fn dispatch(
        &self,
        c: &dyn HostContext,
        p: &NeutralPublication,
        d: &ActivityDescriptor,
        i: &ActivityInstance,
        values: &std::collections::BTreeMap<String, Value>,
        snapshot: Option<&std::collections::BTreeMap<String, Value>>,
    ) -> Result<(), String> {
        if self.denied.load(Ordering::Acquire) {
            return Err("native authority refused".into());
        }
        self.inner.dispatch(c, p, d, i, values, snapshot)
    }
}
impl NeutralHost for Source {
    fn context(&self, i: &NeutralInvocation) -> Result<Box<dyn HostContext>, String> {
        self.inner.context(i)
    }
    fn registry(&self) -> HostRegistry<'_> {
        let mut registry = self.inner.registry();
        registry
            .handlers
            .insert(self.template.descriptor.handler.clone(), self);
        registry
    }
    fn prepare(&self, i: &NeutralInvocation) -> Result<EffectPreparation, String> {
        self.inner.prepare(i)
    }
    fn invoke_effect(&self, e: NeutralEffect) -> ObservationFuture {
        self.calls.fetch_add(1, Ordering::AcqRel);
        self.inner.invoke_effect(e)
    }
}
impl NeutralCatalogHost for Source {
    fn catalog(&self, _: &str) -> Result<WorkflowCatalog, String> {
        Ok(self.catalog.clone())
    }
    fn publication_context(&self, _: &str) -> Result<Box<dyn HostContext>, String> {
        self.inner.context(&self.template)
    }
}
pub(super) fn source() -> (tempfile::TempDir, Arc<Source>, NeutralDefinition) {
    let (dir, _owner, run, _) = super::super::neutral_execution_tests::fixture(20, false);
    let descriptor = run
        .run
        .closure
        .descriptors
        .iter()
        .find(|d| d.handler.name == "host.increment")
        .unwrap()
        .clone();
    let instance = run
        .publication
        .definition
        .instances
        .iter()
        .find(|i| i.descriptor == descriptor.id)
        .unwrap()
        .clone();
    let mut catalog = WorkflowCatalog::new();
    catalog.register(descriptor.clone()).unwrap();
    let definition:NeutralDefinition=serde_json::from_value(json!({
        "control":{"nodes":{"start":{"type":"start"},"work":{"type":"activity","activity":0},"wait":{"type":"wait","duration_ms":2},"end":{"type":"end","outcome":"done"}},"edges":{"start":{"next":"work"},"work":{"ok":"wait"},"wait":{"next":"end"}}},
        "inputs":{"fields":{"value":{"value_type":"integer","required":true,"default":null}}},
        "outputs":{"fields":{"answer":{"value_type":"integer","required":true,"default":null}}},
        "outcomes":["done"],"subject":null,"instances":[instance],"input_bindings":{"0":{"value":{"source":"workflow_input","field":"value"}}},"conditions":{},"collections":{},"loops":{},"forks":{},"output_bindings":{"answer":{"source":"activity_output","activity":0,"field":"next"}},"subflows":[]
    })).unwrap();
    let template = NeutralInvocation {
        fence: super::super::neutral_execution::NeutralAttemptFence {
            run: "fixture".into(),
            execution: "root".into(),
            activation: 1,
            attempt: 1,
            scope: None,
        },
        publication: run.publication.clone(),
        descriptor,
        instance,
        inputs: Default::default(),
        subject: None,
    };
    (
        dir,
        Arc::new(Source {
            inner: Host::new(false, false),
            catalog,
            template,
            calls: AtomicUsize::new(0),
            denied: AtomicBool::new(false),
        }),
        definition,
    )
}
pub(super) fn state() -> Arc<AppState> {
    Arc::new(crate::state::tests_support::make_test_app_state())
}
pub(super) fn call(state: &Arc<AppState>, project: &str, value: Value) -> Result<Value, String> {
    let action: NeutralAction = serde_json::from_value(value).map_err(|e| e.to_string())?;
    serde_json::to_value(neutral_action(state, project, action, None)?).map_err(|e| e.to_string())
}
