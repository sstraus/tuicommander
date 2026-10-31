//! A deterministic ACP agent for TUICommander's client tests.
//!
//! It is a state machine over a JSONL scenario, not a transcript with sleeps:
//! every step either *waits for* a frame the client must send or *writes* one,
//! so a test that depends on ordering — a cancel racing a response, a late
//! update after settlement, a reverse request answered by a human — advances on
//! the frame itself and has no timing window to lose.
//!
//! It is launched through the exact production path, `ego acp -C <root>`, and
//! asserts that argv. That is why the scenario arrives as `scenario.jsonl`
//! inside the root rather than as `--scenario <path>`: a fixture reached by a
//! second, easier argv would stop proving the launch path the product uses.
//!
//! ## Scenario vocabulary
//!
//! One JSON object per line, dispatched on `step`. Blank lines and lines
//! starting with `//` are ignored so a scenario can explain itself.
//!
//! | Step | Meaning |
//! | --- | --- |
//! | `expect` | Read the next client frame. Assert `method`; subset-match `params`, reject `forbid_params` keys when given; remember its `id` under `capture`. |
//! | `expect_env` | Assert a variable in the launched ACP agent's environment. |
//! | `expect_response` | Read the next client frame. Assert it answers `id`; subset-match `result`, or assert `errorCode`. |
//! | `respond` | Write a result for a captured request id. `result_file` reads the result from a file in the root instead, so one recording can serve every scenario that needs it. |
//! | `error` | Write a JSON-RPC error for a captured request id. |
//! | `notify` | Write an agent notification. |
//! | `request` | Write an agent-to-client request under a scenario-chosen `id`. |
//! | `raw` | Write bytes to stdout verbatim, including ones that are not JSON. |
//! | `stderr` | Write diagnostic bytes to stderr, which is never protocol. |
//! | `close_stdout` | Close stdout while staying alive, so the client sees EOF from a live process. |
//! | `await_stdin_eof` | Block until the client closes stdin. |
//! | `exit` | Flush and exit with `code`. |
//!
//! `respond`, `error`, `notify` and `request` accept `fragments`: a list of
//! byte counts to write and flush separately, so a client that assumes one
//! frame arrives in one read is caught here rather than in production.
//!
//! A step that cannot be satisfied panics. The panic message reaches the test
//! through the SDK's bounded stderr tail, which is the only channel a failing
//! child has.

use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;

use serde_json::{Value, json};

fn main() {
    let mut args = std::env::args();
    let _executable = args.next().expect("argv[0]");
    assert_eq!(args.next().as_deref(), Some("acp"), "production argv");
    assert_eq!(args.next().as_deref(), Some("-C"), "production argv");
    let root = PathBuf::from(args.next().expect("production argv carries a root"));
    let expected_profile = root.join("expected-profile.txt");
    if expected_profile.exists() {
        let expected = fs::read_to_string(expected_profile).expect("expected profile name");
        assert_eq!(args.next().as_deref(), Some("--profile"), "production argv");
        assert_eq!(
            args.next().as_deref(),
            Some(expected.as_str()),
            "production argv"
        );
    }
    let expected_prompt = root.join("expected-master-prompt.txt");
    if expected_prompt.exists() {
        let expected = fs::read_to_string(expected_prompt).expect("expected master prompt");
        assert_eq!(
            args.next().as_deref(),
            Some("--append-system"),
            "production argv"
        );
        assert_eq!(
            args.next().as_deref(),
            Some(expected.as_str()),
            "production prompt argv"
        );
    }
    assert!(
        args.next().is_none(),
        "production argv carries nothing else"
    );

    let path = root.join("scenario.jsonl");
    let scenario = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("scenario {}: {error}", path.display()));

    let mut agent = Agent::new(root);
    for (line, text) in scenario.lines().enumerate() {
        let text = text.trim();
        if text.is_empty() || text.starts_with("//") {
            continue;
        }
        let step: Value = serde_json::from_str(text)
            .unwrap_or_else(|error| panic!("scenario line {}: {error}", line + 1));
        agent.run(&step, line + 1);
    }
    io::stdout().flush().expect("final flush");
}

struct Agent {
    /// Where `result_file` resolves against — the same root the scenario came
    /// from, so a scenario names a sibling file and nothing else.
    root: PathBuf,
    stdin: BufReader<io::Stdin>,
    /// Request ids the client generated, by the name the scenario gave them.
    ///
    /// The client owns its own id namespace and may spell an id as a number or
    /// a string; the scenario names the request instead of guessing the id, and
    /// the captured value is echoed back verbatim.
    captured: HashMap<String, Value>,
}

impl Agent {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            stdin: BufReader::new(io::stdin()),
            captured: HashMap::new(),
        }
    }

    /// The result a `respond` step should write.
    ///
    /// `result_file` exists so a long recording — ego's advertised
    /// capabilities, above all — lives in one file that every scenario needing
    /// it points at, instead of being pasted into each and drifting apart one
    /// scenario at a time.
    fn result(&self, step: &Value, line: usize) -> Value {
        let Some(name) = step.get("result_file").and_then(Value::as_str) else {
            return field(step, "result");
        };
        let path = self.root.join(name);
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("scenario line {line}: {}: {error}", path.display()));
        serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("scenario line {line}: {}: {error}", path.display()))
    }

    fn run(&mut self, step: &Value, line: usize) {
        match step.get("step").and_then(Value::as_str) {
            Some("expect_env") => {
                let name = required_str(step, "name", line);
                let expected = step.get("value").and_then(Value::as_str);
                assert_eq!(std::env::var(name).ok().as_deref(), expected, "scenario line {line}: {name}");
            }
            Some("expect") => self.expect(step, line),
            Some("expect_response") => self.expect_response(step, line),
            Some("respond") => {
                let id = self.captured_id(step, line);
                let result = self.result(step, line);
                self.write(&json!({"jsonrpc": "2.0", "id": id, "result": result}), step);
            }
            Some("error") => {
                let id = self.captured_id(step, line);
                let mut error = json!({
                    "code": step.get("code").and_then(Value::as_i64).unwrap_or(-32603),
                    "message": step.get("message").and_then(Value::as_str).unwrap_or("fixture error"),
                });
                if let Some(data) = step.get("data") {
                    error["data"] = data.clone();
                }
                self.write(&json!({"jsonrpc": "2.0", "id": id, "error": error}), step);
            }
            Some("notify") => self.write(
                &json!({
                    "jsonrpc": "2.0",
                    "method": required_str(step, "method", line),
                    "params": field(step, "params"),
                }),
                step,
            ),
            Some("request") => self.write(
                &json!({
                    "jsonrpc": "2.0",
                    "id": step.get("id").unwrap_or_else(|| panic!("scenario line {line}: request needs an id")),
                    "method": required_str(step, "method", line),
                    "params": field(step, "params"),
                }),
                step,
            ),
            // Verbatim, because the point of this step is the bytes a
            // well-formed writer would never produce: a truncated frame, a
            // second frame glued to the first, a line that is not JSON at all.
            Some("raw") => {
                let bytes = required_str(step, "bytes", line);
                let mut stdout = io::stdout();
                stdout.write_all(bytes.as_bytes()).expect("raw bytes");
                stdout.flush().expect("raw flush");
            }
            Some("stderr") => {
                let bytes = required_str(step, "bytes", line);
                io::stderr().write_all(bytes.as_bytes()).expect("stderr");
                io::stderr().flush().expect("stderr flush");
            }
            Some("close_stdout") => {
                io::stdout().flush().expect("flush before closing stdout");
                close_descriptor(1);
            }
            Some("await_stdin_eof") => {
                let mut drained = Vec::new();
                let _ = self.stdin.read_to_end(&mut drained);
            }
            // The assertion a scenario cannot otherwise make: that nothing was
            // sent. Waiting for EOF alone would let a wrongly-sent request sit
            // in the buffer while the client waits for an answer that is never
            // coming, and the test would hang instead of naming the frame.
            Some("expect_no_frame") => {
                let mut text = String::new();
                let read = self.stdin.read_line(&mut text).expect("read client frame");
                assert_eq!(
                    read, 0,
                    "scenario line {line}: nothing should have been sent, and the client sent {text:?}"
                );
            }
            Some("exit") => {
                io::stdout().flush().expect("flush before exit");
                let code = step.get("code").and_then(Value::as_i64).unwrap_or(0);
                std::process::exit(i32::try_from(code).expect("exit code"));
            }
            other => panic!("scenario line {line}: unknown step {other:?}"),
        }
    }

    fn expect(&mut self, step: &Value, line: usize) {
        let frame = self.read_frame(line);
        let method = required_str(step, "method", line);
        assert_eq!(
            frame.get("method").and_then(Value::as_str),
            Some(method),
            "scenario line {line}: expected {method}, got {frame}"
        );
        if let Some(params) = step.get("params") {
            assert!(
                contains(frame.get("params").unwrap_or(&Value::Null), params),
                "scenario line {line}: params mismatch\nwanted {params}\ngot    {frame}"
            );
        }
        if let Some(forbidden) = step.get("forbid_params").and_then(Value::as_array) {
            let params = frame.get("params").and_then(Value::as_object);
            for key in forbidden {
                let key = key.as_str().expect("forbid_params entries are strings");
                assert!(
                    params.is_none_or(|params| !params.contains_key(key)),
                    "scenario line {line}: forbidden session policy key {key} in {frame}"
                );
            }
        }
        if let Some(name) = step.get("capture").and_then(Value::as_str) {
            let id = frame
                .get("id")
                .unwrap_or_else(|| {
                    panic!("scenario line {line}: {method} carried no id to capture")
                })
                .clone();
            self.captured.insert(name.to_owned(), id);
        }
    }

    fn expect_response(&mut self, step: &Value, line: usize) {
        let frame = self.read_frame(line);
        let wanted = step
            .get("id")
            .unwrap_or_else(|| panic!("scenario line {line}: expect_response needs an id"));
        assert_eq!(
            frame.get("id"),
            Some(wanted),
            "scenario line {line}: expected a response to {wanted}, got {frame}"
        );
        if let Some(result) = step.get("result") {
            assert!(
                contains(frame.get("result").unwrap_or(&Value::Null), result),
                "scenario line {line}: result mismatch\nwanted {result}\ngot    {frame}"
            );
        }
        if let Some(code) = step.get("errorCode").and_then(Value::as_i64) {
            assert_eq!(
                frame.pointer("/error/code").and_then(Value::as_i64),
                Some(code),
                "scenario line {line}: expected error {code}, got {frame}"
            );
        }
    }

    fn read_frame(&mut self, line: usize) -> Value {
        let mut text = String::new();
        let read = self.stdin.read_line(&mut text).expect("read client frame");
        assert!(
            read > 0,
            "scenario line {line}: the client closed stdin with steps left"
        );
        serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("scenario line {line}: client sent {text:?}: {error}"))
    }

    fn captured_id(&self, step: &Value, line: usize) -> Value {
        let name = required_str(step, "to", line);
        self.captured
            .get(name)
            .unwrap_or_else(|| panic!("scenario line {line}: nothing captured as {name:?}"))
            .clone()
    }

    /// Write one frame, honouring a `fragments` split.
    ///
    /// The counts are byte counts into the serialized frame; whatever they do
    /// not cover is written last, and the newline always goes with it, so a
    /// scenario cannot accidentally leave a frame unterminated.
    fn write(&self, frame: &Value, step: &Value) {
        let mut line = serde_json::to_string(frame).expect("serialize frame");
        line.push('\n');
        let bytes = line.as_bytes();
        let mut stdout = io::stdout();
        let mut written = 0;
        if let Some(fragments) = step.get("fragments").and_then(Value::as_array) {
            for fragment in fragments {
                let take = usize::try_from(fragment.as_u64().expect("fragment size"))
                    .expect("fragment size")
                    .min(bytes.len() - written);
                stdout
                    .write_all(&bytes[written..written + take])
                    .expect("write fragment");
                stdout.flush().expect("flush fragment");
                written += take;
            }
        }
        stdout.write_all(&bytes[written..]).expect("write frame");
        stdout.flush().expect("flush frame");
    }
}

fn field(step: &Value, name: &str) -> Value {
    step.get(name).cloned().unwrap_or(Value::Null)
}

fn required_str<'a>(step: &'a Value, name: &str, line: usize) -> &'a str {
    step.get(name)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("scenario line {line}: missing {name:?}"))
}

/// Whether `actual` contains everything `expected` names.
///
/// Objects match on the keys the expectation lists and ignore the rest, so a
/// scenario pins the fields its test is about and does not break when an
/// unrelated optional field is added. Arrays match element for element,
/// because a partial list is almost never what a test means.
fn contains(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => expected.iter().all(|(key, value)| {
            actual
                .get(key)
                .is_some_and(|actual| contains(actual, value))
        }),
        (Value::Array(actual), Value::Array(expected)) => {
            actual.len() == expected.len()
                && actual
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| contains(actual, expected))
        }
        _ => actual == expected,
    }
}

/// Close one standard descriptor while the process stays alive.
///
/// Taking ownership of the raw descriptor and dropping it is the whole
/// operation; `io::Stdout` cannot be closed because the process keeps a handle
/// to it. This is how a scenario produces EOF on stdout from a running agent,
/// or a broken pipe on the client's next write.
#[cfg(unix)]
fn close_descriptor(descriptor: std::os::fd::RawFd) {
    use std::os::fd::FromRawFd;

    // SAFETY: the descriptor is one of the two the process was started with and
    // the scenario step exists to close exactly it. Nothing else in this
    // fixture holds an owned handle to it, and every later step that would use
    // it is a scenario authoring error the panic below already reports.
    drop(unsafe { std::os::fd::OwnedFd::from_raw_fd(descriptor) });
}

/// Windows has no descriptor table to close by number: the pipe end the parent
/// waits on lives behind a `HANDLE`, which is what the standard streams wrap,
/// so the number selects the stream and the handle is what gets dropped.
///
/// Until this existed the `not(unix)` arm panicked, and a panic is not EOF: the
/// process died, and `stdout-closed-alive` — the one scenario whose whole point
/// is a live agent behind a closed pipe — settled as a transport failure with
/// "ACP process ended before initialization completed". It passed on GitHub's
/// runner for the same reason every other Windows gap did: nothing there runs
/// this scenario differently, it simply never ran green and was never read.
#[cfg(windows)]
fn close_descriptor(descriptor: i32) {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

    let handle = match descriptor {
        0 => io::stdin().as_raw_handle(),
        1 => io::stdout().as_raw_handle(),
        2 => io::stderr().as_raw_handle(),
        other => panic!("{other} is not a standard descriptor"),
    };
    // SAFETY: as on unix, the handle is one of the three the process was
    // started with and the scenario step exists to close exactly it. Nothing
    // else in this fixture holds an owned handle to it.
    drop(unsafe { OwnedHandle::from_raw_handle(handle) });
}

#[cfg(not(any(unix, windows)))]
fn close_descriptor(_descriptor: i32) {
    panic!("closing a standard descriptor is only implemented for unix and windows");
}
