use std::path::Path;
use tuic_ipc::app_instance::AppInstance;

// Catches: changing default paths, losing instance config isolation or changing the deployed hash.
#[test]
fn ipc_instance_paths_preserve_default_and_named_namespaces() {
    let default = AppInstance::default();
    let named = AppInstance::named("ipc-cli-regression").unwrap();
    let base = Path::new("/platform");
    let home = Path::new("/home");
    let temp = Path::new("/scratch");
    assert_eq!(
        default.config_dir_from(Some(base), home),
        Path::new("/platform/com.tuic.commander")
    );
    assert_eq!(
        default.config_dir_from(None, home),
        Path::new("/home/.tuicommander")
    );
    let config = named.config_dir_from(Some(base), home);
    assert_eq!(
        config,
        Path::new("/platform/com.tuic.commander/instances/ipc-cli-regression")
    );
    assert_eq!(
        tuic_ipc::socket_path(&default, &default.config_dir_from(Some(base), home), temp),
        Path::new("/platform/com.tuic.commander/mcp.sock")
    );
    assert_eq!(
        tuic_ipc::socket_path(&named, &config, temp),
        Path::new("/scratch/tuic-mcp-1953e60b4e4d5340.sock")
    );
    for invalid in ["", "default", "UPPER", "../escape", "-start", "end-"] {
        assert!(AppInstance::named(invalid).is_err(), "{invalid}");
    }
}

// Catches: the alternate drifting from the `tuic-mcp-<hash>-<pid>.sock` shape
// the bridge's fallback scan and the Local-connect identity check accept.
#[test]
fn named_alternate_socket_keeps_the_primary_stem_and_appends_the_pid() {
    let temp = Path::new("/scratch");
    assert_eq!(
        tuic_ipc::named_alternate_socket_path("ipc-cli-regression", temp, 4242),
        Path::new("/scratch/tuic-mcp-1953e60b4e4d5340-4242.sock")
    );
}

// Catches: an over-long socket path reaching bind() as an opaque SUN_LEN error,
// and an off-by-one at the 103/104-byte boundary.
#[test]
fn unix_socket_path_check_is_exact_at_the_boundary_and_names_the_way_out() {
    assert_eq!(tuic_ipc::UNIX_SOCKET_PATH_MAX, 103);
    let name = "tuic-mcp-1953e60b4e4d5340-4242.sock";
    let dir = |len: usize| format!("/{}", "d".repeat(len - 1));
    // 103 bytes fits, 104 does not (Rust std refuses it: SUN_LEN).
    let fits = Path::new(&dir(103 - 1 - name.len())).join(name);
    assert_eq!(fits.as_os_str().len(), 103);
    assert!(tuic_ipc::check_unix_socket_path(&fits).is_ok());
    let over = Path::new(&dir(104 - 1 - name.len())).join(name);
    assert_eq!(over.as_os_str().len(), 104);
    let message = tuic_ipc::check_unix_socket_path(&over).unwrap_err();
    for needle in [
        "104 bytes",
        "103-byte",
        "TMPDIR",
        "at most 67 bytes",
        "start the app with a TMPDIR",
    ] {
        assert!(message.contains(needle), "{needle} missing: {message}");
    }
    // TUIC_SOCKET is read only by clients (bridge/CLI); the listener ignores it,
    // so it must never be offered as the fix for a listener that cannot bind.
    assert!(
        !message.contains("set TUIC_SOCKET"),
        "advises a client-only env var as the listener fix: {message}"
    );
    // The 72-byte sandbox TMPDIR holds the primary with no headroom, never
    // the alternate.
    let sandbox = format!("/{}", "s".repeat(71));
    let primary = tuic_ipc::named_socket_path("ipc-cli-regression", Path::new(&sandbox));
    assert_eq!(primary.as_os_str().len(), 103);
    assert!(tuic_ipc::check_unix_socket_path(&primary).is_ok());
    let alternate =
        tuic_ipc::named_alternate_socket_path("ipc-cli-regression", Path::new(&sandbox), 4242);
    assert!(tuic_ipc::check_unix_socket_path(&alternate).is_err());
}

// Catches: the length being measured in chars (or lossy UTF-8) instead of the
// raw bytes sun_path holds, which would let a multi-byte or non-UTF-8 TMPDIR
// slip past the check and fail at bind().
#[cfg(unix)]
#[test]
fn unix_socket_path_check_counts_raw_bytes_including_non_utf8() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    // 52 two-byte chars = 104 bytes but only 52 chars.
    let multibyte = format!("/{}", "é".repeat(51));
    assert_eq!(multibyte.len(), 103);
    assert!(tuic_ipc::check_unix_socket_path(Path::new(&multibyte)).is_ok());
    let multibyte_over = format!("/{}", "é".repeat(52));
    assert_eq!(multibyte_over.len(), 105);
    assert!(tuic_ipc::check_unix_socket_path(Path::new(&multibyte_over)).is_err());
    // 0xFF is invalid UTF-8; a lossy conversion would turn it into a 3-byte U+FFFD.
    let mut raw = vec![b'/'];
    raw.extend(std::iter::repeat_n(0xFFu8, 103));
    let raw = OsString::from_vec(raw);
    assert_eq!(raw.len(), 104);
    assert!(tuic_ipc::check_unix_socket_path(Path::new(&raw)).is_err());
    let mut ok = vec![b'/'];
    ok.extend(std::iter::repeat_n(0xFFu8, 102));
    assert!(tuic_ipc::check_unix_socket_path(Path::new(&OsString::from_vec(ok))).is_ok());
}

// Catches: a path with no file name (or a name longer than the whole budget)
// panicking or underflowing the "longest TMPDIR" hint.
#[test]
fn unix_socket_path_check_hint_saturates_for_degenerate_paths() {
    let huge = format!("/{}", "n".repeat(200));
    let message = tuic_ipc::check_unix_socket_path(Path::new(&huge)).unwrap_err();
    assert!(message.contains("at most 0 bytes"), "{message}");
}
