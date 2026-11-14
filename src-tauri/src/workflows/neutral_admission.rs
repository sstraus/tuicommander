//! Internal host admission checks for neutral publications. No effects are run.
//! The host must supply real adapters and context; this module grants nothing.

// DEFERRED (2026-11-10) — contract entry points await the 004-2809 runtime consumer.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "002-eeb6 contract; first consumer 004-2809")
)]

use super::WorkflowStore;
use super::catalog::{
    ActivityDescriptor, ActivityInstance, HandlerRef, ReusableTarget, WorkflowCatalog,
};
use super::neutral_publication::{NeutralDefinition, NeutralPublication};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

type DispatchValues<'a> = (
    &'a BTreeMap<String, Value>,
    Option<&'a BTreeMap<String, Value>>,
);

/// Backend-owned context. Implementations decide which concrete state they need;
/// no deserialized graph value implements host authority.
pub(crate) trait HostContext: std::any::Any {
    /// Backend-only derivation: resolve explicit IDs using current host state.
    /// The returned context must never widen this parent's effective rights.
    /// No default inheritance is safe when the host cannot establish that fact.
    fn for_subflow(
        &self,
        _references: &super::data_reference::ReferenceInputs,
    ) -> Result<Box<dyn HostContext>, String> {
        Err("host subflow context derivation unavailable".into())
    }
}

/// A trusted host adapter owns current rights, binding and freshness decisions.
/// Unknown state must return an error. The adapter must not rely on graph labels
/// as grants, and must enforce inherited scope using its actual host context.
/// Publication callbacks run inside a SQLite write transaction: checks must be
/// bounded and local, with no external I/O or WorkflowStore re-entry.
pub(crate) trait HostAdmission {
    fn publication(
        &self,
        context: &dyn HostContext,
        publication: &NeutralPublication,
        descriptor: &ActivityDescriptor,
        instance: &ActivityInstance,
    ) -> Result<(), String>;
    fn dispatch(
        &self,
        context: &dyn HostContext,
        publication: &NeutralPublication,
        descriptor: &ActivityDescriptor,
        instance: &ActivityInstance,
        inputs: &BTreeMap<String, Value>,
        subject: Option<&BTreeMap<String, Value>>,
    ) -> Result<(), String>;
}

/// A host adapter's supported target kind, not a descriptor-supplied label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReusableHandlerKind {
    Script,
    Http,
    Subflow,
}

/// Constructed by trusted backend code, never from workflow/client JSON.
#[derive(Default)]
pub(crate) struct HostRegistry<'a> {
    pub handlers: BTreeMap<HandlerRef, &'a dyn HostAdmission>,
    pub capabilities: BTreeMap<String, &'a dyn HostAdmission>,
    reusable_handlers: BTreeMap<HandlerRef, ReusableHandlerKind>,
}

impl<'a> HostRegistry<'a> {
    /// Backend registration binds an exact handler version to one target kind.
    /// A native entry cannot be relabelled through operation configuration.
    pub(crate) fn register_reusable_handler(
        &mut self,
        handler: HandlerRef,
        kind: ReusableHandlerKind,
        adapter: &'a dyn HostAdmission,
    ) -> Result<(), String> {
        if self.handlers.contains_key(&handler) {
            return Err("handler is already registered".into());
        }
        self.reusable_handlers.insert(handler.clone(), kind);
        self.handlers.insert(handler, adapter);
        Ok(())
    }

    pub(super) fn check_reusable_handler(
        &self,
        descriptor: &ActivityDescriptor,
    ) -> Result<(), String> {
        let kind = match &descriptor.target {
            Some(ReusableTarget::Script { .. }) => ReusableHandlerKind::Script,
            Some(ReusableTarget::Http { .. }) => ReusableHandlerKind::Http,
            Some(ReusableTarget::Subflow { .. }) => ReusableHandlerKind::Subflow,
            None => return Err("native operations are host-owned".into()),
        };
        if self.reusable_handlers.get(&descriptor.handler) != Some(&kind) {
            return Err("reusable target handler kind is not registered by the host".into());
        }
        Ok(())
    }

    /// Derive context only from a trusted parent and typed resolved references.
    /// This is not an effect permit; the child must pass ordinary dispatch checks.
    pub(crate) fn subflow_context(
        &self,
        parent: &dyn HostContext,
        descriptor: &ActivityDescriptor,
        inputs: &BTreeMap<String, Value>,
    ) -> Result<Box<dyn HostContext>, String> {
        if !matches!(&descriptor.target, Some(ReusableTarget::Subflow { .. })) {
            return Err("child context requires a subflow target".into());
        }
        self.check_reusable_handler(descriptor)?;
        let references =
            super::data_reference::ReferenceInputs::resolve(&descriptor.inputs, inputs)?;
        parent.for_subflow(&references)
    }

    /// Actual activation boundary: values have already been projected by the
    /// retained run owner. No current catalog or root-input preflight is used.
    pub(crate) fn admit_resolved(
        &self,
        context: &dyn HostContext,
        publication: &NeutralPublication,
        descriptor: &ActivityDescriptor,
        instance: &ActivityInstance,
        inputs: &BTreeMap<String, Value>,
        subject: Option<&BTreeMap<String, Value>>,
    ) -> Result<ActivityInstance, String> {
        if instance.descriptor != descriptor.id
            || !descriptor
                .execution_policies
                .contains(&instance.execution_policy)
        {
            return Err("resolved invocation descriptor or policy mismatch".into());
        }
        if descriptor.target.is_some() {
            self.check_reusable_handler(descriptor)?;
        }
        let mut instance = instance.clone();
        instance.config = descriptor.config.resolve(&instance.config, "config")?;
        let inputs = descriptor
            .inputs
            .resolve(inputs, "resolved invocation inputs")?;
        let subject = match &descriptor.subject {
            Some(schema) => schema.resolve(subject)?,
            None => subject.cloned(),
        };
        let handler = self
            .handlers
            .get(&descriptor.handler)
            .ok_or("resolved invocation handler unavailable")?;
        handler.dispatch(
            context,
            publication,
            descriptor,
            &instance,
            &inputs,
            subject.as_ref(),
        )?;
        for capability in &descriptor.required_capabilities {
            self.capabilities
                .get(capability)
                .ok_or_else(|| format!("capability adapter unavailable: {capability}"))?
                .dispatch(
                    context,
                    publication,
                    descriptor,
                    &instance,
                    &inputs,
                    subject.as_ref(),
                )?;
        }
        Ok(instance)
    }

    pub(super) fn available_handlers(&self) -> BTreeSet<HandlerRef> {
        self.handlers.keys().cloned().collect()
    }

    fn check(
        &self,
        context: &dyn HostContext,
        catalog: &WorkflowCatalog,
        root: &NeutralPublication,
        children: &[NeutralPublication],
        values: Option<DispatchValues<'_>>,
    ) -> Result<(), String> {
        for publication in std::iter::once(root).chain(children) {
            for (activity, instance) in publication.definition.instances.iter().enumerate() {
                let descriptor = catalog
                    .get(&instance.descriptor)
                    .ok_or("missing descriptor after pin verification")?;
                if descriptor.target.is_some() {
                    self.check_reusable_handler(descriptor)?;
                }
                let mut resolved_instance = instance.clone();
                resolved_instance.config = descriptor.config.resolve(&instance.config, "config")?;
                let activity_inputs = values
                    .map(|(inputs, _)| {
                        if let Some(bindings) = publication.definition.input_bindings.get(&activity)
                        {
                            let workflow = inputs
                                .iter()
                                .map(|(name, value)| {
                                    (
                                        name.clone(),
                                        super::data_binding::Datum {
                                            state: super::data_binding::DataState::Present(
                                                value.clone(),
                                            ),
                                            provenance: BTreeSet::new(),
                                        },
                                    )
                                })
                                .collect();
                            super::data_binding::resolve_bindings(
                                bindings,
                                &workflow,
                                &BTreeMap::new(),
                                &descriptor.inputs,
                                &BTreeMap::new(),
                            )
                            .map(|resolved| resolved.values)
                        } else {
                            descriptor.inputs.resolve(inputs, "inputs")
                        }
                    })
                    .transpose()?;
                let activity_subject = match (values, &descriptor.subject) {
                    (Some((_, subject)), Some(schema)) => schema.resolve(subject)?,
                    (Some((_, subject)), None) => subject.cloned(),
                    (None, _) => None,
                };
                let handler = self.handlers.get(&descriptor.handler).ok_or_else(|| {
                    format!(
                        "handler unavailable: {}@{}",
                        descriptor.handler.name, descriptor.handler.version
                    )
                })?;
                let invoke = |adapter: &dyn HostAdmission| match values {
                    None => {
                        adapter.publication(context, publication, descriptor, &resolved_instance)
                    }
                    Some(_) => adapter.dispatch(
                        context,
                        publication,
                        descriptor,
                        &resolved_instance,
                        activity_inputs.as_ref().expect("dispatch inputs resolved"),
                        activity_subject.as_ref(),
                    ),
                };
                invoke(*handler).map_err(|e| {
                    format!(
                        "host handler {}@{} refused: {e}",
                        descriptor.handler.name, descriptor.handler.version
                    )
                })?;
                for capability in &descriptor.required_capabilities {
                    let adapter = self
                        .capabilities
                        .get(capability)
                        .ok_or_else(|| format!("capability adapter unavailable: {capability}"))?;
                    invoke(*adapter)
                        .map_err(|e| format!("host capability {capability} refused: {e}"))?;
                }
            }
        }
        Ok(())
    }
}

/// Publication and pre-dispatch validation only. No result is a dispatch permit:
/// the actual executor must call the host checks again at its effect boundary.
pub(crate) struct NeutralAdmissionService<'a> {
    pub store: &'a WorkflowStore,
    pub catalog: &'a WorkflowCatalog,
    pub registry: &'a HostRegistry<'a>,
}

impl NeutralAdmissionService<'_> {
    pub(crate) fn publish(
        &self,
        context: Option<&dyn HostContext>,
        project: &str,
        id: &str,
        expected_revision: i64,
        definition: NeutralDefinition,
    ) -> Result<NeutralPublication, String> {
        let context = context.ok_or("host context unavailable")?;
        let handlers = self.registry.available_handlers();
        self.store.save_neutral_publication_checked(
            project,
            id,
            expected_revision,
            definition,
            self.catalog,
            &handlers,
            |root, children| {
                self.registry
                    .check(context, self.catalog, root, children, None)
            },
        )
    }

    /// The draft revision and presentation commit atomically with the admitted publication.
    pub(crate) fn publish_draft(
        &self,
        context: Option<&dyn HostContext>,
        draft: &super::neutral_api::model::NeutralDraft,
    ) -> Result<NeutralPublication, String> {
        let context = context.ok_or("host context unavailable")?;
        let handlers = self.registry.available_handlers();
        self.store.save_neutral_publication_checked_from_draft(
            &draft.project,
            &draft.id,
            draft.latest_published_revision,
            draft.definition.clone(),
            self.catalog,
            &handlers,
            |root, children| {
                self.registry
                    .check(context, self.catalog, root, children, None)
            },
            Some(draft),
        )
    }

    /// Root/workflow-input-only preflight, not resolved activation admission.
    /// Activity-output bindings and required item/index/carried inputs refuse
    /// because no activation output or scoped-input snapshot is supplied here.
    /// Unbound activity fields retain the inherited root-field-name identity
    /// compatibility; that behavior is not explicit data-mapping evidence.
    /// The host checks current subject freshness and authority, not an effect permit.
    // DEFERRED (2026-11-10) — 004 must admit each activity's fully resolved inputs
    // at every actual dispatch; never use this whole-publication preflight as a permit.
    pub(crate) fn check_dispatch(
        &self,
        context: Option<&dyn HostContext>,
        project: &str,
        id: &str,
        revision: i64,
        inputs: &BTreeMap<String, Value>,
        subject: Option<&BTreeMap<String, Value>>,
    ) -> Result<(), String> {
        let context = context.ok_or("host context unavailable")?;
        let handlers = self.registry.available_handlers();
        let (root, children) = self.store.resolve_neutral_publication(
            project,
            id,
            revision,
            self.catalog,
            &handlers,
        )?;
        if !children.is_empty() {
            return Err("child arguments unresolved: subflow mappings are unavailable".into());
        }
        let (inputs, subject) = root.definition.validate_values(inputs, subject)?;
        self.registry.check(
            context,
            self.catalog,
            &root,
            &children,
            Some((&inputs, subject.as_ref())),
        )
    }
}
