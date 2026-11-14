//! OpenCode adapter: a native v2 server plugin rather than a settings
//! hook. TUIC owns the whole plugin file `~/.config/opencode/plugin/tuic.ts`,
//! identified by a `/* tuic-managed */` marker on the first line.
//!
//! Install writes the file only if it is absent or already ours (never clobbers a
//! user plugin sharing that path); uninstall deletes it only if marked. The
//! generated plugin emits OSC 7770 to the controlling tty on lifecycle events,
//! inert outside a TUIC session.

use crate::agent_hook_installer::InstallState;
use std::path::Path;

/// First-line ownership marker — both the install guard and the uninstall gate.
pub(crate) const OPENCODE_MARKER: &str = "/* tuic-managed */";

/// The generated native v2 plugin. Request ownership comes from the native
/// event envelope, not tool completion or terminal text.
pub(crate) fn opencode_plugin_source() -> String {
    format!(
        r#"{OPENCODE_MARKER}
// TUICommander native status signals. Do not edit this managed file.
// Requires the native v2 default plugin interface and a TUIC controlling tty.
import {{ openSync, writeSync, closeSync }} from "node:fs";

function write(frame) {{
  try {{
    const fd = openSync("/dev/tty", "w");
    try {{
      writeSync(fd, `\x1b]7770;${{frame}}\x1b\\`);
    }} finally {{
      closeSync(fd);
    }}
  }} catch {{
    /* no controlling tty — no-op */
  }}
}}

const emit = state => write(`state=${{state}}`);

// Tool readout metadata: the call id and name only, never the input.
function tool(event, id, name) {{
  const frame = Buffer.from(JSON.stringify({{ event, id, name }})).toString("base64");
  write(`agent-tool=${{process.env.TUIC_SESSION}}:${{frame}}`);
}}

export default {{
  id: "tuic.state",
  setup(context) {{
    // --standalone starts a private --stdio server with this controlling tty.
    // A shared background service has no per-terminal event/tty ownership.
    if (!process.env.TUIC_SESSION || !process.argv.includes("--stdio")) return;
    const controller = new AbortController();
    const sessions = new Map();
    let last;
    const endTools = session => {{
      for (const id of session.tools) tool("end", id);
      session.tools.clear();
    }};
    const publish = () => {{
      let next = "idle";
      for (const session of sessions.values()) {{
        if (session.pending.size) {{ next = "awaiting"; break; }}
        if (session.busy) next = "busy";
      }}
      if (next !== last) {{ last = next; emit(next); }}
    }};
    const receive = event => {{
      if (event.type === "location.shutdown") {{
        for (const session of sessions.values()) endTools(session);
        sessions.clear(); publish(); return;
      }}
      const data = event.data;
      const form = event.type === "form.created" ? data?.form : undefined;
      const owner = form?.sessionID ?? data?.sessionID;
      // Global forms have no terminal session owner.
      if (typeof owner !== "string" || !owner.startsWith("ses_")) return;
      let session = sessions.get(owner);
      switch (event.type) {{
        case "form.created":
        case "permission.asked": {{
          const id = form?.id ?? data?.id;
          if (typeof id !== "string") return;
          if (!session) session = {{ busy: false, pending: new Set(), tools: new Set() }};
          const kind = form ? "form" : "permission";
          session.pending.add(`${{kind}}:${{id}}`);
          sessions.set(owner, session);
          break;
        }}
        case "form.replied":
        case "form.cancelled":
          if (!session?.pending.delete(`form:${{data.id}}`)) return;
          break;
        case "permission.replied":
          if (!session?.pending.delete(`permission:${{data.requestID}}`)) return;
          break;
        case "session.execution.started":
          if (!session) session = {{ busy: false, pending: new Set(), tools: new Set() }};
          session.busy = true;
          sessions.set(owner, session);
          break;
        case "session.tool.input.started":
          if (!session?.busy || typeof data.id !== "string" || typeof data.name !== "string") return;
          session.tools.add(data.id);
          tool("start", data.id, data.name);
          return;
        case "session.tool.success":
        case "session.tool.failed":
          if (!session?.tools.delete(data.id)) return;
          tool("end", data.id);
          return;
        case "session.execution.succeeded":
        case "session.execution.failed":
        case "session.execution.interrupted":
        case "session.deleted":
          if (!session) return;
          endTools(session);
          sessions.delete(owner);
          publish(); return;
        default:
          return;
      }}
      if (!session.busy && !session.pending.size) sessions.delete(owner);
      publish();
    }};
    const run = (async () => {{
      try {{
        for await (const event of context.event.subscribe({{ signal: controller.signal }})) {{
          receive(event);
        }}
      }} catch {{
        /* Instrumentation must not fail the native session. */
      }} finally {{
        for (const session of sessions.values()) endTools(session);
        sessions.clear(); publish();
      }}
    }})();
    return async () => {{ controller.abort(); await run; }};
  }},
}};
"#
    )
}

/// Install: write the plugin, but refuse to overwrite a user plugin that doesn't
/// carry our marker.
pub(crate) fn install_opencode_plugin(path: &Path) -> Result<(), String> {
    if path.exists() {
        let existing =
            std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        if !existing.contains(OPENCODE_MARKER) {
            return Err(format!(
                "{} exists and is not TUIC-managed — refusing to overwrite",
                path.display()
            ));
        }
    }
    crate::config::persist_atomic(path, opencode_plugin_source().as_bytes())
}

/// Uninstall: delete the plugin only if it carries our marker. Missing/unmarked
/// file is a no-op (we never delete a user plugin).
pub(crate) fn uninstall_opencode_plugin(path: &Path) -> Result<(), String> {
    if path.exists() {
        let existing =
            std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        if existing.contains(OPENCODE_MARKER) {
            std::fs::remove_file(path).map_err(|e| format!("remove {}: {e}", path.display()))?;
        }
    }
    Ok(())
}

/// State: present + marked + current source → Installed; present + marked + stale
/// → Outdated; absent or present-but-unmarked → NotInstalled.
pub(crate) fn opencode_plugin_state(path: &Path) -> InstallState {
    let Ok(content) = std::fs::read_to_string(path) else {
        return InstallState::NotInstalled;
    };
    if !content.contains(OPENCODE_MARKER) {
        return InstallState::NotInstalled;
    }
    if content == opencode_plugin_source() {
        InstallState::Installed
    } else {
        InstallState::Outdated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn agent_hook_opencode_install_then_state_installed() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("plugin/tuic.ts");
        assert_eq!(opencode_plugin_state(&path), InstallState::NotInstalled);
        install_opencode_plugin(&path).unwrap();
        assert!(path.exists());
        assert_eq!(opencode_plugin_state(&path), InstallState::Installed);
    }

    #[test]
    fn agent_hook_opencode_never_overwrites_unmarked_user_plugin() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("tuic.ts");
        std::fs::write(&path, b"// my own plugin\nexport const mine = 1;\n").unwrap();
        let err = install_opencode_plugin(&path);
        assert!(err.is_err(), "must refuse to overwrite an unmarked file");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "// my own plugin\nexport const mine = 1;\n",
            "user plugin left untouched"
        );
        // And state never claims a user plugin as ours.
        assert_eq!(opencode_plugin_state(&path), InstallState::NotInstalled);
    }

    #[test]
    fn agent_hook_opencode_uninstall_deletes_only_marked_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("tuic.ts");
        // Unmarked user file: uninstall must NOT delete it.
        std::fs::write(&path, b"// user plugin\n").unwrap();
        uninstall_opencode_plugin(&path).unwrap();
        assert!(path.exists(), "unmarked file preserved");
        // Our file: uninstall deletes it.
        install_opencode_plugin(&dir.path().join("ours.ts")).unwrap();
        let ours = dir.path().join("ours.ts");
        uninstall_opencode_plugin(&ours).unwrap();
        assert!(!ours.exists(), "marked file deleted");
    }

    #[test]
    fn agent_hook_opencode_reinstall_idempotent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("tuic.ts");
        install_opencode_plugin(&path).unwrap();
        install_opencode_plugin(&path).unwrap(); // ours → allowed to rewrite
        assert_eq!(opencode_plugin_state(&path), InstallState::Installed);
    }

    fn recorded_events() -> Vec<serde_json::Value> {
        let capture = crate::pty_capture::decode_capture(include_bytes!(
            "fixtures/agent_prompts/opencode-native-plugin-v2-events-2628.tcap"
        ))
        .unwrap();
        assert_eq!(capture.geometry, Some((41, 128)));
        let bytes: Vec<u8> = capture
            .records
            .into_iter()
            .filter(|record| record.direction == crate::pty_capture::CaptureDirection::Output)
            .flat_map(|record| record.data)
            .collect();
        String::from_utf8(bytes)
            .unwrap()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|event| event["type"].is_string())
            .collect()
    }

    fn event(events: &[serde_json::Value], kind: &str) -> serde_json::Value {
        events
            .iter()
            .find(|event| event["type"] == kind)
            .unwrap()
            .clone()
    }

    // Execute the actual generated module. Only the controlling-tty I/O is
    // replaced: the oracle is literal OSC bytes and recorded native events.
    fn run_plugin(events: &[serde_json::Value], in_tuic: bool) -> serde_json::Value {
        run_plugin_in_server(events, in_tuic, "--stdio")
    }

    fn run_plugin_in_server(
        events: &[serde_json::Value],
        in_tuic: bool,
        mode: &str,
    ) -> serde_json::Value {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let source = serde_json::to_string(&opencode_plugin_source()).unwrap();
        let events = serde_json::to_string(events).unwrap();
        let script = format!(
            r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {{ syncBuiltinESMExports }} from 'node:module';
const guard = setTimeout(() => {{
  process.stderr.write('generated plugin failed to finish replay or cleanup\n');
  process.exit(1);
}}, 10000);
const writes = [];
let opened = 0, closed = 0, subscribed = 0;
fs.openSync = (path, flags) => {{
  assert.equal(path, '/dev/tty'); assert.equal(flags, 'w');
  opened++; return 7;
}};
fs.writeSync = (fd, bytes) => {{ assert.equal(fd, 7); writes.push(bytes); }};
fs.closeSync = fd => {{ assert.equal(fd, 7); closed++; }};
syncBuiltinESMExports();
const module = await import('data:text/javascript;base64,' + Buffer.from({source}).toString('base64'));
assert.equal(typeof module.default?.id, 'string', 'native v2 loader requires a default definition with id');
assert.equal(typeof module.default?.setup, 'function', 'native v2 loader requires setup');
const trace = [];
let consumed;
const allConsumed = new Promise(resolve => {{ consumed = resolve; }});
const context = {{ event: {{ subscribe(options) {{
  subscribed++;
  return (async function* () {{
    for (const event of {events}) {{
      yield event;
      trace.push(writes.splice(0));
    }}
    consumed();
    if (!options.signal.aborted) await new Promise(resolve => options.signal.addEventListener('abort', resolve, {{ once: true }}));
  }})();
}} }} }};
const cleanup = await module.default.setup(context);
if (subscribed) await allConsumed;
await cleanup?.();
assert.equal(opened, closed, 'every controlling tty descriptor must close');
clearTimeout(guard);
process.stdout.write(JSON.stringify({{trace, cleanup: writes, subscribed}}));
"#
        );
        let mut command = Command::new("node");
        command.arg("--input-type=module").arg("-").arg(mode);
        if in_tuic {
            command.env("TUIC_SESSION", "fixture-owner");
        } else {
            command.env_remove("TUIC_SESSION");
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Node is required to execute the generated native plugin");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn frames(states: &[&str]) -> serde_json::Value {
        serde_json::Value::Array(
            states
                .iter()
                .map(|state| serde_json::Value::String(format!("\x1b]7770;state={state}\x1b\\")))
                .collect(),
        )
    }

    // Catches: a tool frame carries the tool input, an end of another session
    // or call clears a live tool, or an execution end leaves a tool behind.
    #[test]
    fn opencode_tool_frames_name_only_their_own_call() {
        let events = recorded_events();
        let started = event(&events, "session.execution.started");
        let ended = event(&events, "session.execution.succeeded");
        let owner = started["data"]["sessionID"].clone();
        assert_eq!(ended["data"]["sessionID"], owner, "one recorded session");
        // No tool event was recorded: these follow the retained 2.2.3 schema
        // (`session.tool.input.started` / `.success` / `.failed`).
        let tool = |kind: &str, session: &serde_json::Value, id: &str| {
            serde_json::json!({"type": kind, "data": {"sessionID": session,
                "assistantMessageID": "msg_fixture", "id": id, "name": "bash",
                "content": [{"type": "text", "text": "secret output"}]}})
        };
        let other = serde_json::json!("ses_other");
        let output = run_plugin(
            &[
                tool("session.tool.input.started", &owner, "call_early"),
                started,
                tool("session.tool.input.started", &owner, "call_a"),
                tool("session.tool.input.started", &owner, "call_b"),
                tool("session.tool.success", &other, "call_a"),
                tool("session.tool.failed", &owner, "call_a"),
                ended,
            ],
            true,
        );
        let decode = |frame: &serde_json::Value| -> serde_json::Value {
            let frame = frame.as_str().unwrap();
            let Some(body) = frame.strip_prefix("\x1b]7770;agent-tool=fixture-owner:") else {
                return serde_json::json!(frame);
            };
            use base64::Engine;
            let json = base64::engine::general_purpose::STANDARD
                .decode(body.strip_suffix("\x1b\\").unwrap())
                .unwrap();
            serde_json::from_slice(&json).unwrap()
        };
        let trace: Vec<Vec<serde_json::Value>> = output["trace"]
            .as_array()
            .unwrap()
            .iter()
            .map(|step| step.as_array().unwrap().iter().map(decode).collect())
            .collect();
        let start = |id: &str| serde_json::json!({"event": "start", "id": id, "name": "bash"});
        let end = |id: &str| serde_json::json!({"event": "end", "id": id});
        assert_eq!(
            trace,
            vec![
                vec![],
                vec![serde_json::json!("\x1b]7770;state=busy\x1b\\")],
                vec![start("call_a")],
                vec![start("call_b")],
                vec![],
                vec![end("call_a")],
                vec![
                    end("call_b"),
                    serde_json::json!("\x1b]7770;state=idle\x1b\\")
                ],
            ]
        );
    }

    #[test]
    fn opencode_v2_plugin_loads_and_reports_native_state() {
        // Catches: the legacy named-function export never loads in native v2.
        let failure = crate::pty_capture::decode(include_bytes!(
            "fixtures/agent_prompts/opencode-native-plugin-v2-load-failure-2628.tcap"
        ))
        .unwrap();
        let bytes: Vec<u8> = failure.into_iter().flat_map(|record| record.data).collect();
        assert!(String::from_utf8(bytes).unwrap().contains(
            "Plugin must export a default definition with an id and an effect or setup function"
        ));
        let events = recorded_events();
        let output = run_plugin(&[event(&events, "session.execution.started")], true);
        assert_eq!(output["trace"], serde_json::json!([frames(&["busy"])]));
    }

    #[test]
    fn opencode_answer_retracts_wait_without_next_tool() {
        // Catches: answer, rejection or cancellation needs another tool to clear.
        let events = recorded_events();
        let relevant: Vec<_> = events
            .into_iter()
            .filter(|event| {
                matches!(
                    event["type"].as_str(),
                    Some(
                        "form.created"
                            | "form.replied"
                            | "form.cancelled"
                            | "permission.asked"
                            | "permission.replied"
                    )
                )
            })
            .take(8)
            .collect();
        assert_eq!(
            relevant.len(),
            8,
            "two forms and two permission decisions recorded"
        );
        let output = run_plugin(&relevant, true);
        assert_eq!(
            output["trace"],
            serde_json::json!([
                frames(&["awaiting"]),
                frames(&["idle"]),
                frames(&["awaiting"]),
                frames(&["idle"]),
                frames(&["awaiting"]),
                frames(&["idle"]),
                frames(&["awaiting"]),
                frames(&["idle"])
            ])
        );
    }

    #[test]
    fn opencode_modal_ownership_survives_sibling_and_duplicate_replies() {
        // Catches: a duplicate answer or another session releases an open form.
        let events = recorded_events();
        let forms: Vec<_> = events
            .iter()
            .filter(|event| event["type"] == "form.created")
            .cloned()
            .collect();
        let replies: Vec<_> = events
            .iter()
            .filter(|event| {
                matches!(
                    event["type"].as_str(),
                    Some("form.replied" | "form.cancelled")
                )
            })
            .cloned()
            .collect();
        // Native capture: idle answer, idle cancel, then concurrent A/B/sibling.
        let output = run_plugin(
            &[
                forms[2].clone(),
                forms[3].clone(),
                forms[4].clone(),
                replies[2].clone(),
                replies[2].clone(),
                event(&events, "session.execution.failed"),
                replies[3].clone(),
                replies[4].clone(),
                replies[4].clone(),
            ],
            true,
        );
        assert_eq!(
            output["trace"],
            serde_json::json!([
                frames(&["awaiting"]),
                frames(&[]),
                frames(&[]),
                frames(&[]),
                frames(&[]),
                frames(&[]),
                frames(&[]),
                frames(&["idle"]),
                frames(&[])
            ])
        );
    }

    #[test]
    fn opencode_reply_restores_current_execution_not_open_time() {
        // Catches: busy state saved when a form opens overrides newer activity.
        let events = recorded_events();
        let output = run_plugin(
            &[
                event(&events, "form.created"),
                event(&events, "session.execution.started"),
                event(&events, "form.replied"),
                event(&events, "session.execution.interrupted"),
                event(&events, "form.replied"),
            ],
            true,
        );
        assert_eq!(
            output["trace"],
            serde_json::json!([
                frames(&["awaiting"]),
                frames(&[]),
                frames(&["busy"]),
                frames(&["idle"]),
                frames(&[])
            ])
        );
    }

    #[test]
    fn opencode_permission_decisions_restore_running_execution() {
        // Catches: allow/reject clears busy along with its permission wait.
        let events = recorded_events();
        let decisions: Vec<_> = events
            .iter()
            .filter(|event| {
                matches!(
                    event["type"].as_str(),
                    Some("permission.asked" | "permission.replied")
                )
            })
            .take(4)
            .cloned()
            .collect();
        let mut replay = vec![event(&events, "session.execution.started")];
        replay.extend(decisions);
        let output = run_plugin(&replay, true);
        assert_eq!(
            output["trace"],
            serde_json::json!([
                frames(&["busy"]),
                frames(&["awaiting"]),
                frames(&["busy"]),
                frames(&["awaiting"]),
                frames(&["busy"])
            ])
        );
    }

    #[test]
    fn opencode_wrong_owner_or_unknown_request_cannot_release_wait() {
        // Catches: settlement matches only a session or only a request id.
        let events = recorded_events();
        let asked = event(&events, "form.created");
        let answered = event(&events, "form.replied");
        let sibling = events
            .iter()
            .find(|event| {
                event["type"] == "form.created"
                    && event["data"]["form"]["sessionID"] != asked["data"]["form"]["sessionID"]
            })
            .unwrap();
        // Recombine recorded owner/id values to probe impossible correlations;
        // the fixture itself remains the unmodified native event stream.
        let mut wrong_owner = answered.clone();
        wrong_owner["data"]["sessionID"] = sibling["data"]["form"]["sessionID"].clone();
        let mut unknown_request = answered.clone();
        unknown_request["data"]["id"] = sibling["data"]["form"]["id"].clone();
        let output = run_plugin(&[asked, wrong_owner, unknown_request, answered], true);
        assert_eq!(
            output["trace"],
            serde_json::json!([
                frames(&["awaiting"]),
                frames(&[]),
                frames(&[]),
                frames(&["idle"])
            ])
        );
    }

    #[test]
    fn opencode_execution_end_and_shutdown_clear_owned_waits() {
        // Catches: stop, failure, interrupt, session exit or plugin close latches a wait.
        let events = recorded_events();
        for kind in [
            "session.execution.succeeded",
            "session.execution.failed",
            "session.execution.interrupted",
            "session.deleted",
            "location.shutdown",
        ] {
            let ending = event(&events, kind);
            let owner = ending["data"]["sessionID"].as_str();
            let form = events
                .iter()
                .find(|event| {
                    event["type"] == "form.created"
                        && owner.is_none_or(|owner| event["data"]["form"]["sessionID"] == owner)
                })
                .unwrap()
                .clone();
            let output = run_plugin(&[form.clone(), ending.clone()], true);
            assert_eq!(
                output["trace"],
                serde_json::json!([frames(&["awaiting"]), frames(&["idle"])]),
                "{kind}"
            );
            let output = run_plugin(&[form], true);
            assert_eq!(output["cleanup"], frames(&["idle"]), "cleanup: {kind}");
            if let Some(permission) = events.iter().find(|event| {
                event["type"] == "permission.asked"
                    && owner.is_none_or(|owner| event["data"]["sessionID"] == owner)
            }) {
                let output = run_plugin(&[permission.clone(), event(&events, kind)], true);
                assert_eq!(
                    output["trace"],
                    serde_json::json!([frames(&["awaiting"]), frames(&["idle"])]),
                    "permission: {kind}"
                );
                let output = run_plugin(std::slice::from_ref(permission), true);
                assert_eq!(
                    output["cleanup"],
                    frames(&["idle"]),
                    "permission cleanup: {kind}"
                );
            }
        }
    }

    #[test]
    fn opencode_outside_tuic_does_not_subscribe_or_write() {
        // Catches: globally installed plugin attaches or writes outside TUIC.
        let output = run_plugin(&recorded_events(), false);
        assert_eq!(
            output,
            serde_json::json!({"trace": [], "cleanup": [], "subscribed": 0})
        );
    }

    #[test]
    fn opencode_shared_server_cannot_badge_a_single_terminal() {
        // Catches: a detached shared service publishes other clients' waits.
        let output = run_plugin_in_server(&recorded_events(), true, "--service");
        assert_eq!(
            output,
            serde_json::json!({"trace": [], "cleanup": [], "subscribed": 0})
        );
    }

    #[test]
    fn opencode_legacy_marked_plugin_is_outdated_until_reinstalled() {
        // Catches: a marked but obsolete plugin is reported as operational.
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let path = dir.path().join("tuic.ts");
        std::fs::write(
            &path,
            format!("{OPENCODE_MARKER}\nexport const legacy = 1;\n"),
        )
        .unwrap();
        assert_eq!(opencode_plugin_state(&path), InstallState::Outdated);
        install_opencode_plugin(&path).unwrap();
        assert_eq!(opencode_plugin_state(&path), InstallState::Installed);
    }
}
