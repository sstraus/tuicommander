#!/usr/bin/env python3
"""Build and supervise the isolated macOS desktop verification binary.

Dry runs do not build, create directories, read application config, or launch TUIC.
"""
import argparse
import hashlib
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import threading

ROOT = Path(__file__).resolve().parent.parent
BINARY = "tuicommander-agent-test"


def runtime_root() -> Path:
    """Where each instance's socket, pid file and stop FIFO live.

    The host temp dir (the one scripts/with-test-tmp.sh records before it
    points TMPDIR at a long per-run test root), so the socket path stays under
    the macOS Unix socket limit. TUIC_AGENT_TEST_ROOT overrides it.
    """
    override = os.environ.get("TUIC_AGENT_TEST_ROOT")
    if override:
        return Path(override)
    # $TMPDIR is the caller's scratch root (in a sandbox, the only place it may write), so the
    # runtime files live under it, never under a broader host-wide temp dir.
    host = Path(os.environ.get("TUIC_TEST_HOST_TMPDIR") or tempfile.gettempdir())
    return host / "tuic-at"


def build_root(root: Path = ROOT) -> Path:
    """Where this checkout's isolated binaries are copied, outside the checkout.

    `<base>/<first 8 hex of sha256(resolved checkout path)>`, so two worktrees never share a
    binary. The base is TUIC_AGENT_TEST_BUILD_ROOT, else `tuic-agent-test` under the host temp
    dir. The OS may purge it; the binary is a rebuild-on-demand artifact.
    """
    override = os.environ.get("TUIC_AGENT_TEST_BUILD_ROOT")
    base = Path(override) if override else (
        Path(os.environ.get("TUIC_TEST_HOST_TMPDIR") or tempfile.gettempdir()) / "tuic-agent-test")
    return base / hashlib.sha256(str(root.resolve()).encode()).hexdigest()[:8]


def build_runner() -> list[str]:
    """Optional build admission wrapper (e.g. a build-slot scheduler), named by TUIC_BUILD_SLOT."""
    slot = os.environ.get("TUIC_BUILD_SLOT")
    return [slot] if slot and Path(slot).is_file() else []


MBX_BIN = Path.home() / "Library/Application Support/mbx/bin"


def mbx_active() -> bool:
    """Whether builds run through mbx (https://mr-boxington.jdx.dev), an optional Cargo cache.

    mbx is a developer convenience, never a requirement: without it the launcher builds with the
    plain `cargo` on PATH, only slower. Set TUIC_NO_MBX=1 to ignore an installed mbx.
    """
    return os.environ.get("TUIC_NO_MBX") != "1" and shutil.which("mbx") is not None


def plan(instance: str, port: str, speech_output: bool = False) -> dict:
    if not re.fullmatch(r"[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?", instance) or instance == "default":
        raise ValueError("instance must be a non-default lowercase DNS label")
    if not port.isascii() or not port.isdecimal() or not 1024 <= int(port) <= 65535 or int(port) == 9876:
        raise ValueError("port must be in 1024..65535 and must not be 9876")
    if speech_output and not instance.startswith("agent-test-"):
        raise ValueError("speech output requires an agent-test-<name> instance")
    if mbx_active() and os.environ.get("CARGO_TARGET_DIR"):
        raise ValueError("unset CARGO_TARGET_DIR: mbx leaves an explicitly chosen target directory unmanaged")
    # Short, instance-specific runtime files. The directory takes 8 hex digits
    # of the instance hash: with the full 16 a 49-byte macOS host temp dir puts
    # the socket at exactly the 104-byte SUN_LEN limit.
    digest = hashlib.sha256(instance.encode()).hexdigest()[:16]
    runtime = runtime_root() / digest[:8]
    socket = runtime / f"tuic-mcp-{digest}.sock"
    if len(os.fsencode(socket)) >= 104:
        raise ValueError("runtime socket path exceeds the macOS Unix socket limit; set TUIC_AGENT_TEST_ROOT to a shorter directory")
    executable = build_root() / instance / BINARY
    # Tao's launch activation hack makes every initially visible window key,
    # even with focus=false. Agent-test presents it without a key only at Ready.
    config = json.loads((ROOT / "src-tauri/tauri.conf.json").read_text())
    window = {**config["app"]["windows"][0], "visible": False, "focus": False,
              "focusable": False, "center": False, "alwaysOnTop": True,
              "width": 1400, "height": 900, "minWidth": 390, "minHeight": 420,
              "backgroundThrottling": "disabled"}
    override = {"identifier": "com.tuic.commander.agent-test",
                "productName": "TUICommander Agent Test", "bundle": {"externalBin": []},
                "app": {"windows": [window], "security": {"capabilities": ["default", {
                    "identifier": "agent-test-presentation", "windows": ["main"],
                    "permissions": ["core:window:deny-set-focus", "core:window:deny-show",
                                    "core:window:allow-set-size"]
                }]}}}
    cargo = ["cargo", "build", "--manifest-path", "src-tauri/Cargo.toml", "-p", "tuicommander", "-p", "tuic-bridge", "-p", "tuic-cli", "--bin", BINARY, "--bin", "tuic-bridge", "--bin", "tuic", "--features", "tauri/custom-protocol", "--message-format=json-render-diagnostics"]
    if speech_output:
        cargo[cargo.index("tauri/custom-protocol")] += ",dictation"
    if os.environ.get("AGENT_TEST_HEAP_PROFILE") == "1":
        cargo[cargo.index("--features") + 1] += ",heap-profile"
    return {
        "binary": str(executable),
        "config_dir": str(Path.home() / "Library/Application Support/com.tuic.commander/instances" / instance),
        "listener": f"127.0.0.1:{int(port)} (no port fallback)",
        "socket": str(socket),
        "connect": f"curl --unix-socket '{socket}' http://localhost/health # Agents use this socket; TCP requires Basic auth",
        "pid_file": str(runtime / "desktop.pid"),
        "stop_fifo": str(runtime / "stop.fifo"),
        "environment": {"TUIC_APP_INSTANCE": instance, "TUIC_PORT": str(int(port)), "TMPDIR": str(runtime) + "/", "TUIC_AGENT_TEST_SPEECH_OUTPUT": "1" if speech_output else "0",
                        **({"_RJEM_MALLOC_CONF": os.environ.get("_RJEM_MALLOC_CONF", "prof:true,prof_active:false")} if os.environ.get("AGENT_TEST_HEAP_PROFILE") == "1" else {})},
        "cargo": cargo,
        "frontend": ["pnpm", "exec", "vite", "build"],
        "build_environment": {"mbx": mbx_active(), "PATH_prefix": str(MBX_BIN) if mbx_active() and (MBX_BIN / "cargo").is_file() else None, "TAURI_CONFIG": json.dumps(override)},
        "launch": [str(executable)],
        "disabled": ["global shortcuts", "microphone" if speech_output else "dictation/microphone", "Telegram", "updater", "global MCP install", "global CLI mutations", "global agent hooks", "deep links", "sleep inhibition"],
    }


def stop(launch: dict) -> None:
    # No PID signalling from a file: only the live supervisor can stop its child.
    try:
        if not stat.S_ISFIFO(os.stat(launch["stop_fifo"]).st_mode):
            raise RuntimeError("agent-test stop endpoint is not a FIFO")
        descriptor = os.open(launch["stop_fifo"], os.O_WRONLY | os.O_NONBLOCK)
    except (FileNotFoundError, OSError) as error:
        raise RuntimeError("no running agent-test supervisor at this instance") from error
    try:
        os.write(descriptor, b"stop\n")
    finally:
        os.close(descriptor)
    print("Stop requested for this agent-test instance only")


def check_reused_build(launch: dict, expected: str) -> dict:
    if len(expected) != 64 or any(c not in '0123456789abcdef' for c in expected):
        raise ValueError('reuse requires an explicit lowercase SHA-256')
    directory = Path(launch['binary']).parent
    hashes = {}
    for name in (BINARY, 'tuic-bridge', 'tuic'):
        path = directory/name
        if not path.is_file() or path.is_symlink():
            raise RuntimeError(f'reuse requires the existing regular artifact {name}')
        hashes[name] = hashlib.sha256(path.read_bytes()).hexdigest()
    if hashes[BINARY] != expected:
        raise RuntimeError('existing agent-test binary differs from the expected SHA-256')
    return hashes


def start(launch: dict, reuse_sha256=None) -> int:
    if sys.platform != "darwin":
        raise RuntimeError("agent-test is a macOS desktop target")
    # Refuse the daily/main checkout even though this target has its own name.
    if not (ROOT / ".git").is_file():
        raise RuntimeError("run agent-test from a linked worktree, not the daily checkout")
    runtime = Path(launch["pid_file"]).parent
    runtime.mkdir(parents=True, exist_ok=True)
    with (runtime / "supervisor.lock").open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RuntimeError("this agent-test instance already has a supervisor") from None
        env = dict(os.environ)
        # rustc can itself embed jemalloc without profiling: diagnostic options
        # belong to the app process, never mbx/rustc or frontend build tools.
        env.pop("_RJEM_MALLOC_CONF", None)
        use_mbx = launch["build_environment"]["mbx"]
        if use_mbx:
            # A standalone `mbx setup` shim lives under MBX_BIN; a mise-managed install wraps
            # cargo elsewhere, so `mbx doctor` is the check that the active cargo is wrapped.
            if launch["build_environment"]["PATH_prefix"]:
                env["PATH"] = launch["build_environment"]["PATH_prefix"] + os.pathsep + env["PATH"]
                if shutil.which("cargo", path=env["PATH"]) != str(MBX_BIN / "cargo"):
                    raise RuntimeError("cargo must resolve to the mbx shim")
        elif os.environ.get("TUIC_NO_MBX") == "1":
            # A shim already on PATH would still wrap cargo; MBX_DISABLE=1 is mbx's own off switch.
            env["MBX_DISABLE"] = "1"
        if reuse_sha256 is None:
            if use_mbx:
                subprocess.run(["mbx", "doctor"], env=env, cwd=ROOT, check=True)
            else:
                print("mbx not installed (or TUIC_NO_MBX=1): building with plain cargo; "
                      "see docs/guides/development-setup.md#optional-mbx-shared-cargo-cache", file=sys.stderr)
            runner = build_runner()
            subprocess.run([*runner, *launch["frontend"]], env=env, cwd=ROOT, check=True)
            # Embedded assets, separate application identity, no dev server or bundle writes.
            env["TAURI_CONFIG"] = launch["build_environment"]["TAURI_CONFIG"]
            # An optional shared build freeze file, named by TUIC_BUILD_FREEZE.
            freeze = Path(os.environ.get("TUIC_BUILD_FREEZE") or os.devnull)
            if freeze.is_file() and "feat-2086" not in freeze.read_text():
                raise RuntimeError("BUILD_FREEZE does not allow feat-2086")
            artifacts = {}
            with subprocess.Popen([*runner, *launch["cargo"]], cwd=ROOT, env=env, stdout=subprocess.PIPE, text=True) as build:
                for line in build.stdout:
                    try:
                        message = json.loads(line)
                    except json.JSONDecodeError:
                        print(line, end="", file=sys.stderr)
                        continue
                    if message.get("reason") == "compiler-artifact" and message.get("executable"):
                        artifacts[message["target"]["name"]] = Path(message["executable"])
                    if message.get("reason") == "compiler-message":
                        print(message["message"].get("rendered", ""), end="", file=sys.stderr)
                if build.wait():
                    raise RuntimeError("agent-test Cargo build failed")
            directory = Path(launch["binary"]).parent
            directory.mkdir(parents=True, exist_ok=True)
            for name in [BINARY, "tuic-bridge", "tuic"]:
                source = artifacts.get(name)
                if source is None or not source.is_file():
                    raise RuntimeError(f"build did not produce {name}")
                staging = directory / f".{name}.new"
                shutil.copy2(source, staging)
                staging.replace(directory / name)
        else:
            launch['reused_artifacts'] = check_reused_build(launch, reuse_sha256)
        # Do not inherit client endpoints, dev paths or global MCP ownership overrides.
        for key in ("TUIC_SOCKET", "TUIC_DEV_BIN_DIR", "TUIC_MCP_CONFIG_OWNER", "TUIC_CAPTURE_DIR", "TUIC_SESSION", "TUIC_PARENT"):
            env.pop(key, None)
        env.update(launch["environment"])
        fifo = Path(launch["stop_fifo"])
        fifo.unlink(missing_ok=True)
        os.mkfifo(fifo, 0o600)
        child = subprocess.Popen([launch["binary"]], cwd=ROOT, env=env)
        pid_file = Path(launch["pid_file"])
        try:
            pid_file.write_text(str(child.pid) + "\n")

            def forward(signum, _frame):
                if child.poll() is None:
                    child.send_signal(signum)

            def listen():
                with fifo.open() as requests:
                    if requests.readline().strip() == "stop" and child.poll() is None:
                        child.terminate()

            for signum in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
                signal.signal(signum, forward)
            threading.Thread(target=listen, daemon=True).start()
            print(json.dumps(launch, indent=2), flush=True)
            status = child.wait()
            return status if status >= 0 else 128 - status
        finally:
            if child.poll() is None:
                child.terminate()
                child.wait()
            pid_file.unlink(missing_ok=True)
            fifo.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["start", "stop"])
    parser.add_argument("--instance", default="agent-test")
    parser.add_argument("--port", default="19876")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--speech-output", action="store_true", help="enable TTS only; microphone remains disabled")
    parser.add_argument("--reuse-build-sha256", help="restart the existing isolated binary only at this exact digest")
    args = parser.parse_args()
    launch = plan(args.instance, args.port, args.speech_output)
    if args.dry_run:
        print(json.dumps(launch, indent=2))
        return 0
    if args.action == "stop":
        stop(launch)
        return 0
    return start(launch, args.reuse_build_sha256)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"agent-test: {error}", file=sys.stderr)
        sys.exit(1)
