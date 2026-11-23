use tuic_gateway::model::Provider;
pub(crate) use tuic_git::circleci::{CircleCiJob, TokenSource, parse_check_url};

use crate::github::gateway::Origin;
use crate::provider_gateway::direct::{self, Direct};

const MAX_FAILED_ACTIONS: usize = 5;
const MAX_LOG_BODY_BYTES: usize = 64 * 1024;
const MAX_LOG_DOWNLOAD_BYTES: usize = 16 * 1024 * 1024;

fn allowed_log_url(value: &str, api_base: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    let host = url.host_str().unwrap_or_default();
    let provider_host = host == "amazonaws.com" || host.ends_with(".amazonaws.com");
    (url.scheme() == "https"
        && provider_host
        && url.port().is_none_or(|port| port == 443)
        && url.username().is_empty()
        && url.password().is_none())
        || (cfg!(test) && url::Url::parse(api_base).is_ok_and(|base| base.origin() == url.origin()))
}
/// Bounds the complete CircleCI API request so callers cannot leave healing active indefinitely.
const CIRCLECI_API_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// A CircleCI send: no redirect (a credential header or a pre-signed URL must
/// not follow one), the API's own timeout, one budget for the provider.
fn circleci_call(caller: &'static str) -> Direct<'static> {
    Direct::new(Origin::visible(caller), Provider::Generic, "circleci")
        .no_redirect()
        .redact_url()
        .timeout(CIRCLECI_API_TIMEOUT)
}

fn token_from_cli_config(config: &str) -> Option<String> {
    let config: serde_yaml::Value = serde_yaml::from_str(config).ok()?;
    let mapping = config.as_mapping()?;
    let host = mapping.get("host").and_then(serde_yaml::Value::as_str);
    if host.is_some_and(|host| host.trim_end_matches('/').ne("https://circleci.com")) {
        return None;
    }
    mapping
        .get("token")
        .and_then(serde_yaml::Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
}

fn token_from_cli_config_file() -> Option<String> {
    let path = dirs::home_dir()?.join(".circleci").join("cli.yml");
    token_from_cli_config(&std::fs::read_to_string(path).ok()?)
}

pub(crate) fn resolve_token() -> Result<(Option<String>, TokenSource), String> {
    resolve_token_with(
        || crate::credentials::get(crate::credentials::Credential::CircleCiToken),
        std::env::var("CIRCLE_TOKEN").ok(),
        token_from_cli_config_file(),
    )
}

fn resolve_token_with(
    vault: impl FnOnce() -> Result<Option<String>, String>,
    env: Option<String>,
    cli_config: Option<String>,
) -> Result<(Option<String>, TokenSource), String> {
    resolve_token_from_vault_result(vault(), env, cli_config)
}

fn resolve_token_from_vault_result(
    vault: Result<Option<String>, String>,
    env: Option<String>,
    cli_config: Option<String>,
) -> Result<(Option<String>, TokenSource), String> {
    let vault = vault.map_err(|error| format!("Failed to read CircleCI token: {error}"))?;
    Ok(resolve_token_from_sources(vault, env, cli_config))
}

/// Stored TUIC credential only: Settings must not inspect environment or CLI fallback files.
pub(crate) fn stored_token_status() -> &'static str {
    match crate::credentials::get(crate::credentials::Credential::CircleCiToken) {
        Ok(Some(token)) if !token.trim().is_empty() => "set",
        Ok(_) => "unset",
        Err(_) => "unavailable",
    }
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn circleci_token_status() -> Result<serde_json::Value, String> {
    let (token, source) = resolve_token()?;
    Ok(serde_json::json!({ "configured": token.is_some(), "source": source }))
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn circleci_set_token(token: String) -> Result<(), String> {
    if token.trim().is_empty() {
        return Err("CircleCI token must not be empty; use Remove token instead".to_string());
    }
    crate::credentials::set(crate::credentials::Credential::CircleCiToken, &token)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn circleci_delete_token() -> Result<(), String> {
    crate::credentials::delete(crate::credentials::Credential::CircleCiToken)
}

fn resolve_token_from_sources(
    vault: Option<String>,
    env: Option<String>,
    cli_config: Option<String>,
) -> (Option<String>, TokenSource) {
    for (token, source) in [
        (vault, TokenSource::Vault),
        (env, TokenSource::Env),
        (cli_config, TokenSource::CliConfig),
    ] {
        if let Some(token) = token.map(|value| value.trim().to_string())
            && !token.is_empty()
        {
            return (Some(token), source);
        }
    }

    (None, TokenSource::None)
}

/// Return a bounded list of failed CircleCI action logs. CircleCI occasionally
/// reports a failed job without marking an action failed, so retain the last
/// fetchable action as a best-effort fallback for that case.
pub(crate) fn failed_actions(detail: &serde_json::Value) -> Vec<(String, String)> {
    let actions: Vec<(bool, String, String)> = detail
        .get("steps")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|step| {
            let step_name = step
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("failed step")
                .to_string();
            let step_name = crate::github::sanitize_ci_label(&step_name);
            step.get("actions")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(move |action| {
                    let output_url = action.get("output_url")?.as_str()?.trim();
                    (!output_url.is_empty()).then(|| {
                        (
                            action.get("failed").and_then(serde_json::Value::as_bool) == Some(true),
                            step_name.clone(),
                            output_url.to_string(),
                        )
                    })
                })
        })
        .collect();

    let failed: Vec<_> = actions
        .iter()
        .filter(|(failed, _, _)| *failed)
        .take(MAX_FAILED_ACTIONS)
        .map(|(_, step, output_url)| (step.clone(), output_url.clone()))
        .collect();
    if !failed.is_empty() {
        return failed;
    }

    (detail.get("status").and_then(serde_json::Value::as_str) == Some("failed"))
        .then(|| actions.last())
        .flatten()
        .map(|(_, step, output_url)| vec![(step.clone(), output_url.clone())])
        .unwrap_or_default()
}

/// Fetches a CircleCI v1.1 job and the pre-signed S3 logs for its failed
/// actions. CircleCI v2 does not expose step output, so keep this v1.1 detail
/// contained here for a future adapter replacement.
pub(crate) async fn fetch_job_log(
    job: &CircleCiJob,
    token: &str,
    expected_sha: &str,
) -> Result<String, String> {
    fetch_job_log_from_base(
        job,
        token,
        "https://circleci.com/api/v1.1/project/",
        expected_sha,
    )
    .await
}

pub(crate) async fn fetch_job_log_from_base(
    job: &CircleCiJob,
    token: &str,
    api_base: &str,
    expected_sha: &str,
) -> Result<String, String> {
    let mut url =
        url::Url::parse(api_base).map_err(|error| format!("Invalid CircleCI API base: {error}"))?;
    url.path_segments_mut()
        .expect("static URL can hold path segments")
        .pop_if_empty()
        .extend([
            job.vcs.as_str(),
            job.org.as_str(),
            job.repo.as_str(),
            &job.build_num.to_string(),
        ]);
    // The credential must never follow a provider-controlled redirect. S3 log
    // downloads below use a separate call without this header.
    let detail: serde_json::Value = circleci_call("circleci.fetch_job_log")
        .send(
            direct::builder(reqwest::Method::GET, url)
                .header("Circle-Token", token)
                .header("Accept", "application/json"),
        )
        .await
        .map_err(|error| format!("CircleCI request failed: {error}"))?
        .error_for_status()
        .map_err(|error| format!("CircleCI API error: {}", error.without_url()))?
        .json()
        .await
        .map_err(|error| format!("Failed to parse CircleCI job: {error}"))?;

    let actions = failed_actions(&detail);
    if detail
        .get("vcs_revision")
        .and_then(serde_json::Value::as_str)
        != Some(expected_sha)
    {
        return Err(
            "CircleCI job revision does not match the expected PR or branch head".to_string(),
        );
    }
    if actions.is_empty() {
        return Err("CircleCI job contains no failed action logs".to_string());
    }

    let mut output = String::new();
    for (step, output_url) in actions {
        // output_url is a pre-signed S3 bearer URL: never attach Circle-Token
        // and never log it.
        if !allowed_log_url(&output_url, api_base) {
            return Err(format!("CircleCI log URL for {step} has an untrusted host"));
        }
        let response = circleci_call("circleci.fetch_action_log")
            .stream(direct::builder(reqwest::Method::GET, &output_url))
            .await
            .map_err(|error| format!("CircleCI log fetch failed for {step}: {error}"))?
            .error_for_status()
            .map_err(|error| {
                format!("CircleCI log API error for {step}: {}", error.without_url())
            })?;
        let messages = parse_log_tail(response)
            .await
            .map_err(|error| format!("Failed to parse CircleCI log for {step}: {error}"))?;
        output.push_str(&format!("===== FAILED STEP: {step} =====\n"));
        output.push_str(&messages);
        output.push('\n');
    }

    Ok(output)
}

/// Parse the provider's JSON array incrementally. Only the most recent output
/// bytes are retained, even when the response has no Content-Length.
async fn parse_log_tail(response: reqwest::Response) -> Result<String, String> {
    use futures_util::StreamExt;
    let (sender, receiver) = tokio::sync::mpsc::channel(2);
    let parser = tokio::task::spawn_blocking(move || {
        struct BodyReader {
            receiver: tokio::sync::mpsc::Receiver<Result<Vec<u8>, String>>,
            current: Vec<u8>,
            offset: usize,
        }
        impl std::io::Read for BodyReader {
            fn read(&mut self, target: &mut [u8]) -> std::io::Result<usize> {
                if target.is_empty() {
                    return Ok(0);
                }
                while self.offset == self.current.len() {
                    match self.receiver.blocking_recv() {
                        Some(Ok(bytes)) => {
                            self.current = bytes;
                            self.offset = 0;
                        }
                        Some(Err(error)) => return Err(std::io::Error::other(error)),
                        None => return Ok(0),
                    }
                }
                let count = target.len().min(self.current.len() - self.offset);
                target[..count].copy_from_slice(&self.current[self.offset..self.offset + count]);
                self.offset += count;
                Ok(count)
            }
        }
        struct TailVisitor;
        impl<'de> serde::de::Visitor<'de> for TailVisitor {
            type Value = String;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a CircleCI action output array")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<String, A::Error> {
                let mut tail = String::new();
                let mut truncated = false;
                while let Some(chunk) = sequence.next_element::<serde_json::Value>()? {
                    if let Some(message) = chunk.get("message").and_then(serde_json::Value::as_str)
                    {
                        tail.push_str(message);
                        // Trim in batches so short messages do not move the
                        // entire retained window on every array element.
                        if tail.len() > MAX_LOG_BODY_BYTES * 2 {
                            let mut cut = tail.len() - MAX_LOG_BODY_BYTES;
                            while !tail.is_char_boundary(cut) {
                                cut += 1;
                            }
                            tail.drain(..cut);
                            truncated = true;
                        }
                    }
                }
                if tail.len() > MAX_LOG_BODY_BYTES {
                    let mut cut = tail.len() - MAX_LOG_BODY_BYTES;
                    while !tail.is_char_boundary(cut) {
                        cut += 1;
                    }
                    tail.drain(..cut);
                    truncated = true;
                }
                if truncated {
                    Ok(format!("[... CircleCI action log truncated ...]\n{tail}"))
                } else {
                    Ok(tail)
                }
            }
        }
        let reader = BodyReader {
            receiver,
            current: Vec::new(),
            offset: 0,
        };
        let mut deserializer = serde_json::Deserializer::from_reader(reader);
        // DeserializeSeed is used because this visitor returns the retained text.
        use serde::de::DeserializeSeed;
        struct TailSeed;
        impl<'de> DeserializeSeed<'de> for TailSeed {
            type Value = String;
            fn deserialize<D: serde::de::Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> Result<String, D::Error> {
                deserializer.deserialize_seq(TailVisitor)
            }
        }
        let tail = TailSeed
            .deserialize(&mut deserializer)
            .map_err(|error| error.to_string())?;
        deserializer.end().map_err(|error| error.to_string())?;
        Ok::<_, String>(tail)
    });
    let mut stream = response.bytes_stream();
    let mut downloaded = 0usize;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk
            .map(|bytes| bytes.to_vec())
            .map_err(|error| error.without_url().to_string());
        if let Ok(bytes) = &chunk {
            downloaded = downloaded.saturating_add(bytes.len());
            if downloaded > MAX_LOG_DOWNLOAD_BYTES {
                drop(sender);
                let _ = parser.await;
                return Err(format!(
                    "CircleCI action output exceeds {MAX_LOG_DOWNLOAD_BYTES} bytes"
                ));
            }
        }
        if sender.send(chunk).await.is_err() {
            break;
        }
    }
    drop(sender);
    parser.await.map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::parse_check_url;

    #[test]
    fn strips_control_sequences_from_failed_step_names() {
        let detail = serde_json::json!({"steps": [{
            "name": "safe\n\u{1b}]52;c;clipboard\u{7}\u{202e}step",
            "actions": [{"failed": true, "output_url": "https://s3.amazonaws.com/log"}]
        }]});
        assert_eq!(super::failed_actions(&detail)[0].0, "safestep");
    }

    #[test]
    fn parses_classic_circleci_job_url_with_query() {
        assert_eq!(
            parse_check_url("https://circleci.com/gh/acme/widget/42?utm_source=github"),
            Some(super::CircleCiJob {
                vcs: "gh".into(),
                org: "acme".into(),
                repo: "widget".into(),
                build_num: 42,
            })
        );
    }

    #[test]
    fn parses_app_urls_and_normalizes_vcs_slug() {
        for (vcs, expected) in [
            ("gh", "gh"),
            ("github", "gh"),
            ("bb", "bb"),
            ("bitbucket", "bb"),
        ] {
            let url = format!(
                "https://app.circleci.com/pipelines/{vcs}/acme/widget/77/workflows/workflow-id/jobs/19"
            );
            assert_eq!(
                parse_check_url(&url),
                Some(super::CircleCiJob {
                    vcs: expected.into(),
                    org: "acme".into(),
                    repo: "widget".into(),
                    build_num: 19,
                })
            );
        }
    }

    #[test]
    fn rejects_non_circleci_or_incomplete_job_urls() {
        for url in [
            "https://github.com/acme/widget/actions/runs/42",
            "https://app.circleci.com/pipelines/github/acme/widget/77/workflows/workflow-id",
            "https://circleci.com/gh/acme/widget/not-a-number",
            "https://app.circleci.com/pipelines/gitlab/acme/widget/77/workflows/workflow-id/jobs/19",
        ] {
            assert_eq!(parse_check_url(url), None, "{url}");
        }
    }

    #[test]
    fn reads_only_the_token_key_from_circleci_cli_config() {
        let config = "# CircleCI CLI config\napi: https://circleci.com\ntoken:  cli-read-only-token  \nother_token: ignored\n";

        assert_eq!(
            super::token_from_cli_config(config),
            Some("cli-read-only-token".into())
        );
    }

    #[test]
    fn resolves_tokens_in_vault_env_cli_config_order() {
        assert_eq!(
            super::resolve_token_from_sources(
                Some("vault-token".into()),
                Some("env-token".into()),
                Some("cli-token".into()),
            ),
            (Some("vault-token".into()), super::TokenSource::Vault)
        );
        assert_eq!(
            super::resolve_token_from_sources(
                None,
                Some("env-token".into()),
                Some("cli-token".into()),
            ),
            (Some("env-token".into()), super::TokenSource::Env)
        );
        assert_eq!(
            super::resolve_token_from_sources(None, None, Some("cli-token".into())),
            (Some("cli-token".into()), super::TokenSource::CliConfig)
        );
        assert_eq!(
            super::resolve_token_from_sources(None, None, None),
            (None, super::TokenSource::None)
        );
    }

    #[test]
    fn resolves_a_present_vault_token_through_the_injected_reader() {
        assert_eq!(
            super::resolve_token_with(|| Ok(Some("vault-token".into())), None, None),
            Ok((Some("vault-token".into()), super::TokenSource::Vault))
        );
    }

    #[test]
    fn vault_failure_is_not_reported_as_an_absent_token() {
        assert_eq!(
            super::resolve_token_from_vault_result(Err("keychain unavailable".into()), None, None),
            Err("Failed to read CircleCI token: keychain unavailable".into())
        );
    }

    #[test]
    fn selects_failed_actions_and_caps_the_s3_fan_out() {
        let detail: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/circleci/job.json")).unwrap();

        assert_eq!(
            super::failed_actions(&detail),
            vec![
                ("unit".into(), "https://logs.example/unit".into()),
                ("lint".into(), "https://logs.example/lint".into()),
                (
                    "integration".into(),
                    "https://logs.example/integration".into()
                ),
                ("e2e".into(), "https://logs.example/e2e".into()),
                ("package".into(), "https://logs.example/package".into()),
            ]
        );
    }

    #[test]
    fn falls_back_to_the_last_action_for_a_failed_job_without_flagged_actions() {
        let detail = serde_json::json!({
            "status": "failed",
            "steps": [
                { "name": "first", "actions": [{ "failed": false, "output_url": "https://logs.example/first" }] },
                { "name": "last", "actions": [{ "failed": false, "output_url": "https://logs.example/last" }] }
            ]
        });

        assert_eq!(
            super::failed_actions(&detail),
            vec![("last".into(), "https://logs.example/last".into())]
        );
    }

    #[tokio::test]
    async fn presigned_s3_download_never_receives_circleci_credentials() {
        use mockito::{Matcher, Server};

        let mut server = Server::new_async().await;
        let output_url = format!("{}/output", server.url());
        let _api = server
            .mock("GET", "/api/v1.1/project/gh/acme/widget/42")
            .match_header("circle-token", "secret")
            .with_status(200)
            .with_body(serde_json::json!({"vcs_revision":"head-sha","steps":[{"name":"unit","actions":[{"failed":true,"output_url":output_url}]}]}).to_string())
            .create_async()
            .await;
        let output = server
            .mock("GET", "/output")
            .match_header("circle-token", Matcher::Missing)
            .match_header("authorization", Matcher::Missing)
            .with_status(200)
            .with_body("[{\"message\":\"failed\"}]")
            .create_async()
            .await;
        let job = super::CircleCiJob {
            vcs: "gh".into(),
            org: "acme".into(),
            repo: "widget".into(),
            build_num: 42,
        };

        assert!(
            super::fetch_job_log_from_base(
                &job,
                "secret",
                &(server.url() + "/api/v1.1/project/"),
                "head-sha"
            )
            .await
            .unwrap()
            .contains("failed")
        );
        _api.assert_async().await;
        output.assert_async().await;
    }

    #[tokio::test]
    async fn oversized_circleci_action_keeps_the_last_messages() {
        use mockito::Server;
        let mut server = Server::new_async().await;
        let output_url = format!("{}/output", server.url());
        let _api = server.mock("GET", "/api/v1.1/project/gh/acme/widget/42")
            .with_body(serde_json::json!({"vcs_revision":"head-sha","steps":[{"name":"unit","actions":[{"failed":true,"output_url":output_url}]}]}).to_string())
            .create_async().await;
        let body = serde_json::json!([
            {"message": "old".repeat(super::MAX_LOG_BODY_BYTES)},
            {"message": "last failure\n"}
        ])
        .to_string();
        let _output = server
            .mock("GET", "/output")
            .with_body(body)
            .create_async()
            .await;
        let job = super::CircleCiJob {
            vcs: "gh".into(),
            org: "acme".into(),
            repo: "widget".into(),
            build_num: 42,
        };
        let logs = super::fetch_job_log_from_base(
            &job,
            "token",
            &(server.url() + "/api/v1.1/project/"),
            "head-sha",
        )
        .await
        .unwrap();
        assert!(logs.ends_with("last failure\n\n"));
        assert!(logs.contains("[... CircleCI action log truncated ...]"));
        assert!(logs.len() <= super::MAX_LOG_BODY_BYTES + 100);
    }

    #[tokio::test]
    async fn circleci_action_download_has_a_finite_ceiling() {
        use mockito::Server;
        let mut server = Server::new_async().await;
        let body = format!(
            "[{{\"message\":\"{}\"}}]",
            "x".repeat(super::MAX_LOG_DOWNLOAD_BYTES)
        );
        let _output = server
            .mock("GET", "/output")
            .with_body(body)
            .create_async()
            .await;
        let response = reqwest::Client::new()
            .get(format!("{}/output", server.url()))
            .send()
            .await
            .unwrap();
        let error = super::parse_log_tail(response).await.unwrap_err();
        assert!(error.contains("exceeds"));
    }

    #[tokio::test]
    async fn mismatched_revision_never_downloads_the_action_log() {
        use mockito::Server;
        let mut server = Server::new_async().await;
        let output_url = format!("{}/output", server.url());
        let _api = server.mock("GET", "/api/v1.1/project/gh/acme/widget/42")
            .with_body(serde_json::json!({"vcs_revision":"fork-sha","steps":[{"actions":[{"failed":true,"output_url":output_url}]}]}).to_string())
            .create_async().await;
        let output = server.mock("GET", "/output").expect(0).create_async().await;
        let job = super::CircleCiJob {
            vcs: "gh".into(),
            org: "acme".into(),
            repo: "widget".into(),
            build_num: 42,
        };
        let error = super::fetch_job_log_from_base(
            &job,
            "token",
            &(server.url() + "/api/v1.1/project/"),
            "local-sha",
        )
        .await
        .unwrap_err();
        assert!(error.contains("revision does not match"));
        output.assert_async().await;
    }

    #[tokio::test]
    async fn presigned_log_redirect_is_not_followed() {
        use mockito::Server;
        let mut server = Server::new_async().await;
        let output_url = format!("{}/output", server.url());
        let _api = server.mock("GET", "/api/v1.1/project/gh/acme/widget/42")
            .with_body(serde_json::json!({"vcs_revision":"local-sha","steps":[{"actions":[{"failed":true,"output_url":output_url}]}]}).to_string())
            .create_async().await;
        let _output = server
            .mock("GET", "/output")
            .with_status(302)
            .with_header("location", "/private")
            .create_async()
            .await;
        let private = server
            .mock("GET", "/private")
            .expect(0)
            .create_async()
            .await;
        let job = super::CircleCiJob {
            vcs: "gh".into(),
            org: "acme".into(),
            repo: "widget".into(),
            build_num: 42,
        };
        assert!(
            super::fetch_job_log_from_base(
                &job,
                "token",
                &(server.url() + "/api/v1.1/project/"),
                "local-sha"
            )
            .await
            .is_err()
        );
        private.assert_async().await;
    }

    #[test]
    fn rejects_non_provider_log_hosts() {
        assert!(!super::allowed_log_url(
            "http://169.254.169.254/latest/meta-data",
            "https://circleci.com/api/v1.1/project/"
        ));
        assert!(!super::allowed_log_url(
            "https://s3.amazonaws.com.evil.test/log",
            "https://circleci.com/api/v1.1/project/"
        ));
        assert!(super::allowed_log_url(
            "https://bucket.s3.amazonaws.com/log",
            "https://circleci.com/api/v1.1/project/"
        ));
    }
}
