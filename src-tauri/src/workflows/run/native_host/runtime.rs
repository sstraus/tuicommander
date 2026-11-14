use super::*;

impl NeutralHost for NativeHost {
    fn bound(&self, run: &str) -> bool {
        self.bindings.lock().contains_key(run)
    }
    fn preview(&self, invocation: &NeutralInvocation) -> Result<EffectPreparation, String> {
        let context = self.publication_context(&invocation.publication.project)?;
        let mut invocation = invocation.clone();
        invocation.instance.config = invocation
            .descriptor
            .config
            .resolve(&invocation.instance.config, "preview config")?;
        invocation.inputs = invocation
            .descriptor
            .inputs
            .resolve(&invocation.inputs, "preview inputs")?;
        if let Some(schema) = &invocation.descriptor.subject {
            schema.resolve(invocation.subject.as_ref())?;
        }
        self.publication(
            context.as_ref(),
            &invocation.publication,
            &invocation.descriptor,
            &invocation.instance,
        )?;
        if self.native(&invocation.descriptor) {
            native_dispatch::validate_request(
                &invocation.descriptor.handler.name,
                &invocation.inputs,
            )?;
        }
        self.prepare(&invocation)
    }
    fn context(&self, invocation: &NeutralInvocation) -> Result<Box<dyn HostContext>, String> {
        let bound = self.binding(invocation)?;
        Ok(Box::new(
            bound.context.with_run(&invocation.fence.run, bound.store),
        ))
    }
    fn registry(&self) -> HostRegistry<'_> {
        let mut registry = HostRegistry::default();
        for (name, _) in crate::workflows::native_catalog::OPERATIONS {
            let id = crate::workflows::catalog::DescriptorId {
                namespace: "native".into(),
                name: (*name).into(),
                version: 1,
            };
            if let Some(descriptor) = self.catalog.get(&id) {
                registry.handlers.insert(descriptor.handler.clone(), self);
            }
        }
        for descriptor in self
            .catalog
            .descriptors()
            .filter(|d| d.handler.name.starts_with("native.correction."))
        {
            registry.handlers.insert(descriptor.handler.clone(), self);
        }
        Self::reusable(&mut registry, self);
        for installed in self.installed.read().values() {
            for descriptor in installed.catalog.descriptors() {
                registry.handlers.insert(descriptor.handler.clone(), self);
                for capability in &descriptor.required_capabilities {
                    registry.capabilities.insert(capability.clone(), self);
                }
            }
        }
        registry
    }
    fn prepare(&self, invocation: &NeutralInvocation) -> Result<EffectPreparation, String> {
        if !self.native(&invocation.descriptor) {
            if families::subflow_target(&invocation.descriptor) {
                return Ok(EffectPreparation {
                    mutation: false,
                    resources: BTreeSet::new(),
                    approval: None,
                    model_tools: None,
                });
            }
            if families::script_target(&invocation.descriptor) {
                return super::super::neutral_script::ScriptAdapter::prepare(invocation);
            }
            if super::super::native_agent::is_model(&invocation.descriptor.handler.name) {
                self.family(invocation, true)?;
                let state = self
                    .state
                    .upgrade()
                    .ok_or("native provider owner unavailable")?;
                let runtime = crate::acp_commands::ego_config(&state).map_err(|e| e.message)?;
                return native_model::prepare(invocation, &runtime);
            }
            if invocation.descriptor.handler.name == "host.approval" {
                self.family(invocation, true)?;
                super::super::native_approval::validate(invocation)?;
                let mut question = self.question(invocation)?;
                question.title = super::super::native_approval::title(invocation)?;
                return Ok(EffectPreparation {
                    mutation: false,
                    resources: BTreeSet::new(),
                    approval: Some(question),
                    model_tools: None,
                });
            }
            return self
                .connector(invocation, None, true)?
                .host()
                .prepare(invocation);
        }
        if invocation.descriptor.handler.name == "native.issue.triage" {
            // Triage is a tool-less mediated model parent (coordinator T1, 2026-11-12):
            // the policy lets its session bind; a model parent owns no resources.
            return Ok(EffectPreparation {
                mutation: false,
                resources: BTreeSet::new(),
                approval: None,
                model_tools: Some(super::super::neutral_model_tools::ModelToolPolicy {
                    tools: BTreeMap::new(),
                    max_calls: 1,
                    max_concurrency: 1,
                }),
            });
        }
        if matches!(
            invocation.descriptor.handler.name.as_str(),
            "native.review.bounded" | "native.correction.policy.assess"
        ) {
            super::super::native_review_policy::policy(&invocation.instance)?;
        }
        let mutation = matches!(
            invocation.descriptor.handler.name.as_str(),
            "native.issue.effect"
                | super::super::native_issue_prepare::HANDLER
                | super::super::native_issue_publication::HANDLER
                | "native.issue.handoff"
                | "native.commit.create"
                | "native.publication.push"
                | "native.publication.github"
                | "native.story.transition"
                | "native.story.create"
                | super::super::native_operator::HANDLER
        );
        let bound = self.bindings.lock().get(&invocation.fence.run).cloned();
        let workspace = bound
            .as_ref()
            .and_then(|b| b.context.workspace.as_ref().map(|w| w.git_dir.clone()))
            .unwrap_or_else(|| invocation.publication.project.clone());
        let mut resources = BTreeSet::from([format!("native:workspace:{}", workspace)]);
        let mut title = None;
        if matches!(
            invocation.descriptor.handler.name.as_str(),
            "native.publication.github" | super::super::native_issue_publication::HANDLER
        ) {
            let r: native_github::GitHubRequest = serde_json::from_value(
                if invocation.descriptor.handler.name
                    == super::super::native_issue_publication::HANDLER
                {
                    invocation.inputs["request"]["publication"].clone()
                } else {
                    invocation.inputs["request"].clone()
                },
            )
            .map_err(|e| e.to_string())?;
            native_github::validate(&r)?;
            resources.insert(format!("native:github:{}:{}", r.account, r.repository));
        }
        if invocation.descriptor.handler.name == "native.issue.effect" {
            let request: super::super::native_issue_effect::IssueEffectRequest =
                serde_json::from_value(invocation.inputs["request"].clone())
                    .map_err(|e| e.to_string())?;
            request.validate()?;
            resources.insert(format!(
                "native:github:issue:{}:{}:{}",
                request.source.account, request.source.repository, request.source.number
            ));
            title = Some(request.approval_title());
        }
        let covered = (invocation.descriptor.handler.name == "native.story.transition"
            && bound.as_ref().is_some_and(|b| {
                serde_json::from_value::<super::super::native_story::StoryRequest>(
                    invocation.inputs["request"].clone(),
                )
                .is_ok_and(|r| {
                    super::super::native_story::transition_covered(
                        &b.context,
                        &b.store,
                        &invocation.fence.run,
                        &r,
                    )
                })
            }))
            || invocation.descriptor.handler.name == "native.commit.create"
                && bound.as_ref().is_some_and(|b| {
                    serde_json::from_value::<super::super::native_git::CommitRequest>(
                        invocation.inputs["request"].clone(),
                    )
                    .is_ok_and(|r| {
                        super::super::native_git::run_covered(
                            &b.context,
                            &b.store,
                            &invocation.fence.run,
                            &r,
                        )
                    })
                });
        Ok(EffectPreparation {
            mutation,
            resources,
            approval: if mutation && !covered {
                let mut question = self.question(invocation)?;
                if let Some(title) = title {
                    question.title = title;
                }
                Some(question)
            } else {
                None
            },
            model_tools: None,
        })
    }
    fn invoke_effect(&self, effect: NeutralEffect) -> ObservationFuture {
        if matches!(
            effect.invocation.descriptor.handler.name.as_str(),
            "native.issue.triage" | "native.issue.handoff"
        ) {
            return self.issue_invoke(effect);
        }
        let binding = self.binding(&effect.invocation);
        if !self.native(&effect.invocation.descriptor) {
            let prepared = (|| {
                let bound = binding?;
                let state = bound
                    .context
                    .current(Some(crate::agent_capabilities::Capability::Operator))?;
                if !effect.dispatched
                    || !effect.approved()
                    || self.prepare(&effect.invocation)? != effect.preparation
                    || bound
                        .store
                        .neutral_execution(&effect.key.fence.run)?
                        .effect(&effect.key)?
                        != &effect
                {
                    return Err(
                        "configured effect lacks exact current ledger approval/dispatch".into(),
                    );
                }
                Ok((bound, state))
            })();
            if effect.invocation.descriptor.handler.name == "host.approval" {
                return Box::pin(async move {
                    prepared?;
                    Ok(super::super::neutral_http::completed(BTreeMap::from([(
                        "approved".into(),
                        serde_json::json!(true),
                    )])))
                });
            }
            if families::script_target(&effect.invocation.descriptor) {
                return Box::pin(async move {
                    let (bound, _) = prepared?;
                    let adapter = Self::script(&bound)?;
                    tokio::task::spawn_blocking(move || {
                        bound
                            .context
                            .current(Some(crate::agent_capabilities::Capability::Operator))?;
                        Ok(adapter
                            .invoke(&effect, bound.context.workspace_path())
                            .effect)
                    })
                    .await
                    .map_err(|e| format!("script worker ownership uncertain: {e}"))?
                });
            }
            if super::super::native_agent::is_model(&effect.invocation.descriptor.handler.name) {
                return self.invoke_model(effect, prepared);
            }
            let adapter = prepared.and_then(|(bound, _)| {
                self.connector(&effect.invocation, Some(bound.context.clone()), true)
                    .map(|adapter| (bound.context, adapter))
            });
            return Box::pin(async move {
                let (context, adapter) = adapter?;
                context.current(Some(crate::agent_capabilities::Capability::Operator))?;
                adapter.host().invoke_effect(effect).await
            });
        }
        Box::pin(async move {
            let bound = binding?;
            tokio::task::spawn_blocking(move || native_dispatch::invoke(&bound, &effect))
                .await
                .map_err(|e| format!("native worker ownership uncertain: {e}"))
        })
    }

    fn reconcile(&self, effect: NeutralEffect) -> ObservationFuture {
        if effect.invocation.descriptor.handler.name == super::super::native_operator::HANDLER {
            let binding = self.binding(&effect.invocation);
            return Box::pin(async move {
                let bound = binding?;
                let request = super::super::native_operator::request(&effect.invocation.inputs)?;
                let receipt = super::super::native_operator::receipt(&effect, &request)?;
                if receipt.answer.accept
                    && !matches!(
                        request.decision,
                        super::super::native_operator::OperatorKind::Resume
                    )
                {
                    return Err("original operator action requires retained service evidence; no write repeated".into());
                }
                Ok(native_dispatch::invoke(&bound, &effect))
            });
        }
        if effect.invocation.descriptor.handler.name == "host.model" {
            return self.reconcile_model(effect);
        }
        if super::super::native_batch::HANDLERS
            .contains(&effect.invocation.descriptor.handler.name.as_str())
        {
            // The ledger write and its receipt commit together: an absent receipt means
            // nothing was recorded, and recovery never repeats the write.
            let binding = self.binding(&effect.invocation);
            return Box::pin(async move {
                let bound = binding?;
                super::super::native_batch::retained(
                    &bound.store,
                    &effect.key.fence.run,
                    &effect.key.digest,
                )?
                .ok_or("batch effect has no retained receipt; no ledger write repeated")?;
                Ok(native_dispatch::invoke(&bound, &effect))
            });
        }
        if effect.invocation.descriptor.handler.name == super::super::native_issue_prepare::HANDLER
        {
            let binding = self.binding(&effect.invocation);
            return Box::pin(async move {
                let bound = binding?;
                let backend = crate::stories::story_backend_for(
                    &bound.context.workspace_path().to_string_lossy(),
                )?;
                let receipt = super::super::native_issue_prepare::reconcile(
                    &bound,
                    &effect,
                    backend.as_ref(),
                )?;
                Ok(super::super::neutral_http::completed(BTreeMap::from([(
                    "receipt".into(),
                    serde_json::to_value(receipt).map_err(|e| e.to_string())?,
                )])))
            });
        }
        if effect.invocation.descriptor.handler.name == "native.story.create" {
            let binding = self.binding(&effect.invocation);
            return Box::pin(async move {
                let bound = binding?;
                let receipt = super::super::native_story_create::reconcile(
                    &bound.context,
                    &bound.store,
                    &effect,
                )?;
                Ok(super::super::neutral_http::completed(BTreeMap::from([(
                    "receipt".into(),
                    serde_json::to_value(receipt).map_err(|e| e.to_string())?,
                )])))
            });
        }
        if effect.invocation.descriptor.handler.name == "native.issue.handoff" {
            return self.issue_observe(effect, false);
        }
        if families::script_target(&effect.invocation.descriptor) {
            let adapter = self
                .binding(&effect.invocation)
                .and_then(|b| Self::script(&b));
            return Box::pin(async move { Ok(adapter?.reconcile(&effect)) });
        }
        if !self.native(&effect.invocation.descriptor)
            && !super::super::native_agent::is_model(&effect.invocation.descriptor.handler.name)
        {
            let adapter = self
                .binding(&effect.invocation)
                .and_then(|b| self.connector(&effect.invocation, Some(b.context), true));
            return match adapter {
                Ok(adapter) => adapter.host().reconcile(effect),
                Err(error) => Box::pin(async { Err(error) }),
            };
        }
        Box::pin(async {
            Err("original native/model effect requires retained service evidence; no operation repeated".into())
        })
    }

    fn cancel(&self, effect: NeutralEffect) -> ObservationFuture {
        if effect.invocation.descriptor.handler.name == "native.issue.handoff" {
            return self.issue_observe(effect, true);
        }
        if effect.invocation.descriptor.handler.name == "host.model" {
            return Box::pin(async {
                Err("model cancellation cannot start a recovery prompt; original transport stop must reconcile".into())
            });
        }
        if families::script_target(&effect.invocation.descriptor) {
            let adapter = self
                .binding(&effect.invocation)
                .and_then(|b| Self::script(&b));
            return Box::pin(async move { Ok(adapter?.cancel(&effect)) });
        }
        if !self.native(&effect.invocation.descriptor)
            && !super::super::native_agent::is_model(&effect.invocation.descriptor.handler.name)
        {
            let adapter = self
                .binding(&effect.invocation)
                .and_then(|b| self.connector(&effect.invocation, Some(b.context), true));
            return match adapter {
                Ok(adapter) => adapter.host().cancel(effect),
                Err(error) => Box::pin(async { Err(error) }),
            };
        }
        self.reconcile(effect)
    }
}
