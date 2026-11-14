"""Immutable Call pins observed through the real workflow executor."""
import json
import uuid
from pathlib import Path
from client import graph_completed, command, graph_agent, new_project, new_story, publish, start
from local import prompt, routing


def returned(outcome):
    return {'nodes': [{'id': 'start', 'kind': {'type': 'start'}},
                      {'id': 'result', 'kind': {'type': 'return', 'outcome': outcome,
                                              'outputs': {'accepted': True}}}],
            'edges': [{'from': 'start', 'to': 'result', 'outcome': None}]}


# Catches: a current child publication replacing a run's immutable transitive
# pin, or a repeated root request duplicating the owned Call execution.
def pinned_call(client, manifest, output):
    plan = new_project(client, output / 'repository')
    routing(client, manifest)
    story = new_story(client, plan, 'Call pin proof', ['pin.txt contains original'], ['pin.txt'])
    child = publish(client, returned('completed'))
    graph = graph_agent(prompt('pin.txt', 'original'))
    graph['nodes'] = [n for n in graph['nodes'] if n['id'] != 'end']
    graph['nodes'] += [
        {'id': 'hold', 'kind': {'type': 'pause', 'resume_to': 'call'}},
        {'id': 'call', 'kind': {'type': 'call', 'definition_id': child['id'],
                               'revision': child['revision'], 'inputs': {},
                               'outputs': ['accepted']}}]
    graph['nodes'].append({'id':'route','kind':{'type':'if_else',
        'predicate':{'source':'input','name':'hold_for_publication','value':True}}})
    graph['edges'][-1]['to'] = 'route'
    graph['edges'] += [
        {'from':'route','to':'hold','outcome':'yes'},
        {'from':'route','to':'call','outcome':'no'},
        {'from':'route','to':'call','outcome':'uncertain'}]
    # Call is statically reachable; the actual completed author selects Pause.
    for outcome in ('completed', 'blocked', 'failed', 'cancelled', 'uncertain'):
        graph['nodes'].append({'id': outcome, 'kind': {'type': 'end'}})
        graph['edges'].append({'from': 'call', 'to': outcome, 'outcome': outcome})
    root = publish(client, graph)
    key = str(uuid.uuid4())
    initial = start(client, story, root, key)
    held = client.await_run(initial['id'], lambda r: r['status'] == 'paused' and any(
        a['nodeId']=='hold' for g in r['graphExecutions'] for a in g['activations']))
    workspace = next(s['worktreePath'] for s in held['stories'] if s.get('worktreePath'))
    assert (Path(workspace) / 'pin.txt').read_text() == 'original\n'
    commit = command(['git', 'rev-parse', 'HEAD'], workspace)
    assert command(['git', 'show', f'{commit}:pin.txt'], workspace) == 'original'
    draft = client.definition('get_draft', id=child['id'])
    updated = client.definition('update_draft', id=child['id'],
                                expected_revision=draft['draftRevision'], graph=returned('failed'))
    client.definition('publish', id=child['id'], expected_revision=updated['draftRevision'])
    execution = held['graphExecutions'][0]
    activation = next(a for a in execution['activations'] if a['nodeId'] == 'hold')
    client.run('command', run_id=held['id'], command_id='release-pinned-call',
               expected_sequence=held['sequence'], command={'action': 'resume_graph',
               'execution_id': execution['id'], 'activation_id': activation['id'],
               'resolution': 'Continue the already-pinned child revision'})
    done = client.await_run(initial['id'], graph_completed)
    assert len(done['graphExecutions']) == 2, done
    assert done['graphExecutions'][1]['definition'] == child, done
    assert any(a['nodeId'] == 'completed' for a in done['graphExecutions'][0]['activations'])
    retry = start(client, story, root, key)
    assert retry['id'] == done['id']
    assert len(retry['graphExecutions']) == 2
    (output / 'pin-proof.json').write_text(json.dumps({
        'sourceCommit': commit, 'child': child, 'finished': done, 'retried': retry}, indent=2)+'\n')
