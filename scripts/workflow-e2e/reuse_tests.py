"""Exact-artifact admission for native restart evidence; no process launches."""
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('agent_test',ROOT/'scripts/agent-test.py')
agent_test=importlib.util.module_from_spec(spec)
spec.loader.exec_module(agent_test)


class ReuseAdmission(unittest.TestCase):
    def setUp(self):
        root=ROOT/'.tmp/workflow-e2e-preflight'
        root.mkdir(parents=True,exist_ok=True)
        self.directory=tempfile.TemporaryDirectory(dir=root)
        self.addCleanup(self.directory.cleanup)
        self.path=Path(self.directory.name)
        for name in (agent_test.BINARY,'tuic-bridge','tuic'):
            (self.path/name).write_bytes(name.encode())
        self.launch={'binary':str(self.path/agent_test.BINARY)}
        self.digest=hashlib.sha256((self.path/agent_test.BINARY).read_bytes()).hexdigest()

    # Catches: copying/replacing a pinned executable during an intended restart.
    def test_exact_bytes_accepted_without_modification(self):
        before={p.name:p.stat().st_ino for p in self.path.iterdir()}
        actual=agent_test.check_reused_build(self.launch,self.digest)
        self.assertEqual(actual[agent_test.BINARY],self.digest)
        self.assertEqual(before,{p.name:p.stat().st_ino for p in self.path.iterdir()})

    # Catches: running a stale or replaced binary under another candidate's receipt.
    def test_wrong_digest_refuses(self):
        with self.assertRaisesRegex(RuntimeError,'differs from the expected'):
            agent_test.check_reused_build(self.launch,'0'*64)

    # Catches: a good daemon hash hiding an absent transport executable.
    def test_missing_bridge_refuses(self):
        (self.path/'tuic-bridge').unlink()
        with self.assertRaisesRegex(RuntimeError,'tuic-bridge'):
            agent_test.check_reused_build(self.launch,self.digest)

    # Catches: reuse following an artifact symlink outside the isolated directory.
    def test_linked_artifact_refuses(self):
        (self.path/'tuic').unlink()
        (self.path/'tuic').symlink_to(self.path/agent_test.BINARY)
        with self.assertRaisesRegex(RuntimeError,'regular artifact tuic'):
            agent_test.check_reused_build(self.launch,self.digest)


if __name__=='__main__':
    unittest.main()
