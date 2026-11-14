use super::*;

impl HostAdmission for NativeHost {
    fn publication(
        &self,
        context: &dyn HostContext,
        publication: &NeutralPublication,
        descriptor: &ActivityDescriptor,
        instance: &ActivityInstance,
    ) -> Result<(), String> {
        let context = (context as &dyn std::any::Any)
            .downcast_ref::<PublicationContext>()
            .ok_or("native publication context unavailable")?;
        if context.project != publication.project {
            return Err("native publication project differs".into());
        }
        if matches!(
            descriptor.handler.name.as_str(),
            "native.review.bounded" | "native.correction.policy.assess"
        ) {
            super::super::native_review_policy::policy(instance)?;
        }
        if self.native(descriptor) || families::subflow_target(descriptor) {
            return Ok(());
        }
        if families::script_target(descriptor) {
            return super::super::neutral_script::ScriptPolicy::resolve(descriptor, instance)
                .map(|_| ());
        }
        let invocation = families::authoring_invocation(publication, descriptor, instance);
        if super::super::native_agent::is_model(&descriptor.handler.name) {
            self.family(&invocation, false)?;
            let state = self
                .state
                .upgrade()
                .ok_or("native provider owner unavailable")?;
            let runtime = crate::acp_commands::ego_config(&state).map_err(|e| e.message)?;
            return native_model::publication(&invocation, &runtime).map(|_| ());
        }
        if descriptor.handler.name == "host.approval" {
            self.family(&invocation, false)?;
            return Ok(());
        }
        let adapter = self.connector(&invocation, None, false)?;
        let context = adapter.host().context(&invocation)?;
        adapter
            .admission()
            .publication(context.as_ref(), publication, descriptor, instance)
    }
    fn dispatch(
        &self,
        context: &dyn HostContext,
        publication: &NeutralPublication,
        descriptor: &ActivityDescriptor,
        instance: &ActivityInstance,
        inputs: &BTreeMap<String, Value>,
        subject: Option<&BTreeMap<String, Value>>,
    ) -> Result<(), String> {
        let context = Self::admitted_context(context)?;
        if !self.native(descriptor) {
            if families::subflow_target(descriptor) {
                if descriptor.handler.name == super::super::native_cohort::HANDLER {
                    super::super::native_cohort::admit(context, instance, inputs, subject)?;
                }
                return context.current(None).map(|_| ());
            }
            context.current(Some(crate::agent_capabilities::Capability::Operator))?;
            let mut invocation = families::authoring_invocation(publication, descriptor, instance);
            invocation.inputs = inputs.clone();
            invocation.subject = subject.cloned();
            if families::script_target(descriptor) {
                return super::super::neutral_script::ScriptAdapter::prepare(&invocation)
                    .map(|_| ());
            }
            if super::super::native_agent::is_model(&descriptor.handler.name) {
                self.family(&invocation, true)?;
                if descriptor.handler.name == super::super::native_agent::HANDLER {
                    let source = super::super::native_agent::source(&invocation)?;
                    context.require_artifact(&source.commit, &source.tree)?;
                    let workspace = subject
                        .and_then(|s| s.get("workspace"))
                        .ok_or("agent workspace subject absent")?;
                    if crate::workflows::data_reference::DataReference::parse(workspace)?
                        != (crate::workflows::data_reference::DataReference::Workspace {
                            id: context.workspace_path().to_string_lossy().into_owned(),
                        })
                    {
                        return Err("agent workspace differs from inherited scope".into());
                    }
                }
                let state = self
                    .state
                    .upgrade()
                    .ok_or("native provider owner unavailable")?;
                let runtime = crate::acp_commands::ego_config(&state).map_err(|e| e.message)?;
                return native_model::prepare(&invocation, &runtime).map(|_| ());
            }
            if descriptor.handler.name == "host.approval" {
                self.family(&invocation, true)?;
                return super::super::native_approval::validate(&invocation);
            }
            let adapter = self.connector(&invocation, Some(context.clone()), true)?;
            let adapted = adapter.host().context(&invocation)?;
            return adapter.admission().dispatch(
                adapted.as_ref(),
                publication,
                descriptor,
                instance,
                inputs,
                subject,
            );
        }
        context.git_workspace()?;
        context.current(
            matches!(
                descriptor.handler.name.as_str(),
                "native.publication.push"
                    | "native.publication.github"
                    | "native.issue.effect"
                    | super::super::native_issue_publication::HANDLER
            )
            .then_some(crate::agent_capabilities::Capability::Push),
        )?;
        let workspace = subject
            .and_then(|s| s.get("workspace"))
            .ok_or("native workspace subject absent")?;
        if crate::workflows::data_reference::DataReference::parse(workspace)?
            != (crate::workflows::data_reference::DataReference::Workspace {
                id: context.workspace_path().to_string_lossy().into_owned(),
            })
        {
            return Err("native workspace subject differs from trusted binding".into());
        }
        native_dispatch::validate_request(&descriptor.handler.name, inputs)?;
        Ok(())
    }
}
