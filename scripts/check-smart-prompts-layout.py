#!/usr/bin/env python3
"""Check the real Smart Prompt editor geometry through the shared stealth browser.

Catches: shared row selectors replacing the placement grid and right-aligning the shortcut.
Run with --capture-only for post-edit screenshot collection without a GREEN run.
"""
import argparse
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import tempfile
import time
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parents[1]
# The stealth browser wrapper (the global rules' agent-browser entry point) is machine-specific.
WRAPPER = Path(os.environ.get('TUIC_BROWSER_WRAPPER') or 'agent-browser-stealth-wrapper-not-configured')
MEASURE = """JSON.stringify((() => {
 const editor=[...document.querySelectorAll('section')].find(s=>s.querySelector('h3')?.textContent==='Edit Layout review');
 const rect=e=>{const r=e.getBoundingClientRect();return {left:r.left,right:r.right,top:r.top,bottom:r.bottom,height:r.height,width:r.width};};
 const placements=[...editor.querySelectorAll('label:has(input[type=checkbox])')].map(label=>({
 text:label.textContent,check:rect(label.querySelector('input')),textRect:rect(label.querySelector('span')),
 display:getComputedStyle(label).display,direction:getComputedStyle(label).flexDirection}));
 const rows=[...editor.querySelectorAll('label[for]')].map(label=>({label:label.textContent,
 control:document.getElementById(label.htmlFor) ? rect(document.getElementById(label.htmlFor)):null}));
 return {placements,rows,selects:[...editor.querySelectorAll('select')].map(rect),
 shortcut:rect([...editor.querySelectorAll('button')].find(b=>b.title==='Click to change shortcut')),
 text:editor.textContent,card:rect(editor)};
})())"""
READY = """new Promise((resolve,reject)=>{
 const ready=()=>[...document.querySelectorAll('h3')].some(h=>h.textContent==='Edit Layout review');
 if(ready())return resolve(true);
 const observer=new MutationObserver(()=>{if(ready()){observer.disconnect();resolve(true)}});
 observer.observe(document.documentElement,{childList:true,subtree:true});
 setTimeout(()=>{observer.disconnect();reject(new Error('editor did not mount'))},45000);
})"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--evidence', type=Path, required=True)
    parser.add_argument('--capture-only', action='store_true')
    parser.add_argument('--voice-only', action='store_true', help='Run only the shared Voice wrapper regression')
    parser.add_argument('--mock', type=Path, help='Optional approved HTML mock to capture before the component')
    args = parser.parse_args()
    evidence = args.evidence.resolve()
    evidence.mkdir(parents=True, exist_ok=False)
    (evidence / 'reap-scope').mkdir()
    session = 'smart-prompts-' + uuid.uuid4().hex[:12]
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
    config.write_text("import solid from " + json.dumps(solid_path) + ";export default {plugins:[solid()],define:{__APP_VERSION__:'\"layout\"',__BUILD_GIT_HASH__:'\"layout\"'},optimizeDeps:{entries:['src/__tests__/browser/smartPromptsLayout.html']},server:{host:'127.0.0.1',port:15979,strictPort:true}};")
    daemon = None
    results = []
    voice_results = []
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
            if args.mock:
                run([args.mock.resolve().as_uri()])
                run(['--', 'screenshot', str(evidence / 'approved-mock.png')])
            for _ in range(80):
                try:
                    urllib.request.urlopen('http://127.0.0.1:15979/src/__tests__/browser/smartPromptsLayout.html', timeout=1)
                    break
                except OSError:
                    time.sleep(.25)
            for name, query in ([] if args.voice_only else [(f'{mode}-{theme}', f'?mode={mode}&theme={theme}') for mode in ['inject','shell','api','headless'] for theme in ['dark','light']]):
                run(['http://127.0.0.1:15979/src/__tests__/browser/smartPromptsLayout.html' + query], optional=True)
                run(['--', 'eval', READY])
                run(['--', 'set', 'viewport', '1100', '1200'])
                measured = json.loads(json.loads(run(['--', 'eval', MEASURE])))
                results.append({'name': name, 'blocks': measured})
                run(['--', 'screenshot', str(evidence / (name + '.png'))])
            for theme in ['dark', 'light']:
                run(['http://127.0.0.1:15979/src/__tests__/browser/smartPromptsLayout.html?voice&theme=' + theme])
                run(['--', 'eval', "new Promise((resolve,reject)=>{const ready=()=>document.querySelector('input[placeholder=\"Heard text...\"]');if(ready())return resolve(true);const observer=new MutationObserver(()=>{if(ready()){observer.disconnect();resolve(true)}});observer.observe(document.documentElement,{childList:true,subtree:true});setTimeout(()=>reject(new Error('Voice editor did not mount')),45000);})"])
                run(['--', 'set', 'viewport', '1100', '1200'])
                measured = json.loads(json.loads(run(['--', 'eval', "JSON.stringify((()=>{const first=document.querySelector('input[placeholder=\"Heard text...\"]');const row=first.parentElement;row.scrollIntoView({block:'center'});const rect=e=>{const r=e.getBoundingClientRect();return {left:r.left,top:r.top,width:r.width,height:r.height};};return {fields:[...row.querySelectorAll('input')].map(rect),wrapper:rect(row),card:rect(row.closest('section'))};})())"])))
                voice_results.append({'theme': theme, **measured})
                run(['--', 'screenshot', str(evidence / ('voice-' + theme + '.png'))])
            (evidence / 'voice-measurements.json').write_text(json.dumps(voice_results, indent=2) + '\n')
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
        # Catches F1: a global named-wrapper exemption shrinks both Voice correction inputs to 88px.
        for voice in voice_results:
            fields = voice['fields']
            if len(fields) != 2 or any(field['width'] < 230 for field in fields):
                failures.append('VoiceAutoCorrectionsWidth ' + voice['theme'] + ': fields lost their full-width wrapper contract')
            if len(fields) == 2 and (abs(fields[0]['left'] - fields[1]['left']) > 1 or fields[1]['top'] <= fields[0]['top']):
                failures.append('VoiceAutoCorrectionsWidth ' + voice['theme'] + ': fields no longer wrap on separate lines')
        expected = ['Toolbar', 'Command palette', 'Git · Changes', 'Git · Branches', 'PR popover', 'Issue popover', 'Terminal menu', 'File menu']
        for case in results:
            block = case['blocks']
            name = case['name']
            placements = block['placements']
            if [p['text'] for p in placements] != expected:
                failures.append(name + ': placement names/order are not the approved eight readable names')
            for placement in placements:
                check, text = placement['check'], placement['textRect']
                if abs(check['top'] + check['height']/2 - text['top'] - text['height']/2) > 2:
                    failures.append(name + ': checkbox above label: ' + placement['text'])
            if len({round(p['check']['left']) for p in placements}) != 2:
                failures.append(name + ': placement does not have exactly two columns')
            controls = [r['control'] for r in block['rows'] if r['control']]
            if len({round(c['left']) for c in controls} | {round(block['shortcut']['left'])}) != 1:
                failures.append(name + ': controls do not share one left edge')
            if any(abs(c['height']-28) > .1 for c in block['selects'] + [block['shortcut']]):
                failures.append(name + ': selects/shortcut are not 28px high')
            labels = [r['label'] for r in block['rows']]
            mode = name.split('-')[0]
            for label, shown in [('Target', mode=='inject'), ('Send immediately', mode=='inject'), ('Preferred Agent',mode=='headless'), ('Output Target',mode!='inject'), ('System Prompt',mode=='api')]:
                if (label in labels) != shown:
                    failures.append(name + ': incorrect conditional field: ' + label)
            if 'Uncheck to review' in block['text']:
                failures.append(name + ': explanatory hint remains')
        assert not failures, 'SmartPromptsTabLayout: ' + '; '.join(failures)
        print('PASS: SmartPromptsTabLayout — eight placements, aligned controls, density and conditional fields')
    print(evidence)


if __name__ == '__main__':
    main()
