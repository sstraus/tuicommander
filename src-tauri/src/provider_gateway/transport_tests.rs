use super::transport::ReqwestTransport;
use axum::{Router, body::to_bytes, extract::Request, http::StatusCode, routing::any};
use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Method};
use tuic_gateway::model::{TransportError, WireRequest};
use tuic_gateway::ports::Transport;

async fn echo(request: Request) -> (StatusCode, [(&'static str, String); 2], Vec<u8>) {
    let (method, accept, version) = {
        let header = |name: &str| {
            request
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("<missing>")
                .to_owned()
        };
        (
            request.method().to_string(),
            header("accept"),
            header("x-github-api-version"),
        )
    };
    let body = to_bytes(request.into_body(), 1024).await.unwrap();
    let echoed = format!(
        "{method}|{accept}|{version}|{}",
        String::from_utf8_lossy(&body)
    );
    (
        StatusCode::CREATED,
        [
            ("etag", "\"abc\"".to_owned()),
            ("x-ratelimit-remaining", "41".to_owned()),
        ],
        echoed.into_bytes(),
    )
}

async fn serve() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, Router::new().fallback(any(echo)))
            .await
            .unwrap();
    });
    format!("http://{addr}/repos/o/r")
}

#[tokio::test]
async fn adapter_sends_headers_body_and_returns_status_headers_bytes() {
    let url = serve().await;
    let mut headers = HeaderMap::new();
    headers.insert(
        "accept",
        HeaderValue::from_static("application/vnd.github+json"),
    );
    headers.insert(
        "x-github-api-version",
        HeaderValue::from_static("2022-11-28"),
    );
    let transport = ReqwestTransport::new(reqwest::Client::new());

    let response = transport
        .send(WireRequest {
            method: Method::POST,
            url: url.parse().unwrap(),
            headers,
            body: Some(Bytes::from_static(b"{\"k\":1}")),
        })
        .await
        .unwrap();

    assert_eq!(response.status, 201);
    assert_eq!(response.headers["etag"], "\"abc\"");
    assert_eq!(response.headers["x-ratelimit-remaining"], "41");
    assert_eq!(
        response.body,
        Bytes::from("POST|application/vnd.github+json|2022-11-28|{\"k\":1}")
    );
}

#[tokio::test]
async fn adapter_reports_a_refused_connection_as_connect() {
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let transport = ReqwestTransport::new(reqwest::Client::new());
    let err = transport
        .send(WireRequest {
            method: Method::GET,
            url: format!("http://127.0.0.1:{port}/").parse().unwrap(),
            headers: HeaderMap::new(),
            body: None,
        })
        .await
        .unwrap_err();
    assert!(matches!(err, TransportError::Connect(_)), "{err:?}");
}
