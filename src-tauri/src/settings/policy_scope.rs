//! Boss-approved host policy: confirm only an agent-written change that adds authority.
//! Catalogue annotations cannot expand confirmation authority.
use super::super::model::Field;
use serde_json::Value;

const SERVICE_PREFIXES: [&str; 4] = [
    "/services/server/",
    "/services/auth/",
    "/services/tls/",
    "/services/relay/",
];

pub(super) fn adds_authority(fields: &[&Field], latest: &Value, next: &Value) -> bool {
    fields
        .iter()
        .any(|field| match (field.store.as_str(), field.path.as_str()) {
            ("app", "/ego_executable") => new_text(latest, next, "/ego_executable"),
            ("app", "/ai_chat_launches/{key}/executable") => chat_launch_added(latest, next),
            ("app", "/plugin_http_destinations") => destination_added(latest, next),
            // An added push provider becomes an outbound destination (#2668-aa16).
            ("app", "/services/push/added_providers") => push_provider_added(latest, next),
            (
                "app",
                "/custom_launchers/{id}/executable"
                | "/custom_launchers/{id}/args"
                | "/custom_launchers/{id}/enabled",
            ) => launcher_added(latest, next),
            ("app", path) if SERVICE_PREFIXES.iter().any(|p| path.starts_with(p)) => {
                service_widened(latest, next)
            }
            (
                "tunnels",
                "/profiles/{id}/port" | "/profiles/{id}/auto_connect" | "/profiles/{id}/forwards",
            ) => tunnel_widened(latest, next),
            (
                "upstreams",
                "/servers/{id}/enabled"
                | "/servers/{id}/transport/type"
                | "/servers/{id}/transport/command"
                | "/servers/{id}/transport/args"
                | "/servers/{id}/transport/cwd"
                | "/servers/{id}/transport/env/{key}",
            ) => stdio_added(latest, next),
            _ => false,
        })
}

fn text<'a>(value: &'a Value, path: &str) -> &'a str {
    value.pointer(path).and_then(Value::as_str).unwrap_or("")
}
fn number(value: &Value, path: &str) -> Option<u64> {
    value.pointer(path).and_then(Value::as_u64)
}
fn flag(value: &Value, path: &str) -> bool {
    value.pointer(path).and_then(Value::as_bool) == Some(true)
}
/// A different non-empty value; clearing never adds authority.
fn new_text(latest: &Value, next: &Value, path: &str) -> bool {
    !text(next, path).is_empty() && text(next, path) != text(latest, path)
}
fn turned_on(latest: &Value, next: &Value, path: &str) -> bool {
    flag(next, path) && !flag(latest, path)
}
fn rows<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value[key].as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn row_by_id<'a>(rows: &'a [Value], id: &Value) -> Option<&'a Value> {
    rows.iter().find(|row| row["id"] == *id)
}

fn service_widened(latest: &Value, next: &Value) -> bool {
    let changed = |path: &str| latest.pointer(path) != next.pointer(path);
    // Zero removes the bound: no rate limit (auth.rs admit_auth_attempt), no cookie Max-Age.
    let grew = |path: &str| {
        number(next, path) > number(latest, path)
            || (number(next, path) == Some(0) && number(latest, path) != Some(0))
    };
    let shrank =
        |path: &str| number(next, path).is_some() && number(next, path) < number(latest, path);
    turned_on(latest, next, "/services/server/enabled")
        || turned_on(latest, next, "/services/server/ipv6_enabled")
        || changed("/services/server/port")
        || turned_on(latest, next, "/services/auth/lan_auth_bypass")
        || grew("/services/auth/session_token_duration_secs")
        || grew("/services/auth/auth_rate_limit_max")
        || shrank("/services/auth/auth_rate_limit_window_secs")
        || new_text(latest, next, "/services/auth/username")
        || new_text(latest, next, "/services/auth/password_hash")
        || new_text(latest, next, "/services/auth/session_token")
        || (text(latest, "/services/tls/mode") == "manual"
            && text(next, "/services/tls/mode") != "manual")
        || new_text(latest, next, "/services/tls/cert_path")
        || new_text(latest, next, "/services/tls/key_path")
        || turned_on(latest, next, "/services/relay/enabled")
        || new_text(latest, next, "/services/relay/url")
        || new_text(latest, next, "/services/relay/token")
}

fn chat_launch_added(latest: &Value, next: &Value) -> bool {
    next["ai_chat_launches"]
        .as_object()
        .into_iter()
        .flatten()
        .any(|(key, launch)| {
            launch["executable"].as_str().is_some_and(|e| !e.is_empty())
                && launch["executable"] != latest["ai_chat_launches"][key]["executable"]
        })
}

/// A new pattern lets plugins reach another host (LAN included); removing one never widens.
fn destination_added(latest: &Value, next: &Value) -> bool {
    let before = rows(latest, "plugin_http_destinations");
    rows(next, "plugin_http_destinations")
        .iter()
        .any(|pattern| !before.contains(pattern))
}

/// A new push service host receives notification payloads; removing one never widens.
fn push_provider_added(latest: &Value, next: &Value) -> bool {
    let hosts = |value: &Value| {
        value
            .pointer("/services/push/added_providers")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let before = hosts(latest);
    hosts(next).iter().any(|host| !before.contains(host))
}

/// Args and executable are redacted or opaque in the preview, so any replacement counts.
fn launcher_added(latest: &Value, next: &Value) -> bool {
    let before = rows(latest, "custom_launchers");
    rows(next, "custom_launchers").iter().any(|row| {
        let prev = row_by_id(before, &row["id"]);
        let differs = |key: &str| prev.is_none_or(|p| p[key] != row[key]);
        (differs("executable") && row["executable"].as_str().is_some_and(|e| !e.is_empty()))
            || differs("args") && !row["args"].as_array().is_none_or(Vec::is_empty)
            || (row["enabled"] == true && prev.is_none_or(|p| p["enabled"] != true))
    })
}

fn tunnel_widened(latest: &Value, next: &Value) -> bool {
    let before = rows(latest, "profiles");
    rows(next, "profiles").iter().any(|profile| {
        let Some(prev) = row_by_id(before, &profile["id"]) else {
            return true;
        };
        let old_forwards = prev["forwards"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        profile["port"] != prev["port"]
            || (profile["auto_connect"] == true && prev["auto_connect"] != true)
            || rows(profile, "forwards")
                .iter()
                .any(|forward| !old_forwards.contains(forward))
    })
}

/// Also the raw `PUT /mcp/upstreams` check: the caller's `base` snapshot is `latest`, so the
/// saved delta is exactly their difference and the comparison is race-free.
pub(crate) fn stdio_added(latest: &Value, next: &Value) -> bool {
    let before = rows(latest, "servers");
    rows(next, "servers").iter().any(|record| {
        if record.pointer("/transport/type").and_then(Value::as_str) != Some("stdio") {
            return false;
        }
        let Some(prev) = row_by_id(before, &record["id"]) else {
            return true;
        };
        (record["enabled"] == true && prev["enabled"] != true)
            || [
                "/transport/type",
                "/transport/command",
                "/transport/args",
                "/transport/cwd",
                "/transport/env",
            ]
            .iter()
            .any(|path| prev.pointer(path) != record.pointer(path))
    })
}

/// Whether a raw app-config write adds authority the facade gates on owner confirmation.
/// Same predicates as `adds_authority`, applied to whole config documents so a writer
/// that bypasses the facade (`PUT /config`) cannot skip them.
pub(crate) fn app_config_requires_confirmation(latest: &Value, next: &Value) -> bool {
    new_text(latest, next, "/ego_executable")
        || chat_launch_added(latest, next)
        || destination_added(latest, next)
        || launcher_added(latest, next)
        || service_widened(latest, next)
}

/// Whether a raw tunnel-profile write adds authority the facade gates on owner
/// confirmation. `stored` is the profile on disk, or a default profile for a new one.
pub(crate) fn tunnel_profile_requires_confirmation(stored: &Value, next: &Value) -> bool {
    let mut next = next.clone();
    next["id"] = stored["id"].clone();
    tunnel_widened(
        &serde_json::json!({ "profiles": [stored] }),
        &serde_json::json!({ "profiles": [next] }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Catches: a removed or reduced tunnel/upstream prompting, or an added forward/stdio command passing.
    #[test]
    fn tunnel_and_stdio_prompt_only_when_authority_grows() {
        let forward = json!({"type":"Local","bind_port":1,"remote_host":"h","remote_port":2});
        let other = json!({"type":"Local","bind_port":3,"remote_host":"h","remote_port":4});
        let profile = |auto: bool, forwards: Vec<serde_json::Value>| json!({"profiles":[{"id":"p","port":22,"auto_connect":auto,"forwards":forwards}]});
        let before = profile(true, vec![forward.clone(), other.clone()]);
        assert!(!tunnel_widened(
            &before,
            &profile(false, vec![forward.clone()])
        ));
        assert!(!tunnel_widened(&before, &before));
        assert!(tunnel_widened(
            &profile(false, vec![]),
            &profile(true, vec![])
        ));
        assert!(tunnel_widened(
            &profile(false, vec![forward.clone()]),
            &profile(false, vec![forward, other])
        ));
        let stdio = |command: &str, enabled: bool| json!({"servers":[{"id":"u","enabled":enabled,"transport":{"type":"stdio","command":command,"args":[],"cwd":null,"env":{}}}]});
        assert!(!stdio_added(&stdio("run", true), &stdio("run", false)));
        assert!(!stdio_added(&stdio("run", true), &json!({"servers":[]})));
        assert!(stdio_added(&stdio("run", false), &stdio("run", true)));
        assert!(stdio_added(&stdio("run", true), &stdio("other", true)));
        assert!(stdio_added(&json!({"servers":[]}), &stdio("run", false)));
    }

    // Catches: removing a push provider prompting under the add-only policy, or an added one passing.
    #[test]
    fn push_providers_prompt_only_when_a_host_is_added() {
        let list = |hosts: &[&str]| json!({"services": {"push": {"added_providers": hosts}}});
        let both = list(&["push.example.net", "push.example.org"]);
        assert!(!push_provider_added(&both, &list(&["push.example.net"])));
        assert!(!push_provider_added(&both, &list(&[])));
        assert!(push_provider_added(&list(&["push.example.net"]), &both));
        assert!(push_provider_added(
            &json!({}),
            &list(&["push.example.net"])
        ));
    }

    // Catches: removing or reordering a plugin HTTP destination prompting, or an added one passing.
    #[test]
    fn plugin_http_destinations_prompt_only_when_a_pattern_is_added() {
        let list = |patterns: &[&str]| json!({"plugin_http_destinations": patterns});
        let both = list(&["http://192.168.1.20:3000/*", "https://api.example/*"]);
        assert!(!destination_added(&both, &list(&["https://api.example/*"])));
        assert!(!destination_added(&both, &list(&[])));
        assert!(!destination_added(
            &both,
            &list(&["https://api.example/*", "http://192.168.1.20:3000/*"])
        ));
        assert!(destination_added(&list(&["https://api.example/*"]), &both));
        assert!(destination_added(&json!({}), &list(&["http://10.0.0.2/*"])));
    }
}
