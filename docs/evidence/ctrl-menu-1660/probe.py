"""Owned PTY key probe. Run with OUTPUT_ROOT [AGENT ...]; never uses real credentials.

Every process gets a fresh HOME/XDG/config root, dummy keys and a local 401
endpoint. ANSI and HTTP request bodies are retained for manual classification.
A request after the candidate proves submission; otherwise a suffix + CR lets
its request body distinguish a newline from an ignored key. Not a TUIC transport
integration test. No agent tools can run: the endpoint never returns a model turn.
"""
import fcntl
import http.server
import json
import os
from pathlib import Path
import pty
import select
import shutil
import signal
import struct
import subprocess
import sys
import termios
import threading
import time
import urllib.request
import shlex

ROOT = Path(sys.argv[1]).resolve()
ROOT.mkdir(parents=True, exist_ok=True)
BINS = {a: shutil.which(b) for a, b in {
    'claude': 'claude', 'codex': 'codex', 'opencode': 'opencode',
    'goose': 'goose', 'grok': 'grok', 'pi': 'pi', 'ego': 'ego',
}.items()}
SEQUENCES = {'csi-u': b'\x1b[13;5u', 'modify-other-keys': b'\x1b[27;5;13~', 'lf': b'\n', 'cr': b'\r'}
requests = []

class Handler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get('Content-Length', 0)))
        requests.append({'path': self.path, 'body': json.loads(body)})
        (ROOT/'latest-requests.json').write_text(json.dumps(requests,indent=2))
        if self.path.endswith('/api/chat') and not json.loads(body).get('messages'):
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.end_headers()
            self.wfile.write(b'{"model":"qwen3","message":{"role":"assistant","content":""},"done":true}')
            return
        self.send_response(401)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(b'{"error":{"type":"authentication_error","message":"Owned local key probe"}}')

    def do_GET(self):
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(b'{"models":[{"name":"qwen3","model":"qwen3","context_length":32768,"size":1,"digest":"probe","details":{"family":"qwen3","parameter_size":"1B","quantization_level":"Q4"}}],"data":[{"id":"probe","object":"model"}]}')

    def log_message(self, *_args):
        pass

server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
threading.Thread(target=server.serve_forever, daemon=True).start()
BASE_URL = f'http://127.0.0.1:{server.server_port}'
URL = BASE_URL

def configuration(agent, root):
    env = {'PATH': os.environ['PATH'], 'TERM': 'xterm-256color', 'LANG': 'en_US.UTF-8', 'TMPDIR': str(root)}
    for key, name in [('HOME','home'), ('XDG_CONFIG_HOME','config'), ('XDG_DATA_HOME','data'), ('XDG_CACHE_HOME','cache')]:
        path = root / name
        path.mkdir(parents=True, exist_ok=True)
        env[key] = str(path)
    cfg = root / 'agent'
    cfg.mkdir(exist_ok=True)
    binary = BINS[agent]
    if agent == 'claude':
        env.update(CLAUDE_CONFIG_DIR=str(cfg), ANTHROPIC_API_KEY='local-probe-only', ANTHROPIC_BASE_URL=URL, DISABLE_AUTOUPDATER='1', CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC='1')
        (cfg / '.claude.json').write_text(json.dumps({'hasCompletedOnboarding': True, 'customApiKeyResponses': {'approved':['local-probe-only'], 'rejected':[]}}))
        args = [binary, '--bare', '--model', 'claude-sonnet-4-6', '--tools', '']
    elif agent == 'codex':
        env.update(CODEX_HOME=str(cfg), PROBE_KEY='local-probe-only')
        (cfg/'config.toml').write_text('[tui]\nscreen_reader_detection_done = true\n')
        args = [binary, '--no-alt-screen', '-C', str(root)]
        for setting in ['check_for_update_on_startup=false', 'model="probe"', 'model_provider="probe"', 'model_providers.probe.name="Local probe"', f'model_providers.probe.base_url="{URL}/v1"', 'model_providers.probe.env_key="PROBE_KEY"', 'model_providers.probe.wire_api="responses"', 'model_providers.probe.request_max_retries=0', 'model_providers.probe.stream_max_retries=0', f'projects."{root}".trust_level="trusted"']:
            args.extend(['-c', setting])
    elif agent == 'opencode':
        env.update(OPENCODE_DISABLE_AUTOUPDATE='true', OPENCODE_DISABLE_MODELS_FETCH='true', OPENCODE_CONFIG_CONTENT=json.dumps({'autoupdate':False,'model':'probe/probe','provider':{'probe':{'npm':'@ai-sdk/openai-compatible','name':'Probe','options':{'baseURL':URL+'/v1','apiKey':'local-probe-only'},'models':{'probe':{'name':'probe','limit':{'context':32000,'output':1000}}}}}}))
        args = [binary, '--pure', str(root)]
    elif agent == 'goose':
        env.update(GOOSE_TELEMETRY_ENABLED='false', GOOSE_TELEMETRY_OFF='1', GOOSE_PATH_ROOT=str(cfg), GOOSE_PROVIDER='openai', GOOSE_MODEL='gpt-4o', OPENAI_API_KEY='local-probe-only', OPENAI_HOST=URL, OPENAI_BASE_PATH='v1/chat/completions', GOOSE_DISABLE_KEYRING='true')
        args = [binary, 'session', '--no-profile', '--provider', 'openai', '--model', 'gpt-4o']
    elif agent == 'grok':
        env.update(GROK_HOME=str(cfg), PROBE_KEY='local-probe-only', XAI_API_KEY='local-probe-only', GROK_AGENT_DASHBOARD='0')
        (cfg/'config.toml').write_text(f'[cli]\nauto_update = false\n[features]\ntelemetry = false\n[models]\ndefault = "probe"\n[model.probe]\nmodel = "probe"\nname = "Probe"\nbase_url = "{URL}/v1"\nenv_key = "PROBE_KEY"\n')
        args = [binary, '--model', 'probe', '--tools', '', '--cwd', str(root)]
    elif agent == 'pi':
        env.update(PI_CODING_AGENT_DIR=str(cfg), PI_OFFLINE='1')
        (cfg/'models.json').write_text(json.dumps({'providers':{'probe':{'baseUrl':URL+'/v1','api':'openai-completions','apiKey':'local-probe-only','models':[{'id':'probe','contextWindow':32000,'maxTokens':1000}]}}}))
        args = [binary, '--offline', '--no-session', '--no-extensions', '--no-skills', '--no-prompt-templates', '--no-context-files', '--no-tools', '--provider', 'probe', '--model', 'probe']
    else:
        env.update(EGO_HOME=str(cfg), OLLAMA_HOST=URL)
        args = [binary, '--sandbox', 'off', '--thinking', 'off', '--model', 'ollama/qwen3']
    return args, env

HTTP = os.environ.get('TUIC_PROBE_HTTP')
def request_http(method, path, body=None):
    request = urllib.request.Request(HTTP+path+('&' if '?' in path else '?')+'token=ctrl-menu-owned-probe', data=json.dumps(body).encode() if body is not None else None, method=method, headers={'Content-Type':'application/json'})
    with urllib.request.urlopen(request, timeout=10) as response: return json.load(response)

results = []
for agent in sys.argv[2:] or BINS:
    if not BINS[agent]:
        continue
    for name, sequence in SEQUENCES.items():
        root = ROOT / agent / name
        root.mkdir(parents=True, exist_ok=True)
        if agent == 'ego':
            # ego resolves /v1 against the origin, dropping a URL path prefix.
            # Give each case its own listening port instead.
            server.shutdown()
            server.server_close()
            server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
            threading.Thread(target=server.serve_forever, daemon=True).start()
            BASE_URL = f'http://127.0.0.1:{server.server_port}'
            requests.clear()
        case_path = '/' if agent == 'ego' else f'/probe/{agent}/{name}/'
        URL = BASE_URL + case_path.rstrip('/')
        args, env = configuration(agent, root)
        version = subprocess.run([BINS[agent], '--version'], env=env, capture_output=True, timeout=10).stdout.decode().strip()
        if HTTP:
            launcher = root/'launch.sh'
            launcher.write_text('#!/bin/sh\nexec env -i '+ ' '.join(shlex.quote(k+'='+v) for k,v in env.items())+' '+shlex.join(args)+'\n')
            launcher.chmod(0o700)
            session = request_http('POST','/sessions',{'shell':str(launcher),'cwd':str(root),'rows':32,'cols':120})
            session_id = session['session_id']
            proc = None
            master = slave = None
        else:
            master, slave = pty.openpty()
        if not HTTP:
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 32, 120, 0, 0))
            proc = subprocess.Popen(args, stdin=slave, stdout=slave, stderr=slave, cwd=root, env=env, start_new_session=True)
            os.close(slave)
        output = bytearray()
        def case_requests(): return [r for r in requests if r['path'].startswith(case_path)]
        start = len(case_requests())
        def write(data):
            if HTTP: request_http('POST',f'/sessions/{session_id}/write',{'data':data.decode()})
            else: os.write(master,data)
        def read(seconds):
            end = time.monotonic() + seconds
            while time.monotonic() < end:
                if HTTP:
                    time.sleep(.05)
                    output[:] = request_http('GET',f'/sessions/{session_id}/output?limit=1000000')['data'].encode()
                    continue
                if select.select([master], [], [], .05)[0]:
                    try:
                        data = os.read(master, 65536)
                    except OSError:
                        break
                    output.extend(data)
                    for query, reply in [(b'\x1b[6n',b'\x1b[1;1R'),(b'\x1b[?u',b'\x1b[?5u'),(b'\x1b[c',b'\x1b[?1;2c')]:
                        if query in data:
                            os.write(master, reply)
        try:
            read(3)
            if agent == 'claude':
                if b'Accessing' in output:
                    write(b'\x1b[B'); read(.1); write(b'\r'); read(1)
                if b'External' in output:
                    write(b'\r'); read(1)
            (root/'boot.ansi').write_bytes(output)
            write(b'first-probe'); read(.4)
            before = len(case_requests())
            write(sequence); read(1.2)
            after = len(case_requests())
            (root/'candidate.ansi').write_bytes(output)
            write(b'second-probe'); read(.4)
            write(b'\r'); read(1.2)
            (root/'final.ansi').write_bytes(output)
            captured = case_requests()[start:]
            (root/'requests.json').write_text(json.dumps(captured,indent=2))
            result={'agent':agent,'version':version,'sequence':name,'bytes':sequence.hex(),'requestsBefore':before-start,'requestsAfterCandidate':after-start,'requestsFinal':len(case_requests())-start,'exit':proc.poll() if proc else None, 'transport':'tuic-http-pty' if HTTP else 'python-pty'}
            results.append(result)
            print(json.dumps(result),flush=True)
        finally:
            # Only this owned process group; never TUIC or another live agent.
            if HTTP:
                request_http('DELETE',f'/sessions/{session_id}')
            else:
                try:
                    if proc.poll() is None: os.killpg(proc.pid,signal.SIGTERM)
                except (ProcessLookupError, PermissionError): pass
                try: proc.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    os.killpg(proc.pid,signal.SIGKILL); proc.wait()
                os.close(master)
        (ROOT/'summary.json').write_text(json.dumps(results,indent=2))
server.shutdown()
