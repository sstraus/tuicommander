use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use serde_json::{Value, json};
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use tower::ServiceExt;

async fn read(repo: &Path, file: &str, range: &[(&str, &str)]) -> (StatusCode, Value) {
    read_route(repo, file, range, "/fs/read").await
}

async fn read_route(
    repo: &Path,
    file: &str,
    range: &[(&str, &str)],
    route: &str,
) -> (StatusCode, Value) {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query
        .append_pair("repoPath", repo.to_str().unwrap())
        .append_pair("file", file);
    for (name, value) in range {
        query.append_pair(name, value);
    }
    let response = Router::new()
        .route("/fs/read", get(super::fs_read_file_http))
        .route("/fs/read-editor", get(super::remote_read_editor_file_http))
        .oneshot(
            Request::get(format!("{route}?{}", query.finish()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!(String::from_utf8_lossy(&bytes))),
    )
}

// Catches: range parameters are ignored and a file over the editor limit is rejected or read in full.
#[tokio::test]
async fn fs_range_read_large_file_returns_only_requested_bytes() {
    let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let mut file = std::fs::File::create(repo.path().join("large.bin")).unwrap();
    file.set_len(20 * 1024 * 1024).unwrap();
    file.seek(SeekFrom::Start(12 * 1024 * 1024)).unwrap();
    file.write_all(b"page").unwrap();
    let (status, body) = read(
        repo.path(),
        "large.bin",
        &[("offset", "12582912"), ("length", "4")],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "bounded range must bypass editor size cap: {body}"
    );
    assert_eq!(
        body,
        json!({"data":"cGFnZQ==", "offset":12582912, "bytesRead":4,
        "totalSize":20971520, "hasMore":true, "clamped":false})
    );
    assert_eq!(
        read(repo.path(), "large.bin", &[]).await.0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

// Catches: range mode bypasses the repo-relative input check, refuses an owner
// link that full reads follow, or silently ignores the requested slice.
#[tokio::test]
async fn fs_range_read_keeps_full_read_repository_boundary() {
    let fixture = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let repo = fixture.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    std::fs::write(repo.join("inside.txt"), "inside").unwrap();
    let outside = fixture.path().join("outside.txt");
    std::fs::write(&outside, "outside").unwrap();
    let paths = [
        "../outside.txt".to_string(),
        outside.to_str().unwrap().to_string(),
    ];
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, repo.join("linked.txt")).unwrap();
        let full = read(&repo, "linked.txt", &[]).await;
        let ranged = read(&repo, "linked.txt", &[("offset", "1"), ("length", "2")]).await;
        assert_eq!(full.0, StatusCode::OK);
        assert_eq!(ranged.0, StatusCode::OK);
        assert_eq!(
            ranged.1["data"], "dXQ=",
            "range follows the link full reads follow"
        );
    }
    for path in paths {
        let full = read(&repo, &path, &[]).await;
        let ranged = read(&repo, &path, &[("offset", "1"), ("length", "2")]).await;
        assert_eq!(full.0, StatusCode::INTERNAL_SERVER_ERROR, "{path}");
        assert_eq!(ranged, full, "range must keep full-read refusal for {path}");
    }
    let (status, body) = read(&repo, "inside.txt", &[("offset", "1"), ("length", "2")]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["data"], "bnM=",
        "positive control must actually read a range"
    );
}

// Catches: oversized lengths allocate unbounded pages, omitted length reads the whole file,
// or a length-only request is mistaken for a legacy full read.
#[tokio::test]
async fn fs_range_read_clamps_requests_and_defaults_to_bounded_pages() {
    use base64::Engine;
    let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    std::fs::write(repo.path().join("pages.bin"), vec![b'x'; 1048578]).unwrap();
    for (query, offset, clamped) in [
        (vec![("length", "18446744073709551615")], 0, true),
        (vec![("offset", "1")], 1, false),
        (vec![("length", "1048576")], 0, false),
    ] {
        let (status, body) = read(repo.path(), "pages.bin", &query).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["offset"], offset);
        assert_eq!(body["bytesRead"], 1048576);
        assert_eq!(body["totalSize"], 1048578);
        assert_eq!(body["clamped"], clamped);
        assert_eq!(body["hasMore"], true);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(body["data"].as_str().unwrap())
            .unwrap();
        assert_eq!(bytes, vec![b'x'; 1048576]);
    }
}

// Catches: UTF-8 conversion corrupts a split sequence, EOF arithmetic overflows,
// or zero length/full reads change their existing meaning.
#[tokio::test]
async fn fs_range_read_preserves_bytes_and_handles_empty_and_eof_pages() {
    let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    std::fs::write(repo.path().join("text.txt"), "AéZ").unwrap();
    assert_eq!(
        read(repo.path(), "text.txt", &[]).await,
        (StatusCode::OK, json!("AéZ"))
    );
    for (offset, length, data, count, more) in [
        ("1", "1", "ww==", 1, true),
        ("2", "1", "qQ==", 1, true),
        ("3", "10", "Wg==", 1, false),
        ("0", "0", "", 0, true),
        ("4", "1", "", 0, false),
        ("18446744073709551615", "10", "", 0, false),
    ] {
        let (status, body) = read(
            repo.path(),
            "text.txt",
            &[("offset", offset), ("length", length)],
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{offset}/{length}");
        assert_eq!(
            body,
            json!({"data": data, "offset": offset.parse::<u64>().unwrap(),
            "bytesRead":count, "totalSize":4, "hasMore":more, "clamped":false})
        );
    }
    std::fs::write(repo.path().join("empty"), []).unwrap();
    assert_eq!(
        read(repo.path(), "empty", &[("length", "1")]).await.1,
        json!({"data":"", "offset":0, "bytesRead":0, "totalSize":0, "hasMore":false, "clamped":false})
    );
    std::fs::write(repo.path().join("binary"), [0xff, 0x00]).unwrap();
    assert_eq!(
        read(repo.path(), "binary", &[("length", "2")]).await.1["data"],
        "/wA="
    );
    assert_eq!(
        read(repo.path(), "binary", &[]).await.0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

// Catches: invalid range queries fall back to whole-file reads, or missing/directory targets succeed.
#[tokio::test]
async fn fs_range_read_rejects_invalid_numbers_and_non_files() {
    let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    std::fs::write(repo.path().join("file"), "bytes").unwrap();
    for (name, value) in [
        ("offset", "-1"),
        ("length", "-1"),
        ("length", "18446744073709551616"),
        ("offset", "bad"),
    ] {
        assert_eq!(
            read(repo.path(), "file", &[(name, value)]).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    for file in ["missing", "."] {
        let (status, body) = read(repo.path(), file, &[("length", "1")]).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(body["error"].is_string());
    }
}

// Catches: a remote editor whole read bypasses the standard text cap, or an
// explicit page request on it is answered with a whole read.
#[tokio::test]
async fn fs_range_read_remote_editor_whole_reads_stay_text_capped() {
    let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let file = std::fs::File::create(repo.path().join("large")).unwrap();
    file.set_len(20 * 1024 * 1024).unwrap();
    assert_eq!(
        read_route(repo.path(), "large", &[], "/fs/read-editor")
            .await
            .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    std::fs::write(repo.path().join("small"), "text").unwrap();
    assert_eq!(
        read_route(repo.path(), "small", &[], "/fs/read-editor").await,
        (StatusCode::OK, json!("text"))
    );
    assert_eq!(
        read_route(repo.path(), "small", &[("length", "1")], "/fs/read-editor").await,
        (
            StatusCode::OK,
            json!({"data":"dA==", "offset":0, "bytesRead":1, "totalSize":4, "hasMore":true, "clamped":false})
        )
    );
}
