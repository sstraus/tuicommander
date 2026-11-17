#!/usr/bin/env python3
"""Check real Chat code-copy geometry through the shared stealth browser.

Catches: an absolutely positioned Copy control overflowing a one-line pre.
Run with --capture-only for post-edit screenshot collection without a GREEN run.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import tempfile
import time
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parents[1]
# The browser-stealth wrapper: TUIC_BROWSER_WRAPPER, else ab-stealth.sh on PATH.
WRAPPER = os.environ.get('TUIC_BROWSER_WRAPPER') or shutil.which('ab-stealth.sh') or 'ab-stealth.sh'
MEASURE = """JSON.stringify([...document.querySelectorAll('pre[data-raw-code]')].map(pre => {
 const button = pre.querySelector('[data-copy-code]');
 const rect = element => { const r=element.getBoundingClientRect();return {left:r.left,right:r.right,top:r.top,bottom:r.bottom,height:r.height}; };
 const range=document.createRange();range.selectNodeContents(button);
 const codeRange=document.createRange();codeRange.selectNodeContents(pre.querySelector('code'));
 const codeLines=[...codeRange.getClientRects()].map(r=>({left:r.left,right:r.right,top:r.top,bottom:r.bottom}));
 const style=getComputedStyle(button);
 return {pre:rect(pre),button:rect(button),label:rect(range),codeLines,clientHeight:pre.clientHeight,scrollHeight:pre.scrollHeight,
 buttonStyle:{font:style.font,lineHeight:style.lineHeight,padding:style.padding,position:style.position},
 documentWidth:document.documentElement.scrollWidth,viewportWidth:innerWidth};
}))"""
READY = """new Promise((resolve,reject)=>{
 const ready=()=>document.querySelector('pre[data-raw-code] [data-copy-code]');
 if(ready())return resolve(true);
 const observer=new MutationObserver(()=>{if(ready()){observer.disconnect();resolve(true)}});
 observer.observe(document.documentElement,{childList:true,subtree:true});
 setTimeout(()=>{observer.disconnect();reject(new Error('code Copy did not mount'))},45000);
})"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--evidence', type=Path, required=True)
    parser.add_argument('--capture-only', action='store_true')
    args = parser.parse_args()
    evidence = args.evidence.resolve()
    evidence.mkdir(parents=True, exist_ok=False)
    (evidence / 'reap-scope').mkdir()
    session = 'code-copy-' + uuid.uuid4().hex[:12]
    env = dict(os.environ, AB_SESSION=session, AB_AGENT_BROWSER_DIR=str(evidence / 'reap-scope'))
    env.pop('AB_KEEP_FOCUS', None)
    # Scratch lives under the caller's temp dir, never inside the checkout.
    scratch = Path(os.environ.get('TUIC_TEST_TMP_ROOT') or tempfile.gettempdir())
    scratch.mkdir(parents=True, exist_ok=True)
    config = scratch / (session + '.config.mjs')
    # The config sits outside the checkout, so bare imports would not resolve: pin the plugin by path.
    solid_path = subprocess.run(
        ['node', '--input-type=module', '-e', "import {fileURLToPath} from 'node:url';console.log(fileURLToPath(import.meta.resolve('vite-plugin-solid')))"],
        cwd=ROOT, text=True, capture_output=True, check=True).stdout.strip()
    config.write_text("import solid from " + json.dumps(solid_path) + ";export default {plugins:[solid()],define:{__APP_VERSION__:'\"layout\"',__BUILD_GIT_HASH__:'\"layout\"'},optimizeDeps:{entries:['src/__tests__/browser/sessionChatAppearance.html']},server:{host:'127.0.0.1',port:15970,strictPort:true}};")
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
            for _ in range(80):
                try:
                    urllib.request.urlopen('http://127.0.0.1:15970/src/__tests__/browser/sessionChatAppearance.html', timeout=1)
                    break
                except OSError:
                    time.sleep(.25)
            for name, query in [('mobile-dark', '?mobile'), ('mobile-light', '?mobile&theme=light'), ('desktop-narrow-dark', ''), ('mobile-long-code', '?mobile&longCode'), ('desktop-long-code', '?longCode')]:
                run(['http://127.0.0.1:15970/src/__tests__/browser/sessionChatAppearance.html' + query], optional=True)
                run(['--', 'eval', READY])
                run(['--', 'set', 'viewport', '390', '844'])
                measured = json.loads(json.loads(run(['--', 'eval', MEASURE])))
                results.append({'name': name, 'blocks': measured})
                run(['--', 'screenshot', str(evidence / (name + '.png'))])
            (evidence / 'measurements.json').write_text(json.dumps(results, indent=2) + '\n')
        finally:
            # Navigation may time out before the wrapper prints its daemon name.
            if daemon is None:
                pins = list((Path.home() / '.agent-browser').glob(f'ab-shared-{session}-*.target'))
                if len(pins) == 1:
                    daemon = pins[0].stem
            if daemon:
                run(['--', 'tab', 'close'], optional=True)
                closed = subprocess.run(['agent-browser', '--session', daemon, 'close'], env=env, text=True,
                                        capture_output=True, timeout=15)
                log.write('daemon cleanup: ' + closed.stdout + closed.stderr)
            os.killpg(server.pid, signal.SIGTERM)
            try:
                server.wait(timeout=15)
            except subprocess.TimeoutExpired:
                os.killpg(server.pid, signal.SIGKILL)
                server.wait(timeout=5)
            config.unlink(missing_ok=True)
    if not args.capture_only:
        failures = []
        for case in results:
            if not case['blocks']:
                failures.append(case['name'] + ': no code blocks')
            for block in case['blocks']:
                if (block['button']['top'] < block['pre']['top'] or block['button']['bottom'] > block['pre']['bottom']
                        or block['button']['left'] < block['pre']['left'] or block['button']['right'] > block['pre']['right']):
                    failures.append(case['name'] + ': Copy extends outside short code block')
                if block['scrollHeight'] > block['clientHeight']:
                    failures.append(case['name'] + ': one-line code block has vertical overflow')
                if block['label']['bottom'] > block['button']['bottom']:
                    failures.append(case['name'] + ': Copy label is clipped')
                # Catches: a contained Copy button still masks a wrapped code line.
                button = block['button']
                if any(button['left'] < line['right'] and button['right'] > line['left']
                       and button['top'] < line['bottom'] and button['bottom'] > line['top']
                       for line in block['codeLines']):
                    failures.append(case['name'] + ': Copy covers code text')
        assert not failures, 'short_code_copy_fits: ' + '; '.join(failures)
        print('PASS: short_code_copy_fits in three presentations plus long-code boundary')
    print(evidence)


if __name__ == '__main__':
    main()
