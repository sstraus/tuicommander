//! Claude Code status line relay (plan step 6, spike S5).
//!
//! TUIC's launch settings replace the user's `statusLine` with a relay
//! script. The script posts the status line stdin here, receives the user's own
//! command and runs it with the same stdin, so the user's status line does not
//! change. The stdin carries `rate_limits` for the session account, which the
//! `claude-code` owner returns from `observed()` without a request.
//! Spike evidence: `docs/backend/quota-source-spikes.md` (S5).
use super::{QuotaSnapshot, QuotaSource, QuotaWindow, SubscriptionKey};
use serde_json::Value;
use std::path::Path;

/// The relay route below `/sessions/{id}/`, served on the local socket only.
pub(crate) const ROUTE: &str = "claude-status-line";

/// What one relayed status line update produced.
#[derive(Debug, Default)]
pub(crate) struct Relayed {
    /// The credential whose windows changed; `None` when nothing new was stored.
    pub changed: Option<SubscriptionKey>,
    /// The user's own status line command, if any source defines one.
    pub user_command: Option<String>,
}

/// Handle one update: `body` is the stdin JSON, `config_dir` and `home` are
/// `CLAUDE_CONFIG_DIR` and `HOME` of the agent process.
pub(crate) fn relay(body: &str, config_dir: &str, home: &str, now_ms: u64) -> Relayed {
    let env = [("CLAUDE_CONFIG_DIR", config_dir), ("HOME", home)];
    let Some(key) = super::binding::resolve("claude", &env, None, None) else {
        return Relayed::default();
    };
    let stdin: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let project = ["/workspace/project_dir", "/workspace/current_dir", "/cwd"]
        .iter()
        .find_map(|pointer| stdin.pointer(pointer)?.as_str())
        .filter(|dir| Path::new(dir).is_absolute());
    let user_command = user_command(Path::new(&key.credential), project.map(Path::new));
    let changed = parse_rate_limits(&stdin, now_ms)
        .filter(|snapshot| super::adapters::record_claude_status_line(&key, snapshot.clone()))
        .map(|_| key);
    Relayed {
        changed,
        user_command,
    }
}

/// `five_hour` and `seven_day` from the stdin JSON. The payload carries no
/// time of its own, so the caller passes when it arrived.
pub(crate) fn parse_rate_limits(stdin: &Value, observed_at_ms: u64) -> Option<QuotaSnapshot> {
    let limits = stdin.get("rate_limits")?;
    let windows: Vec<_> = [("five_hour", 18_000), ("seven_day", 604_800)]
        .into_iter()
        .filter_map(|(name, secs)| {
            let window = limits.get(name)?;
            let resets_at = window
                .get("resets_at")
                .and_then(Value::as_i64)
                .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
                .map(|d| d.to_rfc3339());
            QuotaWindow::from_duration(
                window.get("used_percentage")?.as_f64()?,
                Some(secs),
                resets_at,
                None,
            )
        })
        .collect();
    (!windows.is_empty()).then_some(QuotaSnapshot {
        windows,
        detail: None,
        plan: None,
        credits: None,
        account: None,
        observed_at_ms,
        source: QuotaSource::Passive,
    })
}

/// The command Claude would run without the relay: settings merge low to high
/// (user, project, local), field by field, so the highest source that names a
/// command wins.
fn user_command(config_dir: &Path, project: Option<&Path>) -> Option<String> {
    let mut sources = Vec::new();
    if let Some(project) = project {
        sources.push(project.join(".claude/settings.local.json"));
        sources.push(project.join(".claude/settings.json"));
    }
    sources.push(config_dir.join("settings.json"));
    sources.iter().find_map(|path| {
        let settings: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        let command = settings.pointer("/statusLine/command")?.as_str()?;
        (!command.trim().is_empty()).then(|| command.to_owned())
    })
}

/// The POSIX relay. It reads stdin exactly (trailing newlines included), asks
/// TUIC for the user's command over the local socket and runs it under
/// `/bin/sh` with the same stdin, as Claude does. Without an answer it prints
/// nothing.
pub(crate) fn posix_script() -> String {
    format!(
        r#"#!/bin/sh
# {sentinel}
__in=$(cat; printf x); __in=${{__in%x}}
[ -n "${{TUIC_SESSION:-}}" ] && [ -S "${{TUIC_SOCKET:-}}" ] || exit 0
__c=$(printf %s "$__in" | curl -sf -m 2 -H 'Expect:' -H "x-claude-config-dir: ${{CLAUDE_CONFIG_DIR:-}}" -H "x-home: ${{HOME:-}}" --unix-socket "$TUIC_SOCKET" --data-binary @- "http://tuic/sessions/$TUIC_SESSION/{ROUTE}") || exit 0
[ -n "$__c" ] || exit 0
printf %s "$__in" | /bin/sh -c "$__c"
"#,
        sentinel = crate::agent_hook::SENTINEL,
    )
}

#[cfg(test)]
#[path = "status_line_tests.rs"]
mod tests;
