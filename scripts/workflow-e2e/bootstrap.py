"""Freeze harness bytes before execution so active runs survive development edits."""
import hashlib
import os
from pathlib import Path
import shutil
import sys
import tempfile

root=Path(__file__).resolve().parents[2]
source=Path(__file__).parent
files={p.name:p.read_bytes() for p in sorted(source.glob('*.py'))}
digest=hashlib.sha256(b''.join(name.encode()+b'\0'+body for name,body in files.items())).hexdigest()
cache=root/'.tmp/workflow-e2e-harness'
cache.mkdir(parents=True,exist_ok=True)
target=cache/digest
if not target.exists():
    staging=Path(tempfile.mkdtemp(dir=cache))
    try:
        for name,body in files.items():(staging/name).write_bytes(body)
        try:staging.rename(target)
        except FileExistsError:pass
    finally:
        if staging.exists():shutil.rmtree(staging)
if any((target/name).read_bytes()!=body for name,body in files.items()):
    raise SystemExit('Existing harness snapshot differs')
os.environ['TUIC_WORKFLOW_E2E_ROOT']=str(root)
args=sys.argv[1:]
entry='run.py'
if args and args[0]=='--isolated':entry='isolated.py';args=args[1:]
os.execv(sys.executable,[sys.executable,'-B',str(target/entry),*args])
