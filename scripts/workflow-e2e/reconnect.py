"""Reconnect and duplicate-start proof against a live, independently running agent."""
import json
import uuid
from pathlib import Path
from client import Client, command, graph_agent, graph_completed, new_project, new_story, publish, start
from local import prompt, routing


def reconnect_case(client, manifest, output):
    plan = new_project(client, output/'repository')
    routing(client, manifest)
    story = new_story(client, plan, 'Reconnect author', ['result.txt contains retained'], ['result.txt'])
    definition = publish(client, graph_agent(prompt('result.txt','retained')))
    key = str(uuid.uuid4())
    run = start(client,story,definition,key)
    bound = client.await_run(run['id'],lambda r:any(a.get('agent') for a in r['attempts']))
    from daemon_control import restart
    restart(client,manifest)
    restored_state = client.run('get',run_id=run['id'])
    assert restored_state['id']==bound['id']
    assert len(restored_state['attempts'])==len(bound['attempts'])
    assert not any(s['accepted'] for s in restored_state['stories'])
    (output/'after-daemon-restart.json').write_text(json.dumps(restored_state,indent=2)+'\n')
    # New HTTP/MCP client, same real store; no synthetic binding.
    restored = Client(client.socket,output/'reconnected')
    restored.evidence.mkdir()
    restored.project = client.project
    same = start(restored,story,definition,key)
    assert same['id']==run['id']
    done = restored.run('get',run_id=run['id'])
    if not graph_completed(done):
        assert done['status'] in ('paused','interrupted','running'), done
        assert not any(s['accepted'] or s.get('integrationReceipt') for s in done['stories'])
        (output/'reconnect-observation.json').write_text(json.dumps({
            'bound':bound,'reconnected':same,'interrupted':done},indent=2)+'\n')
        return
    assert len(done['attempts'])==1 and done['spawns']==1, done
    workspace=done['stories'][0]['worktreePath']
    assert (Path(workspace)/'result.txt').read_bytes()==b'retained\n'
    commit=command(['git','rev-parse','HEAD'],workspace)
    assert command(['git','show',f'{commit}:result.txt'],workspace)=='retained'
    (output/'reconnect-observation.json').write_text(json.dumps({
        'bound':bound,'reconnected':same,'finished':done,'commit':commit},indent=2)+'\n')
