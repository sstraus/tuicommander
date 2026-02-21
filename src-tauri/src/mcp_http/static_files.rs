use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use include_dir::{include_dir, Dir};

/// Frontend dist/ embedded at compile time for single-binary distribution.
pub(super) static FRONTEND_DIST: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../dist");

/// In debug builds, the dist/ directory on disk at compile-time location is checked first.
/// This lets `pnpm build` (or `vite build --watch`) update the remote UI without a Rust recompile.
#[cfg(debug_assertions)]
const DEV_DIST_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../dist");

/// Serve embedded static files from dist/ with correct MIME types.
/// Unknown paths fall back to index.html for SPA client-side routing.
pub(super) async fn serve_static(axum::extract::Path(path): axum::extract::Path<String>) -> Response {
    serve_file(&path)
}

/// Serve the root index.html.
pub(super) async fn serve_index() -> Response {
    serve_file("index.html")
}

/// Look up a file and return it with the correct content-type.
/// In debug builds: reads from disk so pnpm build is reflected immediately.
/// In release builds: uses embedded bytes (no filesystem dependency).
fn serve_file(path: &str) -> Response {
    #[cfg(debug_assertions)]
    {
        let disk_path = format!("{}/{}", DEV_DIST_PATH, path);
        if let Ok(bytes) = std::fs::read(&disk_path) {
            let mime = mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string();
            return (StatusCode::OK, [(header::CONTENT_TYPE, mime)], bytes).into_response();
        }
        // path not found on disk → SPA fallback to index.html from disk
        let index_path = format!("{}/index.html", DEV_DIST_PATH);
        if let Ok(bytes) = std::fs::read(&index_path) {
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "text/html".to_string())],
                bytes,
            )
                .into_response();
        }
        // fall through to embedded (e.g. dist/ not yet built)
    }

    serve_embedded_file(path)
}

fn serve_embedded_file(path: &str) -> Response {
    if let Some(file) = FRONTEND_DIST.get_file(path) {
        let mime = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, mime)],
            file.contents(),
        )
            .into_response()
    } else {
        // SPA fallback: return index.html for unknown paths
        if let Some(index) = FRONTEND_DIST.get_file("index.html") {
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "text/html".to_string())],
                index.contents(),
            )
                .into_response()
        } else {
            StatusCode::NOT_FOUND.into_response()
        }
    }
}
