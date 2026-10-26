use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdkbPing {
    pub pong: bool,
    pub version: Option<String>,
}

/// A symbol as mdkb reports it. `line_start`/`line_end` are **0-based** — see
/// `mdkb_commands::editor_line` for the conversion to editor lines.
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

/// `code_find`'s envelope on the wire.
///
/// Both counts are optional because a daemon that does not send them is a
/// different fact from one that sends zero, and the two must not collapse: an
/// absent `total` means "mdkb did not say how many there were", never "there
/// were as many as arrived".
///
/// `cfg(unix)` because only the socket client decodes it; the Windows stub
/// answers without ever talking to a daemon.
#[cfg(unix)]
#[derive(Debug, Deserialize)]
struct CodeFindResponse {
    #[serde(default)]
    total: Option<u32>,
    #[serde(default)]
    showing: Option<u32>,
    symbols: Vec<MdkbSymbol>,
}

/// What `code_find` found, and how much of it this is.
///
/// mdkb caps the rows at `limit.unwrap_or(50)` and ships `total` precisely so a
/// capped list cannot read as the whole set. This client used to parse that
/// count and throw it away, so fifty matches out of three hundred reached the
/// UI looking exactly like fifty out of fifty, and raising the cap upstream
/// would have changed a displayed number with no shape change to notice.
///
/// `total` is NOT `symbols.len()`. `None` means mdkb sent no count.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MdkbFindResult {
    pub symbols: Vec<MdkbSymbol>,
    pub total: Option<u32>,
}

impl MdkbFindResult {
    /// Whether mdkb held rows back. `None` when it did not say — a caller that
    /// must render a count has to show that it does not know one.
    pub fn capped(&self) -> Option<bool> {
        self.total.map(|total| total as usize > self.symbols.len())
    }
}

// Unix sockets are not available on Windows
//
// Every method stays `async` with nothing to await: this stub must present the
// exact signature the socket client does, or every call site would need a cfg.
#[cfg(not(unix))]
#[allow(dead_code, clippy::unused_async_trait_impl)]
mod platform {
    use super::*;

    #[derive(Debug)]
    pub struct MdkbClient;

    impl MdkbClient {
        pub fn socket_path() -> Option<PathBuf> {
            None
        }

        pub async fn connect() -> Result<Self> {
            bail!("mdkb: Unix socket client not available on this platform")
        }

        pub async fn call(&mut self, _method: &str, _params: Value) -> Result<Value> {
            bail!("mdkb: not available on this platform")
        }

        pub async fn ping_info(&mut self) -> Result<MdkbPing> {
            Ok(MdkbPing {
                pong: false,
                version: None,
            })
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
        ) -> Result<Vec<MdkbSymbol>> {
            bail!("mdkb: not available on this platform")
        }

        pub async fn code_find(
            &mut self,
            _root: &str,
            _name: &str,
            _kind: Option<&str>,
        ) -> Result<MdkbFindResult> {
            Ok(MdkbFindResult::default())
        }
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use anyhow::Context;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

    const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

    /// How long one request/response exchange may take before the client gives
    /// up. A daemon that accepts the connection and then never answers used to
    /// wedge the caller for ever — and, through the shared `MdkbDaemon` lock,
    /// every other mdkb caller in the process with it.
    const CALL_TIMEOUT: Duration = Duration::from_secs(10);

    fn unwrap_text_field(resp: &Value) -> Result<String> {
        match resp.get("text").and_then(Value::as_str) {
            Some(t) => Ok(t.to_string()),
            None => serde_json::to_string(resp).context("mdkb: serialize fallback response"),
        }
    }

    /// Pull the resolved symbols out of a `code_graph` envelope.
    ///
    /// A missing `symbols` is an error, not an empty result: mdkb sends `[]`
    /// when a symbol genuinely has no callers, so "absent" can only mean a
    /// daemon too old to carry the field, and reporting that as "no callers"
    /// would be a silent lie.
    pub(super) fn parse_code_graph_symbols(resp: &Value) -> Result<Vec<MdkbSymbol>> {
        let symbols = resp
            .get("symbols")
            .ok_or_else(|| anyhow::anyhow!("mdkb: code_graph response has no 'symbols'"))?;
        serde_json::from_value(symbols.clone()).context("mdkb: parse code_graph symbols")
    }

    /// Frame the request, send it, and read the framed reply.
    ///
    /// Split out of `call` so a single `timeout` can bound the whole exchange
    /// while borrowing nothing but the socket.
    async fn exchange(stream: &mut UnixStream, body: &[u8]) -> Result<Vec<u8>> {
        let len = u32::try_from(body.len()).context("request too large")?;

        stream.write_all(&len.to_le_bytes()).await?;
        stream.write_all(body).await?;
        stream.flush().await?;

        let mut hdr = [0u8; 4];
        stream
            .read_exact(&mut hdr)
            .await
            .context("mdkb: read response header")?;
        let resp_len = u32::from_le_bytes(hdr) as usize;
        if resp_len == 0 || resp_len > MAX_RESPONSE_BYTES {
            bail!("mdkb: invalid response length {resp_len}");
        }

        let mut resp_buf = vec![0u8; resp_len];
        stream
            .read_exact(&mut resp_buf)
            .await
            .context("mdkb: read response body")?;
        Ok(resp_buf)
    }

    #[derive(Debug)]
    pub struct MdkbClient {
        stream: UnixStream,
        timeout: Duration,
        /// Set when a deadline cut an exchange in half. The daemon may still
        /// write the abandoned reply, and the next `call` would read it as its
        /// own length header — so the connection can never be trusted again.
        poisoned: bool,
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
        /// mdkb's daemon socket, `~/.mdkb/daemon-hook.sock`. `None` when there
        /// is no home directory: there is no shared fallback (a predictable
        /// socket in `/tmp` is one any local user could plant).
        pub fn socket_path() -> Option<PathBuf> {
            Self::socket_path_in(dirs::home_dir())
        }

        pub(crate) fn socket_path_in(home: Option<PathBuf>) -> Option<PathBuf> {
            home.map(|home| home.join(".mdkb/daemon-hook.sock"))
        }

        pub async fn connect() -> Result<Self> {
            Self::connect_to(Self::socket_path()).await
        }

        pub(crate) async fn connect_to(path: Option<PathBuf>) -> Result<Self> {
            let path =
                path.context("mdkb: no home directory, so no daemon socket to connect to")?;
            let stream = UnixStream::connect(&path)
                .await
                .with_context(|| format!("mdkb: connect to {}", path.display()))?;
            Ok(Self {
                stream,
                timeout: CALL_TIMEOUT,
                poisoned: false,
            })
        }

        /// Build a client on an already-connected socket with an explicit
        /// deadline. Tests use it to keep the suite fast.
        #[cfg(test)]
        pub(crate) fn from_stream(stream: UnixStream, timeout: Duration) -> Self {
            Self {
                stream,
                timeout,
                poisoned: false,
            }
        }

        pub async fn call(&mut self, method: &str, params: Value) -> Result<Value> {
            if self.poisoned {
                bail!("mdkb: connection abandoned after a timeout, cannot send '{method}'");
            }

            let id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
            let req = json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params,
            });
            let body = serde_json::to_vec(&req)?;

            // One deadline over the whole exchange, so a stalled write and a
            // reply that never arrives are bounded by the same budget.
            let resp_buf =
                match tokio::time::timeout(self.timeout, exchange(&mut self.stream, &body)).await {
                    Ok(result) => result?,
                    Err(_) => {
                        self.poisoned = true;
                        bail!(
                            "mdkb: '{method}' timed out after {}ms",
                            self.timeout.as_millis()
                        );
                    }
                };

            let resp: RpcResponse =
                serde_json::from_slice(&resp_buf).context("mdkb: parse response")?;

            if let Some(err) = resp.error {
                bail!("mdkb RPC error {}: {}", err.code, err.message);
            }

            resp.result
                .ok_or_else(|| anyhow::anyhow!("mdkb: response missing both result and error"))
        }

        pub async fn ping_info(&mut self) -> Result<MdkbPing> {
            let resp = self.call("ping", json!({})).await?;
            Ok(MdkbPing {
                pong: resp.get("pong").and_then(Value::as_bool).unwrap_or(false),
                version: resp
                    .get("version")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
            })
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
        ) -> Result<Vec<MdkbSymbol>> {
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
            // `text` is prose written for agents — never parse it. The resolved
            // symbols ride alongside it in `symbols`.
            parse_code_graph_symbols(&resp)
        }

        pub async fn code_find(
            &mut self,
            root: &str,
            name: &str,
            kind: Option<&str>,
        ) -> Result<MdkbFindResult> {
            let mut params = json!({ "root": root, "name": name });
            if let Some(k) = kind {
                params["kind"] = json!(k);
            }
            let resp = self.call("code_find", params).await?;
            let text = unwrap_text_field(&resp)?;
            // `code_find` is the one code method that does not return a bare
            // array: the row cap means `total` has to travel with the rows.
            let found: CodeFindResponse =
                serde_json::from_str(&text).context("mdkb: parse code_find response")?;
            // `showing` must describe the rows that actually arrived. When it
            // does not, this is not the envelope this client understands, and
            // guessing which of the two numbers to believe would put a wrong
            // count on screen. Say so instead.
            if let Some(showing) = found.showing
                && showing as usize != found.symbols.len()
            {
                bail!(
                    "mdkb: code_find reported showing={showing} but sent {} rows",
                    found.symbols.len()
                );
            }
            Ok(MdkbFindResult {
                symbols: found.symbols,
                total: found.total,
            })
        }
    }
}

pub use platform::MdkbClient;

#[cfg(all(test, unix))]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{UnixListener, UnixStream};

    // Catches: a `/tmp` (or any shared) fallback when HOME is unresolvable.
    #[test]
    fn no_home_means_no_daemon_socket() {
        assert_eq!(MdkbClient::socket_path_in(None), None);
        assert_eq!(
            MdkbClient::socket_path_in(Some(PathBuf::from("/h"))),
            Some(PathBuf::from("/h/.mdkb/daemon-hook.sock"))
        );
    }

    // Catches: connect() reaching for any socket when there is no path.
    #[tokio::test]
    async fn connect_without_socket_path_errors() {
        let err = MdkbClient::connect_to(None).await.unwrap_err();
        assert!(err.to_string().contains("no home directory"), "{err}");
    }

    async fn spawn_mock_server() -> (PathBuf, tokio::task::JoinHandle<()>) {
        let dir = crate::test_support::short_socket_tempdir();
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

                // Every arm mirrors the real 3.7.x wire shape, verbatim — the
                // shapes differ per method (bare array / envelope / prose +
                // symbols) and that difference is exactly what silently broke
                // before. An unknown method is an ERROR, never a null result:
                // a permissive fallback lets a new call "pass" against a shape
                // the daemon never sends.
                let response = match method {
                    "ping" => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {"pong": true, "version": "9.8.7"}
                    }),
                    "symbols_in_file" => {
                        // Bare array. Lines are 0-based, as mdkb stores them.
                        let symbols = json!([
                            {"name": "foo", "kind": "Function", "file_path": "src/main.rs", "line_start": 0, "line_end": 9, "col_start": 0, "col_end": 1, "signature": "fn foo()", "scope_context": "Module"},
                            {"name": "bar", "kind": "Method", "file_path": "src/main.rs", "line_start": 11, "line_end": 19, "col_start": 4, "col_end": 5, "signature": "fn bar(x: i32)", "scope_context": "ClassMember { class_name: None }"}
                        ]);
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {"text": symbols.to_string(), "tokens": 0}
                        })
                    }
                    "symbol_at_position" => {
                        // Single object, and NO `scope_context` — this method
                        // sends `module_path` instead.
                        let sym = json!({
                            "name": "foo", "kind": "Function", "file_path": "src/main.rs",
                            "line_start": 41, "line_end": 47, "col_start": 0, "col_end": 1,
                            "signature": "fn foo()", "module_path": null
                        });
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {"text": sym.to_string(), "tokens": 0}
                        })
                    }
                    "code_find" => {
                        // Envelope, not a bare array: `total` travels with the
                        // rows so a capped list cannot read as the whole set.
                        let found = json!({
                            "total": 7,
                            "showing": 1,
                            "symbols": [
                                {"name": "foo", "kind": "Function", "file_path": "src/main.rs", "line_start": 4, "line_end": 8, "col_start": 0, "col_end": 1, "signature": "fn foo()", "scope_context": "Module"}
                            ]
                        });
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {"text": found.to_string(), "tokens": 0}
                        })
                    }
                    "code_graph" => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        // `text` is prose for agents; `symbols` is the
                        // machine-readable half, in the envelope rather than
                        // stringified inside `text`.
                        "result": {
                            "text": "foo (Function) is called by 1 function(s):\n\n  sym#42 Method caller in src/lib.rs:6\n",
                            "tokens": 0,
                            "symbols": [
                                {"name": "caller", "kind": "Method", "file_path": "src/lib.rs", "line_start": 6, "line_end": 12, "col_start": 4, "col_end": 5, "signature": "fn caller()", "scope_context": "Module"}
                            ]
                        }
                    }),
                    other => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": -32601, "message": format!("unknown tool: {other}")}
                    }),
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

    /// A deadline short enough to keep the suite fast, long enough that a
    /// mock answering over a local socket is never a flake.
    const TEST_TIMEOUT: Duration = Duration::from_millis(300);

    async fn connect_to_mock(path: &Path) -> MdkbClient {
        let stream = UnixStream::connect(path).await.unwrap();
        MdkbClient::from_stream(stream, TEST_TIMEOUT)
    }

    #[tokio::test]
    async fn test_ping_reports_daemon_version() {
        let (path, _server) = spawn_mock_server().await;
        let mut client = connect_to_mock(&path).await;
        assert_eq!(
            client.ping_info().await.unwrap(),
            MdkbPing {
                pong: true,
                version: Some("9.8.7".to_string()),
            }
        );
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
        // Passed through raw — 0-based here, converted at the command boundary.
        assert_eq!(symbols[0].line_start, 0);
        assert_eq!(symbols[1].name, "bar");
        assert_eq!(
            symbols[1].scope_context.as_deref(),
            Some("ClassMember { class_name: None }")
        );
    }

    #[tokio::test]
    async fn symbol_at_position_parses_a_response_without_scope_context() {
        // `symbol_at_position` omits `scope_context` where the other methods
        // send it. A required field would make this method fail to parse.
        let (path, _server) = spawn_mock_server().await;
        let mut client = connect_to_mock(&path).await;
        let sym = client
            .symbol_at_position("/repo", "src/main.rs", 42, None)
            .await
            .unwrap()
            .expect("symbol present");
        assert_eq!(sym.name, "foo");
        assert_eq!(sym.line_start, 41);
        assert_eq!(sym.scope_context, None);
    }

    #[tokio::test]
    async fn code_find_reads_rows_out_of_the_total_envelope() {
        // Regression: mdkb 3.7.14 wrapped the rows in {total, showing, symbols}.
        // Deserializing the envelope as a bare Vec fails, and the command layer
        // swallows the error as "no results" — a silent, total blind spot.
        let (path, _server) = spawn_mock_server().await;
        let mut client = connect_to_mock(&path).await;
        let found = client.code_find("/repo", "foo", None).await.unwrap();
        assert_eq!(found.symbols.len(), 1, "envelope rows must be unwrapped");
        assert_eq!(found.symbols[0].name, "foo");
        assert_eq!(found.symbols[0].file_path, "src/main.rs");
    }

    /// The mock answers `total: 7` with one row: a capped list. That count must
    /// survive the parse, or the UI shows one match where there are seven.
    #[tokio::test]
    async fn a_capped_code_find_list_does_not_read_as_the_whole_set() {
        let (path, _server) = spawn_mock_server().await;
        let mut client = connect_to_mock(&path).await;
        let found = client.code_find("/repo", "foo", None).await.unwrap();

        assert_eq!(found.total, Some(7), "the unclamped count must survive");
        assert_eq!(
            found.capped(),
            Some(true),
            "one row out of seven matches is a capped list"
        );
    }

    /// The other half of the same fact: a list that is complete must say so.
    /// Without this, `capped()` could return `Some(true)` always and the test
    /// above would still pass.
    #[tokio::test]
    async fn a_complete_code_find_list_is_not_reported_as_capped() {
        let complete = MdkbFindResult {
            symbols: vec![MdkbSymbol {
                name: "foo".into(),
                kind: "Function".into(),
                file_path: "src/main.rs".into(),
                line_start: 4,
                line_end: Some(8),
                signature: None,
                scope_context: None,
            }],
            total: Some(1),
        };
        assert_eq!(complete.capped(), Some(false));
    }

    /// A daemon that sends no count leaves the question open. Answering it with
    /// the row count would be the same lie in a quieter voice.
    #[tokio::test]
    async fn a_missing_total_is_unknown_not_complete() {
        let silent = MdkbFindResult {
            symbols: vec![],
            total: None,
        };
        assert_eq!(silent.capped(), None);
    }

    #[tokio::test]
    async fn code_graph_reads_symbols_never_the_prose() {
        // `text` is prose for agents and has never been JSON. The symbols must
        // come from the `symbols` field of the envelope.
        let (path, _server) = spawn_mock_server().await;
        let mut client = connect_to_mock(&path).await;
        let symbols = client.code_graph("/repo", "foo", "callers").await.unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "caller");
        assert_eq!(symbols[0].file_path, "src/lib.rs");
        assert_eq!(symbols[0].line_start, 6);
    }

    #[test]
    fn code_graph_errors_when_the_daemon_omits_symbols() {
        // An mdkb older than the one that added `symbols` answers with `text`
        // alone. That must surface as an error the command layer logs, not as a
        // silent empty list that reads like "this symbol has no callers".
        let err = platform::parse_code_graph_symbols(&json!({
            "text": "foo (Function) has no indexed callers.", "tokens": 0
        }))
        .unwrap_err();
        assert!(err.to_string().contains("no 'symbols'"), "err: {err}");
    }

    #[test]
    fn code_graph_accepts_an_empty_symbol_list() {
        // "No callers" is an empty array, not an absent field — the two must
        // stay distinguishable, or the version guard above is useless.
        let symbols = platform::parse_code_graph_symbols(&json!({
            "text": "foo (Function) has no indexed callers.", "tokens": 0, "symbols": []
        }))
        .unwrap();
        assert!(symbols.is_empty());
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

    /// A daemon that accepts the connection and then goes silent. This is the
    /// shape that used to hang `call` for ever.
    pub(crate) async fn spawn_silent_server() -> (PathBuf, tokio::task::JoinHandle<()>) {
        let dir = crate::test_support::short_socket_tempdir();
        let sock_path = dir.path().join("silent.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();
        let path = sock_path.clone();

        let handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
            // Never reached: the task owns the accepted connection and the
            // temp dir so both outlive every client the test builds.
            drop((stream, dir));
        });

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        (path, handle)
    }

    #[tokio::test]
    async fn call_times_out_when_the_daemon_never_answers() {
        let (path, _server) = spawn_silent_server().await;
        let mut client = connect_to_mock(&path).await;

        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            client.call("ping", json!({})),
        )
        .await;

        let err = outcome
            .expect("call must return on its own deadline, never hang")
            .unwrap_err();
        assert!(err.to_string().contains("timed out"), "err: {err}");
    }

    #[tokio::test]
    async fn a_timed_out_client_refuses_every_later_call() {
        // The abandoned reply may still arrive. Reusing the socket would read
        // it as the length header of the next request and answer the wrong
        // question with a straight face, so the client must stay shut.
        let (path, _server) = spawn_silent_server().await;
        let mut client = connect_to_mock(&path).await;

        client.call("ping", json!({})).await.unwrap_err();

        let err = client.call("ping", json!({})).await.unwrap_err();
        assert!(err.to_string().contains("abandoned"), "err: {err}");
    }

    #[tokio::test]
    async fn test_connection_refused() {
        let result = MdkbClient::connect().await;
        // Will fail unless mdkb daemon is actually running — that's expected in test env
        // The important thing is it doesn't panic
        if let Err(err) = result {
            // With no home directory there is no socket path to dial at all.
            let want = if MdkbClient::socket_path().is_some() {
                "mdkb: connect"
            } else {
                "no home directory"
            };
            assert!(err.to_string().contains(want), "{err}");
        }
    }

    /// The mdkb binary this machine would actually talk to, or `None`.
    ///
    /// Same order the app uses: whatever is on `PATH` first, then the usual
    /// install location.
    fn installed_mdkb() -> Option<PathBuf> {
        if let Ok(out) = std::process::Command::new("sh")
            .args(["-c", "command -v mdkb"])
            .output()
            && out.status.success()
        {
            let path = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
            if path.is_file() {
                return Some(path);
            }
        }
        let fallback = dirs::home_dir()?.join(".local/bin/mdkb");
        fallback.is_file().then_some(fallback)
    }

    /// mdkb reports symbol lines **0-based**, and `editor_line` adds one.
    ///
    /// Every other test around that conversion asserts the SHIFT — that TUIC
    /// adds one — which stays true whatever mdkb does. If mdkb ever normalised
    /// `symbol_to_json`'s `line_start` to 1-based, every outline click and
    /// go-to-definition would land one line late and this repo's suite would
    /// stay green. This test asserts the SOURCE instead: it runs the installed
    /// mdkb over a fixture whose symbol sits on a known line and reads back
    /// what mdkb calls that line.
    ///
    /// The daemon it spawns is fully isolated — its own `HOME`, so its own
    /// `~/.mdkb`, pid file and sockets. It cannot see or disturb the daemon the
    /// developer is using.
    ///
    /// When mdkb is not installed the contract cannot be exercised and the test
    /// says so on stderr rather than asserting anything. That is a real gap: CI
    /// runners have no mdkb, so upstream breakage is caught on a developer
    /// machine, not in CI. Wiring mdkb into CI would close it.
    #[tokio::test]
    async fn mdkb_reports_symbol_lines_zero_based() {
        let Some(mdkb) = installed_mdkb() else {
            eprintln!(
                "mdkb_reports_symbol_lines_zero_based: mdkb is not installed, \
                 so the 0-based line contract went UNCHECKED on this machine"
            );
            return;
        };

        // mdkb binds daemon-hook.sock.<pid>.tmp before renaming it. The normal
        // repository test root can exceed macOS SUN_LEN before that bind.
        let tmp = tempfile::Builder::new()
            .prefix("s")
            .tempdir_in(crate::test_support::short_socket_test_temp_root())
            .expect("short socket test dir");
        let home = tmp.path().to_path_buf();
        let repo = tmp.path().join("repo");
        assert!(
            home.join(".mdkb/daemon-hook.sock.4294967295.tmp")
                .as_os_str()
                .len()
                < 104,
            "mdkb daemon socket path exceeds macOS SUN_LEN: {}",
            home.display()
        );
        std::fs::create_dir_all(home.join(".mdkb")).expect("home");
        std::fs::create_dir_all(repo.join("src")).expect("repo");
        // Symbols on human lines 1, 5, and 9 expose the base and any drift.
        std::fs::write(
            repo.join("src/lib.rs"),
            "pub fn first_harvest() {}\n// 2\n// 3\n// 4\npub fn zonk_harvest() {}\n// 6\n// 7\n// 8\npub fn last_harvest() {}\n",
        )
        .expect("fixture");
        std::fs::write(
            home.join(".mdkb/daemon.toml"),
            format!("whitelist_dirs = [\"{}\"]\n", tmp.path().display()),
        )
        .expect("daemon config");

        let run = |args: &[&str]| {
            std::process::Command::new(&mdkb)
                .args(args)
                .current_dir(&repo)
                .env("HOME", &home)
                .output()
                .expect("run mdkb")
        };
        run(&["init"]);
        run(&["code", "index"]);
        run(&["serve", "--daemon", "--global", "--detach"]);

        let sock = home.join(".mdkb/daemon-hook.sock");
        let mut stream = None;
        for _ in 0..50 {
            if let Ok(s) = UnixStream::connect(&sock).await {
                stream = Some(s);
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let symbols = match stream {
            Some(stream) => {
                let mut client = MdkbClient::from_stream(stream, Duration::from_secs(30));
                client
                    .symbols_in_file(&repo.to_string_lossy(), "src/lib.rs")
                    .await
            }
            None => Err(anyhow::anyhow!("the isolated daemon never opened {sock:?}")),
        };
        // Stop the daemon before asserting, so a failure does not leak it.
        std::process::Command::new(&mdkb)
            .args(["daemon", "stop"])
            .env("HOME", &home)
            .output()
            .ok();

        let symbols = symbols.expect("symbols_in_file");
        for (name, expected_line) in [
            ("first_harvest", 0),
            ("zonk_harvest", 4),
            ("last_harvest", 8),
        ] {
            let found = symbols
                .iter()
                .find(|s| s.name == name)
                .unwrap_or_else(|| panic!("mdkb indexed no {name}: {symbols:?}"));
            assert_eq!(
                found.line_start, expected_line,
                "mdkb must report {name} as a 0-based line; \
                 `mdkb_commands::editor_line` adds one for editor jumps"
            );
        }
    }
}
