//! Shared provider quota transport operations.
use crate::{
    AppState,
    provider_quota::{QuotaView, RefreshError, SubscriptionKey},
};
#[cfg(feature = "desktop")]
use tauri::State;

pub(crate) fn all(state: &AppState) -> Vec<QuotaView> {
    state.provider_quota.all()
}
pub(crate) fn binding(state: &AppState, session_id: &str) -> Option<SubscriptionKey> {
    state.provider_quota.binding(session_id)
}
pub(crate) async fn refresh(state: &AppState, key: &SubscriptionKey) -> Result<QuotaView, String> {
    state
        .provider_quota
        .refresh(key)
        .await
        .map_err(|e| match e {
            RefreshError::TooSoon { retry_at_ms } => {
                format!("Quota refresh is limited; retry at {retry_at_ms}")
            }
        })
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn get_provider_quotas(
    state: State<'_, std::sync::Arc<AppState>>,
) -> Result<Vec<QuotaView>, String> {
    Ok(all(&state))
}
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn get_session_quota_binding(
    state: State<'_, std::sync::Arc<AppState>>,
    session_id: String,
) -> Result<Option<SubscriptionKey>, String> {
    Ok(binding(&state, &session_id))
}
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn refresh_provider_quota(
    state: State<'_, std::sync::Arc<AppState>>,
    key: SubscriptionKey,
) -> Result<QuotaView, String> {
    refresh(&state, &key).await
}

pub(crate) fn spawn(state: std::sync::Arc<AppState>) {
    let mut bindings = state.provider_quota.subscribe_bindings();
    let binding_state = state.clone();
    tokio::spawn(async move {
        loop {
            match bindings.recv().await {
                Ok(change) => {
                    let payload = serde_json::to_value(&change).expect("quota binding serializes");
                    let _ = binding_state.event_bus.send(
                        crate::state::AppEvent::ProviderQuotaBindingChanged(payload.clone()),
                    );
                    #[cfg(feature = "desktop")]
                    {
                        use tauri::Emitter;
                        if let Some(app) = binding_state.app_handle.read().as_ref() {
                            let _ = app.emit("provider-quota-binding-changed", payload);
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    let mut changes = state.provider_quota.subscribe();
    tokio::spawn(async move {
        loop {
            match changes.recv().await {
                Ok(view) => {
                    let payload = serde_json::to_value(&view).expect("quota view serializes");
                    let _ = state
                        .event_bus
                        .send(crate::state::AppEvent::ProviderQuotaChanged(
                            payload.clone(),
                        ));
                    #[cfg(feature = "desktop")]
                    {
                        use tauri::Emitter;
                        if let Some(app) = state.app_handle.read().as_ref() {
                            let _ = app.emit("provider-quota-changed", payload);
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}
