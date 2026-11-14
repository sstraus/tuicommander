import json, os, pathlib, shutil, socket, subprocess, sys, threading, time, urllib.request, urllib.parse
root = pathlib.Path('/Users/stefano.straus/Gits/.tmp/feat-1705r-native')
root.mkdir(exist_ok=True)
home = root / 'home'; home.mkdir(exist_ok=True)
repo = root / 'repo'; repo.mkdir(exist_ok=True)
def git(*args):
    return subprocess.check_output(['git', '-C', str(repo), *args], stderr=subprocess.STDOUT, text=True).strip()
git('init', '-b', 'main'); git('config', 'user.email', 'benchmark@example.invalid'); git('config', 'user.name', 'Benchmark')
(repo / '.gitignore').write_text('cache/\n')
for i in range(100):
    (repo / f'source-{i}.txt').write_text('tracked fixture\n')
git('add', '.'); git('commit', '-m', 'chore: seed benchmark fixture')
s = socket.socket(); s.bind(('127.0.0.1', 0)); port = s.getsockname()[1]; s.close()
env = os.environ.copy(); env['HOME'] = str(home); env['TUIC_PORT'] = str(port); env['TMPDIR'] = str(root); env.pop('TUIC_SESSION', None); env.pop('TUIC_PARENT', None)
subprocess.run([sys.argv[1], '--instance', 'feat-1705r-benchmark', '--set-password'], env=env, input='benchmark\nfixture-only-password\n', text=True, stdout=subprocess.DEVNULL, check=True)
log = (root / 'daemon.log').open('w')
proc = subprocess.Popen([sys.argv[1], '--instance', 'feat-1705r-benchmark', '--bind', '127.0.0.1', '--no-agent-configs'], env=env, stdout=log, stderr=log)
(root / 'daemon.pid').write_text(str(proc.pid))
base = f'http://127.0.0.1:{port}'
cookie = None
def request(path, method='GET', data=None):
    global cookie
    headers={'Content-Type':'application/json'}
    if cookie: headers['Cookie'] = cookie
    else: headers['Authorization'] = 'Basic YmVuY2htYXJrOmZpeHR1cmUtb25seS1wYXNzd29yZA=='
    req = urllib.request.Request(base + path, method=method, data=json.dumps(data).encode() if data is not None else None, headers=headers)
    with urllib.request.urlopen(req, timeout=60) as response:
        if response.headers.get('Set-Cookie'): cookie = response.headers['Set-Cookie'].split(';')[0]
        return json.load(response)
samples=[]; stop=threading.Event()
def sample():
    while not stop.is_set():
        output = subprocess.check_output(['ps','-axo','pid=,pcpu=,comm='], text=True)
        selected=[]
        for line in output.splitlines():
            fields=line.strip().split(None,2)
            if len(fields)==3 and ('wdavdaemon' in fields[2] or 'fseventsd' in fields[2]):
                selected.append({'pid':int(fields[0]),'pcpu':float(fields[1]),'comm':fields[2]})
        samples.append({'at':time.time(),'processes':selected}); stop.wait(0.1)
try:
    deadline=time.monotonic()+30
    while True:
        try: request('/health'); break
        except Exception:
            if proc.poll() is not None or time.monotonic()>deadline: raise
            time.sleep(0.1)
    seed=request('/worktrees','POST',{'base_repo':str(repo),'branch_name':'seed'})
    checkout=pathlib.Path(seed['path'])
    deadline=time.monotonic()+30
    while True:
        paths=request('/worktrees/paths?'+urllib.parse.urlencode({'path':str(repo)}))
        if 'pending' not in json.dumps(paths): break
        if time.monotonic()>deadline: raise RuntimeError('seed warming did not settle')
        time.sleep(0.1)
    cache=checkout/'cache'; cache.mkdir()
    for i in range(1000): (cache/f'artifact-{i}').write_bytes(b'x'*1024)
    inode=(cache/'artifact-0').stat().st_ino
    remove=lambda branch: request('/worktrees/'+branch+'?'+urllib.parse.urlencode({'repoPath':str(repo),'deleteBranch':'true'}),'DELETE')
    remove('seed')
    worker=threading.Thread(target=sample); worker.start()
    creates=[]
    for batch,count in [('pilot',3),('main',10),('completion',2)]:
        for i in range(count):
            branch=f'{batch}-{i}'
            begin=time.monotonic(); result=request('/worktrees','POST',{'base_repo':str(repo),'branch_name':branch}); elapsed=time.monotonic()-begin
            rename=result.get('worktree_rename'); assert rename, result
            path=pathlib.Path(result['path']); assert not pathlib.Path(rename['old_path']).exists()
            assert (path/'cache'/'artifact-0').stat().st_ino == inode
            assert git('rev-parse', branch)==git('rev-parse','main')
            creates.append({'batch':batch,'branch':branch,'seconds':elapsed,'rename':rename,'artifact_inode':inode})
            remove(branch)
    stop.set(); worker.join()
    report={'port':port,'daemon_pid':proc.pid,'fixture':{'tracked_files':101,'ignored_files':1000,'ignored_bytes':1024000},'creates':creates,'cpu_samples':samples,'method':'Serial create HTTP wall time, followed by safe removal to archive for next reuse. ps -axo pid,pcpu,comm every 100ms; pcpu is OS rolling process average, not interval CPU. Pilot 3, main 10, completion 2; no agent PTYs. Shared host not idle.'}
    (root/'measurement.json').write_text(json.dumps(report,indent=2))
    print(json.dumps({'count':len(creates),'total_create_seconds':sum(x['seconds'] for x in creates),'min_seconds':min(x['seconds'] for x in creates),'max_seconds':max(x['seconds'] for x in creates),'samples':len(samples)}))
finally:
    stop.set()
    if 'worker' in locals(): worker.join()
    proc.terminate()
    try: proc.wait(timeout=10)
    except subprocess.TimeoutExpired: proc.kill(); proc.wait()
    log.close()
