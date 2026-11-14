//! Shared setup for the ACP client tests.
//!
//! `tests/fixtures/acp/ego-initialize.json` is a recording of what real ego
//! answers, copied from ego's own committed golden at
//! `crates/ego-cli/tests/goldens/tuicommander/initialize.json`, which that
//! repository generates by spawning the packaged binary. Editing it here to
//! make a test pass would turn the recording into a wish.
//!
//! Every ACP test needs the same three things: a root directory, a scenario
//! inside it for the fixture agent to run, and a manager pointed at that agent.
//! They are here so a scenario name is the only thing a test has to say, and so
//! the root outlives the manager — a `TempDir` dropped early deletes the
//! directory the child is still running in, which fails as a protocol error
//! somewhere far from the cause.
//!
//! Compiled once by the integration target and shared by its ACP test modules.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use agent_client_protocol::schema::v1;
use tempfile::TempDir;
use tuicommander_lib::acp::{
    AcpClientEvent, AcpClientManager, AcpConnectRequest, AcpConnectionSnapshot, AcpEventEnvelope,
    AcpEventStream, AcpSessionAuthority, EgoAcpConfig,
};

pub struct Fixture {
    root: TempDir,
    pub manager: AcpClientManager,
}

impl Fixture {
    /// A manager whose agent will run `tests/fixtures/acp/<scenario>.jsonl`.
    ///
    /// `ego-initialize.json` travels with every scenario because most of them
    /// want the same recorded ego capabilities and no scenario should have to
    /// carry its own copy.
    pub fn with(scenario: &str) -> Self {
        let root = TempDir::new_in(tuic_test_support::test_temp_root()).expect("ACP root");
        let fixtures = Path::new("tests/fixtures/acp");
        for (from, to) in [
            (format!("{scenario}.jsonl"), "scenario.jsonl"),
            ("ego-initialize.json".to_owned(), "ego-initialize.json"),
        ] {
            std::fs::copy(fixtures.join(&from), root.path().join(to))
                .unwrap_or_else(|error| panic!("scenario {scenario}: {from}: {error}"));
        }
        Self {
            root,
            manager: AcpClientManager::new(),
        }
    }

    pub fn root(&self) -> PathBuf {
        self.root
            .path()
            .canonicalize()
            .expect("canonical fixture root")
    }

    /// The launch authority every scenario runs under: the fixture agent, and
    /// nothing a request could name.
    #[must_use]
    pub fn config() -> EgoAcpConfig {
        EgoAcpConfig {
            executable: fixture_agent(),
            profile: String::new(),
            master_prompt: String::new(),
            ai_chat: false,
        }
    }

    pub async fn connect(&self) -> AcpConnectionSnapshot {
        self.manager
            .connect(
                &Self::config(),
                AcpConnectRequest {
                    root: self.root(),
                    env: None,
                },
            )
            .await
            .expect("connect")
    }
}

#[must_use]
pub fn fixture_agent() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_tuic-acp-fixture-agent"))
}

/// An absolute path on this host, written once in `/` form.
///
/// The ACP launch checks its paths with `Path::is_absolute`, which answers for
/// the host and is right to: the ego binary and the worktree are on this
/// machine. On Windows a leading `/` is merely rooted, so a POSIX-spelled path
/// is rejected there before a test reaches its subject.
#[must_use]
pub fn absolute(path: &str) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(format!("C:{}", path.replace('/', "\\")))
    } else {
        PathBuf::from(path)
    }
}

/// A session request for one directory and nothing else.
#[must_use]
pub fn authority(cwd: PathBuf) -> AcpSessionAuthority {
    AcpSessionAuthority {
        cwd,
        additional_directories: Vec::new(),
        mcp_servers: Vec::new(),
    }
}

#[must_use]
pub fn text(body: &str) -> v1::ContentBlock {
    v1::ContentBlock::Text(v1::TextContent::new(body))
}

/// The text of an agent message chunk, or `None` for anything else.
#[must_use]
pub fn chunk(envelope: &AcpEventEnvelope) -> Option<String> {
    let AcpClientEvent::SessionUpdate { update } = &envelope.event else {
        return None;
    };
    let v1::SessionUpdate::AgentMessageChunk(chunk) = &**update else {
        return None;
    };
    match &chunk.content {
        v1::ContentBlock::Text(text) => Some(text.text.clone()),
        _ => None,
    }
}

/// How long to wait for an event before calling it a failure.
///
/// Bounded on purpose. The failure these tests are built to catch is a request
/// that is never answered, and an unbounded wait turns that into a hang: the
/// agent waits for the client, the client waits for the agent, and the test
/// waits for both with nothing to say about why.
pub const PATIENCE: std::time::Duration = std::time::Duration::from_secs(10);

/// Read events until one of them matches, and hand back everything seen.
pub async fn until(
    stream: &mut AcpEventStream,
    wanted: impl Fn(&AcpClientEvent) -> bool,
) -> Vec<AcpEventEnvelope> {
    let mut seen = Vec::new();
    loop {
        let event = tokio::time::timeout(PATIENCE, stream.recv())
            .await
            .unwrap_or_else(|_| panic!("no further event within {PATIENCE:?}; saw {seen:?}"));
        let Some(event) = event else {
            panic!("the stream ended before the awaited event; saw {seen:?}");
        };
        let event = event.expect("no gap on a stream subscribed from the start");
        let done = wanted(&event.event);
        seen.push(event);
        if done {
            return seen;
        }
    }
}

/// Read events until the turn settles, and hand back everything seen.
///
/// Waiting for the settlement rather than for a count is what makes this
/// deterministic: the scenario decides when the turn ends, and the test reads
/// exactly that far.
pub async fn until_settled(stream: &mut AcpEventStream) -> Vec<AcpEventEnvelope> {
    until(stream, |event| {
        matches!(event, AcpClientEvent::TurnSettled { .. })
    })
    .await
}
