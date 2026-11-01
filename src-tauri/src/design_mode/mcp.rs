//! Agent-facing Design Mode lifecycle uses the same core as HTTP and IPC.
use crate::AppState;
use serde_json::{Value, json};
use std::sync::Arc;

pub(crate) fn tool_definition() -> Value {
    json!({
        "name": "design_mode",
        "description": "Start or stop Design Mode on the host. Use start with session_id and url as the default path for layout work: it binds the browser to the agent that printed the link and opens that HTTP(S) URL directly. Omit url to use dev-server discovery/onboarding. Requires a live terminal agent; an ACP coordinator should name the target terminal agent. stop, reload and navigate use repo_path from start/status; reload makes the browser reload the reviewed tab once, after you change the page; navigate opens HTTP(S) pages (local by default; remote hosts require an extension host grant); screenshot returns an image and path with format, quality (null for PNG), and byte size; choose WebP (default, quality 25) to read text and layout, quality 50+ for small glyphs and icons, PNG for pixel-exact detail. Optional max_long_edge downsizes before encoding. Token cost follows pixel dimensions, not format or bytes (Claude: about 1,296 tokens per megapixel, capped at 1,568 standard / 4,784 high-res); errors returns captured page errors; eval runs JavaScript only with an explicit optional host grant, including an all-sites grant. Browser commands return correlated replies within 10 seconds; status lists repository bindings. Browser extension setup is required once per repository profile.",
        "inputSchema": {"type":"object", "additionalProperties":false, "required":["action"], "properties": {
            "action":{"type":"string","enum":["start","stop","status","reload","navigate","screenshot","errors","eval"]},
            "session_id":{"type":"string","description":"Live agent PTY id, TUIC session, alias, unique prefix or display name; required for start"},
            "repo_path":{"type":"string","description":"Repository path returned by start/status; required for stop, reload and navigate"},
            "script":{"type":"string","description":"JavaScript expression for eval; requires an explicit host permission grant"},
            "format":{"type":"string","enum":["webp","png"],"default":"webp","description":"Screenshot encoding: WebP for text/layout, PNG for pixel-exact detail"},
            "quality":{"type":"integer","minimum":1,"maximum":100,"default":25,"description":"WebP quality; use 50+ for small glyphs and icons. Ignored for PNG"},
            "max_long_edge":{"type":"integer","minimum":1,"maximum":9007199254740991u64,"description":"Maximum screenshot long edge in pixels; downsize only, before encoding"},
            "browser":{"type":"string","enum":["chrome","edge"]},
            "url":{"type":"string","description":"Absolute HTTP or HTTPS URL, including localhost, opened directly on start; for navigate, the page to open"}
        }}
    })
}

pub(crate) async fn handle(state: &Arc<AppState>, args: &Value) -> Value {
    #[cfg(feature = "desktop")]
    let result = execute(state, args).await;
    #[cfg(not(feature = "desktop"))]
    let result: Result<Value, String> = {
        let _ = (state, args);
        Err("Design Mode requires the desktop host".into())
    };
    result.unwrap_or_else(|error| json!({"error":error}))
}

#[cfg(feature = "desktop")]
async fn execute(state: &Arc<AppState>, args: &Value) -> Result<Value, String> {
    let required = |key: &str| {
        args[key]
            .as_str()
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| format!("{key} is required"))
    };
    let optional = |key: &str| -> Result<Option<String>, String> {
        match args.get(key) {
            None => Ok(None),
            Some(Value::String(text)) => Ok(Some(text.clone())),
            Some(_) => Err(format!("{key} must be a string")),
        }
    };
    match args["action"].as_str() {
        Some("start") => {
            let reference = required("session_id")?;
            let (browser, url) = (optional("browser")?, optional("url")?);
            let session = state
                .resolve_session_ref_checked(&reference)?
                .ok_or("Agent session no longer exists")?;
            super::start(state, session, browser, url).await
        }
        Some("stop") => super::stop(state, &required("repo_path")?).await,
        Some(action @ ("reload" | "navigate" | "screenshot" | "errors" | "eval")) => {
            let repo = required("repo_path")?;
            let fields = match action {
                "navigate" => json!({"url":required("url")?}),
                "eval" => json!({"script":required("script")?}),
                "screenshot" => {
                    let mut fields = json!({});
                    for key in ["format", "quality", "max_long_edge"] {
                        if let Some(value) = args.get(key) {
                            fields[key] = value.clone();
                        }
                    }
                    fields
                }
                _ => json!({}),
            };
            super::browser_command(state, &repo, action, fields).await
        }
        Some("status") => Ok(json!(super::statuses(state).await)),
        _ => Err("Choose Design Mode start, stop, status, reload or navigate".into()),
    }
}

/// Fallback for clients that cache schemas: dispatch still rejects an unarmed caller.
pub(crate) fn owns_repo(state: &Arc<AppState>, session: Option<&str>, repo: &str) -> bool {
    #[cfg(feature = "desktop")]
    {
        state.design_mode.get().is_some_and(|manager| {
            manager
                .command_channels
                .lock()
                .unwrap()
                .get(repo)
                .is_some_and(|channel| Some(channel.session_id.as_str()) == session)
        })
    }
    #[cfg(not(feature = "desktop"))]
    {
        let _ = (state, session, repo);
        false
    }
}

pub(crate) fn visible_to(state: &Arc<AppState>, session: Option<&str>) -> bool {
    #[cfg(feature = "desktop")]
    {
        let Some(session) = session else {
            return false;
        };
        state.design_mode.get().is_some_and(|manager| {
            manager
                .command_channels
                .lock()
                .unwrap()
                .values()
                .any(|channel| channel.session_id == session)
        })
    }
    #[cfg(not(feature = "desktop"))]
    {
        let _ = (state, session);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn mcp_rejects_invalid_start_fields_without_launching() {
        // Catches: omitted or incorrectly typed fields reaching native launch.
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        for (args, expected) in [
            (json!({"action":"start"}), "session_id is required"),
            (json!({"action":"stop"}), "repo_path is required"),
            (
                json!({"action":"unknown"}),
                "Choose Design Mode start, stop, status, reload or navigate",
            ),
            (
                json!({"action":"navigate","url":"http://localhost/"}),
                "repo_path is required",
            ),
            (
                json!({"action":"navigate","repo_path":"/r"}),
                "url is required",
            ),
            (
                json!({"action":"start","session_id":"missing","url":4}),
                "url must be a string",
            ),
        ] {
            #[cfg(not(feature = "desktop"))]
            let expected = "Design Mode requires the desktop host";
            assert_eq!(handle(&state, &args).await["error"], expected, "{args}");
        }
    }
    #[tokio::test]
    async fn mcp_status_reads_the_shared_empty_lifecycle() {
        // Catches: status adapter inventing a different lifecycle snapshot.
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        #[cfg(feature = "desktop")]
        assert_eq!(handle(&state, &json!({"action":"status"})).await, json!([]));
        #[cfg(not(feature = "desktop"))]
        assert!(
            handle(&state, &json!({"action":"status"}))
                .await
                .get("error")
                .is_some()
        );
    }

    // Catches: an unarmed or different agent inheriting another session's browser tool.
    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn tool_visibility_tracks_arm_rebind_and_stop() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        assert!(!visible_to(&state, Some("one")));
        let manager = super::super::manager(&state).await;
        manager.arm_extension("/repo".into(), "one".into()).await;
        assert!(visible_to(&state, Some("one")));
        assert!(!visible_to(&state, Some("two")));
        assert!(!owns_repo(&state, Some("one"), "/other"));
        manager.arm_extension("/repo".into(), "two".into()).await;
        assert!(!visible_to(&state, Some("one")));
        assert!(owns_repo(&state, Some("two"), "/repo"));
        manager.stop("/repo").await.unwrap();
        assert!(!visible_to(&state, Some("two")));
    }
}
