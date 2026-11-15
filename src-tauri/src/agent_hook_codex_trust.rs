//! Native Codex trust is authoritative; file presence is only installation state.
//! Hashes and keys come from hooks/list, never from a local hash implementation.

use crate::agent_hook::HookEntry;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

/// Serializes the whole Codex install/read/trust/readback transaction in this host.
pub(crate) static INSTALL_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
pub(crate) const REVIEW_REQUIRED: &str = "Codex hook review required. Open /hooks in Codex to review the current definitions, then refresh Settings. Profile-specific hooks must be reviewed in that profile.";

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Hook {
    key: String,
    current_hash: String,
    event_name: String,
    handler_type: String,
    command: Option<String>,
    matcher: Option<String>,
    source_path: std::path::PathBuf,
    source: String,
    enabled: bool,
    is_managed: bool,
    trust_status: String,
    #[serde(rename = "async")]
    asynchronous: bool,
    timeout_sec: u64,
    status_message: Option<String>,
    additional_context_limit: Option<u64>,
}

#[derive(Deserialize)]
struct Listing {
    data: Vec<ListEntry>,
}

#[derive(Deserialize)]
struct ListEntry {
    cwd: String,
    hooks: Vec<Hook>,
    warnings: Vec<String>,
    errors: Vec<Value>,
}

fn owned_hooks(list: Listing, settings: &Path, map: &[HookEntry]) -> Result<Vec<Hook>, String> {
    let home = settings.parent().ok_or(REVIEW_REQUIRED)?;
    if list.data.len() != 1 {
        return Err(REVIEW_REQUIRED.into());
    }
    let entry = list.data.into_iter().next().ok_or(REVIEW_REQUIRED)?;
    if Path::new(&entry.cwd) != home || !entry.errors.is_empty() || !entry.warnings.is_empty() {
        return Err(REVIEW_REQUIRED.into());
    }
    let mut owned = Vec::new();
    for (event, matcher, command) in map {
        let mut matches = entry.hooks.iter().filter(|hook| {
            hook.source_path == settings
                && hook.source == "user"
                && !hook.is_managed
                && hook.handler_type == "command"
                && hook.command.as_deref() == Some(command.as_str())
                && hook.event_name.eq_ignore_ascii_case(event)
                && hook.matcher.as_deref().unwrap_or("") == *matcher
                && !hook.asynchronous
                && hook.timeout_sec == 600
                && hook.status_message.as_deref() == Some(crate::agent_hook::CODEX_HOOK_LABEL)
                && hook.additional_context_limit.is_none()
                && hook.enabled
                && !hook.key.is_empty()
                && !hook.current_hash.is_empty()
                && matches!(
                    hook.trust_status.as_str(),
                    "trusted" | "untrusted" | "modified"
                )
        });
        let hook = matches.next().ok_or(REVIEW_REQUIRED)?;
        if matches.next().is_some() || owned.iter().any(|h: &Hook| h.key == hook.key) {
            return Err(REVIEW_REQUIRED.into());
        }
        owned.push(hook.clone());
    }
    if owned.is_empty() {
        return Err(REVIEW_REQUIRED.into());
    }
    Ok(owned)
}

fn trust_edits(hooks: &[Hook]) -> Vec<Value> {
    hooks
        .iter()
        .filter(|hook| hook.trust_status != "trusted")
        .map(|hook| {
            // JSON strings use the same escaping needed by a TOML basic key.
            let key = serde_json::to_string(&hook.key).expect("string serialization");
            json!({"keyPath": format!("hooks.state.{key}.trusted_hash"),
                "value": hook.current_hash, "mergeStrategy": "replace"})
        })
        .collect()
}

fn config_version(config: &Value, path: &Path) -> Result<String, String> {
    if config
        .pointer("/config/features/hooks")
        .and_then(Value::as_bool)
        != Some(true)
        || config
            .pointer("/config/profile")
            .is_some_and(|v| !v.is_null())
    {
        return Err(REVIEW_REQUIRED.into());
    }
    let layers = config["layers"].as_array().ok_or(REVIEW_REQUIRED)?;
    let layer = layers
        .iter()
        .find(|layer| {
            layer.pointer("/name/type").and_then(Value::as_str) == Some("user")
                && layer
                    .pointer("/name/file")
                    .and_then(Value::as_str)
                    .map(Path::new)
                    == Some(path)
                && layer.pointer("/name/profile").is_none_or(Value::is_null)
        })
        .ok_or(REVIEW_REQUIRED)?;
    layer["version"]
        .as_str()
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| REVIEW_REQUIRED.into())
}

fn verify_readback(before: &[Hook], after: &[Hook]) -> Result<(), String> {
    if before.len() != after.len()
        || before.iter().zip(after).any(|(before, after)| {
            before.key != after.key
                || before.current_hash != after.current_hash
                || after.trust_status != "trusted"
        })
    {
        return Err(REVIEW_REQUIRED.into());
    }
    Ok(())
}

async fn rpc(
    binary: &Path,
    home: &Path,
    requests: &[(u64, &str, Value)],
) -> Result<std::collections::HashMap<u64, Value>, String> {
    let mut messages = vec![
        json!({"method":"initialize","id":0,"params":{
            "clientInfo":{"name":"tuicommander","version":env!("CARGO_PKG_VERSION")},
            "capabilities":{"experimentalApi":true}}}),
        json!({"method":"initialized","params":{}}),
    ];
    messages.extend(
        requests
            .iter()
            .map(|(id, method, params)| json!({"id":id,"method":method,"params":params})),
    );
    crate::cli_usage_rpc::request_jsonl_with_env(
        binary,
        &["app-server"],
        &messages,
        &requests.iter().map(|(id, _, _)| *id).collect::<Vec<_>>(),
        Duration::from_secs(10),
        &[("CODEX_HOME".into(), home.to_string_lossy().into_owned())],
    )
    .await
    .map_err(|_| REVIEW_REQUIRED.to_string())
}

fn result<T: serde::de::DeserializeOwned>(
    responses: &std::collections::HashMap<u64, Value>,
    id: u64,
) -> Result<T, String> {
    crate::cli_usage_rpc::decode_result(responses, id, "Codex hooks")
        .map_err(|_| REVIEW_REQUIRED.to_string())
}

/// `trust` is true only for an explicit install/update, never for status reads.
/// Three bounded RPC processes cost at most 30s of response waiting. Config CAS
/// protects unrelated edits; fresh native readback rejects a concurrent hook edit.
pub(crate) async fn verify(settings: &Path, trust: bool) -> Result<(), String> {
    let detection = crate::agent::detect_agent_binary("codex".into()).await;
    let binary = detection.path.ok_or(REVIEW_REQUIRED)?;
    verify_with_binary(Path::new(&binary), settings, trust).await
}

async fn verify_with_binary(binary: &Path, settings: &Path, trust: bool) -> Result<(), String> {
    let home = settings.parent().ok_or(REVIEW_REQUIRED)?;
    transact(
        &Native { binary, home },
        settings,
        &crate::agent_hook::codex_hook_map(),
        trust,
    )
    .await
}

type Responses = std::collections::HashMap<u64, Value>;

/// One `codex app-server` process per call; a seam so tests can script the server.
trait Codex {
    fn call(
        &self,
        requests: &[(u64, &str, Value)],
    ) -> impl Future<Output = Result<Responses, String>> + Send;
}

struct Native<'a> {
    binary: &'a Path,
    home: &'a Path,
}

impl Codex for Native<'_> {
    fn call(
        &self,
        requests: &[(u64, &str, Value)],
    ) -> impl Future<Output = Result<Responses, String>> + Send {
        rpc(self.binary, self.home, requests)
    }
}

/// Prior trust for each edited key, read from the pre-write config: restoring
/// replaces a known hash and removes a key that had none (`null`).
fn restore_edits(edits: &[Value], config: &Value) -> Vec<Value> {
    edits
        .iter()
        .map(|edit| {
            let key = edit["keyPath"].as_str().unwrap_or_default();
            let prior = key
                .strip_prefix("hooks.state.")
                .and_then(|rest| rest.strip_suffix(".trusted_hash"))
                .and_then(|quoted| serde_json::from_str::<String>(quoted).ok())
                .and_then(|name| {
                    config
                        .pointer("/config/hooks/state")?
                        .get(name)?
                        .get("trusted_hash")
                        .cloned()
                })
                .unwrap_or(Value::Null);
            json!({"keyPath": key, "value": prior, "mergeStrategy": "replace"})
        })
        .collect()
}

async fn transact(
    codex: &impl Codex,
    settings: &Path,
    map: &[HookEntry],
    trust: bool,
) -> Result<(), String> {
    let home = settings.parent().ok_or(REVIEW_REQUIRED)?;
    let config_path = home.join("config.toml");
    let params = json!({"cwds":[home]});
    let responses = codex
        .call(&[
            (1, "hooks/list", params.clone()),
            (2, "config/read", json!({"includeLayers":true})),
        ])
        .await?;
    let hooks = owned_hooks(result(&responses, 1)?, settings, map)?;
    let config: Value = result(&responses, 2)?;
    let version = config_version(&config, &config_path)?;
    if !trust {
        return verify_readback(&hooks, &hooks);
    }
    let edits = trust_edits(&hooks);
    let mut written_version = None;
    if !edits.is_empty() {
        let responses = codex
            .call(&[(
                1,
                "config/batchWrite",
                json!({"filePath": config_path, "expectedVersion":version, "edits":edits}),
            )])
            .await?;
        let written: Value = result(&responses, 1)?;
        if written["status"] != "ok"
            || written["filePath"].as_str().map(Path::new) != Some(config_path.as_path())
        {
            return Err(REVIEW_REQUIRED.into());
        }
        written_version = written["version"].as_str().map(str::to_owned);
    }
    let verified = async {
        let responses = codex.call(&[(1, "hooks/list", params)]).await?;
        let readback = owned_hooks(result(&responses, 1)?, settings, map)?;
        verify_readback(&hooks, &readback)
    }
    .await;
    if verified.is_err() && !edits.is_empty() {
        // Best effort, guarded by the write's own version: a concurrent config
        // edit makes the native CAS reject the restore instead of being overwritten.
        let _ = codex
            .call(&[(
                1,
                "config/batchWrite",
                json!({"filePath": config_path, "expectedVersion":written_version,
                "edits":restore_edits(&edits, &config)}),
            )])
            .await;
    }
    verified
}

#[cfg(test)]
mod tests {
    use super::*;

    fn captured(id: u64) -> Value {
        include_str!("fixtures/codex_hook_trust/codex-0.166.0.jsonl")
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .find_map(|row| {
                let response = row.get("received")?;
                (response["id"] == id).then(|| response.clone())
            })
            .map(|mut response| {
                // The capture predates the label (#2622-6809): stamp it on the
                // TUIC-owned entry, as the installer now writes it.
                if let Some(hooks) = response.pointer_mut("/result/data/0/hooks") {
                    for hook in hooks.as_array_mut().into_iter().flatten() {
                        if hook["command"]
                            .as_str()
                            .is_some_and(|c| c.contains("tuic-managed-hook"))
                        {
                            hook["statusMessage"] = json!("TUICommander session");
                        }
                    }
                }
                response
            })
            .unwrap()
    }

    fn fixture_path() -> &'static Path {
        Path::new("/Users/stefano.straus/Gits/.tmp/codex-hook-trust-2625/home/hooks.json")
    }

    fn fixture_map() -> Vec<HookEntry> {
        vec![("Stop", "", "printf tuic-probe # tuic-managed-hook".into())]
    }

    fn selected(id: u64) -> Vec<Hook> {
        owned_hooks(
            serde_json::from_value(captured(id)["result"].clone()).unwrap(),
            fixture_path(),
            &fixture_map(),
        )
        .unwrap()
    }

    #[test]
    fn codex_native_hash_is_trusted_only_after_matching_readback() {
        // Bug caught: a successful write alone is mistaken for usable hooks.
        let untrusted = selected(1);
        assert!(verify_readback(&untrusted, &untrusted).is_err());
        let trusted = selected(4);
        verify_readback(&untrusted, &trusted).unwrap();
        let modified: Listing = serde_json::from_value(captured(5)["result"].clone()).unwrap();
        let changed_map = vec![(
            "Stop",
            "",
            "printf changed-probe # tuic-managed-hook".into(),
        )];
        let changed = owned_hooks(modified, fixture_path(), &changed_map).unwrap();
        assert!(verify_readback(&trusted, &changed).is_err());
        assert!(verify_readback(&changed, &changed).is_err());
    }

    #[test]
    fn codex_trust_edit_cannot_grant_unrelated_hook_trust() {
        // Bug caught: a blanket trust write includes user hooks or replaces state.
        let edits = trust_edits(&selected(1));
        assert_eq!(edits.len(), 1);
        let request = include_str!("fixtures/codex_hook_trust/codex-0.166.0.jsonl")
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|row| row.pointer("/sent/id") == Some(&json!(3)))
            .unwrap();
        assert_eq!(
            edits,
            *request
                .pointer("/sent/params/edits")
                .unwrap()
                .as_array()
                .unwrap()
        );
        assert_eq!(trust_edits(&selected(4)), [] as [serde_json::Value; 0]);
    }

    #[test]
    fn codex_modified_or_misowned_definition_is_never_auto_trusted() {
        // Bug caught: the sentinel alone authorizes altered or project hooks.
        for (field, value) in [
            ("command", json!("printf different # tuic-managed-hook")),
            ("source", json!("project")),
            ("sourcePath", json!("/other/hooks.json")),
            ("matcher", json!("different")),
            ("eventName", json!("preToolUse")),
            ("async", json!(true)),
            ("enabled", json!(false)),
            ("isManaged", json!(true)),
            ("timeoutSec", json!(1)),
            ("statusMessage", json!("altered")),
            ("additionalContextLimit", json!(1)),
            ("trustStatus", json!("future-policy")),
            ("key", json!("")),
            ("currentHash", json!("")),
        ] {
            let mut listing = captured(1)["result"].clone();
            listing["data"][0]["hooks"][0][field] = value;
            assert!(
                owned_hooks(
                    serde_json::from_value(listing).unwrap(),
                    fixture_path(),
                    &fixture_map()
                )
                .is_err(),
                "{field}"
            );
        }
    }

    #[test]
    fn codex_wrong_profile_or_disabled_hooks_cannot_report_usable() {
        // Bug caught: default-profile trust is applied to another profile or disabled hooks.
        let config = captured(2)["result"].clone();
        let path = fixture_path().with_file_name("config.toml");
        assert!(config_version(&config, &path).is_ok());
        for change in ["profile", "file", "disabled"] {
            let mut config = config.clone();
            match change {
                "profile" => config["layers"][0]["name"]["profile"] = json!("work"),
                "file" => config["layers"][0]["name"]["file"] = json!("/other/config.toml"),
                _ => config["config"]["features"]["hooks"] = json!(false),
            }
            assert!(config_version(&config, &path).is_err(), "{change}");
        }
    }

    #[test]
    fn codex_equivalent_config_paths_preserve_native_version() {
        // Bug caught: string equality rejects the same file after host path joining.
        let config = captured(2)["result"].clone();
        let home = fixture_path().parent().unwrap();
        for path in [
            home.join("config.toml"),
            home.join(".").join("config.toml"),
            std::path::PathBuf::from(format!("{}//config.toml", home.display())),
            #[cfg(windows)]
            std::path::PathBuf::from(
                "/Users/stefano.straus/Gits/.tmp/codex-hook-trust-2625/home/config.toml"
                    .replace('/', "\\"),
            ),
        ] {
            assert_eq!(
                config_version(&config, &path).unwrap(),
                "sha256:28fcee910d705969e049580a0bded0221705dae0ec6d34ad19e016001f05497b",
                "{path:?}"
            );
        }
        assert!(config_version(&config, &home.join("other/config.toml")).is_err());
    }

    #[test]
    fn codex_failed_native_write_cannot_become_success() {
        // Bug caught: a failed config CAS is accepted as successful trust persistence.
        let responses = std::collections::HashMap::from([(6, captured(6))]);
        assert!(result::<Value>(&responses, 6).is_err());
        assert!(verify_readback(&selected(4), &[]).is_err());
    }

    #[tokio::test]
    async fn codex_missing_cli_requires_review_without_rewriting_files() {
        // Bug caught: discovery failure reports installed or changes config.
        let dir = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
        let hooks = dir.path().join("hooks.json");
        crate::agent_hook_commands::apply_at("codex", &hooks, true).unwrap();
        let config = dir.path().join("config.toml");
        let before = std::fs::read(&config).unwrap();
        assert!(
            verify_with_binary(&dir.path().join("missing-codex"), &hooks, true)
                .await
                .is_err()
        );
        assert_eq!(std::fs::read(&config).unwrap(), before);
    }

    #[test]
    fn codex_status_waits_for_concurrent_install_transaction() {
        // Bug caught: status reads an intermediate install and races trust persistence.
        let dir = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
        crate::agent_test::with_test_profile(dir.path(), || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let guard = INSTALL_LOCK.lock().await;
                    let mut status = Box::pin(crate::agent_hook_commands::get_agent_hook_state(
                        "codex".into(),
                    ));
                    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
                    assert!(std::future::Future::poll(status.as_mut(), &mut context).is_pending());
                    drop(guard);
                    assert_eq!(status.await, "notInstalled");
                });
        });
    }

    #[test]
    fn codex_changed_hook_cannot_use_stale_trust() {
        // Bug caught: canonical files report usable despite stale native trust.
        let dir = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
        let hooks = dir.path().join("hooks.json");
        crate::agent_hook_commands::apply_at("codex", &hooks, true).unwrap();
        std::fs::write(
            dir.path().join("config.toml"),
            "[features]\nhooks = true\n[hooks.state.unrelated]\ntrusted_hash = 'sha256:old'\n",
        )
        .unwrap();
        assert_ne!(
            crate::agent_hook_commands::state_at("codex", &hooks),
            crate::agent_hook_installer::InstallState::Installed,
            "file presence cannot establish current native hook trust"
        );
    }

    /// In-process stand-in for `codex app-server`: a config with per-key trust
    /// and a native version that every accepted write replaces.
    struct FakeCodex {
        state: std::sync::Mutex<(u32, std::collections::BTreeMap<String, String>)>,
        fail_readback: bool,
    }

    impl FakeCodex {
        fn new(fail_readback: bool) -> Self {
            let prior = [("unrelated".to_string(), "sha256:keep".to_string())];
            Self {
                state: std::sync::Mutex::new((0, prior.into())),
                fail_readback,
            }
        }

        fn snapshot(&self) -> std::collections::BTreeMap<String, String> {
            self.state.lock().unwrap().1.clone()
        }

        fn handle(&self, id: u64, method: &str, params: &Value) -> Result<Responses, String> {
            let mut state = self.state.lock().unwrap();
            let key = selected(1)[0].key.clone();
            let version = |n: u32| format!("sha256:v{n}");
            let reply = match method {
                "hooks/list" => {
                    if state.1.contains_key(&key) && self.fail_readback {
                        return Err(REVIEW_REQUIRED.into());
                    }
                    captured(if state.1.contains_key(&key) { 4 } else { 1 })
                }
                "config/read" => {
                    let mut config = captured(2);
                    config["result"]["layers"][0]["version"] = json!(version(state.0));
                    let trust: serde_json::Map<String, Value> = state
                        .1
                        .iter()
                        .map(|(k, v)| (k.clone(), json!({"trusted_hash": v})))
                        .collect();
                    config["result"]["config"]["hooks"] = json!({"state": trust});
                    config
                }
                _ => {
                    if params["expectedVersion"] != json!(version(state.0)) {
                        return Ok(Responses::from([(id, captured(6))]));
                    }
                    for edit in params["edits"].as_array().unwrap() {
                        let quoted = edit["keyPath"]
                            .as_str()
                            .unwrap()
                            .strip_prefix("hooks.state.")
                            .unwrap()
                            .strip_suffix(".trusted_hash")
                            .unwrap();
                        let name = serde_json::from_str::<String>(quoted).unwrap();
                        match edit["value"].as_str() {
                            Some(hash) => state.1.insert(name, hash.to_string()),
                            None => state.1.remove(&name),
                        };
                    }
                    state.0 += 1;
                    json!({"id": id, "result": {"status": "ok", "version": version(state.0),
                        "filePath": params["filePath"]}})
                }
            };
            Ok(Responses::from([(id, reply)]))
        }
    }

    impl Codex for FakeCodex {
        // The trait returns a future; the scripted fake has nothing to await.
        #[allow(clippy::unused_async_trait_impl)]
        async fn call(&self, requests: &[(u64, &str, Value)]) -> Result<Responses, String> {
            let mut all = Responses::new();
            for (id, method, params) in requests {
                all.extend(self.handle(*id, method, params)?);
            }
            Ok(all)
        }
    }

    async fn run_transaction(fake: &FakeCodex) -> Result<(), String> {
        transact(fake, fixture_path(), &fixture_map(), true).await
    }

    #[tokio::test]
    async fn codex_trust_readback_failure_leaves_file_unchanged() {
        // Bug caught: trusted_hash stays persisted after batchWrite succeeds and readback fails.
        let fake = FakeCodex::new(true);
        let before = fake.snapshot();
        assert!(run_transaction(&fake).await.is_err());
        assert_eq!(fake.snapshot(), before);
    }

    #[tokio::test]
    async fn codex_trust_success_keeps_the_persisted_hash() {
        // Bug caught: restoration runs on success and discards the requested trust.
        let fake = FakeCodex::new(false);
        run_transaction(&fake).await.unwrap();
        assert!(fake.snapshot().contains_key(&selected(1)[0].key));
    }

    #[test]
    fn codex_trust_restore_puts_back_a_prior_hash() {
        // Bug caught: restoring a modified hook deletes its previous trust record.
        let hook = selected(1);
        let edits = trust_edits(&hook);
        let mut config = captured(2)["result"].clone();
        config["config"]["hooks"] =
            json!({"state": {hook[0].key.clone(): {"trusted_hash": "sha256:old"}}});
        assert_eq!(
            restore_edits(&edits, &json!(config))[0]["value"],
            "sha256:old"
        );
    }

    #[test]
    fn codex_native_listing_without_our_label_is_not_ours() {
        // Bug caught: label-less (pre-2622) entries stay "owned" and never get re-trusted
        // after the reinstall changes their hash.
        let mut listing = captured(1)["result"].clone();
        listing["data"][0]["hooks"][0]["statusMessage"] = Value::Null;
        assert!(
            owned_hooks(
                serde_json::from_value(listing).unwrap(),
                fixture_path(),
                &fixture_map()
            )
            .is_err()
        );
    }
}
