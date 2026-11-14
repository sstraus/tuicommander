//! Backend-created native scope. Graph values select data, never caller identity.
mod provenance;
use super::check::{CheckReceipt, execute_run_check, git_output};
use super::neutral_agent::{CoarseArtifact, CoarseWorkspace, WorkspaceMutation};
use crate::agent_capabilities::{Capability, CapabilitySet};
use crate::state::AppState;
use crate::workflows::CheckDefinition;
use crate::workflows::data_reference::{DataReference, ReferenceInputs};
use crate::workflows::neutral_admission::HostContext;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

/// Backend-only authenticated identity. Never deserialize from workflow JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NativeCaller {
    Owner,
    Agent(String),
}

/// Constructed by a trusted backend binding, not deserialized from graph inputs.
/// The caller and workspace must come from that binding's transport/run provenance.
#[derive(Clone)]
pub(crate) struct NativeContext {
    state: Weak<AppState>,
    caller: NativeCaller,
    caller_id: String,
    ceiling: Option<CapabilitySet>,
    project: PathBuf,
    pub(super) workspace: Option<CoarseWorkspace>,
    checks: Vec<CheckDefinition>,
    /// The run this context was bound to, set only by the native host binding.
    bound: Option<(String, super::RunStore)>,
}

impl NativeContext {
    pub(super) fn require_artifact(&self, commit: &str, tree: &str) -> Result<(), String> {
        self.current(None)?;
        self.git_workspace()?;
        if super::check::staged_artifact(self.workspace_path())? != (commit.into(), tree.into()) {
            return Err("native artifact is stale".into());
        }
        Ok(())
    }

    pub(crate) fn pin_artifact(&self, paths: &[String]) -> Result<CoarseArtifact, String> {
        self.current(None)?;
        self.git_workspace()?.pin_artifact(paths)
    }

    pub(crate) fn verify(
        &self,
        check_id: &str,
        commit: &str,
        tree: &str,
        db: &Path,
        run: &str,
    ) -> Result<CheckReceipt, String> {
        self.require_artifact(commit, tree)?;
        let check = self
            .checks
            .iter()
            .find(|c| c.id == check_id)
            .ok_or("native verification definition unavailable")?;
        // The existing runner owns cancellation/process stop and verifies that
        // the command left the same clean branch, commit and tree.
        let receipt = execute_run_check(check, self.workspace_path(), db, run, false)?;
        if receipt.exit_code < 0 {
            return Err("native verification did not finish normally".into());
        }
        Ok(receipt)
    }

    /// Observations retain a failed command's receipt; infrastructure still fails closed.
    pub(crate) fn verify_observed(
        &self,
        check_id: &str,
        commit: &str,
        tree: &str,
        db: &Path,
        run: &str,
    ) -> Result<CheckReceipt, String> {
        self.verify(check_id, commit, tree, db, run)
    }

    pub(super) fn matches_check(&self, receipt: &CheckReceipt) -> bool {
        self.checks
            .iter()
            .any(|c| c.id == receipt.check_id && c.argv == receipt.argv)
    }

    pub(crate) fn verification_check_names(&self) -> Vec<String> {
        self.checks.iter().map(|check| check.id.clone()).collect()
    }

    /// The same context, bound to the run whose ledger admission may read.
    pub(super) fn with_run(mut self, run: &str, store: super::RunStore) -> Self {
        self.bound = Some((run.into(), store));
        self
    }

    pub(super) fn bound_run(&self) -> Option<(&str, &super::RunStore)> {
        self.bound
            .as_ref()
            .map(|(run, store)| (run.as_str(), store))
    }

    pub(crate) fn workspace_path(&self) -> &Path {
        &self.project
    }

    pub(super) fn git_workspace(&self) -> Result<&CoarseWorkspace, String> {
        self.workspace
            .as_ref()
            .ok_or_else(|| "native Git workspace unavailable for directory-only run".into())
    }

    pub(super) fn verify_identity(&self) -> Result<(), String> {
        if let Some(workspace) = &self.workspace {
            return workspace.verify_identity();
        }
        if self.project.is_dir()
            && self.project.canonicalize().map_err(|e| e.to_string())? == self.project
        {
            Ok(())
        } else {
            Err("native project identity changed".into())
        }
    }

    pub(super) fn belongs_to(&self, state: &Weak<AppState>) -> bool {
        self.state.ptr_eq(state)
    }

    pub(super) fn actor(&self) -> Option<&str> {
        match &self.caller {
            NativeCaller::Owner => None,
            NativeCaller::Agent(key) => Some(key),
        }
    }

    pub(super) fn caller(&self) -> &str {
        &self.caller_id
    }
}

impl HostContext for NativeContext {
    fn for_subflow(&self, references: &ReferenceInputs) -> Result<Box<dyn HostContext>, String> {
        self.current(None)?;
        let workspace = self.workspace_path().to_string_lossy();
        if !references.fields.is_empty() {
            self.git_workspace()?;
        }
        for reference in references.fields.values() {
            match reference {
                DataReference::Workspace { id } if id == workspace.as_ref() => {}
                DataReference::Artifact { id, revision }
                    if id == workspace.as_ref()
                        && revision
                            == &git_output(self.workspace_path(), &["rev-parse", "HEAD"])? => {}
                _ => return Err("native subflow reference exceeds inherited scope".into()),
            }
        }
        Ok(Box::new(self.clone()))
    }
}

impl NativeContext {
    pub(super) fn authorize(
        &self,
        store: &super::RunStore,
        key: &super::neutral_effects::EffectKey,
        handler: &str,
        request: &(impl serde::Serialize + serde::de::DeserializeOwned),
        capability: Option<Capability>,
    ) -> Result<Arc<AppState>, String> {
        self.authorize_with(store, key, handler, request, capability, true)
    }

    /// Exact ownership without an answered question; only callers that re-prove a
    /// named run-start coverage rule (native_git::run_covered) may use it.
    pub(super) fn authorize_covered(
        &self,
        store: &super::RunStore,
        key: &super::neutral_effects::EffectKey,
        handler: &str,
        request: &(impl serde::Serialize + serde::de::DeserializeOwned),
    ) -> Result<Arc<AppState>, String> {
        self.authorize_with(store, key, handler, request, None, false)
    }

    fn authorize_with<T: serde::Serialize + serde::de::DeserializeOwned>(
        &self,
        store: &super::RunStore,
        key: &super::neutral_effects::EffectKey,
        handler: &str,
        request: &T,
        capability: Option<Capability>,
        approval: bool,
    ) -> Result<Arc<AppState>, String> {
        let state = self.current(capability)?;
        let snapshot = store.neutral_execution(&key.fence.run)?;
        let effect = snapshot.effect(key)?;
        let subject = effect
            .invocation
            .subject
            .as_ref()
            .ok_or("native workspace subject absent")?;
        let workspace = subject
            .get("workspace")
            .ok_or("native workspace reference absent")?;
        if DataReference::parse(workspace)?
            != (DataReference::Workspace {
                id: self.workspace_path().to_string_lossy().into_owned(),
            })
        {
            return Err("native workspace subject differs from trusted binding".into());
        }
        if snapshot.status != super::neutral_execution::NeutralStatus::Running
            || !effect.dispatched
            || effect.cancel_started
            || effect.observation.is_some()
            || effect.invocation.descriptor.handler.name != handler
            || effect.invocation.descriptor.handler.version != 1
            || effect.invocation.inputs.len() != 1
            || canonical::<T>(effect.invocation.inputs.get("request"))?
                != serde_json::to_value(request).map_err(|e| e.to_string())?
            || !effect.preparation.resources.contains(&format!(
                "native:workspace:{}",
                self.git_workspace()?.git_dir
            ))
            || (approval
                && (effect.preparation.approval.is_none()
                    || effect.answer != Some(true)
                    || effect
                        .question
                        .as_ref()
                        .is_none_or(|q| q.deadline_ms <= super::store::now_ms())))
            || (!approval && effect.preparation.approval.is_some())
        {
            return Err("native write lacks current exact effect ownership and approval".into());
        }
        Ok(state)
    }
}

/// The approved request in its typed form. A mapped input can spell an optional field
/// as `null` while the typed request omits it on serialization (a real model sent
/// `"storyType": null`); both are the same request, so compare canonical values.
fn canonical<T: serde::Serialize + serde::de::DeserializeOwned>(
    stored: Option<&serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let stored = stored.ok_or("native write lacks its approved request")?;
    let typed: T = serde_json::from_value(stored.clone()).map_err(|e| e.to_string())?;
    serde_json::to_value(typed).map_err(|e| e.to_string())
}
