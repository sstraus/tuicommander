#!/usr/bin/env python3
"""Record real agent CLIs in an isolated debug headless TUIC daemon."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
import re
from pathlib import Path
import secrets
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

from scenario import AGENTS, exit_session, expand, passthrough_env, run_steps, validate, write_cwd_files

ROOT = Path(__file__).resolve().parents[2]


def inventory():
    return {agent: {"binary": shutil.which(binary), "status": "unverified",
                    "reason": "not captured" if shutil.which(binary) else "not installed"}
            for agent, binary in AGENTS.items()}


def scratch_root():
    """The host temp dir; scripts/with-test-tmp.sh records it as TUIC_TEST_HOST_TMPDIR
    before pointing TMPDIR at a long per-run root that would overflow the daemon's
    socket path."""
    return Path(os.environ.get("TUIC_TEST_HOST_TMPDIR") or tempfile.gettempdir()).resolve()


def under_scratch(path):
    path = Path(path).expanduser().resolve()
    if not path.is_relative_to(scratch_root()):
        raise ValueError(f"path must stay under the host temp dir {scratch_root()}: {path}")
    return path


def recording_output(data):
    if not data.startswith(b"TUICCAP2\n") or len(data) <= 13:
        raise ValueError("promotion requires a nonempty TUICCAP2 recording")
    cursor, records, output = 13, 0, []
    while cursor < len(data):
        if len(data) - cursor < 13 or data[cursor] not in (0, 1):
            raise ValueError("truncated or invalid capture record")
        length = struct.unpack_from("<I", data, cursor + 9)[0]
        direction = data[cursor]
        begin = cursor + 13
        cursor += 13 + length
        if cursor > len(data):
            raise ValueError("truncated capture payload")
        if direction == 0:
            output.append(data[begin:cursor])
        records += 1
    if not records:
        raise ValueError("capture contains no records")
    return b"".join(output).decode(errors="replace")


def reject_login(text):
    # Match real CLI failures without depending on cursor-position whitespace.
    clean = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text)
    clean = re.sub(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)", "", clean)
    compact = "".join(clean.casefold().split())
    if any(marker in compact for marker in ("loginexpired", "notloggedin", "pleaserun/login", "run/loginto")):
        raise RuntimeError("CLI requires login; agent remains unverified")


def portable(metadata):
    """Replace absolute recording paths (they embed the user's home) with placeholders."""
    metadata = json.loads(json.dumps(metadata))
    health = metadata.get("daemon_health")
    if health and health.get("socket_path"):
        health["socket_path"] = "<output>/" + Path(health["socket_path"]).name
    if metadata.get("capture"):
        metadata["capture"] = "<output>/raw/" + Path(metadata["capture"]).name
    return metadata


def promote(capture, metadata, name):
    """Promote only complete real recordings; baseline regeneration is explicit."""
    if not name or any(c not in "abcdefghijklmnopqrstuvwxyz0123456789-" for c in name):
        raise ValueError("fixture name must contain lowercase letters, digits and hyphens")
    if metadata.get("agent") not in {*AGENTS, "claude", "codex", "grok", "opencode"}:
        raise ValueError("unsupported capture agent")
    data = capture.read_bytes()
    reject_login(recording_output(data))
    if metadata.get("status") != "captured" or not metadata.get("expected_states"):
        raise ValueError("only a completed scenario with expected states can be promoted")
    if hashlib.sha256(data).hexdigest() != metadata["sha256"]:
        raise ValueError("recording changed since capture; review it again")
    dest = ROOT / "src-tauri/src/fixtures/agent_prompts" / f"{metadata['agent']}-{name}.tcap"
    if any(path.exists() for path in (dest, dest.with_suffix(".scenario.json"), dest.with_suffix(".md"))):
        raise ValueError("fixture already exists; promotion never overwrites evidence")
    dest.write_bytes(data)
    dest.with_suffix(".scenario.json").write_text(json.dumps(portable(metadata), indent=2) + "\n")
    dest.with_suffix(".md").write_text(
        f"# {metadata['agent']} scenario capture\n\n"
        "Source: real CLI output recorded through `/diagnostics/capture` on an\n"
        "isolated headless `tuic-remote --instance` (agent capture driver).\n\n"
        f"Captured at: {metadata.get('recorded_at', 'see original capture report')}.\n\n"
        f"SHA-256: `{metadata['sha256']}`. The `.tcap` bytes are unchanged.\n\n"
        "See the companion `.scenario.json` for source binary, original scenario,\n"
        "observed states and independent ordered replay expectations.\n"
    )
    return dest


def capture_run(args):
    scenario = json.loads(args.scenario.read_text())
    expected = validate(scenario)
    installed = inventory()
    agent = scenario["agent"]
    if not installed[agent]["binary"]:
        raise RuntimeError(f"{agent}: unverified (not installed)")
    output = under_scratch(args.output)
    output.mkdir(parents=True, exist_ok=False)
    home = output / "daemon-home"
    home.mkdir()
    agent_home = Path(args.agent_home).expanduser().resolve()
    if agent_home != Path.home():
        agent_home = under_scratch(agent_home)
    if not agent_home.is_dir():
        raise ValueError("agent-home must already exist; no login is automated")
    daemon = args.daemon.resolve()
    # Release startup probes the native vault. Only accept an explicit debug artifact.
    if "debug" not in daemon.parts or daemon.name != "tuic-remote":
        raise ValueError("use a debug/tuic-remote build (release may access the native keychain)")
    token = secrets.token_hex(32)
    instance = "capture-" + secrets.token_hex(6)
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    # Do not inherit config-directory overrides, credentials, MCP identity or proxy
    # settings from the managed peer. Agents use the explicitly provisioned HOME.
    env = {key: os.environ[key] for key in ("PATH", "LANG", "LC_ALL", "SYSTEMROOT")
           if key in os.environ}
    env.update(HOME=str(home), XDG_CONFIG_HOME=str(home / ".config"),
               XDG_CACHE_HOME=str(home / ".cache"), TUIC_PORT=str(port),
               TUIC_CAPTURE_DIR=str(output / "raw"), TUIC_PAIRING_TOKEN=token,
               TMPDIR=str(output), TMP=str(output), TEMP=str(output))
    report = {"recorded_at": datetime.now(timezone.utc).isoformat(), "agent": agent, "binary": installed[agent]["binary"], "scenario": scenario,
              "expected_states": expected, "status": "unverified"}
    session_id = None
    # One rule text per scenario, placed into args/env by {canary_rule}/{canary_file}.
    rule_file = output / "canary-rule.md"
    rule_file.write_text(scenario.get("canary_rule", "") + "\n")
    placeholders = {"canary_rule": scenario.get("canary_rule", ""), "canary_file": str(rule_file),
                    "home": str(agent_home)}
    sessions, checks, markers = [], [], []
    if scenario.get("cwd_files"):
        write_cwd_files(output, scenario["cwd_files"], placeholders)

    def api(method, path, body=None):
        request = urllib.request.Request(
            f"http://127.0.0.1:{port}{path}", method=method,
            data=None if body is None else json.dumps(body).encode(),
            headers={"Cookie": f"tui-session={token}", "Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(request, timeout=10) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            # Never log response bodies: an agent may echo prompt or credential content.
            # A rejected submit names its cause in enum fields only; keep just those.
            detail = ""
            if error.code == 409:
                try:
                    body = json.load(error)
                    detail = " " + " ".join(f"{key}={body[key]}" for key in ("reason", "composer_state")
                                            if isinstance(body.get(key), str) and body[key].isidentifier())
                except (ValueError, OSError):
                    pass
            raise RuntimeError(f"{method} {path}: HTTP {error.code}{detail.rstrip()}") from None

    with (output / "daemon.log").open("w") as log:
        process = subprocess.Popen([str(daemon), "--instance", instance, "--bind", "127.0.0.1",
                                    "--no-agent-configs"], env=env, stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 60
            while True:
                if process.poll() is not None:
                    raise RuntimeError("headless daemon exited; inspect its isolated log")
                try:
                    health = api("GET", "/health")
                    # instance_id is a fresh process UUID, not the --instance label.
                    # The daemon's socket must live in this run's unique temp/HOME.
                    socket_path = health.get("socket_path")
                    if os.name == "posix" and (not socket_path or not Path(socket_path).is_relative_to(output)):
                        raise RuntimeError("port belongs to another instance")
                    if health.get("session_count") != 0:
                        raise RuntimeError("capture daemon must start without existing sessions")
                    report["daemon_health"] = health
                    break
                except (urllib.error.URLError, TimeoutError):
                    if time.monotonic() >= deadline:
                        raise RuntimeError("headless daemon did not become ready") from None
                    time.sleep(0.1)
            enabled = api("POST", "/diagnostics/capture", {"enabled": True})
            if not enabled.get("enabled"):
                raise RuntimeError("capture tap did not start")
            def spawn(args, env=None):
                new_id = api("POST", "/sessions/agent", {
                    "agent_type": agent, "binary_path": installed[agent]["binary"], "prompt": "",
                    "args": expand(args, placeholders), "cwd": str(output), "rows": 41, "cols": 128,
                    "env": {"HOME": str(agent_home), "XDG_CONFIG_HOME": str(agent_home / ".config"),
                            "XDG_CACHE_HOME": str(agent_home / ".cache"), "TMPDIR": str(output),
                            "TMP": str(output), "TEMP": str(output),
                            # The agent's TUIC MCP bridge must reach this isolated daemon,
                            # never the user's running TUICommander.
                            "TUIC_SOCKET": report["daemon_health"]["socket_path"],
                            # Login-keychain lookups are keyed by account name.
                            **{key: os.environ[key] for key in ("USER", "LOGNAME") if key in os.environ},
                            # API keys a scenario names; the VM matrix has no logged-in HOME.
                            **passthrough_env(scenario, os.environ),
                            **expand(scenario.get("env", {}) if env is None else env, placeholders)},
                })["session_id"]
                sessions.append(new_id)
                return new_id

            def end(old_id, exit_text=None):
                # One compliance read per session, before it is gone (#4421 counters).
                rows = api("GET", "/diagnostics/markers").get("sessions", [])
                markers.append(next((row for row in rows if row.get("session_id") == old_id),
                                    {"session_id": old_id, "missing": True}))
                if exit_text:
                    exit_session(api, old_id, exit_text)
                try:
                    api("DELETE", f"/sessions/{old_id}")
                except RuntimeError as error:
                    if "HTTP 404" not in str(error):  # the agent already exited
                        raise

            def relaunch(old_id, args, env, exit_text, cwd_files=None):
                end(old_id, exit_text)
                if cwd_files:
                    write_cwd_files(output, cwd_files, placeholders)
                return spawn(args, env)

            session_id = spawn(scenario["args"])
            try:
                report["observed_states"] = run_steps(api, session_id, scenario["steps"],
                                                      canary=scenario.get("canary"),
                                                      relaunch=relaunch, results=checks,
                                                      ignore_prefixes=scenario.get("ignore_prefixes", ()),
                                                      idle_gate=scenario.get("idle_gate", False))
            finally:
                report.update(checks=checks, markers=markers, sessions=list(sessions))
            session_id = sessions[-1]
            end(session_id)
            session_id = None
            api("POST", "/diagnostics/capture", {"enabled": False})
            for sid in sessions:
                recording = output / "raw" / f"{sid}.tcap"
                if recording.exists():
                    reject_login(recording_output(recording.read_bytes()))
            recording = output / "raw" / f"{sessions[0]}.tcap"
            report.update(status="captured", sha256=hashlib.sha256(recording.read_bytes()).hexdigest())
            report["capture"] = str(recording)
        except Exception as error:
            report["reason"] = str(error)
            raise
        finally:
            for method, path, body in [
                ("POST", "/diagnostics/capture", {"enabled": False}),
                *(("DELETE", f"/sessions/{sid}", None) for sid in sessions if session_id),
            ]:
                if process.poll() is None:
                    try:
                        api(method, path, body)
                    except (OSError, RuntimeError):
                        pass
            if process.poll() is None:
                process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
            (output / "capture.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("inventory")
    record = commands.add_parser("record")
    record.add_argument("--daemon", type=Path, required=True)
    record.add_argument("--scenario", type=Path, required=True)
    record.add_argument("--agent-home", type=Path, default=Path.home())
    record.add_argument("--output", type=Path, required=True)
    promotion = commands.add_parser("promote")
    promotion.add_argument("report", type=Path)
    promotion.add_argument("--name", required=True)
    args = parser.parse_args()
    try:
        if args.command == "inventory":
            result = inventory()
        elif args.command == "record":
            result = capture_run(args)
        else:
            metadata = json.loads(args.report.read_text())
            result = str(promote(Path(metadata["capture"]), metadata, args.name))
        print(json.dumps(result, indent=2))
    except (OSError, ValueError, RuntimeError) as error:
        print(f"unverified: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
