#!/usr/bin/env python3
"""Catch missing headless GitHub routes with a real isolated tuic-remote binary.

Usage: python3 scripts/test-headless-github.py /absolute/path/to/tuic-remote
No GitHub credentials or network data are required or verified.
"""

import hashlib
import http.client
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import time
from urllib.parse import urlencode


class UnixHTTP(http.client.HTTPConnection):
    def __init__(self, path):
        super().__init__("localhost", timeout=10)
        self.path = str(path)

    def connect(self):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(self.timeout)
        self.sock.connect(self.path)


def main():
    binary = Path(sys.argv[1]).resolve(strict=True)
    base = Path.home() / "Gits" / ".tmp"
    base.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="gh2345-", dir=base) as temporary:
        root = Path(temporary)
        home = root / "home"
        home.mkdir()
        repo = root / "repo"
        subprocess.run(["git", "init", "--quiet", "--initial-branch=main", str(repo)], check=True)
        instance = "gh2345-" + hashlib.sha256(str(root).encode()).hexdigest()[:16]
        digest = hashlib.sha256(instance.encode()).hexdigest()[:16]
        endpoint = root / ("tuic-mcp-" + digest + ".sock")
        env = {
            "HOME": str(home),
            "XDG_CONFIG_HOME": str(home / ".config"),
            "TMPDIR": str(root) + "/",
            "PATH": "/usr/bin:/bin:/usr/sbin:/sbin",
            "TUIC_PORT": "0",
            "TUIC_TEST_USE_FILE_VAULT": "1",
        }
        with (root / "daemon.log").open("w+") as log:
            process = subprocess.Popen(
                [str(binary), "--instance", instance, "--bind", "127.0.0.1", "--no-agent-configs"],
                env=env, stdout=log, stderr=log,
            )
            try:
                # Setup budget only; request deadlines start after socket readiness.
                deadline = time.monotonic() + 60
                while not endpoint.exists():
                    if process.poll() is not None or time.monotonic() >= deadline:
                        raise RuntimeError("daemon did not create its isolated Unix socket")
                    time.sleep(0.05)

                def request(method, path, body=None, expected=200):
                    connection = UnixHTTP(endpoint)
                    try:
                        connection.request(method, path, None if body is None else json.dumps(body),
                                           {"Content-Type": "application/json"})
                        response = connection.getresponse()
                        payload = response.read().decode()
                        print(method, path, response.status, payload, flush=True)
                        assert response.status == expected, (path, response.status, payload)
                        return json.loads(payload)
                    finally:
                        connection.close()

                for paths in ([], [str(repo)]):
                    assert request("POST", "/repo/github-poller/start", {
                        "paths": paths, "issueFilter": "all", "prHideDrafts": False,
                    }) == {"ok": True}
                    status = request("GET", "/repo/github?" + urlencode({"path": str(repo)}))
                    assert status == {"has_remote": False, "current_branch": "main", "ahead": 0, "behind": 0}
                    for path in ("/repo/prs", "/repo/issues"):
                        assert request("GET", path + "?" + urlencode({"path": str(repo)})) == []
                    assert request("POST", "/repo/prs/batch", {"paths": paths, "include_merged": False}) == {}
                    assert request("POST", "/repo/github-poller/visibility", {"visible": False}) == {"ok": True}
                    assert request("POST", "/repo/github-poller/update-paths", {"paths": paths}) == {"ok": True}
                    assert request("POST", "/repo/github-poller/set-issue-filter", {"filter": "all"}) == {"ok": True}
                    assert request("POST", "/repo/github-poller/poll-repo", {"path": str(repo)}) == {"ok": True}
                    assert request("GET", "/repo/github-poller/api-debug") == {"enabled": False}
                    assert request("POST", "/repo/github-poller/stop") == {"ok": True}
                    # Catches: controls falsely succeed after the poller has stopped.
                    result = request("POST", "/repo/github-poller/visibility", {"visible": True}, expected=500)
                    assert "error" in result
                print("PASS: real headless Unix lifecycle, no-remote status/data, stop and restart; no GitHub authentication claim")
            except BaseException:
                log.flush()
                log.seek(0)
                print(log.read(), file=sys.stderr)
                raise
            finally:
                if process.poll() is None:
                    process.send_signal(signal.SIGINT)
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=10)


if __name__ == "__main__":
    main()
