use super::*;
use std::sync::Arc;
use tokio::sync::Notify;

struct GatedEffects {
    inner: Effects,
    launch_gate: bool,
    entered: Notify,
    release: Notify,
    published: Notify,
}

impl DispatchEffects for GatedEffects {
    async fn workspace(
        &self,
        definition: &AutomationDefinition,
        id: &str,
    ) -> Result<ResolvedWorkspace, String> {
        if !self.launch_gate {
            self.entered.notify_one();
            self.release.notified().await;
        }
        self.inner.workspace(definition, id).await
    }

    async fn launch(&self, request: LaunchRequest) -> Result<LaunchBinding, String> {
        if self.launch_gate {
            self.entered.notify_one();
            self.release.notified().await;
        }
        self.inner.launch(request).await
    }

    fn stop(&self, binding: &LaunchBinding) -> Result<(), String> {
        self.inner.stop(binding)
    }

    fn publish(&self, _: &AutomationRun) {
        self.published.notify_one();
    }
}

// Catches: cancellation at workspace or launch stranding a manual run before binding.
#[tokio::test]
async fn run_now_request_cancellation_still_binds_owned_launch() {
    for (launch_gate, fail_launch, http) in [
        (false, false, false),
        (true, false, false),
        (true, true, false),
        (true, false, true),
    ] {
        let (fixture, _) = Fixture::new(None, true);
        let mut original = Arc::try_unwrap(fixture.dispatcher).ok().unwrap();
        original.effects.fail_launch = fail_launch;
        let dispatcher = Arc::new(Dispatcher {
            definitions: original.definitions,
            runs: original.runs,
            effects: GatedEffects {
                inner: original.effects,
                launch_gate,
                entered: Notify::new(),
                release: Notify::new(),
                published: Notify::new(),
            },
        });
        let caller = dispatcher.clone();
        // Real HTTP disconnect probes axum's lifecycle; explicit abort above
        // also protects callers whose transport actually drops handler futures.
        let mut server = None;
        let request = if http {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let app = axum::Router::new().route(
                "/run",
                axum::routing::post(move || {
                    let caller = caller.clone();
                    async move { axum::Json(caller.run_now("once").await.unwrap()) }
                }),
            );
            server = Some(tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap()
            }));
            tokio::spawn(async move {
                let _response = reqwest::Client::new()
                    .post(format!("http://{address}/run"))
                    .send()
                    .await
                    .unwrap();
            })
        } else {
            tokio::spawn(async move {
                caller.run_now("once").await.unwrap();
            })
        };
        dispatcher.effects.entered.notified().await;
        request.abort();
        assert!(request.await.unwrap_err().is_cancelled());
        dispatcher.effects.release.notify_one();
        let finished = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            dispatcher.effects.published.notified(),
        )
        .await;
        let saved = dispatcher.runs.history(None, 1, 0).unwrap().remove(0);
        assert_eq!(
            saved.status,
            if fail_launch {
                RunStatus::Failed
            } else {
                RunStatus::Running
            },
            "cancelled at launch={launch_gate}, http={http}; completion={finished:?}"
        );
        if fail_launch {
            assert_eq!(saved.reason.as_deref(), Some("agent spawn refused"));
            assert!(saved.session_id.is_none());
            continue;
        }
        assert_eq!(saved.session_id.as_deref(), Some("owned-session"));
        assert_eq!(saved.task_id.as_deref(), Some("owned-task"));
        assert_eq!(dispatcher.effects.inner.calls.lock().len(), 1);
        if let Some(server) = server {
            server.abort();
            assert!(server.await.unwrap_err().is_cancelled());
        }
    }
}
