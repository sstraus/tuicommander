use super::*;
use crate::workflows::catalog::{HandlerRef, ReusableTarget};

impl NativeHost {
    pub(super) fn native(&self, descriptor: &ActivityDescriptor) -> bool {
        self.catalog.get(&descriptor.id) == Some(descriptor)
    }

    pub(super) fn family(
        &self,
        invocation: &NeutralInvocation,
        current: bool,
    ) -> Result<Option<native_config::FamilyBinding>, String> {
        if invocation.descriptor.target.is_some() {
            return Ok(None);
        }
        let installed = if current {
            let state = self
                .state
                .upgrade()
                .ok_or("native provider owner unavailable")?;
            native_config::InstalledFamilies::load(state.as_ref())?
        } else {
            self.installed
                .read()
                .get(&invocation.publication.project)
                .cloned()
                .ok_or("configured catalog snapshot unavailable")?
        };
        if installed.catalog.get(&invocation.descriptor.id) != Some(&invocation.descriptor) {
            return Err("activity differs from current installed configuration".into());
        }
        installed
            .bindings
            .get(&invocation.descriptor.handler)
            .cloned()
            .map(Some)
            .ok_or("installed family unavailable".into())
    }

    pub(super) fn connector(
        &self,
        invocation: &NeutralInvocation,
        caller: Option<NativeContext>,
        current: bool,
    ) -> Result<native_connectors::ConnectorAdapter, String> {
        native_connectors::ConnectorAdapter::new(
            self.state
                .upgrade()
                .ok_or("native provider owner unavailable")?,
            &invocation.descriptor,
            self.family(invocation, current)?,
            caller,
            Some(self.question(invocation)?),
        )
    }

    pub(super) fn question(
        &self,
        invocation: &NeutralInvocation,
    ) -> Result<EffectQuestion, String> {
        let bound = self.bindings.lock().get(&invocation.fence.run).cloned();
        let deadline_ms = match bound {
            Some(bound) => {
                bound
                    .store
                    .neutral_execution(&invocation.fence.run)?
                    .deadline_ms
            }
            None => i64::MAX,
        };
        Ok(EffectQuestion {
            revision: 1,
            title: format!(
                "Run exact {} operation?",
                invocation.descriptor.handler.name
            ),
            deadline_ms,
        })
    }

    pub(super) fn script(
        bound: &NativeBinding,
    ) -> Result<super::super::neutral_script::ScriptAdapter, String> {
        let parent = bound
            .store
            .database_path()
            .parent()
            .ok_or("script database parent unavailable")?;
        Ok(super::super::neutral_script::ScriptAdapter {
            runs_db: bound.store.database_path().into(),
            input_directory: parent.join("neutral-script-inputs"),
        })
    }

    pub(super) fn reusable<'a>(registry: &mut HostRegistry<'a>, host: &'a Self) {
        super::super::neutral_script::register(registry, host)
            .expect("closed script handler is distinct");
        // These fixed handler identities are disjoint from configured native families.
        for (name, kind) in [
            (
                super::super::native_cohort::HANDLER,
                crate::workflows::neutral_admission::ReusableHandlerKind::Subflow,
            ),
            (
                "host.http",
                crate::workflows::neutral_admission::ReusableHandlerKind::Http,
            ),
            (
                "host.subflow",
                crate::workflows::neutral_admission::ReusableHandlerKind::Subflow,
            ),
        ] {
            registry
                .register_reusable_handler(
                    HandlerRef {
                        name: name.into(),
                        version: 1,
                    },
                    kind,
                    host,
                )
                .expect("closed reusable handlers are distinct");
        }
    }
}

pub(super) fn authoring_invocation(
    publication: &NeutralPublication,
    descriptor: &ActivityDescriptor,
    instance: &ActivityInstance,
) -> NeutralInvocation {
    NeutralInvocation {
        fence: super::super::neutral_execution::NeutralAttemptFence {
            run: "preview".into(),
            execution: "preview".into(),
            activation: 1,
            attempt: 1,
            scope: None,
        },
        publication: publication.clone(),
        descriptor: descriptor.clone(),
        instance: instance.clone(),
        inputs: BTreeMap::new(),
        subject: None,
    }
}

pub(super) fn script_target(descriptor: &ActivityDescriptor) -> bool {
    matches!(descriptor.target, Some(ReusableTarget::Script { .. }))
}

pub(super) fn subflow_target(descriptor: &ActivityDescriptor) -> bool {
    matches!(descriptor.target, Some(ReusableTarget::Subflow { .. }))
}
