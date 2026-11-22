"""Validate and execute developer-authored scenarios against the real HTTP API."""
import math
from pathlib import Path
import re
import time

AGENTS = {
    "claude": "claude", "codex": "codex", "grok": "grok", "opencode": "opencode",
    "ego": "ego", "agy": "agy", "pi": "pi", "goose": "goose",
    "amp": "amp", "cursor": "cursor-agent", "droid": "droid",
    "qwen": "qwen", "crush": "crush", "copilot": "copilot", "kiro": "kiro-cli",
}
KEYS = {"enter": "\r", "escape": "\x1b", "up": "\x1b[A", "down": "\x1b[B",
        "right": "\x1b[C", "left": "\x1b[D"}
POLL_SECS = 0.5
# The buffer tail a check reads; a short reply plus TUI chrome fits easily.
TAIL_LINES = 200


def expand(value, values):
    """Substitute {name} placeholders in args (list) or env (dict) values."""
    def one(text):
        def sub(match):
            if match.group(1) not in values:
                raise ValueError(f"unknown placeholder: {match.group(0)}")
            return values[match.group(1)]
        return re.sub(r"\{([a-z_]+)\}", sub, text)
    if isinstance(value, dict):
        return {key: one(text) for key, text in value.items()}
    return [one(text) for text in value]


def passthrough_env(scenario, environ):
    """Credentials the scenario names (env_passthrough) taken from the driver's own environment.

    An entry is NAME or TARGET=SOURCE (the agent sees TARGET with the value of SOURCE)."""
    pairs = (entry.partition("=")[::2] for entry in scenario.get("env_passthrough", []))
    return {target: environ[source or target] for target, source in pairs if (source or target) in environ}


def _cwd_files(files, allow_delete):
    ok = isinstance(files, dict) and files and all(
        isinstance(name, str) and name and not name.startswith("/") and ".." not in name.split("/")
        and (isinstance(text, str) or (allow_delete and text is None))
        for name, text in files.items())
    if not ok:
        raise ValueError("cwd_files must map relative paths to text" + (" or null" if allow_delete else ""))


def write_cwd_files(directory, files, values):
    """Create (text) or delete (None) instruction files in the agent's working directory."""
    for name, text in files.items():
        path = Path(directory) / name
        if text is None:
            path.unlink(missing_ok=True)
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(expand([text], values)[0] + "\n")


def _echoed(prompt):
    """An agent's history may echo "What is 10+7?" as "What is 10+7": match without the trailing question mark."""
    return prompt.rstrip("?")


def judge_reply(lines, prompt, answer, canary, ignore_prefixes=()):
    """Judge the reply below the latest echo of prompt; None until it shows the answer.

    Lines starting with one of ignore_prefixes (an agent's reasoning panel) are not reply."""
    echo = max((i for i, line in enumerate(lines) if _echoed(prompt) in line), default=None)
    if echo is None:
        return None
    region = [line for line in lines[echo + 1:]
              if not line.lstrip().startswith(tuple(ignore_prefixes))]
    if not any(answer in line for line in region):
        return None
    # Agents prefix replies with a bullet glyph; the rule is about the text start.
    # Tool calls (for example a progress tool quoting the result) may precede the
    # reply, so a canary-led line anywhere in the turn wins; otherwise quote the
    # last line with the answer, which is the reply rather than a tool argument.
    led = [line for line in region if re.sub(r"^\W+", "", line).startswith(canary)]
    reply = led[0] if led else [line for line in region if answer in line][-1]
    return {"followed": bool(led), "reply_line": reply.strip()}


def _turn(lines, prompt, limit=80):
    echo = max((i for i, line in enumerate(lines) if _echoed(prompt) in line), default=max(0, len(lines) - 15))
    return [line for line in lines[echo:] if line.strip()][:limit]


def _seconds(step, key, default, ceiling):
    seconds = step.get(key, default)
    if (isinstance(seconds, bool) or not isinstance(seconds, (int, float))
            or not math.isfinite(seconds) or not 0 < seconds <= ceiling):
        raise ValueError(f"{step.get('action')} {key} must be in (0, {ceiling}]")
    return seconds


def _text(step, key):
    if not isinstance(step.get(key), str) or not step[key].strip():
        raise ValueError(f"{step.get('action')} requires {key}")
    return step[key]


def validate(scenario):
    if not isinstance(scenario, dict) or scenario.get("agent") not in AGENTS:
        raise ValueError("scenario.agent must name a supported capture agent")
    if not isinstance(scenario.get("args"), list) or not all(
        isinstance(arg, str) for arg in scenario["args"]
    ):
        raise ValueError("scenario.args must contain explicit CLI arguments")
    steps = scenario.get("steps")
    if not isinstance(steps, list) or not steps:
        raise ValueError("scenario.steps must be nonempty")
    expected, labels, prompts = [], set(), set()
    for step in steps:
        if not isinstance(step, dict):
            raise ValueError("each scenario step must be an object")
        action = step.get("action")
        if action in ("prompt", "question", "approval"):
            if action != "prompt" and "key" in step:
                if step["key"] not in KEYS:
                    raise ValueError("dialog key must be enter, escape or an arrow")
            elif not isinstance(step.get("text"), str) or not step["text"].strip():
                raise ValueError(f"{action} requires text or a dialog key")
        elif action == "wait":
            seconds = step.get("timeout_secs", 60)
            if (isinstance(seconds, bool) or not isinstance(seconds, (int, float))
                    or not math.isfinite(seconds) or not 0 < seconds <= 600):
                raise ValueError("wait timeout_secs must be in (0, 600]")
            state = step.get("expect")
            if state is None and isinstance(step.get("text"), str) and step["text"].strip():
                continue  # Startup/dialog readiness is observable grid text, not a detector state.
            if not isinstance(state, dict) or not state or set(state) - {"agent", "awaiting"}:
                raise ValueError("wait.expect requires agent and/or awaiting")
            if "agent" in state and state["agent"] not in ("idle", "working", "awaiting_input", "completed"):
                raise ValueError("expected agent must be idle, working, awaiting_input or completed")
            if "awaiting" in state and not isinstance(state["awaiting"], bool):
                raise ValueError("expected awaiting must be boolean")
            replay = step.get("replay_expect", state)
            if (not isinstance(replay, dict) or not replay
                    or any(key not in state or state[key] != value for key, value in replay.items())):
                raise ValueError("replay_expect must be a nonempty subset of wait.expect")
            expected.append(replay)
        elif action in ("slash", "settle"):
            if action == "slash" and not _text(step, "text").startswith("/"):
                raise ValueError("slash text must start with /")
            _seconds(step, "quiet_secs", 5, 120)
            _seconds(step, "timeout_secs", 180, 600)
        elif action == "check":
            label, text, answer = _text(step, "label"), _text(step, "text"), _text(step, "answer")
            if answer in text:
                raise ValueError("check answer must not occur in its prompt (the echo would pass)")
            if label in labels or text in prompts:
                raise ValueError("check labels and prompts must be unique (replayed history would pass)")
            labels.add(label)
            prompts.add(text)
            _seconds(step, "quiet_secs", 3, 120)
            _seconds(step, "timeout_secs", 180, 600)
        elif action == "dismiss":
            _text(step, "text")
            if step.get("key") not in KEYS:
                raise ValueError("dismiss key must be enter, escape or an arrow")
            _seconds(step, "timeout_secs", 30, 600)
        elif action == "restart":
            if not isinstance(step.get("args"), list) or not all(isinstance(a, str) for a in step["args"]):
                raise ValueError("restart requires explicit args")
            if "exit_text" in step:
                _text(step, "exit_text")
            if "cwd_files" in step:
                _cwd_files(step["cwd_files"], True)
            if "env" in step and not (isinstance(step["env"], dict) and all(
                    isinstance(v, str) for v in step["env"].values())):
                raise ValueError("restart env must map names to strings")
        elif action != "interrupt":
            raise ValueError(f"unknown scenario action: {action}")
    if not isinstance(scenario.get("idle_gate", False), bool):
        raise ValueError("idle_gate must be boolean")
    if "cwd_files" in scenario:
        _cwd_files(scenario["cwd_files"], False)
    passthrough = scenario.get("env_passthrough", [])
    if not isinstance(passthrough, list) or not all(
            isinstance(n, str) and re.fullmatch(r"[A-Z_][A-Z0-9_]*(=[A-Z_][A-Z0-9_]*)?", n) for n in passthrough):
        raise ValueError("env_passthrough must list environment variable names")
    ignore = scenario.get("ignore_prefixes", [])
    if not isinstance(ignore, list) or not all(isinstance(p, str) and p.strip() for p in ignore):
        raise ValueError("ignore_prefixes must list nonempty strings")
    if labels:
        canary = scenario.get("canary")
        if not isinstance(canary, str) or not canary.strip() or canary not in str(scenario.get("canary_rule")):
            raise ValueError("checks require a canary that occurs in canary_rule")
    elif not expected:
        raise ValueError("scenario requires at least one expected state")
    return expected


def _tail(api, session_id):
    total = api("GET", f"/sessions/{session_id}/terminal/scroll-info")["total_lines"]
    start = max(0, total - TAIL_LINES)
    return api("GET", f"/sessions/{session_id}/terminal/lines?start={start}&end={total}")["lines"]


def _settle(api, session_id, quiet_secs, timeout_secs, done=lambda lines: True):
    """Poll the buffer tail until done(lines) holds and it stopped changing for quiet_secs."""
    deadline = time.monotonic() + timeout_secs
    needed = max(1, math.ceil(quiet_secs / POLL_SECS))
    previous, still = None, 0
    while True:
        lines = _tail(api, session_id)
        still = still + 1 if lines == previous else 0
        previous = lines
        if still >= needed and done(lines):
            return lines, True
        if time.monotonic() >= deadline:
            return lines, False
        time.sleep(POLL_SECS)


def _submit_when_free(api, session_id, text, timeout_secs):
    """Submit, retrying while TUIC answers 409 (the agent is still busy or in a dialog)."""
    deadline = time.monotonic() + timeout_secs
    while True:
        try:
            return api("POST", f"/sessions/{session_id}/submit", {"input": text})
        except RuntimeError as error:
            if "HTTP 409" not in str(error) or time.monotonic() >= deadline:
                raise
            time.sleep(2)


def exit_session(api, session_id, exit_text):
    """Ask the agent to quit and wait until its session is gone (it may hold a store lease)."""
    api("POST", f"/sessions/{session_id}/submit", {"input": exit_text})
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline and any(
            row["session_id"] == session_id for row in api("GET", "/sessions")):
        time.sleep(POLL_SECS)


def _working(api, session_id):
    row = next((row for row in api("GET", "/sessions") if row["session_id"] == session_id), None)
    return row is not None and (row.get("state") or {}).get("agent_state") == "working"


def run_steps(api, session_id, steps, canary=None, relaunch=None, results=None, ignore_prefixes=(),
              idle_gate=False):
    """Run steps; check verdicts go to results, restart calls relaunch(old_id, args, env, exit_text, cwd_files)."""
    observed = []
    for step in steps:
        action = step["action"]
        if action in ("slash", "settle"):
            if action == "slash":
                _submit_when_free(api, session_id, step["text"], step.get("timeout_secs", 180))
            _settle(api, session_id, step.get("quiet_secs", 5), step.get("timeout_secs", 180))
        elif action == "check":
            result = _submit_when_free(api, session_id, step["text"], step.get("timeout_secs", 180))
            if not result.get("submitted"):
                raise RuntimeError("agent rejected scenario submission")
            judge = lambda lines: judge_reply(lines, step["text"], step["answer"], canary, ignore_prefixes)
            # idle_gate: an agent that queues input and interleaves tool calls is only
            # done when TUIC no longer reports it working, not when the screen pauses.
            done = lambda lines: judge(lines) is not None and not (idle_gate and _working(api, session_id))
            lines, settled = _settle(api, session_id, step.get("quiet_secs", 3),
                                     step.get("timeout_secs", 180), done)
            verdict = judge(lines) if settled else None
            results.append({"label": step["label"], "prompt": step["text"],
                            **(verdict or {"followed": None, "reply_line": None,
                                           "reason": "no reply with the answer observed"}),
                            # The whole turn from its echo, so a verdict can be re-judged offline.
                            "tail": _turn(lines, step["text"])})
        elif action == "dismiss":
            # A dialog some launches show and others do not: answer it only if it appears.
            appeared = lambda lines: step["text"] in "\n".join(lines)
            if _settle(api, session_id, POLL_SECS, step.get("timeout_secs", 30), appeared)[1]:
                time.sleep(0.5)
                api("POST", f"/sessions/{session_id}/write", {"data": KEYS[step["key"]]})
        elif action == "restart":
            # env replaces the scenario env for this launch (e.g. resume without injection).
            session_id = relaunch(session_id, step["args"], step.get("env"), step.get("exit_text"),
                                 step.get("cwd_files"))
        elif action == "interrupt":
            api("POST", f"/sessions/{session_id}/write", {"data": "\x03"})
        elif action == "wait":
            deadline = time.monotonic() + step.get("timeout_secs", 60)
            while True:
                if "expect" not in step:
                    # Rows are absolute (0 = oldest); after scrollback the screen is the tail.
                    if step["text"] in "\n".join(_tail(api, session_id)):
                        break
                    if time.monotonic() >= deadline:
                        raise RuntimeError("expected screen text was not reached")
                    time.sleep(0.1)
                    continue
                rows = api("GET", "/sessions")
                row = next((row for row in rows if row["session_id"] == session_id), None)
                if row is None:
                    raise RuntimeError("agent exited before the expected state")
                state = row.get("state") or {}
                snapshot = {"agent": state.get("agent_state"),
                            "awaiting": state.get("awaiting_input", False)}
                if all(snapshot.get(key) == value for key, value in step["expect"].items()):
                    observed.append(snapshot)
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError(f"expected state was not reached: {step['expect']}; last snapshot: {snapshot}")
                time.sleep(0.1)
        elif "key" in step:
            # Initial Ink dialogs can paint before their input handler attaches.
            # A single dialog key is not a text+Enter command submission.
            time.sleep(0.5)
            api("POST", f"/sessions/{session_id}/write", {"data": KEYS[step["key"]]})
        else:
            # Managed submit owns agent-specific split Enter writes; never append CR here.
            result = api("POST", f"/sessions/{session_id}/submit", {"input": step["text"]})
            if not result.get("submitted"):
                raise RuntimeError("agent rejected scenario submission")
    return observed
