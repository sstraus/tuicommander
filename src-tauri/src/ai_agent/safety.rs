use serde::Serialize;
use std::sync::LazyLock;

/// Closure that checks whether a file write is sensitive, given (path_lower, filename_lower, components).
type SensitiveFileCheck = dyn Fn(&str, &str, &[&str]) -> bool;

static SAFETY_CHECKER: LazyLock<RegexSafetyChecker> = LazyLock::new(RegexSafetyChecker::compile);

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

/// Split a command string on shell metacharacters (;, &&, ||, |, newlines)
/// and expand $(...) / backtick subshells, returning individual sub-commands
/// for independent safety evaluation.
fn split_shell_commands(input: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        match chars[i] {
            '\n' | ';' => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    commands.push(trimmed);
                }
                current.clear();
                i += 1;
            }
            '&' if i + 1 < len && chars[i + 1] == '&' => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    commands.push(trimmed);
                }
                current.clear();
                i += 2;
            }
            '|' if i + 1 < len && chars[i + 1] == '|' => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    commands.push(trimmed);
                }
                current.clear();
                i += 2;
            }
            '|' => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    commands.push(trimmed);
                }
                current.clear();
                i += 1;
            }
            '$' if i + 1 < len && chars[i + 1] == '(' => {
                // Extract subshell content
                let start = i + 2;
                let mut depth = 1;
                let mut j = start;
                while j < len && depth > 0 {
                    if chars[j] == '(' { depth += 1; }
                    if chars[j] == ')' { depth -= 1; }
                    j += 1;
                }
                let inner: String = chars[start..j.saturating_sub(1)].iter().collect();
                let trimmed = inner.trim().to_string();
                if !trimmed.is_empty() {
                    commands.push(trimmed);
                }
                current.push_str("$(...)");
                i = j;
            }
            '`' => {
                let start = i + 1;
                let end = chars[start..].iter().position(|&c| c == '`').map(|p| start + p).unwrap_or(len);
                let inner: String = chars[start..end].iter().collect();
                let trimmed = inner.trim().to_string();
                if !trimmed.is_empty() {
                    commands.push(trimmed);
                }
                current.push_str("`...`");
                i = if end < len { end + 1 } else { len };
            }
            c => {
                current.push(c);
                i += 1;
            }
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        commands.push(trimmed);
    }
    commands
}

/// Regex-based safety checker that matches known destructive patterns.
/// Evaluates each sub-command independently after splitting on shell operators.
pub struct RegexSafetyChecker {
    needs_approval: Vec<(regex::Regex, &'static str)>,
    blocked: Vec<(regex::Regex, &'static str)>,
}

impl RegexSafetyChecker {
    pub fn get() -> &'static Self {
        &SAFETY_CHECKER
    }

    fn compile() -> Self {
        let needs_approval = vec![
            // rm with -r and -f in any order, including split flags like `rm -r -f`
            (regex::Regex::new(r"\brm\s+(?:-\w*r\w*\s+)*(?:-\w*f|-\w*r)\b.*-\w*[rf]|\brm\s+-\w*rf|\brm\s+-\w*fr").unwrap(), "recursive force delete"),
            // rm -r without -f is also dangerous
            (regex::Regex::new(r"\brm\s+(?:.*\s)?-\w*r").unwrap(), "recursive delete"),
            // git push --force anywhere in args
            (regex::Regex::new(r"\bgit\s+push\b.*(?:--force\b|-f\b)").unwrap(), "force push"),
            (regex::Regex::new(r"\bgit\s+reset\s+--hard\b").unwrap(), "hard reset"),
            (regex::Regex::new(r"\bgit\s+clean\b.*-\w*[fd]").unwrap(), "git clean removes untracked files"),
            (regex::Regex::new(r"(?i)\bdrop\s+(table|database|schema|index)\b").unwrap(), "SQL DROP statement"),
            (regex::Regex::new(r"(?i)\btruncate\s+table\b").unwrap(), "SQL TRUNCATE"),
            (regex::Regex::new(r">\s*/dev/").unwrap(), "write to /dev/"),
            (regex::Regex::new(r"(?i)\bmkfs\b").unwrap(), "filesystem format"),
            (regex::Regex::new(r"(?i)\bdd\s+.*of=/dev/").unwrap(), "dd to device"),
            (regex::Regex::new(r"\bchmod\s+777\b").unwrap(), "world-writable permissions"),
            (regex::Regex::new(r"\bchown\s+-R\b").unwrap(), "recursive ownership change"),
        ];

        let blocked = vec![
            (regex::Regex::new(r"(?:^|\s)\s*sudo\b").unwrap(), "sudo is not allowed in agent context"),
            // Data exfiltration: reading secrets
            (regex::Regex::new(r"\bcat\s+.*(?:\.ssh/|\.env\b|/etc/shadow)").unwrap(), "reading sensitive file"),
            (regex::Regex::new(r"(?:^|\s)\s*(?:env|printenv|set)\s*$").unwrap(), "dumping environment variables"),
        ];

        Self { needs_approval, blocked }
    }
}

impl RegexSafetyChecker {
    pub fn evaluate(&self, command: &str) -> SafetyVerdict {
        let sub_commands = split_shell_commands(command);

        // Evaluate each sub-command — worst verdict wins
        let mut worst = SafetyVerdict::Allow;
        for sub in &sub_commands {
            let verdict = self.evaluate_single(sub);
            match (&worst, &verdict) {
                (_, SafetyVerdict::Block { .. }) => return verdict,
                (SafetyVerdict::Allow, SafetyVerdict::NeedsApproval { .. }) => worst = verdict,
                _ => {}
            }
        }
        worst
    }
}

impl RegexSafetyChecker {
    fn evaluate_single(&self, command: &str) -> SafetyVerdict {
        for (pattern, reason) in &self.blocked {
            if pattern.is_match(command) {
                return SafetyVerdict::Block {
                    reason: reason.to_string(),
                };
            }
        }
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

impl RegexSafetyChecker {
    /// Evaluate a file path before write/edit operations.
    /// Read-only ops (read_file, list_files, search_files) skip this check
    /// entirely — the sandbox is the boundary for reads.
    pub fn evaluate_file_write(&self, path: &str) -> SafetyVerdict {
        use std::path::Path;

        let p = Path::new(path);
        let components: Vec<&str> = p
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect();

        // Defense in depth: block `..` traversal (sandbox should already catch).
        if components.contains(&"..") {
            return SafetyVerdict::Block {
                reason: "path contains `..` — traversal not allowed".to_string(),
            };
        }

        let file_name = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        let file_name_lower = file_name.to_lowercase();
        let path_lower = path.to_lowercase();

        // Sensitive file patterns → NeedsApproval
        let sensitive_patterns: &[(&SensitiveFileCheck, &str)] = &[
            (&|_p, f, _c| f.starts_with(".env"), ".env file"),
            (&|_p, _f, c| c.contains(&".ssh"), ".ssh directory"),
            (&|_p, f, _c| f == ".bashrc" || f == ".zshrc" || f == ".profile" || f == ".bash_profile", "shell config"),
            (&|p, _f, _c| p.contains("credentials") || p.contains("credential"), "credentials file"),
            (&|p, _f, _c| p.contains("secret"), "secrets file"),
            (&|_p, f, _c| f == "cargo.toml" || f == "package.json", "dependency manifest"),
            (&|_p, f, _c| f.ends_with(".lock"), "lock file"),
        ];

        for (check, reason) in sensitive_patterns {
            if check(&path_lower, &file_name_lower, &components) {
                return SafetyVerdict::NeedsApproval {
                    reason: format!("writing to sensitive path: {reason}"),
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

    fn checker() -> &'static RegexSafetyChecker {
        RegexSafetyChecker::get()
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
        let v = checker().evaluate("git push --force-with-lease origin feat");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn sudo_in_string_not_blocked() {
        assert_eq!(checker().evaluate("echo pseudopod"), SafetyVerdict::Allow);
    }

    #[test]
    fn empty_command_is_safe() {
        assert_eq!(checker().evaluate(""), SafetyVerdict::Allow);
    }

    // ── Split-flag and structured parsing ─────────────────────

    #[test]
    fn rm_r_f_split_flags() {
        let v = checker().evaluate("rm -r -f /tmp/stuff");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn rm_r_alone_needs_approval() {
        let v = checker().evaluate("rm -r /tmp/stuff");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn git_push_force_trailing() {
        let v = checker().evaluate("git push origin main --force");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn git_clean_fd() {
        let v = checker().evaluate("git clean -fd");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn git_clean_xfd() {
        let v = checker().evaluate("git clean -xfd");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    // ── Shell metachar splitting ──────────────────────────────

    #[test]
    fn sudo_in_chain_blocked() {
        let v = checker().evaluate("ls && sudo rm -rf /");
        assert!(matches!(v, SafetyVerdict::Block { .. }));
    }

    #[test]
    fn sudo_in_subshell_blocked() {
        let v = checker().evaluate("echo $(sudo cat /etc/shadow)");
        assert!(matches!(v, SafetyVerdict::Block { .. }));
    }

    #[test]
    fn sudo_in_backticks_blocked() {
        let v = checker().evaluate("echo `sudo whoami`");
        assert!(matches!(v, SafetyVerdict::Block { .. }));
    }

    #[test]
    fn destructive_after_newline() {
        let v = checker().evaluate("echo safe\nrm -rf /");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn destructive_after_pipe() {
        let v = checker().evaluate("cat file | rm -rf /tmp");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    // ── Data exfiltration blocks ──────────────────────────────

    #[test]
    fn cat_ssh_key_blocked() {
        let v = checker().evaluate("cat ~/.ssh/id_rsa");
        assert!(matches!(v, SafetyVerdict::Block { .. }));
    }

    #[test]
    fn cat_env_file_blocked() {
        let v = checker().evaluate("cat .env");
        assert!(matches!(v, SafetyVerdict::Block { .. }));
    }

    #[test]
    fn bare_env_blocked() {
        let v = checker().evaluate("env");
        assert!(matches!(v, SafetyVerdict::Block { .. }));
    }

    #[test]
    fn bare_printenv_blocked() {
        let v = checker().evaluate("printenv");
        assert!(matches!(v, SafetyVerdict::Block { .. }));
    }

    #[test]
    fn env_with_args_allowed() {
        // `env VAR=val cmd` is a runner, not a dump
        assert_eq!(checker().evaluate("env FOO=bar cargo test"), SafetyVerdict::Allow);
    }

    // ── ReDoS guard ──────────────────────────────────────────

    #[test]
    fn redos_guard_long_input() {
        let long = "a".repeat(10_000);
        let _ = checker().evaluate(&long); // should not hang
    }

    // ── sudo in variable name not blocked ─────────────────────

    #[test]
    fn sudo_in_var_not_blocked() {
        assert_eq!(checker().evaluate("SUDO_USER=test echo hi"), SafetyVerdict::Allow);
    }

    // ── chmod/chown ───────────────────────────────────────────

    #[test]
    fn chmod_777_needs_approval() {
        let v = checker().evaluate("chmod 777 /tmp/dir");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn chown_recursive_needs_approval() {
        let v = checker().evaluate("chown -R root:root /var");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    // ── split_shell_commands unit tests ───────────────────────

    #[test]
    fn split_simple() {
        let cmds = super::split_shell_commands("ls");
        assert_eq!(cmds, vec!["ls"]);
    }

    #[test]
    fn split_semicolon() {
        let cmds = super::split_shell_commands("echo a; echo b");
        assert_eq!(cmds, vec!["echo a", "echo b"]);
    }

    #[test]
    fn split_and() {
        let cmds = super::split_shell_commands("cd /tmp && ls");
        assert_eq!(cmds, vec!["cd /tmp", "ls"]);
    }

    #[test]
    fn split_or() {
        let cmds = super::split_shell_commands("cmd1 || cmd2");
        assert_eq!(cmds, vec!["cmd1", "cmd2"]);
    }

    #[test]
    fn split_pipe() {
        let cmds = super::split_shell_commands("cat file | grep foo");
        assert_eq!(cmds, vec!["cat file", "grep foo"]);
    }

    #[test]
    fn split_newline() {
        let cmds = super::split_shell_commands("echo a\necho b");
        assert_eq!(cmds, vec!["echo a", "echo b"]);
    }

    #[test]
    fn split_subshell() {
        let cmds = super::split_shell_commands("echo $(whoami)");
        assert_eq!(cmds.len(), 2);
        assert!(cmds.contains(&"whoami".to_string()));
    }

    #[test]
    fn split_backtick() {
        let cmds = super::split_shell_commands("echo `id`");
        assert_eq!(cmds.len(), 2);
        assert!(cmds.contains(&"id".to_string()));
    }

    // ── evaluate_file_write ───────────────────────────────────

    #[test]
    fn write_env_needs_approval() {
        let v = checker().evaluate_file_write(".env");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn write_env_local_needs_approval() {
        let v = checker().evaluate_file_write(".env.local");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn write_env_production_needs_approval() {
        let v = checker().evaluate_file_write("config/.env.production");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn write_ssh_needs_approval() {
        let v = checker().evaluate_file_write(".ssh/authorized_keys");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn write_bashrc_needs_approval() {
        let v = checker().evaluate_file_write(".bashrc");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn write_zshrc_needs_approval() {
        let v = checker().evaluate_file_write(".zshrc");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn write_credentials_needs_approval() {
        let v = checker().evaluate_file_write("config/credentials.json");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn write_secrets_needs_approval() {
        let v = checker().evaluate_file_write("deploy/secrets.yaml");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn write_cargo_toml_needs_approval() {
        let v = checker().evaluate_file_write("Cargo.toml");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn write_package_json_needs_approval() {
        let v = checker().evaluate_file_write("package.json");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn write_lock_file_needs_approval() {
        let v = checker().evaluate_file_write("Cargo.lock");
        assert!(matches!(v, SafetyVerdict::NeedsApproval { .. }));
    }

    #[test]
    fn write_dotdot_blocked() {
        let v = checker().evaluate_file_write("../outside.txt");
        assert!(matches!(v, SafetyVerdict::Block { .. }));
    }

    #[test]
    fn write_normal_file_allowed() {
        assert_eq!(
            checker().evaluate_file_write("src/main.rs"),
            SafetyVerdict::Allow
        );
    }

    #[test]
    fn write_nested_source_allowed() {
        assert_eq!(
            checker().evaluate_file_write("src/utils/helper.ts"),
            SafetyVerdict::Allow
        );
    }
}
