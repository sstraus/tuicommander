"""Negative admission cases: each must fail before touching any daemon."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class PreflightAdmission(unittest.TestCase):
    def rejected(self, manifest, reason):
        scratch = ROOT/'.tmp/workflow-e2e-preflight'
        scratch.mkdir(parents=True,exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch) as directory:
            path=Path(directory)/'manifest.json'
            path.write_text(json.dumps(manifest))
            evidence=Path(directory)/'evidence'
            candidate=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
            result=subprocess.run(['bash',str(ROOT/'scripts/workflow-e2e.sh'),
                '--candidate',candidate,'--manifest',str(path),'--case','E24',
                '--evidence',str(evidence),'--preflight-only'],capture_output=True,text=True)
            self.assertEqual(result.returncode,1,result.stdout+result.stderr)
            self.assertIn(reason,result.stdout)
            self.assertFalse(evidence.exists(),'rejected manifest created execution evidence')

    # Catches: verifying one tree while the launch manifest names another.
    def test_candidate_mismatch(self):
        self.rejected({'candidate':'0'*40},'manifest candidate differs')

    # Catches: misspelled setup silently replacing the early-answer regression variant.
    def test_unknown_answer_timing(self):
        candidate=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
        self.rejected({'candidate':candidate,'budgetExpiresMs':9999999999999,
                       'experiment':{'answerTiming':'quiet'}},'unknown answerTiming variant')

    # Catches: accidentally probing the daily/orchestrator namespace.
    def test_daily_instance(self):
        candidate=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
        self.rejected({'candidate':candidate,'budgetExpiresMs':9999999999999,'environment':{'TUIC_APP_INSTANCE':'default'}},
                      'requires a named agent-test instance')

    # Catches: a safe instance label hiding the production socket.
    def test_socket_mismatch(self):
        candidate=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
        self.rejected({'candidate':candidate,'budgetExpiresMs':9999999999999,'environment':{'TUIC_APP_INSTANCE':'agent-test-test'},
                       'socket':'/not-an-isolated-socket'},'socket does not match')


if __name__=='__main__':
    unittest.main()
