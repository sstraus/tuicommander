//! Adversarial receiver/sender tests for the remote drop transfer (story 1434-1719).
use super::*;
#[cfg(unix)]
use std::sync::atomic::{AtomicUsize, Ordering};

fn scratch() -> tempfile::TempDir {
    tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap()
}

// Catches: a detached extraction worker leaking staging or its upload slot when
// a corrupt archive fails after the HTTP handler has already been cancelled.
#[tokio::test]
async fn cancelled_handler_with_corrupt_archive_cleans_staging_and_releases_slot() {
    let tmp = scratch();
    let root = tmp.path().to_path_buf();
    let roots = vec![root.to_str().unwrap().to_owned()];
    let request = query(&root, "file", false);
    let mut tar = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(5);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, "file", &b"bytes"[..]).unwrap();
    let mut archive = tar.into_inner().unwrap();
    archive.truncate(128); // An interrupted copy of a real tar header.
    let (started, waiting) = tokio::sync::oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let (finished, completion) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        receive_copy_with_extractor(request, &roots, Body::from(archive), move |stage, query| {
            let _ = started.send(());
            blocked.recv().map_err(io::Error::other)?;
            let result = extract_and_publish(stage, query);
            let _ = finished.send(result.is_err());
            result
        })
        .await
    });
    waiting.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    release.send(()).unwrap();
    assert!(completion.await.unwrap(), "corrupt archive was accepted");
    let slots = UPLOAD_SLOTS.acquire_many(2).await.unwrap();
    assert!(names(&root).is_empty(), "failed worker left files behind");
    drop(slots);
}

/// One raw tar member. The `tar` builder refuses `..`/absolute names, so the
/// header bytes are written by hand, exactly as a hostile client would.
fn raw_member(name: &[u8], kind: u8, link: &[u8], declared_size: u64, body: &[u8]) -> Vec<u8> {
    raw_member_mode(name, kind, link, declared_size, body, 0o644)
}

fn raw_member_mode(
    name: &[u8],
    kind: u8,
    link: &[u8],
    declared_size: u64,
    body: &[u8],
    mode: u32,
) -> Vec<u8> {
    let mut header = tar::Header::new_gnu();
    {
        let old = header.as_old_mut();
        old.name[..name.len()].copy_from_slice(name);
        old.linkname[..link.len()].copy_from_slice(link);
    }
    header.set_entry_type(tar::EntryType::new(kind));
    header.set_size(declared_size);
    header.set_mode(mode);
    header.set_cksum();
    let mut out = header.as_bytes().to_vec();
    out.extend_from_slice(body);
    out.resize(out.len().next_multiple_of(512), 0);
    out
}

fn finish(mut members: Vec<u8>) -> Vec<u8> {
    members.extend_from_slice(&[0u8; 1024]);
    members
}

fn file(name: &str, body: &[u8]) -> Vec<u8> {
    raw_member(name.as_bytes(), b'0', b"", body.len() as u64, body)
}

fn dir(name: &str) -> Vec<u8> {
    raw_member_mode(name.as_bytes(), b'5', b"", 0, b"", 0o755)
}

fn query(dest: &Path, name: &str, directory: bool) -> UploadQuery {
    UploadQuery {
        dest_dir: dest.to_str().unwrap().into(),
        name: name.into(),
        directory,
    }
}

fn names(path: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

async fn upload(
    root: &Path,
    dest: &Path,
    name: &str,
    directory: bool,
    archive: Vec<u8>,
) -> Result<TransferResult, String> {
    receive_copy(
        query(dest, name, directory),
        &[root.to_str().unwrap().to_owned()],
        Body::from(archive),
    )
    .await
}

// Catches: a hostile tar member (traversal, absolute, symlink, hardlink, device,
// FIFO, sibling top-level name) escaping the staging tree, or a rejected archive
// leaving a half-published name or a `.tuic-upload-*` directory behind.
#[tokio::test]
async fn hostile_archives_publish_nothing_and_leave_no_staging() {
    let cases: Vec<(&str, Vec<u8>, bool)> = vec![
        ("dotdot", file("x/../../escape", b"p"), false),
        ("dotdot_in_dir", file("x/../../escape", b"p"), true),
        ("absolute", file("/tmp/tuic-critic-absolute", b"p"), false),
        ("symlink", raw_member(b"x", b'2', b"/etc", 0, b""), false),
        (
            "hardlink",
            raw_member(b"x", b'1', b"/etc/hostname", 0, b""),
            false,
        ),
        ("chardev", raw_member(b"x", b'3', b"", 0, b""), false),
        ("fifo", raw_member(b"x", b'6', b"", 0, b""), false),
        ("sparse", raw_member(b"x", b'S', b"", 0, b""), false),
        ("pax_global", raw_member(b"x", b'g', b"", 0, b""), false),
        (
            "sibling",
            [file("x", b"a"), file("y", b"b")].concat(),
            false,
        ),
        (
            "prefix_sibling",
            [dir("x"), file("xy/f", b"b")].concat(),
            true,
        ),
        (
            "dup_entry",
            [file("x", b"a"), file("x", b"b")].concat(),
            false,
        ),
        ("backslash", file("x\\..\\y", b"a"), false),
        ("file_for_dir", file("x", b"a"), true),
        ("dir_for_file", dir("x"), false),
        ("empty_archive", Vec::new(), false),
        (
            "link_then_write_through",
            [
                dir("x"),
                raw_member(b"x/l", b'2', b"..", 0, b""),
                file("x/l/pwn", b"p"),
            ]
            .concat(),
            true,
        ),
    ];
    for (label, members, directory) in cases {
        let tmp = scratch();
        let root = tmp.path().join("repo");
        std::fs::create_dir(&root).unwrap();
        let outside = tmp.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        let outcome = upload(&root, &root, "x", directory, finish(members)).await;
        assert!(
            outcome.is_err(),
            "{label}: hostile archive must be rejected, got {:?}",
            outcome.map(|r| (r.moved, r.skipped))
        );
        assert_eq!(
            names(&root),
            Vec::<String>::new(),
            "{label}: residue in dest"
        );
        assert_eq!(names(&outside), Vec::<String>::new(), "{label}: escaped");
        assert_eq!(
            names(tmp.path()),
            vec!["outside", "repo"],
            "{label}: escaped"
        );
        assert!(!Path::new("/tmp/tuic-critic-absolute").exists(), "{label}");
    }
}

// Catches: the destination handle following a symlinked directory out of the
// registered root (open_dir without cap-std confinement), or an upload target that
// is a dangling symlink being written through.
#[tokio::test]
#[cfg(unix)]
async fn symlinked_destination_and_target_do_not_escape_the_root() {
    let tmp = scratch();
    let root = tmp.path().join("repo");
    let outside = tmp.path().join("outside");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
    std::os::unix::fs::symlink(outside.join("fresh"), root.join("dangling")).unwrap();
    std::os::unix::fs::symlink("../outside", root.join("rel_escape")).unwrap();

    for via in ["escape", "rel_escape"] {
        let out = upload(&root, &root.join(via), "f", false, finish(file("f", b"p"))).await;
        assert!(out.is_err(), "{via} must not be followed");
        assert_eq!(
            names(&outside),
            Vec::<String>::new(),
            "{via} leaked a write"
        );
    }
    let out = upload(
        &root,
        &root,
        "dangling",
        false,
        finish(file("dangling", b"p")),
    )
    .await;
    assert!(
        out.is_err(),
        "symlink target must be refused, not written through"
    );
    assert!(!outside.join("fresh").exists());
    // A symlink pointing inside the root is legitimate and stays usable.
    std::fs::create_dir(root.join("real")).unwrap();
    std::os::unix::fs::symlink("real", root.join("alias")).unwrap();
    let out = upload(
        &root,
        &root.join("alias"),
        "f",
        false,
        finish(file("f", b"p")),
    )
    .await;
    assert_eq!(out.unwrap().moved, 1);
    assert!(root.join("real/f").exists());
}

// Catches: destDir/name validation accepting a spelling that leaves the root:
// relative paths, `..`, a sibling sharing the root's string prefix, NUL, or a
// multi-component / dot name.
#[tokio::test]
async fn invalid_destinations_and_names_are_rejected() {
    let tmp = scratch();
    let root = tmp.path().join("repo");
    let sibling = tmp.path().join("repo2");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&sibling).unwrap();
    let r = root.to_str().unwrap().to_owned();
    for (dest, name) in [
        (sibling.to_str().unwrap().to_owned(), "f"),
        (format!("{r}/../repo2"), "f"),
        ("repo".to_owned(), "f"),
        (format!("{r}\0"), "f"),
        (r.clone(), ".."),
        (r.clone(), "."),
        (r.clone(), ""),
        (r.clone(), "a/b"),
        (r.clone(), "/abs"),
        (r.clone(), "a\0b"),
        (r.clone(), "a\\b"),
        (r.clone(), "C:evil"),
    ] {
        let out = receive_copy(
            UploadQuery {
                dest_dir: dest.clone(),
                name: name.into(),
                directory: false,
            },
            std::slice::from_ref(&r),
            Body::from(finish(file("f", b"p"))),
        )
        .await;
        assert!(out.is_err(), "dest={dest:?} name={name:?} accepted");
    }
    assert_eq!(names(&root), Vec::<String>::new());
    assert_eq!(names(&sibling), Vec::<String>::new());
}

// Catches: an existing name being replaced or the skip leaving staging behind;
// an existing *empty* directory being replaced by rename(2) semantics.
#[tokio::test]
async fn existing_name_is_skipped_and_publish_never_replaces() {
    let tmp = scratch();
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("f"), b"original").unwrap();
    let out = upload(&root, &root, "f", false, finish(file("f", b"new")))
        .await
        .unwrap();
    assert_eq!((out.moved, out.skipped), (0, 1));
    assert_eq!(std::fs::read(root.join("f")).unwrap(), b"original");
    assert_eq!(names(&root), vec!["f"]);

    // publish() itself, with the conflict appearing after the pre-check (the TOCTOU).
    let from = root.join("from");
    std::fs::create_dir_all(from.join("d")).unwrap();
    std::fs::write(from.join("d/new"), b"n").unwrap();
    std::fs::create_dir(root.join("d")).unwrap(); // empty destination directory
    let a = Dir::open_ambient_dir(&from, cap_std::ambient_authority()).unwrap();
    let b = Dir::open_ambient_dir(&root, cap_std::ambient_authority()).unwrap();
    let err = publish(&a, &b, "d").unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
    assert!(from.join("d/new").exists(), "source must stay in staging");
    assert!(
        names(&root.join("d")).is_empty(),
        "empty dir must not be replaced"
    );
}

// Catches: a dropped connection (handler future cancelled mid-body) leaking the
// `.tuic-upload-*` staging directory and its partial archive on the registered repo.
#[tokio::test]
async fn cancelled_upload_removes_its_staging_directory() {
    let tmp = scratch();
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).unwrap();
    let stream = futures_util::stream::iter([Ok::<_, io::Error>(vec![1u8; 4096])])
        .chain(futures_util::stream::pending());
    let roots = [root.to_str().unwrap().to_owned()];
    let fut = receive_copy(query(&root, "f", false), &roots, Body::from_stream(stream));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(300), fut)
            .await
            .is_err()
    );
    assert_eq!(names(&root), Vec::<String>::new());

    // A transport error mid-body takes the same cleanup path.
    let stream = futures_util::stream::iter([
        Ok::<_, io::Error>(vec![1u8; 4096]),
        Err(io::Error::other("reset")),
    ]);
    let out = receive_copy(
        query(&root, "f", false),
        &[root.to_str().unwrap().to_owned()],
        Body::from_stream(stream),
    )
    .await;
    assert!(out.is_err());
    assert_eq!(names(&root), Vec::<String>::new());
}

// Catches: the 256 MiB body cap missing/off-by-chunk, or exceeding it leaving the
// partial archive on disk.
#[tokio::test]
async fn oversized_body_is_cut_off_and_cleaned() {
    let tmp = scratch();
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).unwrap();
    let chunk = vec![0u8; 1024 * 1024];
    let stream =
        futures_util::stream::iter((0..257).map(move |_| Ok::<_, io::Error>(chunk.clone())));
    let out = receive_copy(
        query(&root, "f", false),
        &[root.to_str().unwrap().to_owned()],
        Body::from_stream(stream),
    )
    .await;
    assert!(out.unwrap_err().contains("256 MiB"));
    assert_eq!(names(&root), Vec::<String>::new());
}

// Catches: a header that *declares* a huge size (tar bomb / sparse-style lie) being
// trusted for allocation or counted wrongly, the 10 000-entry cap off by one,
// and directory fixtures without search permission leaking the temporary tree.
#[tokio::test]
async fn declared_size_and_entry_count_limits_hold() {
    let tmp = scratch();
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).unwrap();
    let huge = raw_member(b"x", b'0', b"", 300 * 1024 * 1024, b"");
    let out = upload(&root, &root, "x", false, huge).await;
    assert!(out.is_err());
    let lie = raw_member(b"x", b'0', b"", u64::MAX / 2, b"");
    assert!(
        upload(&root, &root, "x", false, [lie.clone(), lie].concat())
            .await
            .is_err()
    );
    assert_eq!(names(&root), Vec::<String>::new());

    let build = |n: usize| {
        let mut members = dir("x");
        for i in 1..n {
            members.extend(dir(&format!("x/d{i}")));
        }
        finish(members)
    };
    let out = upload(&root, &root, "x", true, build(10_001)).await;
    assert!(out.is_err(), "10001 entries must be refused");
    assert_eq!(names(&root), Vec::<String>::new());
    let out = upload(&root, &root, "x", true, build(10_000))
        .await
        .unwrap();
    assert_eq!(out.moved, 1, "exactly 10000 entries is within the cap");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let published = root.join("x");
        let mut all_searchable =
            std::fs::metadata(&published).unwrap().permissions().mode() & 0o100 != 0;
        // Restore access before inspecting children so even the RED run cleans up.
        std::fs::set_permissions(&published, std::fs::Permissions::from_mode(0o755)).unwrap();
        for entry in std::fs::read_dir(&published).unwrap() {
            let entry = entry.unwrap();
            all_searchable &= entry.metadata().unwrap().permissions().mode() & 0o100 != 0;
            std::fs::set_permissions(entry.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let temporary_path = tmp.path().to_path_buf();
        tmp.close().unwrap();
        assert!(
            !temporary_path.exists(),
            "temporary transfer tree survived cleanup"
        );
        assert!(
            all_searchable,
            "directory fixtures lack owner search permission"
        );
    }
}

// Catches: path-length limit missing (long GNU name accepted / leaks an OS error
// naming the host path).
#[tokio::test]
async fn overlong_member_path_is_rejected_without_leaking_host_paths() {
    let tmp = scratch();
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).unwrap();
    let mut b = tar::Builder::new(Vec::new());
    let mut h = tar::Header::new_gnu();
    h.set_size(1);
    h.set_mode(0o644);
    h.set_cksum();
    b.append_data(&mut h, format!("x/{}", "a".repeat(5000)), &b"p"[..])
        .unwrap();
    let out = upload(&root, &root, "x", true, b.into_inner().unwrap()).await;
    let err = out.unwrap_err();
    assert!(
        !err.contains(root.to_str().unwrap()),
        "leaked host path: {err}"
    );
    assert_eq!(names(&root), Vec::<String>::new());
}

// Catches: the 2-slot concurrency limit being bypassed or a refused upload still
// creating a staging directory.
#[tokio::test]
#[serial_test::serial]
async fn third_concurrent_upload_is_refused_without_side_effects() {
    let tmp = scratch();
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).unwrap();
    let a = UPLOAD_SLOTS.try_acquire().unwrap();
    let b = UPLOAD_SLOTS.try_acquire().unwrap();
    let out = upload(&root, &root, "f", false, finish(file("f", b"p"))).await;
    assert!(out.unwrap_err().contains("busy"));
    assert_eq!(names(&root), Vec::<String>::new());
    drop((a, b));
    assert_eq!(
        upload(&root, &root, "f", false, finish(file("f", b"p")))
            .await
            .unwrap()
            .moved,
        1
    );
}

// Catches: a legitimate POSIX filename (colon, backslash) failing the whole upload
// because the validator applies Windows rules to a Linux receiver.
#[cfg(unix)]
#[tokio::test]
async fn posix_names_with_colon_or_backslash_upload() {
    for name in ["a:b.txt", "back\\slash"] {
        let tmp = scratch();
        let root = tmp.path().join("repo");
        std::fs::create_dir(&root).unwrap();
        let out = upload(&root, &root, name, false, finish(file(name, b"p"))).await;
        assert_eq!(out.map(|r| r.moved), Ok(1), "{name:?}");
        assert_eq!(std::fs::read(root.join(name)).unwrap(), b"p");
    }
}

// Catches: the sender following a symlink / opening a FIFO and uploading the
// target's bytes (exfiltration of ~/.ssh) or hanging; a hostile source must send no
// request at all.
#[cfg(unix)]
#[tokio::test]
async fn symlink_and_fifo_sources_are_refused_before_any_request() {
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let app = axum::Router::new().fallback(move || {
        let counter = counter.clone();
        async move {
            counter.fetch_add(1, Ordering::SeqCst);
            axum::Json(serde_json::json!({"moved": 1, "skipped": 0}))
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await });

    let tmp = scratch();
    let secret = tmp.path().join("secret");
    std::fs::write(&secret, b"s3cret").unwrap();
    let link = tmp.path().join("link");
    std::os::unix::fs::symlink(&secret, &link).unwrap();
    let tree = tmp.path().join("tree");
    std::fs::create_dir(&tree).unwrap();
    std::os::unix::fs::symlink(&secret, tree.join("inner")).unwrap();
    let fifo = tmp.path().join("fifo");
    let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);

    let paths = [&link, &tree, &fifo].map(|p| p.to_str().unwrap().to_owned());
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        send_copies(
            reqwest::Client::new(),
            &endpoint,
            None,
            "/dest",
            paths.to_vec(),
            true,
        ),
    )
    .await
    .expect("a FIFO source must not hang the sender")
    .unwrap();
    server.abort();
    assert_eq!((out.moved, out.errors.len()), (0, 3), "{:?}", out.errors);
    assert_eq!(
        hits.load(Ordering::SeqCst),
        0,
        "hostile source reached the wire"
    );
}

// Catches: the session token (sent as `?token=`) echoed in the error text when the
// daemon answers 2xx with a non-JSON body (reqwest's decode error carries the URL).
#[tokio::test]
async fn token_never_appears_in_error_text() {
    let app = axum::Router::new().fallback(|| async { "<html>not the daemon</html>" });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let tmp = scratch();
    let f = tmp.path().join("f");
    std::fs::write(&f, b"x").unwrap();
    let out = send_copies(
        reqwest::Client::new(),
        &endpoint,
        Some("SECRET-TOKEN-1434"),
        "/dest",
        vec![f.to_str().unwrap().into()],
        true,
    )
    .await;
    server.abort();
    let text = match out {
        Ok(r) => r.errors.join("\n"),
        Err(e) => e,
    };
    assert_ne!(text, "");
    assert!(!text.contains("SECRET-TOKEN-1434"), "token leaked: {text}");
}

// Catches: /fs/transfer-remote registered outside the auth middleware (a
// network client without the token driving the desktop's remote connections).
#[tokio::test]
#[serial_test::serial]
async fn transfer_remote_route_requires_the_session_token() {
    use tower::ServiceExt;
    let config = scratch();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    *state.session_token.write() = "existing-token".into();
    state.config.write().services.auth.lan_auth_bypass = false;
    let app = crate::mcp_http::build_remote_router(state);
    for uri in ["/fs/transfer-remote", "/fs/transfer-remote?token=wrong"] {
        let mut req = axum::http::Request::post(uri)
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"connectionId":"x","destDir":"/d","paths":["/etc/hostname"],"allowRecursive":true}"#,
            ))
            .unwrap();
        req.extensions_mut().insert(axum::extract::ConnectInfo(
            "203.0.113.9:4000".parse::<std::net::SocketAddr>().unwrap(),
        ));
        let res = app.clone().oneshot(req).await.unwrap();
        assert!(
            res.status().is_client_error() || res.status().is_redirection(),
            "{uri}: reached the handler ({})",
            res.status()
        );
    }
}

#[cfg(unix)]
fn running_as_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

// Catches (round 2): the mode masks making a legitimate read-only directory
// (0o555, e.g. a vendored cache) unpublishable — rename(2) of a directory into a
// different parent needs write permission on the moved directory, so restoring
// the archive's r-x mode before publish fails the whole upload with EACCES.
#[cfg(unix)]
#[tokio::test]
async fn read_only_top_level_directory_is_published() {
    if running_as_root() {
        return;
    }
    let tmp = scratch();
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).unwrap();
    let members = [
        raw_member_mode(b"x", b'5', b"", 0, b"", 0o555),
        raw_member_mode(b"x/f", b'0', b"", 1, b"a", 0o444),
    ]
    .concat();
    let out = upload(&root, &root, "x", true, finish(members)).await;
    assert_eq!(out.map(|r| r.moved), Ok(1));
    assert_eq!(std::fs::read(root.join("x/f")).unwrap(), b"a");
    assert_eq!(names(&root), vec!["x"], "no staging residue");
    // The test's own cleanup needs the owner write bit back.
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(root.join("x"), std::fs::Permissions::from_mode(0o755)).unwrap();
}

// Catches (round 2): removing the global timeout for /fs/upload-copy while the only
// remaining bound is a per-chunk idle limit — a client trickling one byte every
// 29 s never idles out and pins an upload slot (2 in total) for hours.
#[tokio::test(start_paused = true)]
async fn trickling_upload_cannot_pin_a_slot_indefinitely() {
    let tmp = scratch();
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).unwrap();
    let stream = futures_util::stream::unfold(0u32, |n| async move {
        if n >= 400 {
            return None;
        }
        tokio::time::sleep(std::time::Duration::from_secs(29)).await;
        Some((Ok::<_, io::Error>(vec![0u8]), n + 1))
    });
    let started = tokio::time::Instant::now();
    let roots = [root.to_str().unwrap().to_owned()];
    let out = receive_copy(query(&root, "f", false), &roots, Body::from_stream(stream)).await;
    assert!(out.is_err());
    assert!(
        started.elapsed() < std::time::Duration::from_secs(3600),
        "slot held for {:?} by a 1 B / 29 s trickle",
        started.elapsed()
    );
    assert_eq!(names(&root), Vec::<String>::new());
}
