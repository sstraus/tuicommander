"""Fail-closed entrypoint for serial, real-agent workflow scenarios."""
import argparse
import hashlib
import datetime
import json
import os
from pathlib import Path
import re
import sys
import traceback
import time
from client import Client, command

ROOT = Path(os.environ.get('TUIC_WORKFLOW_E2E_ROOT', Path(__file__).resolve().parents[2])).resolve()
LOCAL = ('E01', 'E02', 'E03', 'E05', 'E07', 'E20', 'E22', 'E24')


def admission(args):
    manifest = json.loads(Path(args.manifest).read_text())
    candidate = command(['git', 'rev-parse', args.candidate+'^{commit}'], ROOT)
    if not re.fullmatch(r'[0-9a-f]{40}', candidate):
        raise ValueError('candidate must resolve to a full Git commit')
    if manifest['candidate'] != candidate:
        raise ValueError('manifest candidate differs')
    if time.time()*1000 >= manifest.get('budgetExpiresMs', 0):
        raise ValueError('explicit execution budget missing or exhausted')
    if manifest.get('experiment',{}).get('answerTiming','immediate') not in ('immediate','after_acp_idle'):
        raise ValueError('unknown answerTiming variant')
    if manifest.get('experiment',{}).get('closure','human') not in ('human','automatic'):
        raise ValueError('unknown closure variant')
    instance = manifest['environment']['TUIC_APP_INSTANCE']
    if not re.fullmatch(r'agent-test-[a-z0-9-]+', instance):
        raise ValueError('requires a named agent-test instance')
    digest = hashlib.sha256(instance.encode()).hexdigest()[:16]
    expected = Path.home() / f'Gits/.tuic/tuic-at/{digest}/tuic-mcp-{digest}.sock'
    if Path(manifest['socket']) != expected:
        raise ValueError('socket does not match the isolated instance')
    binary = Path(manifest['binary']).resolve(strict=True)
    if not binary.is_relative_to(ROOT / '.tmp/agent-test' / instance):
        raise ValueError('binary is outside this worktree instance')
    changed=command(['git','diff',candidate,'--name-only'],ROOT).splitlines()
    allowed=lambda p: (p.startswith(('docs/','scripts/workflow-e2e/')) or p in
        ('.gitignore','Makefile','scripts/agent-test.py','scripts/workflow-e2e.sh'))
    if any(not allowed(p) for p in changed):
        raise ValueError('application source differs from pinned candidate')
    if manifest.get('binarySha256') != hashlib.sha256(binary.read_bytes()).hexdigest():
        raise ValueError('binary differs from the approved build digest')
    output = Path(args.evidence).resolve()
    if not output.is_relative_to(Path.home() / 'Gits'):
        raise ValueError('evidence must stay under Gits')
    if output.exists():
        raise ValueError('evidence directory must be fresh')
    return manifest, expected, binary, output, candidate


def preflight(args, context):
    manifest, expected, binary, output, candidate = admission(args)
    output.mkdir(parents=True, exist_ok=False)
    context['output'] = output
    client = Client(expected, output)
    health = client.request('GET', '/health')
    binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
    if manifest.get('binarySha256') != binary_hash:
        raise ValueError('binary differs from the approved build digest')
    if health.get('socket_path') != str(expected) or health['build']['sha256'] != binary_hash:
        raise ValueError('running daemon does not match isolated binary/socket')
    versions = {kind: command([manifest[kind], '--version']) for kind in ('ego', 'secondary')}
    versions.update({'python':sys.version.split()[0], 'git':command(['git','--version'])})
    provider = manifest['egoModel'].split('/',1)[0]
    catalogue = json.loads(command([manifest['ego'],'models','--provider',provider,'--json']))
    selected = next((m for m in catalogue['models'] if m['slug']==manifest['egoModel']),None)
    if not selected or selected.get('availability',{}).get('status')!='available':
        raise ValueError('exact ego model is not available in the installed catalogue')
    (output/'model-admission.json').write_text(json.dumps(selected,indent=2)+'\n')
    runtime = Path(manifest['egoRuntime']).resolve(strict=True)
    ego = Path(manifest['ego']).resolve(strict=True)
    provenance = {'experiment': manifest['experiment'],
                  'runtimeSha256': hashlib.sha256(runtime.read_bytes()).hexdigest(),
                  'secondaryRuntimeSha256': hashlib.sha256(Path(manifest['secondary']).resolve(strict=True).read_bytes()).hexdigest(),
                  'egoBinary': {'path': manifest['ego'], 'resolvedPath': str(ego),
                                'sha256': hashlib.sha256(ego.read_bytes()).hexdigest(),
                                'version': versions['ego']},
                  'models': [manifest['egoModel'], manifest['secondaryModel']],
                  'candidate': candidate, 'binarySha256': binary_hash,
                  'health': health, 'versions': versions,
                  'harnessCommit': command(['git', 'rev-parse', 'HEAD'], ROOT),
                  'candidateTree': command(['git', 'rev-parse', candidate + '^{tree}'], ROOT),
                  'command': sys.argv, 'startedUtc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  'harnessFiles': {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                      for p in sorted(Path(__file__).parent.glob('*.py'))}}
    (output / 'provenance.json').write_text(json.dumps(provenance, indent=2) + '\n')
    return manifest, client, output


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--candidate', required=True)
    parser.add_argument('--manifest', required=True)
    parser.add_argument('--evidence', required=True)
    parser.add_argument('--case', choices=LOCAL, required=True)
    parser.add_argument('--preflight-only', action='store_true')
    args = parser.parse_args()
    output = None
    context = {}
    try:
        manifest, client, output = preflight(args, context)
        if args.preflight_only:
            print('PREFLIGHT PASS (no scenario executed)')
            return 0
        from local import execute
        primary = None
        try:
            execute(args.case, client, manifest, output)
        except Exception as error:
            for run_id in client.started_runs:
                try:
                    state=client.run('get',run_id=run_id)
                    for attempt in state['attempts']:
                        agent=attempt.get('agent')
                        if agent and not agent.get('acp'):
                            client.request('GET','/sessions/'+agent['sessionId']+'/output')
                except Exception as diagnostic_error:
                    (output/'diagnostic-error.txt').write_text(str(diagnostic_error))
            primary = error
            raise
        finally:
            try:
                client.close_owned()
            except Exception as cleanup_error:
                (output / 'cleanup-error.json').write_text(json.dumps({
                    'reason': str(cleanup_error), 'runs': client.started_runs}, indent=2)+'\n')
                if primary is None:
                    raise
        result = {'case': args.case, 'status': 'PASS',
                  'scope': 'implemented assertions only', 'familyStatus': 'INCOMPLETE',
                  'note': 'No family or preservation-row PASS without its complete evidence ledger'}
        code = 0
    except Exception as error:
        output = output or context.get('output')
        result = {'case': args.case, 'status': 'FAIL', 'reason': str(error)}
        if output:
            (output / 'traceback.txt').write_text(traceback.format_exc())
        code = 1
    result['finishedUtc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    if output:
        (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result), flush=True)
    return code


if __name__ == '__main__':
    sys.exit(main())
