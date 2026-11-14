//! Pi extension ownership and native lifecycle instrumentation.
//!
//! API evidence: Pi 0.84.2, recorded in fixtures/agent_prompts/pi-extension-api.jsonl.
//! Pi exposes modal promises, not prompt-start/end events. Wrap the public shared
//! UI context during session_start and retract each owner in finally.

use crate::agent_hook_installer::InstallState;
use std::path::Path;

const MARKER: &str = "/* tuic-managed-pi */";

/// The extension forwards only the quota headers TUIC accepts: one list.
static SOURCE: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    let headers = serde_json::to_string(crate::provider_quota::pi::QUOTA_HEADERS)
        .expect("static header names serialize");
    SOURCE_TEMPLATE
        .replace("__TUIC_QUOTA_HEADERS__", &headers)
        .replace(
            "__TUIC_MAX_HEADER_VALUE__",
            &crate::provider_quota::pi::MAX_HEADER_VALUE.to_string(),
        )
});

pub(crate) fn pi_extension_source() -> &'static str {
    &SOURCE
}

const SOURCE_TEMPLATE: &str = r#"/* tuic-managed-pi */
// TUICommander Pi extension. Managed by Settings; inert outside a TUIC terminal.
import { openSync, writeSync, closeSync } from "node:fs";

export default function (pi) {
  if (!process.env.TUIC_SESSION) return;
  let running = false;
  let epoch = 0;
  let lastState;
  let active = false;
  const waits = new Set();
  const restorers = [];

  function write(frame) {
    let fd;
    try {
      fd = openSync("/dev/tty", "w");
      writeSync(fd, `\x1b]7770;${frame}\x1b\\`);
      return true;
    } catch {
      // No controlling terminal: leave the normal heuristic path available.
      return false;
    } finally {
      if (fd !== undefined) {
        try { closeSync(fd); } catch { /* terminal already closed */ }
      }
    }
  }

  function publish() {
    const state = waits.size ? "awaiting" : running ? "busy" : "idle";
    if (state === lastState) return;
    if (write(`state=${state}`)) lastState = state;
  }

  // Tool readout metadata: the call id and name only, never the arguments.
  function tool(kind, event) {
    if (!active || typeof event?.toolCallId !== "string") return;
    if (kind === "start" && typeof event.toolName !== "string") return;
    const frame = { event: kind, id: event.toolCallId, name: kind === "start" ? event.toolName : undefined };
    write(`agent-tool=${process.env.TUIC_SESSION}:${Buffer.from(JSON.stringify(frame)).toString("base64")}`);
  }

  // Generic typed channel to TUIC: `pi-event=<TUIC_SESSION>:<base64 JSON {kind, ...}>`.
  // New pi data adds a kind; TUIC ignores kinds it does not know.
  function emit(kind, data) {
    write(`pi-event=${process.env.TUIC_SESSION}:${Buffer.from(JSON.stringify({ ...data, kind })).toString("base64")}`);
  }

  // An allow-list, never a deny-list: no auth header can cross the channel.
  const QUOTA_HEADERS = new Set(__TUIC_QUOTA_HEADERS__);
  function quotaHeaders(headers) {
    const allowed = {};
    for (const [name, value] of Object.entries(headers ?? {})) {
      const key = name.toLowerCase();
      if (QUOTA_HEADERS.has(key) && typeof value === "string" && value.length <= __TUIC_MAX_HEADER_VALUE__) {
        allowed[key] = value;
      }
    }
    return allowed;
  }

  function close() {
    active = false;
    epoch++;
    waits.clear();
    running = false;
    for (const restore of restorers.splice(0)) restore();
    publish();
  }

  pi.on("session_start", (_event, ctx) => {
    epoch++;
    waits.clear();
    active = true;
    running = !ctx.isIdle();
    // Print-mode methods are no-ops. RPC belongs to its own UI owner, not a PTY.
    if (ctx.mode === "tui") {
      const ui = ctx.ui;
      for (const name of ["select", "confirm", "input", "editor", "custom"]) {
        const original = ui[name];
        const wrapped = async function (...args) {
          if (!active) return original.apply(ui, args);
          const ownerEpoch = epoch;
          const owner = {};
          waits.add(owner);
          publish();
          try {
            return await original.apply(ui, args);
          } finally {
            // A late promise from a closed session cannot affect its successor.
            if (active && epoch === ownerEpoch) {
              waits.delete(owner);
              publish();
            }
          }
        };
        ui[name] = wrapped;
        restorers.push(() => { if (ui[name] === wrapped) ui[name] = original; });
      }
    }
    publish();
  });
  for (const event of ["agent_start", "tool_call", "tool_execution_start", "tool_execution_end"]) {
    pi.on(event, (data) => {
      if (!active) return;
      running = true;
      publish();
      if (event === "tool_execution_start") tool("start", data);
      if (event === "tool_execution_end") tool("end", data);
    });
  }
  pi.on("agent_end", () => { if (active) { running = false; publish(); } });
  // Status and quota headers of each provider response; the agent dir and
  // home name the pi credential, as pi resolves it.
  pi.on("after_provider_response", (event, ctx) => {
    const provider = ctx?.model?.provider;
    if (!active || typeof provider !== "string" || typeof event?.status !== "number") return;
    emit("provider_response", {
      provider,
      status: event.status,
      headers: quotaHeaders(event.headers),
      agent_dir: process.env.PI_CODING_AGENT_DIR ?? "",
      home: process.env.HOME ?? process.env.USERPROFILE ?? "",
    });
  });
  pi.on("session_shutdown", close);
  // model_select is configuration, not submitted work.
}
"#;

fn is_owned(content: &str) -> bool {
    content.lines().next() == Some(MARKER)
}

pub(crate) fn install_pi_extension(path: &Path) -> Result<(), String> {
    if path.exists() {
        let content = std::fs::read_to_string(path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        if !is_owned(&content) {
            return Err(format!("{} exists and is not TUIC-managed", path.display()));
        }
    }
    crate::config::persist_atomic(path, pi_extension_source().as_bytes())
}

pub(crate) fn uninstall_pi_extension(path: &Path) -> Result<(), String> {
    if path.exists() {
        let content = std::fs::read_to_string(path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        if is_owned(&content) {
            std::fs::remove_file(path)
                .map_err(|error| format!("remove {}: {error}", path.display()))?;
        }
    }
    Ok(())
}

pub(crate) fn pi_extension_state(path: &Path) -> InstallState {
    match std::fs::read_to_string(path) {
        Ok(content) if content == pi_extension_source() => InstallState::Installed,
        Ok(content) if is_owned(&content) => InstallState::Outdated,
        _ => InstallState::NotInstalled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_modal_survives_background_tool_and_clears_on_close() {
        // Catches: a tool clears an open modal, a rejected promise latches it,
        // or an old session's finally clears a successor's modal; a tool frame
        // carries arguments or results, or an end names another call.
        // This exercises our extension through its public callback/UI ports.
        // Native API compatibility is separately recorded in the Pi fixtures.
        let dir = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
        let source = dir.path().join("tuic.mjs");
        std::fs::write(&source, pi_extension_source()).unwrap();
        let output = std::process::Command::new("node")
            .args([
                "--input-type=module",
                "-e",
                r#"
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
const frames = [];
const callbacks = new Map();
let settle;
let reject;
const ui = Object.fromEntries(['select', 'confirm', 'input', 'editor', 'custom'].map(name =>
  [name, () => new Promise((yes, no) => { settle = yes; reject = no; })]));
const ctx = {mode: 'tui', ui, isIdle: () => true};
const code = readFileSync(process.argv[1], 'utf8')
  .replace(/^import .* from "node:fs";$/m, '')
  .replace('export default function', 'globalThis.start = function');
const sandbox = {
  process: {env: {TUIC_SESSION: 'pi-test'}}, Buffer,
  openSync: () => 1, closeSync: () => {},
  writeSync: (_fd, data) => frames.push(data),
};
runInNewContext(code, sandbox);
sandbox.start({on: (name, callback) => callbacks.set(name, callback)});
const event = name => callbacks.get(name)?.({}, ctx);
const expect = states => {
  assert.deepEqual(frames.splice(0), states.map(state => `\x1b]7770;state=${state}\x1b\\`));
};
event('session_start'); expect(['idle']);
event('model_select'); expect([]);
for (const name of ['select', 'confirm', 'input', 'editor', 'custom']) {
  const pending = ui[name](); expect(['awaiting']);
  event('tool_execution_start'); event('tool_call'); event('tool_execution_end');
  expect([]);
  settle(undefined); await pending; expect(['busy']);
  event('agent_end'); expect(['idle']);
  const cancelled = ui[name](); expect(['awaiting']);
  settle(undefined); await cancelled; expect(['idle']);
  const failed = ui[name](); expect(['awaiting']);
  reject(new Error('modal failure'));
  await assert.rejects(failed, /modal failure/); expect(['idle']);
}
const old = ui.select(); const oldSettle = settle; expect(['awaiting']);
event('session_shutdown'); expect(['idle']);
event('session_start'); expect([]);
const current = ui.input(); const currentSettle = settle; expect(['awaiting']);
oldSettle(undefined); await old; expect([]);
event('agent_start'); event('agent_end'); expect([]);
currentSettle(undefined); await current; expect(['idle']);
event('session_shutdown'); expect([]);
// Tool readout: id and name only; an end names only its own call.
const fire = (name, data) => callbacks.get(name)?.(data, ctx);
const tools = () => frames.splice(0).filter(f => f.includes('agent-tool=')).map(f => {
  const [owner, encoded] = f.slice('\x1b]7770;agent-tool='.length, -2).split(':');
  assert.equal(owner, 'pi-test');
  return JSON.parse(Buffer.from(encoded, 'base64').toString());
});
fire('tool_execution_start', {toolCallId: 'a', toolName: 'bash', args: {command: 'secret'}});
assert.deepEqual(tools(), []);
event('session_start'); frames.splice(0);
fire('tool_execution_start', {toolCallId: 'a', toolName: 'bash', args: {command: 'secret'}});
fire('tool_execution_start', {toolCallId: 'b', toolName: 'read', args: {path: 'x'}});
fire('tool_execution_end', {toolCallId: 'a', toolName: 'bash', result: 'secret', isError: true});
fire('tool_execution_start', {toolName: 'nameless-id'});
assert.deepEqual(tools(), [
  {event: 'start', id: 'a', name: 'bash'},
  {event: 'start', id: 'b', name: 'read'},
  {event: 'end', id: 'a'},
]);
event('session_shutdown'); frames.splice(0);
const outside = {process: {env: {}}};
runInNewContext(code, outside);
outside.start({on: () => assert.fail('registered outside TUIC')});
"#,
            ])
            .arg(&source)
            .output()
            .expect("Node.js is required for generated Pi extension behavior tests");
        assert!(
            output.status.success(),
            "generated extension failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, [] as [u8; 0]);
        assert_eq!(output.stderr, [] as [u8; 0]);
    }

    #[test]
    fn pi_installer_preserves_unmarked_files_and_updates_only_owned_extensions() {
        // Catches: enabling or removing hooks destroys a user extension.
        let dir = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
        let path = dir.path().join("extensions/tuic.ts");
        install_pi_extension(&path).unwrap();
        assert_eq!(pi_extension_state(&path), InstallState::Installed);
        std::fs::write(&path, format!("{MARKER}\n// old version\n")).unwrap();
        assert_eq!(pi_extension_state(&path), InstallState::Outdated);
        install_pi_extension(&path).unwrap();
        assert_eq!(pi_extension_state(&path), InstallState::Installed);
        uninstall_pi_extension(&path).unwrap();
        assert!(!path.exists());
        uninstall_pi_extension(&path).unwrap();
        let user = format!("// user extension\n// quoted marker: {MARKER}\n");
        std::fs::write(&path, &user).unwrap();
        assert!(install_pi_extension(&path).is_err());
        uninstall_pi_extension(&path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), user);
        assert_eq!(pi_extension_state(&path), InstallState::NotInstalled);
    }
}
