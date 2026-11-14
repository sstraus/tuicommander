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
    // Remember the temp dir this process was given before it is redirected
    // below, so host_temp_dir() and nested processes still see it.
    if non_empty_env("TUIC_TEST_HOST_TMPDIR").is_none() {
        let host = without_trailing_separator(std::env::temp_dir());
        // SAFETY: this constructor runs before libtest creates worker threads.
        unsafe { std::env::set_var("TUIC_TEST_HOST_TMPDIR", host) };
    }
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
    fence_git_discovery(root);
}

/// Fail closed: no git process this test binary starts may discover a
/// repository above the scratch root. A fixture whose `git init` failed used to
/// fall through to the enclosing real checkout and rename or delete its
/// branches. See [`git_ceiling_directories`].
///
/// The host temp dir is fenced too: it holds the `t.XXXXXX` socket roots,
/// which live outside `root` to fit `sun_path`.
fn fence_git_discovery(root: &std::path::Path) {
    let root = std::path::absolute(root).expect("absolute test temp root");
    let existing = std::env::var_os("GIT_CEILING_DIRECTORIES");
    let ceiling = git_ceiling_directories(&root, existing.as_deref())
        .unwrap_or_else(|err| panic!("fence git discovery at {}: {err}", root.display()));
    let host = host_temp_dir();
    let ceiling = if host.is_absolute() {
        git_ceiling_with_dir(&host, Some(&ceiling))
            .unwrap_or_else(|err| panic!("fence git discovery at {}: {err}", host.display()))
    } else {
        ceiling
    };
    // SAFETY: called from the process constructor, before libtest starts threads.
    unsafe { std::env::set_var("GIT_CEILING_DIRECTORIES", ceiling) };
}

/// The `GIT_CEILING_DIRECTORIES` value that stops git's repository discovery
/// from climbing out of `root`: `existing` (kept verbatim, so a stricter value
/// set by an outer wrapper survives) plus `root`'s parent, spelled both
/// lexically and canonically, each added once.
///
/// Git skips any entry it cannot compare with its canonicalized cwd: relative
/// entries are ignored, and after an empty entry symlinks are no longer
/// resolved (macOS `/var` vs `/private/var`). Listing both spellings keeps the
/// fence up either way. The ceiling is the parent, not `root` itself, because
/// git still ascends from a cwd that *equals* a ceiling entry.
///
/// # Errors
///
/// `root` is relative or has no parent.
pub fn git_ceiling_directories(
    root: &std::path::Path,
    existing: Option<&std::ffi::OsStr>,
) -> Result<std::ffi::OsString, String> {
    let lexical = lexically_normal(root)?;
    let parent = lexical
        .parent()
        .ok_or_else(|| format!("{} has no parent", root.display()))?
        .to_path_buf();
    // The parent of the canonical root, not the canonical parent: when `root`
    // itself is a symlink, git compares against where it really lives.
    let canonical = std::fs::canonicalize(&lexical)
        .ok()
        .and_then(|real| real.parent().map(std::path::Path::to_path_buf));
    Ok(append_ceiling_entries(
        existing,
        std::iter::once(parent).chain(canonical),
    ))
}

/// `existing` plus `dir` itself (not its parent), lexically and canonically,
/// each added once: git may still discover a repository *below* `dir`, never
/// one at or above it. Used for directories that hold scratch next to the
/// test root rather than inside it (the host temp dir's socket roots).
///
/// # Errors
///
/// `dir` is relative.
pub fn git_ceiling_with_dir(
    dir: &std::path::Path,
    existing: Option<&std::ffi::OsStr>,
) -> Result<std::ffi::OsString, String> {
    let lexical = lexically_normal(dir)?;
    let canonical = std::fs::canonicalize(&lexical).ok();
    Ok(append_ceiling_entries(
        existing,
        std::iter::once(lexical).chain(canonical),
    ))
}

/// `path` with `.` and `..` resolved lexically; refuses a relative path, which
/// git would silently ignore as a ceiling entry.
fn lexically_normal(path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    use std::path::{Component, PathBuf};

    if !path.is_absolute() {
        return Err(format!("{} is not absolute", path.display()));
    }
    let mut lexical = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                lexical.pop();
            }
            other => lexical.push(other),
        }
    }
    Ok(lexical)
}

fn append_ceiling_entries(
    existing: Option<&std::ffi::OsStr>,
    entries: impl IntoIterator<Item = std::path::PathBuf>,
) -> std::ffi::OsString {
    let sep = if cfg!(windows) { ';' } else { ':' };
    let mut value = existing
        .map(|v| v.to_string_lossy().into_owned())
        .unwrap_or_default();
    for entry in entries {
        let entry = entry.to_string_lossy().into_owned();
        if entry.is_empty() || value.split(sep).any(|present| present == entry) {
            continue;
        }
        if !value.is_empty() {
            value.push(sep);
        }
        value.push_str(&entry);
    }
    value.into()
}

fn non_empty_env(key: &str) -> Option<std::path::PathBuf> {
    std::env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(|value| without_trailing_separator(value.into()))
}

/// `"/var/folders/…/T/"` and `"/var/folders/…/T"` are the same directory, but
/// only one spelling keeps the socket budget arithmetic exact.
fn without_trailing_separator(path: std::path::PathBuf) -> std::path::PathBuf {
    if path.as_os_str().len() > 1 {
        path.components().collect()
    } else {
        path
    }
}

/// This checkout's 16-hex-digit hash. FNV-1a (`tuic_checkout_hash` in
/// scripts/test-tmp-lib.sh computes the same): stable across Rust releases,
/// unlike `DefaultHasher`, so a toolchain bump never orphans the previous
/// run's per-checkout directories.
pub fn checkout_hash() -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in checkout_root().as_os_str().as_encoded_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// The temp directory this test process was handed before the constructor
/// pointed `TMPDIR`/`TMP`/`TEMP` at its per-run root: `TUIC_TEST_HOST_TMPDIR`
/// (exported by `scripts/with-test-tmp.sh`, the nextest setup script and the
/// constructor itself), else the OS temp dir. Never derived from `$HOME`.
pub fn host_temp_dir() -> std::path::PathBuf {
    non_empty_env("TUIC_TEST_HOST_TMPDIR")
        .unwrap_or_else(|| without_trailing_separator(std::env::temp_dir()))
}

/// Parent of every per-run test root: `TUIC_TEST_TMP_BASE` (opt-in, e.g.
/// `<checkout>/.tmp/tuic-tests` for the old in-checkout layout), else
/// `<host temp>/tuic-tests`.
pub fn test_base() -> std::path::PathBuf {
    non_empty_env("TUIC_TEST_TMP_BASE").unwrap_or_else(|| host_temp_dir().join("tuic-tests"))
}

/// Scratch space for Rust tests: `TUIC_TEST_TMP_ROOT` when a runner chose it,
/// else `<`[`test_base`]`>/tuic-co-<checkout hash>` — the same per-checkout
/// default `scripts/test-tmp-lib.sh` gives the shell entry points, and outside
/// the checkout, so `find_repo_root` from a fixture never walks up into it.
pub fn test_temp_root() -> std::path::PathBuf {
    let root = non_empty_env("TUIC_TEST_TMP_ROOT")
        .unwrap_or_else(|| test_base().join(format!("tuic-co-{}", checkout_hash())));
    std::fs::create_dir_all(&root).expect("create test temp root");
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

/// The longest socket path Rust's std binds or connects: one byte for the NUL
/// (a 104-byte path fails with "path must be shorter than SUN_LEN"). The one
/// definition is `tuic_ipc::UNIX_SOCKET_PATH_MAX`, which production code checks
/// named-instance sockets against; `scripts/test-tmp-lib.sh` reads the same
/// constant out of its source (`tuic_unix_socket_path_max`). Never restate it.
pub const SUN_PATH_USABLE: usize = tuic_ipc::UNIX_SOCKET_PATH_MAX;
/// macOS `sun_path` size, NUL included (Linux allows 108; the smaller wins).
pub const SUN_PATH_MAX: usize = SUN_PATH_USABLE + 1;
/// Socket roots are `<host temp>/t.XXXXXX`: this prefix plus
/// [`SOCKET_ROOT_RANDOM`] random characters.
pub const SOCKET_ROOT_PREFIX: &str = "t.";
pub const SOCKET_ROOT_RANDOM: usize = 6;
/// Per-test socket directories ([`socket_dir`]) are this many random
/// characters below the socket root.
pub const SOCKET_DIR_RANDOM: usize = 4;
/// The longest socket name a test binds inside a [`socket_dir`]:
/// `mcp-<pid>.sock` with a 7-digit Linux pid (`mcp-4194304.sock`).
pub const MAX_TEST_SOCKET_NAME: usize = 16;
/// Longest socket root that still holds `/<dir>/<name>`:
/// 103 − `/` − 4 − `/` − 16 = 81 bytes.
pub const MAX_SOCKET_ROOT_LEN: usize =
    SUN_PATH_USABLE - 1 - SOCKET_DIR_RANDOM - 1 - MAX_TEST_SOCKET_NAME;
/// Longest host temp dir a default socket root fits under:
/// 81 − `/` − `t.` − 6 = 72 bytes (the agent sandbox's `TMPDIR`, exactly).
pub const MAX_SOCKET_HOST_LEN: usize =
    MAX_SOCKET_ROOT_LEN - 1 - SOCKET_ROOT_PREFIX.len() - SOCKET_ROOT_RANDOM;
/// Marks a socket root this crate created; `scripts/with-test-tmp.sh` prunes
/// only stale `t.??????` directories that carry it.
pub const SOCKET_ROOT_MARKER: &str = ".tuic-socket-root";

/// mdkb's own staging socket, relative to the HOME a test gives it, with the
/// longest pid it budgets for (7 digits: Linux `pid_max`; macOS pids have 5).
pub const MDKB_STAGING_SOCKET: &str = ".mdkb/daemon-hook.sock.4194304.tmp";

/// Sockets whose NAME is fixed by something the tests cannot shorten (an
/// external tool's own scheme), as `(what, path relative to the per-test
/// socket dir)`. Unlike every socket this crate names itself, these can
/// overflow `sun_path` for a long host temp dir, so their tests go `UNCHECKED`
/// instead of running. `tests/socket_environment.rs` fails the run when that
/// would happen; add every new fixed-name socket test's name here.
pub const FIXED_NAME_SOCKETS: &[(&str, &str)] = &[("mdkb staging socket", MDKB_STAGING_SOCKET)];

/// The [`FIXED_NAME_SOCKETS`] that do not fit `sun_path` below `home` (the
/// per-test socket dir a test runs them in), each as a message naming its
/// length and the budget. Empty when every one fits.
pub fn fixed_name_sockets_over_budget(home: &std::path::Path) -> Vec<String> {
    FIXED_NAME_SOCKETS
        .iter()
        .filter_map(|(what, relative)| {
            unix_socket_path_fits(&home.join(relative))
                .err()
                .map(|reason| format!("{what}: {reason}"))
        })
        .collect()
}

/// Whether `root` leaves room for every socket the tests bind below it.
pub fn socket_root_fits(root: &std::path::Path) -> bool {
    root.as_os_str().len() <= MAX_SOCKET_ROOT_LEN
}

/// `Ok` when `path` is short enough to bind or connect as a Unix socket,
/// else a message naming its length and the budget.
pub fn unix_socket_path_fits(path: &std::path::Path) -> Result<(), String> {
    let len = path.as_os_str().len();
    if len <= SUN_PATH_USABLE {
        Ok(())
    } else {
        Err(format!(
            "{} is {len} bytes, over the {SUN_PATH_USABLE}-byte Unix socket path budget \
             ({SUN_PATH_MAX}-byte sun_path − NUL) by {}",
            path.display(),
            len - SUN_PATH_USABLE
        ))
    }
}

/// For a test whose socket path is fixed by something it cannot shorten (an
/// external tool's own socket name, a production naming scheme): `true` when
/// `path` fits; otherwise prints an `UNCHECKED` line naming the reason and the
/// `TUIC_TEST_SOCKET_ROOT` override, and returns `false` so the test can return
/// early. Never silent: the line goes to stdout and stderr.
pub fn socket_test_can_run(test: &str, path: &std::path::Path) -> bool {
    match unix_socket_path_fits(path) {
        Ok(()) => true,
        Err(reason) => {
            let line = format!(
                "UNCHECKED {test}: {reason}. The host temp dir {} is {} bytes; set \
                 TUIC_TEST_SOCKET_ROOT=<short private dir> or a shorter TMPDIR to run it.",
                host_temp_dir().display(),
                host_temp_dir().as_os_str().len()
            );
            println!("{line}");
            eprintln!("{line}");
            false
        }
    }
}

#[cfg(unix)]
static PROBES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Make `dir` usable as a socket root, or say why it is not. A `private` root
/// (one this crate creates) must be a real directory owned by us, mode 0700:
/// a symlink or a directory owned by another user is refused, never followed.
#[cfg(unix)]
pub fn prepare_socket_root(dir: &std::path::Path, private: bool) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if !socket_root_fits(dir) {
        return Err(format!(
            "{} bytes, over the {MAX_SOCKET_ROOT_LEN}-byte socket-root budget",
            dir.as_os_str().len()
        ));
    }
    if !private {
        std::fs::create_dir_all(dir).map_err(|error| format!("cannot create: {error}"))?;
    }
    let meta = std::fs::symlink_metadata(dir).map_err(|error| format!("cannot stat: {error}"))?;
    if private && meta.file_type().is_symlink() {
        return Err("is a symlink".to_owned());
    }
    if !std::fs::metadata(dir).is_ok_and(|meta| meta.is_dir()) {
        return Err("is not a directory".to_owned());
    }
    // A file this process just created carries its effective uid, so the
    // probe both proves the directory is writable and tells us who we are.
    let probe = dir.join(format!(
        ".tuic-probe-{}-{}",
        std::process::id(),
        PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&probe, b"").map_err(|error| format!("not writable: {error}"))?;
    let own_uid = std::fs::symlink_metadata(&probe).map(|probe| probe.uid());
    let _ = std::fs::remove_file(&probe);
    let own_uid = own_uid.map_err(|error| format!("cannot stat probe: {error}"))?;
    if private {
        if meta.uid() != own_uid {
            return Err(format!("owned by uid {}, not {own_uid}", meta.uid()));
        }
        if meta.mode() & 0o077 != 0 {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|error| format!("cannot make private: {error}"))?;
        }
    }
    Ok(())
}

/// `n` random lowercase alphanumerics from std's per-process random hash keys
/// (no `rand`/`tempfile` dependency in this crate).
fn random_name(n: usize) -> String {
    use std::hash::{BuildHasher, Hasher};
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    static CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    hasher.write_u32(std::process::id());
    if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        hasher.write_u128(now.as_nanos());
    }
    let mut bits = hasher.finish();
    (0..n)
        .map(|_| {
            let index = (bits % ALPHABET.len() as u64) as usize;
            bits /= ALPHABET.len() as u64;
            ALPHABET[index] as char
        })
        .collect()
}

/// Create a fresh, private (0700) directory `parent/<prefix><random>`. `mkdir`
/// never follows a symlink at the final component and fails on any existing
/// entry, so a name another user planted is skipped, not reused.
#[cfg(unix)]
fn create_private_dir(
    parent: &std::path::Path,
    prefix: &str,
    random: usize,
) -> Result<std::path::PathBuf, String> {
    use std::os::unix::fs::DirBuilderExt;
    let mut last = String::new();
    for _ in 0..16 {
        let dir = parent.join(format!("{prefix}{}", random_name(random)));
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => return Ok(dir),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                last = format!("{} exists", dir.display());
            }
            Err(error) => return Err(format!("cannot create {}: {error}", dir.display())),
        }
    }
    Err(format!("no free name after 16 tries ({last})"))
}

/// Socket roots this process created, by host temp dir, removed at exit.
#[cfg(unix)]
static CREATED_SOCKET_ROOTS: std::sync::Mutex<Vec<(std::path::PathBuf, std::path::PathBuf)>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(unix)]
#[ctor::dtor]
fn remove_created_socket_roots() {
    let roots = CREATED_SOCKET_ROOTS
        .lock()
        .map(|mut roots| std::mem::take(&mut *roots))
        .unwrap_or_default();
    for (_, root) in roots {
        // Only our own marked directory: never a symlink, never unmarked.
        let is_ours = std::fs::symlink_metadata(&root).is_ok_and(|meta| meta.is_dir())
            && root.join(SOCKET_ROOT_MARKER).is_file();
        if is_ours {
            let _ = std::fs::remove_dir_all(&root);
        }
    }
}

#[cfg(unix)]
fn over_budget_message(host: &std::path::Path) -> String {
    let host_len = host.as_os_str().len();
    let root_len = host_len + 1 + SOCKET_ROOT_PREFIX.len() + SOCKET_ROOT_RANDOM;
    format!(
        "no Unix-socket test root: the host temp dir {} is {host_len} bytes, so \
         `<host>/{SOCKET_ROOT_PREFIX}XXXXXX` would be {root_len} bytes; a socket root \
         must be at most {MAX_SOCKET_ROOT_LEN} bytes ({SUN_PATH_USABLE}-byte usable \
         sun_path − `/` − {SOCKET_DIR_RANDOM}-char per-test dir − `/` − \
         {MAX_TEST_SOCKET_NAME}-byte socket name), i.e. a host temp dir of at most \
         {MAX_SOCKET_HOST_LEN} bytes. There is no shared-temp-dir fallback. Set \
         TUIC_TEST_SOCKET_ROOT=<short, private, writable dir of at most \
         {MAX_SOCKET_ROOT_LEN} bytes> or run with a shorter TMPDIR.",
        host.display()
    )
}

/// The socket root for host temp dir `host`: a private `t.XXXXXX` this process
/// creates there once (marked, removed at exit), or a message saying why none
/// fits.
#[cfg(unix)]
pub fn socket_root_under(host: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let mut created = CREATED_SOCKET_ROOTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((_, root)) = created.iter().find(|(seen, _)| seen == host) {
        return Ok(root.clone());
    }
    if host.as_os_str().len() > MAX_SOCKET_HOST_LEN {
        return Err(over_budget_message(host));
    }
    let root =
        create_private_dir(host, SOCKET_ROOT_PREFIX, SOCKET_ROOT_RANDOM).map_err(|reason| {
            format!(
                "no Unix-socket test root under {}: {reason}",
                host.display()
            )
        })?;
    if let Err(reason) = prepare_socket_root(&root, true).and_then(|()| {
        std::fs::write(root.join(SOCKET_ROOT_MARKER), b"")
            .map_err(|error| format!("cannot write marker: {error}"))
    }) {
        let _ = std::fs::remove_dir_all(&root);
        return Err(format!("{}: {reason}", root.display()));
    }
    created.push((host.to_path_buf(), root.clone()));
    Ok(root)
}

#[cfg(unix)]
fn resolve_socket_root() -> Result<std::path::PathBuf, String> {
    if let Some(explicit) = non_empty_env("TUIC_TEST_SOCKET_ROOT") {
        return prepare_socket_root(&explicit, false)
            .map(|()| explicit.clone())
            .map_err(|reason| {
                format!(
                    "no Unix-socket test root: TUIC_TEST_SOCKET_ROOT is set but unusable: \
                     {}: {reason}. It must be a writable directory of at most \
                     {MAX_SOCKET_ROOT_LEN} bytes ({SUN_PATH_USABLE}-byte usable sun_path − \
                     `/` − {SOCKET_DIR_RANDOM}-char per-test dir − `/` − \
                     {MAX_TEST_SOCKET_NAME}-byte socket name).",
                    explicit.display()
                )
            });
    }
    socket_root_under(&host_temp_dir())
}

/// The Unix-domain socket root for this test process: `TUIC_TEST_SOCKET_ROOT`
/// when set (the one override; used as is), else a private, random
/// `<host temp>/t.XXXXXX` (mode 0700, created once per process, removed at
/// exit). Never `/tmp`, never the per-run test root (too long), never `$HOME`.
/// Panics naming the budget, the lengths and `TUIC_TEST_SOCKET_ROOT` when the
/// host temp dir is too long to hold one.
#[cfg(unix)]
pub fn short_socket_test_temp_root() -> std::path::PathBuf {
    resolve_socket_root().unwrap_or_else(|message| panic!("{message}"))
}

/// `name` directly inside [`short_socket_test_temp_root`], asserted to fit
/// `sun_path`. The root is private to this process, so names need no pid.
#[cfg(unix)]
pub fn short_socket_path(name: &str) -> std::path::PathBuf {
    let path = short_socket_test_temp_root().join(name);
    if let Err(reason) = unix_socket_path_fits(&path) {
        panic!("{reason}");
    }
    path
}

/// A private per-test directory for sockets, removed on drop.
#[cfg(unix)]
pub struct SocketDir(std::path::PathBuf);

#[cfg(unix)]
impl SocketDir {
    pub fn path(&self) -> &std::path::Path {
        &self.0
    }
}

#[cfg(unix)]
impl Drop for SocketDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A fresh `<socket root>/xxxx` directory: any socket name of up to
/// [`MAX_TEST_SOCKET_NAME`] bytes inside it fits `sun_path`.
#[cfg(unix)]
pub fn socket_dir() -> SocketDir {
    let root = short_socket_test_temp_root();
    let dir = create_private_dir(&root, "", SOCKET_DIR_RANDOM)
        .unwrap_or_else(|reason| panic!("per-test socket dir: {reason}"));
    SocketDir(dir)
}

/// Serialises every working-directory change [`in_dir`] makes in this process.
static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Run `f` with the working directory set to `dir`, then restore it, holding a
/// process-wide lock. For binding/connecting a Unix socket by a RELATIVE name
/// when its absolute path cannot fit `sun_path`.
///
/// The working directory is process-wide: a sibling test thread that resolves
/// a relative path or spawns a child during `f` would see `dir`. So this
/// refuses (panics) unless the process runs one test at a time — under
/// `cargo nextest` (`NEXTEST_RUN_ID`, one process per test) or libtest with
/// `RUST_TEST_THREADS=1` / `--test-threads=1`.
pub fn in_dir<R>(dir: &std::path::Path, f: impl FnOnce() -> R) -> R {
    let one_test_per_process = std::env::var_os("NEXTEST_RUN_ID").is_some()
        || std::env::var("RUST_TEST_THREADS").is_ok_and(|threads| threads == "1");
    assert!(
        one_test_per_process,
        "in_dir changes the process-wide working directory; run this test under \
         `cargo nextest` (one process per test) or with RUST_TEST_THREADS=1"
    );
    let _lock = CWD_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    struct Restore(std::path::PathBuf);
    impl Drop for Restore {
        fn drop(&mut self) {
            std::env::set_current_dir(&self.0).expect("restore the working directory");
        }
    }
    let _restore = Restore(std::env::current_dir().expect("read the working directory"));
    std::env::set_current_dir(dir).expect("enter the socket directory");
    f()
}

/// How a test and its children should spell sockets in one directory.
#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketSpelling {
    /// `dir/name` fits `sun_path`.
    Absolute,
    /// It does not: bind/connect `name` with the working directory at `dir`.
    Relative,
}

#[cfg(unix)]
impl SocketSpelling {
    /// The longest name the caller will use in `dir` decides the spelling.
    pub fn for_dir(dir: &std::path::Path, longest_name: &str) -> Self {
        if unix_socket_path_fits(&dir.join(longest_name)).is_ok() {
            Self::Absolute
        } else {
            Self::Relative
        }
    }

    /// The temp dir to hand a child whose cwd is `dir` (`TMPDIR=.` resolves
    /// `temp_dir().join(name)` to `./name`).
    pub fn temp_dir(self, dir: &std::path::Path) -> std::path::PathBuf {
        match self {
            Self::Absolute => dir.to_path_buf(),
            Self::Relative => std::path::PathBuf::from("."),
        }
    }

    /// `name` as a child whose cwd is `dir` should spell it.
    pub fn path(self, dir: &std::path::Path, name: &str) -> std::path::PathBuf {
        self.temp_dir(dir).join(name)
    }

    /// Bind `dir/name`, through [`in_dir`] when the absolute path cannot fit.
    pub fn bind(
        self,
        dir: &std::path::Path,
        name: &str,
    ) -> std::io::Result<std::os::unix::net::UnixListener> {
        match self {
            Self::Absolute => std::os::unix::net::UnixListener::bind(dir.join(name)),
            Self::Relative => in_dir(dir, || std::os::unix::net::UnixListener::bind(name)),
        }
    }
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

#[cfg(test)]
mod ceiling_tests {
    use super::git_ceiling_directories;
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};

    const SEP: &str = if cfg!(windows) { ";" } else { ":" };

    fn entries(value: &OsStr) -> Vec<PathBuf> {
        value
            .to_str()
            .unwrap()
            .split(SEP)
            .map(PathBuf::from)
            .collect()
    }

    /// A `git` in `cwd` with the given ceiling (`None`: no ceiling at all).
    fn toplevel(cwd: &Path, ceiling: Option<&OsStr>) -> Result<PathBuf, String> {
        let mut cmd = std::process::Command::new("git");
        cmd.args(["rev-parse", "--show-toplevel"]).current_dir(cwd);
        match ceiling {
            Some(value) => cmd.env("GIT_CEILING_DIRECTORIES", value),
            None => cmd.env_remove("GIT_CEILING_DIRECTORIES"),
        };
        let out = cmd.output().expect("run git rev-parse");
        if out.status.success() {
            Ok(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
        } else {
            Err(String::from_utf8_lossy(&out.stderr).into_owned())
        }
    }

    #[cfg(unix)]
    #[test]
    fn ceiling_lists_the_parent_lexically_and_canonically() {
        let dir = tempfile::tempdir_in(super::test_temp_root()).unwrap();
        let real = dir.path().canonicalize().unwrap().join("real");
        std::fs::create_dir_all(real.join("root")).unwrap();
        let link = dir.path().canonicalize().unwrap().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let value = git_ceiling_directories(&link.join("root"), None).unwrap();
        assert_eq!(entries(&value), vec![link.clone(), real.clone()]);

        // Trailing slash and `.`/`..` segments are normalized away.
        let messy = PathBuf::from(format!("{}/./x/../root/", link.display()));
        assert_eq!(
            git_ceiling_directories(&messy, None).unwrap(),
            value,
            "a non-normalized root spells the same ceiling"
        );
    }

    #[test]
    fn ceiling_keeps_an_existing_value_and_adds_each_entry_once() {
        let dir = tempfile::tempdir_in(super::test_temp_root()).unwrap();
        let parent = dir.path().canonicalize().unwrap();
        let root = parent.join("root");
        std::fs::create_dir_all(&root).unwrap();
        let outer = format!("{SEP}/stricter/outer");
        let value = git_ceiling_directories(&root, Some(OsStr::new(&outer))).unwrap();
        assert_eq!(
            value.to_str().unwrap(),
            format!("{outer}{SEP}{}", parent.display())
        );
        // Re-applying it (nested wrapper, then the constructor) changes nothing.
        assert_eq!(git_ceiling_directories(&root, Some(&value)).unwrap(), value);
    }

    #[test]
    fn ceiling_rejects_a_relative_or_parentless_root() {
        assert!(git_ceiling_directories(Path::new("relative/root"), None).is_err());
        assert!(git_ceiling_directories(Path::new("/"), None).is_err());
    }

    /// The hazard itself: an outer repository, a test root inside it, and a
    /// scratch dir whose fixture repo was never created. Without the ceiling,
    /// git in the scratch dir silently operates on the outer repository.
    #[test]
    fn ceiling_stops_git_from_finding_a_repository_above_the_root() {
        let dir = tempfile::tempdir_in(super::test_temp_root()).unwrap();
        let outer = dir.path().canonicalize().unwrap().join("outer");
        let root = outer.join("sub/root");
        let scratch = root.join("x");
        std::fs::create_dir_all(&scratch).unwrap();
        let init = std::process::Command::new("git")
            .args(["init", "-q", "--template="])
            .current_dir(&outer)
            .status()
            .unwrap();
        assert!(init.success(), "git init {}", outer.display());

        assert_eq!(
            toplevel(&scratch, None).as_deref(),
            Ok(outer.as_path()),
            "control: without a ceiling git climbs to the outer repository"
        );
        let ceiling = git_ceiling_directories(&root, None).unwrap();
        let err = toplevel(&scratch, Some(&ceiling)).expect_err("git escaped the test root");
        assert!(err.contains("not a git repository"), "{err}");
        // A fixture repo below the root is still found.
        let fixture = root.join("fixture");
        std::fs::create_dir_all(fixture.join("deep")).unwrap();
        let init = std::process::Command::new("git")
            .args(["init", "-q", "--template="])
            .current_dir(&fixture)
            .status()
            .unwrap();
        assert!(init.success());
        assert_eq!(
            toplevel(&fixture.join("deep"), Some(&ceiling)).as_deref(),
            Ok(fixture.as_path())
        );
    }

    /// The constructor fences this very process: git started in a repo-less
    /// dir under the test root finds nothing, even when that root sits inside
    /// a real checkout (the opt-in `TUIC_TEST_TMP_BASE=<checkout>/.tmp/tuic-tests`).
    #[test]
    fn git_in_a_test_process_cannot_leave_the_test_root() {
        let dir = tempfile::tempdir_in(super::test_temp_root()).unwrap();
        let inherited = std::env::var_os("GIT_CEILING_DIRECTORIES");
        let err = toplevel(dir.path(), inherited.as_deref())
            .expect_err("git found a repository above the test root");
        assert!(err.contains("not a git repository"), "{err}");
        let parent = super::test_temp_root().canonicalize().unwrap();
        let parent = parent.parent().unwrap();
        assert!(
            entries(inherited.as_deref().expect("constructor sets a ceiling"))
                .iter()
                .any(|entry| entry == parent),
            "ceiling must name the test root's parent {}",
            parent.display()
        );
        // The socket roots' home, the host temp dir, is fenced as well.
        let host = super::host_temp_dir();
        assert!(
            entries(inherited.as_deref().unwrap()).contains(&host),
            "ceiling must name the host temp dir {}",
            host.display()
        );
    }

    #[test]
    fn ceiling_with_dir_lists_the_dir_itself_once() {
        let dir = tempfile::tempdir_in(super::test_temp_root()).unwrap();
        let real = dir.path().canonicalize().unwrap();
        let value = super::git_ceiling_with_dir(&real, None).unwrap();
        assert_eq!(entries(&value), vec![real.clone()]);
        assert_eq!(
            super::git_ceiling_with_dir(&real, Some(&value)).unwrap(),
            value
        );
        assert!(super::git_ceiling_with_dir(Path::new("relative"), None).is_err());
    }
}
