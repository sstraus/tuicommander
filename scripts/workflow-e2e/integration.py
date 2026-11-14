"""Real Git integration idempotence in a disposable canonical repository."""
import json
import urllib.parse
from pathlib import Path
from client import command
from lifecycle import delivery_case


def integrate_case(client, manifest, output):
    finished = delivery_case(client, manifest, output)
    client.run('set_native_integration_policy', native_integration_enabled=True)
    story = finished['stories'][0]
    workspace = story['worktreePath']
    source = command(['git','rev-parse','HEAD'],workspace)
    base = command(['git','rev-parse','HEAD'],client.project)
    # Untracked canonical user data must survive the actual merge.
    sentinel = client.project/'unrelated-sentinel.txt'
    sentinel.write_text('retain exact user bytes\n')
    current = client.run('get',run_id=finished['id'])
    request = {'runId':current['id'],'storyId':story['storyId'],
               'commandId':'one-integration','expectedSequence':current['sequence'],
               'expectedBase':base,'expectedSource':source}
    denied=client.request('POST','/workflows/run/action?'+urllib.parse.urlencode({'path':str(client.project)}),
                          {'action':'integrate','request':{**request,'commandId':'dirty-refusal'}},expect_error=True)
    assert 'workflow check requires a clean worktree' in denied.get('error',''),denied
    assert command(['git','rev-parse','HEAD'],client.project)==base
    assert sentinel.read_bytes()==b'retain exact user bytes\n'
    command(['git','add','unrelated-sentinel.txt'],client.project)
    command(['git','commit','-m','test: retain unrelated canonical data'],client.project)
    base=command(['git','rev-parse','HEAD'],client.project)
    current=client.run('get',run_id=finished['id'])
    request.update(expectedBase=base,expectedSequence=current['sequence'])
    first = client.run('integrate',request=request)
    head = command(['git','rev-parse','HEAD'],client.project)
    assert first['merged'] and first['receiptRecorded'], first
    assert command(['git','rev-list','--parents','-n','1',head],client.project).split()==[head,base,source]
    assert (client.project/'result.txt').read_bytes()==b'correct\n'
    assert sentinel.read_bytes()==b'retain exact user bytes\n'
    second = client.run('integrate',request=request)
    assert second['receipt']==first['receipt'], (first,second)
    assert command(['git','rev-parse','HEAD'],client.project)==head
    obligations = client.run('integration_obligations')
    assert any(o['mergeCommit']==head for o in obligations), obligations
    (output/'integration-observation.json').write_text(json.dumps({
        'dirtyRefusal':denied,'first':first,'retried':second,'obligations':obligations,'head':head},indent=2)+'\n')
