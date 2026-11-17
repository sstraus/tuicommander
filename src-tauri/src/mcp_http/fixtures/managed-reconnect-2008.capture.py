import os, json, socket, subprocess, time, urllib.request, urllib.error, pathlib, uuid, http.client, hashlib, sys
root=pathlib.Path(__file__).resolve().parents[4]
home=pathlib.Path(sys.argv[1]).resolve()
# Scratch HOME under the host temp dir, never inside the real HOME.
assert pathlib.Path(os.environ.get('TUIC_TEST_HOST_TMPDIR') or tempfile.gettempdir()).resolve() in home.parents
home.mkdir(exist_ok=False)
out=root/'src-tauri/src/mcp_http/fixtures/managed-reconnect-2008.json'
with socket.socket() as s: s.bind(('127.0.0.1',0)); port=s.getsockname()[1]
token=str(uuid.uuid4()); env=dict(os.environ, HOME=str(home), TMPDIR=str(home), TUIC_PORT=str(port), TUIC_PAIRING_TOKEN=token)
# Debug builds select the file-backed credential store under this isolated HOME.
records=[]; procs=[]
def req(label,path,body=None,headers=None,method=None):
    h={'Cookie':'tui-session='+token, 'Content-Type':'application/json'}; h.update(headers or {})
    data=None if body is None else json.dumps(body).encode()
    r=urllib.request.Request(f'http://127.0.0.1:{port}'+path,data=data,headers=h,method=method)
    if path == '/mcp':
        c=http.client.HTTPConnection('localhost',timeout=10)
        c.sock=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM); c.sock.settimeout(10)
        c.sock.connect(str(home/('tuic-mcp-'+hashlib.sha256(b'fixture-2008').hexdigest()[:16]+'.sock')))
        c.request(r.get_method(),path,body=data,headers=h); response=c.getresponse()
    else:
        try: response=urllib.request.urlopen(r,timeout=10)
        except urllib.error.HTTPError as e: response=e
    raw=response.read().decode(); value=json.loads(raw)
    records.append(dict(label=label,path=path,method=r.get_method(),headers=headers or {},request=body,status=response.status,response_headers={k:v for k,v in response.headers.items() if k.lower() in ('mcp-session-id','mcp-protocol-version')},response=value))
    out.write_text(json.dumps(dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),port=port,instance='fixture-2008',exchanges=records),indent=2)+'\n')
    return value,dict(response.headers)
def start():
    log=open(home/f'daemon-{len(procs)}.log','w')
    p=subprocess.Popen([str(root/'src-tauri/target/debug/tuic-remote'),'--instance','fixture-2008','--bind','127.0.0.1','--no-agent-configs'],cwd=root,env=env,stdout=log,stderr=log); procs.append(p)
    deadline=time.monotonic()+30
    while time.monotonic()<deadline:
        if p.poll() is not None: raise RuntimeError('daemon exited: '+str(p.returncode))
        try:
            with urllib.request.urlopen(f'http://127.0.0.1:{port}/health',timeout=1) as r: r.read()
            return p
        except (OSError,urllib.error.URLError): time.sleep(.1)
    raise RuntimeError('daemon setup exceeded 30s')
def tool(label,name,args,peer=None,sid=None):
    h={'MCP-Protocol-Version':'2026-07-28'}
    if peer: h['x-tuic-session']=peer
    if sid: h['mcp-session-id']=sid; h['MCP-Protocol-Version']='2025-11-25'
    return req(label,'/mcp',{'jsonrpc':'2.0','id':len(records)+1,'method':'tools/call','params':{'name':name,'arguments':args}},h)[0]
def payload(v): return json.loads(v['result']['content'][0]['text'])
try:
    p=start()
    req('version-before','/api/version')
    session,_=req('create-tab','/sessions',{'shell':'/bin/sh','cwd':str(root),'rows':24,'cols':80})
    peer=session['session_id']
    discover=json.loads((root/'src-tauri/src/mcp_http/fixtures/ego_server_discover.json').read_text())
    req('ego-discover-before','/mcp',discover,{'MCP-Protocol-Version':'2026-07-28','x-tuic-session':peer})
    tool('managed-inbox-before','agent',{'action':'inbox'},peer)
    tool('subscribe-before-restart','agent',{'action':'subscribe','subscription_id':'fixture-2008'},peer)
    sent=payload(tool('send-retained-mail','agent',{'action':'send','to':peer,'message':'fixture-2008 retained restart mail'},peer)); assert sent.get('message_id'),sent
    deadline=time.monotonic()+30
    while True:
        suspended,_=req('suspend-tab','/sessions/suspended',{'action':'suspend','sessionId':peer,'metadata':{}})
        if suspended.get('ok'): break
        if suspended.get('error')!='command running' or time.monotonic()>deadline: raise RuntimeError(suspended)
        time.sleep(.1)
    p.terminate(); p.wait(timeout=15)
    p=start()
    req('version-after','/api/version')
    resumed,_=req('resume-tab','/sessions/suspended',{'action':'resume','tuicSession':peer})
    newpty=resumed['session_id']; assert newpty!=peer,(peer,resumed)
    req('restored-sessions','/sessions')
    req('ego-discover-after','/mcp',discover,{'MCP-Protocol-Version':'2026-07-28','x-tuic-session':peer})
    inbox=payload(tool('managed-inbox-after-restart','agent',{'action':'inbox','subscription_id':'fixture-2008'},peer)); assert any(m['id']==sent['message_id'] for m in inbox['messages']),inbox
    tool('managed-project-after-restart','progress',{'type':'done','text':'fixture-2008 recovered project'},peer)
    tool('external-headerless-inbox','agent',{'action':'inbox'})
    tool('external-missing-project','progress',{'type':'done','text':'fixture-2008 no guessed project'})
    def initialize(label):
        v,h=req(label,'/mcp',{'jsonrpc':'2.0','id':len(records)+1,'method':'initialize','params':{'protocolVersion':'2025-11-25','capabilities':{},'clientInfo':{'name':'fixture-2008','version':'1'}}},{'MCP-Protocol-Version':'2025-11-25'})
        return next(v for k,v in h.items() if k.lower()=='mcp-session-id')
    old=str(uuid.uuid4()); replacement=str(uuid.uuid4())
    sid=initialize('legacy-initialize-old')
    tool('legacy-register-old','agent',{'action':'register','tuic_session':old,'name':'fixture-old'},sid=sid)
    tool('send-orphan-mail','agent',{'action':'send','to':old,'message':'fixture-554 orphan mail'},peer)
    sid2=initialize('legacy-initialize-replacement')
    migrated=payload(tool('legacy-replace-orphan','agent',{'action':'register','tuic_session':replacement,'name':'fixture-new','replaces':old},sid=sid2))
    assert migrated['mail_migrated']==1,migrated
    tool('legacy-migrated-inbox','agent',{'action':'inbox'},sid=sid2)
    tool('send-live-owner-mail','agent',{'action':'send','to':peer,'message':'fixture-554 live-owner mail'},peer)
    sid3=initialize('legacy-initialize-claimant')
    stranded=payload(tool('legacy-replace-live-owner','agent',{'action':'register','tuic_session':str(uuid.uuid4()),'name':'fixture-claimant','replaces':peer},sid=sid3))
    assert stranded['mail_migrated']==0 and stranded['mail_stranded']>=1,stranded
    tool('live-owner-retains-mail','agent',{'action':'inbox'},peer)
    req('delete-restored-tab','/sessions/'+newpty,method='DELETE')
    print('CAPTURE OK',out)
finally:
    for p in procs:
        if p.poll() is None:
            p.terminate()
            try: p.wait(timeout=15)
            except subprocess.TimeoutExpired: p.kill(); p.wait()
