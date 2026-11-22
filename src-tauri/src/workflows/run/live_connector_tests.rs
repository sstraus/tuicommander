//! Live Jira, GitLab and Telegram checks. No write is ever sent from here: the
//! suite has no write authority for these systems. Without every required
//! variable a test reports `unavailable: <missing item>`; with them it runs one
//! read-only probe of the account and reports `probe_only`, never `passed`,
//! because no workflow run touched the connector.
use super::live_support::Live;
use serde_json::json;

async fn probe(live: &mut Live, url: String, auth: Option<(&str, String)>) {
    let client = reqwest::Client::builder()
        .user_agent("tuicommander-workflow-live")
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .unwrap();
    let mut request = client.get(url);
    if let Some((name, value)) = auth {
        request = request.header(name, value);
    }
    let status = request.send().await.map(|r| r.status().as_u16());
    match status {
        Ok(code) if (200..300).contains(&code) => live.finish(
            "probe_only",
            json!({"http": code, "reason": "read-only account probe; no workflow write was attempted"}),
        ),
        Ok(code) => live.finish("failed", json!({"http": code, "reason": "account probe rejected"})),
        Err(e) => live.finish("failed", json!({"reason": format!("probe error: {}", e.without_url())})),
    }
}

// Catches: a connector check reported as passed while its site, account or
// scratch resource was never supplied.
#[tokio::test]
#[ignore = "live: needs JIRA_LIVE_SITE, JIRA_LIVE_EMAIL, JIRA_LIVE_TOKEN, JIRA_LIVE_PROJECT"]
async fn live_jira_reports_unavailable_without_site_account_and_scratch_project() {
    let mut live =
        Live::start("live_jira_reports_unavailable_without_site_account_and_scratch_project");
    let vars = [
        "JIRA_LIVE_SITE",
        "JIRA_LIVE_EMAIL",
        "JIRA_LIVE_TOKEN",
        "JIRA_LIVE_PROJECT",
    ];
    if live.gate(&vars, "unavailable") {
        return;
    }
    let site = std::env::var("JIRA_LIVE_SITE").unwrap();
    let basic = format!(
        "{}:{}",
        std::env::var("JIRA_LIVE_EMAIL").unwrap(),
        std::env::var("JIRA_LIVE_TOKEN").unwrap()
    );
    use base64::Engine;
    let header = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(basic)
    );
    probe(
        &mut live,
        format!("https://{}/rest/api/3/myself", site.trim_end_matches('/')),
        Some(("Authorization", header)),
    )
    .await;
}

// Catches: as for Jira, for the GitLab host, token and scratch project.
#[tokio::test]
#[ignore = "live: needs GITLAB_LIVE_HOST, GITLAB_LIVE_TOKEN, GITLAB_LIVE_PROJECT"]
async fn live_gitlab_reports_unavailable_without_host_account_and_scratch_project() {
    let mut live =
        Live::start("live_gitlab_reports_unavailable_without_host_account_and_scratch_project");
    let vars = [
        "GITLAB_LIVE_HOST",
        "GITLAB_LIVE_TOKEN",
        "GITLAB_LIVE_PROJECT",
    ];
    if live.gate(&vars, "unavailable") {
        return;
    }
    let host = std::env::var("GITLAB_LIVE_HOST").unwrap();
    probe(
        &mut live,
        format!("https://{}/api/v4/user", host.trim_end_matches('/')),
        Some(("PRIVATE-TOKEN", std::env::var("GITLAB_LIVE_TOKEN").unwrap())),
    )
    .await;
}

// Catches: as for Jira, for the Telegram bot and scratch chat.
#[tokio::test]
#[ignore = "live: needs TELEGRAM_LIVE_BOT_TOKEN, TELEGRAM_LIVE_CHAT_ID"]
async fn live_telegram_reports_unavailable_without_bot_and_scratch_chat() {
    let mut live = Live::start("live_telegram_reports_unavailable_without_bot_and_scratch_chat");
    if live.gate(
        &["TELEGRAM_LIVE_BOT_TOKEN", "TELEGRAM_LIVE_CHAT_ID"],
        "unavailable",
    ) {
        return;
    }
    let token = std::env::var("TELEGRAM_LIVE_BOT_TOKEN").unwrap();
    probe(
        &mut live,
        format!("https://api.telegram.org/bot{token}/getMe"),
        None,
    )
    .await;
}
