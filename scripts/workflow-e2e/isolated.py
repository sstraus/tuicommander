"""Lifecycle owner for serial native cases; no provider credentials are loaded here."""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import threading
import time

from run import LOCAL, ROOT, admission
import argparse

parser=argparse.ArgumentParser(description='Own one named native daemon for a real workflow case')
parser.add_argument('--candidate',required=True)
parser.add_argument('--manifest',required=True)
parser.add_argument('--evidence',required=True)
parser.add_argument('--case',choices=LOCAL,required=True)
args=parser.parse_args()
manifest,_,_,evidence,candidate=admission(args)
root=evidence.parent
root.mkdir(parents=True,exist_ok=True)
name=evidence.name
case=args.case
instance=manifest['environment']['TUIC_APP_INSTANCE']
port=manifest['environment']['TUIC_PORT']
experiment=manifest['experiment']
if experiment['sandbox'] not in ('workspace','adaptive','off') or experiment['mode'] not in ('default','yolo'):
    raise SystemExit('Unsupported experiment variant')
os.environ['TUIC_E2E_SANDBOX']=experiment['sandbox']
os.environ['TUIC_E2E_MODE']=experiment['mode']
for key in ('EGO_HOME','OPENROUTER_API_KEY'):
    os.environ.pop(key,None)
control=root/(name+'-lifecycle.sock')
if control.exists():raise SystemExit('Refusing an existing lifecycle socket')
manifest['lifecycleSocket']=str(control)
launch_manifest=root/(name+'-launch.json')
if launch_manifest.exists():raise SystemExit('Refusing an existing launch record')
launch_manifest.write_text(json.dumps(manifest,indent=2)+'\n')
current=[None]
receipts=[]
lock=threading.Lock()
finished=threading.Event()


def inventory():
    output=subprocess.check_output(['ps','-axo','pid=,ppid=,lstart=,comm='],text=True)
    entries={}
    for line in output.splitlines():
        fields=line.split(None,7)
        if len(fields)==8:
            entries[int(fields[0])]={'parent':int(fields[1]),'identity':' '.join(fields[2:])}
    return entries


def stop_owned():
    if current[0] is None:return
    before=inventory()
    pid_file=Path(manifest['pid_file'])
    native=int(pid_file.read_text().strip()) if pid_file.exists() else None
    owned={native} if native is not None else set()
    while True:
        expanded=owned|{pid for pid,item in before.items() if item['parent'] in owned}
        if expanded==owned:break
        owned=expanded
    result=subprocess.run(['make','agent-test-stop',f'AGENT_TEST_INSTANCE={instance}',f'AGENT_TEST_PORT={port}'],cwd=ROOT)
    current[0].wait(timeout=30)
    # A bridge may finish its bounded stdio shutdown just after the native parent.
    deadline=time.monotonic()+10
    while True:
        after=inventory()
        retained=[pid for pid in owned if pid in after and pid in before and
                  after[pid]['identity']==before[pid]['identity']]
        if not retained or time.monotonic()>=deadline:break
        time.sleep(0.1)
    receipt={'supervisorPid':current[0].pid,'exit':current[0].returncode,
             'nativePid':native,'ownedPids':sorted(owned),'stillPresent':retained,
             'stopExit':result.returncode}
    receipts.append(receipt)
    (root/(name+'-daemon-stops.json')).write_text(json.dumps(receipts,indent=2)+'\n')
    current[0]=None
    if retained:raise RuntimeError('owned child processes survived daemon stop: '+str(retained))


def launch():
    current[0]=subprocess.Popen(['make','agent-test',f'AGENT_TEST_INSTANCE={instance}',
        f'AGENT_TEST_PORT={port}',f"AGENT_TEST_REUSE_BUILD_SHA256={manifest['binarySha256']}"],cwd=ROOT)
    readiness=subprocess.Popen(['/Users/stefano.straus/Gits/personal/orchestrator/tools/build/agent-test-up.sh',
                                str(port),'600'])
    while readiness.poll() is None:
        if current[0].poll() is not None:
            readiness.terminate()
            readiness.wait(timeout=10)
            raise RuntimeError('isolated daemon exited before readiness: '+str(current[0].returncode))
        time.sleep(0.25)
    if readiness.returncode:
        raise RuntimeError('isolated daemon readiness failed: '+str(readiness.returncode))


def serve(listener):
    listener.settimeout(0.5)
    while not finished.is_set():
        try:connection,_=listener.accept()
        except socket.timeout:continue
        except OSError:
            if finished.is_set():break
            raise
        with connection:
            try:
                request=json.loads(connection.makefile('r').readline())
                if request!={'action':'restart','instance':instance,
                              'candidate':manifest['candidate']}:
                    raise ValueError('Unexpected isolated lifecycle request')
                with lock:
                    stop_owned()
                    launch()
                response={'status':'ready','stops':receipts}
            except Exception as error:
                response={'status':'failed','reason':str(error)}
            connection.sendall((json.dumps(response)+'\n').encode())


with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as listener:
    listener.bind(str(control))
    os.chmod(control,0o600)
    listener.listen(1)
    thread=threading.Thread(target=serve,args=(listener,),daemon=True)
    try:
        launch()
        thread.start()
        result=subprocess.run([sys.executable,'-B',str(Path(__file__).parent/'run.py'),
            '--candidate',candidate,
            '--manifest',str(launch_manifest),'--evidence',str(evidence),'--case',case],cwd=ROOT,
            timeout=max(1,(manifest['budgetExpiresMs']/1000)-time.time()))
    finally:
        finished.set()
        with lock:stop_owned()
        control.unlink(missing_ok=True)
sys.exit(result.returncode)
