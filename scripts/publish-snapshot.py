#!/usr/bin/env python3
"""Wire Make publication targets to the shared public-snapshot policy."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys


def run(*args):
    return subprocess.run(args, check=True, text=True, capture_output=True).stdout.strip()


def publish(args):
    repo = run('git', 'rev-parse', '--show-toplevel')

    def git(*argv):
        return run('git', '-C', repo, *argv)

    def tool(action, *argv):
        return run(sys.executable, args.tool, action, '--repo', repo,
                   '--name', 'tuicommander', *argv)

    def remote_ref(ref):
        result = git('ls-remote', 'origin', ref).split()
        return result[0] if result else ''

    # Refresh only public refs; never merge them into private development main.
    git('fetch', '--no-tags', 'origin',
        '+refs/heads/main:refs/remotes/origin/main')
    if remote_ref('refs/heads/nightly'):
        git('fetch', '--no-tags', 'origin',
            '+refs/heads/nightly:refs/remotes/origin/nightly')
    else:
        git('update-ref', '-d', 'refs/remotes/origin/nightly')

    if args.commit:
        if args.action != 'release':
            raise ValueError('only a release has a separate tagging phase')
        commit = git('rev-parse', '--verify', args.commit + '^{commit}')
        public = git('rev-parse', 'refs/remotes/origin/main')
        if commit != public:
            raise ValueError('public main must fast-forward to the reviewed snapshot before tagging')
        subject = git('show', '-s', '--format=%s', commit)
        if subject != f'release: tuicommander {args.version}':
            raise ValueError('release version does not match the integrated snapshot')
        destination = f'refs/tags/v{args.version}'
    else:
        if git('status', '--porcelain'):
            raise ValueError('working tree is dirty; commit the reviewed source first')
        source = git('rev-parse', 'refs/heads/main' if args.action == 'nightly' else 'HEAD')
        if args.action == 'nightly':
            options = ['--date', args.date] if args.date else []
            snapshot = json.loads(tool('nightly', '--source', source, *options))
        else:
            if not args.notes:
                raise ValueError('provide RELEASE_NOTES with reviewed grouped JSON notes')
            snapshot = json.loads(tool('snapshot', '--source', source,
                                      '--version', args.version, '--notes', args.notes))
        commit, destination = snapshot['commit'], snapshot['ref']

    tool('push', '--commit', commit, '--destination', destination)
    if args.action == 'nightly':
        # The shared tag guard covers public main. A nightly tag instead points
        # only to the exact snapshot just accepted on the public nightly branch.
        if remote_ref('refs/heads/nightly') != commit:
            raise ValueError('public nightly changed before its tag could move')
        tag = 'refs/tags/nightly'
        previous = remote_ref(tag)
        git('-c', 'remote.origin.mirror=false', 'push', '--no-follow-tags',
            '--no-mirror', '--recurse-submodules=no',
            f'--force-with-lease={tag}:{previous}', 'origin', f'{commit}:{tag}')
    print(json.dumps({'commit': commit, 'ref': destination}))
    if args.action == 'release' and not args.commit:
        print(f'Open a PR from release/{args.version} to public main; merge after CI.')
        print(f'After main fast-forwards, run make github-release RELEASE_COMMIT={commit}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['nightly', 'release'])
    parser.add_argument('--tool', default=str(Path.home() /
                        'Gits/personal/orchestrator/tools/release/snapshot.py'))
    parser.add_argument('--notes', default='')
    parser.add_argument('--date', default='')
    parser.add_argument('--version', default='')
    parser.add_argument('--commit', default='')
    args = parser.parse_args()
    if args.action == 'release' and not args.version:
        args.version = json.loads(Path('src-tauri/tauri.conf.json').read_text())['version']
    try:
        publish(args)
    except subprocess.CalledProcessError as error:
        print(error.stderr.strip() or 'publication command failed', file=sys.stderr)
        return 1
    except (ValueError, OSError) as error:
        print(f'publication: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
