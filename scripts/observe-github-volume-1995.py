#!/usr/bin/env python3
"""Opt-in native macOS joined GitHub observation; see fixtures/github_volume/README.md."""
import hashlib, json, os, pathlib, re, shutil, subprocess, tempfile, time, urllib.request, uuid
root = pathlib.Path.cwd()
# Evidence must outlive the wrapper's per-run root (deleted on exit), so it lives under the host temp
# dir, never inside the checkout; it can hold account data.
base = pathlib.Path(os.environ.get('TUIC_TEST_HOST_TMPDIR') or tempfile.gettempdir()) / 'tuic-tests' / 'github-live-1995'
evidence = base / ('run-' + uuid.uuid4().hex[:8])
evidence.mkdir(parents=True)
(evidence / 'reap-scope').mkdir()
wrapper = os.environ.get('TUIC_BROWSER_WRAPPER') or shutil.which('ab-stealth.sh') or 'ab-stealth.sh'
browserenv = dict(os.environ, AB_SESSION='chore-1995-' + uuid.uuid4().hex[:8], AB_AGENT_BROWSER_DIR=str(evidence / 'reap-scope'))
browserenv['TUIC_1995_TRACE'] = str(base / 'frontend-trace.ndjson')
browserenv.pop('GH_TOKEN', None)
browserenv.pop('GITHUB_TOKEN', None)
token = subprocess.run(['gh', 'auth', 'token'], capture_output=True, text=True, check=True, timeout=30).stdout.strip()
assert token
backendenv = dict(os.environ, GH_TOKEN=token, TUIC_1995_EVIDENCE=str(evidence), TUIC_1995_SOURCE_REF=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(), TMPDIR=str(pathlib.Path(os.environ.get('TUIC_TEST_HOST_TMPDIR') or tempfile.gettempdir()) / 'tuic-tests'), TUIC_TEST_TMP_ROOT=str(base / 'test-root'))
(base / 'test-root').mkdir(parents=True, exist_ok=True)
backendenv.pop('GITHUB_TOKEN', None)
exe = None
for line in pathlib.Path(os.environ['TUIC_1995_BUILD_LOG']).read_text().splitlines():
    try:
        r = json.loads(line)
    except:
        continue
    if r.get('reason') == 'compiler-artifact' and r.get('executable') and r.get('profile', {}).get('test'):
        exe = r['executable']
assert exe
backend = server = None
daemon = None
result = {'evidence': str(evidence), 'samples': []}
started = time.monotonic()
source_files = ['src-tauri/src/github_live_measurement_1995.rs', 'src-tauri/src/github_poller.rs', 'scripts/observe-github-volume-1995.py', 'scripts/observe-github-volume-1995.vite.ts']
(evidence / 'source-sha256.json').write_text(json.dumps({name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in source_files}, indent=2) + '\n')
for name in source_files:
    (evidence / pathlib.Path(name).name).write_bytes((root / name).read_bytes())

def api(path, method='GET'):
    with urllib.request.urlopen(urllib.request.Request('http://127.0.0.1:14360' + path, method=method), timeout=15) as r:
        return r.read()
with (evidence / 'backend.log').open('w') as blog, (evidence / 'vite.log').open('w') as vlog, (evidence / 'transcript.txt').open('w') as transcript:

    def run(args):
        global daemon
        out = subprocess.run([wrapper] + args, env=browserenv, capture_output=True, text=True, timeout=180)
        transcript.write('$ wrapper ' + repr(args) + '\n' + out.stdout + out.stderr + '\nexit=' + str(out.returncode) + '\n')
        transcript.flush()
        m = re.search('Chrome=(\\d+)', out.stderr)
        if m:
            daemon = 'ab-shared-' + browserenv['AB_SESSION'] + '-' + m.group(1)
        if out.returncode:
            raise RuntimeError('browser command failed; see transcript: ' + repr(args))
        return out.stdout

    def status():
        return json.loads(api('/__1995/status'))

    def sample(phase, action=None):
        api('/__1995/phase/' + phase, 'POST')
        visibility_start = run(['--', 'eval', 'document.visibilityState']).strip()
        start = status()['elapsed_seconds']
        if action:
            action()
        time.sleep(float(os.environ.get('MEASUREMENT_SECONDS', '60')))
        stop = status()['elapsed_seconds']
        rows = [r for r in status()['trace'] if start <= r['elapsed_seconds'] < stop]
        counts = {kind: sum((r['kind'] == kind for r in rows)) for kind in ['REST', 'GraphQL']}
        item = {'phase': phase, 'visibility_start': visibility_start, 'start_seconds': start, 'end_seconds': stop, 'duration_seconds': stop - start, 'received': counts, 'requests_per_minute': {k: v * 60 / (stop - start) for (k, v) in counts.items()}, 'statuses': [r['status'] for r in rows]}
        result['samples'].append(item)
    try:
        backend = subprocess.Popen([exe, '--exact', 'github_poller::live_measurement_1995::observe_joined_live_github_1995', '--ignored', '--nocapture'], env=backendenv, stdout=blog, stderr=blog)
        token = None
        backendenv.pop('GH_TOKEN', None)
        for _ in range(600):
            if (evidence / 'ready.json').exists():
                break
            if backend.poll() is not None:
                raise RuntimeError('backend exited before ready')
            time.sleep(0.2)
        else:
            raise RuntimeError('backend readiness timeout')
        ready = json.loads((evidence / 'ready.json').read_text())
        subprocess.run(['git', '-C', ready['repo_path'], 'remote', 'set-url', 'origin', 'https://github.com/tauri-apps/tauri.git'], check=True, timeout=15)
        result['workload_remote'] = 'https://github.com/tauri-apps/tauri.git'
        (base / 'frontend-trace.ndjson').write_text('')
        server = subprocess.Popen(['node', 'node_modules/vite/bin/vite.js', '--config', str(root / 'scripts/observe-github-volume-1995.vite.ts')], env=browserenv, stdout=vlog, stderr=vlog)
        for _ in range(100):
            try:
                urllib.request.urlopen('http://127.0.0.1:14361/', timeout=1).close()
                break
            except OSError:
                time.sleep(0.2)
        run(['http://127.0.0.1:14361/'])
        run(['--', 'set', 'viewport', '1400', '900'])
        run(['--', 'wait', '[data-gh-panel-trigger]', '--timeout', '90000'])
        run(['--', 'snapshot'])
        run(['--', 'screenshot', str(evidence / 'idle.png')])
        sample('idle')

        def open_pr():
            run(['--', 'click', '[data-gh-panel-trigger]'])
            run(['--', 'eval', '(()=>{const row=document.querySelector("div[class*=ghItemRow]");if(!row)throw Error("No real PR row");row.setAttribute("data-1995-row","");return row.textContent})()'])
            run(['--', 'click', '[data-1995-row]'])
            run(['--', 'wait', '[data-compact]', '--timeout', '30000'])
            run(['--', 'snapshot'])
            run(['--', 'screenshot', str(evidence / 'pr-open.png')])
        sample('pr_open', open_pr)
        result['cold_trace'] = [r for r in status()['trace'] if r['phase'] == 'cold']
        result['trace'] = status()['trace']
        assert result['trace'] and all((r['status'] == 200 for r in result['trace'])), 'Non-success GitHub API response; inspect fixtures'
        assert any((r['phase'] == 'pr_open' and 'PRChecks' in r['query'] for r in result['trace'])), 'Opened panel did not exercise production CI query'
        for sample in result['samples']:
            rows = [row for row in result['trace'] if sample['start_seconds'] <= row['elapsed_seconds'] < sample['end_seconds']]
            sample['statuses'] = [row['status'] for row in rows]
        result['success'] = True
    except Exception as e:
        result['success'] = False
        result['error'] = str(e)
        try:
            run(['--', 'snapshot'])
            run(['--', 'screenshot', str(evidence / 'failure.png')])
        except Exception:
            pass
    finally:
        if daemon:
            try:
                run(['--', 'tab', 'close'])
            except Exception as e:
                transcript.write('tab cleanup: ' + str(e) + '\n')
            subprocess.run(['agent-browser', '--session', daemon, 'close'], env=browserenv, timeout=30, stdout=transcript, stderr=transcript)
        if server:
            server.terminate()
            try:
                server.wait(timeout=10)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait()
        if backend:
            try:
                sessions = json.loads(api('/sessions'))
                if isinstance(sessions, dict):
                    sessions = sessions.get('sessions', [])
                for session in sessions:
                    try:
                        api('/sessions/' + session['id'], 'DELETE')
                    except Exception:
                        pass
                api('/__1995/stop', 'POST')
                backend.wait(timeout=20)
            except Exception:
                backend.terminate()
                try:
                    backend.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    backend.kill()
                    backend.wait()
            result['backend_exit'] = backend.returncode
        result['total_seconds'] = time.monotonic() - started
        if (base / 'frontend-trace.ndjson').exists():
            (evidence / 'frontend-trace.ndjson').write_text((base / 'frontend-trace.ndjson').read_text())
        (evidence / 'summary.json').write_text(json.dumps(result, indent=2) + '\n')
        (base / 'latest.txt').write_text(str(evidence) + '\n')
print(json.dumps({k: v for (k, v) in result.items() if k not in ['trace', 'cold_trace']}, indent=2))
if not result.get('success') or result.get('backend_exit') != 0:
    raise SystemExit(1)
