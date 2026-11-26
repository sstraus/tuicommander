#!/usr/bin/env python3
"""Check that modal toast actions stay reachable at both scroll boundaries.

Catches: flex-end placing the first nonshrinking toast before the scroll origin.
Uses the real ToastList, shared controls, store and stylesheet, not copied markup.
"""
import argparse
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parents[1]
WRAPPER = Path.home() / 'Gits/personal/brainstorming/tools/browser-stealth/ab-stealth.sh'
PROBE = """JSON.stringify((()=>{
 const actions=[...document.querySelectorAll('[data-level] button')];
 const container=document.querySelector('[data-level]').parentElement;
 const boundary=new URLSearchParams(location.search).get('boundary')||'first';
 container.scrollTop=boundary==='last'?container.scrollHeight:0;
 const button=boundary==='last'?actions.at(-1):actions[0];
 // A single tall card can exceed the viewport. Reachability means ordinary
 // scrolling can reveal its action, not that every action is visible at zero.
 button.scrollIntoView({block:'nearest',inline:'nearest',behavior:'instant'});
 const c=container.getBoundingClientRect(),b=button.getBoundingClientRect();
 const x=(b.left+b.right)/2,y=(b.top+b.bottom)/2;
 const hit=document.elementFromPoint(x,y);
 return {boundary,container:c.toJSON(),button:b.toJSON(),scrollTop:container.scrollTop,
 scrollHeight:container.scrollHeight,clientHeight:container.clientHeight,
 action:button.textContent,reachable:b.top>=c.top&&b.bottom<=c.bottom&&!!hit&&button.contains(hit)};
})())"""
READY = """new Promise((resolve,reject)=>{
 const ready=()=>document.querySelectorAll('[data-level] button').length===2;
 if(ready())return resolve(true);
 const observer=new MutationObserver(()=>{if(ready()){observer.disconnect();resolve(true)}});
 observer.observe(document.documentElement,{childList:true,subtree:true});
 setTimeout(()=>{observer.disconnect();reject(new Error('toast fixture did not mount'))},15000);
})"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--evidence', type=Path, required=True)
    args = parser.parse_args()
    evidence = args.evidence.resolve()
    evidence.mkdir(parents=True, exist_ok=False)
    (evidence / 'reap-scope').mkdir()
    session = 'toast-scroll-' + uuid.uuid4().hex[:10]
    env = dict(os.environ, AB_SESSION=session, AB_AGENT_BROWSER_DIR=str(evidence / 'reap-scope'))
    env.pop('AB_KEEP_FOCUS', None)
    scratch = ROOT / '.tmp' / session
    scratch.mkdir(parents=True)
    (scratch / 'fixture.html').write_text('<!doctype html><html><head><meta name="viewport" content="width=device-width, initial-scale=1.0"><title>Modal toast scroll</title></head><body><div id="fixture"></div><script type="module" src="./fixture.tsx"></script></body></html>')
    (scratch / 'fixture.tsx').write_text('''import {render} from "solid-js/web";
import {ToastList} from "/src/components/ToastContainer/ToastList";
import {toastsStore} from "/src/stores/toasts";
import "/src/global.css";
const short=new URLSearchParams(location.search).has("short");
for(let i=1;i<=2;i++)toastsStore.add(
 `Notice ${i}: `+(short?"Workflow update":"A retained actionable workflow notification needs operator attention. ".repeat(12)),
 "This sticky notification must remain reachable without covering the dialog controls.",
 "error",false,{label:`Action ${i}`,onClick:()=>{document.body.dataset.action=String(i)}},0,undefined,undefined,false);
render(()=><><div role="dialog" aria-modal="true" aria-label="Layout fixture">Modal toast layout fixture</div>
<ToastList onDismiss={t=>toastsStore.remove(t.id)} repoName={()=>null}/></>,document.getElementById("fixture")!);
''')
    config = scratch / 'vite.config.ts'
    config.write_text("import {defineConfig} from 'vite';import solid from 'vite-plugin-solid';export default defineConfig({plugins:[solid()],define:{__APP_VERSION__:'\"layout\"',__BUILD_GIT_HASH__:'\"layout\"'},server:{host:'127.0.0.1',port:15972,strictPort:true}});")
    daemon = None
    results = []
    with (evidence / 'transcript.txt').open('w') as log, (evidence / 'vite.log').open('w') as vite_log:
        server = subprocess.Popen(['node', 'node_modules/vite/bin/vite.js', '--config', str(config)], cwd=ROOT,
                                  stdout=vite_log, stderr=subprocess.STDOUT, start_new_session=True)

        def run(command, optional=False):
            nonlocal daemon
            result = subprocess.run([str(WRAPPER), *command], env=env, text=True, capture_output=True, timeout=65)
            log.write('$ ' + repr(command) + '\n' + result.stdout + result.stderr + f'exit={result.returncode}\n')
            log.flush()
            match = re.search(r'Chrome=(\d+)', result.stderr)
            if match:
                daemon = f'ab-shared-{session}-{match[1]}'
            if result.returncode and not optional:
                raise RuntimeError('browser command failed; see transcript')
            return result.stdout

        try:
            url = f'http://127.0.0.1:15972/.tmp/{session}/fixture.html'
            for _ in range(80):
                try:
                    urllib.request.urlopen(url, timeout=1)
                    break
                except OSError:
                    if server.poll() is not None:
                        raise RuntimeError('Vite failed; see vite.log')
                    time.sleep(.25)
            for name, query, width, height in [
                ('overflow-first', '?boundary=first', 720, 720),
                ('overflow-last', '?boundary=last', 720, 720),
                ('short-stack', '?short&boundary=first', 1280, 900),
            ]:
                run([url + query], optional=True)
                run(['--', 'eval', READY])
                run(['--', 'set', 'viewport', str(width), str(height)])
                measured = json.loads(json.loads(run(['--', 'eval', PROBE])))
                results.append(dict(name=name, **measured))
                run(['--', 'screenshot', str(evidence / (name + '.png'))])
            (evidence / 'measurements.json').write_text(json.dumps(results, indent=2) + '\n')
        finally:
            if daemon is None:
                pins = list((Path.home() / '.agent-browser').glob(f'ab-shared-{session}-*.target'))
                if len(pins) == 1:
                    daemon = pins[0].stem
            if daemon:
                run(['--', 'tab', 'close'], optional=True)
                closed = subprocess.run(['agent-browser', '--session', daemon, 'close'], env=env,
                                        text=True, capture_output=True, timeout=15)
                log.write('daemon cleanup: ' + closed.stdout + closed.stderr)
            os.killpg(server.pid, signal.SIGTERM)
            try:
                server.wait(timeout=15)
            except subprocess.TimeoutExpired:
                os.killpg(server.pid, signal.SIGKILL)
                server.wait(timeout=5)
            for path in scratch.iterdir():
                path.unlink()
            scratch.rmdir()
    assert all(row['reachable'] for row in results), 'modal_toast_actions_reachable: ' + repr(results)
    assert results[0]['scrollHeight'] > results[0]['clientHeight'], 'fixture did not overflow'
    assert results[2]['scrollHeight'] == results[2]['clientHeight'], 'short fixture unexpectedly overflowed'
    print('PASS modal_toast_actions_reachable: overflow first/last and short stack; ' + str(evidence))


if __name__ == '__main__':
    main()
