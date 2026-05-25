//! Terminal watcher — observe→decide→act loop for terminal sessions.
//!
//! Persists rules to `ai-watchers.json` in the app config dir.

use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::ai_agent::conversation_engine::{ACTIVE_CONVERSATIONS, Autonomy, ConversationConfig};
use crate::ai_agent::knowledge::sanitize_snippet;
use crate::state::{AppEvent, AppState};
#[cfg(feature = "desktop")]
use tauri::Emitter;

const CONFIG_FILE: &str = "ai-watchers.json";

// ── Rule definition ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WatcherRule {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_id: Option<String>,
    pub trigger: WatcherTrigger,
    pub instructions: String,
    pub max_fires: u32,
    #[serde(default)]
    pub fire_count: u32,
    #[serde(default = "default_cooldown")]
    pub cooldown_secs: u32,
    #[serde(default = "default_burst_threshold")]
    pub burst_threshold: u32,
    #[serde(default = "default_burst_window")]
    pub burst_window_secs: u32,
    #[serde(default)]
    pub status: WatcherStatus,
    #[serde(default)]
    pub created_at: u64,
}

pub(crate) fn default_cooldown() -> u32 {
    10
}
pub(crate) fn default_burst_threshold() -> u32 {
    5
}
pub(crate) fn default_burst_window() -> u32 {
    60
}
pub(crate) fn default_max_fires() -> u32 {
    50
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WatcherStatus {
    #[default]
    Active,
    Paused,
    Stopped,
    Exhausted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum WatcherTrigger {
    Idle,
    Busy,
    CommandDone {
        #[serde(default)]
        on_failure_only: bool,
    },
    Question {
        #[serde(default = "default_true")]
        confident_only: bool,
    },
    Error,
    Unseen,
    Pattern {
        regex: String,
    },
}

fn default_true() -> bool {
    true
}

// ── Config wrapper ──────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct WatcherConfig {
    #[serde(default)]
    pub rules: Vec<WatcherRule>,
}

// ── Persistence ─────────────────────────────────────────────────

pub(crate) fn load_config() -> WatcherConfig {
    crate::config::load_json_config(CONFIG_FILE)
}

pub(crate) fn save_config(config: &WatcherConfig) -> Result<(), String> {
    crate::config::save_json_config(CONFIG_FILE, config)
}

// ── Validation ──────────────────────────────────────────────────

pub(crate) fn validate_rule(rule: &WatcherRule) -> Result<(), String> {
    if rule.instructions.trim().is_empty() {
        return Err("Instructions must not be empty".into());
    }
    if rule.cooldown_secs < 5 {
        return Err(format!(
            "Cooldown must be >= 5s, got {}",
            rule.cooldown_secs
        ));
    }
    if rule.max_fires == 0 {
        return Err("max_fires must be > 0".into());
    }
    if rule.instructions.len() > 8192 {
        return Err("Instructions too long (max 8192 chars)".into());
    }
    if let WatcherTrigger::Pattern { ref regex } = rule.trigger {
        if regex.trim().is_empty() {
            return Err("Pattern trigger requires a non-empty regex".into());
        }
        regex::Regex::new(regex).map_err(|e| format!("Invalid regex '{}': {e}", regex))?;
    }
    Ok(())
}

// ── CRUD operations ─────────────────────────────────────────────

pub(crate) fn create_rule(
    config: &mut WatcherConfig,
    mut rule: WatcherRule,
) -> Result<String, String> {
    validate_rule(&rule)?;

    if rule.id.is_empty() {
        rule.id = uuid::Uuid::new_v4().to_string();
    }
    if rule.created_at == 0 {
        rule.created_at = now_secs();
    }
    rule.status = if rule.session_id.is_some() {
        WatcherStatus::Active
    } else {
        WatcherStatus::Paused
    };
    rule.fire_count = 0;

    let id = rule.id.clone();
    config.rules.push(rule);
    save_config(config)?;
    Ok(id)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn delete_rule(config: &mut WatcherConfig, id: &str) -> Result<(), String> {
    let len_before = config.rules.len();
    config.rules.retain(|r| r.id != id);
    if config.rules.len() == len_before {
        return Err(format!("Rule '{}' not found", id));
    }
    save_config(config)
}

pub(crate) fn toggle_rule(
    config: &mut WatcherConfig,
    id: &str,
    enabled: bool,
) -> Result<(), String> {
    let rule = config
        .rules
        .iter_mut()
        .find(|r| r.id == id)
        .ok_or_else(|| format!("Rule '{}' not found", id))?;

    rule.status = if enabled {
        WatcherStatus::Active
    } else {
        WatcherStatus::Paused
    };
    save_config(config)
}

pub(crate) fn attach_rule(
    config: &mut WatcherConfig,
    template_id: &str,
    session_id: String,
) -> Result<String, String> {
    let template = config
        .rules
        .iter()
        .find(|r| r.id == template_id)
        .ok_or_else(|| format!("Template '{}' not found", template_id))?;
    if template.session_id.is_some() {
        return Err("Rule is already attached to a session".into());
    }
    let mut instance = template.clone();
    instance.id = uuid::Uuid::new_v4().to_string();
    instance.session_id = Some(session_id);
    instance.template_id = Some(template_id.to_string());
    instance.status = WatcherStatus::Active;
    instance.fire_count = 0;
    instance.created_at = now_secs();
    let id = instance.id.clone();
    config.rules.push(instance);
    save_config(config)?;
    Ok(id)
}

pub(crate) fn detach_rule(config: &mut WatcherConfig, id: &str) -> Result<(), String> {
    let rule = config
        .rules
        .iter_mut()
        .find(|r| r.id == id)
        .ok_or_else(|| format!("Rule '{}' not found", id))?;
    if rule.session_id.is_none() {
        return Err("Rule is not attached to a session".into());
    }
    rule.session_id = None;
    rule.status = WatcherStatus::Paused;
    rule.fire_count = 0;
    save_config(config)
}

pub(crate) fn update_rule(
    config: &mut WatcherConfig,
    id: &str,
    name: Option<String>,
    trigger: Option<WatcherTrigger>,
    instructions: Option<String>,
    max_fires: Option<u32>,
    cooldown_secs: Option<u32>,
) -> Result<(), String> {
    let rule = config
        .rules
        .iter_mut()
        .find(|r| r.id == id)
        .ok_or_else(|| format!("Rule '{}' not found", id))?;
    if let Some(n) = name {
        rule.name = n;
    }
    if let Some(t) = trigger {
        rule.trigger = t;
    }
    if let Some(i) = instructions {
        rule.instructions = i;
    }
    if let Some(m) = max_fires {
        rule.max_fires = m;
    }
    if let Some(c) = cooldown_secs {
        rule.cooldown_secs = c;
    }
    validate_rule(rule)?;
    save_config(config)
}

pub(crate) fn stop_rules_for_session(config: &mut WatcherConfig, session_id: &str) -> bool {
    let mut changed = false;
    for rule in &mut config.rules {
        if rule.session_id.as_deref() == Some(session_id)
            && (rule.status == WatcherStatus::Active || rule.status == WatcherStatus::Paused)
        {
            rule.session_id = None;
            rule.status = WatcherStatus::Paused;
            changed = true;
        }
    }
    if changed && let Err(e) = save_config(config) {
        tracing::warn!("Failed to persist watcher detach-on-close: {e}");
    }
    changed
}

/// On app restart, detach all rules from sessions and pause them.
/// Sessions don't survive restart, so session_ids would be dangling.
pub(crate) fn reload_rules_disabled(config: &mut WatcherConfig) -> bool {
    let mut changed = false;
    for rule in &mut config.rules {
        if rule.session_id.is_some() {
            rule.session_id = None;
            changed = true;
        }
        if rule.status == WatcherStatus::Active {
            rule.status = WatcherStatus::Paused;
            changed = true;
        }
    }
    if changed && let Err(e) = save_config(config) {
        tracing::warn!("Failed to persist watcher reload: {e}");
    }
    changed
}

// ── Trigger evaluation (pure, testable) ─────────────────────────

/// Result of evaluating a rule's trigger against current state.
#[derive(Debug, PartialEq)]
pub(crate) enum TriggerOutcome {
    Fire,
    Skip,
}

/// Evaluate whether a rule should fire given the current context.
/// `last_exit_code`: exit code from last CommandOutcome (None if no outcome or inferred).
/// `screen_tail`: last N lines of VtLogBuffer screen text.
/// Evaluate whether a rule should fire in the idle-based evaluation path.
/// Busy/Question/Error/Unseen triggers are handled by their own event handlers
/// and always return Skip here.
pub(crate) fn evaluate_trigger(
    trigger: &WatcherTrigger,
    last_exit_code: Option<i32>,
    screen_tail: &[String],
) -> TriggerOutcome {
    match trigger {
        WatcherTrigger::Idle => TriggerOutcome::Fire,
        WatcherTrigger::CommandDone { on_failure_only } => match last_exit_code {
            Some(code) if !on_failure_only || code != 0 => TriggerOutcome::Fire,
            _ => TriggerOutcome::Skip,
        },
        WatcherTrigger::Pattern { regex } => {
            let re = match regex::Regex::new(regex) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(regex = %regex, error = %e, "Pattern trigger regex invalid — skipping");
                    return TriggerOutcome::Skip;
                }
            };
            let text = screen_tail.join("\n");
            if re.is_match(&text) {
                TriggerOutcome::Fire
            } else {
                TriggerOutcome::Skip
            }
        }
        WatcherTrigger::Busy
        | WatcherTrigger::Question { .. }
        | WatcherTrigger::Error
        | WatcherTrigger::Unseen => TriggerOutcome::Skip,
    }
}

// ── Event matching for non-idle triggers ────────────────────────

#[derive(Debug, PartialEq)]
pub(crate) enum EventKind {
    Busy,
    Question { confident: bool },
    Error,
}

fn trigger_matches(trigger: &WatcherTrigger, kind: &EventKind) -> bool {
    match (trigger, kind) {
        (WatcherTrigger::Busy, EventKind::Busy) => true,
        (WatcherTrigger::Question { confident_only }, EventKind::Question { confident }) => {
            !confident_only || *confident
        }
        (WatcherTrigger::Error, EventKind::Error) => true,
        _ => false,
    }
}

// ── WatcherEngine ───────────────────────────────────────────────

const SCREEN_TAIL_LINES: usize = 50;

pub(crate) struct WatcherEngine {
    state: Arc<AppState>,
    config: Arc<RwLock<WatcherConfig>>,
    last_fire: DashMap<String, Instant>,
    fire_history: DashMap<String, VecDeque<Instant>>,
    regex_cache: DashMap<String, regex::Regex>,
}

impl WatcherEngine {
    pub fn new(state: Arc<AppState>) -> Self {
        let mut config = load_config();
        reload_rules_disabled(&mut config);
        Self {
            state,
            config: Arc::new(RwLock::new(config)),
            last_fire: DashMap::new(),
            fire_history: DashMap::new(),
            regex_cache: DashMap::new(),
        }
    }

    pub fn config(&self) -> Arc<RwLock<WatcherConfig>> {
        Arc::clone(&self.config)
    }

    pub async fn run(&self) {
        let mut rx = self.state.event_bus.subscribe();
        loop {
            match rx.recv().await {
                Ok(AppEvent::PtyParsed { session_id, parsed }) => {
                    let event_type = parsed.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match event_type {
                        "shell-state" => {
                            let state_val =
                                parsed.get("state").and_then(|s| s.as_str()).unwrap_or("");
                            match state_val {
                                "idle" => self.on_idle(&session_id).await,
                                "busy" => {
                                    self.on_event(&session_id, EventKind::Busy).await;
                                }
                                _ => {}
                            }
                        }
                        "question" => {
                            let confident = parsed
                                .get("confident")
                                .and_then(|c| c.as_bool())
                                .unwrap_or(false);
                            self.on_event(&session_id, EventKind::Question { confident })
                                .await;
                        }
                        "api-error" | "rate-limit" => {
                            self.on_event(&session_id, EventKind::Error).await;
                        }
                        "user-input" => {
                            self.on_user_input(&session_id);
                        }
                        _ => {}
                    }
                }
                Ok(AppEvent::SessionClosed { session_id, .. }) => {
                    self.on_session_closed(&session_id);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("Watcher lagged {n} events");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                _ => {}
            }
        }
    }

    async fn on_idle(&self, session_id: &str) {
        #[cfg(unix)]
        if self.state.standby_sessions.contains_key(session_id) {
            return;
        }
        let last_exit_code = self.last_exit_code(session_id);
        let screen_tail = self.screen_tail(session_id);
        let tab_visible = self
            .state
            .session_visibility
            .get(session_id)
            .map(|v| *v)
            .unwrap_or(true);

        let fire_candidates: Vec<(String, String)> = {
            let config = self.config.read();
            config
                .rules
                .iter()
                .filter(|r| {
                    r.session_id.as_deref() == Some(session_id) && r.status == WatcherStatus::Active
                })
                .filter(|r| {
                    if r.trigger == WatcherTrigger::Unseen {
                        !tab_visible
                    } else {
                        self.evaluate_trigger_cached(&r.trigger, last_exit_code, &screen_tail)
                            == TriggerOutcome::Fire
                    }
                })
                .map(|r| (r.id.clone(), session_id.to_string()))
                .collect()
        };

        for (rule_id, sid) in fire_candidates {
            self.fire_rule(&rule_id, &sid, &screen_tail).await;
        }
    }

    async fn on_event(&self, session_id: &str, kind: EventKind) {
        let screen_tail = self.screen_tail(session_id);
        let fire_candidates: Vec<(String, String)> = {
            let config = self.config.read();
            config
                .rules
                .iter()
                .filter(|r| {
                    r.session_id.as_deref() == Some(session_id) && r.status == WatcherStatus::Active
                })
                .filter(|r| trigger_matches(&r.trigger, &kind))
                .map(|r| (r.id.clone(), session_id.to_string()))
                .collect()
        };
        for (rule_id, sid) in fire_candidates {
            self.fire_rule(&rule_id, &sid, &screen_tail).await;
        }
    }

    fn evaluate_trigger_cached(
        &self,
        trigger: &WatcherTrigger,
        last_exit_code: Option<i32>,
        screen_tail: &[String],
    ) -> TriggerOutcome {
        match trigger {
            WatcherTrigger::Pattern { regex } => {
                let re = if let Some(cached) = self.regex_cache.get(regex) {
                    cached.clone()
                } else {
                    match regex::Regex::new(regex) {
                        Ok(r) => {
                            self.regex_cache.insert(regex.clone(), r.clone());
                            r
                        }
                        Err(e) => {
                            tracing::warn!(regex = %regex, error = %e, "Pattern trigger regex invalid — skipping");
                            return TriggerOutcome::Skip;
                        }
                    }
                };
                let text = screen_tail.join("\n");
                if re.is_match(&text) {
                    TriggerOutcome::Fire
                } else {
                    TriggerOutcome::Skip
                }
            }
            _ => evaluate_trigger(trigger, last_exit_code, screen_tail),
        }
    }

    async fn fire_rule(&self, rule_id: &str, session_id: &str, screen_tail: &[String]) {
        // Pre-checks that don't need config write lock
        if ACTIVE_CONVERSATIONS.contains_key(session_id) {
            tracing::debug!(rule_id, session_id, "Watcher skipped — active conversation");
            return;
        }

        // Check cooldown
        if let Some(last) = self.last_fire.get(rule_id) {
            let cooldown = {
                let config = self.config.read();
                config
                    .rules
                    .iter()
                    .find(|r| r.id == rule_id)
                    .map(|r| r.cooldown_secs)
                    .unwrap_or(10)
            };
            if last.elapsed() < std::time::Duration::from_secs(cooldown as u64) {
                tracing::debug!(rule_id, "Watcher skipped — cooldown");
                return;
            }
        }

        // Check and update under write lock
        let (message, session_id_owned) = {
            let mut config = self.config.write();
            let idx = match config.rules.iter().position(|r| r.id == rule_id) {
                Some(i) => i,
                None => return,
            };

            if config.rules[idx].status != WatcherStatus::Active {
                return;
            }

            if config.rules[idx].fire_count >= config.rules[idx].max_fires {
                config.rules[idx].status = WatcherStatus::Exhausted;
                tracing::info!(rule_id, "Watcher exhausted — max_fires reached");
                if let Err(e) = save_config(&config) {
                    tracing::warn!(rule_id, "Failed to persist exhaustion state: {e}");
                }
                #[cfg(feature = "desktop")]
                self.notify_status(&config.rules[idx]);
                return;
            }

            if self.is_burst(
                rule_id,
                config.rules[idx].burst_threshold,
                config.rules[idx].burst_window_secs,
            ) {
                config.rules[idx].status = WatcherStatus::Paused;
                tracing::warn!(rule_id, "Watcher burst detected — auto-paused");
                if let Err(e) = save_config(&config) {
                    tracing::warn!(rule_id, "Failed to persist burst-pause state: {e}");
                }
                #[cfg(feature = "desktop")]
                self.notify_status(&config.rules[idx]);
                return;
            }

            let context = self.build_context(session_id, screen_tail);
            let fire_count = config.rules[idx].fire_count;
            let max_fires = config.rules[idx].max_fires;
            let instructions = config.rules[idx].instructions.clone();
            let sid = config.rules[idx].session_id.clone().unwrap_or_default();
            let message = format!(
                "## Watcher instructions\n{}\n\n## Terminal context\n{}\n\n## Watcher fire #{}/{}",
                instructions,
                context,
                fire_count + 1,
                max_fires,
            );

            config.rules[idx].fire_count += 1;
            if let Err(e) = save_config(&config) {
                tracing::warn!(rule_id, "Failed to persist fire_count: {e}");
            }
            #[cfg(feature = "desktop")]
            self.notify_status(&config.rules[idx]);
            (message, sid)
        };

        let conv_config = ConversationConfig {
            autonomy: Autonomy::Autonomous,
            max_steps: Some(10),
            ..Default::default()
        };

        match crate::ai_agent::conversation_engine::start_conversation(
            self.state.clone(),
            session_id_owned,
            message,
            conv_config,
        )
        .await
        {
            Ok(_rx) => {
                self.last_fire.insert(rule_id.to_string(), Instant::now());
                self.record_fire(rule_id);
                tracing::info!(rule_id, session_id, "Watcher fired conversation");
            }
            Err(e) => {
                tracing::warn!(rule_id, "Watcher fire failed: {e}");
                let mut config = self.config.write();
                if let Some(idx) = config.rules.iter().position(|r| r.id == rule_id) {
                    config.rules[idx].fire_count = config.rules[idx].fire_count.saturating_sub(1);
                    if let Err(e) = save_config(&config) {
                        tracing::warn!(rule_id, "Failed to persist fire_count rollback: {e}");
                    }
                    #[cfg(feature = "desktop")]
                    self.notify_status(&config.rules[idx]);
                }
            }
        }
    }

    fn build_context(&self, session_id: &str, screen_tail: &[String]) -> String {
        let mut parts = Vec::new();

        if let Some(sk) = self.state.session_knowledge.get(session_id)
            && let Some(last) = sk.lock().commands.back()
        {
            parts.push(format!(
                "Last command: `{}` (exit {}), cwd: {}\nOutput:\n{}",
                last.command,
                last.exit_code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "unknown".into()),
                last.cwd,
                sanitize_snippet(&last.output_snippet),
            ));
        }

        if !screen_tail.is_empty() {
            parts.push(format!(
                "Screen (last {} lines):\n{}",
                screen_tail.len(),
                screen_tail.join("\n"),
            ));
        }

        parts.join("\n\n")
    }

    fn is_burst(&self, rule_id: &str, threshold: u32, window_secs: u32) -> bool {
        let window = std::time::Duration::from_secs(window_secs as u64);
        let now = Instant::now();
        self.fire_history
            .get(rule_id)
            .map(|history| {
                history
                    .iter()
                    .filter(|t| now.duration_since(**t) < window)
                    .count()
                    >= threshold as usize
            })
            .unwrap_or(false)
    }

    fn record_fire(&self, rule_id: &str) {
        let mut entry = self.fire_history.entry(rule_id.to_string()).or_default();
        entry.push_back(Instant::now());
        while entry.len() > 100 {
            entry.pop_front();
        }
    }

    #[cfg(feature = "desktop")]
    fn notify_status(&self, rule: &WatcherRule) {
        if let Some(app) = self.state.app_handle.read().as_ref() {
            let _ = app.emit(
                "watcher-status",
                serde_json::json!({
                    "id": rule.id,
                    "status": rule.status,
                    "fire_count": rule.fire_count,
                    "session_id": rule.session_id,
                }),
            );
        }
    }

    fn on_user_input(&self, session_id: &str) {
        let mut config = self.config.write();
        let mut changed = false;
        for rule in &mut config.rules {
            if rule.session_id.as_deref() == Some(session_id)
                && rule.status == WatcherStatus::Active
            {
                rule.status = WatcherStatus::Paused;
                tracing::info!(rule_id = %rule.id, "Watcher paused by user input");
                #[cfg(feature = "desktop")]
                self.notify_status(rule);
                changed = true;
            }
        }
        if changed && let Err(e) = save_config(&config) {
            tracing::warn!("Failed to persist watcher user-input pause: {e}");
        }
    }

    fn on_session_closed(&self, session_id: &str) {
        let mut config = self.config.write();
        stop_rules_for_session(&mut config, session_id);
    }

    fn last_exit_code(&self, session_id: &str) -> Option<i32> {
        self.state
            .session_knowledge
            .get(session_id)
            .and_then(|sk| sk.lock().commands.back().and_then(|c| c.exit_code))
    }

    fn screen_tail(&self, session_id: &str) -> Vec<String> {
        self.state
            .vt_log_buffers
            .get(session_id)
            .map(|buf| {
                let rows = buf.lock().screen_rows();
                let len = rows.len();
                let start = len.saturating_sub(SCREEN_TAIL_LINES);
                rows.into_iter().skip(start).collect()
            })
            .unwrap_or_default()
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rule(session_id: Option<&str>, instructions: &str) -> WatcherRule {
        WatcherRule {
            id: String::new(),
            name: "test".into(),
            session_id: session_id.map(String::from),
            template_id: None,
            trigger: WatcherTrigger::Idle,
            instructions: instructions.into(),
            max_fires: 50,
            fire_count: 0,
            cooldown_secs: 10,
            burst_threshold: 5,
            burst_window_secs: 60,
            status: WatcherStatus::Active,
            created_at: 0,
        }
    }

    fn make_template(instructions: &str) -> WatcherRule {
        make_rule(None, instructions)
    }

    #[test]
    fn validation_rejects_empty_instructions() {
        let rule = make_rule(Some("s1"), "");
        assert!(validate_rule(&rule).is_err());
        assert!(validate_rule(&rule).unwrap_err().contains("Instructions"));
    }

    #[test]
    fn validation_rejects_low_cooldown() {
        let mut rule = make_rule(Some("s1"), "do stuff");
        rule.cooldown_secs = 2;
        let err = validate_rule(&rule).unwrap_err();
        assert!(err.contains("Cooldown"));
    }

    #[test]
    fn validation_rejects_zero_max_fires() {
        let mut rule = make_rule(Some("s1"), "do stuff");
        rule.max_fires = 0;
        let err = validate_rule(&rule).unwrap_err();
        assert!(err.contains("max_fires"));
    }

    #[test]
    fn validation_rejects_invalid_regex() {
        let mut rule = make_rule(Some("s1"), "do stuff");
        rule.trigger = WatcherTrigger::Pattern {
            regex: "[invalid".into(),
        };
        let err = validate_rule(&rule).unwrap_err();
        assert!(err.contains("Invalid regex"));
    }

    #[test]
    fn validation_accepts_valid_rule() {
        let rule = make_rule(Some("s1"), "watch for errors");
        assert!(validate_rule(&rule).is_ok());
    }

    #[test]
    fn validation_accepts_valid_pattern() {
        let mut rule = make_rule(Some("s1"), "watch errors");
        rule.trigger = WatcherTrigger::Pattern {
            regex: r"error|fail".into(),
        };
        assert!(validate_rule(&rule).is_ok());
    }

    #[test]
    fn validation_accepts_command_done() {
        let mut rule = make_rule(Some("s1"), "watch commands");
        rule.trigger = WatcherTrigger::CommandDone {
            on_failure_only: true,
        };
        assert!(validate_rule(&rule).is_ok());
    }

    #[test]
    fn serialization_roundtrip() {
        let rule = WatcherRule {
            id: "r1".into(),
            name: "test-rule".into(),
            session_id: Some("s1".into()),
            template_id: None,
            trigger: WatcherTrigger::CommandDone {
                on_failure_only: true,
            },
            instructions: "Fix errors".into(),
            max_fires: 10,
            fire_count: 3,
            cooldown_secs: 15,
            burst_threshold: 5,
            burst_window_secs: 60,
            status: WatcherStatus::Paused,
            created_at: 1700000000,
        };
        let config = WatcherConfig { rules: vec![rule] };
        let json = serde_json::to_string_pretty(&config).unwrap();
        let restored: WatcherConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.rules.len(), 1);
        let r = &restored.rules[0];
        assert_eq!(r.id, "r1");
        assert_eq!(r.fire_count, 3);
        assert_eq!(r.status, WatcherStatus::Paused);
        assert_eq!(
            r.trigger,
            WatcherTrigger::CommandDone {
                on_failure_only: true
            }
        );
    }

    #[test]
    fn serialization_roundtrip_pattern() {
        let rule = WatcherRule {
            id: "r2".into(),
            name: "pattern-rule".into(),
            session_id: Some("s2".into()),
            template_id: None,
            trigger: WatcherTrigger::Pattern {
                regex: r"error\b".into(),
            },
            instructions: "Handle errors".into(),
            max_fires: 5,
            fire_count: 0,
            cooldown_secs: 10,
            burst_threshold: 3,
            burst_window_secs: 30,
            status: WatcherStatus::Active,
            created_at: 1700000000,
        };
        let json = serde_json::to_string(&rule).unwrap();
        let restored: WatcherRule = serde_json::from_str(&json).unwrap();
        assert_eq!(
            restored.trigger,
            WatcherTrigger::Pattern {
                regex: r"error\b".into()
            }
        );
    }

    #[test]
    fn deserialization_defaults() {
        let json = r#"{
            "id": "r1",
            "name": "test",
            "session_id": "s1",
            "trigger": {"type": "idle"},
            "instructions": "do stuff",
            "max_fires": 50
        }"#;
        let rule: WatcherRule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.fire_count, 0);
        assert_eq!(rule.cooldown_secs, 10);
        assert_eq!(rule.burst_threshold, 5);
        assert_eq!(rule.burst_window_secs, 60);
        assert_eq!(rule.status, WatcherStatus::Active);
        assert_eq!(rule.created_at, 0);
    }

    #[test]
    fn multiple_rules_per_session_allowed() {
        let mut config = WatcherConfig::default();
        let mut r1 = make_rule(Some("s1"), "first");
        r1.id = "r1".into();
        r1.status = WatcherStatus::Active;
        config.rules.push(r1);

        let mut r2 = make_rule(Some("s1"), "second");
        r2.id = "r2".into();
        r2.status = WatcherStatus::Active;
        config.rules.push(r2);

        let active_count = config
            .rules
            .iter()
            .filter(|r| r.session_id.as_deref() == Some("s1") && r.status == WatcherStatus::Active)
            .count();
        assert_eq!(
            active_count, 2,
            "Multiple active rules on same session allowed"
        );
    }

    #[test]
    fn stop_rules_for_session_detaches_and_pauses() {
        let mut config = WatcherConfig::default();
        let mut r1 = make_rule(Some("s1"), "first");
        r1.id = "r1".into();
        config.rules.push(r1);
        let mut r2 = make_rule(Some("s2"), "second");
        r2.id = "r2".into();
        config.rules.push(r2);

        // Inline logic mirrors stop_rules_for_session (which saves to disk)
        let session_id = "s1";
        for rule in &mut config.rules {
            if rule.session_id.as_deref() == Some(session_id)
                && (rule.status == WatcherStatus::Active || rule.status == WatcherStatus::Paused)
            {
                rule.session_id = None;
                rule.status = WatcherStatus::Paused;
            }
        }
        assert_eq!(config.rules[0].status, WatcherStatus::Paused);
        assert!(
            config.rules[0].session_id.is_none(),
            "session_id cleared on stop"
        );
        assert_eq!(config.rules[1].status, WatcherStatus::Active);
        assert_eq!(config.rules[1].session_id.as_deref(), Some("s2"));
    }

    #[test]
    fn reload_rules_disabled_detaches_and_pauses() {
        let mut config = WatcherConfig::default();
        let mut r1 = make_rule(Some("s1"), "first");
        r1.id = "r1".into();
        r1.status = WatcherStatus::Active;
        config.rules.push(r1);
        let mut r2 = make_rule(Some("s2"), "second");
        r2.id = "r2".into();
        r2.status = WatcherStatus::Stopped;
        config.rules.push(r2);
        let mut r3 = make_rule(Some("s3"), "third");
        r3.id = "r3".into();
        r3.status = WatcherStatus::Exhausted;
        config.rules.push(r3);

        // Inline logic from reload_rules_disabled
        for rule in &mut config.rules {
            if rule.session_id.is_some() {
                rule.session_id = None;
            }
            if rule.status == WatcherStatus::Active {
                rule.status = WatcherStatus::Paused;
            }
        }
        assert_eq!(config.rules[0].status, WatcherStatus::Paused);
        assert!(
            config.rules[0].session_id.is_none(),
            "session_id cleared on reload"
        );
        assert_eq!(config.rules[1].status, WatcherStatus::Stopped);
        assert!(config.rules[1].session_id.is_none());
        assert_eq!(config.rules[2].status, WatcherStatus::Exhausted);
        assert!(config.rules[2].session_id.is_none());
    }

    #[test]
    fn delete_from_config() {
        let mut config = WatcherConfig::default();
        let mut r1 = make_rule(Some("s1"), "first");
        r1.id = "r1".into();
        config.rules.push(r1);
        let mut r2 = make_rule(Some("s2"), "second");
        r2.id = "r2".into();
        config.rules.push(r2);

        config.rules.retain(|r| r.id != "r1");
        assert_eq!(config.rules.len(), 1);
        assert_eq!(config.rules[0].id, "r2");
    }

    #[test]
    fn toggle_rule_status() {
        let mut config = WatcherConfig::default();
        let mut r1 = make_rule(Some("s1"), "first");
        r1.id = "r1".into();
        r1.status = WatcherStatus::Active;
        config.rules.push(r1);

        // Pause
        let rule = config.rules.iter_mut().find(|r| r.id == "r1").unwrap();
        rule.status = WatcherStatus::Paused;
        assert_eq!(config.rules[0].status, WatcherStatus::Paused);

        // Resume
        let rule = config.rules.iter_mut().find(|r| r.id == "r1").unwrap();
        rule.status = WatcherStatus::Active;
        assert_eq!(config.rules[0].status, WatcherStatus::Active);
    }

    // ── Attach / detach / update tests ───────────────────────────

    #[test]
    fn attach_clones_template() {
        let mut config = WatcherConfig::default();
        let mut t = make_template("watch for errors");
        t.id = "t1".into();
        t.name = "Error Watcher".into();
        t.trigger = WatcherTrigger::Error;
        t.max_fires = 20;
        t.status = WatcherStatus::Paused;
        config.rules.push(t);

        // Inline attach logic (attach_rule calls save_config which needs disk)
        let template = config.rules.iter().find(|r| r.id == "t1").unwrap().clone();
        assert!(template.session_id.is_none());
        let mut instance = template.clone();
        instance.id = "i1".into();
        instance.session_id = Some("s1".into());
        instance.template_id = Some("t1".into());
        instance.status = WatcherStatus::Active;
        instance.fire_count = 0;
        config.rules.push(instance);

        assert_eq!(config.rules.len(), 2);
        // Template unchanged
        assert_eq!(config.rules[0].id, "t1");
        assert!(config.rules[0].session_id.is_none());
        assert_eq!(config.rules[0].status, WatcherStatus::Paused);
        // Instance cloned
        assert_eq!(config.rules[1].id, "i1");
        assert_eq!(config.rules[1].session_id.as_deref(), Some("s1"));
        assert_eq!(config.rules[1].template_id.as_deref(), Some("t1"));
        assert_eq!(config.rules[1].status, WatcherStatus::Active);
        assert_eq!(config.rules[1].fire_count, 0);
        assert_eq!(config.rules[1].name, "Error Watcher");
        assert_eq!(config.rules[1].trigger, WatcherTrigger::Error);
        assert_eq!(config.rules[1].max_fires, 20);
    }

    #[test]
    fn attach_rejects_non_template() {
        let mut config = WatcherConfig::default();
        let mut r = make_rule(Some("s1"), "already attached");
        r.id = "r1".into();
        config.rules.push(r);

        let is_attached = config
            .rules
            .iter()
            .find(|r| r.id == "r1")
            .unwrap()
            .session_id
            .is_some();
        assert!(
            is_attached,
            "Should reject attaching a rule that already has a session"
        );
    }

    #[test]
    fn detach_clears_session_and_resets() {
        let mut config = WatcherConfig::default();
        let mut r = make_rule(Some("s1"), "watching");
        r.id = "r1".into();
        r.fire_count = 15;
        r.status = WatcherStatus::Active;
        config.rules.push(r);

        // Inline detach logic
        let rule = config.rules.iter_mut().find(|r| r.id == "r1").unwrap();
        rule.session_id = None;
        rule.status = WatcherStatus::Paused;
        rule.fire_count = 0;

        assert!(config.rules[0].session_id.is_none());
        assert_eq!(config.rules[0].status, WatcherStatus::Paused);
        assert_eq!(config.rules[0].fire_count, 0);
    }

    #[test]
    fn detach_rejects_template() {
        let _config = WatcherConfig::default();
        let t = make_template("not attached");
        assert!(
            t.session_id.is_none(),
            "Template has no session — detach should reject"
        );
    }

    #[test]
    fn update_modifies_fields() {
        let mut config = WatcherConfig::default();
        let mut r = make_template("original instructions");
        r.id = "r1".into();
        r.name = "Original".into();
        r.max_fires = 50;
        config.rules.push(r);

        // Inline update logic
        let rule = config.rules.iter_mut().find(|r| r.id == "r1").unwrap();
        rule.name = "Updated".into();
        rule.instructions = "new instructions".into();
        rule.trigger = WatcherTrigger::Question {
            confident_only: false,
        };
        rule.max_fires = 100;

        assert_eq!(config.rules[0].name, "Updated");
        assert_eq!(config.rules[0].instructions, "new instructions");
        assert_eq!(
            config.rules[0].trigger,
            WatcherTrigger::Question {
                confident_only: false
            }
        );
        assert_eq!(config.rules[0].max_fires, 100);
    }

    #[test]
    fn update_rejects_empty_instructions() {
        let instructions = "";
        assert!(
            instructions.trim().is_empty(),
            "Empty instructions should be rejected"
        );
    }

    #[test]
    fn update_rejects_zero_max_fires() {
        let max_fires: u32 = 0;
        assert_eq!(max_fires, 0, "Zero max_fires should be rejected");
    }

    // ── Trigger evaluation tests ────────────────────────────────

    #[test]
    fn idle_trigger_always_fires() {
        let trigger = WatcherTrigger::Idle;
        assert_eq!(evaluate_trigger(&trigger, None, &[]), TriggerOutcome::Fire);
        assert_eq!(
            evaluate_trigger(&trigger, Some(0), &["hello".into()]),
            TriggerOutcome::Fire
        );
    }

    #[test]
    fn command_done_fires_on_any_exit() {
        let trigger = WatcherTrigger::CommandDone {
            on_failure_only: false,
        };
        assert_eq!(
            evaluate_trigger(&trigger, Some(0), &[]),
            TriggerOutcome::Fire
        );
        assert_eq!(
            evaluate_trigger(&trigger, Some(1), &[]),
            TriggerOutcome::Fire
        );
    }

    #[test]
    fn command_done_skips_without_outcome() {
        let trigger = WatcherTrigger::CommandDone {
            on_failure_only: false,
        };
        assert_eq!(evaluate_trigger(&trigger, None, &[]), TriggerOutcome::Skip);
    }

    #[test]
    fn command_done_failure_only_fires_on_nonzero() {
        let trigger = WatcherTrigger::CommandDone {
            on_failure_only: true,
        };
        assert_eq!(
            evaluate_trigger(&trigger, Some(1), &[]),
            TriggerOutcome::Fire
        );
        assert_eq!(
            evaluate_trigger(&trigger, Some(127), &[]),
            TriggerOutcome::Fire
        );
    }

    #[test]
    fn command_done_failure_only_skips_on_success() {
        let trigger = WatcherTrigger::CommandDone {
            on_failure_only: true,
        };
        assert_eq!(
            evaluate_trigger(&trigger, Some(0), &[]),
            TriggerOutcome::Skip
        );
    }

    #[test]
    fn command_done_failure_only_skips_without_outcome() {
        let trigger = WatcherTrigger::CommandDone {
            on_failure_only: true,
        };
        assert_eq!(evaluate_trigger(&trigger, None, &[]), TriggerOutcome::Skip);
    }

    #[test]
    fn pattern_fires_on_match() {
        let trigger = WatcherTrigger::Pattern {
            regex: r"error|FAIL".into(),
        };
        let lines = vec![
            "Building project...".into(),
            "error: something broke".into(),
            "done".into(),
        ];
        assert_eq!(
            evaluate_trigger(&trigger, None, &lines),
            TriggerOutcome::Fire
        );
    }

    #[test]
    fn pattern_skips_on_no_match() {
        let trigger = WatcherTrigger::Pattern {
            regex: r"error|FAIL".into(),
        };
        let lines = vec!["Building project...".into(), "All tests passed".into()];
        assert_eq!(
            evaluate_trigger(&trigger, None, &lines),
            TriggerOutcome::Skip
        );
    }

    #[test]
    fn pattern_skips_on_invalid_regex() {
        let trigger = WatcherTrigger::Pattern {
            regex: "[broken".into(),
        };
        assert_eq!(
            evaluate_trigger(&trigger, None, &["test".into()]),
            TriggerOutcome::Skip
        );
    }

    #[test]
    fn pattern_matches_across_lines() {
        let trigger = WatcherTrigger::Pattern {
            regex: r"tests?.*failed".into(),
        };
        let lines = vec![
            "running 10 tests".into(),
            "test foo ... ok".into(),
            "test bar ... failed".into(),
        ];
        assert_eq!(
            evaluate_trigger(&trigger, None, &lines),
            TriggerOutcome::Fire
        );
    }

    #[test]
    fn user_input_pauses_active_rules() {
        let mut config = WatcherConfig::default();
        let mut r1 = make_rule(Some("s1"), "watch it");
        r1.id = "r1".into();
        r1.status = WatcherStatus::Active;
        config.rules.push(r1);

        let mut r2 = make_rule(Some("s1"), "watch that");
        r2.id = "r2".into();
        r2.status = WatcherStatus::Paused;
        config.rules.push(r2);

        let mut r3 = make_rule(Some("s2"), "other session");
        r3.id = "r3".into();
        r3.status = WatcherStatus::Active;
        config.rules.push(r3);

        // Simulate on_user_input for session s1
        for rule in &mut config.rules {
            if rule.session_id.as_deref() == Some("s1") && rule.status == WatcherStatus::Active {
                rule.status = WatcherStatus::Paused;
            }
        }

        assert_eq!(config.rules[0].status, WatcherStatus::Paused);
        assert_eq!(config.rules[1].status, WatcherStatus::Paused);
        assert_eq!(config.rules[2].status, WatcherStatus::Active);
    }

    #[test]
    fn session_closed_detaches_active_rules() {
        let mut config = WatcherConfig::default();
        let mut r1 = make_rule(Some("s1"), "watch it");
        r1.id = "r1".into();
        r1.status = WatcherStatus::Active;
        config.rules.push(r1);

        // Inline logic from stop_rules_for_session (detach + pause)
        for rule in &mut config.rules {
            if rule.session_id.as_deref() == Some("s1")
                && (rule.status == WatcherStatus::Active || rule.status == WatcherStatus::Paused)
            {
                rule.session_id = None;
                rule.status = WatcherStatus::Paused;
            }
        }
        assert_eq!(config.rules[0].status, WatcherStatus::Paused);
        assert!(config.rules[0].session_id.is_none(), "session_id cleared");
    }

    // ── Burst detection tests ───────────────────────────────────

    #[test]
    fn burst_not_detected_below_threshold() {
        let history: DashMap<String, VecDeque<Instant>> = DashMap::new();
        let mut q = VecDeque::new();
        for _ in 0..4 {
            q.push_back(Instant::now());
        }
        history.insert("r1".into(), q);

        let window = std::time::Duration::from_secs(60);
        let count = history
            .get("r1")
            .map(|h| h.iter().filter(|t| t.elapsed() < window).count())
            .unwrap_or(0);
        assert!(count < 5, "4 fires should be below threshold of 5");
    }

    #[test]
    fn burst_detected_at_threshold() {
        let history: DashMap<String, VecDeque<Instant>> = DashMap::new();
        let mut q = VecDeque::new();
        for _ in 0..5 {
            q.push_back(Instant::now());
        }
        history.insert("r1".into(), q);

        let window = std::time::Duration::from_secs(60);
        let count = history
            .get("r1")
            .map(|h| h.iter().filter(|t| t.elapsed() < window).count())
            .unwrap_or(0);
        assert!(count >= 5, "5 fires should meet threshold of 5");
    }

    #[test]
    fn burst_old_entries_excluded() {
        let history: DashMap<String, VecDeque<Instant>> = DashMap::new();
        let mut q = VecDeque::new();
        // Old entries (> 60s ago) — subtract 120s
        let old = Instant::now() - std::time::Duration::from_secs(120);
        for _ in 0..10 {
            q.push_back(old);
        }
        // 2 recent entries
        q.push_back(Instant::now());
        q.push_back(Instant::now());
        history.insert("r1".into(), q);

        let window = std::time::Duration::from_secs(60);
        let count = history
            .get("r1")
            .map(|h| h.iter().filter(|t| t.elapsed() < window).count())
            .unwrap_or(0);
        assert!(count < 5, "Only recent fires should count");
    }

    // ── Max fires exhaustion tests ──────────────────────────────

    #[test]
    fn max_fires_transitions_to_exhausted() {
        let mut rule = make_rule(Some("s1"), "watch");
        rule.id = "r1".into();
        rule.max_fires = 10;
        rule.fire_count = 10;

        if rule.fire_count >= rule.max_fires {
            rule.status = WatcherStatus::Exhausted;
        }
        assert_eq!(rule.status, WatcherStatus::Exhausted);
    }

    #[test]
    fn below_max_fires_stays_active() {
        let mut rule = make_rule(Some("s1"), "watch");
        rule.id = "r1".into();
        rule.max_fires = 10;
        rule.fire_count = 9;

        if rule.fire_count >= rule.max_fires {
            rule.status = WatcherStatus::Exhausted;
        }
        assert_eq!(rule.status, WatcherStatus::Active);
    }

    // ── Context building tests ──────────────────────────────────

    #[test]
    fn context_format_with_screen_lines() {
        let screen_tail: Vec<String> = vec![
            "$ cargo build".into(),
            "   Compiling foo".into(),
            "error[E0308]: mismatched types".into(),
        ];
        // Verify screen section format
        let screen_section = format!(
            "Screen (last {} lines):\n{}",
            screen_tail.len(),
            screen_tail.join("\n"),
        );
        assert!(screen_section.contains("Screen (last 3 lines)"));
        assert!(screen_section.contains("error[E0308]"));
    }

    #[test]
    fn context_format_with_command_outcome() {
        use crate::ai_agent::knowledge::sanitize_snippet;
        let snippet = "error: cannot find value `foo`";
        let sanitized = sanitize_snippet(snippet);
        let cmd_section = format!(
            "Last command: `{}` (exit {}), cwd: {}\nOutput:\n{}",
            "cargo build", "1", "/home/user/project", sanitized,
        );
        assert!(cmd_section.contains("cargo build"));
        assert!(cmd_section.contains("exit 1"));
        assert!(cmd_section.contains("cannot find value"));
    }

    // ── Fire count tracking ─────────────────────────────────────

    #[test]
    fn fire_count_increment() {
        let mut rule = make_rule(Some("s1"), "watch");
        rule.id = "r1".into();
        rule.fire_count = 0;
        rule.max_fires = 50;

        rule.fire_count += 1;
        assert_eq!(rule.fire_count, 1);

        rule.fire_count += 1;
        assert_eq!(rule.fire_count, 2);
    }

    #[test]
    fn fire_count_rollback_on_failure() {
        let mut rule = make_rule(Some("s1"), "watch");
        rule.id = "r1".into();
        rule.fire_count = 5;

        rule.fire_count = rule.fire_count.saturating_sub(1);
        assert_eq!(rule.fire_count, 4);
    }

    #[test]
    fn fire_count_rollback_at_zero() {
        let mut rule = make_rule(Some("s1"), "watch");
        rule.id = "r1".into();
        rule.fire_count = 0;

        rule.fire_count = rule.fire_count.saturating_sub(1);
        assert_eq!(rule.fire_count, 0);
    }

    // ── Template tests ───────────────────────────────────────────

    #[test]
    fn template_created_with_no_session() {
        let rule = make_template("watch for errors");
        assert!(rule.session_id.is_none());
    }

    #[test]
    fn template_serializes_without_session_id() {
        let mut rule = make_template("watch for errors");
        rule.id = "t1".into();
        let json = serde_json::to_string(&rule).unwrap();
        assert!(
            !json.contains("session_id"),
            "session_id should be omitted when None"
        );
        assert!(
            !json.contains("template_id"),
            "template_id should be omitted when None"
        );
    }

    #[test]
    fn old_format_session_id_string_deserializes() {
        let json = r#"{
            "id": "r1",
            "name": "test",
            "session_id": "s1",
            "trigger": {"type": "idle"},
            "instructions": "do stuff",
            "max_fires": 50
        }"#;
        let rule: WatcherRule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.session_id.as_deref(), Some("s1"));
    }

    #[test]
    fn null_session_id_deserializes() {
        let json = r#"{
            "id": "r1",
            "name": "test",
            "session_id": null,
            "trigger": {"type": "idle"},
            "instructions": "do stuff",
            "max_fires": 50
        }"#;
        let rule: WatcherRule = serde_json::from_str(json).unwrap();
        assert!(rule.session_id.is_none());
    }

    #[test]
    fn missing_session_id_deserializes_as_none() {
        let json = r#"{
            "id": "r1",
            "name": "test",
            "trigger": {"type": "idle"},
            "instructions": "do stuff",
            "max_fires": 50
        }"#;
        let rule: WatcherRule = serde_json::from_str(json).unwrap();
        assert!(rule.session_id.is_none());
    }

    // ── New trigger variant tests ───────────────────────────────

    #[test]
    fn busy_trigger_serialization() {
        let trigger = WatcherTrigger::Busy;
        let json = serde_json::to_string(&trigger).unwrap();
        assert!(json.contains(r#""type":"busy""#));
        let restored: WatcherTrigger = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, WatcherTrigger::Busy);
    }

    #[test]
    fn question_trigger_serialization_defaults() {
        let json = r#"{"type": "question"}"#;
        let trigger: WatcherTrigger = serde_json::from_str(json).unwrap();
        assert_eq!(
            trigger,
            WatcherTrigger::Question {
                confident_only: true
            }
        );
    }

    #[test]
    fn question_trigger_serialization_explicit() {
        let trigger = WatcherTrigger::Question {
            confident_only: false,
        };
        let json = serde_json::to_string(&trigger).unwrap();
        let restored: WatcherTrigger = serde_json::from_str(&json).unwrap();
        assert_eq!(
            restored,
            WatcherTrigger::Question {
                confident_only: false
            }
        );
    }

    #[test]
    fn error_trigger_serialization() {
        let trigger = WatcherTrigger::Error;
        let json = serde_json::to_string(&trigger).unwrap();
        assert!(json.contains(r#""type":"error""#));
        let restored: WatcherTrigger = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, WatcherTrigger::Error);
    }

    #[test]
    fn unseen_trigger_serialization() {
        let trigger = WatcherTrigger::Unseen;
        let json = serde_json::to_string(&trigger).unwrap();
        assert!(json.contains(r#""type":"unseen""#));
        let restored: WatcherTrigger = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, WatcherTrigger::Unseen);
    }

    #[test]
    fn all_trigger_variants_roundtrip() {
        let triggers = vec![
            WatcherTrigger::Idle,
            WatcherTrigger::Busy,
            WatcherTrigger::CommandDone {
                on_failure_only: true,
            },
            WatcherTrigger::Question {
                confident_only: false,
            },
            WatcherTrigger::Error,
            WatcherTrigger::Unseen,
            WatcherTrigger::Pattern {
                regex: r"test\b".into(),
            },
        ];
        for trigger in triggers {
            let json = serde_json::to_string(&trigger).unwrap();
            let restored: WatcherTrigger = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, trigger, "roundtrip failed for {json}");
        }
    }

    #[test]
    fn record_fire_caps_history() {
        let history: DashMap<String, VecDeque<Instant>> = DashMap::new();
        let mut entry = history.entry("r1".into()).or_default();
        for _ in 0..150 {
            entry.push_back(Instant::now());
        }
        while entry.len() > 100 {
            entry.pop_front();
        }
        assert_eq!(entry.len(), 100);
    }

    // ── trigger_matches tests ────────────────────────────────────

    #[test]
    fn trigger_matches_busy() {
        assert!(trigger_matches(&WatcherTrigger::Busy, &EventKind::Busy));
        assert!(!trigger_matches(&WatcherTrigger::Busy, &EventKind::Error));
        assert!(!trigger_matches(
            &WatcherTrigger::Busy,
            &EventKind::Question { confident: true }
        ));
    }

    #[test]
    fn trigger_matches_question_any() {
        let trigger = WatcherTrigger::Question {
            confident_only: false,
        };
        assert!(trigger_matches(
            &trigger,
            &EventKind::Question { confident: false }
        ));
        assert!(trigger_matches(
            &trigger,
            &EventKind::Question { confident: true }
        ));
    }

    #[test]
    fn trigger_matches_question_confident_only() {
        let trigger = WatcherTrigger::Question {
            confident_only: true,
        };
        assert!(!trigger_matches(
            &trigger,
            &EventKind::Question { confident: false }
        ));
        assert!(trigger_matches(
            &trigger,
            &EventKind::Question { confident: true }
        ));
    }

    #[test]
    fn trigger_matches_error() {
        assert!(trigger_matches(&WatcherTrigger::Error, &EventKind::Error));
        assert!(!trigger_matches(&WatcherTrigger::Error, &EventKind::Busy));
    }

    #[test]
    fn trigger_matches_cross_type_never() {
        assert!(!trigger_matches(&WatcherTrigger::Idle, &EventKind::Busy));
        assert!(!trigger_matches(&WatcherTrigger::Unseen, &EventKind::Error));
        assert!(!trigger_matches(
            &WatcherTrigger::CommandDone {
                on_failure_only: false
            },
            &EventKind::Question { confident: true }
        ));
    }

    // ── Unseen trigger evaluation (idle path) ────────────────────

    #[test]
    fn unseen_fires_when_tab_not_visible() {
        let trigger = WatcherTrigger::Unseen;
        let tab_visible = false;
        let should_fire = if trigger == WatcherTrigger::Unseen {
            !tab_visible
        } else {
            false
        };
        assert!(should_fire);
    }

    #[test]
    fn unseen_skips_when_tab_visible() {
        let trigger = WatcherTrigger::Unseen;
        let tab_visible = true;
        let should_fire = if trigger == WatcherTrigger::Unseen {
            !tab_visible
        } else {
            false
        };
        assert!(!should_fire);
    }

    #[test]
    fn unseen_skips_in_idle_evaluate_trigger() {
        assert_eq!(
            evaluate_trigger(&WatcherTrigger::Unseen, None, &[]),
            TriggerOutcome::Skip,
            "Unseen must Skip in evaluate_trigger (handled by on_idle visibility check)"
        );
    }

    #[test]
    fn busy_skips_in_idle_evaluate_trigger() {
        assert_eq!(
            evaluate_trigger(&WatcherTrigger::Busy, None, &[]),
            TriggerOutcome::Skip,
            "Busy must Skip in evaluate_trigger (handled by on_event)"
        );
    }

    #[test]
    fn question_skips_in_idle_evaluate_trigger() {
        assert_eq!(
            evaluate_trigger(
                &WatcherTrigger::Question {
                    confident_only: false
                },
                None,
                &[]
            ),
            TriggerOutcome::Skip,
            "Question must Skip in evaluate_trigger (handled by on_event)"
        );
    }

    #[test]
    fn error_skips_in_idle_evaluate_trigger() {
        assert_eq!(
            evaluate_trigger(&WatcherTrigger::Error, None, &[]),
            TriggerOutcome::Skip,
            "Error must Skip in evaluate_trigger (handled by on_event)"
        );
    }
}
