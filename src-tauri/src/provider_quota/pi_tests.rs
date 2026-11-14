use super::*;
use serde_json::json;

const RECORDED: &[u8] = include_bytes!("../fixtures/agent_prompts/pi-provider-response-2994.osc");

/// `pi-event` payloads (`<owner>:<base64>`) in terminal bytes, in order.
fn payloads(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    text.split("\x1b]7770;pi-event=")
        .skip(1)
        .filter_map(|rest| rest.split_once("\x1b\\").map(|(p, _)| p.to_owned()))
        .collect()
}

fn frame(owner: &str, value: serde_json::Value) -> String {
    use base64::Engine;
    format!(
        "{owner}:{}",
        base64::engine::general_purpose::STANDARD.encode(value.to_string())
    )
}

// Catches: a credential forwarded with the quota headers, by the extension
// (deny-list instead of allow-list, case-sensitive filter) or accepted by TUIC
// from a frame that carries one.
#[test]
fn auth_headers_never_cross_the_pi_channel() {
    let dir = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    let source = dir.path().join("tuic.mjs");
    std::fs::write(&source, crate::agent_hook_pi::pi_extension_source()).unwrap();
    let output = std::process::Command::new("node")
        .args([
            "--input-type=module",
            "-e",
            r#"
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
const callbacks = new Map();
const code = readFileSync(process.argv[1], 'utf8')
  .replace(/^import .* from "node:fs";$/m, '')
  .replace('export default function', 'globalThis.start = function');
const frames = [];
const sandbox = {
  process: {env: {TUIC_SESSION: 'pi-test', HOME: '/home/fixture'}}, Buffer,
  openSync: () => 1, closeSync: () => {},
  writeSync: (_fd, data) => frames.push(data),
};
runInNewContext(code, sandbox);
sandbox.start({on: (name, callback) => callbacks.set(name, callback)});
const ctx = {mode: 'print', ui: {}, isIdle: () => true, model: {provider: 'openai-codex'}};
callbacks.get('session_start')({}, ctx);
callbacks.get('after_provider_response')({status: 200, headers: {
  'authorization': 'Bearer fixture-secret',
  'Proxy-Authorization': 'Basic fixture-secret',
  'set-cookie': 'session=fixture-secret',
  'cookie': 'a=fixture-secret',
  'x-api-key': 'fixture-secret',
  'x-codex-turn-state': 'fixture-secret',
  'X-Codex-Primary-Used-Percent': '14',
  'x-codex-primary-window-minutes': '10080',
  'x-codex-plan-type': 'x'.repeat(65),
}}, ctx);
process.stdout.write(frames.join(''));
"#,
        ])
        .arg(&source)
        .output()
        .expect("Node.js is required for generated Pi extension behavior tests");
    assert!(
        output.status.success(),
        "generated extension failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stderr, [] as [u8; 0]);
    let sent = payloads(&output.stdout);
    assert_eq!(sent.len(), 1, "one frame per response");
    let (_, encoded) = sent[0].split_once(':').unwrap();
    use base64::Engine;
    let wire = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .unwrap();
    let wire: serde_json::Value = serde_json::from_slice(&wire).unwrap();
    assert!(
        !wire.to_string().contains("fixture-secret"),
        "credential on the wire: {wire}"
    );
    assert_eq!(
        wire["headers"],
        json!({"x-codex-primary-used-percent": "14", "x-codex-primary-window-minutes": "10080"})
    );
    assert_eq!(wire["kind"], "provider_response");
    assert_eq!(wire["provider"], "openai-codex");

    // TUIC enforces the same list on a frame an older or other extension wrote.
    let foreign = frame(
        "pi-test",
        json!({"kind": "provider_response", "provider": "anthropic", "status": 200,
               "headers": {"authorization": "Bearer fixture-secret",
                           "set-cookie": "fixture-secret",
                           "anthropic-ratelimit-unified-5h-utilization": "0.5"}}),
    );
    let Some(PiEvent::ProviderResponse { headers, .. }) = decode(&foreign, Some("pi-test")) else {
        panic!("frame rejected");
    };
    assert_eq!(
        headers.keys().collect::<Vec<_>>(),
        ["anthropic-ratelimit-unified-5h-utilization"]
    );
}

// Catches: the recorded frame of real pi not reaching the registry, binding
// to another credential, a fraction shown as 0.25 %, or a frame of another
// TUIC session accepted.
#[test]
fn recorded_pi_frame_becomes_a_passive_quota_for_the_pi_credential() {
    let sent = payloads(RECORDED);
    assert_eq!(sent.len(), 1);
    assert_eq!(decode(&sent[0], Some("other-session")), None);
    let event = decode(&sent[0], Some("fixture-owner")).expect("recorded frame decodes");
    let registry = QuotaRegistry::default();
    receive(&registry, "tab-pi", event, 1_000);
    let key = SubscriptionKey::new("pi", "anthropic", "/home/fixture/.pi-scratch/agent");
    assert_eq!(registry.binding("tab-pi"), Some(key.clone()));
    assert_eq!(session_provider("tab-pi").as_deref(), Some("anthropic"));
    let snapshot = registry.get(&key).snapshot.expect("observed");
    assert_eq!(snapshot.source, QuotaSource::Passive);
    let windows: Vec<_> = snapshot
        .windows
        .iter()
        .map(|w| (w.kind, w.used_percent))
        .collect();
    assert_eq!(
        windows,
        [(WindowKind::FiveHour, 25.0), (WindowKind::Weekly, 82.0)]
    );
    assert_eq!(
        snapshot.windows[1].resets_at.as_deref(),
        Some("2026-11-23T00:08:23+00:00")
    );
    assert_eq!(Pi.observed(&key), Some(snapshot));
    retain_sessions(&Default::default());
    assert_eq!(session_provider("tab-pi"), None);
}

// Catches: a zero secondary window shown as a 0 % row, or window minutes
// read as seconds. Values from the recorded Codex response (ego S1 fixture).
#[test]
fn recorded_codex_headers_map_to_windows() {
    let headers: BTreeMap<String, String> = [
        ("x-codex-plan-type", "pro"),
        ("x-codex-primary-used-percent", "14"),
        ("x-codex-primary-window-minutes", "10080"),
        ("x-codex-primary-reset-at", "1794992503"),
        ("x-codex-secondary-used-percent", "0"),
        ("x-codex-secondary-window-minutes", "0"),
        ("x-codex-secondary-reset-at", ""),
        ("x-codex-credits-balance", "8418.4673090000"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect();
    let snapshot = snapshot_from_headers(&headers, 1).unwrap();
    assert_eq!(snapshot.windows.len(), 1);
    assert_eq!(snapshot.windows[0].kind, WindowKind::Weekly);
    assert_eq!(snapshot.windows[0].used_percent, 14.0);
    assert_eq!(snapshot.plan.as_deref(), Some("pro"));
    assert_eq!(snapshot_from_headers(&BTreeMap::new(), 1), None);
}

// Catches: an active read that uses another provider's token, sends an
// expired token, or calls a subscription endpoint for an API key login.
#[test]
fn pi_endpoint_request_uses_only_the_provider_subscription_login() {
    let auth = json!({
        "anthropic": {"type": "oauth", "access": "a-token", "expires": 2_000},
        "openai-codex": {"type": "oauth", "access": "c-token", "expires": 2_000, "accountId": "acct"},
        "github-copilot": {"type": "oauth", "access": "g-token", "expires": 2_000},
        "openai": {"type": "api_key", "key": "k"},
    });
    let anthropic = endpoint_request("anthropic", &auth["anthropic"], 1_000).unwrap();
    assert_eq!(anthropic.url, "https://api.anthropic.com/api/oauth/usage");
    assert_eq!(anthropic.token, "a-token");
    let codex = endpoint_request("openai-codex", &auth["openai-codex"], 1_000).unwrap();
    assert_eq!(codex.url, "https://chatgpt.com/backend-api/wham/usage");
    assert_eq!(codex.token, "c-token");
    assert_eq!(codex.headers, [("ChatGPT-Account-Id", "acct".to_owned())]);
    assert!(matches!(
        endpoint_request("anthropic", &auth["anthropic"], 2_000),
        Err(QuotaError::Auth(_))
    ));
    for (provider, entry) in [
        ("github-copilot", &auth["github-copilot"]),
        ("openai", &auth["openai"]),
        ("xai", &auth["xai"]),
    ] {
        assert!(
            matches!(
                endpoint_request(provider, entry, 1_000),
                Err(QuotaError::Unavailable(_))
            ),
            "{provider}"
        );
    }
}
