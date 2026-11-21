#!/usr/bin/env python3
"""Exercise remote PTYs through the desktop MCP, never the daemon directly.

Read-only by default. --exercise explicitly authorizes submit and mail to the
selected session. Use a disposable, idle agent for the writable check.
"""

import argparse
import copy
import os
from pathlib import Path
import subprocess
import tempfile
import time
import http.client
import json
import socket
import sys
import urllib.error
import urllib.request
import uuid


class UnixHttpConnection(http.client.HTTPConnection):
    def __init__(self, path):
        super().__init__("localhost", timeout=75)
        self.path = path

    def connect(self):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(self.timeout)
        self.sock.connect(self.path)


class DesktopMcp:
    def __init__(self, url, socket_path=None):
        self.url = url.rstrip("/") + "/mcp"
        self.socket_path = socket_path
        self.session = None
        self.sequence = 0
        self.request("initialize", {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": "remote-mcp-regression", "version": "1"},
        })

    def http(self, method, data, headers):
        if self.socket_path:
            connection = UnixHttpConnection(self.socket_path)
            try:
                connection.request(method, "/mcp", body=data, headers=headers)
                response = connection.getresponse()
                body = response.read().decode()
                if response.status >= 400:
                    raise RuntimeError(f"MCP HTTP {response.status}: {body}")
                return response.headers, body
            finally:
                connection.close()
        request = urllib.request.Request(self.url, method=method, headers=headers, data=data)
        with urllib.request.urlopen(request, timeout=75) as response:
            return response.headers, response.read().decode()

    def request(self, method, params):
        self.sequence += 1
        headers = {"Content-Type": "application/json",
                   "Accept": "application/json, text/event-stream"}
        if self.session:
            headers["mcp-session-id"] = self.session
        data = json.dumps({
            "jsonrpc": "2.0", "id": self.sequence,
            "method": method, "params": params,
        }).encode()
        response_headers, body = self.http("POST", data, headers)
        self.session = response_headers.get("mcp-session-id", self.session)
        if response_headers.get_content_type() == "text/event-stream":
            body = next(line[5:].strip() for line in body.splitlines()
                        if line.startswith("data:"))
        payload = json.loads(body)
        if "error" in payload:
            raise RuntimeError(json.dumps(payload["error"]))
        return payload["result"]

    def call(self, tool_name, **arguments):
        result = self.request("tools/call", {"name": tool_name, "arguments": arguments})
        text = "".join(item["text"] for item in result.get("content", [])
                       if item.get("type") == "text")
        try:
            value = json.loads(text)
        except json.JSONDecodeError as error:
            raise RuntimeError(f"Non-JSON {tool_name} result: {text}") from error
        if result.get("isError") or (isinstance(value, dict) and "error" in value):
            raise RuntimeError(json.dumps(value))
        return value

    def close(self):
        if self.session:
            self.http("DELETE", None, {
                "mcp-session-id": self.session,
            })



def fixture_request(socket_path, method, path, value=None):
    """Fixture setup only; consumer assertions always use the hub MCP."""
    connection = UnixHttpConnection(socket_path)
    try:
        connection.request(method, path, body=json.dumps(value).encode() if value is not None else None,
                           headers={"Content-Type": "application/json"})
        response = connection.getresponse()
        body = response.read().decode()
        if response.status >= 400:
            raise RuntimeError(f"Fixture setup {path}: HTTP {response.status}: {body}")
        return json.loads(body)
    finally:
        connection.close()


def local_fixture(args):
    """Real isolated daemons, native mail clients; never a hand-written daemon."""
    # Logs and session cwd: the caller's temp dir (or TUIC_FIXTURE_ROOT), never $HOME.
    root = Path(os.environ.get("TUIC_FIXTURE_ROOT") or tempfile.gettempdir()) / "tuic-1419"
    root.mkdir(parents=True, exist_ok=True)
    launcher = Path(args.fixture_launcher)
    if not launcher.is_file():
        raise RuntimeError("Shared run-remote-fixture.sh is missing; pass --fixture-launcher")
    processes, logs, clients, sockets, urls = [], [], [], [], []
    token = "remote-mcp-fixture-" + uuid.uuid4().hex
    try:
        for role in ("hub", "first", "second"):
            probe = socket.socket()
            probe.bind(("127.0.0.1", 0))
            port = probe.getsockname()[1]
            probe.close()
            url = f"http://127.0.0.1:{port}"
            log = open(root / f"fixture-{role}.log", "w")
            logs.append(log)
            # The launcher picks each daemon's socket TMPDIR; /health reports it.
            env = dict(os.environ, TUIC_FIXTURE_TOKEN=token,
                       PAGER="cat", GIT_PAGER="cat")
            instance = "mcp-" + role + "-" + uuid.uuid4().hex[:8]
            process = subprocess.Popen(["bash", str(launcher), args.fixture_bin,
                                        str(port), instance], env=env, stdout=log, stderr=log)
            processes.append(process)
            deadline = time.monotonic() + 180
            while True:
                if process.poll() is not None:
                    raise RuntimeError(f"Fixture {role} exited; see {log.name}")
                try:
                    with urllib.request.urlopen(url + "/health?token=" + token, timeout=2) as response:
                        health = json.load(response)
                    socket_path = health["socket_path"]
                    client = DesktopMcp(url, socket_path)
                    break
                except (OSError, KeyError, urllib.error.URLError):
                    if time.monotonic() >= deadline:
                        raise RuntimeError(f"Fixture {role} never became ready; see {log.name}")
                    time.sleep(.1)
            clients.append(client)
            sockets.append(socket_path)
            urls.append(url)
        hub, first, second = clients
        connection_ids = []
        sessions, peers = [], []
        for index, remote in enumerate((first, second), start=1):
            password = "fixture-" + uuid.uuid4().hex
            hashed = fixture_request(sockets[index], "POST", "/config/hash-password",
                                     {"password": password})["hash"]
            base = fixture_request(sockets[index], "GET", "/config")
            config = copy.deepcopy(base)
            config["services"]["auth"].update(username="fixture", password_hash=hashed)
            fixture_request(sockets[index], "PUT", "/config", {"base": base, "config": config})
            peer = remote.call("agent", action="register", name=f"fixture-peer-{index}")
            peers.append(peer["tuic_session"])
            # Native shell PTY is enough to exercise output and semantic rejection;
            # no fake external agent is used or credited as a composer-wake check.
            created = remote.call("session", action="create", cwd=str(root))
            sessions.append(created["session_id"])
            connection_id = str(uuid.uuid4())
            connection_ids.append(connection_id)
            fixture_request(sockets[0], "PUT", "/config/remote-connections", {
                "base": None, "connection": {
                    "id": connection_id, "name": f"fixture-{index}", "enabled": True,
                    "transport": {"type": "Direct", "url": urls[index]},
                    "auth_username": "fixture", "deploy": "never",
                }})
            fixture_request(sockets[0], "PUT", f"/config/remote-connections/{connection_id}/password",
                            {"password": password})
            fixture_request(sockets[0], "POST", f"/config/remote-connections/{connection_id}/connect")
        registration = hub.call("agent", action="register", name="fixture-mac")
        mac_id = registration["tuic_session"]
        rows = hub.call("session", action="list")
        for connection_id, session_id in zip(connection_ids, sessions):
            if not any(row.get("connection_id") == connection_id and row["session_id"] == session_id
                       for row in rows):
                raise RuntimeError(f"Hub MCP omitted fixture PTY {connection_id}/{session_id}")
            output = hub.call("session", action="output", connection_id=connection_id,
                              session_id=session_id, limit=10)
            if not isinstance(output.get("data"), str) or "exited" not in output:
                raise RuntimeError(f"Output lost native MCP fields: {output}")
            rejected = hub.call("session", action="submit", connection_id=connection_id,
                                session_id=session_id, input="must never execute in this shell")
            if rejected.get("submitted") is not False or "agent" not in json.dumps(rejected).lower():
                raise RuntimeError(f"Semantic submit lost the shell-agent rejection: {rejected}")
        listed = hub.call("agent", action="list_peers")
        for connection_id, peer_id in zip(connection_ids, peers):
            if not any(row.get("address") == f"{connection_id}/{peer_id}"
                       for row in listed["peers"]):
                raise RuntimeError("Hub MCP omitted a remote native peer")
        marker = "fixture-" + uuid.uuid4().hex
        hub.call("agent", action="send", connection_id=connection_ids[0],
                 to=peers[0], message=marker)
        received = first.call("agent", action="wait", timeout_ms=1000)
        if received["messages"][0]["content"] != marker:
            raise RuntimeError("Mac-to-remote payload changed")
        first.call("agent", action="send", to=f"{connection_ids[1]}/{peers[1]}", message=marker)
        received = second.call("agent", action="wait", timeout_ms=1000)
        if received["messages"][0]["from_tuic_session"] != f"{connection_ids[0]}/{peers[0]}":
            raise RuntimeError("Remote-to-remote sender provenance changed")
        second.call("agent", action="send", to=f"local/{mac_id}", message=marker)
        reply = hub.call("agent", action="wait", timeout_ms=1000)
        if reply["messages"][0]["from_tuic_session"] != f"{connection_ids[1]}/{peers[1]}":
            raise RuntimeError("Remote reply did not reach the Mac native inbox")
        print(json.dumps({"step": "local-fixture-star", "ok": True,
                          "submit": "shell rejected by semantic agent guard",
                          "wake": "live composer requires --exercise"}))
        # Stop only our own hub; a spoke must still deliver local native mail.
        hub.close()
        clients[0] = None
        processes[0].terminate()
        processes[0].wait(timeout=15)
        recipient = DesktopMcp(urls[1], sockets[1])
        clients.append(recipient)
        recipient_id = recipient.call("agent", action="register", name="offline-local")["tuic_session"]
        first.call("agent", action="send", to=recipient_id, message=marker)
        local = recipient.call("agent", action="wait", timeout_ms=1000)
        if local["messages"][0]["content"] != marker:
            raise RuntimeError("Daemon-local mail failed after hub loss")
        print(json.dumps({"step": "hub-down-intrahost", "ok": True}))
    finally:
        for client in clients:
            if client:
                try:
                    client.close()
                except (OSError, RuntimeError):
                    pass
        for process in processes:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
        for log in logs:
            log.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--desktop", default="http://127.0.0.1:9876")
    parser.add_argument("--socket", help="Desktop MCP Unix socket; use the shared headless fixture launcher")
    parser.add_argument("--connection", help="Configured connection ID")
    parser.add_argument("--session", help="Remote PTY ID or alias; default first remote row")
    parser.add_argument("--exercise", action="store_true", help="Submit/mail to an idle disposable agent")
    parser.add_argument("--second-connection", help="Also exercise remote-to-remote mail through the hub")
    parser.add_argument("--second-session", help="Disposable recipient on the second connection")
    parser.add_argument("--fixture-bin", help="Run a local three-daemon fixture; requires a test-support binary")
    parser.add_argument("--fixture-launcher", default=str(Path(__file__).with_name("run-remote-fixture.sh")))
    args = parser.parse_args()
    if args.fixture_bin:
        local_fixture(args)
        return
    if not args.connection:
        parser.error("--connection is required unless --fixture-bin is supplied")
    client = DesktopMcp(args.desktop, args.socket)
    try:
        rows = client.call("session", action="list")
        remote = [row for row in rows if row.get("connection_id") == args.connection]
        if args.session:
            remote = [row for row in remote if args.session in (
                row.get("session_id"), row.get("alias"), row.get("tuic_session"))]
        if not remote:
            raise RuntimeError(f"Desktop MCP omitted remote PTYs for connection {args.connection}")
        row = remote[0]
        address = {"connection_id": args.connection, "session_id": row["session_id"]}
        output = client.call("session", action="output", limit=10, **address)
        if not isinstance(output.get("data"), str):
            raise RuntimeError(f"Remote output has no data: {output}")
        print(json.dumps({"step": "list/output", "session": row, "output": output}))
        peers = client.call("agent", action="list_peers", connection_id=args.connection)
        print(json.dumps({"step": "list_peers", "result": peers}))
        second = None
        if args.second_connection:
            second_rows = [item for item in rows if item.get("connection_id") == args.second_connection]
            if args.second_session:
                second_rows = [item for item in second_rows if args.second_session in (
                    item.get("session_id"), item.get("alias"), item.get("tuic_session"))]
            if not second_rows:
                raise RuntimeError(f"Desktop MCP omitted the second connection {args.second_connection}")
            second = second_rows[0]
            print(json.dumps({"step": "second-connection", "session": second}))
        if not args.exercise:
            return
        marker = "remote-mcp-" + uuid.uuid4().hex
        registration = client.call("agent", action="register", name=marker)
        sender = registration.get("address", "local/" + registration["tuic_session"])
        submitted = client.call("session", action="submit", input=(
            f"Print {marker}. This is a disposable MCP routing check."), **address)
        if not submitted.get("submitted"):
            raise RuntimeError(f"Remote submit rejected: {submitted}")
        print(json.dumps({"step": "submit", "result": submitted}))
        recipient = row.get("tuic_session") or row["session_id"]
        message = (f"Routing check {marker}: reply with agent action=send "
                   f"to={sender} message={marker}. Use mail, not terminal output.")
        if second:
            second_id = second.get("tuic_session") or second["session_id"]
            message = (f"Routing check {marker}: send mail with agent action=send "
                       f"to={args.second_connection}/{second_id}. Ask that peer to reply "
                       f"with agent action=send to={sender} message={marker}. "
                       "This checks remote-to-remote mail through the desktop hub.")
        sent = client.call("agent", action="send", connection_id=args.connection,
                           to=recipient, message=message)
        if not sent.get("delivered") or sent.get("delivery_path") == "inbox_only":
            raise RuntimeError(f"Remote mail failed to wake its recipient: {sent}")
        print(json.dumps({"step": "mail/wake", "result": sent}))
        reply = client.call("agent", action="wait", timeout_ms=60000)
        if not any(marker in message.get("content", "") for message in reply.get("messages", [])):
            raise RuntimeError(f"Remote reply did not reach the desktop sender: {reply}")
        if second and not any(message.get("from_tuic_session", "").startswith(args.second_connection + "/")
                              and marker in message.get("content", "") for message in reply.get("messages", [])):
            raise RuntimeError(f"Reply came from the wrong host; remote-to-remote check failed: {reply}")
        print(json.dumps({"step": "reply", "result": reply}))
    finally:
        client.close()


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, urllib.error.URLError, OSError, KeyError, ValueError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        sys.exit(1)
