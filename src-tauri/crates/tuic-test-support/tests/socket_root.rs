#![cfg(unix)]
// Nextest runs every test in this binary in its own process, so each test may
// change the environment without restoring it. The socket root is cached per
// host temp dir, so a test that needs a fresh one points
// TUIC_TEST_HOST_TMPDIR at a fresh directory.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tuic_test_support::{
    MAX_SOCKET_HOST_LEN, MAX_SOCKET_ROOT_LEN, MAX_TEST_SOCKET_NAME, SUN_PATH_MAX, SUN_PATH_USABLE,
};

const CHILD: &str = "TUIC_SOCKET_ROOT_CHILD";

/// The host temp dir this test process was given (before the constructor
/// pointed TMPDIR at the per-run root).
fn host() -> PathBuf {
    tuic_test_support::host_temp_dir()
}

/// A directory removed on drop.
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700));
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A fresh directory directly in the host temp dir whose path is exactly
/// `len` bytes, or `None` when the host temp dir is already too long for that.
fn host_dir_of_len(len: usize) -> Option<Scratch> {
    let host = host();
    let tag = format!("{}", std::process::id());
    let pad = len.checked_sub(host.as_os_str().len() + 1 + tag.len())?;
    let dir = host.join(format!("{tag}{}", "p".repeat(pad)));
    assert_eq!(dir.as_os_str().len(), len);
    std::fs::create_dir(&dir).ok()?;
    Some(Scratch(dir))
}

/// A fresh, short directory in the host temp dir (`<host>/<tag><pid>`).
fn short_host_dir(tag: &str) -> Scratch {
    let dir = host().join(format!("{tag}{}", std::process::id()));
    std::fs::create_dir(&dir).expect("short host scratch");
    Scratch(dir)
}

/// A fresh host temp dir for the resolver when one fits the budget below the
/// real one; else the real host temp dir itself (the agent sandbox's is
/// exactly 72 bytes, so nothing below it fits). Returns the dir in use.
fn fresh_host(tag: &str) -> (Option<Scratch>, PathBuf) {
    let fresh = short_host_dir(tag);
    if fresh.0.as_os_str().len() <= MAX_SOCKET_HOST_LEN {
        let path = fresh.0.clone();
        unsafe { std::env::set_var("TUIC_TEST_HOST_TMPDIR", &path) };
        (Some(fresh), path)
    } else {
        println!(
            "note: {} is over the {MAX_SOCKET_HOST_LEN}-byte host budget; using the host temp dir",
            fresh.0.display()
        );
        (None, host())
    }
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

fn run_child(test: &str, configure: impl FnOnce(&mut std::process::Command)) -> String {
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env(CHILD, "1");
    configure(&mut child);
    let output = child.output().unwrap();
    assert!(
        output.status.success(),
        "child failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

// Exact budget: Rust's std binds at most 103 bytes (104-byte sun_path − NUL);
// a socket is `<host>/t.XXXXXX/xxxx/<name of up to 16 bytes>`.
#[test]
fn socket_budget_math_is_exact() {
    assert_eq!(SUN_PATH_MAX, 104);
    assert_eq!(SUN_PATH_USABLE, 103);
    assert_eq!(MAX_TEST_SOCKET_NAME, "mcp-4194304.sock".len());
    assert_eq!(MAX_SOCKET_ROOT_LEN, 81);
    assert_eq!(MAX_SOCKET_HOST_LEN, 72);
    let socket_len = |host: usize| host + "/t.XXXXXX".len() + "/xxxx".len() + 1 + 16;
    // The three TMPDIR lengths that matter: macOS default (49), the agent
    // sandbox (72) and a long CI-like one (90).
    assert_eq!(socket_len(49), 80, "macOS default: 23 bytes of headroom");
    assert_eq!(
        socket_len(72),
        103,
        "agent sandbox: fits with 0 bytes to spare"
    );
    assert_eq!(socket_len(73), 104, "one byte longer no longer fits");
    assert_eq!(socket_len(90), 121, "long CI TMPDIR: 18 bytes over");
    let at = |len: usize| PathBuf::from(format!("/{}", "x".repeat(len - 1)));
    assert!(tuic_test_support::unix_socket_path_fits(&at(103)).is_ok());
    let over = tuic_test_support::unix_socket_path_fits(&at(104)).unwrap_err();
    assert!(
        over.contains("104 bytes") && over.contains("103-byte"),
        "{over}"
    );
    assert!(tuic_test_support::socket_root_fits(&at(81)));
    assert!(!tuic_test_support::socket_root_fits(&at(82)));
}

// Catches: a test socket name outgrowing the per-test-dir budget.
#[test]
fn every_test_socket_name_fits_the_budgeted_name_length() {
    for name in [
        "mcp-4194304.sock", // mcp_http, 7-digit Linux pid
        "story.sock",
        "probeable.sock",
        "silent.sock",
        "override.sock",
        "m99999.sock",
        "nw99999.sock",
        "b99999.sock",
    ] {
        assert!(name.len() <= MAX_TEST_SOCKET_NAME, "{name} is too long");
    }
}

// Catches: a resolver that lands anywhere but a private dir under TMPDIR
// (the old one picked `/tmp/tuic-s<hash>`).
#[test]
fn socket_root_is_a_private_marked_dir_under_the_host_temp_dir_never_tmp() {
    let (_fresh, host) = fresh_host("h");
    let root = tuic_test_support::short_socket_test_temp_root();
    assert_eq!(root.parent(), Some(host.as_path()), "{}", root.display());
    let name = root.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        name.len() == 8 && name.starts_with("t."),
        "socket root name {name}"
    );
    // A shared temp dir is only a defect when the root fell back to it: a host
    // temp dir that is itself a `/tmp/...` symlink (a sandbox's short TMPDIR)
    // legitimately puts the root below one.
    for shared in ["/tmp", "/private/tmp", "/var/tmp"] {
        assert!(
            host.starts_with(shared) || !root.starts_with(shared),
            "{}",
            root.display()
        );
    }
    let meta = std::fs::symlink_metadata(&root).unwrap();
    assert!(meta.is_dir() && !meta.file_type().is_symlink());
    assert_eq!(meta.permissions().mode() & 0o777, 0o700);
    assert!(root.join(tuic_test_support::SOCKET_ROOT_MARKER).is_file());
    assert_eq!(tuic_test_support::short_socket_test_temp_root(), root);
    let dir = tuic_test_support::socket_dir();
    assert_eq!(dir.path().parent(), Some(root.as_path()));
    assert_eq!(dir.path().as_os_str().len(), root.as_os_str().len() + 5);
    assert_eq!(
        std::fs::metadata(dir.path()).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let socket = dir.path().join("mcp-4194304.sock");
    assert!(tuic_test_support::unix_socket_path_fits(&socket).is_ok());
    match std::os::unix::net::UnixListener::bind(&socket) {
        Ok(listener) => drop(listener),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            println!("SKIP bind: the sandbox denies Unix binds here ({error})")
        }
        Err(error) => panic!("bind {}: {error}", socket.display()),
    }
    let gone = dir.path().to_path_buf();
    drop(dir);
    assert!(!gone.exists(), "a SocketDir is removed on drop");
}

// Catches: an off-by-one at the 72-byte host boundary (the sandbox TMPDIR).
#[test]
fn a_host_temp_dir_of_exactly_the_budget_holds_the_longest_test_socket() {
    let Some(host72) = host_dir_of_len(MAX_SOCKET_HOST_LEN) else {
        let host = host();
        if host.as_os_str().len() == MAX_SOCKET_HOST_LEN {
            // The host temp dir itself is the boundary case (the sandbox).
            let dir = tuic_test_support::socket_dir();
            assert_eq!(
                dir.path().join("mcp-4194304.sock").as_os_str().len(),
                SUN_PATH_USABLE
            );
            return;
        }
        println!(
            "SKIP: host temp dir {} is {} bytes, no 72-byte dir fits below it",
            host.display(),
            host.as_os_str().len()
        );
        return;
    };
    unsafe { std::env::set_var("TUIC_TEST_HOST_TMPDIR", &host72.0) };
    let dir = tuic_test_support::socket_dir();
    let socket = dir.path().join("mcp-4194304.sock");
    assert_eq!(socket.as_os_str().len(), SUN_PATH_USABLE);
    if let Ok(listener) = std::os::unix::net::UnixListener::bind(&socket) {
        drop(listener);
    }
}

// Catches: a silent /tmp fallback (or a mystery SUN_LEN bind error) when the
// host temp dir is too long, instead of a message naming the fix.
#[test]
fn an_over_budget_host_temp_dir_fails_naming_the_budget_lengths_and_the_override() {
    for len in [MAX_SOCKET_HOST_LEN + 1, 90] {
        // Never created: the length check comes first.
        let long = PathBuf::from(format!("/{}", "L".repeat(len - 1)));
        let message = tuic_test_support::socket_root_under(&long).unwrap_err();
        for needle in [
            format!("{len} bytes"),
            format!("would be {} bytes", len + 9),
            "at most 81 bytes".to_string(),
            "at most 72 bytes".to_string(),
            "TUIC_TEST_SOCKET_ROOT".to_string(),
            "There is no shared-temp-dir fallback".to_string(),
        ] {
            assert!(message.contains(&needle), "{needle} missing: {message}");
        }
        assert!(!long.exists());
    }
    let long = PathBuf::from(format!("/{}", "L".repeat(89)));
    unsafe { std::env::set_var("TUIC_TEST_HOST_TMPDIR", &long) };
    let panic = std::panic::catch_unwind(tuic_test_support::short_socket_test_temp_root)
        .expect_err("an over-budget host temp dir must fail");
    assert!(panic_message(panic).contains("TUIC_TEST_SOCKET_ROOT"));
}

#[test]
fn explicit_socket_root_override_is_honoured() {
    let scratch = short_host_dir("e");
    let explicit = scratch.0.join("s");
    unsafe { std::env::set_var("TUIC_TEST_SOCKET_ROOT", &explicit) };
    // Even with a host temp dir that could never hold a default root.
    unsafe { std::env::set_var("TUIC_TEST_HOST_TMPDIR", format!("/{}", "L".repeat(99))) };
    assert_eq!(tuic_test_support::short_socket_test_temp_root(), explicit);
    assert!(explicit.is_dir());
}

// Catches: an over-budget override failing later as a mystery bind error.
#[test]
fn unusable_override_fails_naming_the_budget_and_the_variable() {
    let too_long = host().join("y".repeat(100));
    unsafe { std::env::set_var("TUIC_TEST_SOCKET_ROOT", &too_long) };
    let panic = std::panic::catch_unwind(tuic_test_support::short_socket_test_temp_root)
        .expect_err("an over-budget override must fail");
    let message = panic_message(panic);
    assert!(message.contains("TUIC_TEST_SOCKET_ROOT"), "{message}");
    assert!(message.contains("81"), "{message}");
    assert!(
        message.contains(&too_long.display().to_string()),
        "{message}"
    );
    assert!(!too_long.exists());
}

// Catches: following a symlink another user planted, keeping a lax mode, or
// accepting an unwritable dir as a private socket root.
#[test]
fn private_socket_root_checks_refuse_symlinks_tighten_modes_and_need_write() {
    let scratch = short_host_dir("k");
    let target = scratch.0.join("x");
    std::fs::create_dir(&target).unwrap();
    let link = scratch.0.join("l");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert_eq!(
        tuic_test_support::prepare_socket_root(&link, true).unwrap_err(),
        "is a symlink"
    );
    let lax = scratch.0.join("m");
    std::fs::create_dir(&lax).unwrap();
    std::fs::set_permissions(&lax, std::fs::Permissions::from_mode(0o755)).unwrap();
    tuic_test_support::prepare_socket_root(&lax, true).unwrap();
    assert_eq!(
        std::fs::metadata(&lax).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let locked = scratch.0.join("r");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).unwrap();
    let refused = tuic_test_support::prepare_socket_root(&locked, true);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(refused.unwrap_err().starts_with("not writable"));
}

// Catches: a name that does not fit reaching bind() as an opaque SUN_LEN error.
#[test]
fn short_socket_path_refuses_a_name_that_cannot_fit() {
    // Longer than the whole budget, so it overflows below any host temp dir.
    let panic = std::panic::catch_unwind(|| {
        tuic_test_support::short_socket_path(&"n".repeat(SUN_PATH_USABLE))
    })
    .expect_err("an over-long socket name must fail");
    assert!(panic_message(panic).contains(&format!("{SUN_PATH_USABLE}-byte")));
    let path = tuic_test_support::short_socket_path("story.sock");
    assert_eq!(
        path.parent(),
        Some(tuic_test_support::short_socket_test_temp_root().as_path())
    );
}

// A socket whose absolute path cannot fit sun_path still binds and connects
// by a relative name with the working directory inside its directory.
#[test]
fn relative_spelling_binds_and_connects_where_the_absolute_path_cannot_fit() {
    let deep = tuic_test_support::socket_dir();
    let mut dir = deep.path().to_path_buf();
    while dir
        .join("tuic-mcp-b199f45760a9ee6c-123.sock")
        .as_os_str()
        .len()
        <= SUN_PATH_USABLE
    {
        dir.push("deeper-than-sun-path");
    }
    std::fs::create_dir_all(&dir).unwrap();
    let name = "tuic-mcp-b199f45760a9ee6c-123.sock";
    let spelling = tuic_test_support::SocketSpelling::for_dir(&dir, name);
    assert_eq!(spelling, tuic_test_support::SocketSpelling::Relative);
    assert_eq!(spelling.temp_dir(&dir), Path::new("."));
    assert_eq!(
        spelling.path(&dir, name),
        Path::new("./tuic-mcp-b199f45760a9ee6c-123.sock")
    );
    let cwd = std::env::current_dir().unwrap();
    let listener = match spelling.bind(&dir, name) {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            println!("SKIP: the sandbox denies Unix binds here ({error})");
            return;
        }
        Err(error) => panic!("relative bind in {}: {error}", dir.display()),
    };
    assert_eq!(std::env::current_dir().unwrap(), cwd, "cwd restored");
    assert!(
        dir.join(name).exists(),
        "the socket landed in its directory"
    );
    tuic_test_support::in_dir(&dir, || {
        std::os::unix::net::UnixStream::connect(name).expect("relative connect")
    });
    listener.accept().expect("accept the relative connection");
    assert_eq!(std::env::current_dir().unwrap(), cwd, "cwd restored");
    assert!(
        std::os::unix::net::UnixStream::connect(dir.join(name)).is_err(),
        "the absolute path really is too long"
    );
}

#[test]
fn child_in_dir_outside_one_test_per_process() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    let dir = tuic_test_support::socket_dir();
    tuic_test_support::in_dir(dir.path(), || ());
}

// Catches: a cwd change racing sibling test threads under multi-threaded libtest.
#[test]
fn in_dir_refuses_a_process_that_runs_tests_in_parallel() {
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child
        .args([
            "--exact",
            "child_in_dir_outside_one_test_per_process",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .env_remove("NEXTEST_RUN_ID")
        .env("RUST_TEST_THREADS", "4");
    let output = child.output().unwrap();
    assert!(!output.status.success(), "in_dir ran outside nextest");
    let text = String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr);
    assert!(text.contains("cargo nextest"), "{text}");
    // RUST_TEST_THREADS=1 is the documented libtest escape hatch.
    run_child("child_in_dir_outside_one_test_per_process", |child| {
        child
            .env_remove("NEXTEST_RUN_ID")
            .env("RUST_TEST_THREADS", "1");
    });
}

#[test]
fn child_prints_its_socket_root() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    let dir = tuic_test_support::socket_dir();
    println!(
        "ROOT={}",
        tuic_test_support::short_socket_test_temp_root().display()
    );
    std::mem::forget(dir); // the exit-time cleanup must remove it anyway
}

// Catches: one leaked `t.XXXXXX` per test process (nextest runs thousands).
#[test]
fn a_process_removes_the_socket_root_it_created_at_exit() {
    let (_fresh, host) = fresh_host("x");
    let out = run_child("child_prints_its_socket_root", |child| {
        child.env("TUIC_TEST_HOST_TMPDIR", &host);
    });
    let root = out
        .lines()
        .find_map(|line| line.split_once("ROOT=").map(|(_, root)| root))
        .map(PathBuf::from)
        .expect("child printed its root");
    assert_eq!(root.parent(), Some(host.as_path()), "{}", root.display());
    assert!(!root.exists(), "{} survived the process", root.display());
}

// Catches: resolving any root from $HOME (the old `~/Gits` convention).
#[test]
fn no_root_is_derived_from_home() {
    let fake_home = short_host_dir("hh");
    unsafe { std::env::set_var("HOME", &fake_home.0) };
    for root in [
        tuic_test_support::host_temp_dir(),
        tuic_test_support::test_base(),
        tuic_test_support::test_temp_root(),
        tuic_test_support::short_socket_test_temp_root(),
    ] {
        assert!(
            !root.starts_with(&fake_home.0),
            "{} derives from HOME",
            root.display()
        );
    }
}
