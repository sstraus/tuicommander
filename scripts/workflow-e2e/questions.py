"""Branch-local decisions and timer expiry with real workers."""
import json
import uuid
import time
from pathlib import Path
from client import new_project, new_story, publish, start, fresh_command
from local import prompt, routing, review_contract


def await_question_idle(client, run, output):
    attempt=next(a for a in run['attempts'] if a['nodeId']=='ask')
    binding=attempt['agent']['acp']
    setup_deadline=time.monotonic()+120
    while time.monotonic()<setup_deadline:
        snapshot=client.request('GET','/acp/connections/'+binding['connectionId'])
        seats=[a for a in snapshot['attachments'] if a['sessionId']==binding['conversationId']]
        if len(seats)==1:
            seat=seats[0]
            turn=seat.get('activeTurn')
            if (seat['state']=='idle' and (not turn or turn['state']=='settled') and
                not seat['queuedPrompts'] and not seat['pendingPermissionIds'] and not seat['pendingElicitationIds']):
                (output/'question-acp-idle.json').write_text(json.dumps(snapshot,indent=2)+'\n')
                return
        time.sleep(0.1)
    raise RuntimeError('question ACP did not become quiescent during setup')


def parallel_graph(deadline, manifest):
    question = ('You are a read-only reviewer. Use ONLY the workflow MCP report action to ask this question. '
                'Do not call ask_question, elicitation, ui, or any native question tool. '
                'Submit a typed needs_input report and end your turn; the workflow delivers the answer later. '
                'Before reviewing, report needs_input with inputRequest question '
                '"Which color?", options ["blue","red"] and answerSchema '
                '{"type":"single_choice","options":["blue","red"]}. '
                'End the ACP turn after the needs_input report; do not block or wait in a tool. The host will deliver the answer later. After the answer, independently inspect seed.txt '
                'and submit a completed typed review with the chosen color in the summary, '
                'the supplied artifact digest and actual byte evidence. Never edit or commit. ' + review_contract())
    nodes = [
        {'id': 'start', 'kind': {'type': 'start'}},
        {'id': 'seed', 'kind': {'type': 'agent', 'role': 'implementer',
                              'capabilities': ['story_read', 'story_report'], 'execution': None,
                              'prompt_template': prompt('seed.txt', 'original')}},
        {'id': 'fork', 'kind': {'type': 'fork', 'join': 'join', 'repair_to': None,
                               'branches': [{'id': 'ask', 'target': 'ask', 'required': True},
                                            {'id': 'work', 'target': 'work', 'required': True}]}},
        {'id': 'ask', 'kind': {'type': 'agent', 'role': 'reviewer',
                              'capabilities': ['story_read', 'story_report'],
                              'execution': {'transport':'acp','profile':'e2e',
                                            'model':manifest['egoModel'],'effort':'medium'},
                              'prompt_template': question}},
        {'id': 'work', 'kind': {'type': 'agent', 'role': 'reviewer',
                               'capabilities': ['story_read', 'story_report'],
                               'execution': {'transport': 'pty', 'profile': manifest['secondaryProfile']},
                               'prompt_template': 'Independently read seed.txt and verify exact bytes original followed by newline. '
                                  'Submit a typed completed review with actual commit/tree and supplied artifact digest. '
                                  'Do not wait for another reviewer or any operator answer. Never edit, commit, push or launch agents.' + review_contract()}},
        {'id': 'join', 'kind': {'type': 'all_join', 'fork': 'fork'}}]
    edges = [{'from': 'start', 'to': 'seed', 'outcome': None},
             {'from': 'seed', 'to': 'fork', 'outcome': None},
             {'from': 'ask', 'to': 'join', 'outcome': None},
             {'from': 'work', 'to': 'join', 'outcome': None}]
    for outcome in ('completed', 'failed', 'cancelled', 'skipped'):
        nodes.append({'id': outcome, 'kind': {'type': 'end'}})
        edges.append({'from': 'join', 'to': outcome, 'outcome': outcome})
    return {'nodes': nodes, 'edges': edges}


def sibling_exists(run):
    return any(a['nodeId']=='work' and a['state']=='reported' and a['outcome']=='completed'
               and a.get('report',{}).get('review') for a in run['attempts'])


# Catches: a pending activation question globally stopping an independent live
# worker, or stale/duplicate answers advancing an activation more than once.
def question_case(client, manifest, output, deadline=False):
    plan = new_project(client, output / 'repository')
    routing(client, manifest)
    story = new_story(client, plan, 'Independent question branches',
                      ['seed.txt contains exactly original followed by newline'], ['seed.txt'])
    definition = publish(client, parallel_graph(deadline, manifest))
    run = start(client, story, definition, str(uuid.uuid4()))
    waiting = client.await_run(run['id'], lambda r: bool(r.get('questions')))
    question = waiting['questions'][0]
    assert question['state'] == 'pending', question
    assert question['request']['question'] == 'Which color?', question
    sibling = client.await_run(run['id'], lambda r: sibling_exists(r) and any(
        a['nodeId'] == 'work' and a['state'] == 'reported' and a['outcome'] == 'completed'
        for a in r['attempts']), allow_questions=True)
    workspace = next(s['worktreePath'] for s in sibling['stories'] if s.get('worktreePath'))
    assert (Path(workspace) / 'seed.txt').read_bytes() == b'original\n'
    assert not (Path(workspace) / 'question.txt').exists()
    if deadline:
        # Arm the behaviour deadline only after both real branches have reached setup.
        current = client.run('get', run_id=run['id'])
        request = dict(question['request'])
        request['deadline'] = {'atMs':int(time.time()*1000)+30000,'outcome':'timed_out'}
        fresh_command(client, run['id'], 'arm-question-deadline', {
                       'action':'revise_question','question_id':question['id'],
                       'expected_revision':question['revision'],'request':request,
                       'actor':{'kind':'human','id':'workflow-e2e-operator'}})
        from daemon_control import restart
        restart(client,manifest)
        retained = client.run('get',run_id=run['id'])
        saved = max((q for q in retained['questions'] if q['id']==question['id']),key=lambda q:q['revision'])
        assert saved['revision']==question['revision']+1,saved
        assert saved['request']['deadline']['atMs']==request['deadline']['atMs']
        # Catches: a Timer deadline becoming implicit consent or disappearing.
        expired = client.await_run(run['id'], lambda r: any(
            q['state'] == 'timed_out' for q in r.get('questions', [])), allow_questions=True)
        assert not (Path(workspace) / 'question.txt').exists()
        fired = [t for t in expired.get('timers', []) if t['state']=='fired' and
                 t['purpose'].get('question_id')==question['id']]
        assert len(fired)==1, expired
        assert not any(a['nodeId']=='ask' and a['outcome']=='completed' for a in expired['attempts'])
        assert not any(s['accepted'] or s.get('integrationReceipt') for s in expired['stories'])
        observation = expired
    else:
        if manifest['experiment'].get('answerTiming','immediate')=='after_acp_idle':
            await_question_idle(client,sibling,output)
        answer = {'action': 'answer_question', 'question_id': question['id'],
                  'expected_revision': question['revision'],
                  'subject_digest': question['subjectDigest'],
                  'actor': {'kind': 'human', 'id': 'workflow-e2e-operator'}, 'answer': 'blue'}
        current = client.run('get', run_id=run['id'])
        stale = dict(answer, expected_revision=question['revision'] + 10)
        try:
            client.run('command', run_id=run['id'], command_id='stale-answer',
                       expected_sequence=current['sequence'], command=stale)
        except RuntimeError as error:
            assert 'question revision or state is stale' in str(error),error
        else:
            raise AssertionError('stale question answer advanced the activation')
        receipt,sequence = fresh_command(client,run['id'],'answer-blue',answer)
        repeated = client.run('command', run_id=run['id'], command_id='answer-blue',
                              expected_sequence=sequence, command=answer)
        assert repeated == receipt, (receipt, repeated)
        observation = client.await_run(run['id'], lambda r: any(
            a['nodeId']=='ask' and a['outcome']=='completed' for a in r['attempts']))
        assert any(a['nodeId']=='ask' and a.get('report') and
                   'blue' in a['report']['summary'].lower() for a in observation['attempts'])
        assert (Path(workspace)/'seed.txt').read_bytes()==b'original\n'
        assert len([q for q in observation['questions'] if q.get('answer')]) == 1
    (output / 'question-observation.json').write_text(json.dumps(observation, indent=2)+'\n')
