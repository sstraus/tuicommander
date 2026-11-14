//! Core-owned account registry. Public replies contain metadata and presence only.
mod api;
// DEFERRED (2026-10-11) — runtime caller arrives with story 1675-ccd4 (settings-mcp catalogue); ungate there.
#[cfg(test)]
mod catalogue;
mod edit;
#[cfg(test)]
pub(crate) use catalogue::catalogue_schema;
mod model;
mod providers;
mod store;
mod verify;
pub(crate) use verify::read_verified_identity;
#[cfg(test)]
mod critic_tests;
#[cfg(test)]
mod tests;

pub(crate) use api::{execute, load_credential};
#[cfg(test)]
pub(crate) use edit::list_accounts;
pub(crate) use edit::{add_account, precheck_new, set_secret};
pub(crate) use model::{Account, AccountAction, AuthKind, NewAccount};
#[cfg(test)]
pub(crate) use providers::VerifyRequest;
pub(crate) use providers::{ProviderDescriptor, register_provider, unregister_providers};
pub(crate) use store::installed_accounts;
pub(crate) use store::{github_account, github_accounts, remove, upsert_github};

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn account_action(
    state: tauri::State<'_, std::sync::Arc<crate::state::AppState>>,
    request: AccountAction,
) -> Result<serde_json::Value, String> {
    execute(state.inner(), request).await
}
