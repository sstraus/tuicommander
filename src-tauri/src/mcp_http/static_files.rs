use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
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

/// Serve the root: redirect mobile browsers to /mobile, otherwise index.html.
pub(super) async fn serve_index(headers: HeaderMap) -> Response {
    if is_mobile_user_agent(&headers) {
        return Redirect::to("/mobile").into_response();
    }
    serve_file("index.html")
}

/// Check if the User-Agent indicates a mobile device.
fn is_mobile_user_agent(headers: &HeaderMap) -> bool {
    let ua = match headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()) {
        Some(s) => s,
        None => return false,
    };
    // Phone-only: "Mobile" covers iPhone and Android phones.
    // Tablets (iPad, Android tablets) get the desktop UI — their screens are large enough.
    // Note: modern iPad UA doesn't contain "iPad" anyway (it mimics desktop Safari).
    ua.contains("Mobile")
}

/// Determine which SPA shell to serve as fallback for client-side routing.
/// Paths starting with "mobile" use mobile.html; everything else uses index.html.
fn spa_fallback_file(path: &str) -> &str {
    if path == "mobile" || path.starts_with("mobile/") {
        "mobile.html"
    } else {
        "index.html"
    }
}

/// Look up a file and return it with the correct content-type.
/// In debug builds: reads from disk so pnpm build is reflected immediately.
/// In release builds: uses embedded bytes (no filesystem dependency).
fn serve_file(path: &str) -> Response {
    #[cfg(debug_assertions)]
    {
        let base = match std::path::Path::new(DEV_DIST_PATH).canonicalize() {
            Ok(p) => p,
            Err(_) => std::path::PathBuf::from(DEV_DIST_PATH),
        };
        let candidate = base.join(path);
        if let Ok(canonical) = candidate.canonicalize()
            && canonical.starts_with(&base)
            && let Ok(bytes) = std::fs::read(&canonical)
        {
            let mime = mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string();
            return (StatusCode::OK, [(header::CONTENT_TYPE, mime)], bytes).into_response();
        }
        // path not found on disk → SPA fallback from disk
        let fallback = spa_fallback_file(path);
        let fallback_path = base.join(fallback);
        if let Ok(bytes) = std::fs::read(&fallback_path) {
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
        // SPA fallback: serve the appropriate shell for the requested path
        let fallback = spa_fallback_file(path);
        if let Some(index) = FRONTEND_DIST.get_file(fallback) {
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
