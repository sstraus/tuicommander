//! Platform-neutral helpers shared by Rust tests.

fn checkout_root() -> std::path::PathBuf {
    // A cached test-support crate may have been compiled in another worktree.
    // Find the checkout running this test, not its build path.
    std::env::current_dir()
        .ok()
        .and_then(|cwd| {
            cwd.ancestors()
                .find(|dir| dir.join(".git").exists() && dir.join("src-tauri/Cargo.toml").is_file())
                .map(std::path::Path::to_path_buf)
        })
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."))
}

// Cargo's libtest runner has no per-binary setup hook. This crate is linked
// only into test binaries, so install their temp root before the harness starts.
#[ctor::ctor]
fn install_test_temp_root() {
    let root = test_temp_root();
    // Freeze the checkout selected at process start: tests may change cwd.
    // SAFETY: this constructor runs before libtest creates worker threads.
    unsafe { std::env::set_var("TUIC_TEST_TMP_ROOT", &root) };
    for key in ["TMPDIR", "TMP", "TEMP"] {
        // SAFETY: a process constructor runs before main and before libtest
        // creates its worker threads. No test can read the environment yet.
        unsafe { std::env::set_var(key, &root) };
    }
    isolate_git_config(&root);
}

/// The only git configuration a test process sees besides each fixture
/// repo's own `.git/config`.
const TEST_GITCONFIG: &str = "\
# Written by tuic-test-support: tests never read the developer's git config.
[user]
\tname = TUIC Test
\temail = tests@tuic.invalid
[init]
\tdefaultBranch = main
[commit]
\tgpgSign = false
[tag]
\tgpgSign = false
[core]
\tautocrlf = false
";

#[derive(Debug)]
struct OwnedGitConfig {
    owner: u32,
    path: std::path::PathBuf,
    #[cfg(windows)]
    wide_path: Vec<u16>,
}

static OWNED_GIT_CONFIG: std::sync::OnceLock<OwnedGitConfig> = std::sync::OnceLock::new();

// CRT normal exit runs this finalizer. Windows ExitProcess is covered by the
// callback below. Abort/SIGKILL can still leave the owned file behind.
#[ctor::dtor]
fn remove_test_git_config() {
    if let Some(config) = OWNED_GIT_CONFIG.get()
        && config.owner == std::process::id()
    {
        // Tests may replace GIT_CONFIG_GLOBAL or change cwd. Never derive the
        // cleanup target from their environment, or remove a fork parent's file.
        let _ = std::fs::remove_file(&config.path);
    }
}

// Windows std::process::exit calls ExitProcess, not the CRT exit path used by
// ctor::dtor. A process-detach TLS callback covers that path without trusting a
// test-modified environment or a surviving worker thread.
#[cfg(windows)]
#[used]
#[unsafe(link_section = ".CRT$XLB")]
static GIT_CONFIG_PROCESS_DETACH: unsafe extern "system" fn(
    *mut std::ffi::c_void,
    u32,
    *mut std::ffi::c_void,
) = git_config_process_detach;

#[cfg(windows)]
unsafe extern "system" fn git_config_process_detach(
    _module: *mut std::ffi::c_void,
    reason: u32,
    _reserved: *mut std::ffi::c_void,
) {
    const DLL_PROCESS_DETACH: u32 = 0;
    if reason == DLL_PROCESS_DETACH
        && let Some(config) = OWNED_GIT_CONFIG.get()
        && config.owner == std::process::id()
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            #[link_name = "DeleteFileW"]
            fn delete_file_w(path: *const u16) -> i32;
        }
        // SAFETY: the constructor retained this NUL-terminated absolute path.
        // Do not allocate or acquire Rust locks under the loader's detach lock.
        unsafe { delete_file_w(config.wide_path.as_ptr()) };
    }
}

/// Point every git process this test binary starts — the fixtures' own `git`
/// calls and the production code under test alike — at [`TEST_GITCONFIG`]
/// instead of the developer's global and system config. A global
/// `merge.ff = only` turned the fixtures' diverged merges into refusals, and a
/// `url.<base>.insteadOf` made `git remote get-url` disagree with the raw
/// `.git/config` read it is compared against; rerere, hooks paths, excludes
/// files and signing settings leak in the same way. A test that needs a
/// specific global setting still sets `GIT_CONFIG_GLOBAL` itself, after this.
fn isolate_git_config(root: &std::path::Path) {
    // Each live process owns its pathname. Starting another nextest worker must
    // not change the mtime used by an existing worker's Git config cache.
    let owner = std::process::id();
    let path = std::path::absolute(root.join(format!("gitconfig.{owner}")))
        .expect("resolve the test gitconfig path");
    std::fs::write(&path, TEST_GITCONFIG).expect("write the test gitconfig");
    #[cfg(windows)]
    let wide_path = {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    };
    OWNED_GIT_CONFIG
        .set(OwnedGitConfig {
            owner,
            path: path.clone(),
            #[cfg(windows)]
            wide_path,
        })
        .expect("record the process-owned test gitconfig");
    // SAFETY: called from the process constructor, before libtest starts threads.
    unsafe {
        std::env::set_var("GIT_CONFIG_GLOBAL", &path);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
        std::env::remove_var("GIT_CONFIG_SYSTEM");
        // Command-scope config an invoking `git -c …` or shell may export.
        std::env::remove_var("GIT_CONFIG_PARAMETERS");
        std::env::remove_var("GIT_CONFIG_COUNT");
        // Honoured by `git config` itself (no --file given) in place of the
        // layered lookup, so it would bypass GIT_CONFIG_GLOBAL for exactly the
        // `git config --get` reads tests use to inspect a repo.
        std::env::remove_var("GIT_CONFIG");
    }
}

/// Scratch space for Rust tests, overridable by the test runner.
pub fn test_temp_root() -> std::path::PathBuf {
    let root = std::env::var_os("TUIC_TEST_TMP_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| checkout_root().join(".tmp/tuic-tests"));
    std::fs::create_dir_all(&root).expect("create repository test temp root");
    root
}

/// Fixture location for tests whose subject distinguishes persistent paths from
/// temporary roots. Each caller must own a TempDir here so teardown removes it.
/// This is separate from TMPDIR, not a classifier bypass (story 2160).
pub fn non_temporary_fixture_root() -> std::path::PathBuf {
    let root = checkout_root().join(".tmp/orphan-persistent-fixtures");
    std::fs::create_dir_all(&root).expect("create non-temporary fixture root");
    root
}

#[cfg(unix)]
fn socket_root_fits(root: &std::path::Path) -> bool {
    root.join("sXXXXXX/.mdkb/daemon-hook.sock.4294967295.tmp")
        .as_os_str()
        .len()
        + 8
        < 104
}

/// Return a short, checkout-specific scratch path for Unix-domain socket tests.
#[cfg(unix)]
pub fn short_socket_test_temp_root() -> std::path::PathBuf {
    let requested = test_temp_root();
    if socket_root_fits(&requested) {
        return requested;
    }
    use std::hash::{Hash, Hasher};

    let checkout = checkout_root();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    checkout.hash(&mut hasher);
    let suffix = format!("s{:016x}", hasher.finish());
    // A nested Gits directory can itself be too long. Try every ancestor,
    // preserving the short Gits scratch root used by developer worktrees.
    let root = checkout
        .ancestors()
        .filter(|path| path.file_name().is_some_and(|name| name == "Gits"))
        .map(|gits| gits.join(".tmp").join(&suffix))
        .find(|root| socket_root_fits(root))
        // CI checkouts need not live under Gits. Do not use env::temp_dir():
        // the test constructor points it back at the oversized requested root.
        .unwrap_or_else(|| std::path::Path::new("/tmp").join(format!("tuic-{suffix}")));
    assert!(
        socket_root_fits(&root),
        "short socket root exceeds Unix path budget"
    );
    std::fs::create_dir_all(&root).expect("create short socket test root");
    root
}

/// Return the host shell and its script argument.
pub fn host_shell() -> (&'static str, &'static str) {
    if cfg!(windows) {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    }
}

/// Evaluate a value whose output marker is absent from the submitted command.
pub fn relay_answer_command() -> &'static str {
    if cfg!(windows) {
        "set /a tuic_relay_answer=6*7\r\necho %tuic_relay_answer%relay\r\n"
    } else {
        "echo $((6*7))relay\n"
    }
}

/// Wait for stdin without spawning a child process; the parent keeps the pipe open.
pub fn wait_for_stdin_script() -> String {
    if cfg!(windows) {
        "set /p value=".to_owned()
    } else {
        "read value".to_owned()
    }
}

/// Print a file's contents in the host shell.
pub fn print_file_script(path: &str) -> String {
    if cfg!(windows) {
        format!("type {path}")
    } else {
        format!("cat {path}")
    }
}

/// Create an empty file in the host shell.
pub fn touch_script(path: &str) -> String {
    if cfg!(windows) {
        format!("type nul > {path}")
    } else {
        format!("touch {path}")
    }
}

/// Copy a file in the host shell.
pub fn copy_file_script(source: &str, destination: &str) -> String {
    if cfg!(windows) {
        format!("copy /y {source} {destination} >nul")
    } else {
        format!("cp {source} {destination}")
    }
}

/// Run one host-shell script after another.
pub fn chain(first: &str, second: &str) -> String {
    if cfg!(windows) {
        format!("{first}& {second}")
    } else {
        format!("{first}; {second}")
    }
}

/// Print an environment variable in the host shell.
pub fn print_var_script(key: &str) -> String {
    if cfg!(windows) {
        format!("if defined {key} (echo %{key}%)")
    } else {
        format!("echo \"${key}\"")
    }
}

/// Return a command that sleeps longer than test timeouts.
pub fn sleep_argv() -> (String, Vec<String>) {
    if cfg!(windows) {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        (
            format!("{root}\\System32\\ping.exe"),
            vec!["-n".to_string(), "61".to_string(), "127.0.0.1".to_string()],
        )
    } else {
        ("sleep".to_string(), vec!["60".to_string()])
    }
}

/// Return the same sleep command as a host-shell script.
pub fn sleep_script() -> String {
    let (program, args) = sleep_argv();
    let call = format!("{program} {}", args.join(" "));
    if cfg!(windows) {
        format!("{call} >nul")
    } else {
        call
    }
}

/// Write a fake SSH executable to a stable path and warm it before use.
pub fn fake_ssh_script(name: &str, posix: &str, windows: &str) -> std::path::PathBuf {
    const WARMUP_VAR: &str = "TUIC_FAKE_SSH_WARMUP";

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/fake-ssh");
    std::fs::create_dir_all(&dir).expect("create fake-ssh dir");
    let (extension, desired) = if cfg!(windows) {
        (
            "cmd",
            format!("@echo off\r\nif defined {WARMUP_VAR} exit /b 0\r\n{windows}\r\n"),
        )
    } else {
        (
            "sh",
            format!("#!/bin/sh\n[ -n \"${WARMUP_VAR}\" ] && exit 0\n{posix}\n"),
        )
    };
    let path = dir.join(format!("{name}.{extension}"));

    if std::fs::read_to_string(&path).is_ok_and(|found| found == desired) {
        return path;
    }

    let staging = dir.join(format!("{name}.{extension}.{}", std::process::id()));
    std::fs::write(&staging, &desired).expect("write fake ssh script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake ssh script");
    }
    std::fs::rename(&staging, &path).expect("install fake ssh script");

    let _ = std::process::Command::new(&path)
        .env(WARMUP_VAR, "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    path
}

/// Write a message to stderr and exit with the given code.
pub fn fail_with_stderr_script(line: &str, code: i32) -> String {
    if cfg!(windows) {
        format!("echo {line} 1>&2& exit /b {code}")
    } else {
        format!("echo {line} >&2; exit {code}")
    }
}

/// Normalize Windows CRLF output to LF.
pub fn normalize_newlines(output: &str) -> String {
    output.replace("\r\n", "\n")
}

/// Build a host-shell command that replays a file into a PTY.
pub fn replay_file_command(path: &std::path::Path) -> portable_pty::CommandBuilder {
    let (shell, flag) = host_shell();
    let spelled = if cfg!(windows) {
        assert!(
            !path.to_string_lossy().contains(' '),
            "a space in {} cannot survive `cmd /C` through CommandBuilder; \
             give the test a temp directory without one",
            path.display(),
        );
        path.display().to_string().replace('/', "\\")
    } else {
        format!("\"{}\"", path.display())
    };
    let mut command = portable_pty::CommandBuilder::new(shell);
    command.arg(flag);
    command.arg(print_file_script(&spelled));
    command
}

/// Return a path using forward slashes on every platform.
pub fn slashed(path: &str) -> String {
    path.replace('\\', "/")
}

/// Spell a Unix fixture path (`/home/user`) as an absolute path of this host.
/// On Windows a leading `/` is only rooted, so `Path::is_absolute` refuses it;
/// the path moves to drive `C:` there and stays unchanged elsewhere.
pub fn host_absolute(unix: &str) -> String {
    if cfg!(windows) {
        format!("C:{unix}")
    } else {
        unix.to_owned()
    }
}

/// Return an existing directory outside the current user's home directory.
pub fn dir_outside_home() -> std::path::PathBuf {
    if cfg!(windows) {
        std::path::PathBuf::from(
            std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string()),
        )
    } else {
        std::path::PathBuf::from("/tmp")
    }
}

/// HTTP request bytes captured by an IPC test server.
pub struct HttpRequest {
    pub request_line: String,
    /// Header lines retain their trailing CRLF for existing wire assertions.
    pub headers: Vec<String>,
    pub body: Vec<u8>,
}

/// Read one length-delimited request from a test connection.
pub fn read_http_request(reader: &mut impl std::io::Read) -> std::io::Result<HttpRequest> {
    use std::io::BufRead;
    let mut reader = std::io::BufReader::new(reader);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
    }
    let request_line = line.trim_end().to_string();
    let mut headers = Vec::new();
    let mut length = 0;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
        }
        if line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid Content-Length")
            })?;
        }
        headers.push(line.clone());
    }
    let mut body = vec![0; length];
    std::io::Read::read_exact(&mut reader, &mut body)?;
    Ok(HttpRequest {
        request_line,
        headers,
        body,
    })
}

#[cfg(test)]
mod tests {
    fn git_config_get(cwd: &std::path::Path, key: &str) -> Option<String> {
        let out = std::process::Command::new("git")
            .args(["config", "--get", key])
            .current_dir(cwd)
            .output()
            .expect("run git config");
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Assert the process sees only the test gitconfig, in a fresh repo.
    fn assert_only_test_gitconfig() {
        let dir = tempfile::tempdir_in(super::test_temp_root()).expect("temp dir");
        // Its own repo: the temp root sits inside this checkout, whose local
        // config would otherwise answer.
        let init = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .status()
            .expect("run git init");
        assert!(init.success(), "git init failed");
        assert_eq!(
            git_config_get(dir.path(), "user.email").as_deref(),
            Some("tests@tuic.invalid")
        );
        assert_eq!(
            git_config_get(dir.path(), "init.defaultBranch").as_deref(),
            Some("main")
        );
        for leaked in ["merge.ff", "pull.rebase", "rerere.enabled"] {
            assert_eq!(
                git_config_get(dir.path(), leaked),
                None,
                "{leaked} leaked in from the developer's git config"
            );
        }
    }

    // Catches: each completed worker leaving its constructor-created config behind,
    // or teardown deleting a replacement config selected by a test.
    #[test]
    fn test_process_removes_its_isolated_gitconfig_on_exit() {
        let parent = std::path::PathBuf::from(std::env::var_os("GIT_CONFIG_GLOBAL").unwrap());
        let parent_bytes = std::fs::read(&parent).unwrap();
        let parent_modified = std::fs::metadata(&parent).unwrap().modified().unwrap();
        for mode in ["return", "exit", "override"] {
            let dir = tempfile::tempdir_in(super::test_temp_root()).unwrap();
            let report = dir.path().join("config-path");
            let unrelated = dir.path().join("gitconfig.unrelated");
            std::fs::write(&unrelated, "preserve this config").unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "tests::gitconfig_exit_probe_child",
                    "--nocapture",
                ])
                .env("TUIC_TEST_TMP_ROOT", dir.path())
                .env("TUIC_GITCONFIG_EXIT_REPORT", &report)
                .env("TUIC_GITCONFIG_EXIT_MODE", mode)
                .env("TUIC_GITCONFIG_REPLACEMENT", &unrelated)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{mode}: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let child_path = std::path::PathBuf::from(std::fs::read_to_string(report).unwrap());
            assert!(child_path.starts_with(dir.path()));
            assert_ne!(child_path, parent);
            assert_eq!(std::fs::read(&parent).unwrap(), parent_bytes);
            assert_eq!(
                std::fs::metadata(&parent).unwrap().modified().unwrap(),
                parent_modified
            );
            assert_eq!(
                std::fs::read_to_string(unrelated).unwrap(),
                "preserve this config"
            );
            assert!(
                !child_path.exists(),
                "{mode}: completed test process retained {}",
                child_path.display()
            );
        }
    }

    // Child half of the exit regression: observe the real constructor's config.
    #[test]
    fn gitconfig_exit_probe_child() {
        let Some(report) = std::env::var_os("TUIC_GITCONFIG_EXIT_REPORT") else {
            return;
        };
        let path = std::path::PathBuf::from(std::env::var_os("GIT_CONFIG_GLOBAL").unwrap());
        assert!(path.is_file());
        assert_only_test_gitconfig();
        std::fs::write(report, path.to_str().unwrap()).unwrap();
        match std::env::var("TUIC_GITCONFIG_EXIT_MODE").unwrap().as_str() {
            "exit" => std::process::exit(0),
            "override" => {
                // SAFETY: this re-exec runs only this probe; no concurrent test
                // or subprocess reads the environment after this point.
                unsafe {
                    std::env::set_var(
                        "GIT_CONFIG_GLOBAL",
                        std::env::var_os("TUIC_GITCONFIG_REPLACEMENT").unwrap(),
                    );
                }
            }
            "return" => {}
            mode => panic!("unexpected exit mode: {mode}"),
        }
    }

    const HOSTILE_MARKER: &str = "TUIC_TEST_SUPPORT_HOSTILE_CHILD";
    const HOSTILE_CHILD: &str = "tests::hostile_env_probe_child";

    /// The constructor's config is the one git reads in this process.
    #[test]
    fn git_in_a_test_process_reads_only_the_test_gitconfig() {
        assert_only_test_gitconfig();
    }

    /// Body of the re-exec'd child below; a no-op in a normal run.
    #[test]
    fn hostile_env_probe_child() {
        if std::env::var_os(HOSTILE_MARKER).is_some() {
            assert_only_test_gitconfig();
        }
    }

    // Catches: another nextest process replacing our config and invalidating Git caches.
    #[test]
    fn child_constructor_preserves_parent_git_config_metadata() {
        let path = std::path::PathBuf::from(std::env::var_os("GIT_CONFIG_GLOBAL").unwrap());
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", HOSTILE_CHILD, "--nocapture"])
            .env(HOSTILE_MARKER, "1")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(String::from_utf8_lossy(&out.stdout).contains("1 passed"));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            super::TEST_GITCONFIG
        );
        assert_eq!(std::fs::metadata(path).unwrap().modified().unwrap(), before);
    }

    /// Re-run this test binary with a hostile inherited git environment (the
    /// constructor runs in the child, before its tests) and prove the
    /// constructor replaces it. In-process this cannot be shown: the
    /// constructor has already run before any test can set the hostile state.
    #[test]
    fn constructor_overrides_a_hostile_inherited_git_environment() {
        if std::env::var_os(HOSTILE_MARKER).is_some() {
            return;
        }
        let dir = tempfile::tempdir_in(super::test_temp_root()).expect("temp dir");
        let hostile = dir.path().join("hostile");
        std::fs::write(
            &hostile,
            "[user]\n\temail = hostile@example.invalid\n[merge]\n\tff = only\n\
             [pull]\n\trebase = true\n[rerere]\n\tenabled = true\n",
        )
        .expect("write hostile config");
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).expect("home");
        std::fs::copy(&hostile, home.join(".gitconfig")).expect("hostile ~/.gitconfig");
        let out = std::process::Command::new(std::env::current_exe().expect("current exe"))
            .args(["--exact", HOSTILE_CHILD, "--nocapture", "--test-threads=1"])
            .env(HOSTILE_MARKER, "1")
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &home)
            .env("GIT_CONFIG_GLOBAL", &hostile)
            .env("GIT_CONFIG_SYSTEM", &hostile)
            .env("GIT_CONFIG", &hostile)
            .env("GIT_CONFIG_PARAMETERS", "'merge.ff=only'")
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "pull.rebase")
            .env("GIT_CONFIG_VALUE_0", "true")
            .env_remove("GIT_CONFIG_NOSYSTEM")
            .output()
            .expect("re-exec the test binary");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success() && stdout.contains("1 passed"),
            "child failed or did not run the probe:\n{stdout}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
