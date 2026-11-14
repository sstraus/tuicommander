//! Native Windows hook commands (#2632-97ef): `tuic.exe agent-state` over the
//! named pipe. Exercised on every host through an explicit transport.

use super::*;

fn every_map(t: &HookTransport) -> Vec<HookEntry> {
    [
        claude_hook_map_on(t),
        gemini_hook_map_on(t),
        grok_hook_map_on(t),
        codex_hook_map_on(t),
    ]
    .concat()
}

// Catches: a Windows hook still carries POSIX `sh` (ps, base64, curl), which
// native Windows cannot run, or loses the ownership sentinel the installer
// prunes by.
#[test]
fn windows_hooks_call_the_cli_and_keep_the_sentinel() {
    let cli = r"C:\Program Files\TUICommander\tuic.exe";
    let t = HookTransport::windows_cli(std::path::Path::new(cli)).unwrap();
    for (event, _, command) in every_map(&t) {
        assert!(
            command.starts_with(&format!("\"{cli}\" agent-state ")),
            "{event}: {command}"
        );
        assert!(command.ends_with(&format!("\"{SENTINEL}\"")), "{event}");
        for posix in ["$(", "printf", "curl", "ps -o", "base64"] {
            assert!(!command.contains(posix), "{event} kept {posix}: {command}");
        }
    }
}

// Catches: a path that `cmd` (`%VAR%`, `!`) or `sh` (`$`, backtick) would
// expand inside double quotes, or that holds a quote, is generated anyway and
// runs a different program.
#[test]
fn windows_cli_path_that_a_shell_expands_is_refused() {
    for bad in [
        r"C:\a%PATH%\tuic.exe",
        r"C:\a!x\tuic.exe",
        r"C:\$x\tuic.exe",
        r"C:\`x\tuic.exe",
        "C:\\a\"b\\tuic.exe",
    ] {
        assert_eq!(
            HookTransport::windows_cli(std::path::Path::new(bad)),
            None,
            "{bad}"
        );
    }
    assert!(HookTransport::windows_cli(std::path::Path::new(r"C:\Users\Zoë Ü\tuic.exe")).is_some());
}

// Catches: a CLI path with spaces or non-ASCII letters splits into several
// words, or the event verb and state reach the CLI in the wrong order, when a
// shell runs the generated command. Proven with `sh`, one of the shells a
// Windows agent may use; `cmd` and PowerShell need the Windows lab.
#[cfg(unix)]
#[test]
fn windows_hook_reaches_a_cli_path_with_spaces_and_unicode_intact() {
    let dir = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    let bin = dir.path().join("Program Files é");
    std::fs::create_dir_all(&bin).unwrap();
    let cli = bin.join("tuic.exe");
    let argv = dir.path().join("argv");
    std::fs::write(
        &cli,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > '{}'\n",
            argv.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
    let t = HookTransport::windows_cli(&cli).unwrap();
    for (command, expected) in [
        (state_hook(&t, "idle"), vec!["agent-state", "state", "idle"]),
        (
            claude_hook(&t, Some("busy")),
            vec!["agent-state", "claude", "busy"],
        ),
        (claude_hook(&t, None), vec!["agent-state", "claude"]),
        (
            codex_hook_map_on(&t).remove(1).2,
            vec!["agent-state", "codex-session", "prompt"],
        ),
    ] {
        let status = std::process::Command::new("sh")
            .args(["-c", &command])
            .status()
            .unwrap();
        assert!(status.success(), "{command}");
        let got = std::fs::read_to_string(&argv).unwrap();
        let mut want = expected.join("\n");
        want.push('\n');
        want.push_str(SENTINEL);
        want.push('\n');
        assert_eq!(got, want, "{command}");
    }
}
