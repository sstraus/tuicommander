use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use include_dir::{include_dir, Dir};

/// Frontend dist/ embedded at compile time for single-binary distribution.
pub(super) static FRONTEND_DIST: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../dist");

/// Serve embedded static files from dist/ with correct MIME types.
/// Unknown paths fall back to index.html for SPA client-side routing.
pub(super) async fn serve_static(axum::extract::Path(path): axum::extract::Path<String>) -> Response {
    serve_embedded_file(&path)
}

/// Serve the root index.html.
pub(super) async fn serve_index() -> Response {
    serve_embedded_file("index.html")
}

/// Look up an embedded file and return it with the correct content-type.
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
