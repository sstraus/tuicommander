//! OSC 133 shell integration scripts for command block detection.
//!
//! Injects shell hooks (precmd/preexec for zsh, PROMPT_COMMAND/DEBUG trap for bash)
//! that emit FinalTerm/iTerm2-compatible OSC 133 markers:
//!   A = prompt start, C = pre-execution, D = command finished (with exit code)
//!
//! Injection strategy per shell:
//!   zsh  — ZDOTDIR trick: point ZDOTDIR at a wrapper dir whose .zshenv sources
//!          the integration script then delegates to the real dotfiles.
//!   bash — (future) BASH_ENV or --init-file
//!   fish — (future) XDG_CONFIG_HOME/fish/conf.d/ auto-source

use std::path::Path;

/// Zsh shell integration script.
const ZSH_INTEGRATION: &str = r#"# TUIC Shell Integration — OSC 133 command block markers
__tuic_precmd() {
  local ec=$?
  if [[ -n "$__tuic_cmd" ]]; then
    printf '\e]133;D;%d\a' "$ec"
    unset __tuic_cmd
  fi
  printf '\e]133;A\a'
}
__tuic_preexec() {
  printf '\e]133;C\a'
  __tuic_cmd=1
}
[[ " ${precmd_functions[*]} " == *" __tuic_precmd "* ]] || precmd_functions+=(__tuic_precmd)
[[ " ${preexec_functions[*]} " == *" __tuic_preexec "* ]] || preexec_functions+=(__tuic_preexec)
"#;

/// Bash shell integration script.
const BASH_INTEGRATION: &str = r#"# TUIC Shell Integration — OSC 133 command block markers
__tuic_precmd() {
  local ec=$?
  if [[ -n "$__tuic_cmd" ]]; then
    printf '\e]133;D;%d\a' "$ec"
    unset __tuic_cmd
  fi
  printf '\e]133;A\a'
  __tuic_preexec_ready=1
}
__tuic_preexec_trap() {
  [[ -n "$__tuic_preexec_ready" ]] || return
  unset __tuic_preexec_ready
  printf '\e]133;C\a'
  __tuic_cmd=1
}
if [[ -z "$__tuic_installed" ]]; then
  __tuic_installed=1
  PROMPT_COMMAND="__tuic_precmd${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
  trap '__tuic_preexec_trap' DEBUG
fi
"#;

/// Fish shell integration script.
const FISH_INTEGRATION: &str = r#"# TUIC Shell Integration — OSC 133 command block markers
function __tuic_prompt --on-event fish_prompt
  set -l ec $status
  if set -q __tuic_cmd
    printf '\e]133;D;%d\a' $ec
    set -e __tuic_cmd
  end
  printf '\e]133;A\a'
end
function __tuic_preexec --on-event fish_preexec
  printf '\e]133;C\a'
  set -g __tuic_cmd 1
end
"#;

/// Template for the ZDOTDIR `.zshenv` wrapper.  At runtime `{script}` is
/// replaced with the absolute path to `tuic-integration.zsh`.
const ZDOTDIR_ZSHENV: &str = r#"# TUIC ZDOTDIR wrapper — sources integration then restores real dotfiles
source "{script}"
ZDOTDIR="${TUIC_ORIGINAL_ZDOTDIR:-$HOME}"
[[ -f "$ZDOTDIR/.zshenv" ]] && source "$ZDOTDIR/.zshenv"
"#;

/// Zsh dotfile names that ZDOTDIR affects.  We create passthrough wrappers
/// for each so the user's config loads normally from the original ZDOTDIR.
const ZSH_DOTFILES: &[&str] = &[".zprofile", ".zshrc", ".zlogin", ".zlogout"];

/// Write shell integration files to `app_data_dir/shell-integration/` and
/// apply the appropriate injection env vars to `cmd`.
///
/// For zsh this sets up the ZDOTDIR trick.  For other shells it sets an env
/// var pointing to the integration script (manual sourcing for now).
pub(crate) fn inject(
    app_data_dir: &Path,
    shell: &str,
    cmd: &mut portable_pty::CommandBuilder,
) {
    let base = app_data_dir.join("shell-integration");
    if std::fs::create_dir_all(&base).is_err() {
        return;
    }

    if crate::pty::is_wsl_shell(shell) {
        // WSL default shell is bash. Inject bash integration with
        // translated paths so /mnt/c/... references work inside WSL.
        inject_bash_wsl(&base, cmd);
    } else if shell.contains("zsh") {
        inject_zsh(&base, cmd);
    } else if shell.contains("bash") {
        inject_bash(&base, cmd);
    } else if shell.contains("fish") {
        inject_fish(&base, cmd);
    }
}

fn write_if_changed(path: &Path, content: &str) -> bool {
    let needs_write = std::fs::read_to_string(path)
        .map(|existing| existing != content)
        .unwrap_or(true);
    if needs_write {
        std::fs::write(path, content).is_ok()
    } else {
        true
    }
}

fn inject_zsh(base: &Path, cmd: &mut portable_pty::CommandBuilder) {
    // Write the integration script
    let script_path = base.join("tuic-integration.zsh");
    if !write_if_changed(&script_path, ZSH_INTEGRATION) {
        return;
    }

    // Create ZDOTDIR wrapper directory
    let zdotdir = base.join("zdotdir");
    if std::fs::create_dir_all(&zdotdir).is_err() {
        return;
    }

    // .zshenv — sources integration, then restores real ZDOTDIR and sources real .zshenv
    let zshenv_content = ZDOTDIR_ZSHENV.replace("{script}", &script_path.to_string_lossy());
    if !write_if_changed(&zdotdir.join(".zshenv"), &zshenv_content) {
        return;
    }

    // Passthrough wrappers for other dotfiles (so user config still loads)
    for dotfile in ZSH_DOTFILES {
        let wrapper = format!(
            "# TUIC passthrough — load real {dotfile}\n\
             [[ -f \"${{TUIC_ORIGINAL_ZDOTDIR:-$HOME}}/{dotfile}\" ]] && \
             source \"${{TUIC_ORIGINAL_ZDOTDIR:-$HOME}}/{dotfile}\"\n"
        );
        write_if_changed(&zdotdir.join(dotfile), &wrapper);
    }

    // Preserve original ZDOTDIR (may be unset, defaults to $HOME)
    if let Ok(original) = std::env::var("ZDOTDIR") {
        cmd.env("TUIC_ORIGINAL_ZDOTDIR", original);
    }
    cmd.env("ZDOTDIR", zdotdir_path_str(&zdotdir));
}

fn inject_bash(base: &Path, cmd: &mut portable_pty::CommandBuilder) {
    let script_path = base.join("tuic-integration.bash");
    if write_if_changed(&script_path, BASH_INTEGRATION) {
        // BASH_ENV is sourced for non-interactive bash; for interactive login
        // shells we rely on the user sourcing it or a future --init-file approach.
        cmd.env("TUIC_SHELL_INTEGRATION", script_path_str(&script_path));
    }
}

fn inject_fish(base: &Path, cmd: &mut portable_pty::CommandBuilder) {
    // Fish auto-sources scripts in conf.d/ directories under XDG_CONFIG_HOME.
    // For now, just point to the script via env var.
    let script_path = base.join("tuic-integration.fish");
    if write_if_changed(&script_path, FISH_INTEGRATION) {
        cmd.env("TUIC_SHELL_INTEGRATION", script_path_str(&script_path));
    }
}

/// Inject bash integration for WSL shells. The script files live on the
/// Windows filesystem but env vars reference them via `/mnt/` paths so
/// they're accessible inside the WSL Linux environment.
fn inject_bash_wsl(base: &Path, cmd: &mut portable_pty::CommandBuilder) {
    let script_path = base.join("tuic-integration.bash");
    if write_if_changed(&script_path, BASH_INTEGRATION) {
        let wsl_path = crate::pty::windows_to_wsl_path(&script_path_str(&script_path));
        cmd.env("TUIC_SHELL_INTEGRATION", wsl_path);
    }
}

fn zdotdir_path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn script_path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}
