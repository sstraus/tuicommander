//! Length-framed Chromium native messaging, pinned to one explicit IPC endpoint.
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub(crate) const EXTENSION_ID: &str = "eefbgjmmpaimpeaflioojmdipmfchefo";
const MAX_FRAME: usize = 1024 * 1024;

pub(crate) fn read_frame(reader: &mut impl Read) -> Result<Option<Value>, String> {
    let mut size = [0; 4];
    if reader.read(&mut size[..1]).map_err(|e| e.to_string())? == 0 {
        return Ok(None);
    }
    reader
        .read_exact(&mut size[1..])
        .map_err(|e| e.to_string())?;
    let size = u32::from_ne_bytes(size) as usize;
    if size == 0 || size > MAX_FRAME {
        return Err("Native frame must be 1 byte to 1 MiB".into());
    }
    let mut bytes = vec![0; size];
    reader.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| e.to_string())
}
fn write_frame(writer: &mut impl Write, value: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FRAME {
        return Err("Native response exceeds 1 MiB".into());
    }
    writer
        .write_all(&(bytes.len() as u32).to_ne_bytes())
        .map_err(|e| e.to_string())?;
    writer.write_all(&bytes).map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())
}

async fn http(
    endpoint: &str,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<Value, String> {
    #[cfg(unix)]
    let mut stream = tokio::net::UnixStream::connect(endpoint)
        .await
        .map_err(|e| format!("IPC {endpoint}: {e}"))?;
    #[cfg(windows)]
    let mut stream = tokio::net::windows::named_pipe::ClientOptions::new()
        .open(endpoint)
        .map_err(|e| format!("IPC {endpoint}: {e}"))?;
    let body = body.map(Value::to_string);
    stream
        .write_all(&tuic_ipc::http::request(method, path, body.as_deref(), &[]))
        .await
        .map_err(|e| e.to_string())?;
    let mut decoder = tuic_ipc::http::ResponseDecoder::default();
    let mut bytes = [0; 8192];
    loop {
        let n = stream.read(&mut bytes).await.map_err(|e| e.to_string())?;
        decoder.push(&bytes[..n]);
        if let Some(response) = decoder.response(n == 0).map_err(|e| e.to_string())? {
            if !response.is_success() {
                return Err(format!(
                    "IPC {endpoint}: HTTP {} for {path}. Restart TUICommander to load Design Mode v2.",
                    response.status
                ));
            }
            return serde_json::from_str(&response.body)
                .map_err(|e| format!("Invalid IPC response: {e}"));
        }
    }
}
async fn ipc(
    endpoint: &str,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<Value, String> {
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        http(endpoint, method, path, body),
    )
    .await
    .map_err(|_| format!("IPC {endpoint}: timed out requesting {path}"))?
}

async fn dispatch(config: &Value, message: &Value) -> Result<Value, String> {
    if message["version"]
        .as_str()
        .and_then(|v| v.split('.').next())
        != Some("2")
    {
        return Err("Design Mode version mismatch; reinstall the extension".into());
    }
    if message["repoPath"] != config["repoPath"] {
        return Err("Extension repository does not match its installed profile".into());
    }
    let endpoint = config["endpoint"]
        .as_str()
        .ok_or("Native host has no IPC endpoint")?;
    let health = ipc(endpoint, "GET", "/health", None).await?;
    if health["instance_id"] != config["instanceId"] {
        return Err(format!(
            "IPC {endpoint}: TUICommander instance changed; reopen Design Mode from that instance"
        ));
    }
    match message["action"].as_str() {
        Some("hello") => {
            Ok(json!({"version":"2.0.0","endpoint":endpoint,"instanceId":health["instance_id"]}))
        }
        Some("mode_status") => ipc(endpoint, "GET", "/design-mode", None).await,
        Some("list_agents") => {
            let rows = ipc(endpoint, "GET", "/sessions", None).await?;
            let repo = config["repoPath"]
                .as_str()
                .ok_or("Native host has no repository")?;
            Ok(json!(tuic_ipc::review_agents::review_agents(
                &rows,
                Path::new(repo)
            )?))
        }
        // Feedback and discovery stay in the core backend. No generic proxy is exposed.
        Some(
            "pick" | "send" | "list_dev_servers" | "start_dev_server" | "heartbeat"
            | "command_next" | "command_reply" | "command_disconnect",
        ) => {
            let reply = ipc(endpoint, "POST", "/design-mode/extension", Some(message)).await?;
            if reply["ok"] == true {
                Ok(reply["result"].clone())
            } else {
                Err(reply["error"]
                    .as_str()
                    .unwrap_or("TUICommander rejected feedback")
                    .to_owned())
            }
        }
        _ => Err("Unknown Design Mode action".into()),
    }
}

pub(crate) async fn run(origin: &str, config_path: &Path) -> Result<(), String> {
    if origin != format!("chrome-extension://{EXTENSION_ID}/") {
        return Err("Unrecognised native messaging origin".into());
    }
    let config: Value = serde_json::from_slice(
        &std::fs::read(config_path)
            .map_err(|e| format!("Native host config {}: {e}", config_path.display()))?,
    )
    .map_err(|e| e.to_string())?;
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut input = stdin.lock();
        loop {
            match read_frame(&mut input) {
                Ok(Some(message)) => {
                    if tx.blocking_send(Ok(message)).is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    let _ = tx.blocking_send(Err(e));
                    break;
                }
            }
        }
    });
    // A long-lived receive must not block replies or normal extension requests.
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<Value>(32);
    let mut tasks = tokio::task::JoinSet::new();
    let mut input_open = true;
    let mut outstanding = 0usize;
    loop {
        if !input_open && outstanding == 0 {
            break;
        }
        tokio::select! {
            message = rx.recv(), if input_open => {
                let Some(message) = message else {
                    input_open = false;
                    let disconnected = json!({"version":"2.0.0", "repoPath":config["repoPath"], "action":"command_disconnect"});
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), dispatch(&config, &disconnected)).await;
                    continue;
                };
                let message = message?;
                if tasks.len() >= 32 {
                    write_frame(&mut std::io::stdout().lock(), &json!({"id":message["id"],"ok":false,"error":"Too many native requests"}))?;
                    continue;
                }
                outstanding += 1;
                let config = config.clone();
                let tx = out_tx.clone();
                tasks.spawn(async move {
                    let reply = match dispatch(&config, &message).await {
                        Ok(result) => json!({"id":message["id"],"ok":true,"result":result}),
                        Err(error) => json!({"id":message["id"],"ok":false,"error":error}),
                    };
                    let _ = tx.send(reply).await;
                });
            }
            Some(reply) = out_rx.recv() => { outstanding -= 1; write_frame(&mut std::io::stdout().lock(), &reply)?; }
            Some(_) = tasks.join_next(), if !tasks.is_empty() => {}
        }
    }
    tasks.abort_all();
    Ok(())
}

pub(crate) async fn install(
    profile: &Path,
    repo: &Path,
    endpoint: &str,
    session_id: Option<&str>,
) -> Result<Value, String> {
    if !profile.is_absolute() || !repo.is_absolute() {
        return Err("Profile and repository must be absolute paths".into());
    }
    let repo = repo.canonicalize().map_err(|e| e.to_string())?;
    tuic_ipc::review_agents::repository_key(&repo).ok_or("Not a repository")?;
    let health = ipc(endpoint, "GET", "/health", None).await?;
    let identity = health["instance_id"]
        .as_str()
        .ok_or("TUICommander did not identify its instance")?;
    let extension = profile.join("tuic-design-extension");
    let native = profile.join("tuic-design-host");
    std::fs::create_dir_all(&extension).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&native).map_err(|e| e.to_string())?;
    for (name, contents) in super::native_assets::ASSETS {
        std::fs::write(extension.join(name), contents).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        extension.join("binding.json"),
        json!({"repoPath":repo,"sessionId":session_id}).to_string(),
    )
    .map_err(|e| e.to_string())?;
    let host = native.join(if cfg!(windows) {
        "tuic-design-host.exe"
    } else {
        "tuic-design-host"
    });
    // A fresh inode avoids macOS invalidating the cached executable signature.
    let staged = host.with_extension(format!("install-{}", std::process::id()));
    std::fs::copy(std::env::current_exe().map_err(|e| e.to_string())?, &staged)
        .map_err(|e| e.to_string())?;
    #[cfg(windows)]
    if host.exists() {
        std::fs::remove_file(&host).map_err(|e| e.to_string())?;
    }
    std::fs::rename(staged, &host).map_err(|e| e.to_string())?;
    let config = json!({"repoPath":repo,"endpoint":endpoint,"instanceId":identity});
    std::fs::write(host.with_extension("json"), config.to_string()).map_err(|e| e.to_string())?;
    let manifest = host_manifest(&host);
    let hosts = profile.join("NativeMessagingHosts");
    std::fs::create_dir_all(&hosts).map_err(|e| e.to_string())?;
    let manifest_path = hosts.join("com.tuic.design_mode.json");
    std::fs::write(&manifest_path, manifest.to_string()).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        let status = std::process::Command::new("reg")
            .args([
                "add",
                r"HKCU\Software\Microsoft\Edge\NativeMessagingHosts\com.tuic.design_mode",
                "/ve",
                "/t",
                "REG_SZ",
                "/d",
            ])
            .arg(&manifest_path)
            .arg("/f")
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("Native host registry registration failed".into());
        }
    }
    Ok(
        json!({"extensionPath":extension,"profilePath":profile,"endpoint":endpoint,"instanceId":identity}),
    )
}
fn host_manifest(host: &Path) -> Value {
    json!({"name":"com.tuic.design_mode","description":"TUICommander Design Mode","path":host,"type":"stdio","allowed_origins":[format!("chrome-extension://{EXTENSION_ID}/")]})
}

#[cfg(test)]
mod tests {
    use super::*;
    // Catches: heartbeat omitted from the native host allowlist or its reply envelope changed.
    #[cfg(unix)]
    #[tokio::test]
    async fn native_host_forwards_heartbeat_to_the_configured_instance() {
        // Short root: a sandbox TMPDIR leaves too little sun_path budget.
        let dir = tempfile::tempdir_in(tuic_test_support::short_socket_test_temp_root()).unwrap();
        let endpoint = dir.path().join("beat.sock");
        let listener = tokio::net::UnixListener::bind(&endpoint).unwrap();
        let message = json!({"version":"2.0.0","action":"heartbeat","repoPath":"/repo"});
        let expected = message.clone();
        let server = tokio::spawn(async move {
            for result in [
                json!({"instance_id":"test"}),
                json!({"ok":true,"result":{"armed":true,"reloadSeq":2}}),
            ] {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let mut chunk = [0; 4096];
                loop {
                    let n = stream.read(&mut chunk).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&bytes);
                    if let Some((headers, body)) = text.split_once("\r\n\r\n") {
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|value| value.parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if body.len() >= length {
                            if result.get("result").is_some() {
                                assert!(headers.starts_with("POST /design-mode/extension "));
                                assert_eq!(serde_json::from_str::<Value>(body).unwrap(), expected);
                            } else {
                                assert!(headers.starts_with("GET /health "));
                            }
                            break;
                        }
                    }
                }
                let body = result.to_string();
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        assert_eq!(
            dispatch(
                &json!({"repoPath":"/repo","endpoint":endpoint,"instanceId":"test"}),
                &message
            )
            .await
            .unwrap(),
            json!({"armed":true,"reloadSeq":2})
        );
        server.await.unwrap();
        assert!(
            super::super::native_assets::ASSETS
                .iter()
                .any(|(name, _)| *name == "heartbeat.js")
        );
    }

    // Catches: fragmented reads truncating Unicode or interpreting the next frame as payload.
    #[test]
    fn native_frames_preserve_unicode_and_reject_truncation_and_oversize() {
        let mut frames = Vec::new();
        write_frame(&mut frames, &json!({"text":"é漢字"})).unwrap();
        write_frame(&mut frames, &json!({"next":true})).unwrap();
        let mut input = std::io::Cursor::new(&frames);
        assert_eq!(
            read_frame(&mut input).unwrap(),
            Some(json!({"text":"é漢字"}))
        );
        assert_eq!(read_frame(&mut input).unwrap(), Some(json!({"next":true})));
        assert!(read_frame(&mut std::io::Cursor::new(&frames[..5])).is_err());
        assert!(read_frame(&mut std::io::Cursor::new(u32::MAX.to_ne_bytes())).is_err());
    }
}
