use serde::Serialize;

/// Risk level for special key presses in a terminal context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum KeyRisk {
    /// Safe — no side effects (e.g., Escape)
    Low,
    /// May interrupt a running process (e.g., Ctrl-C)
    Medium,
    /// May terminate the shell or cause data loss (e.g., Ctrl-D on empty line)
    High,
}

/// Special keys that an agent might send to a PTY.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafeKey {
    CtrlC,
    CtrlD,
    CtrlZ,
    Escape,
}

impl SafeKey {
    /// Returns the risk level for this key.
    pub fn risk(&self) -> KeyRisk {
        match self {
            SafeKey::CtrlC => KeyRisk::Medium,
            SafeKey::CtrlD => KeyRisk::High,
            SafeKey::CtrlZ => KeyRisk::Medium,
            SafeKey::Escape => KeyRisk::Low,
        }
    }
}

/// The verdict returned by a safety checker for a given command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum SafetyVerdict {
    /// Command is safe to execute.
    Allow,
    /// Command may be destructive — needs human approval before execution.
    NeedsApproval { reason: String },
    /// Command is blocked outright.
    Block { reason: String },
}

/// Trait for evaluating command safety before PTY execution.
pub trait SafetyChecker: Send + Sync {
    fn evaluate(&self, command: &str) -> SafetyVerdict;
}

/// Regex-based safety checker that matches known destructive patterns.
pub struct RegexSafetyChecker {
    /// Patterns that require approval (destructive but sometimes intentional).
    needs_approval: Vec<(regex::Regex, &'static str)>,
    /// Patterns that are always blocked.
    blocked: Vec<(regex::Regex, &'static str)>,
}

impl RegexSafetyChecker {
    pub fn new() -> Self {
        let needs_approval = vec![
            (regex::Regex::new(r"rm\s+.*-[^\s]*r[^\s]*f|rm\s+.*-[^\s]*f[^\s]*r|rm\s+-rf\b").unwrap(), "recursive force delete"),
            (regex::Regex::new(r"git\s+push\s+.*--force\b|git\s+push\s+-f\b").unwrap(), "force push"),
            (regex::Regex::new(r"git\s+reset\s+--hard\b").unwrap(), "hard reset"),
            (regex::Regex::new(r"(?i)\bdrop\s+(table|database|schema|index)\b").unwrap(), "SQL DROP statement"),
            (regex::Regex::new(r"(?i)\btruncate\s+table\b").unwrap(), "SQL TRUNCATE"),
            (regex::Regex::new(r">\s*/dev/").unwrap(), "write to /dev/"),
            (regex::Regex::new(r"(?i)\bmkfs\b").unwrap(), "filesystem format"),
            (regex::Regex::new(r"(?i)\bdd\s+.*of=/dev/").unwrap(), "dd to device"),
        ];

        let blocked = vec![
            (regex::Regex::new(r"(?:^|\s|&&|\|\||;)\s*sudo\b").unwrap(), "sudo is not allowed in agent context"),
        ];

        Self { needs_approval, blocked }
    }
}

impl SafetyChecker for RegexSafetyChecker {
    fn evaluate(&self, command: &str) -> SafetyVerdict {
        // Check blocked patterns first
        for (pattern, reason) in &self.blocked {
            if pattern.is_match(command) {
                return SafetyVerdict::Block {
                    reason: reason.to_string(),
                };
            }
        }

        // Check needs-approval patterns
        for (pattern, reason) in &self.needs_approval {
            if pattern.is_match(command) {
                return SafetyVerdict::NeedsApproval {
                    reason: reason.to_string(),
                };
            }
        }

        SafetyVerdict::Allow
    }
}

/// Format a rejection verdict as structured JSON for LLM consumption.
pub fn format_rejection(verdict: &SafetyVerdict) -> Option<String> {
    match verdict {
        SafetyVerdict::Allow => None,
        SafetyVerdict::NeedsApproval { reason } => {
            Some(serde_json::json!({
                "status": "needs_approval",
                "reason": reason,
                "action": "Ask the user for explicit confirmation before executing this command."
            }).to_string())
        }
        SafetyVerdict::Block { reason } => {
            Some(serde_json::json!({
                "status": "blocked",
                "reason": reason,
                "action": "Do not execute this command. Find a safer alternative."
            }).to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker() -> RegexSafetyChecker {
        RegexSafetyChecker::new()
    }

    // ── Safe commands return Allow ──────────────────────────────

    #[test]
    fn safe_ls() {
        assert_eq!(checker().evaluate("ls -la"), SafetyVerdict::Allow);
    }

    #[test]
    fn safe_cargo_test() {
        assert_eq!(checker().evaluate("cargo test"), SafetyVerdict::Allow);
    }

    #[test]
    fn safe_git_status() {
        assert_eq!(checker().evaluate("git status"), SafetyVerdict::Allow);
    }

    #[test]
    fn safe_git_push_no_force() {
        assert_eq!(checker().evaluate("git push origin main"), SafetyVerdict::Allow);
    }

    #[test]
    fn safe_cat() {
        assert_eq!(checker().evaluate("cat /etc/hosts"), SafetyVerdict::Allow);
    }

    #[test]
    fn safe_echo() {
        assert_eq!(checker().evaluate("echo hello > output.txt"), SafetyVerdict::Allow);
    }

    // ── Destructive commands need approval ─────────────────────

    #[test]
    fn destructive_rm_rf() {
        let v = checker().evaluate("rm -rf /tmp/stuff");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn destructive_rm_fr() {
        let v = checker().evaluate("rm -fr ./build");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn destructive_git_push_force() {
        let v = checker().evaluate("git push --force origin main");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn destructive_git_push_f() {
        let v = checker().evaluate("git push -f origin main");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn destructive_git_reset_hard() {
        let v = checker().evaluate("git reset --hard HEAD~3");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn destructive_drop_table() {
        let v = checker().evaluate("psql -c 'DROP TABLE users'");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn destructive_drop_database() {
        let v = checker().evaluate("DROP DATABASE production");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn destructive_truncate() {
        let v = checker().evaluate("TRUNCATE TABLE sessions");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn destructive_write_dev() {
        let v = checker().evaluate("echo x > /dev/sda");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn destructive_mkfs() {
        let v = checker().evaluate("mkfs.ext4 /dev/sdb1");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn destructive_dd() {
        let v = checker().evaluate("dd if=/dev/zero of=/dev/sda bs=1M");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    // ── Blocked commands ───────────────────────────────────────

    #[test]
    fn blocked_sudo() {
        let v = checker().evaluate("sudo rm -rf /");
        assert!(matches!(v, SafetyVerdict::Block { .. }));
    }

    #[test]
    fn blocked_sudo_after_chain() {
        let v = checker().evaluate("echo hi && sudo apt install foo");
        assert!(matches!(v, SafetyVerdict::Block { .. }));
    }

    #[test]
    fn blocked_sudo_after_pipe() {
        let v = checker().evaluate("cat file | sudo tee /etc/config");
        assert!(matches!(v, SafetyVerdict::Block { .. }));
    }

    #[test]
    fn blocked_sudo_after_semicolon() {
        let v = checker().evaluate("cd /tmp; sudo chmod 777 /");
        assert!(matches!(v, SafetyVerdict::Block { .. }));
    }

    // ── SafeKey risk levels ────────────────────────────────────

    #[test]
    fn key_ctrl_c_medium() {
        assert_eq!(SafeKey::CtrlC.risk(), KeyRisk::Medium);
    }

    #[test]
    fn key_ctrl_d_high() {
        assert_eq!(SafeKey::CtrlD.risk(), KeyRisk::High);
    }

    #[test]
    fn key_ctrl_z_medium() {
        assert_eq!(SafeKey::CtrlZ.risk(), KeyRisk::Medium);
    }

    #[test]
    fn key_escape_low() {
        assert_eq!(SafeKey::Escape.risk(), KeyRisk::Low);
    }

    // ── format_rejection ───────────────────────────────────────

    #[test]
    fn rejection_allow_returns_none() {
        assert!(format_rejection(&SafetyVerdict::Allow).is_none());
    }

    #[test]
    fn rejection_needs_approval_has_status() {
        let verdict = SafetyVerdict::NeedsApproval {
            reason: "force push".into(),
        };
        let json = format_rejection(&verdict).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["status"], "needs_approval");
        assert_eq!(parsed["reason"], "force push");
        assert!(parsed["action"].as_str().unwrap().len() > 0);
    }

    #[test]
    fn rejection_block_has_status() {
        let verdict = SafetyVerdict::Block {
            reason: "sudo".into(),
        };
        let json = format_rejection(&verdict).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["status"], "blocked");
        assert_eq!(parsed["reason"], "sudo");
    }

    // ── Edge cases ─────────────────────────────────────────────

    #[test]
    fn rm_without_rf_is_safe() {
        assert_eq!(checker().evaluate("rm file.txt"), SafetyVerdict::Allow);
    }

    #[test]
    fn git_push_force_lease_needs_approval() {
        // --force-with-lease still matches --force pattern — intentional
        let v = checker().evaluate("git push --force-with-lease origin feat");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn sudo_in_string_not_blocked() {
        // "sudo" as part of a larger word should not match
        assert_eq!(checker().evaluate("echo pseudopod"), SafetyVerdict::Allow);
    }

    #[test]
    fn empty_command_is_safe() {
        assert_eq!(checker().evaluate(""), SafetyVerdict::Allow);
    }
}
