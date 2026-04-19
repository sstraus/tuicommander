//! RFC 7591 Dynamic Client Registration for MCP OAuth.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub(crate) struct DcrRequest {
    pub client_name: String,
    pub redirect_uris: Vec<String>,
    pub grant_types: Vec<String>,
    pub response_types: Vec<String>,
    pub token_endpoint_auth_method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DcrResponse {
    pub client_id: String,
    #[allow(dead_code)]
    pub client_id_issued_at: Option<u64>,
}

impl DcrRequest {
    pub(crate) fn for_tuicommander(redirect_uri: &str, scope: Option<String>) -> Self {
        Self {
            client_name: "TUICommander".to_string(),
            redirect_uris: vec![redirect_uri.to_string()],
            grant_types: vec!["authorization_code".to_string()],
            response_types: vec!["code".to_string()],
            token_endpoint_auth_method: "none".to_string(),
            scope,
        }
    }
}

pub(crate) async fn register_client(
    http_client: &reqwest::Client,
    registration_endpoint: &str,
    request: &DcrRequest,
) -> Result<DcrResponse> {
    let resp = http_client
        .post(registration_endpoint)
        .json(request)
        .send()
        .await
        .with_context(|| {
            format!("Failed to send DCR request to {registration_endpoint}")
        })?;

    let status = resp.status();

    if status == reqwest::StatusCode::CREATED || status == reqwest::StatusCode::OK {
        #[derive(Deserialize)]
        struct RawDcrResponse {
            client_id: String,
            client_id_issued_at: Option<u64>,
            #[allow(dead_code)]
            client_secret: Option<String>,
        }

        let raw: RawDcrResponse = resp
            .json()
            .await
            .context("Failed to parse DCR response JSON")?;

        if raw.client_secret.is_some() {
            tracing::warn!(
                "DCR endpoint returned client_secret for public client — ignoring"
            );
        }

        Ok(DcrResponse {
            client_id: raw.client_id,
            client_id_issued_at: raw.client_id_issued_at,
        })
    } else if status == reqwest::StatusCode::BAD_REQUEST {
        let body = resp.text().await.unwrap_or_default();
        bail!("DCR registration rejected (400): {body}");
    } else {
        bail!("DCR registration failed with HTTP {status}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_request() -> DcrRequest {
        DcrRequest::for_tuicommander("tuic://oauth-callback", None)
    }

    #[tokio::test]
    async fn register_client_happy_path() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/register")
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "client_id": "abc-123",
                    "client_id_issued_at": 1700000000u64
                })
                .to_string(),
            )
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let url = format!("{}/register", server.url());
        let resp = register_client(&client, &url, &test_request())
            .await
            .unwrap();

        assert_eq!(resp.client_id, "abc-123");
        assert_eq!(resp.client_id_issued_at, Some(1700000000));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn register_client_ignores_client_secret() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/register")
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "client_id": "abc-123",
                    "client_secret": "should-be-ignored"
                })
                .to_string(),
            )
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let url = format!("{}/register", server.url());
        let resp = register_client(&client, &url, &test_request())
            .await
            .unwrap();

        assert_eq!(resp.client_id, "abc-123");
    }

    #[tokio::test]
    async fn register_client_400_error() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/register")
            .with_status(400)
            .with_body(r#"{"error":"invalid_client_metadata","error_description":"bad redirect_uri"}"#)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let url = format!("{}/register", server.url());
        let err = register_client(&client, &url, &test_request())
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("rejected (400)"),
            "expected 400 error, got: {err}"
        );
        assert!(
            err.to_string().contains("bad redirect_uri"),
            "expected error body, got: {err}"
        );
    }

    #[tokio::test]
    async fn register_client_500_error() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/register")
            .with_status(500)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let url = format!("{}/register", server.url());
        let err = register_client(&client, &url, &test_request())
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("HTTP 500"),
            "expected HTTP 500 error, got: {err}"
        );
    }

    #[tokio::test]
    async fn register_client_optional_fields_absent() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/register")
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(serde_json::json!({"client_id": "minimal"}).to_string())
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let url = format!("{}/register", server.url());
        let resp = register_client(&client, &url, &test_request())
            .await
            .unwrap();

        assert_eq!(resp.client_id, "minimal");
        assert_eq!(resp.client_id_issued_at, None);
    }
}
