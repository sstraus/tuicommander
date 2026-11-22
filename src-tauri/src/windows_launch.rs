//! Windows launch helpers that stay host-independent so macOS CI can test them.
//!
//! An npm-installed agent is a `.cmd` shim. `CreateProcessW` runs a `.cmd`
//! through `cmd.exe /c`, and `cmd` ends the command line at the first newline:
//! a multi-line prompt then loses every later argument. The launch therefore
//! resolves the shim to the real executable it forwards to.

use std::path::{Path, PathBuf};

/// Extract the `.exe` an npm `cmd-shim` forwards to, e.g.
/// `"%dp0%\node_modules\pkg\bin\tool.exe" %*`. A shim that runs a script
/// through `node` has no `.exe` target and yields `None`.
pub(crate) fn shim_exe_target(shim_text: &str, shim_dir: &Path) -> Option<PathBuf> {
    // Quoted tokens sit at odd indexes when splitting on `"`.
    shim_text.split('"').skip(1).step_by(2).find_map(|token| {
        let rest = token
            .strip_prefix("%dp0%")
            .or_else(|| token.strip_prefix("%~dp0"))?;
        let rest = rest.trim_start_matches(['\\', '/']);
        let name = rest.rsplit(['\\', '/']).next()?;
        // `node.exe` next to the shim is the interpreter probe, not the agent.
        if !name.to_ascii_lowercase().ends_with(".exe") || name.eq_ignore_ascii_case("node.exe") {
            return None;
        }
        Some(
            rest.split(['\\', '/'])
                .fold(shim_dir.to_path_buf(), |dir, part| dir.join(part)),
        )
    })
}

/// Return the program to execute for `path`: the real `.exe` behind a `.cmd`
/// or `.bat` shim when it can be resolved, otherwise `path` unchanged.
pub(crate) fn resolve_launch_program(path: &str) -> String {
    let given = Path::new(path);
    let is_shim = given
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat"));
    if !is_shim {
        return path.to_string();
    }
    let target = given
        .parent()
        .zip(std::fs::read_to_string(given).ok())
        .and_then(|(dir, text)| shim_exe_target(&text, dir))
        .filter(|target| target.is_file());
    match target {
        Some(target) => target.to_string_lossy().into_owned(),
        None => path.to_string(),
    }
}

/// Split a configured shell string into the executable and its arguments.
/// The executable may be quoted or may be an unquoted path with spaces such
/// as `C:\Program Files\Git\bin\bash.exe -l`.
pub(crate) fn split_shell_command(shell: &str) -> Vec<String> {
    let shell = shell.trim();
    let (exe, rest) = if let Some(quoted) = shell.strip_prefix('"') {
        match quoted.split_once('"') {
            Some((exe, rest)) => (exe, rest),
            None => (quoted, ""),
        }
    } else {
        let lower = shell.to_ascii_lowercase();
        let end = [".exe", ".cmd", ".bat", ".com"]
            .iter()
            .flat_map(|ext| lower.match_indices(ext))
            .map(|(at, ext)| at + ext.len())
            .filter(|end| lower[*end..].chars().next().is_none_or(char::is_whitespace))
            .min();
        match end {
            Some(end) => shell.split_at(end),
            None => shell.split_at(shell.find(char::is_whitespace).unwrap_or(shell.len())),
        }
    };
    let mut parts = vec![exe.to_string()];
    parts.extend(rest.split_whitespace().map(str::to_string));
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    // Byte-exact output of npm's bundled cmd-shim 8.0.0 for a global install
    // (`<prefix>/claude.cmd` -> `node_modules/@anthropic-ai/claude-code/bin/claude.exe`).
    const NPM_SHIM: &str = "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n\"%dp0%\\node_modules\\@anthropic-ai\\claude-code\\bin\\claude.exe\"   %*\r\n";
    // cmd-shim 8.0.0 output for a `#!/usr/bin/env node` script target.
    const NODE_SHIM: &str = "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n\r\nIF EXIST \"%dp0%\\node.exe\" (\r\n  SET \"_prog=%dp0%\\node.exe\"\r\n) ELSE (\r\n  SET \"_prog=node\"\r\n  SET PATHEXT=%PATHEXT:;.JS;=;%\r\n)\r\n\r\nendLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\"  \"%dp0%\\node_modules\\pkg\\cli.js\" %*\r\n";

    #[test]
    fn shim_target_is_the_forwarded_exe_under_the_shim_dir() {
        let target = shim_exe_target(NPM_SHIM, Path::new("/npm")).unwrap();
        assert!(target.starts_with("/npm"), "{target:?}");
        assert!(
            target
                .to_string_lossy()
                .replace('\\', "/")
                .ends_with("node_modules/@anthropic-ai/claude-code/bin/claude.exe"),
            "{target:?}"
        );
    }

    #[test]
    fn node_script_shim_has_no_exe_target() {
        // The `node.exe` in the `IF EXIST` probe is the interpreter, not the agent.
        assert_eq!(shim_exe_target(NODE_SHIM, Path::new("/npm")), None);
    }

    #[test]
    fn resolve_returns_the_real_exe_when_it_exists_and_the_shim_otherwise() {
        let dir = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
        let shim = dir.path().join("claude.cmd");
        std::fs::write(&shim, NPM_SHIM).unwrap();
        let shim = shim.to_string_lossy().to_string();
        // Real exe missing: keep the shim (nothing better to launch).
        assert_eq!(resolve_launch_program(&shim), shim);
        let exe = dir
            .path()
            .join(r"node_modules/@anthropic-ai/claude-code/bin/claude.exe");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"").unwrap();
        let resolved = resolve_launch_program(&shim);
        assert!(resolved.ends_with("claude.exe"), "{resolved}");
        assert!(Path::new(&resolved).is_file(), "{resolved}");
    }

    #[test]
    fn resolve_leaves_a_plain_executable_alone() {
        assert_eq!(resolve_launch_program("/usr/bin/true"), "/usr/bin/true");
    }

    #[test]
    fn split_keeps_an_unquoted_exe_path_with_spaces_whole() {
        assert_eq!(
            split_shell_command(r"C:\Program Files\Git\bin\bash.exe"),
            [r"C:\Program Files\Git\bin\bash.exe"]
        );
        assert_eq!(
            split_shell_command(r"C:\Program Files\Git\bin\bash.exe --login -i"),
            [r"C:\Program Files\Git\bin\bash.exe", "--login", "-i"]
        );
    }

    #[test]
    fn split_honours_quotes_and_plain_commands() {
        assert_eq!(
            split_shell_command(r#""C:\Program Files\Git\bin\bash.exe" -l"#),
            [r"C:\Program Files\Git\bin\bash.exe", "-l"]
        );
        assert_eq!(
            split_shell_command("wsl.exe -d Ubuntu"),
            ["wsl.exe", "-d", "Ubuntu"]
        );
        assert_eq!(split_shell_command("/bin/zsh"), ["/bin/zsh"]);
        assert_eq!(split_shell_command("pwsh"), ["pwsh"]);
    }
}
