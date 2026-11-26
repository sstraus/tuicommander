#!/usr/bin/env python3
"""Replay the captured Ego CLI boundary against isolated per-profile state."""
import hashlib
import json
import os
import pathlib
import sys
import tomllib

root = pathlib.Path(__file__).parent
args = sys.argv[1:]
with (root / 'calls.jsonl').open('a') as log:
    log.write(json.dumps({'args': args, 'cwd': os.getcwd()}) + '\n')
if (root / 'fail').exists():
    print('canary-ego-secret-in-diagnostic', file=sys.stderr)
    sys.exit(2)
profile = next((a.split('=', 1)[1] for a in args if a.startswith('--profile=')), None)
if args[:2] == ['config', 'capabilities']:
    print((root / 'capabilities.json').read_text())
elif args[:2] in [['config', op] for op in ['ls', 'snapshot', 'batch', 'set', 'unset']]:
    path = root / ((profile or 'base') + '.json')
    data = json.loads(path.read_text())
    def revision():
        return hashlib.sha256(path.read_bytes()).hexdigest()
    if args[1] == 'snapshot':
        print(json.dumps(dict(data, revision=revision())))
    elif args[1] == 'batch':
        if (root / 'race-before-batch').exists():
            data['values']['model'] = 'ollama/external'
            path.write_text(json.dumps(data))
        if args[args.index('--if-revision') + 1] != revision():
            print((root / 'conflict.json').read_text())
            sys.exit(1)
        for i, arg in enumerate(args):
            if arg == '--set':
                key, raw = args[i + 1].split('=', 1)
                value = tomllib.loads('value=' + raw)['value']
                data['values'][key] = '<redacted>' if key.startswith('auth.') else value
            elif arg == '--unset':
                data['values'].pop(args[i + 1], None)
        path.write_text(json.dumps(data))
        print(json.dumps({'v':1,'changed':True,'publication':'durable'}))
        if (root / 'fail-after-write').exists():
            (root / 'fail').touch()
    elif args[1] == 'ls':
        print(json.dumps(data))
    elif args[1] == 'set':
        assignment = args[-1]
        key, raw = assignment.split('=', 1)
        # The managed listing uses flattened canonical keys.
        value = tomllib.loads('value=' + raw)['value']
        data['values'][key] = value
        path.write_text(json.dumps(data))
        print('published_durably')
        if (root / 'fail-after-write').exists():
            (root / 'fail').touch()
    else:
        data['values'].pop(args[-1], None)
        path.write_text(json.dumps(data))
        print('published_durably')
elif args[0] in ['models', 'doctor']:
    print((root / (args[0] + '.json')).read_text())
elif args[:3] == ['auth', 'status', '--json']:
    print((root / 'auth.json').read_text(), end='')
else:
    sys.exit(2)
