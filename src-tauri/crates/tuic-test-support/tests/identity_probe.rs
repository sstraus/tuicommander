// The identity probe `build.rs` compiles, and `install_identity_probe`: the
// stand-in tests exec wherever they need a process with a specific name.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap()
}

/// A probe that stays alive blocked on its open stdin, as the tests use it.
fn spawn_blocked(exe: &Path) -> Child {
    Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

fn reap(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn the_built_probe_exists_outside_any_test_dir() {
    let bin = tuic_test_support::identity_probe_bin();
    assert!(bin.is_file(), "{} was not built", bin.display());
    assert!(
        bin.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("tuic-identity-probe"),
        "{}",
        bin.display()
    );
}

#[test]
fn print_mode_prints_its_words_and_exits_zero() {
    let dir = scratch();
    let exe = tuic_test_support::install_identity_probe(dir.path(), "tool");
    let out = Command::new(&exe)
        .args(["--print", "tuic-native-fixture", "two"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out.status);
    assert_eq!(out.stdout, b"tuic-native-fixture two\n");
    assert!(out.stderr.is_empty());
}

#[test]
fn default_mode_copies_stdin_to_stdout_until_eof() {
    let dir = scratch();
    let exe = tuic_test_support::install_identity_probe(dir.path(), "cat-like");
    let mut child = Command::new(&exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let payload: Vec<u8> = (0..20_000u32).map(|i| (i % 251) as u8).collect();
    let mut stdin = child.stdin.take().unwrap();
    let writer = {
        let payload = payload.clone();
        std::thread::spawn(move || stdin.write_all(&payload).unwrap())
    };
    let mut echoed = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut echoed)
        .unwrap();
    writer.join().unwrap();
    assert!(child.wait().unwrap().success());
    assert_eq!(echoed, payload, "stdin must come back byte for byte");
}

#[test]
fn default_mode_stays_alive_while_stdin_is_open() {
    let dir = scratch();
    let exe = tuic_test_support::install_identity_probe(dir.path(), "daemon");
    let mut child = Command::new(&exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(
        child.try_wait().unwrap().is_none(),
        "the probe exited with its stdin still open"
    );
    drop(child.stdin.take());
    assert!(child.wait().unwrap().success(), "EOF on stdin exits 0");
}

/// The reason the probe exists: a relocated Apple platform binary is
/// SIGKILLed on exec in the agent sandbox. The probe, copied anywhere under
/// any name, must exit on its own terms — never by a signal.
#[cfg(unix)]
#[test]
fn an_installed_copy_runs_under_any_name_without_being_killed() {
    use std::os::unix::process::ExitStatusExt;
    let dir = scratch();
    for name in ["claude", "bash", "codex", "git", "direnv", "tuic-remote"] {
        let exe = tuic_test_support::install_identity_probe(dir.path(), name);
        let status = Command::new(&exe).arg("--print").status().unwrap();
        assert_eq!(status.signal(), None, "{name} was killed: {status:?}");
        assert!(status.success(), "{name}: {status:?}");
    }
}

#[cfg(unix)]
#[test]
fn install_makes_an_executable_at_a_nested_path_and_leaves_no_staging_file() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch();
    let exe = tuic_test_support::install_identity_probe(dir.path(), "claude/versions/2.1.5");
    assert_eq!(exe, dir.path().join("claude/versions/2.1.5"));
    let mode = std::fs::metadata(&exe).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o755);
    assert_eq!(
        std::fs::read(&exe).unwrap(),
        std::fs::read(tuic_test_support::identity_probe_bin()).unwrap()
    );
    let entries: Vec<_> = std::fs::read_dir(exe.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(entries, vec![std::ffi::OsString::from("2.1.5")]);
}

/// Re-installing over a probe that is still running replaces the file, not
/// its pages: the running process survives and the path names a new inode
/// holding the probe.
///
/// Exactly one fresh executable is exec'd here. Each first exec of a new
/// inode pays exec-time scanning, which a backlog under a full parallel run
/// can stretch past nextest's 120 s kill (src-tauri/AGENTS.md, "A freshly
/// written executable"). That the new copy runs is already
/// `an_installed_copy_runs_under_any_name_without_being_killed`'s subject.
#[cfg(unix)]
#[test]
fn reinstalling_over_a_running_probe_leaves_it_running() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let dir = scratch();
    let exe = tuic_test_support::install_identity_probe(dir.path(), "claude");
    let first_inode = std::fs::metadata(&exe).unwrap().ino();
    let mut running = spawn_with_stall_note(&exe);
    let again = tuic_test_support::install_identity_probe(dir.path(), "claude");
    assert_eq!(again, exe);
    let replaced = std::fs::metadata(&again).unwrap();
    assert_ne!(
        replaced.ino(),
        first_inode,
        "reinstall rewrote the running file in place instead of renaming a new one over it"
    );
    assert_eq!(replaced.permissions().mode() & 0o777, 0o755);
    assert_eq!(
        std::fs::read(&again).unwrap(),
        std::fs::read(tuic_test_support::identity_probe_bin()).unwrap()
    );
    assert!(
        running.try_wait().unwrap().is_none(),
        "the running probe died when its file was replaced"
    );
    reap(running);
}

/// [`spawn_blocked`], printing a diagnosis if the exec has not returned after
/// 20 s. It never fails the test: nextest's bound still decides, but its
/// TIMEOUT report then names exec-time scanning instead of a probe hang.
#[cfg(unix)]
fn spawn_with_stall_note(exe: &Path) -> Child {
    let (done, waiting) = std::sync::mpsc::channel::<()>();
    let path = exe.display().to_string();
    let note = std::thread::spawn(move || {
        if waiting
            .recv_timeout(std::time::Duration::from_secs(20))
            .is_err_and(|e| e == std::sync::mpsc::RecvTimeoutError::Timeout)
        {
            eprintln!(
                "exec of the freshly installed probe {path} has not returned after 20 s: \
                 exec-time scanning of a new inode (src-tauri/AGENTS.md, \"A freshly \
                 written executable\"), not a probe bug"
            );
        }
    });
    let child = spawn_blocked(exe);
    drop(done);
    note.join().unwrap();
    child
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap()
}

/// What production identity detection reads (`proc_pidpath` on macOS,
/// `/proc/<pid>/exe` on Linux) is the installed path itself — the property a
/// symlink, a script or `exec -a` does not have.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn the_os_reports_the_installed_path_as_the_process_executable() {
    let dir = scratch();
    let exe = tuic_test_support::install_identity_probe(dir.path(), "pi/tool");
    let child = spawn_blocked(&exe);
    let reported = tuic_core::process_info::exe(child.id());
    let pid = child.id();
    reap(child);
    assert_eq!(
        reported.as_deref().map(canonical),
        Some(canonical(&exe)),
        "pid {pid}"
    );
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn ps_output(ps: &Path, args: &[&str]) -> std::process::Output {
    Command::new(ps).args(args).output().unwrap()
}

/// Installed as `ps`, the probe answers `-p <pid> -o comm=` with the pid's
/// executable path, no header — what `remote_deploy`'s `verified_kill` reads.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn ps_personality_reports_a_live_pid_s_executable() {
    let dir = scratch();
    let ps = tuic_test_support::install_identity_probe(dir.path(), "bin/ps");
    let target = tuic_test_support::install_identity_probe(dir.path(), "tuic-remote");
    let child = spawn_blocked(&target);
    let pid = child.id().to_string();
    let spaced = ps_output(&ps, &["-p", &pid, "-o", "comm="]);
    let joined = ps_output(&ps, &[&format!("-p{pid}"), "-ocomm="]);
    let flag = ps_output(
        tuic_test_support::identity_probe_bin(),
        &["--ps", "-p", &pid, "-o", "comm="],
    );
    reap(child);
    for out in [&spaced, &joined, &flag] {
        assert!(out.status.success(), "{out:?}");
        let text = String::from_utf8(out.stdout.clone()).unwrap();
        assert_eq!(text.lines().count(), 1, "one line, no header: {text:?}");
        let printed = PathBuf::from(text.trim());
        assert_eq!(printed.file_name().unwrap(), "tuic-remote");
        // The shell code keeps only `${C##*/}`; the whole path must match too.
        assert_eq!(canonical(&printed), canonical(&target));
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn ps_personality_fails_for_a_gone_pid_and_rejects_unknown_options() {
    let dir = scratch();
    let ps = tuic_test_support::install_identity_probe(dir.path(), "ps");
    let target = tuic_test_support::install_identity_probe(dir.path(), "gone");
    let child = spawn_blocked(&target);
    let pid = child.id().to_string();
    reap(child);
    let gone = ps_output(&ps, &["-p", &pid, "-o", "comm="]);
    assert_eq!(gone.status.code(), Some(1), "{gone:?}");
    assert!(gone.stdout.is_empty());

    for args in [
        &["-ax"][..],
        &["-p", "1", "-o", "pid="][..],
        &["-p", "x", "-o", "comm="][..],
        &["-o", "comm="][..],
        &["-p"][..],
    ] {
        let out = ps_output(&ps, args);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {out:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
    }
}

/// Under its own name the probe is NOT `ps`: `-p` is just stdin-echo input
/// it ignores, so a test cannot get the ps personality by accident.
#[test]
fn only_the_name_ps_or_the_flag_selects_the_ps_personality() {
    let dir = scratch();
    let exe = tuic_test_support::install_identity_probe(dir.path(), "psx");
    let out = Command::new(&exe)
        .args(["-p", "1", "-o", "comm="])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(
        out.stdout.is_empty(),
        "echo mode with empty stdin prints nothing"
    );
}
