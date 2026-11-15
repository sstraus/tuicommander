use super::*;

impl NativeContext {
    pub(crate) fn capture(
        state: &Arc<AppState>,
        caller: NativeCaller,
        workspace: &Path,
        check_names: Vec<String>,
    ) -> Result<Self, String> {
        Self::capture_scope(state, caller, workspace, check_names, true)
    }

    pub(in crate::workflows::run) fn capture_project(
        state: &Arc<AppState>,
        caller: NativeCaller,
        project: &Path,
    ) -> Result<Self, String> {
        Self::capture_scope(state, caller, project, Vec::new(), false)
    }

    fn capture_scope(
        state: &Arc<AppState>,
        caller: NativeCaller,
        project: &Path,
        check_names: Vec<String>,
        require_git: bool,
    ) -> Result<Self, String> {
        let project = project.canonicalize().map_err(|e| e.to_string())?;
        if !project.is_dir() {
            return Err("native project must be a directory".into());
        }
        let workspace = require_git
            .then(|| CoarseWorkspace::capture_identity(&project, WorkspaceMutation::Mutate))
            .transpose()?;
        let checks = state
            .workflow_runtime
            .neutral_catalog_host()?
            .verification_checks(&project, &check_names)?;
        crate::workflows::store::validate_checks(&checks)?;
        let (caller_id, ceiling) = match &caller {
            NativeCaller::Owner => ("owner".into(), None),
            NativeCaller::Agent(key) => (
                format!("agent:{key}"),
                Some(
                    state
                        .agent_capabilities
                        .get(key)
                        .ok_or("native caller has no backend capability binding")?
                        .clone(),
                ),
            ),
        };
        Ok(Self {
            state: Arc::downgrade(state),
            caller,
            caller_id,
            ceiling,
            project,
            workspace,
            checks,
            bound: None,
        })
    }

    pub(in crate::workflows::run) fn current(
        &self,
        required: Option<Capability>,
    ) -> Result<Arc<AppState>, String> {
        let state = self
            .state
            .upgrade()
            .ok_or("native service owner unavailable")?;
        if let NativeCaller::Agent(key) = &self.caller {
            let current = state
                .agent_capabilities
                .get(key)
                .ok_or("native caller binding revoked")?;
            if let Some(capability) = required
                && (!self
                    .ceiling
                    .as_ref()
                    .is_some_and(|c| c.contains(capability))
                    || !current.contains(capability))
            {
                return Err(format!(
                    "native caller lacks current {} authority",
                    capability.as_str()
                ));
            }
        }
        // Identity is checked without requiring a clean index before an approved commit.
        self.verify_identity()?;
        Ok(state)
    }
}
