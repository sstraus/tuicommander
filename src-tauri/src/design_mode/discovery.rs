//! Live listener discovery. Ownership uses path components, never string prefixes.
use futures_util::StreamExt;
use serde_json::{Value, json};
#[cfg(unix)]
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[cfg(unix)]
#[derive(Debug)]
struct Listener {
    pid: u32,
    name: String,
    port: u16,
}
#[cfg(unix)]
fn listeners(output: &str) -> Vec<Listener> {
    let mut pid = 0;
    let mut name = String::new();
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for line in output.lines() {
        if let Some(value) = line.strip_prefix('p') {
            pid = value.parse().unwrap_or(0);
        } else if let Some(value) = line.strip_prefix('c') {
            name = value.to_owned();
        } else if let Some(value) = line.strip_prefix('n') {
            let Some((host, port)) = value.rsplit_once(':') else {
                continue;
            };
            if !matches!(host, "127.0.0.1" | "[::1]" | "*" | "0.0.0.0" | "[::]") {
                continue;
            }
            let Ok(port) = port.parse::<u16>() else {
                continue;
            };
            if port > 0 && pid > 0 && seen.insert((pid, port)) {
                found.push(Listener {
                    pid,
                    name: name.clone(),
                    port,
                });
            }
        }
    }
    found
}
#[cfg(unix)]
fn cwd_map(output: &str) -> HashMap<u32, PathBuf> {
    let mut pid = 0;
    let mut paths = HashMap::new();
    for line in output.lines() {
        if let Some(value) = line.strip_prefix('p') {
            pid = value.parse().unwrap_or(0);
        } else if let Some(value) = line.strip_prefix('n') {
            paths.insert(pid, PathBuf::from(value));
        }
    }
    paths
}
fn owner<'a>(cwd: &Path, roots: &'a [PathBuf]) -> Option<&'a PathBuf> {
    roots
        .iter()
        .filter(|root| cwd.starts_with(root))
        .max_by_key(|root| root.components().count())
}
#[cfg(unix)]
fn candidates(listing: &str, cwds: &str, roots: &[PathBuf]) -> Vec<Value> {
    let paths = cwd_map(cwds);
    listeners(listing).into_iter().filter_map(|listener| {
        if listener.pid == std::process::id() { return None; }
        let cwd=paths.get(&listener.pid)?;let root=owner(cwd,roots)?;
        Some(json!({"url":format!("http://localhost:{}/",listener.port),"source":"listener","pid":listener.pid,"process":listener.name,"worktreePath":root,"cwd":cwd}))
    }).collect()
}
// Verify the page, not the runtime name: node/python also host non-web sockets.
// Eight concurrent 750ms probes cost at most ceil(N / 8) * 750ms; the whole
// scan has a 3s budget so a repository with many silent listeners stays bounded.
async fn html_servers(candidates: Vec<Value>) -> Vec<Value> {
    let Ok(client) = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_millis(750))
        .build()
    else {
        return Vec::new();
    };
    let mut probes = futures_util::stream::iter(candidates.into_iter().map(|candidate| {
        let client = &client;
        async move {
            let response = client.get(candidate["url"].as_str()?).send().await.ok()?;
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)?
                .to_str()
                .ok()?;
            (response.status().is_success()
                && content_type
                    .split(';')
                    .next()?
                    .trim()
                    .eq_ignore_ascii_case("text/html"))
            .then_some(candidate)
        }
    }))
    .buffered(8);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    let mut found = Vec::new();
    while let Ok(Some(result)) = tokio::time::timeout_at(deadline, probes.next()).await {
        found.extend(result);
    }
    found
}
#[cfg(unix)]
fn command_owners(output: &str, roots: &[PathBuf]) -> HashMap<u32, PathBuf> {
    let mut owners = HashMap::new();
    for line in output.lines() {
        let mut words = line.split_whitespace();
        let Some(pid) = words.next().and_then(|word| word.parse().ok()) else {
            continue;
        };
        let deepest = words
            .filter_map(|word| owner(Path::new(word.trim_matches(['\"', '\''])), roots))
            .max_by_key(|root| root.components().count());
        if let Some(root) = deepest {
            owners.insert(pid, root.clone());
        }
    }
    owners
}
async fn command(binary: &str, args: &[&str]) -> Result<String, String> {
    let mut command = tokio::process::Command::new(crate::cli::resolve_cli(binary));
    command.args(args).kill_on_drop(true);
    let output = tokio::time::timeout(std::time::Duration::from_secs(5), command.output())
        .await
        .map_err(|_| format!("{binary} discovery timed out"))?
        .map_err(|e| format!("{binary}: {e}"))?;
    // lsof exits 1 when no process matches; that is an empty set, not a scan error.
    if !(output.status.success()
        || binary == "lsof" && output.status.code() == Some(1) && output.stderr.is_empty())
    {
        return Err(format!(
            "{binary}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| e.to_string())
}
fn settings(repo: &Path) -> Option<crate::config::RepoSettingsEntry> {
    crate::config::load_repo_settings()
        .repos
        .into_iter()
        .find_map(|(path, value)| {
            (crate::git::canonical_repo_root(Path::new(&path))
                == crate::git::canonical_repo_root(repo))
            .then_some(value)
        })
}
#[cfg(unix)]
fn attributed_candidates(
    listing: &str,
    cwds: &mut String,
    commands: &str,
    roots: &[PathBuf],
) -> Vec<Value> {
    let known = cwd_map(cwds);
    for (pid, root) in command_owners(commands, roots) {
        if known
            .get(&pid)
            .is_none_or(|cwd| owner(cwd, roots).is_none())
        {
            cwds.push_str(&format!("p{pid}\nfcwd\nn{}\n", root.display()));
        }
    }
    candidates(listing, cwds, roots)
}
#[cfg(unix)]
async fn live_candidates(roots: &[PathBuf]) -> Result<Vec<Value>, String> {
    let listing = command("lsof", &["-nP", "-iTCP", "-sTCP:LISTEN", "-Fpcn"]).await?;
    let pids = listeners(&listing)
        .iter()
        .map(|listener| listener.pid)
        .collect::<Vec<_>>();
    if !pids.is_empty() {
        let ids = pids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let mut cwds = command("lsof", &["-a", "-p", &ids, "-d", "cwd", "-Fn"]).await?;
        // Native process list, never `ps`: sandboxed hosts refuse to exec it, which used to
        // fail the whole discovery (and drop the configured dev-server URL with it).
        let commands = tokio::task::spawn_blocking(move || process_commands(&pids))
            .await
            .map_err(|e| e.to_string())??;
        return Ok(attributed_candidates(&listing, &mut cwds, &commands, roots));
    }
    Ok(Vec::new())
}

/// `pid argv...` lines, the shape `ps -o pid=,command=` printed, for the given processes.
/// A process whose arguments cannot be read (another user's) contributes its short name only.
#[cfg(unix)]
fn process_commands(pids: &[u32]) -> Result<String, String> {
    // One read per listener, not a snapshot of every process on the machine with its argv.
    let mut out = String::new();
    for process in pids
        .iter()
        .filter_map(|&pid| tuic_core::process_info::info(pid))
    {
        let argv = process.argv.as_deref().ok().filter(|argv| !argv.is_empty());
        out.push_str(&command_line(process.pid, argv, &process.comm));
    }
    Ok(out)
}

/// One `pid command` line. An argument may itself contain a newline, which would start a
/// second apparent row (`"\n200 /repo/app/server.js"` forges a command for pid 200 that
/// `command_owners` would accept), so line breaks inside the command become spaces.
#[cfg(unix)]
fn command_line(pid: u32, argv: Option<&[String]>, comm: &str) -> String {
    let command = argv.map_or_else(|| comm.to_string(), |argv| argv.join(" "));
    let command = command.replace(['\n', '\r'], " ");
    format!("{pid} {command}\n")
}

/// Only trusted TUIC settings supply scripts; .tuic.json cannot silently execute code.
pub(super) fn run_script(repo: &Path) -> Option<String> {
    settings(repo)
        .and_then(|entry| entry.run_script)
        .or_else(|| Some(crate::config::load_repo_defaults().run_script))
        .filter(|script| !script.trim().is_empty())
}
/// The worktree of the session being edited: Design Mode lists only that session's servers.
fn session_root(
    state: &crate::AppState,
    session: Option<&str>,
    roots: &[PathBuf],
) -> Option<PathBuf> {
    let entry = state.session_maps.sessions.get(session?)?;
    let cwd = entry.lock().cwd.clone()?;
    owner(Path::new(&cwd), roots).cloned()
}
pub(crate) async fn discover(
    state: &crate::AppState,
    repo: &Path,
    session: Option<&str>,
) -> Result<Value, String> {
    discover_with_source(state, repo, session, |roots| {
        let roots = roots.to_vec();
        async move {
            #[cfg(unix)]
            {
                live_candidates(&roots).await
            }
            #[cfg(not(unix))]
            {
                let _ = roots;
                Ok(Vec::new())
            }
        }
    })
    .await
}

async fn discover_with_source<F, Fut>(
    state: &crate::AppState,
    repo: &Path,
    session: Option<&str>,
    source: F,
) -> Result<Value, String>
where
    F: FnOnce(&[PathBuf]) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<Value>, String>>,
{
    let repo = repo.canonicalize().map_err(|e| e.to_string())?;
    let path = repo.to_str().ok_or("Repository path is not UTF-8")?;
    let worktrees = command("git", &["-C", path, "worktree", "list", "--porcelain"]).await?;
    let roots: Vec<_> = worktrees
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)
        .collect();
    let mut servers = Vec::new();
    if let Some(url) = settings(&repo)
        .and_then(|entry| entry.dev_server_url)
        .filter(|url| !url.trim().is_empty())
    {
        let parsed = url::Url::parse(&url).map_err(|_| "Invalid configured dev-server URL")?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("Configured URL must use HTTP or HTTPS".into());
        }
        servers.push(json!({"url":url,"source":"configured"}));
    }
    {
        let mut owned = source(&roots).await?;
        let scope = session_root(state, session, &roots);
        owned.retain(|row| {
            scope
                .as_ref()
                .is_some_and(|root| row["worktreePath"] == json!(root))
        });
        servers.extend(html_servers(owned).await);
    }
    let hints = session.map_or_else(Vec::new, |sid| {
        super::hints::advertised(state, sid, &servers)
    });
    servers.extend(hints);
    let mut seen = HashSet::new();
    servers.retain(|server| seen.insert(server["url"].as_str().unwrap_or_default().to_owned()));
    Ok(
        json!({"servers":servers,"canStart":run_script(&repo).is_some(),"scanNotice":if cfg!(windows){Some("Live listener attribution is unavailable on Windows; use the configured URL or address bar.")}else{None}}),
    )
}

#[cfg(all(test, unix))]
#[path = "discovery_tests.rs"]
pub(super) mod tests;
