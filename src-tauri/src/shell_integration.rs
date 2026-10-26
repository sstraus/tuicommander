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
const ZSH_INTEGRATION: &str = r#"# TUIC Shell Integration — OSC 133 command block markers + OSC 7770 helpers
__tuic_precmd() {
  local ec=$?
  [[ -z "$TUIC_AGENT_SHIM_DIR" || "$PATH" == "$TUIC_AGENT_SHIM_DIR:"* ]] || PATH="$TUIC_AGENT_SHIM_DIR:$PATH"
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
# OSC 7770 TUIC protocol helpers
tuic_state()   { printf '\e]7770;state=%s\a' "$1"; }
tuic_suggest() { printf '\e]7770;suggest=%s\a' "$*"; }
tuic_intent()  { printf '\e]7770;intent=%s\a' "$*"; }
# Auto-inject --name for Goose so tab↔session mapping is deterministic
if [[ -n "$TUIC_SESSION" ]]; then
  printf '%s\n' "$TUIC_CLAUDE_HELP" | awk '__TUIC_CLAUDE_HELP_USABLE__' || TUIC_CLAUDE_HELP=__TUIC_RECORDED_CLAUDE_HELP__
  __tuic_screen_arg() {
    local flag="$1" skip="$2" a; shift 2
    [[ -n "$flag" && ( -z "$skip" || "$1" != "$skip" ) ]] || return 0
    for a in "$@"; do [[ "$a" == "$flag" ]] && return 0; done
    printf '%s' "$flag"
  }
  claude() {
    case "$1" in
      ""|-*) ;;
      remote-control) command claude "$@"; return;;
      *) if printf '%s\n' "$TUIC_CLAUDE_HELP" | awk -v verb="$1" '/^Commands:/ {commands=1; next} commands && /^  [^ ]/ {split($1, names, "|"); for (i in names) if (names[i] == verb) found=1} END {exit !found}'; then command claude "$@"; return; fi;;
    esac
    local a; for a in "$@"; do
      case "$a" in --settings|--settings=*|--bare) command claude "$@"; return;; esac
    done
    if [[ -n "$TUIC_CLAUDE_SETTINGS" ]]; then command claude --settings "$TUIC_CLAUDE_SETTINGS" "$@"; else command claude "$@"; fi
  }
  codex() {
    local a prev screen
    screen=$(__tuic_screen_arg "$TUIC_CODEX_SCREEN_FLAG" "$TUIC_CODEX_SCREEN_SKIP" "$@")
    for a in "$@"; do
      if [[ "$prev" == "-c" && "$a" == notify=* ]] || [[ "$a" == -cnotify=* || "$a" == --config=notify=* ]]; then command codex ${screen:+"$screen"} "$@"; return; fi
      prev="$a"
    done
    if [[ -n "$TUIC_CODEX_NOTIFY" ]]; then command codex ${screen:+"$screen"} "$@" -c "notify=[\"$TUIC_CODEX_NOTIFY\"]"; else command codex ${screen:+"$screen"} "$@"; fi
  }
  grok() {
    local screen=$(__tuic_screen_arg "$TUIC_GROK_SCREEN_FLAG" "$TUIC_GROK_SCREEN_SKIP" "$@")
    command grok ${screen:+"$screen"} "$@"
  }
  opencode() {
    # Only the default project command starts the interactive TUI.
    local a takes_value=0
    for a in "$@"; do
      if [[ "$takes_value" == 1 ]]; then takes_value=0; continue; fi
      case "$a" in
        --) break;;
        __TUIC_OPENCODE_VALUE_OPTIONS__) takes_value=1;;
        -*) ;;
        __TUIC_OPENCODE_COMMANDS__) command opencode "$@"; return;;
        *) break;;
      esac
    done
    local screen=$(__tuic_screen_arg "$TUIC_OPENCODE_SCREEN_FLAG" "" "$@")
    command opencode ${screen:+"$screen"} "$@"
  }
  goose() {
    local a; for a in "$@"; do
      case "$a" in --name|--name=*|-n|-n?*|--path|--path=*|--resume|-r|--no-session) command goose "$@"; return;; esac
    done
    case "$1" in
      session|run) command goose "$1" --name "$TUIC_SESSION" "${@:2}";;
      *) command goose "$@";;
    esac
  }
fi
"#;

/// Bash shell integration script.
const BASH_INTEGRATION: &str = r#"# TUIC Shell Integration — OSC 133 command block markers + OSC 7770 helpers
__tuic_precmd() {
  local ec=$?
  [[ -z "$TUIC_AGENT_SHIM_DIR" || "$PATH" == "$TUIC_AGENT_SHIM_DIR:"* ]] || PATH="$TUIC_AGENT_SHIM_DIR:$PATH"
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
# OSC 7770 TUIC protocol helpers
tuic_state()   { printf '\e]7770;state=%s\a' "$1"; }
tuic_suggest() { printf '\e]7770;suggest=%s\a' "$*"; }
tuic_intent()  { printf '\e]7770;intent=%s\a' "$*"; }
# Auto-inject --name for Goose so tab↔session mapping is deterministic
if [[ -n "$TUIC_SESSION" ]]; then
  printf '%s\n' "$TUIC_CLAUDE_HELP" | awk '__TUIC_CLAUDE_HELP_USABLE__' || TUIC_CLAUDE_HELP=__TUIC_RECORDED_CLAUDE_HELP__
  __tuic_screen_arg() {
    local flag="$1" skip="$2" a; shift 2
    [[ -n "$flag" && ( -z "$skip" || "$1" != "$skip" ) ]] || return 0
    for a in "$@"; do [[ "$a" == "$flag" ]] && return 0; done
    printf '%s' "$flag"
  }
  claude() {
    case "$1" in
      ""|-*) ;;
      remote-control) command claude "$@"; return;;
      *) if printf '%s\n' "$TUIC_CLAUDE_HELP" | awk -v verb="$1" '/^Commands:/ {commands=1; next} commands && /^  [^ ]/ {split($1, names, "|"); for (i in names) if (names[i] == verb) found=1} END {exit !found}'; then command claude "$@"; return; fi;;
    esac
    local a; for a in "$@"; do
      case "$a" in --settings|--settings=*|--bare) command claude "$@"; return;; esac
    done
    if [[ -n "$TUIC_CLAUDE_SETTINGS" ]]; then command claude --settings "$TUIC_CLAUDE_SETTINGS" "$@"; else command claude "$@"; fi
  }
  codex() {
    local a prev screen
    screen=$(__tuic_screen_arg "$TUIC_CODEX_SCREEN_FLAG" "$TUIC_CODEX_SCREEN_SKIP" "$@")
    for a in "$@"; do
      if [[ "$prev" == "-c" && "$a" == notify=* ]] || [[ "$a" == -cnotify=* || "$a" == --config=notify=* ]]; then command codex ${screen:+"$screen"} "$@"; return; fi
      prev="$a"
    done
    if [[ -n "$TUIC_CODEX_NOTIFY" ]]; then command codex ${screen:+"$screen"} "$@" -c "notify=[\"$TUIC_CODEX_NOTIFY\"]"; else command codex ${screen:+"$screen"} "$@"; fi
  }
  grok() {
    local screen=$(__tuic_screen_arg "$TUIC_GROK_SCREEN_FLAG" "$TUIC_GROK_SCREEN_SKIP" "$@")
    command grok ${screen:+"$screen"} "$@"
  }
  opencode() {
    # Only the default project command starts the interactive TUI.
    local a takes_value=0
    for a in "$@"; do
      if [[ "$takes_value" == 1 ]]; then takes_value=0; continue; fi
      case "$a" in
        --) break;;
        __TUIC_OPENCODE_VALUE_OPTIONS__) takes_value=1;;
        -*) ;;
        __TUIC_OPENCODE_COMMANDS__) command opencode "$@"; return;;
        *) break;;
      esac
    done
    local screen=$(__tuic_screen_arg "$TUIC_OPENCODE_SCREEN_FLAG" "" "$@")
    command opencode ${screen:+"$screen"} "$@"
  }
  goose() {
    local a; for a in "$@"; do
      case "$a" in --name|--name=*|-n|-n?*|--path|--path=*|--resume|-r|--no-session) command goose "$@"; return;; esac
    done
    case "$1" in
      session|run) command goose "$1" --name "$TUIC_SESSION" "${@:2}";;
      *) command goose "$@";;
    esac
  }
fi
"#;

/// Fish shell integration script.
const FISH_INTEGRATION: &str = r#"# TUIC Shell Integration — OSC 133 command block markers + OSC 7770 helpers
function __tuic_prompt --on-event fish_prompt
  set -l ec $status
  if set -q TUIC_AGENT_SHIM_DIR; and test "$PATH[1]" != "$TUIC_AGENT_SHIM_DIR"
    set -gx PATH $TUIC_AGENT_SHIM_DIR $PATH
  end
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
# OSC 7770 TUIC protocol helpers
function tuic_state;   printf '\e]7770;state=%s\a' $argv[1]; end
function tuic_suggest; printf '\e]7770;suggest=%s\a' (string join " " $argv); end
function tuic_intent;  printf '\e]7770;intent=%s\a' (string join " " $argv); end
# Auto-inject --name for Goose so tab↔session mapping is deterministic
if set -q TUIC_SESSION
  if not printf '%s\n' "$TUIC_CLAUDE_HELP" | awk '__TUIC_CLAUDE_HELP_USABLE__'
    set -gx TUIC_CLAUDE_HELP __TUIC_RECORDED_CLAUDE_HELP__
  end
  function __tuic_screen_arg
    set -l flag $argv[1]
    set -l skip $argv[2]
    set -l args $argv[3..]
    if test -z "$flag"
      return
    end
    if test -n "$skip"; and test "$args[1]" = "$skip"
      return
    end
    for a in $args
      if test "$a" = "$flag"
        return
      end
    end
    printf '%s' "$flag"
  end
  function claude --wraps claude
    if test (count $argv) -gt 0
      switch $argv[1]
        case '-*'
        case remote-control
          command claude $argv; return
        case '*'
          if printf '%s\n' "$TUIC_CLAUDE_HELP" | awk -v verb="$argv[1]" '/^Commands:/ {commands=1; next} commands && /^  [^ ]/ {split($1, names, "|"); for (i in names) if (names[i] == verb) found=1} END {exit !found}'
            command claude $argv; return
          end
      end
    end
    for a in $argv
      switch $a
        case --settings '--settings=*' --bare
          command claude $argv; return
      end
    end
    if set -q TUIC_CLAUDE_SETTINGS
      command claude --settings $TUIC_CLAUDE_SETTINGS $argv
    else
      command claude $argv
    end
  end
  function codex --wraps codex
    set -l flag ''
    set -l skip ''
    if set -q TUIC_CODEX_SCREEN_FLAG; set flag $TUIC_CODEX_SCREEN_FLAG; end
    if set -q TUIC_CODEX_SCREEN_SKIP; set skip $TUIC_CODEX_SCREEN_SKIP; end
    set -l screen (__tuic_screen_arg "$flag" "$skip" $argv)
    set -l prev
    for a in $argv
      if test "$prev" = -c; and string match -q 'notify=*' -- $a
        command codex $screen $argv; return
      end
      if string match -q -- '-cnotify=*' $a; or string match -q -- '--config=notify=*' $a
        command codex $screen $argv; return
      end
      set prev $a
    end
    if set -q TUIC_CODEX_NOTIFY
      command codex $screen $argv -c "notify=[\"$TUIC_CODEX_NOTIFY\"]"
    else
      command codex $screen $argv
    end
  end
  function grok --wraps grok
    set -l flag ''
    if set -q TUIC_GROK_SCREEN_FLAG; set flag $TUIC_GROK_SCREEN_FLAG; end
    set -l screen (__tuic_screen_arg "$flag" '' $argv)
    command grok $screen $argv
  end
  function opencode --wraps opencode
    # Only the default project command starts the interactive TUI.
    set -l takes_value 0
    for a in $argv
      if test "$takes_value" = 1
        set takes_value 0
        continue
      end
      switch $a
        case --
          break
        case __TUIC_OPENCODE_VALUE_OPTIONS_FISH__
          set takes_value 1
        case '-*'
        case __TUIC_OPENCODE_COMMANDS_FISH__
          command opencode $argv; return
        case '*'
          break
      end
    end
    set -l flag ''
    if set -q TUIC_OPENCODE_SCREEN_FLAG; set flag $TUIC_OPENCODE_SCREEN_FLAG; end
    set -l screen (__tuic_screen_arg "$flag" '' $argv)
    command opencode $screen $argv
  end
  function goose --wraps goose
    for a in $argv
      switch $a
        case --name '--name=*' '-n*' --path '--path=*' --resume -r --no-session
          command goose $argv
          return
      end
    end
    switch $argv[1]
      case session run
        command goose $argv[1] --name $TUIC_SESSION $argv[2..]
      case '*'
        command goose $argv
    end
  end
end
"#;

/// POSIX sh shim for agents started by name in a TUIC shell (story 1810-c28c).
///
/// Every agent name below is a symlink to this one file in a TUIC-owned dir that
/// `inject` puts first on `PATH`, and that the integration scripts move back to
/// the front at every prompt (a user rc may prepend the agent's own dir later).
/// So a typed command, an alias, a shell function's `command <agent>` and a
/// wrapper script that runs the agent by name all reach it; an absolute path
/// does not. It adds only the launch instructions, which are safe in any child
/// process. The status hooks stay in the shell functions: they must apply only to
/// a command typed in the tab, never to a `codex exec` an agent runs as a tool.
///
/// It reads the instruction file at exec time, so a Settings edit applies to the
/// next launch without reopening the shell. An empty or missing file means off.
#[cfg(unix)]
const AGENT_SHIM: &str = r#"#!/bin/sh
# TUIC agent shim: add TUIC's launch instructions, then exec the real agent.
name=${0##*/}
real=
set -f
old_ifs=$IFS
IFS=:
for dir in $PATH; do
  [ -n "$dir" ] || dir=.
  # Skip every TUIC shim dir, not only this one: a nested instance adds its own.
  [ -e "$dir/tuic-agent-shim" ] && continue
  if [ -f "$dir/$name" ] && [ -x "$dir/$name" ]; then real=$dir/$name; break; fi
done
IFS=$old_ifs
set +f
if [ -z "$real" ]; then
  printf 'tuic: %s: command not found\n' "$name" >&2
  exit 127
fi
ext=md
[ "$name" = codex ] && ext=toml
suffix=
[ "$TUIC_SPAWNED_CHILD" = 1 ] && suffix=-child
file=${TUIC_AGENT_INSTRUCTIONS_DIR:+$TUIC_AGENT_INSTRUCTIONS_DIR/instructions-$name$suffix.$ext}
if [ "$name" = gemini ]; then
  [ -z "$TUIC_ATTACHMENTS_DIR" ] || set -- "--include-directories=$TUIC_ATTACHMENTS_DIR" "$@"
  exec "$real" "$@"
fi
if [ "$name" = ego ] && [ -n "$TUIC_ATTACHMENTS_DIR" ]; then
  case $1 in
    run|resume) first=$1; shift; set -- "$first" "--read-only-dir=$TUIC_ATTACHMENTS_DIR" "$@";;
    ""|-*) set -- "--read-only-dir=$TUIC_ATTACHMENTS_DIR" "$@";;
  esac
fi
if [ "$name" = claude ]; then
  case $1 in
    ""|-*) ;;
    remote-control) exec "$real" "$@";;
    *)
      help=$TUIC_CLAUDE_HELP
      printf '%s\n' "$help" | awk '__TUIC_CLAUDE_HELP_USABLE__' || help=__TUIC_RECORDED_CLAUDE_HELP__
      if printf '%s\n' "$help" | awk -v verb="$1" '/^Commands:/ {commands=1; next} commands && /^  [^ ]/ {split($1, names, "|"); for (i in names) if (names[i] == verb) found=1} END {exit !found}'; then exec "$real" "$@"; fi;;
  esac
  [ -z "$TUIC_ATTACHMENTS_DIR" ] || set -- "--add-dir=$TUIC_ATTACHMENTS_DIR" "$@"
fi
[ -n "$file" ] && [ -s "$file" ] || exec "$real" "$@"
# True when an argument before a bare -- is one of $flags, alone or as flag=value.
has_flag() {
  for arg in "$@"; do
    [ "$arg" = -- ] && return 1
    for flag in $flags; do
      case $arg in "$flag"|"$flag"=*) return 0;; esac
    done
  done
  return 1
}
case $name in
  claude)
    flags="-p --print --system-prompt --system-prompt-file --append-system-prompt --append-system-prompt-file"
    has_flag "$@" || exec "$real" --append-system-prompt-file "$file" "$@";;
  codex)
    case $1 in exec|e) exec "$real" "$@";; esac
    prev=
    for arg in "$@"; do
      [ "$prev" = -c ] && case $arg in developer_instructions=*) exec "$real" "$@";; esac
      case $arg in -cdeveloper_instructions=*|--config=developer_instructions=*) exec "$real" "$@";; esac
      prev=$arg
    done
    exec "$real" -c "$(cat "$file")" "$@";;
  grok)
    flags="-p --single --rules"
    has_flag "$@" || exec "$real" --rules="$(cat "$file")" "$@";;
  goose)
    case $1 in
      session|s)
        flags=--system
        if ! has_flag "$@"; then first=$1; shift; exec "$real" "$first" --system="$(cat "$file")" "$@"; fi;;
    esac;;
  pi)
    case $1 in install|remove|uninstall|update|list|config|auth) exec "$real" "$@";; esac
    flags="-p --print --append-system-prompt"
    has_flag "$@" || exec "$real" --append-system-prompt "$file" "$@";;
  ego)
    flags=--append-system
    has_flag "$@" && exec "$real" "$@"
    case $1 in
      resume) shift; exec "$real" resume --append-system "@$file" "$@";;
      run|__TUIC_EGO_NON_RUN_COMMANDS__) ;;
      *) exec "$real" --append-system "@$file" "$@";;
    esac;;
esac
exec "$real" "$@"
"#;

/// Agents the shim serves. OpenCode is absent: TUIC shells export its native
/// `OPENCODE_CONFIG_CONTENT`, which reaches absolute-path launches too.
#[cfg(unix)]
const SHIM_AGENTS: [&str; 7] = ["claude", "codex", "grok", "goose", "pi", "ego", "gemini"];

/// The marker file the shim looks for to skip a TUIC shim dir on `PATH`.
#[cfg(unix)]
const SHIM_FILE: &str = "tuic-agent-shim";

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
pub(crate) fn inject(app_data_dir: &Path, shell: &str, cmd: &mut portable_pty::CommandBuilder) {
    let base = app_data_dir.join("shell-integration");
    if std::fs::create_dir_all(&base).is_err() {
        return;
    }
    if crate::agent_hook_launch::enabled("claude") {
        // Publish installed help, or the same recorded fallback used by managed launches.
        let help = crate::agent::cli_help("claude");
        cmd.env(
            "TUIC_CLAUDE_HELP",
            help.as_deref()
                .unwrap_or(crate::agent_hook_launch::RECORDED_CLAUDE_HELP),
        );
        cmd.env(
            "TUIC_CLAUDE_SETTINGS",
            app_data_dir.join("agent-hooks/claude.json"),
        );
    }
    if crate::agent_hook_launch::enabled("codex") {
        cmd.env(
            "TUIC_CODEX_NOTIFY",
            app_data_dir.join("agent-hooks/codex-notify.sh"),
        );
    }
    let agents_config = crate::config::load_agents_config();
    for (agent, _, _) in crate::agent_hook_launch::SCREEN_POLICIES {
        inject_screen_policy(cmd, &agents_config, agent, agent);
    }
    inject_launch_instructions(app_data_dir, &base, shell, cmd);

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

/// Launch instructions for agents started from this shell: OpenCode through its
/// env var, the other agents through the PATH shim (unix shells only).
fn inject_launch_instructions(
    app_data_dir: &Path,
    base: &Path,
    shell: &str,
    cmd: &mut portable_pty::CommandBuilder,
) {
    if let Some((key, value)) = crate::launch_instructions::env("opencode", &[], app_data_dir)
        && cmd.get_env(&key).is_none()
    {
        cmd.env(key, value);
    }
    cmd.env(
        "TUIC_AGENT_INSTRUCTIONS_DIR",
        app_data_dir.join("agent-hooks"),
    );
    if cfg!(windows) || crate::pty::is_wsl_shell(shell) {
        return;
    }
    let bin = base.join("bin");
    if let Err(error) = install_agent_shim(&bin) {
        tracing::error!(
            source = "agent_hooks",
            "Failed to install the agent shim: {error}"
        );
        return;
    }
    let path = cmd
        .get_env("PATH")
        .map(|path| path.to_os_string())
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_default();
    let mut entries = vec![bin.clone()];
    entries.extend(std::env::split_paths(&path).filter(|entry| entry != &bin));
    if let Ok(joined) = std::env::join_paths(entries) {
        cmd.env("PATH", joined);
    }
    cmd.env("TUIC_AGENT_SHIM_DIR", bin);
}

/// The shim with the recorded Claude help and ego verbs filled in.
#[cfg(unix)]
fn render_shim() -> String {
    render_integration(AGENT_SHIM).replace(
        "__TUIC_EGO_NON_RUN_COMMANDS__",
        &crate::agent_hook_launch::EGO_NON_RUN_COMMANDS.join("|"),
    )
}

/// Write the shim once (an unchanged file keeps its inode, so the exec-time
/// scan is paid once per content change) and link every served agent to it.
#[cfg(unix)]
fn install_agent_shim(bin: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(bin)?;
    let shim = bin.join(SHIM_FILE);
    let content = render_shim();
    if std::fs::read_to_string(&shim).ok().as_deref() != Some(content.as_str()) {
        let staging = bin.join(format!("{SHIM_FILE}.{}", uuid::Uuid::new_v4()));
        std::fs::write(&staging, &content)?;
        std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o755))?;
        std::fs::rename(&staging, &shim)?;
    }
    for agent in SHIM_AGENTS {
        let link = bin.join(agent);
        if std::fs::read_link(&link).is_ok_and(|target| target == Path::new(SHIM_FILE)) {
            continue;
        }
        let staging = bin.join(format!("{agent}.{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_file(&staging);
        std::os::unix::fs::symlink(SHIM_FILE, &staging)?;
        std::fs::rename(&staging, &link)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn install_agent_shim(_bin: &Path) -> std::io::Result<()> {
    Ok(())
}

fn inject_screen_policy(
    cmd: &mut portable_pty::CommandBuilder,
    config: &crate::config::AgentsConfig,
    agent_type: &str,
    binary_path: &str,
) {
    let prefix = format!(
        "TUIC_{}_SCREEN",
        agent_type.to_ascii_uppercase().replace('-', "_")
    );
    let flag_key = format!("{prefix}_FLAG");
    let skip_key = format!("{prefix}_SKIP");
    cmd.env_remove(&flag_key);
    cmd.env_remove(&skip_key);
    if !crate::agent_hook_launch::prevents_alt_screen_in(config, agent_type) {
        return;
    }
    let Some((flag, skipped_command)) = crate::agent_hook_launch::screen_policy(agent_type) else {
        return;
    };
    if !crate::agent::supports_no_alt_screen(agent_type, binary_path) {
        return;
    }
    cmd.env(flag_key, flag);
    // OpenCode shell wrappers classify all verbs; the shared run exclusion
    // remains relevant only to structured launches.
    if agent_type != "opencode"
        && let Some(command) = skipped_command
    {
        cmd.env(skip_key, command);
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

/// Subcommands and aliases advertised by the recorded OpenCode 2026-10-14 help.
/// Shared by the three shell wrappers; paths such as `./mcp` remain interactive.
const OPENCODE_COMMANDS: &str = "completion|acp|mcp|attach|run|debug|providers|auth|agent|upgrade|uninstall|serve|web|models|stats|export|import|github|pr|session|plugin|plug|db";

/// Root options that consume a value in the same recorded OpenCode help.
const OPENCODE_VALUE_OPTIONS: &str = "--log-level|--port|--hostname|--mdns-domain|--cors|-m|--model|-s|--session|--prompt|--agent|--replay-limit";

/// Embed the recorded help as one shell-quoted value, shared by all wrappers.
fn render_integration(template: &str) -> String {
    let help = crate::agent_hook_launch::RECORDED_CLAUDE_HELP.replace('\'', "'\\''");
    template
        .replace("__TUIC_OPENCODE_COMMANDS__", OPENCODE_COMMANDS)
        .replace("__TUIC_OPENCODE_COMMANDS_FISH__", &OPENCODE_COMMANDS.replace('|', " "))
        .replace("__TUIC_OPENCODE_VALUE_OPTIONS__", OPENCODE_VALUE_OPTIONS)
        .replace("__TUIC_OPENCODE_VALUE_OPTIONS_FISH__", &OPENCODE_VALUE_OPTIONS.replace('|', " "))
        .replace("__TUIC_RECORDED_CLAUDE_HELP__", &format!("'{help}'"))
        .replace(
            "__TUIC_CLAUDE_HELP_USABLE__",
            "/^Commands:/ {commands=1; next} commands && /^  [^ ]/ && NF {found=1} END {exit !found}",
        )
}

fn inject_zsh(base: &Path, cmd: &mut portable_pty::CommandBuilder) {
    // Write the integration script
    let script_path = base.join("tuic-integration.zsh");
    if !write_if_changed(&script_path, &render_integration(ZSH_INTEGRATION)) {
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
        let mut wrapper = format!(
            "# TUIC passthrough — load real {dotfile}\n\
             [[ -f \"${{TUIC_ORIGINAL_ZDOTDIR:-$HOME}}/{dotfile}\" ]] && \
             source \"${{TUIC_ORIGINAL_ZDOTDIR:-$HOME}}/{dotfile}\"\n"
        );
        if dotfile == &".zshrc" {
            wrapper.push_str(
                "# Ensure completion is active (ZDOTDIR trick can skip system compinit)\n\
                 if [[ -o interactive ]] && ! type compdef >/dev/null 2>&1; then\n\
                 \x20 autoload -Uz compinit && compinit -C\n\
                 fi\n",
            );
        }
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
    if write_if_changed(&script_path, &render_integration(BASH_INTEGRATION)) {
        // BASH_ENV is sourced for non-interactive bash; for interactive login
        // shells we rely on the user sourcing it or a future --init-file approach.
        cmd.env("TUIC_SHELL_INTEGRATION", script_path_str(&script_path));
    }
}

fn inject_fish(base: &Path, cmd: &mut portable_pty::CommandBuilder) {
    // Fish auto-sources scripts in conf.d/ directories under XDG_CONFIG_HOME.
    // For now, just point to the script via env var.
    let script_path = base.join("tuic-integration.fish");
    if write_if_changed(&script_path, &render_integration(FISH_INTEGRATION)) {
        cmd.env("TUIC_SHELL_INTEGRATION", script_path_str(&script_path));
    }
}

/// Inject bash integration for WSL shells. The script files live on the
/// Windows filesystem but env vars reference them via `/mnt/` paths so
/// they're accessible inside the WSL Linux environment.
fn inject_bash_wsl(base: &Path, cmd: &mut portable_pty::CommandBuilder) {
    let script_path = base.join("tuic-integration.bash");
    if write_if_changed(&script_path, &render_integration(BASH_INTEGRATION)) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injected_shell_uses_agent_settings_and_clears_stale_screen_environment() {
        use std::ffi::OsStr;
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let mut config = crate::config::AgentsConfig::default();
        for agent in ["codex", "grok", "opencode"] {
            config.agents.insert(
                agent.into(),
                crate::config::AgentSettings {
                    prevent_alt_screen: Some(false),
                    ..Default::default()
                },
            );
        }
        crate::config::save_agents_config(crate::config::AgentsConfig::default(), config).unwrap();
        let mut cmd = portable_pty::CommandBuilder::new("bash");
        cmd.env("TUIC_CODEX_SCREEN_FLAG", "stale");
        inject(dir.path(), "bash", &mut cmd);
        assert_eq!(cmd.get_env("TUIC_CODEX_SCREEN_FLAG"), None);
        assert_eq!(cmd.get_env("TUIC_GROK_SCREEN_FLAG"), None);
        assert_eq!(cmd.get_env("TUIC_OPENCODE_SCREEN_FLAG"), None);
        assert!(cmd.get_env("TUIC_CLAUDE_SETTINGS").is_some());
        assert!(cmd.get_env("TUIC_CODEX_NOTIFY").is_some());
        assert_eq!(
            cmd.get_env("TUIC_SHELL_INTEGRATION"),
            Some(OsStr::new(
                &dir.path()
                    .join("shell-integration")
                    .join("tuic-integration.bash")
            ))
        );
    }

    /// Catches: opencode started from a TUIC shell (absolute path included)
    /// without the instructions, or a shell whose PATH misses the shim.
    #[cfg(unix)]
    #[test]
    fn injected_shell_exports_launch_instructions_and_the_shim_first_on_path() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(
            dir.path()
                .starts_with(crate::test_support::test_temp_root())
        );
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let mut cmd = portable_pty::CommandBuilder::new("zsh");
        // Catches: an inherited TUIC launch config masks this shell's injection.
        // Production intentionally preserves an explicit OpenCode config.
        cmd.env_remove("OPENCODE_CONFIG_CONTENT");
        cmd.env("PATH", "/usr/bin:/bin");
        inject(dir.path(), "zsh", &mut cmd);
        let bin = dir.path().join("shell-integration/bin");
        let opencode = dir.path().join("agent-hooks/instructions-opencode.md");
        assert_eq!(
            cmd.get_env("OPENCODE_CONFIG_CONTENT")
                .and_then(|v| v.to_str()),
            Some(
                serde_json::json!({"instructions": [opencode.to_string_lossy()]})
                    .to_string()
                    .as_str()
            )
        );
        assert!(
            std::fs::read_to_string(&opencode)
                .unwrap()
                .contains("intent:")
        );
        assert_eq!(
            cmd.get_env("TUIC_AGENT_INSTRUCTIONS_DIR"),
            Some(dir.path().join("agent-hooks").as_os_str())
        );
        assert_eq!(cmd.get_env("TUIC_AGENT_SHIM_DIR"), Some(bin.as_os_str()));
        let path = format!("{}:/usr/bin:/bin", bin.display());
        assert_eq!(
            cmd.get_env("PATH").and_then(|v| v.to_str()),
            Some(path.as_str())
        );
        for agent in SHIM_AGENTS {
            assert_eq!(
                std::fs::read_link(bin.join(agent)).unwrap(),
                Path::new(SHIM_FILE)
            );
        }
        assert!(
            !bin.join("opencode").exists(),
            "opencode uses its env var, not the shim"
        );
        // A second shell must not stack a second copy of the dir.
        inject(dir.path(), "zsh", &mut cmd);
        assert_eq!(
            cmd.get_env("PATH").and_then(|v| v.to_str()),
            Some(path.as_str())
        );
    }

    /// Catches: two tabs restored at once (PTY spawns run on parallel threads of
    /// one process) racing on a staging name keyed by the process id, so one
    /// install fails and that tab gets no shim dir on PATH.
    #[cfg(unix)]
    #[test]
    fn concurrent_shim_installs_all_succeed() {
        for round in 0..20 {
            let dir = tempfile::TempDir::new().unwrap();
            assert!(
                dir.path()
                    .starts_with(crate::test_support::test_temp_root())
            );
            let bin = std::sync::Arc::new(dir.path().join(format!("bin{round}")));
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    let (bin, barrier) = (bin.clone(), barrier.clone());
                    std::thread::spawn(move || {
                        barrier.wait();
                        install_agent_shim(&bin)
                    })
                })
                .collect();
            for handle in handles {
                handle
                    .join()
                    .unwrap()
                    .unwrap_or_else(|error| panic!("round {round}: {error}"));
            }
            for agent in SHIM_AGENTS {
                assert_eq!(
                    std::fs::read_link(bin.join(agent)).unwrap(),
                    Path::new(SHIM_FILE)
                );
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn shell_environment_exports_only_supported_enabled_screen_policies() {
        use std::ffi::OsStr;
        let supported = crate::test_support::fake_ssh_script(
            "shell-screen-supported",
            "printf '%s\\n' '--no-alt-screen --mini'",
            "echo --no-alt-screen --mini",
        );
        let unsupported = crate::test_support::fake_ssh_script(
            "shell-screen-unsupported",
            "printf '%s\\n' 'old help'",
            "echo old help",
        );
        let mut config = crate::config::AgentsConfig::default();
        let mut cmd = portable_pty::CommandBuilder::new("bash");
        inject_screen_policy(&mut cmd, &config, "codex", &supported.to_string_lossy());
        assert_eq!(
            cmd.get_env("TUIC_CODEX_SCREEN_FLAG"),
            Some(OsStr::new("--no-alt-screen"))
        );
        assert_eq!(
            cmd.get_env("TUIC_CODEX_SCREEN_SKIP"),
            Some(OsStr::new("exec"))
        );
        inject_screen_policy(&mut cmd, &config, "opencode", &supported.to_string_lossy());
        assert_eq!(
            cmd.get_env("TUIC_OPENCODE_SCREEN_FLAG"),
            Some(OsStr::new("--mini"))
        );

        inject_screen_policy(&mut cmd, &config, "codex", &unsupported.to_string_lossy());
        assert_eq!(cmd.get_env("TUIC_CODEX_SCREEN_FLAG"), None);
        assert_eq!(cmd.get_env("TUIC_CODEX_SCREEN_SKIP"), None);
        config.agents.insert(
            "opencode".into(),
            crate::config::AgentSettings {
                prevent_alt_screen: Some(false),
                ..Default::default()
            },
        );
        inject_screen_policy(&mut cmd, &config, "opencode", &supported.to_string_lossy());
        assert_eq!(cmd.get_env("TUIC_OPENCODE_SCREEN_FLAG"), None);
    }

    fn assert_wrapper_paths(shell: &str, script: &str) {
        let claude_inject = script
            .matches("--settings \"$TUIC_CLAUDE_SETTINGS\"")
            .count()
            + script.matches("--settings $TUIC_CLAUDE_SETTINGS").count();
        let codex_inject = script
            .matches("notify=[\\\"$TUIC_CODEX_NOTIFY\\\"]")
            .count();
        assert_eq!(claude_inject, 1, "{shell}: inject Claude settings once");
        assert_eq!(codex_inject, 1, "{shell}: inject Codex notify once");

        assert!(
            script.contains("--settings=*"),
            "{shell}: skip explicit Claude --settings=value"
        );
        assert!(
            script.contains("--settings"),
            "{shell}: skip explicit Claude --settings value"
        );
        assert!(
            script.contains("--bare"),
            "{shell}: skip Claude injection in bare mode"
        );
        assert!(
            script.contains("notify=*"),
            "{shell}: skip explicit Codex notify override"
        );

        assert!(
            script.contains("else\n      command claude") || script.contains("else command claude"),
            "{shell}: Claude setting-off passthrough"
        );
        assert!(
            script.contains("else\n      command codex") || script.contains("else command codex"),
            "{shell}: Codex setting-off passthrough"
        );
    }

    #[test]
    fn bash_wrappers_cover_inject_user_override_skip_and_setting_off() {
        assert_wrapper_paths("bash", BASH_INTEGRATION);
    }

    #[test]
    fn zsh_wrappers_cover_inject_user_override_skip_and_setting_off() {
        assert_wrapper_paths("zsh", ZSH_INTEGRATION);
    }

    #[test]
    fn fish_wrappers_cover_inject_user_override_skip_and_setting_off() {
        assert_wrapper_paths("fish", FISH_INTEGRATION);
    }

    /// Launch the wrappers in a real shell and read back the command line they
    /// build, for each of inject / skip-when-user-passed / setting-off.
    ///
    /// `assert_wrapper_paths` greps the script text, which is why it cannot
    /// stand in for this: change one wrapper's argument loop from `"$@"` to
    /// `"$1"` and every literal it looks for survives, yet
    /// `claude --model opus --settings /user/settings.json` comes out carrying
    /// a second `--settings` (measured by hand, 2026-09-13). Only a launch
    /// sees that. The two layers are complements — the grep runs on Windows
    /// too, where this module does not exist.
    #[cfg(unix)]
    mod launch {
        // Named, not a glob: these constants live two modules up, and a glob of
        // the parent's own glob is easy to break by accident.
        use super::super::{
            BASH_INTEGRATION, FISH_INTEGRATION, ZSH_INTEGRATION, install_agent_shim,
            render_integration,
        };
        use std::path::{Path, PathBuf};
        use std::process::Command;

        /// Stand-in for the agent binary: prints the argument list the wrapper
        /// built, and nothing else.
        ///
        /// It is installed as a **symlink to `/bin/echo`**, never as a freshly
        /// written script. On macOS the first exec of a new executable blocks
        /// on an exec-time code scan — AGENTS.md, "A freshly written executable
        /// is not a cheap thing to run" — and this matrix execs one per
        /// assertion. A symlink resolves to an inode the OS has already vetted:
        /// measured at ~3ms per exec here against seconds for a new file. Do
        /// not turn it back into a script to make it print more.
        const REAL_ECHO: &str = "/bin/echo";

        /// The launch matrix. Every one of these must be installed wherever
        /// these tests run — there is no presence probe and nothing is skipped.
        ///
        /// An earlier version gated zsh and fish on the interpreter being
        /// found. fish is on neither the macOS base install nor the CI images,
        /// so its half of the matrix had never executed anywhere while still
        /// reporting as passed: nothing in the output said fish was not tried.
        /// A skipped test that reports as passed is worse than a missing one.
        /// `scripts/install-launch-shells.sh` installs and verifies all three
        /// and is the record of what the matrix requires; the CI workflow runs
        /// it, and it is the one command to run on a new machine.
        const LAUNCH_SHELLS: [&str; 3] = ["bash", "zsh", "fish"];

        /// A staging path next to `dir/name` that no other test thread uses.
        ///
        /// The process id alone is not enough: the launch tests run as threads
        /// of one process, and two of them staging the same `name` under the
        /// same pid raced — one renamed the file away while the other was
        /// still about to rename it, and the second failed with `NotFound`.
        fn staging_path(dir: &Path, name: &str) -> PathBuf {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static SEQ: AtomicUsize = AtomicUsize::new(0);
            let seq = SEQ.fetch_add(1, Ordering::Relaxed);
            dir.join(format!("{name}.{}.{seq}", std::process::id()))
        }

        /// A `PATH` prefix holding `claude` and `codex` stand-ins.
        fn agent_bin_dir() -> PathBuf {
            assert!(
                Path::new(REAL_ECHO).exists(),
                "{REAL_ECHO} is missing; the wrapper launch harness needs it"
            );
            // Under `target/`, so it is gitignored and survives between runs.
            let dir = std::env::var_os("TUIC_SHELL_TEST_BIN_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    Path::new(env!("CARGO_MANIFEST_DIR")).join("target/fake-agent-bin")
                });
            std::fs::create_dir_all(&dir).expect("create fake agent bin dir");

            for agent in ["claude", "codex", "grok", "opencode", "goose", "pi", "ego"] {
                let link = dir.join(agent);
                if std::fs::read_link(&link).is_ok_and(|target| target == Path::new(REAL_ECHO)) {
                    continue;
                }
                // Stage under a unique name and rename over the target, so
                // tests running in parallel never observe a missing link.
                let staging = staging_path(&dir, agent);
                std::os::unix::fs::symlink(REAL_ECHO, &staging).expect("stage agent symlink");
                std::fs::rename(&staging, &link).expect("install agent symlink");
            }
            dir
        }

        /// The TUIC agent shim, installed by the production installer. It is an
        /// executable, so it is written only when its content changes: the
        /// exec-time scan is paid once, not per assertion.
        fn shim_dir() -> PathBuf {
            let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/fake-agent-shim");
            install_agent_shim(&dir).expect("install agent shim");
            dir
        }

        /// One-line instruction files, so the echoed argv stays on one line.
        fn instructions_dir() -> PathBuf {
            let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/fake-agent-instructions");
            std::fs::create_dir_all(&dir).expect("create instructions dir");
            for (name, content) in [
                ("instructions-claude.md", "RULES-claude"),
                (
                    "instructions-codex.toml",
                    "developer_instructions=\"RULES-codex\"",
                ),
                ("instructions-grok.md", "RULES-grok"),
                ("instructions-goose.md", "RULES-goose"),
                ("instructions-pi.md", "RULES-pi"),
                ("instructions-ego.md", "RULES-ego"),
            ] {
                let path = dir.join(name);
                if std::fs::read_to_string(&path).ok().as_deref() != Some(content) {
                    let staging = staging_path(&dir, name);
                    std::fs::write(&staging, content).expect("write instructions");
                    std::fs::rename(&staging, &path).expect("install instructions");
                }
            }
            dir
        }

        /// Write the shell's integration constant where the shell can source it.
        ///
        /// Rewritten on every call on purpose: the file is sourced, never
        /// executed, so there is no scan to amortise, and a stale copy of the
        /// constant would make every assertion below a lie.
        fn integration_script(shell: &str) -> PathBuf {
            let (name, body) = match shell {
                "bash" => ("tuic-integration.bash", BASH_INTEGRATION),
                "zsh" => ("tuic-integration.zsh", ZSH_INTEGRATION),
                "fish" => ("tuic-integration.fish", FISH_INTEGRATION),
                other => panic!("no integration script for {other}"),
            };
            let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/shell-integration-tests");
            std::fs::create_dir_all(&dir).expect("create integration script dir");
            let path = dir.join(name);
            let staging = staging_path(&dir, name);
            std::fs::write(&staging, render_integration(body)).expect("write integration script");
            std::fs::rename(&staging, &path).expect("install integration script");
            path
        }

        /// Flags that stop `shell` from reading the machine's dotfiles.
        ///
        /// Without them the assertions below would depend on whoever runs them:
        /// zsh reads `~/.zshenv` even for `-c`, and fish reads `config.fish`, so
        /// a dotfile that greets on stderr, or that defines its own `claude`
        /// alias, would decide the result. The subject here is the integration
        /// script and nothing else.
        fn rc_free_flags(shell: &str) -> &'static [&'static str] {
            match shell {
                "bash" => &["--noprofile", "--norc"],
                "zsh" => &["-f"],
                "fish" => &["--no-config"],
                other => panic!("no rc-free flags for {other}"),
            }
        }

        /// Fail, naming `shell` and how to install it, unless it can be
        /// launched rc-free on this machine.
        ///
        /// The probe is the rc-free launch itself, not `which`, because the
        /// trap this guards against is a shell that IS installed and still
        /// cannot be used: `rc_free_flags` passes `--no-config` to fish, which
        /// only exists in fish >= 3.3. Under the old presence gate an older
        /// fish failed that probe, dropped out of the matrix, and produced a
        /// run log byte-identical to a green one.
        fn require_shell(shell: &str) {
            assert!(
                LAUNCH_SHELLS.contains(&shell),
                "{shell} is not in the launch matrix"
            );
            let probe = Command::new(shell)
                .args(rc_free_flags(shell))
                .arg("-c")
                .arg("exit 0")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            assert!(
                probe.is_ok_and(|status| status.success()),
                "{shell} cannot be launched with {:?}, so the launch matrix is \
                 incomplete. Install it (macOS: `brew install {shell}`; \
                 Debian/Ubuntu: `sudo apt-get install -y {shell}`), or run \
                 `scripts/install-launch-shells.sh`. fish must be >= 3.3, which \
                 is when `--no-config` was added.",
                rc_free_flags(shell)
            );
        }

        /// Source the integration script in `shell`, run `invocation`, and
        /// return one entry per command line the wrappers handed to an agent.
        ///
        /// Every invocation must parse in bash, zsh **and** fish — that is what
        /// lets one case description drive the whole matrix.
        fn wrapper_command_lines(
            shell: &str,
            tuic_env: &[(&str, &str)],
            invocation: &str,
        ) -> Vec<String> {
            let script = integration_script(shell);
            let bin_dir = agent_bin_dir();
            // The shim is always first, as `inject` puts it: with no instructions
            // dir it passes every argv through, which the older cases pin.
            let shim = shim_dir();
            let path = match std::env::var_os("PATH") {
                Some(existing) => format!(
                    "{}:{}:{}",
                    shim.display(),
                    bin_dir.display(),
                    existing.to_string_lossy()
                ),
                None => format!("{}:{}", shim.display(), bin_dir.display()),
            };

            let mut cmd = Command::new(shell);
            cmd.args(rc_free_flags(shell))
                .arg("-c")
                .arg(format!("source '{}'\n{invocation}", script.display()))
                .env("PATH", path)
                // The wrappers are defined only inside a TUIC session.
                .env("TUIC_SESSION", "wrapper-launch-test")
                .env(
                    "TUIC_CLAUDE_HELP",
                    include_str!("../tests/fixtures/agent-help/claude-2026-10-04.txt"),
                )
                // Start from setting-off, so a case that wants injection has to
                // ask for it and the off case cannot pass on an inherited value.
                .env_remove("TUIC_CLAUDE_SETTINGS")
                .env_remove("TUIC_CODEX_NOTIFY")
                .env_remove("TUIC_CODEX_SCREEN_FLAG")
                .env_remove("TUIC_GROK_SCREEN_FLAG")
                .env_remove("TUIC_OPENCODE_SCREEN_FLAG")
                .env_remove("TUIC_AGENT_INSTRUCTIONS_DIR")
                .env_remove("TUIC_ATTACHMENTS_DIR")
                .env_remove("GEMINI_CLI_TRUSTED_FOLDERS_PATH")
                .env_remove("TUIC_AGENT_SHIM_DIR");
            for (key, value) in tuic_env {
                cmd.env(key, value);
            }

            let out = cmd
                .output()
                .unwrap_or_else(|err| panic!("launch {shell}: {err}"));
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            assert!(
                out.status.success(),
                "{shell} exited {:?}: {stderr}",
                out.status.code()
            );
            // A syntax error in the integration script can still leave the exit
            // status at zero; the empty stderr is what rules that out.
            assert!(stderr.is_empty(), "{shell} wrote to stderr: {stderr}");

            String::from_utf8(out.stdout)
                .expect("shell output is utf-8")
                .lines()
                .map(str::to_owned)
                .collect()
        }

        /// Run one case in one shell and compare against `expected`.
        fn assert_in_shell(
            shell: &str,
            case: &str,
            tuic_env: &[(&str, &str)],
            invocation: &str,
            expected: &[&str],
        ) {
            require_shell(shell);
            let actual = wrapper_command_lines(shell, tuic_env, invocation);
            assert_eq!(actual, expected, "{shell}: {case}");
        }

        /// Emit one `#[test]` per case per shell, so the shell that ran is in
        /// the test name and the run log names every shell that was launched.
        ///
        /// One test per case looping over the matrix would hide the shell: a
        /// log line reading `setting_on_prepends_launch_scoped_status_flags`
        /// says nothing about whether fish was among the shells it tried,
        /// which is exactly how the fish half stayed unexecuted.
        macro_rules! launch_matrix {
            ($($case:ident),+ $(,)?) => {
                $(mod $case {
                    #[test]
                    fn in_bash() { super::$case("bash") }
                    #[test]
                    fn in_zsh() { super::$case("zsh") }
                    #[test]
                    fn in_fish() { super::$case("fish") }
                })+
            };
        }

        // Catches: a nonempty cache without command rows prefixes auth with prompt settings.
        fn corrupt_cached_help_preserves_auth_subcommand(shell: &str) {
            require_shell(shell);
            let recorded = crate::agent_hook_launch::RECORDED_CLAUDE_HELP;
            let truncated = recorded.split("Commands:").next().expect("help prefix");
            let actual: Vec<String> = [" \n\t", truncated]
                .into_iter()
                .flat_map(|help| {
                    wrapper_command_lines(
                        shell,
                        &[
                            ("TUIC_CLAUDE_SETTINGS", "/tuic/claude.json"),
                            ("TUIC_CLAUDE_HELP", help),
                        ],
                        "claude auth status",
                    )
                })
                .collect();
            assert_eq!(
                actual,
                ["auth status", "auth status"],
                "{shell}: whitespace and truncated help must retain recorded auth argv"
            );
        }

        launch_matrix!(corrupt_cached_help_preserves_auth_subcommand);

        launch_matrix!(
            setting_on_prepends_launch_scoped_status_flags,
            an_explicit_user_flag_suppresses_injection,
            setting_off_leaves_the_command_line_untouched,
            screen_flags_follow_manual_agent_commands,
            hyphenated_prompts_keep_settings_in_every_shell,
            unavailable_help_keeps_verbs_and_prompts_without_launch_probe,
            unavailable_help_preserves_recorded_auth_subcommand,
        );

        launch_matrix!(
            typed_launches_carry_the_instructions,
            an_alias_reaches_the_wrapper_and_the_shim,
            a_wrapper_script_by_name_reaches_the_shim,
            an_absolute_path_bypasses_the_shim,
            nested_shim_dirs_reach_the_real_binary,
            the_prompt_puts_the_shim_back_in_front,
        );

        /// TUIC shell environment with the status signals and the instructions on.
        fn instructions_on() -> Vec<(&'static str, String)> {
            let dir = instructions_dir();
            vec![
                ("TUIC_CLAUDE_SETTINGS", CLAUDE_SETTINGS.to_string()),
                ("TUIC_CODEX_NOTIFY", CODEX_NOTIFY.to_string()),
                ("TUIC_AGENT_INSTRUCTIONS_DIR", dir.display().to_string()),
                ("TUIC_AGENT_SHIM_DIR", shim_dir().display().to_string()),
            ]
        }

        fn run_with_instructions(shell: &str, invocation: &str) -> Vec<String> {
            require_shell(shell);
            let env = instructions_on();
            let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
            wrapper_command_lines(shell, &env, invocation)
        }

        fn instruction_file(agent: &str) -> String {
            let ext = if agent == "codex" { "toml" } else { "md" };
            instructions_dir()
                .join(format!("instructions-{agent}.{ext}"))
                .display()
                .to_string()
        }

        // Catches: disabling marker instructions also drops Claude's attachment grant.
        #[test]
        fn claude_attachment_grant_survives_disabled_launch_instructions() {
            let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
            let agents = crate::config::AgentsConfig {
                launch_instructions: Some(String::new()),
                ..Default::default()
            };
            crate::launch_instructions::write_files_with(
                root.path(),
                &crate::config::AppConfig::default(),
                &agents,
            )
            .unwrap();
            let instructions = root.path().join("agent-hooks").display().to_string();
            let lines = wrapper_command_lines(
                "bash",
                &[
                    ("TUIC_AGENT_INSTRUCTIONS_DIR", instructions.as_str()),
                    ("TUIC_ATTACHMENTS_DIR", "/tuic/attachments/pty-settings-off"),
                ],
                "claude --model opus",
            );
            assert!(
                lines[0]
                    .split_whitespace()
                    .any(|arg| arg == "--add-dir=/tuic/attachments/pty-settings-off"),
                "disabling marker instructions removed attachment access: {lines:?}"
            );
        }

        // Catches: typed ego launches lose the independent read-only attachment grant.
        #[test]
        fn ego_attachment_grant_survives_disabled_launch_instructions() {
            for invocation in [
                "ego --sandbox workspace",
                "ego run task",
                "ego resume session",
            ] {
                let lines = wrapper_command_lines(
                    "bash",
                    &[("TUIC_ATTACHMENTS_DIR", "/tuic/attachments/pty-ego")],
                    invocation,
                );
                assert!(
                    lines[0]
                        .split_whitespace()
                        .any(|arg| arg == "--read-only-dir=/tuic/attachments/pty-ego"),
                    "{lines:?}"
                );
            }
        }

        /// Catches: a typed agent launched without the marker flag (fresh or
        /// resume), the flag on a headless or verb launch, or a second flag
        /// when the user already passed one.
        fn typed_launches_carry_the_instructions(shell: &str) {
            let claude = instruction_file("claude");
            let pi = instruction_file("pi");
            let ego = format!("@{}", instruction_file("ego"));
            let actual = run_with_instructions(
                shell,
                "claude --model opus\n\
                 claude --resume x\n\
                 claude -p hi\n\
                 claude mcp list\n\
                 claude --append-system-prompt-file /mine.md\n\
                 codex --model o3\n\
                 codex resume --last\n\
                 codex exec task\n\
                 grok --continue\n\
                 grok -p hi\n\
                 goose session\n\
                 goose session --resume\n\
                 goose run -t task\n\
                 pi --continue\n\
                 pi -p hi\n\
                 ego\n\
                 ego resume id\n\
                 ego run task",
            );
            let notify = format!("-c notify=[\"{CODEX_NOTIFY}\"]");
            let expected = [
                format!(
                    "--append-system-prompt-file {claude} --settings {CLAUDE_SETTINGS} --model opus"
                ),
                format!(
                    "--append-system-prompt-file {claude} --settings {CLAUDE_SETTINGS} --resume x"
                ),
                format!("--settings {CLAUDE_SETTINGS} -p hi"),
                "mcp list".to_string(),
                format!("--settings {CLAUDE_SETTINGS} --append-system-prompt-file /mine.md"),
                format!("-c developer_instructions=\"RULES-codex\" --model o3 {notify}"),
                format!("-c developer_instructions=\"RULES-codex\" resume --last {notify}"),
                format!("exec task {notify}"),
                "--rules=RULES-grok --continue".to_string(),
                "-p hi".to_string(),
                "session --system=RULES-goose --name wrapper-launch-test".to_string(),
                "session --system=RULES-goose --resume".to_string(),
                "run --name wrapper-launch-test -t task".to_string(),
                format!("--append-system-prompt {pi} --continue"),
                "-p hi".to_string(),
                format!("--append-system {ego}"),
                format!("resume --append-system {ego} id"),
                "run task".to_string(),
            ];
            assert_eq!(actual, expected, "{shell}: typed launches");
        }

        /// Catches: an alias that resolves past the TUIC wrapper and shim.
        fn an_alias_reaches_the_wrapper_and_the_shim(shell: &str) {
            // zsh parses a whole -c string before it runs it, so the alias is
            // used through eval; bash needs alias expansion turned on outside
            // an interactive shell.
            let invocation = match shell {
                "bash" => "shopt -s expand_aliases\nalias c=claude\neval 'c --model opus'",
                _ => "alias c=claude\neval 'c --model opus'",
            };
            let claude = instruction_file("claude");
            assert_eq!(
                run_with_instructions(shell, invocation),
                [format!(
                    "--append-system-prompt-file {claude} --settings {CLAUDE_SETTINGS} --model opus"
                )],
                "{shell}: alias"
            );
        }

        /// Catches: a wrapper script (c2) that runs the agent by name launching
        /// without the instructions. The status hooks stay with the typed
        /// command on purpose, so the child gets the instructions only.
        fn a_wrapper_script_by_name_reaches_the_shim(shell: &str) {
            let claude = instruction_file("claude");
            assert_eq!(
                run_with_instructions(shell, "/bin/sh -c 'exec claude \"$@\"' c2 --model opus"),
                [format!("--append-system-prompt-file {claude} --model opus")],
                "{shell}: script wrapper"
            );
        }

        /// Catches: a claim that absolute-path launches carry the instructions.
        /// The shim cannot see them; this pins the documented miss.
        fn an_absolute_path_bypasses_the_shim(shell: &str) {
            let real = agent_bin_dir().join("claude");
            assert_eq!(
                run_with_instructions(shell, &format!("'{}' --model opus", real.display())),
                ["--model opus"],
                "{shell}: absolute path"
            );
        }

        /// Catches: a shim that execs another TUIC shim (a nested instance's
        /// dir) and loops instead of reaching the real binary.
        fn nested_shim_dirs_reach_the_real_binary(shell: &str) {
            let outer = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/fake-agent-shim-nested");
            install_agent_shim(&outer).expect("install nested shim");
            let claude = instruction_file("claude");
            assert_eq!(
                run_with_instructions(
                    shell,
                    &format!(
                        "/bin/sh -c 'PATH=\"$0:$PATH\" exec claude --model opus' '{}'",
                        outer.display()
                    ),
                ),
                [format!("--append-system-prompt-file {claude} --model opus")],
                "{shell}: nested shim dirs"
            );
        }

        /// Catches: a user rc that prepends the agent's own dir after TUIC set
        /// PATH, so the real binary shadows the shim for the whole session.
        /// The `true` absorbs the OSC 133;C that bash's DEBUG trap prints for
        /// the first command after a prompt.
        fn the_prompt_puts_the_shim_back_in_front(shell: &str) {
            let real = agent_bin_dir();
            let invocation = match shell {
                "fish" => format!(
                    "set -gx PATH '{}' $PATH\n__tuic_prompt >/dev/null\n/bin/sh -c 'exec claude --model opus'",
                    real.display()
                ),
                _ => format!(
                    "PATH='{}':\"$PATH\"\n{{ __tuic_precmd; true; }} >/dev/null\n/bin/sh -c 'exec claude --model opus'",
                    real.display()
                ),
            };
            let claude = instruction_file("claude");
            assert_eq!(
                run_with_instructions(shell, &invocation),
                [format!("--append-system-prompt-file {claude} --model opus")],
                "{shell}: prompt re-prepends the shim"
            );
        }

        fn screen_flags_follow_manual_agent_commands(shell: &str) {
            let flags = [
                ("TUIC_CODEX_SCREEN_FLAG", "--no-alt-screen"),
                ("TUIC_CODEX_SCREEN_SKIP", "exec"),
                ("TUIC_GROK_SCREEN_FLAG", "--no-alt-screen"),
            ];
            assert_in_shell(
                shell,
                "manual agents use the exported screen policy without changing explicit choices",
                &flags,
                "codex --model o3\n\
                 codex exec --full-auto\n\
                 codex --no-alt-screen --model o3\n\
                 grok --model fast\n\
                 grok --fullscreen --model fast\n\
                 command codex --model o3",
                &[
                    "--no-alt-screen --model o3",
                    "exec --full-auto",
                    "--no-alt-screen --model o3",
                    "--no-alt-screen --model fast",
                    "--no-alt-screen --fullscreen --model fast",
                    "--model o3",
                ],
            );
        }

        // Catches: --no-session is combined with TUIC's conflicting --name.
        fn goose_no_session_preserves_stateless_argv(shell: &str) {
            assert_in_shell(
                shell,
                "stateless Goose must not receive a session name",
                &[],
                "goose session --no-session\ngoose run --no-session -t task",
                &["session --no-session", "run --no-session -t task"],
            );
        }

        // Catches: attached names or legacy paths receive a duplicate/conflicting --name.
        fn goose_explicit_name_or_path_preserves_argv(shell: &str) {
            assert_in_shell(
                shell,
                "Goose explicit names and paths must not receive TUIC naming",
                &[],
                "goose run --name=work -t hi\n\
                 goose session --name=work\n\
                 goose run -nwork -t hi\n\
                 goose session -nwork\n\
                 goose run --path f.jsonl -t hi\n\
                 goose session --path f.jsonl\n\
                 goose run -t hi --path=f.jsonl\n\
                 goose session --path=f.jsonl",
                &[
                    "run --name=work -t hi",
                    "session --name=work",
                    "run -nwork -t hi",
                    "session -nwork",
                    "run --path f.jsonl -t hi",
                    "session --path f.jsonl",
                    "run -t hi --path=f.jsonl",
                    "session --path=f.jsonl",
                ],
            );
        }

        // Catches: suppressing stateless naming also removes normal tab/session binding.
        fn goose_session_keeps_naming_and_explicit_resume(shell: &str) {
            assert_in_shell(
                shell,
                "normal Goose naming and user choices remain authoritative",
                &[],
                "goose session\ngoose run -t task\ngoose session --name user\ngoose session --resume",
                &[
                    "session --name wrapper-launch-test",
                    "run --name wrapper-launch-test -t task",
                    "session --name user",
                    "session --resume",
                ],
            );
        }

        // Catches: OpenCode admin verbs receive a TUI-only flag, including after root options.
        fn opencode_admin_verbs_skip_screen_flag(shell: &str) {
            require_shell(shell);
            let help = include_str!("../tests/fixtures/agent-help/opencode-2026-10-14.txt");
            let mut verbs: Vec<&str> = help
                .lines()
                .filter_map(|line| {
                    let verb = line
                        .strip_prefix("  opencode ")?
                        .split_whitespace()
                        .next()?;
                    (verb != "[project]").then_some(verb)
                })
                .collect();
            verbs.extend(["auth", "plug"]);
            assert!(
                !verbs.is_empty(),
                "recorded OpenCode help must contain commands"
            );
            for verb in verbs {
                let invocation = format!("opencode {verb}\nopencode --print-logs {verb}");
                let expected = [verb.to_owned(), format!("--print-logs {verb}")];
                assert_in_shell(
                    shell,
                    "admin commands bypass screen injection",
                    &[("TUIC_OPENCODE_SCREEN_FLAG", "--mini")],
                    &invocation,
                    &[&expected[0], &expected[1]],
                );
            }
            assert_in_shell(
                shell,
                "MCP arguments and run arguments stay intact",
                &[("TUIC_OPENCODE_SCREEN_FLAG", "--mini")],
                "opencode mcp list\nopencode run task",
                &["mcp list", "run task"],
            );
        }

        // Catches: filtering verbs mistakes a project path or option value for an admin command.
        fn opencode_interactive_launch_keeps_screen_flag(shell: &str) {
            assert_in_shell(
                shell,
                "only interactive launches gain the screen flag",
                &[("TUIC_OPENCODE_SCREEN_FLAG", "--mini")],
                "opencode\nopencode ./mcp\nopencode project\nopencode --model mcp\nopencode --model=mcp\nopencode --mini\nopencode -- ./mcp",
                &[
                    "--mini",
                    "--mini ./mcp",
                    "--mini project",
                    "--mini --model mcp",
                    "--mini --model=mcp",
                    "--mini",
                    "--mini -- ./mcp",
                ],
            );
        }

        launch_matrix!(
            goose_no_session_preserves_stateless_argv,
            goose_explicit_name_or_path_preserves_argv,
            goose_session_keeps_naming_and_explicit_resume,
            opencode_admin_verbs_skip_screen_flag,
            opencode_interactive_launch_keeps_screen_flag,
        );

        const CLAUDE_SETTINGS: &str = "/tuic/agent-hooks/claude.json";
        const CODEX_NOTIFY: &str = "/tuic/agent-hooks/codex-notify.sh";

        fn signals_on() -> [(&'static str, &'static str); 2] {
            [
                ("TUIC_CLAUDE_SETTINGS", CLAUDE_SETTINGS),
                ("TUIC_CODEX_NOTIFY", CODEX_NOTIFY),
            ]
        }

        fn setting_on_prepends_launch_scoped_status_flags(shell: &str) {
            // Catches: root settings injected into a subcommand instead of a prompt.
            let claude = format!("--settings {CLAUDE_SETTINGS} --model opus");
            let prompt = format!("--settings {CLAUDE_SETTINGS} prompt");
            let resume = format!("--settings {CLAUDE_SETTINGS} --resume x");
            assert_in_shell(
                shell,
                "Claude gets the TUIC settings file before its own arguments",
                &signals_on(),
                "claude --model opus\n\
                 claude doctor\n\
                 claude remote-control --resume x\n\
                 claude prompt\n\
                 claude --resume x\n\
                 claude plugins",
                &[
                    claude.as_str(),
                    "doctor",
                    "remote-control --resume x",
                    prompt.as_str(),
                    resume.as_str(),
                    "plugins",
                ],
            );
            let codex = format!("exec --full-auto -c notify=[\"{CODEX_NOTIFY}\"]");
            assert_in_shell(
                shell,
                "Codex gets the TUIC notify script appended",
                &signals_on(),
                "codex exec --full-auto",
                &[codex.as_str()],
            );
        }

        /// Catches: hyphenated prompt text is mistaken for a hidden command.
        fn hyphenated_prompts_keep_settings_in_every_shell(shell: &str) {
            let short = format!("--settings {CLAUDE_SETTINGS} fix-bug");
            let sentence =
                format!("--settings {CLAUDE_SETTINGS} Explain the remote-control failure");
            assert_in_shell(
                shell,
                "ordinary prompts keep status hooks",
                &signals_on(),
                "claude fix-bug\nclaude 'Explain the remote-control failure'",
                &[short.as_str(), sentence.as_str()],
            );
        }

        /// Catches: missing cached help runs an extra CLI or injects settings into a known verb.
        fn unavailable_help_keeps_verbs_and_prompts_without_launch_probe(shell: &str) {
            let prompt = format!("--settings {CLAUDE_SETTINGS} fix-bug");
            assert_in_shell(
                shell,
                "unavailable help uses recorded verbs without a shell probe",
                &[
                    ("TUIC_CLAUDE_SETTINGS", CLAUDE_SETTINGS),
                    ("TUIC_CLAUDE_HELP", ""),
                ],
                "claude doctor\nclaude mcp list\nclaude plugins\nclaude upgrade\nclaude fix-bug",
                &["doctor", "mcp list", "plugins", "upgrade", prompt.as_str()],
            );
        }

        /// Catches: the shell fallback injects settings into recorded auth when help is absent.
        fn unavailable_help_preserves_recorded_auth_subcommand(shell: &str) {
            assert_in_shell(
                shell,
                "recorded auth is still a subcommand without cached help",
                &[
                    ("TUIC_CLAUDE_SETTINGS", CLAUDE_SETTINGS),
                    ("TUIC_CLAUDE_HELP", ""),
                ],
                "claude auth status",
                &["auth status"],
            );
        }

        fn an_explicit_user_flag_suppresses_injection(shell: &str) {
            assert_in_shell(
                shell,
                "Claude leaves the user's own --settings alone",
                &signals_on(),
                "claude --settings /user/settings.json\n\
                 claude --settings=/user/settings.json\n\
                 claude --bare\n\
                 claude --model opus --settings /user/settings.json",
                &[
                    "--settings /user/settings.json",
                    "--settings=/user/settings.json",
                    "--bare",
                    // The flag is not the first argument. This is the case a
                    // grep over the script text cannot see.
                    "--model opus --settings /user/settings.json",
                ],
            );
            assert_in_shell(
                shell,
                "Codex leaves the user's own notify override alone",
                &signals_on(),
                "codex -c 'notify=[\"/user/notify.sh\"]' exec\n\
                 codex -cnotify='[\"/user/notify.sh\"]' exec\n\
                 codex '--config=notify=[\"/user/notify.sh\"]' exec",
                &[
                    "-c notify=[\"/user/notify.sh\"] exec",
                    "-cnotify=[\"/user/notify.sh\"] exec",
                    "--config=notify=[\"/user/notify.sh\"] exec",
                ],
            );
        }

        fn setting_off_leaves_the_command_line_untouched(shell: &str) {
            assert_in_shell(
                shell,
                "Claude is launched exactly as typed",
                &[],
                "claude --model opus --dangerously-skip-permissions",
                &["--model opus --dangerously-skip-permissions"],
            );
            assert_in_shell(
                shell,
                "Codex is launched exactly as typed",
                &[],
                "codex exec --full-auto",
                &["exec --full-auto"],
            );
            assert_in_shell(
                shell,
                "Unsupported or disabled screen policies leave agent arguments unchanged",
                &[],
                "codex --model o3\n\
                 grok --model fast\n\
                 opencode --model fast",
                &["--model o3", "--model fast", "--model fast"],
            );
        }
    }
}
