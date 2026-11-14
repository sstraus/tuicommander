//! One installed provider composes native services and configured activity families.
mod admission;
mod families;
mod issue;
mod model;
mod runtime;
use super::RunStore;
use super::native_operations::NativeContext;
use super::neutral_effects::{EffectPreparation, EffectQuestion, NeutralEffect};
use super::neutral_runtime::{NeutralHost, NeutralInvocation, ObservationFuture};
use super::{native_config, native_connectors, native_dispatch, native_github, native_model};
use crate::state::AppState;
use crate::workflows::catalog::{ActivityDescriptor, ActivityInstance, WorkflowCatalog};
use crate::workflows::neutral_admission::{HostAdmission, HostContext, HostRegistry};
use crate::workflows::neutral_host::NeutralCatalogHost;
use crate::workflows::neutral_publication::NeutralPublication;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Weak};

struct PublicationContext {
    project: String,
}
impl HostContext for PublicationContext {}

#[derive(Clone)]
pub(super) struct NativeBinding {
    pub context: NativeContext,
    pub store: RunStore,
}

pub(crate) struct NativeHost {
    issue_services:
        parking_lot::RwLock<Option<Arc<dyn super::native_issue_service::IssueServices>>>,
    state: Weak<AppState>,
    catalog: WorkflowCatalog,
    checks:
        parking_lot::RwLock<BTreeMap<std::path::PathBuf, Vec<crate::workflows::CheckDefinition>>>,
    bindings: parking_lot::Mutex<BTreeMap<String, NativeBinding>>,
    installed: parking_lot::RwLock<BTreeMap<String, native_config::InstalledFamilies>>,
}
impl NativeHost {
    pub(crate) fn install(state: &Arc<AppState>) -> Result<Arc<Self>, String> {
        let mut catalog = crate::workflows::native_operation_catalog()?;
        super::correction_native::register(&mut catalog)?;
        super::correction_native::register_policy(&mut catalog)?;
        let host = Arc::new(Self {
            issue_services: parking_lot::RwLock::new(Some(Arc::new(
                super::native_issue_mediated::MediatedIssueServices,
            ))),
            state: Arc::downgrade(state),
            catalog,
            checks: Default::default(),
            bindings: Default::default(),
            installed: Default::default(),
        });
        state
            .workflow_runtime
            .install_neutral_catalog_host(host.clone());
        Ok(host)
    }

    /// Backend configuration entry; command definitions never come from a run.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "project verification configuration owner entry")
    )]
    pub(crate) fn install_project_checks(
        &self,
        project: &std::path::Path,
        checks: Vec<crate::workflows::CheckDefinition>,
    ) -> Result<(), String> {
        crate::workflows::store::validate_checks(&checks)?;
        let project = project.canonicalize().map_err(|e| e.to_string())?;
        self.checks.write().insert(project, checks);
        Ok(())
    }

    /// Only a backend caller holding a real NativeContext can bind a run.
    /// Volatile provenance is deliberately unavailable after restart until rebound.
    pub(crate) fn bind_run(
        &self,
        store: &RunStore,
        run: &str,
        context: NativeContext,
    ) -> Result<(), String> {
        context.current(None)?;
        let snapshot = store.neutral_execution(run)?;
        if !context.belongs_to(&self.state)
            || snapshot.publication.project != context.workspace_path().to_string_lossy()
        {
            return Err("native context does not belong to this provider/project".into());
        }
        let mut bindings = self.bindings.lock();
        if let Some(bound) = bindings.get(run) {
            if bound.context.caller() != context.caller()
                || bound.context.workspace != context.workspace
                || bound.store.database_path() != store.database_path()
            {
                return Err("native run already has a different trusted binding".into());
            }
            return Ok(());
        }
        bindings.insert(
            run.into(),
            NativeBinding {
                context,
                store: store.clone(),
            },
        );
        Ok(())
    }

    pub(super) fn binding(&self, invocation: &NeutralInvocation) -> Result<NativeBinding, String> {
        self.state
            .upgrade()
            .ok_or("native provider owner unavailable")?;
        self.bindings
            .lock()
            .get(&invocation.fence.run)
            .cloned()
            .ok_or_else(|| "native trusted caller/run binding unavailable".into())
    }

    fn admitted_context(context: &dyn HostContext) -> Result<&NativeContext, String> {
        (context as &dyn std::any::Any)
            .downcast_ref::<NativeContext>()
            .ok_or_else(|| "native dispatch requires backend caller provenance".into())
    }
}

impl NeutralCatalogHost for NativeHost {
    fn requires_trusted_caller(&self) -> bool {
        true
    }
    fn resume_run(
        &self,
        store: &RunStore,
        run: &str,
        caller: Option<&super::NativeCaller>,
        project: &str,
    ) -> Result<bool, String> {
        let bindings = self.bindings.lock();
        let Some(bound) = bindings.get(run) else {
            return Ok(false);
        };
        let same = match caller {
            Some(super::NativeCaller::Owner) => bound.context.actor().is_none(),
            Some(super::NativeCaller::Agent(key)) => bound.context.actor() == Some(key.as_str()),
            None => false,
        };
        if !same
            || bound.store.database_path() != store.database_path()
            || bound.context.workspace_path().to_string_lossy() != project
        {
            return Err("native run already has a different trusted binding".into());
        }
        bound.context.current(None)?;
        Ok(true)
    }
    fn authorize_answer(
        &self,
        store: &RunStore,
        run: &str,
        caller: Option<&super::NativeCaller>,
    ) -> Result<(), String> {
        let bindings = self.bindings.lock();
        let bound = bindings
            .get(run)
            .ok_or("native trusted caller/run binding unavailable")?;
        if bound.store.database_path() != store.database_path() {
            return Err("native answer store differs from trusted binding".into());
        }
        match caller {
            Some(super::NativeCaller::Owner) => bound.context.verify_identity(),
            Some(super::NativeCaller::Agent(key))
                if bound.context.actor() == Some(key.as_str()) =>
            {
                bound
                    .context
                    .current(Some(crate::agent_capabilities::Capability::Operator))
                    .map(|_| ())
            }
            _ => Err("native answer requires authenticated owner or bound Operator agent".into()),
        }
    }

    fn authorize_effect_answer(
        &self,
        store: &RunStore,
        run: &str,
        key: &super::NeutralEffectKey,
        caller: Option<&super::NativeCaller>,
    ) -> Result<(), String> {
        self.authorize_answer(store, run, caller)?;
        let snapshot = store.neutral_execution(run)?;
        if snapshot
            .effect(key)?
            .invocation
            .descriptor
            .handler
            .name
            .starts_with("native.operator.")
        {
            return Err("operator wait requires typed operator_answer, not effect approval".into());
        }
        Ok(())
    }
    fn operator_answer(
        &self,
        store: &RunStore,
        run: &str,
        key: &super::NeutralEffectKey,
        revision: u64,
        sequence: i64,
        answer: &super::native_operator::OperatorAnswer,
        caller: Option<&super::NativeCaller>,
    ) -> Result<super::neutral_effects::EffectCommand, String> {
        self.authorize_answer(store, run, caller)?;
        if !matches!(caller, Some(super::NativeCaller::Owner)) || key.fence.run != run {
            return Err("operator consent requires authenticated human owner and exact run".into());
        }
        let bound = self
            .bindings
            .lock()
            .get(run)
            .cloned()
            .ok_or("native binding unavailable")?;
        super::native_operator::answer(&bound, key, revision, sequence, answer)
    }

    fn bind_run(&self, store: &RunStore, run: &str, context: NativeContext) -> Result<(), String> {
        NativeHost::bind_run(self, store, run, context)
    }
    fn verification_checks(
        &self,
        project: &std::path::Path,
        names: &[String],
    ) -> Result<Vec<crate::workflows::CheckDefinition>, String> {
        let checks = self.checks.read();
        let definitions = checks.get(project);
        names
            .iter()
            .map(|name| {
                definitions
                    .and_then(|checks| checks.iter().find(|check| &check.id == name))
                    .cloned()
                    .ok_or_else(|| format!("unknown host verification check: {name}"))
            })
            .collect()
    }

    fn catalog(&self, project: &str) -> Result<WorkflowCatalog, String> {
        let state = self
            .state
            .upgrade()
            .ok_or("native provider owner unavailable")?;
        let installed = native_config::InstalledFamilies::load(&state)?;
        let mut catalog = self.catalog.clone();
        for descriptor in installed.catalog.descriptors() {
            catalog.register(descriptor.clone())?;
        }
        self.installed.write().insert(project.into(), installed);
        Ok(catalog)
    }
    fn publication_context(&self, project: &str) -> Result<Box<dyn HostContext>, String> {
        if project.trim().is_empty() {
            return Err("native publication project is empty".into());
        }
        Ok(Box::new(PublicationContext {
            project: project.into(),
        }))
    }
}

#[cfg(test)]
mod catalog_fixture;
