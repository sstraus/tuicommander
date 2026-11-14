//! Authoring access to the same backend-installed execution host. Client JSON
//! cannot supply native descriptors, admission registries or trusted context.
use super::catalog::WorkflowCatalog;
use super::neutral_admission::HostContext;

pub(crate) trait NeutralCatalogHost: super::run::NeutralHost {
    fn requires_trusted_caller(&self) -> bool {
        false
    }
    fn resume_run(
        &self,
        _store: &super::RunStore,
        _run: &str,
        _caller: Option<&super::NativeCaller>,
        _project: &str,
    ) -> Result<bool, String> {
        Ok(false)
    }
    fn authorize_answer(
        &self,
        _store: &super::RunStore,
        _run: &str,
        _caller: Option<&super::NativeCaller>,
    ) -> Result<(), String> {
        Ok(())
    }

    fn authorize_effect_answer(
        &self,
        store: &super::RunStore,
        run: &str,
        key: &super::run::NeutralEffectKey,
        caller: Option<&super::NativeCaller>,
    ) -> Result<(), String> {
        let _ = key;
        self.authorize_answer(store, run, caller)
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "exact ledger fence and authenticated caller stay explicit at host boundary"
    )]
    fn operator_answer(
        &self,
        _store: &super::RunStore,
        _run: &str,
        _key: &super::run::NeutralEffectKey,
        _revision: u64,
        _sequence: i64,
        _answer: &super::run::native_operator::OperatorAnswer,
        _caller: Option<&super::NativeCaller>,
    ) -> Result<super::run::neutral_effects::EffectCommand, String> {
        Err("typed native operator service unavailable".into())
    }

    fn bind_run(
        &self,
        _store: &super::RunStore,
        _run: &str,
        _context: super::NativeContext,
    ) -> Result<(), String> {
        Err("native execution not available".into())
    }
    fn verification_checks(
        &self,
        _project: &std::path::Path,
        names: &[String],
    ) -> Result<Vec<super::CheckDefinition>, String> {
        if names.is_empty() {
            Ok(Vec::new())
        } else {
            Err("native verification configuration unavailable".into())
        }
    }
    fn catalog(&self, project: &str) -> Result<WorkflowCatalog, String>;
    fn publication_context(&self, project: &str) -> Result<Box<dyn HostContext>, String>;
}
