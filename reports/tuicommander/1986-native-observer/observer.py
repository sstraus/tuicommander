#!/usr/bin/env python3
"""Reproducible r5 native observer for story 1986-1236.

Authorized linked-worktree macOS check only: at-1986/19877, own Unix socket.
The repeated-snapshot check catches eval log coalescing mistaken for a timeout.
Source the real bash integration before asserting idle; never manufacture SIGSTOP.
The input event is synthetic, not a physical keyboard/display timing benchmark.
"""
import base64, json, os, pathlib, runpy, subprocess, sys, time, traceback
ROOT = pathlib.Path(__file__).resolve().parents[3]
import argparse
parser = argparse.ArgumentParser(description='Authorized native parked-shell observer; never targets the daily instance.')
parser.add_argument('--evidence-dir', type=pathlib.Path, required=True)
args = parser.parse_args()
P = args.evidence_dir.resolve()
if not P.is_relative_to(pathlib.Path.home() / 'Gits'):
    raise ValueError('evidence must stay under Gits')
P.mkdir(parents=True, exist_ok=False)
helpers = runpy.run_path(str(ROOT / 'scripts/agent-test-visible.py'))
api0 = helpers['api']
evaluate = helpers['evaluate']
frontmost = helpers['frontmost']
variables = ['AGENT_TEST_INSTANCE=at-1986', 'AGENT_TEST_PORT=19877']
launch = json.loads(subprocess.check_output(['make', '--no-print-directory', 'agent-test', 'AGENT_TEST_DRY_RUN=1', *variables], cwd=ROOT, text=True))
(P / 'launch.json').write_text(json.dumps(launch, indent=2))
socket = launch['socket']

def api(path, body=None, method=None):
    return api0(socket, path, body, method)

def check_frontmost():
    observation = {"wall": time.time(), **frontmost()}
    with (P / "frontmost-observations.jsonl").open("a") as output:
        output.write(json.dumps(observation) + "\n")
    name = observation["name"]
    if name == "PlatformSSOUIAgent" or (
        name == "tuicommander-agent-test" and observation["pid"] != owned_pid
    ):
        raise RuntimeError("INTERFERENCE: " + json.dumps(observation))
    return observation


def js(script, name):
    check_frontmost()
    r = evaluate(socket, script, P)
    (P / (name + '.json')).write_text(json.dumps(r, indent=2))
    return r

def keys(command):
    return "const x=document.querySelector('input[aria-hidden=true]');if(!x)throw new Error('terminal input missing');for(const key of COMMAND){x.dispatchEvent(new KeyboardEvent('keydown',{key,bubbles:true,cancelable:true}));await new Promise(r=>setTimeout(r,8));}await new Promise(r=>setTimeout(r,100));x.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',code:'Enter',bubbles:true,cancelable:true}));return {sent:true,trusted:false};".replace('COMMAND', json.dumps(command))
child = None
sid = None
owned_pid = None
failure = None
cleanup = {}
try:
    if pathlib.Path.home().joinpath('Gits/.tmp/BUILD_FREEZE').exists():
        raise RuntimeError('BUILD_FREEZE present')
    if pathlib.Path(launch['pid_file']).exists():
        raise RuntimeError('instance already running')
    front = check_frontmost()
    (P / 'front-before.json').write_text(json.dumps(front, indent=2))
    if front['name'] == 'loginwindow':
        raise RuntimeError('Mac is locked; visible native measurement unavailable')
    log = (P / 'desktop.log').open('w')
    env = {**os.environ, 'PATH': str(pathlib.Path.home() / 'Library/Application Support/mbx/bin') + ':/opt/homebrew/bin:' + os.environ['PATH'], 'TMPDIR': str(pathlib.Path.home() / 'Gits/.tmp') + '/', 'RUST_LOG': 'info,tuicommander_lib::pty::lifecycle=trace'}
    child = subprocess.Popen(['make', 'agent-test', *variables], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
    deadline = time.monotonic() + 3600
    while time.monotonic() < deadline:
        if child.poll() is not None:
            raise RuntimeError('agent-test launch exited ' + str(child.returncode))
        if pathlib.Path(launch['pid_file']).exists() and pathlib.Path(socket).exists():
            owned_pid = int(pathlib.Path(launch['pid_file']).read_text())
            if subprocess.check_output(['ps', '-p', str(owned_pid), '-o', 'comm='], text=True).strip() != launch['binary']:
                raise RuntimeError('wrong binary')
            try:
                api('/health')
                break
            except subprocess.CalledProcessError:
                pass
        time.sleep(0.3)
    else:
        raise RuntimeError('setup deadline exceeded')
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        try:
            if js('return !!(window.__TUIC__&&document.querySelector(\'button[title^="New Tab"]\'));', 'frontend-ready'):
                break
        except Exception as error:
            if str(error).startswith('INTERFERENCE:'):
                raise
        time.sleep(0.2)
    else:
        raise RuntimeError('frontend not ready')
    first = js('return {probe:"same-native-snapshot"};', 'repeated-snapshot-first')
    second = js('return {probe:"same-native-snapshot"};', 'repeated-snapshot-second')
    assert first == second == {'probe': 'same-native-snapshot'}
    try:
        js('throw new Error("owned-eval-error-1986");', 'expected-error')
    except RuntimeError as error:
        if 'owned-eval-error-1986' not in str(error):
            raise
        (P / 'expected-eval-error.txt').write_text(str(error))
    else:
        raise RuntimeError('native evaluation error was lost')
    sessions = api('/sessions')
    (P / 'sessions-before.json').write_text(json.dumps(sessions, indent=2))
    if sessions:
        raise RuntimeError('requires empty instance')
    c = api('/config')
    b = dict(c)
    c['shell'] = '/bin/bash'
    c['standby_timeout_minutes'] = 1
    api('/config', {'base': b, 'config': c}, 'PUT')
    js('document.querySelector(\'button[title^="New Tab"]\').click();return {clicked:true};', 'new-tab')
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        sessions = api('/sessions')
        if len(sessions) == 1:
            sid = sessions[0]['session_id']
            break
        time.sleep(0.2)
    else:
        raise RuntimeError('no owned session')
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        ready = js('return {visibility:document.visibilityState,inputs:document.querySelectorAll("input[aria-hidden=true]").length,canvases:[...document.querySelectorAll("canvas")].map(c=>c.getBoundingClientRect().toJSON())};', 'canvas-precondition')
        if ready['visibility'] == 'visible' and ready['inputs'] and any((c['width'] > 0 and c['height'] > 0 for c in ready['canvases'])):
            break
        time.sleep(0.5)
    else:
        raise RuntimeError('visible canvas precondition missing')
    js(keys('source "$TUIC_SHELL_INTEGRATION"; /bin/sh -c \'printf "CHILD_%s\\n" EXITED1986\''), 'child-command')
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        output = api('/sessions/' + sid + '/output?limit=5000')
        if 'CHILD_EXITED1986' in output['data']:
            (P / 'child-output.json').write_text(json.dumps(output, indent=2))
            break
        time.sleep(0.2)
    else:
        raise RuntimeError('foreground child exit marker missing')
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        status = js('return window.__TUIC__.terminals();', 'idle-precondition')
        backend_state = api('/sessions/' + sid + '/shell-state')
        (P / 'backend-idle.json').write_text(json.dumps(backend_state))
        if backend_state['state'] == 'idle' and any((t['sessionId'] == sid and t['shellState'] == 'idle' for t in status)):
            break
        time.sleep(0.5)
    else:
        raise RuntimeError('shell did not become idle')
    (P / 'standby-config.json').write_text(json.dumps({'standby_timeout_minutes': api('/config')['standby_timeout_minutes']}))
    process_rows = subprocess.check_output(['ps', '-axo', 'pid=,ppid=,pgid=,state=,command='], text=True).splitlines()
    rows = [r for r in process_rows if len(r.split()) >= 5 and r.split()[1] == str(owned_pid) and (r.split()[4] == '/bin/bash')]
    if len(rows) != 1:
        raise RuntimeError('cannot uniquely identify own bash: ' + str(rows))
    shell_pid = int(rows[0].split()[0])
    (P / 'owned.json').write_text(json.dumps({'sid': sid, 'app_pid': owned_pid, 'shell_pid': shell_pid, 'ps': rows}, indent=2))
    js("const s=window.__desk1986={events:[],armed:false};const original=CanvasRenderingContext2D.prototype.fillText;s.original=original;\nCanvasRenderingContext2D.prototype.fillText=function(text,...args){if(s.armed&&String(text).includes('Ω')&&!s.events.some(e=>e.kind==='canvas-echo')){s.events.push({kind:'canvas-echo',time:performance.now(),text:String(text)});requestAnimationFrame(()=>s.events.push({kind:'next-animation-frame',time:performance.now()}));}return original.call(this,text,...args);};\nconst x=document.querySelector('input[aria-hidden=true]');x.addEventListener('keydown',e=>{if(s.armed)s.events.push({kind:'keydown',time:performance.now(),key:e.key,trusted:e.isTrusted});},{capture:true});\nawait window.__TAURI__.core.invoke('set_session_visible',{sessionId:SID,visible:false});return {visibility:document.visibilityState,focus:document.hasFocus(),sid:SID};".replace('SID', json.dumps(sid)), 'park-setup')
    observations = []
    deadline = time.monotonic() + 240
    while time.monotonic() < deadline:
        check_frontmost()
        ps = subprocess.check_output(['ps', '-p', str(shell_pid), '-o', 'pid=,ppid=,pgid=,state=,command='], text=True).strip()
        observations.append({'wall': time.time(), 'ps': ps})
        if 'T' in ps.split()[3]:
            break
        time.sleep(0.5)
    else:
        (P / 'park-observation.json').write_text(json.dumps(observations, indent=2))
        (P / 'standby-failure-state.json').write_text(json.dumps(api('/sessions'), indent=2))
        raise RuntimeError('actual standby not observed')
    (P / 'park-observation.json').write_text(json.dumps(observations, indent=2))
    prekey = js('return {visibility:document.visibilityState,windowVisible:await window.__TAURI__.window.getCurrentWindow().isVisible()};', 'before-key-window')
    (P / 'front-before-key.json').write_text(json.dumps(frontmost(), indent=2))
    if prekey['visibility'] != 'visible' or not prekey['windowVisible']:
        raise RuntimeError('visible window lost before key; no valid latency possible')
    result = js("const s=window.__desk1986;s.events=[];s.armed=true;const x=document.querySelector('input[aria-hidden=true]');s.events.push({kind:'dispatch-start',time:performance.now(),wall:Date.now()});const key=new KeyboardEvent('keydown',{key:'Ω',code:'KeyZ',bubbles:true,cancelable:true});x.dispatchEvent(key);\nawait new Promise(resolve=>{const start=performance.now();const check=()=>{if(s.events.some(e=>e.kind==='next-animation-frame')||performance.now()-start>10000)resolve();else setTimeout(check,5);};check();});s.armed=false;CanvasRenderingContext2D.prototype.fillText=s.original;\nreturn {url:location.href,visibility:document.visibilityState,focus:document.hasFocus(),events:s.events,defaultPrevented:key.defaultPrevented,canvases:[...document.querySelectorAll('canvas')].map(c=>({width:c.width,height:c.height,rect:c.getBoundingClientRect().toJSON(),png:c.toDataURL()}))};", 'native-key-latency-with-png')
    for i, canvas in enumerate(result.pop('canvases')):
        (P / f'native-canvas-{i}.png').write_bytes(base64.b64decode(canvas.pop('png').split(',')[1]))
    (P / 'native-key-latency.json').write_text(json.dumps(result, indent=2))
    after_ps = subprocess.check_output(['ps', '-p', str(shell_pid), '-o', 'pid,ppid,pgid,state,command'], text=True)
    (P / 'after-key-ps.txt').write_text(after_ps)
    (P / 'after-key-output.json').write_text(json.dumps(api('/sessions/' + sid + '/output?limit=5000'), indent=2))
    if not any((e['kind'] == 'canvas-echo' for e in result['events'])):
        raise RuntimeError('native key has no painted echo')
    if 'T' in after_ps.splitlines()[1].split()[3]:
        raise RuntimeError('shell remains stopped after native key')
    js("const x=document.querySelector('input[aria-hidden=true]');x.dispatchEvent(new KeyboardEvent('keydown',{key:'u',code:'KeyU',ctrlKey:true,bubbles:true,cancelable:true}));return {cleared:true};", 'clear-echo')
    js(keys('printf "NATIVE_%s\\n" EXECUTED1986'), 'execution-command')
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        output = api('/sessions/' + sid + '/output?limit=7000')
        if 'NATIVE_EXECUTED1986' in output['data']:
            (P / 'execution-output.json').write_text(json.dumps(output, indent=2))
            break
        time.sleep(0.1)
    else:
        raise RuntimeError('post-wake shell execution missing')
    (P / 'front-after.json').write_text(json.dumps(frontmost(), indent=2))
    print('PASS native synthetic keydown to rendered echo after actual standby', json.dumps(result))
except BaseException as e:
    failure = str(e)
    (P / 'failure.txt').write_text(traceback.format_exc())
    print(traceback.format_exc())
finally:
    if owned_pid:
        try:
            if sid:
                api('/sessions/' + sid, method='DELETE')
            cleanup['sessions_remaining'] = api('/sessions')
        except Exception as e:
            cleanup['session_cleanup_error'] = str(e)
        stop = subprocess.run(['make', 'agent-test-stop', *variables], cwd=ROOT, capture_output=True, text=True)
        cleanup.update(stop_exit=stop.returncode, stop_stdout=stop.stdout, stop_stderr=stop.stderr)
    if child:
        if owned_pid is None and child.poll() is None:
            child.terminate()
        cleanup['supervisor_exit'] = child.wait(timeout=30)
    rows = subprocess.check_output(['ps', '-axo', 'pid=,command='], text=True).splitlines()
    cleanup['worktree_processes_remaining'] = [r for r in rows if launch['binary'] in r]
    (P / 'cleanup.json').write_text(json.dumps(cleanup, indent=2))
    print('cleanup', json.dumps(cleanup))
    if cleanup.get('worktree_processes_remaining') or cleanup.get('session_cleanup_error'):
        failure = failure or 'cleanup failure'
sys.exit(1 if failure else 0)
