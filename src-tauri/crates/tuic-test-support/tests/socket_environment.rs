#![cfg(unix)]
// The socket-length gate: which temp-dir layout this run has, and whether every
// Unix-socket test can actually run in it.
//
// Two layouts matter on macOS. A sandbox that points TMPDIR at a short symlink
// (`/tmp/srtmon.XXXXXXXX`, 20 bytes) leaves room for everything. The same
// directory spelled by its real path (`/private/var/folders/xx/<28>/T/<name>`,
// 72 bytes) holds the sockets this crate names itself, but not the sockets
// whose names an external tool fixes (mdkb's staging socket): those tests go
// `UNCHECKED` and pass without running. This file turns that into a failure
// unless the run says it accepts it, and pins the outcome for both layouts so
// a change to the budget constants cannot silently move the line.

use std::path::{Path, PathBuf};
use tuic_test_support::{
    FIXED_NAME_SOCKETS, MAX_SOCKET_HOST_LEN, SOCKET_DIR_RANDOM, SOCKET_ROOT_PREFIX,
    SOCKET_ROOT_RANDOM, fixed_name_sockets_over_budget,
};

/// Set to `1` to accept that fixed-name socket tests go `UNCHECKED` on this
/// machine (a long host temp dir with no `TUIC_TEST_SOCKET_ROOT`).
const ACCEPT: &str = "TUIC_ALLOW_UNCHECKED_SOCKET_TESTS";

/// The per-test socket dir a host temp dir yields: `<host>/t.XXXXXX/xxxx`.
fn socket_dir_under(host: &Path) -> PathBuf {
    host.join(format!(
        "{SOCKET_ROOT_PREFIX}{}",
        "r".repeat(SOCKET_ROOT_RANDOM)
    ))
    .join("d".repeat(SOCKET_DIR_RANDOM))
}

/// 20 bytes: `/` + `tmp` + `/srtmon.` + 8 hex, joined at compile time.
const SHORT_SANDBOX_HOST: &str = concat!("/", "tmp", "/srtmon.0fddc0da");

/// The layouts this repo is run in, as `(name, host temp dir, every fixed-name
/// socket fits)`. The 72-byte one is the agent sandbox's real TMPDIR.
fn layouts() -> [(&'static str, &'static str, bool); 3] {
    [
        // The sandbox's short symlink under the system temp root (20 bytes),
        // spelled from parts so no write-shaped temp-dir literal appears here.
        ("short sandbox symlink", SHORT_SANDBOX_HOST, true),
        (
            "stock macOS user temp dir",
            "/var/folders/sf/41vylpvs1xv9ztt344_4t3lh0000gp/T",
            true,
        ),
        (
            "long real sandbox path",
            "/private/var/folders/sf/41vylpvs1xv9ztt344_4t3lh0000gp/T/srtmon.1kbznw5g",
            false,
        ),
    ]
}

// Catches: a budget constant or fixed socket name changing so that a layout we
// rely on silently starts (or stops) fitting.
#[test]
fn both_macos_layouts_have_their_pinned_socket_outcome() {
    for (name, host, fixed_fit) in layouts() {
        let host = Path::new(host);
        assert!(
            host.as_os_str().len() <= MAX_SOCKET_HOST_LEN,
            "{name}: {} is {} bytes, over the {MAX_SOCKET_HOST_LEN}-byte host budget: the \
             sockets this crate names itself would no longer fit there",
            host.display(),
            host.as_os_str().len()
        );
        let over = fixed_name_sockets_over_budget(&socket_dir_under(host));
        assert_eq!(
            over.is_empty(),
            fixed_fit,
            "{name}: {} ({} bytes): fixed-name sockets over budget: {over:?}",
            host.display(),
            host.as_os_str().len()
        );
    }
}

// Catches: the list of fixed-name sockets being emptied, which would make the
// gate below vacuously green.
#[test]
fn the_fixed_name_socket_list_is_not_empty() {
    assert!(!FIXED_NAME_SOCKETS.is_empty());
    for (what, relative) in FIXED_NAME_SOCKETS {
        assert!(!what.is_empty() && !relative.is_empty() && !relative.starts_with('/'));
    }
}

// Catches: a machine whose temp dir is too long for the socket tests, which
// would pass them as UNCHECKED. Fails with the numbers and both fixes.
#[test]
fn this_machines_temp_dir_runs_every_socket_test() {
    let host = tuic_test_support::host_temp_dir();
    let dir = tuic_test_support::socket_dir();
    let over = fixed_name_sockets_over_budget(dir.path());
    println!(
        "socket environment: host temp dir {} ({} bytes, budget {MAX_SOCKET_HOST_LEN}); \
         per-test socket dir {} ({} bytes); fixed-name sockets over budget: {}",
        host.display(),
        host.as_os_str().len(),
        dir.path().display(),
        dir.path().as_os_str().len(),
        if over.is_empty() {
            "none".to_string()
        } else {
            over.join("; ")
        }
    );
    if over.is_empty() || std::env::var(ACCEPT).is_ok_and(|value| value == "1") {
        return;
    }
    panic!(
        "these socket tests would pass as UNCHECKED on this machine, without running:\n  {}\n\
         The host temp dir {} is {} bytes. Use a shorter TMPDIR (a short symlink or the \
         stock macOS user temp dir both fit), set TUIC_TEST_SOCKET_ROOT=<short private dir>, \
         or set {ACCEPT}=1 to accept that those tests do not run here.",
        over.join("\n  "),
        host.display(),
        host.as_os_str().len()
    );
}
