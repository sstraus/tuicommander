use super::*;
use std::io::BufRead;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;

pub(in crate::design_mode) fn git(repo: &Path, args: &[&str]) {
    let result = Command::new(crate::cli::resolve_cli("git"))
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
pub(in crate::design_mode) fn repository() -> tempfile::TempDir {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    git(dir.path(), &["init"]);
    git(
        dir.path(),
        &[
            "-c",
            "user.name=Design test",
            "-c",
            "user.email=design@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "fixture",
        ],
    );
    dir
}
pub(in crate::design_mode) struct Server {
    child: Child,
    pub(in crate::design_mode) port: u16,
    cwd: PathBuf,
    script: Option<PathBuf>,
}
impl Server {
    pub(in crate::design_mode) fn start(cwd: &Path, script: Option<&Path>) -> Self {
        Self::with_code(
            cwd,
            script,
            "from http.server import HTTPServer,SimpleHTTPRequestHandler; s=HTTPServer(('127.0.0.1',0),SimpleHTTPRequestHandler); print(s.server_port,flush=True); s.serve_forever()",
        )
    }
    fn with_code(cwd: &Path, script: Option<&Path>, code: &str) -> Self {
        // Real HTTP and raw sockets establish OS ownership and actual response behavior.
        let mut command = Command::new("python3");
        command.arg("-u");
        if let Some(script) = script {
            std::fs::write(script, code).unwrap();
            command.arg(script);
        } else {
            command.args(["-c", code]);
        }
        let mut child = command
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut line = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        Self {
            child,
            cwd: cwd.to_path_buf(),
            script: script.map(Path::to_path_buf),
            port: line.trim().parse().unwrap(),
        }
    }
    pub(in crate::design_mode) fn pid(&self) -> u32 {
        self.child.id()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
/// Registers the session being edited, rooted at `cwd`, and lists what Design Mode offers it.
pub(in crate::design_mode) async fn offered_to(
    state: Arc<crate::AppState>,
    repo: &Path,
    session: &str,
    cwd: &Path,
    servers: &[&Server],
) -> Vec<Value> {
    crate::test_support::insert_recording_session(&state, session);
    state.session_maps.sessions.get(session).unwrap().lock().cwd =
        Some(cwd.to_string_lossy().into_owned());
    let result = discover_with_source(&state, repo, Some(session), |roots| {
        // Snapshot only fixture-owned resources: never enumerate unrelated host processes.
        let mut listing = String::new();
        let mut cwds = String::new();
        let mut commands = String::new();
        for server in servers {
            listing.push_str(&format!(
                "p{}\ncpython3\nn127.0.0.1:{}\n",
                server.pid(),
                server.port
            ));
            cwds.push_str(&format!("p{}\nn{}\n", server.pid(), server.cwd.display()));
            if let Some(script) = &server.script {
                commands.push_str(&format!("{} python3 {}\n", server.pid(), script.display()));
            }
        }
        let owned = attributed_candidates(&listing, &mut cwds, &commands, roots);
        async move { Ok(owned) }
    })
    .await
    .unwrap();
    result["servers"].as_array().unwrap().clone()
}
// Catches: discovery dropping live servers, choosing a parent over the deepest worktree,
// retaining a dead process, or treating a sibling path prefix as repository ownership.
#[tokio::test]
async fn recorded_mac_listeners_use_deepest_cwd_and_require_a_live_owner() {
    let repo = repository();
    let nested = repo.path().join("app");
    git(
        repo.path(),
        &["worktree", "add", "--detach", nested.to_str().unwrap()],
    );
    let sibling = repo.path().with_extension("other");
    std::fs::create_dir(&sibling).unwrap();
    let parent = Server::start(repo.path(), None);
    let child = Server::start(&nested, None);
    let foreign = Server::start(&sibling, None);
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    // The session edits one worktree: it sees that worktree's server, never a sibling's.
    let from_child = offered_to(
        state.clone(),
        repo.path(),
        "edit-child",
        &nested,
        &[&parent, &child, &foreign],
    )
    .await;
    let from_parent = offered_to(
        state.clone(),
        repo.path(),
        "edit-parent",
        repo.path(),
        &[&parent, &child, &foreign],
    )
    .await;
    for (found, server, root, other) in [
        (&from_child, &child, nested.as_path(), &parent),
        (&from_parent, &parent, repo.path(), &child),
    ] {
        let row = found
            .iter()
            .find(|row| row["pid"] == server.pid())
            .expect("live server of the edited worktree was not offered");
        assert_eq!(row["url"], format!("http://localhost:{}/", server.port));
        assert_eq!(row["worktreePath"], json!(root));
        assert!(!found.iter().any(|row| row["pid"] == other.pid()));
        assert!(!found.iter().any(|row| row["pid"] == foreign.pid()));
    }
    let dead_pid = child.pid();
    drop(child);
    assert!(
        !offered_to(
            state,
            repo.path(),
            "edit-child",
            &nested,
            &[&parent, &foreign]
        )
        .await
        .iter()
        .any(|row| row["pid"] == dead_pid)
    );
    drop(foreign);
    std::fs::remove_dir(&sibling).unwrap();
}
// Catches: public discovery losing a linked worktree or failing to use an absolute
// Vite-like argv path when cwd is outside the repository; sibling prefixes cannot claim it.
#[tokio::test]
async fn recorded_vite_processes_distinguish_worktrees_and_command_path_boundaries() {
    let repo = repository();
    let worktree = repo.path().join("linked");
    git(
        repo.path(),
        &["worktree", "add", "--detach", worktree.to_str().unwrap()],
    );
    let outside = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let linked = Server::start(&worktree, None);
    let fallback = Server::start(outside.path(), Some(&repo.path().join("vite.py")));
    let foreign = Server::start(
        outside.path(),
        Some(&repo.path().with_extension("other.py")),
    );
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    for (session, cwd, server, root) in [
        (
            "edit-linked",
            worktree.as_path(),
            &linked,
            worktree.as_path(),
        ),
        ("edit-main", repo.path(), &fallback, repo.path()),
    ] {
        let found = offered_to(
            state.clone(),
            repo.path(),
            session,
            cwd,
            &[&linked, &fallback, &foreign],
        )
        .await;
        let row = found
            .iter()
            .find(|row| row["pid"] == server.pid())
            .expect("owned server absent from discovery response");
        assert_eq!(row["worktreePath"], json!(root));
        assert_eq!(row["url"], format!("http://localhost:{}/", server.port));
        assert!(!found.iter().any(|row| row["pid"] == foreign.pid()));
    }
    drop(foreign);
    std::fs::remove_file(repo.path().with_extension("other.py")).unwrap();
}

// Catches: repository-owned non-HTTP sockets being advertised as dev servers.
#[tokio::test]
async fn non_html_listeners_are_not_dev_servers() {
    let repo = repository();
    let socket = Server::with_code(
        repo.path(),
        None,
        "import socket,sys; s=socket.socket(); s.bind(('127.0.0.1',0)); s.listen(); print(s.getsockname()[1],flush=True); sys.stdin.read()",
    );
    let html = Server::start(repo.path(), None);
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let found = offered_to(state, repo.path(), "edit", repo.path(), &[&socket, &html]).await;
    assert!(!found.iter().any(|row| row["pid"] == socket.pid()));
    assert!(found.iter().any(|row| row["pid"] == html.pid()));
}

// Catches: a request without a live session (or from outside the repository) listing every
// repository listener instead of nothing.
#[tokio::test]
async fn unknown_or_foreign_session_lists_no_listeners() {
    let repo = repository();
    let outside = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let server = Server::start(repo.path(), None);
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let response = discover_with_source(&state, repo.path(), Some("ghost"), |_| async {
        Ok(vec![json!({"url":format!("http://localhost:{}/", server.port),"source":"listener","pid":server.pid(),"worktreePath":repo.path()})])
    }).await.unwrap();
    assert_eq!(
        response["servers"].as_array().unwrap().as_slice(),
        [] as [serde_json::Value; 0]
    );
    assert_eq!(
        offered_to(state, repo.path(), "elsewhere", outside.path(), &[&server]).await,
        [] as [serde_json::Value; 0]
    );
}
// Catches: JSON API/redirect/error listeners being mistaken for usable HTML pages.
#[tokio::test]
async fn html_probe_rejects_json_redirects_and_errors() {
    use axum::{Router, routing::get};
    for (status, content_type, expected) in [
        (200, "text/html; charset=utf-8", true),
        (200, "application/json", false),
        (302, "text/html", false),
        (500, "text/html", false),
    ] {
        let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", socket.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(
                socket,
                Router::new().route(
                    "/",
                    get(move || async move {
                        (
                            axum::http::StatusCode::from_u16(status).unwrap(),
                            [("content-type", content_type)],
                            "<html>fixture</html>",
                        )
                    }),
                ),
            )
            .await
            .unwrap();
        });
        assert_eq!(
            !html_servers(vec![json!({"url":url})]).await.is_empty(),
            expected
        );
        task.abort();
    }
}
// Catches: TUIC advertising its own HTML frontend as a repository dev server.
#[test]
fn tuic_pid_is_not_a_candidate() {
    let root = PathBuf::from("/fixture/repo");
    let pid = std::process::id();
    assert_eq!(
        candidates(
            &format!("p{pid}\nctuicommander\nn127.0.0.1:9876\n"),
            &format!("p{pid}\nn{}\n", root.display()),
            &[root]
        ),
        [] as [serde_json::Value; 0]
    );
}
