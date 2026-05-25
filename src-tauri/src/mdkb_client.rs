use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MdkbSymbol {
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub line_start: u32,
    pub line_end: Option<u32>,
    pub signature: Option<String>,
    pub scope_context: Option<String>,
}

// Unix sockets are not available on Windows
#[cfg(not(unix))]
#[allow(dead_code)]
mod platform {
    use super::*;

    #[derive(Debug)]
    pub struct MdkbClient;

    impl MdkbClient {
        pub fn socket_path() -> PathBuf {
            PathBuf::new()
        }

        pub async fn connect() -> Result<Self> {
            bail!("mdkb: Unix socket client not available on this platform")
        }

        pub async fn call(&mut self, _method: &str, _params: Value) -> Result<Value> {
            bail!("mdkb: not available on this platform")
        }

        pub async fn ping(&mut self) -> Result<bool> {
            Ok(false)
        }

        pub async fn symbols_in_file(
            &mut self,
            _root: &str,
            _file: &str,
        ) -> Result<Vec<MdkbSymbol>> {
            Ok(vec![])
        }

        pub async fn symbol_at_position(
            &mut self,
            _root: &str,
            _file: &str,
            _line: u32,
            _col: Option<u32>,
        ) -> Result<Option<MdkbSymbol>> {
            Ok(None)
        }

        pub async fn code_graph(
            &mut self,
            _root: &str,
            _name: &str,
            _direction: &str,
        ) -> Result<Value> {
            bail!("mdkb: not available on this platform")
        }

        pub async fn code_find(
            &mut self,
            _root: &str,
            _name: &str,
            _kind: Option<&str>,
        ) -> Result<Vec<MdkbSymbol>> {
            Ok(vec![])
        }
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use anyhow::Context;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

    const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

    fn unwrap_text_field(resp: &Value) -> Result<String> {
        match resp.get("text").and_then(Value::as_str) {
            Some(t) => Ok(t.to_string()),
            None => serde_json::to_string(resp).context("mdkb: serialize fallback response"),
        }
    }

    #[derive(Debug)]
    pub struct MdkbClient {
        #[cfg(not(test))]
        stream: UnixStream,
        #[cfg(test)]
        pub(super) stream: UnixStream,
    }

    #[derive(Debug, Deserialize)]
    struct RpcResponse {
        #[allow(dead_code)]
        id: Value,
        result: Option<Value>,
        error: Option<RpcError>,
    }

    #[derive(Debug, Deserialize)]
    struct RpcError {
        code: i32,
        message: String,
    }

    impl MdkbClient {
        pub fn socket_path() -> PathBuf {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join(".mdkb/daemon-hook.sock")
        }

        pub async fn connect() -> Result<Self> {
            let path = Self::socket_path();
            let stream = UnixStream::connect(&path)
                .await
                .with_context(|| format!("mdkb: connect to {}", path.display()))?;
            Ok(Self { stream })
        }

        pub async fn call(&mut self, method: &str, params: Value) -> Result<Value> {
            let id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
            let req = json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params,
            });
            let body = serde_json::to_vec(&req)?;
            let len = u32::try_from(body.len()).context("request too large")?;

            self.stream.write_all(&len.to_le_bytes()).await?;
            self.stream.write_all(&body).await?;
            self.stream.flush().await?;

            let mut hdr = [0u8; 4];
            self.stream
                .read_exact(&mut hdr)
                .await
                .context("mdkb: read response header")?;
            let resp_len = u32::from_le_bytes(hdr) as usize;
            if resp_len == 0 || resp_len > MAX_RESPONSE_BYTES {
                bail!("mdkb: invalid response length {resp_len}");
            }

            let mut resp_buf = vec![0u8; resp_len];
            self.stream
                .read_exact(&mut resp_buf)
                .await
                .context("mdkb: read response body")?;

            let resp: RpcResponse =
                serde_json::from_slice(&resp_buf).context("mdkb: parse response")?;

            if let Some(err) = resp.error {
                bail!("mdkb RPC error {}: {}", err.code, err.message);
            }

            resp.result
                .ok_or_else(|| anyhow::anyhow!("mdkb: response missing both result and error"))
        }

        pub async fn ping(&mut self) -> Result<bool> {
            let resp = self.call("ping", json!({})).await?;
            Ok(resp.get("pong").and_then(|v| v.as_bool()).unwrap_or(false))
        }

        pub async fn symbols_in_file(&mut self, root: &str, file: &str) -> Result<Vec<MdkbSymbol>> {
            let resp = self
                .call(
                    "symbols_in_file",
                    json!({
                        "root": root,
                        "file": file,
                    }),
                )
                .await?;
            let text = unwrap_text_field(&resp)?;
            let symbols: Vec<MdkbSymbol> =
                serde_json::from_str(&text).context("mdkb: parse symbols_in_file response")?;
            Ok(symbols)
        }

        pub async fn symbol_at_position(
            &mut self,
            root: &str,
            file: &str,
            line: u32,
            col: Option<u32>,
        ) -> Result<Option<MdkbSymbol>> {
            let resp = self
                .call(
                    "symbol_at_position",
                    json!({
                        "root": root,
                        "file": file,
                        "line": line,
                        "col": col,
                    }),
                )
                .await?;
            let text = unwrap_text_field(&resp)?;
            if text == "null" || text.is_empty() {
                return Ok(None);
            }
            let sym: MdkbSymbol =
                serde_json::from_str(&text).context("mdkb: parse symbol_at_position response")?;
            Ok(Some(sym))
        }

        pub async fn code_graph(
            &mut self,
            root: &str,
            name: &str,
            direction: &str,
        ) -> Result<Value> {
            let resp = self
                .call(
                    "code_graph",
                    json!({
                        "root": root,
                        "name": name,
                        "direction": direction,
                    }),
                )
                .await?;
            let text = unwrap_text_field(&resp)?;
            serde_json::from_str(&text).context("mdkb: parse code_graph response")
        }

        pub async fn code_find(
            &mut self,
            root: &str,
            name: &str,
            kind: Option<&str>,
        ) -> Result<Vec<MdkbSymbol>> {
            let mut params = json!({ "root": root, "name": name });
            if let Some(k) = kind {
                params["kind"] = json!(k);
            }
            let resp = self.call("code_find", params).await?;
            let text = unwrap_text_field(&resp)?;
            let symbols: Vec<MdkbSymbol> =
                serde_json::from_str(&text).context("mdkb: parse code_find response")?;
            Ok(symbols)
        }
    }
}

pub use platform::MdkbClient;

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{UnixListener, UnixStream};

    async fn spawn_mock_server() -> (PathBuf, tokio::task::JoinHandle<()>) {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();
        let path = sock_path.clone();

        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            loop {
                let mut hdr = [0u8; 4];
                if stream.read_exact(&mut hdr).await.is_err() {
                    break;
                }
                let len = u32::from_le_bytes(hdr) as usize;
                let mut body = vec![0u8; len];
                if stream.read_exact(&mut body).await.is_err() {
                    break;
                }

                let req: Value = serde_json::from_slice(&body).unwrap();
                let id = req.get("id").cloned().unwrap_or(Value::Null);
                let method = req.get("method").and_then(Value::as_str).unwrap_or("");

                let response = match method {
                    "ping" => json!({"jsonrpc": "2.0", "id": id, "result": {"pong": true}}),
                    "symbols_in_file" => {
                        let symbols = json!([
                            {"name": "foo", "kind": "Function", "file_path": "src/main.rs", "line_start": 1, "line_end": 10, "signature": "fn foo()", "scope_context": null},
                            {"name": "bar", "kind": "Function", "file_path": "src/main.rs", "line_start": 12, "line_end": 20, "signature": "fn bar(x: i32)", "scope_context": "foo"}
                        ]);
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {"text": symbols.to_string(), "tokens": 0}
                        })
                    }
                    "bad_method" => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": -32601, "message": "unknown tool: bad_method"}
                    }),
                    _ => json!({"jsonrpc": "2.0", "id": id, "result": null}),
                };

                let resp_bytes = serde_json::to_vec(&response).unwrap();
                let resp_len = resp_bytes.len() as u32;
                stream.write_all(&resp_len.to_le_bytes()).await.unwrap();
                stream.write_all(&resp_bytes).await.unwrap();
            }
            // Keep dir alive
            drop(dir);
        });

        // Wait for socket to be ready
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        (path, handle)
    }

    async fn connect_to_mock(path: &Path) -> platform::MdkbClient {
        let stream = UnixStream::connect(path).await.unwrap();
        platform::MdkbClient { stream }
    }

    #[tokio::test]
    async fn test_ping() {
        let (path, _server) = spawn_mock_server().await;
        let mut client = connect_to_mock(&path).await;
        assert!(client.ping().await.unwrap());
    }

    #[tokio::test]
    async fn test_symbols_in_file() {
        let (path, _server) = spawn_mock_server().await;
        let mut client = connect_to_mock(&path).await;
        let symbols = client
            .symbols_in_file("/repo", "src/main.rs")
            .await
            .unwrap();
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "foo");
        assert_eq!(symbols[0].line_start, 1);
        assert_eq!(symbols[1].name, "bar");
        assert_eq!(symbols[1].scope_context.as_deref(), Some("foo"));
    }

    #[tokio::test]
    async fn test_rpc_error_propagation() {
        let (path, _server) = spawn_mock_server().await;
        let mut client = connect_to_mock(&path).await;
        let err = client
            .call("bad_method", json!({"root": "/repo"}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unknown tool: bad_method"));
    }

    #[tokio::test]
    async fn test_connection_refused() {
        let result = MdkbClient::connect().await;
        // Will fail unless mdkb daemon is actually running — that's expected in test env
        // The important thing is it doesn't panic
        if result.is_err() {
            assert!(result.unwrap_err().to_string().contains("mdkb: connect"));
        }
    }
}
