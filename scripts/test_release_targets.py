"""Publication contracts exercised against local bare Git remotes only."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
TOOL = Path.home() / 'Gits/personal/orchestrator/tools/release/snapshot.py'


class ReleaseTargets(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=os.environ['TUIC_TEST_TMP_ROOT'])
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.repo = self.base / 'repo'
        self.remote = self.base / 'public.git'
        self.repo.mkdir()
        self.run_cmd('git', 'init', '--bare', str(self.remote))
        self.git('init', '-b', 'main')
        self.git('config', 'user.name', 'Release Fixture')
        self.git('config', 'user.email', 'fixture@example.invalid')
        self.git('config', 'commit.gpgsign', 'false')
        self.git('config', 'tag.gpgsign', 'false')
        shutil.copy(ROOT / 'Makefile', self.repo)
        (self.repo / 'scripts').mkdir()
        if (ROOT / 'scripts/publish-snapshot.py').exists():
            shutil.copy(ROOT / 'scripts/publish-snapshot.py', self.repo / 'scripts')
        (self.repo / 'src-tauri').mkdir()
        (self.repo / 'src-tauri/Cargo.toml').write_text('[workspace.package]\nversion = "2.0.1"\n')
        (self.repo / 'src-tauri/tauri.conf.json').write_text('{"version":"2.0.1"}')
        self.git('add', '.')
        self.git('commit', '-m', 'public baseline')
        self.public = self.git('rev-parse', 'HEAD')
        self.git('remote', 'add', 'origin', str(self.remote))
        self.git('push', 'origin', 'main')
        (self.repo / 'stories').mkdir()
        (self.repo / 'stories/private.md').write_text('Private development record\n')
        (self.repo / 'app.txt').write_text('new public behavior\n')
        self.git('add', '.')
        self.git('commit', '-m', 'private development')
        self.private = self.git('rev-parse', 'HEAD')
        self.notes = self.base / 'notes.json'
        self.notes.write_text(json.dumps({'Fixes': ['Preserve saved preferences']}))

    def run_cmd(self, *args, ok=True):
        result = subprocess.run(args, cwd=self.repo, text=True, capture_output=True)
        if ok:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def git(self, *args):
        return self.run_cmd('git', *args).stdout.strip()

    def remote_ref(self, name):
        return self.git('ls-remote', 'origin', name).split()[0]

    def make(self, target, *args, ok=True):
        return self.run_cmd('make', target, f'SNAPSHOT_TOOL={TOOL}',
                            f'RELEASE_NOTES={self.notes}', *args, ok=ok)

    def assert_snapshot(self, commit, parent):
        self.assertEqual(self.git('show', '-s', '--format=%P', commit), parent)
        self.assertNotIn('stories/', self.git('ls-tree', '-r', '--name-only', commit))
        self.assertEqual(self.remote_ref('refs/heads/main'), self.public)
        result = self.run_cmd('git', 'merge-base', '--is-ancestor', self.private, commit, ok=False)
        self.assertEqual(result.returncode, 1, 'private development ancestry reached public remote')

    # Catches: make nightly sends private main and moves a different tag.
    def test_nightly_keeps_main_private_and_moves_only_nightly_snapshot_and_tag(self):
        self.make('nightly', 'NIGHTLY_DATE=2026-11-02')
        self.assertEqual(self.remote_ref('refs/heads/main'), self.public,
                         'make nightly published private main')
        commit = self.remote_ref('refs/heads/nightly')
        self.assert_snapshot(commit, self.public)
        self.assertEqual(self.remote_ref('refs/tags/nightly'), commit)
        self.assertEqual(self.git('ls-remote', 'origin', 'refs/tags/tip'), '')
        self.make('nightly', 'NIGHTLY_DATE=2026-11-03')
        self.assert_snapshot(self.remote_ref('refs/heads/nightly'), commit)
        self.assertNotEqual(self.make('nightly', 'NIGHTLY_DATE=2026-11-03', ok=False).returncode, 0)

    # Catches: nightly publishes a clean, unmerged branch instead of private main.
    def test_nightly_never_publishes_unmerged_head_instead_of_private_main(self):
        self.git('branch', 'unmerged-preview')
        self.git('symbolic-ref', 'HEAD', 'refs/heads/unmerged-preview')
        (self.repo / 'app.txt').write_text('unfinished feature, not merged into main\n')
        self.git('add', 'app.txt')
        self.git('commit', '-m', 'unfinished feature')
        before = self.git('ls-remote', 'origin')
        result = self.make('nightly', 'NIGHTLY_DATE=2026-11-02', ok=False)
        if result.returncode:
            self.assertEqual(self.git('ls-remote', 'origin'), before)
        else:
            snapshot = self.remote_ref('refs/heads/nightly')
            self.assertEqual(self.git('show', f'{snapshot}:app.txt'),
                             'new public behavior',
                             'nightly published unmerged HEAD instead of private main')

    # Catches: release tags private HEAD before the public PR is integrated.
    def test_release_tags_only_the_integrated_snapshot(self):
        self.make('github-release')
        commit = self.remote_ref('refs/heads/release/2.0.1')
        self.assert_snapshot(commit, self.public)
        self.assertEqual(self.git('ls-remote', 'origin', 'refs/tags/v2.0.1'), '')
        self.assertNotEqual(self.make('github-release', f'RELEASE_COMMIT={commit}', ok=False).returncode, 0)
        self.assertEqual(self.git('ls-remote', 'origin', 'refs/tags/v2.0.1'), '')
        self.git('push', 'origin', f'{commit}:refs/heads/main')
        self.make('github-release', f'RELEASE_COMMIT={commit}')
        self.assertEqual(self.remote_ref('refs/tags/v2.0.1'), commit)
        self.assertEqual(self.git('show', '-s', '--format=%B', commit),
                         'release: tuicommander 2.0.1\n\nFixes: Preserve saved preferences')

    # Catches: a target trusts snapshot creation and transmits private ancestors.
    def test_both_targets_guard_creation_output_before_sending_private_ancestry(self):
        adapter = self.base / 'creation-fault.py'
        for target, subject in (('nightly', 'nightly: 2026-11-02'),
                                ('github-release', 'release: tuicommander 2.0.1\n\nFixes: Preserve preferences')):
            with self.subTest(target=target):
                self.git('commit', '--allow-empty', '-m', subject)
                candidate = self.git('rev-parse', 'HEAD')
                # Inject only a creation fault; the real shared guard and push run.
                adapter.write_text(
                    'import json, subprocess, sys\n'
                    "if sys.argv[1] in ('snapshot', 'nightly'):\n"
                    f" print(json.dumps({{'commit': '{candidate}', 'ref': "
                    f"'refs/heads/{'nightly' if target == 'nightly' else 'release/2.0.1'}'}}))\n"
                    'else:\n'
                    f' sys.exit(subprocess.call([sys.executable, {str(TOOL)!r}, *sys.argv[1:]]))\n')
                before = self.git('ls-remote', 'origin')
                result = self.make(target, f'SNAPSHOT_TOOL={adapter}', ok=False)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('sole parent', result.stderr)
                self.assertEqual(self.git('ls-remote', 'origin'), before)

    # Catches: either target bypasses the shared scanner after creation fails.
    def test_both_targets_refuse_secret_content_without_changing_remote_refs(self):
        (self.repo / 'leak.txt').write_text('-----BEGIN ' + 'PRIVATE KEY-----\n')
        self.git('add', '.')
        self.git('commit', '-m', 'unsafe content')
        before = self.git('ls-remote', 'origin')
        for target in ('nightly', 'github-release'):
            with self.subTest(target=target):
                result = self.make(target, ok=False)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('secret pattern', result.stderr)
                self.assertEqual(self.git('ls-remote', 'origin'), before)


if __name__ == '__main__':
    unittest.main()
