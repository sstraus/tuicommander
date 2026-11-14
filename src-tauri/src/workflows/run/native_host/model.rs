use super::*;

impl NativeHost {
    pub(super) fn reconcile_model(&self, effect: NeutralEffect) -> ObservationFuture {
        let binding = self.binding(&effect.invocation);
        let preparation = self.prepare(&effect.invocation);
        Box::pin(async move {
            let bound = binding?;
            if preparation? != effect.preparation {
                return Err("model continuation current preparation differs".into());
            }
            let state = bound
                .context
                .current(Some(crate::agent_capabilities::Capability::Operator))?;
            let runtime = crate::acp_commands::ego_config(&state).map_err(|e| e.message)?;
            let owner = state.workflow_runtime.neutral_owner()?;
            let output = owner
                .resume_model_activity(
                    &state.acp,
                    &effect,
                    &runtime,
                    bound.context.workspace_path().into(),
                )
                .await?;
            Ok(super::super::neutral_http::completed(BTreeMap::from([(
                "result".into(),
                serde_json::to_value(output).map_err(|e| e.to_string())?,
            )])))
        })
    }
}

impl NativeHost {
    pub(super) fn invoke_model(
        &self,
        effect: NeutralEffect,
        prepared: Result<(NativeBinding, Arc<AppState>), String>,
    ) -> ObservationFuture {
        Box::pin(async move {
            let (bound, state) = prepared?;
            bound
                .context
                .current(Some(crate::agent_capabilities::Capability::Operator))?;
            if let Some(exhausted) = super::super::native_role_budget::admit(&bound, &effect)? {
                return Ok(exhausted);
            }
            let runtime = crate::acp_commands::ego_config(&state).map_err(|e| e.message)?;
            let owner = state.workflow_runtime.neutral_owner()?;
            if effect.invocation.descriptor.handler.name == super::super::native_agent::HANDLER {
                let source = super::super::native_agent::source(&effect.invocation)?;
                bound
                    .context
                    .require_artifact(&source.commit, &source.tree)?;
            }
            let output = owner
                .run_model_activity(
                    &state.acp,
                    &effect,
                    &runtime,
                    bound.context.workspace_path().into(),
                )
                .await?;
            let output = if effect.invocation.descriptor.handler.name
                == super::super::native_agent::HANDLER
            {
                super::super::native_agent::report(&bound, &effect, output)?
            } else {
                serde_json::to_value(output).map_err(|e| e.to_string())?
            };
            Ok(super::super::neutral_http::completed(BTreeMap::from([(
                if effect.invocation.descriptor.handler.name == super::super::native_agent::HANDLER
                {
                    "report"
                } else {
                    "result"
                }
                .into(),
                output,
            )])))
        })
    }
}
