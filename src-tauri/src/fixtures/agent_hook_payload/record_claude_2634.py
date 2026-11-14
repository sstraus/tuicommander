"""Record Claude Code question/plan tool hooks with TUIC's real hook commands.

Usage: python3 rec.py <ask|plan_yes|plan_no>
Writes under ~/Gits/.tmp/tuic-2634/cap/<mode>/: hooks.jsonl (native stdin),
capture.tcap (TUICCAP2: real output chunk boundaries plus input), log.json.
"""
import fcntl, json, os, pathlib, pty, select, struct, subprocess, sys, termios, time

mode = sys.argv[1]
root = pathlib.Path.home() / 'Gits/.tmp/tuic-2634/cap' / mode
root.mkdir(parents=True, exist_ok=True)
work = root / 'scratch'
work.mkdir(exist_ok=True)
log_file = root / 'hooks.jsonl'
if log_file.exists():
    log_file.unlink()

SENTINEL = '# tuic-managed-hook'
TTY = r'''__t=$(ps -o tty= -p "$PPID" 2>/dev/null|tr -d '[:space:]');case "$__t" in *[0-9]*)__t="/dev/${__t#/dev/}";;*)__t="/dev/tty";;esac'''


def hook_command(state):
    # agent_hook.rs::hook_command
    return ('[ -n "${TUIC_SESSION:-}" ] && { ' + TTY + "; printf '\\033]7770;state=" + state
            + "\\033\\\\' > \"$__t\"; } >/dev/null 2>&1 || true " + SENTINEL)


def claude_hook_command(state):
    # agent_hook.rs::claude_hook_command
    coarse = ('\\033]7770;state=' + state + '\\033\\\\') if state else ''
    return ('[ -n "${TUIC_SESSION:-}" ] && { ' + TTY
            + "; __p=$(base64 | tr -d '\\r\\n'); [ ${#__p} -le 32768 ] && for __c in $(printf %s \"$__p\" | fold -w 768); do printf '\\033]7770;claude-hook=%s:%s:%s\\033\\\\' \"$TUIC_SESSION\" \"$$\" \"$__c\" >> \"$__t\"; done; printf '\\033]7770;claude-hook=%s:%s:\\033\\\\"
            + coarse + "' \"$TUIC_SESSION\" \"$$\" >> \"$__t\"; } >/dev/null 2>&1 || true " + SENTINEL)


# agent_hook.rs::claude_hook_map, in order.
tuic = [
    ('UserPromptSubmit', '', claude_hook_command('prompt')),
    ('PreToolUse', '', claude_hook_command('busy')),
    ('PreToolUse', 'AskUserQuestion|ExitPlanMode', hook_command('awaiting')),
    ('PostToolUse', 'AskUserQuestion|ExitPlanMode', hook_command('busy')),
    ('PermissionRequest', '', claude_hook_command(None)),
    ('Notification', 'permission_prompt', claude_hook_command(None)),
    ('PostToolUse', '', claude_hook_command(None)),
    ('PostToolUseFailure', '', claude_hook_command(None)),
    ('Elicitation', '', claude_hook_command('awaiting')),
    ('ElicitationResult', '', claude_hook_command('busy')),
    ('Stop', '', hook_command('idle')),
    ('StopFailure', '', hook_command('idle')),
    ('SessionEnd', '', hook_command('idle')),
]
hooks = {}
for event, matcher, command in tuic:
    hooks.setdefault(event, []).append({'matcher': matcher, 'hooks': [{'type': 'command', 'command': command}]})
logger = f'cat >> {log_file}; printf "\\n" >> {log_file}'
for event in ['SessionStart', 'UserPromptSubmit', 'PreToolUse', 'PostToolUse', 'PostToolUseFailure',
              'PermissionRequest', 'Notification', 'Stop', 'SessionEnd']:
    hooks.setdefault(event, []).append({'matcher': '', 'hooks': [{'type': 'command', 'command': logger}]})
dbg = 'echo "ppid=$PPID tty=$(ps -o tty= -p $PPID) comm=$(ps -o comm= -p $PPID) sess=${TUIC_SESSION:-none}" >> ' + str(root / 'dbg.txt')
hooks['PreToolUse'].append({'matcher': '', 'hooks': [{'type': 'command', 'command': dbg}]})
settings = {'hooks': hooks, 'permissions': {'defaultMode': 'plan' if mode.startswith('plan') else 'default'}}
(root / 'settings.json').write_text(json.dumps(settings))

ROWS, COLS = 40, 120
m, s = pty.openpty()
fcntl.ioctl(s, termios.TIOCSWINSZ, struct.pack('HHHH', ROWS, COLS, 0, 0))
env = dict(os.environ)
for k in list(env):
    if k.startswith('CLAUDE_CODE_') or k in ('CLAUDECODE',):
        env.pop(k)
env['TUIC_SESSION'] = 'rec-2634-owner'

if mode == 'ask':
    sysp = ('When the user asks, call the AskUserQuestion tool exactly once with exactly two questions: '
            '"Which color should the probe use?" (header Color, options Red and Blue, single choice) and '
            '"Which sizes should the probe use?" (header Sizes, options Small and Large, multiSelect true). '
            'After the answers arrive, reply with the single word DONE. Never call any other tool.')
    prompt, tools = 'Ask me the probe questions.', 'AskUserQuestion'
else:
    sysp = ('You are in plan mode. When the user asks, call the ExitPlanMode tool exactly once with this plan, verbatim:\n'
            '"# Probe plan\\n\\n1. Write probe.txt\\n2. Read probe.txt back". '
            'After the tool result arrives, reply with the single word DONE. Never call any other tool.')
    prompt, tools = 'Propose the probe plan.', 'ExitPlanMode'

argv = ['claude', '--setting-sources', '', '--settings', str(root / 'settings.json'), '--strict-mcp-config',
        '--mcp-config', '{"mcpServers":{}}', '--model', 'sonnet', '--effort', 'low', '--tools', tools,
        '--system-prompt', sysp]
if mode.startswith('plan'):
    argv += ['--permission-mode', 'plan']
argv.append(prompt)
p = subprocess.Popen(argv, cwd=work, env=env, stdin=s, stdout=s, stderr=s,
                     preexec_fn=lambda: (os.setsid(), fcntl.ioctl(0, termios.TIOCSCTTY, 0)))
os.close(s)

start = time.monotonic()
records = []  # (direction, elapsed_us, bytes)
raw = b''
log = []


def pump(sec):
    global raw
    end = time.monotonic() + sec
    while time.monotonic() < end and p.poll() is None:
        if select.select([m], [], [], 0.1)[0]:
            try:
                b = os.read(m, 65536)
            except OSError:
                return
            raw += b
            records.append((0, int((time.monotonic() - start) * 1e6), b))


def wait_for(token, count, limit):
    end = time.monotonic() + limit
    while time.monotonic() < end and p.poll() is None and raw.count(token) < count:
        pump(0.2)
    return raw.count(token) >= count


def send(b, label):
    os.write(m, b)
    records.append((1, int((time.monotonic() - start) * 1e6), b))
    log.append((round(time.monotonic() - start, 2), label))
    pump(1.5)


try:
    ok = wait_for(b'7770;state=awaiting', 1, 150)
    log.append(('awaiting', ok))
    pump(2.0)
    if mode == 'ask':
        send(b'\r', 'enter: first option of question 1')
        send(b'\r', 'enter: toggle Small on question 2')
        for _ in range(3):
            send(b'\x1b[B', 'down')
        send(b'\r', 'enter on Submit row of question 2')
        send(b'\r', 'enter on Submit answers review')
        ok = wait_for(b'DONE', 1, 90)
        log.append(('done', ok))
    elif mode == 'plan_yes':
        send(b'\r', 'enter: approve plan (highlighted option)')
        ok = wait_for(b'DONE', 1, 90)
        log.append(('done', ok))
    else:
        send(b'\x1b', 'escape: reject plan')
        pump(10)
    pump(3)
except BaseException:
    import traceback
    (root / 'error.txt').write_text(traceback.format_exc())
    raise
finally:
    out = bytearray(b'TUICCAP2\n')
    out += struct.pack('<HH', ROWS, COLS)
    for direction, us, data in records:
        out += struct.pack('<BQI', direction, us, len(data)) + data
    (root / 'capture.tcap').write_bytes(bytes(out))
    (root / 'log.json').write_text(json.dumps({'argv_tools': tools, 'mode': mode, 'log': log,
                                                'rows': ROWS, 'cols': COLS}))
    import signal
    try:
        os.killpg(p.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    for _ in range(50):
        if p.poll() is not None:
            break
        time.sleep(0.1)
    log.append(('exit', p.returncode))
print(mode, log, 'records', len(records), 'awaiting frames', raw.count(b'state=awaiting'), 'hook frames', raw.count(b'claude-hook='))
