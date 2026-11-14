"""Real author/reviewer delivery; assertions inspect commits and check receipts."""
import json
import uuid
from pathlib import Path
from client import command, graph_agent, new_project, new_story, publish, start, graph_completed, transition
from local import routing, review_contract


def reviewed_graph(manifest, repair=False):
    author = ('Implement exactly the story criteria, create only the named file, and commit it. '
              'Report typed completed with actual commit and byte evidence. No push or children or progress. '
              'Use criterionResults=[{index:0,satisfied:true,evidence:actual_byte_evidence}] and evidence=[commit_string]. ')
    if repair:
        author = ('This is real execution, never simulation. Read generation in scoped context. On generation 1, use builtin exec/write to actually write result.txt with wrong\n '
                  'and commit it, so the reviewer/check can reject. On a later repair attempt, '
                  'write exactly correct followed by a newline and commit the fix. '
                  'Before reporting, call builtin exec to inspect pwd, git status and git show HEAD:result.txt. '
                  'Report completed only after actual tool outputs prove the file is committed; include full git rev-parse HEAD SHA. '
                  'Never invent a commit or byte observation. Report the actual observed bytes; never conceal the first defect. '
                  'On generation 1 the intentionally wrong artifact still uses outcome=completed because the implementation attempt finished; '
                  'criterionResults[0].satisfied=false honestly records the defect. Do NOT use outcome=failed for this intentional fixture defect. '
                  'The independent reviewer must reject it and the graph must launch generation 2 to repair. ')
    author += ('You are explicitly authorized to use builtin write and exec for filesystem edits and git commits '
               'inside the assigned worktree. story_read/story_report are scoped workflow MCP capabilities, '
               'not a denial of those builtin tools. Submit criterionResults as [{index:0,satisfied:boolean,evidence:string}] '
               'and evidence as an array of strings. Never call progress. ')
    graph = graph_agent(author)
    graph['nodes'] += [
        {'id': 'review', 'kind': {'type': 'agent', 'role': 'reviewer',
          'execution': {'transport': 'pty', 'profile': manifest['secondaryProfile']},
          'capabilities': ['story_read', 'story_report'],
          'prompt_template': 'Independently inspect the actual committed file against story criteria. Do not edit. '
            'Report typed review with supplied artifact digest and findings. Reject wrong bytes; approve exact correct bytes. '
            'Do not push or launch agents.' + review_contract()}},
        {'id': 'route_submit', 'kind': {'type':'if_else', 'predicate':{'source':'input','name':'operator_submission','value':True}}},
        {'id': 'submit', 'kind': {'type': 'pause', 'resume_to': 'review'}},
        {'id': 'judge', 'kind': {'type': 'judge'}},
        {'id': 'repair', 'kind': {'type': 'loop', 'max_iterations': 1}},
        {'id': 'hold', 'kind': {'type': 'pause', 'resume_to': 'implement'}}]
    graph['edges'] = [
        {'from': 'start', 'to': 'implement', 'outcome': None},
        {'from': 'implement', 'to': 'route_submit', 'outcome': None},
        {'from':'route_submit','to':'submit','outcome':'yes'},
        {'from':'route_submit','to':'review','outcome':'no'},
        {'from':'route_submit','to':'review','outcome':'uncertain'},
        {'from': 'review', 'to': 'judge', 'outcome': None},
        {'from': 'judge', 'to': 'end', 'outcome': 'yes'},
        {'from': 'judge', 'to': 'repair', 'outcome': 'no'},
        {'from': 'judge', 'to': 'hold', 'outcome': 'uncertain'},
        {'from': 'repair', 'to': 'implement', 'outcome': 'repeat'},
        {'from': 'repair', 'to': 'hold', 'outcome': 'exhausted'}]
    return graph


def reviewed_definition(client, manifest, repair=False):
    definition = publish(client, reviewed_graph(manifest, repair))
    draft = client.definition('get_draft', id=definition['id'])
    if repair:
        draft = client.definition('update_checks', id=draft['id'],
            expected_revision=draft['draftRevision'], checks=[{'id':'bytes',
            'argv':['python3','-c',"from pathlib import Path; assert Path('result.txt').read_bytes()==b'correct\\n'"],
            'timeoutSecs':60}])
    draft = client.definition('update_closure', id=draft['id'],
        expected_revision=draft['draftRevision'], closure='human')
    return client.definition('publish', id=draft['id'], expected_revision=draft['draftRevision'])


EVIDENCE_POLICY = {'maxReviews': 2, 'maxRepairs': 1, 'maxStoryProposals': 16, 'requireRed': False,
                   'requireStrictClippy': False, 'nativeTargets': [], 'consumerScope': [],
                   'changedTests': []}


def automatic_graph(manifest):
    """Unattended delivery: the daemon runs the checks at the Gate and the Judge closes."""
    graph = reviewed_graph(manifest)
    graph['nodes'] = [n for n in graph['nodes'] if n['id'] not in ('route_submit', 'submit')]
    graph['nodes'].append({'id': 'gate', 'kind': {'type': 'gate'}})
    graph['nodes'][next(i for i, n in enumerate(graph['nodes']) if n['id'] == 'review')]['kind'][
        'prompt_template'] += ('When artifactSubject.evidencePolicy is present, include review.evidence '
                               'exactly as the report instructions above require. ')
    graph['evidencePolicy'] = EVIDENCE_POLICY
    graph['edges'] = [
        {'from': 'start', 'to': 'implement', 'outcome': None},
        {'from': 'implement', 'to': 'review', 'outcome': None},
        {'from': 'review', 'to': 'gate', 'outcome': None},
        {'from': 'gate', 'to': 'judge', 'outcome': 'pass'},
        {'from': 'gate', 'to': 'repair', 'outcome': 'fail'},
        {'from': 'judge', 'to': 'end', 'outcome': 'yes'},
        {'from': 'judge', 'to': 'repair', 'outcome': 'no'},
        {'from': 'judge', 'to': 'hold', 'outcome': 'uncertain'},
        {'from': 'repair', 'to': 'implement', 'outcome': 'repeat'},
        {'from': 'repair', 'to': 'hold', 'outcome': 'exhausted'}]
    return graph


# Catches: an automatic-closure workflow that cannot close a story without a person
# (missing submission, unrun checks) or closes it on an agent claim alone. The harness
# makes no story transition and runs no check; native history and Git are the oracle.
def automatic_case(client, manifest, output):
    plan = new_project(client, output/'repository')
    routing(client, manifest)
    draft = client.definition('create_draft', name='Workflow e2e automatic', kind='story',
                              graph=automatic_graph(manifest))
    draft = client.definition('update_checks', id=draft['id'], expected_revision=draft['draftRevision'],
        checks=[{'id': 'bytes', 'argv': ['python3', '-c',
                 "from pathlib import Path; assert Path('result.txt').read_bytes()==b'correct\\n'"],
                 'timeoutSecs': 60}])
    draft = client.definition('update_closure', id=draft['id'],
                              expected_revision=draft['draftRevision'], closure='automatic')
    definition = client.definition('publish', id=draft['id'], expected_revision=draft['draftRevision'])
    story = new_story(client, plan, 'Write result.txt containing correct followed by one newline',
                      ['result.txt contains exactly correct followed by one newline'], ['result.txt'])
    run = start(client, story, definition, str(uuid.uuid4()))
    try:
        finished = client.await_run(run['id'], graph_completed, timeout=1200)
    finally:
        current = client.run('get', run_id=run['id'])
        history = client.story('transition_history', story_id=story['id'])
        (output/'automatic-observation.json').write_text(json.dumps(
            {'run': current, 'story': client.story('get_story', story_id=story['id']),
             'transitionHistory': history}, indent=2)+'\n')
    closed = client.story('get_story', story_id=story['id'])
    assert closed['status'] == 'done', closed
    state = next(s for s in finished['stories'] if s['storyId'] == story['id'])
    assert state['accepted'] and state['acceptedRevision'] == closed['revision'], state
    workspace = state['worktreePath']
    commit = command(['git', 'rev-parse', 'HEAD'], workspace)
    assert command(['git', 'show', f'{commit}:result.txt'], workspace) == 'correct'
    assert any(r['commit'] == commit and r['exitCode'] == 0 for r in state['checkReceipts']), state
    authors = [a for a in finished['attempts'] if a['nodeId'] == 'implement']
    reviews = [a for a in finished['attempts'] if a['nodeId'] == 'review']
    assert authors and reviews and set(a['agent']['sessionId'] for a in authors).isdisjoint(
        a['agent']['sessionId'] for a in reviews), finished['attempts']
    # The harness never transitioned the story: no receipt has a human or local API actor.
    assert history and all(h['actor']['type'] == 'managed_session' for h in history), history
    return finished


def delivery_case(client, manifest, output, repair=False):
    plan = new_project(client, output/'repository')
    routing(client, manifest)
    definition = reviewed_definition(client, manifest, repair)
    story = new_story(client, plan, 'Write result.txt containing correct followed by one newline',
                      ['result.txt contains exactly correct followed by one newline'], ['result.txt'])
    run = start(client, story, definition, str(uuid.uuid4()))
    released=set()
    for cycle in range(2 if repair else 1):
        def awaiting_submission(current):
            return any(a['nodeId']=='submit' and a['state']=='paused' and a['id'] not in released
                       for g in current['graphExecutions'] for a in g['activations'])
        held=client.await_run(run['id'],awaiting_submission,timeout=1200)
        execution=held['graphExecutions'][0]
        activation=next(a for a in execution['activations']
                        if a['nodeId']=='submit' and a['state']=='paused' and a['id'] not in released)
        workspace=next(s['worktreePath'] for s in held['stories'] if s.get('worktreePath'))
        actual=(Path(workspace)/'result.txt').read_bytes()
        if actual==b'correct\n':
            current_story=client.story('get_story',story_id=story['id'])
            current_story=transition(client,current_story,{'check_criterion':0})
            transition(client,current_story,'submit_review')
        else:
            assert repair and cycle==0 and actual==b'wrong\n',actual
        released.add(activation['id'])
        current=client.run('get',run_id=run['id'])
        client.run('command',run_id=run['id'],command_id='submit-current-artifact:'+str(cycle),
                   expected_sequence=current['sequence'],command={'action':'resume_graph',
                   'execution_id':execution['id'],'activation_id':activation['id'],
                   'resolution':'Operator inspected actual bytes; review independently; no claim for wrong bytes'})
    finished = client.await_run(run['id'], graph_completed, timeout=1200)
    # Operator closure is explicit; automatic closure remains blocked by2716-944f.
    for check in definition['requiredChecks']:
        current=client.run('get',run_id=run['id'])
        client.run('execute_check',run_id=run['id'],story_id=story['id'],
                   check_id=check['id'],command_id='operator-check:'+check['id'],
                   expected_sequence=current['sequence'])
    reviewed=client.story('get_story',story_id=story['id'])
    if not reviewed['checked'][0]:
        reviewed=transition(client,reviewed,{'check_criterion':0})
    if reviewed['status']=='in_progress':
        reviewed=transition(client,reviewed,'submit_review')
    approved=reviewed if reviewed['status']=='done' else transition(client,reviewed,'approve')
    assert approved['status']=='done',approved
    current=client.run('get',run_id=run['id'])
    client.run('command',run_id=run['id'],command_id='operator-accept',
               expected_sequence=current['sequence'],
               command={'action':'accept_story','story_id':story['id']})
    finished=client.run('get',run_id=run['id'])
    state = next(s for s in finished['stories'] if s['storyId']==story['id'])
    workspace = state['worktreePath']
    assert (Path(workspace)/'result.txt').read_bytes()==b'correct\n'
    attempts = finished['attempts']
    authors = [a for a in attempts if a['nodeId']=='implement']
    reviews = [a for a in attempts if a['nodeId']=='review']
    assert authors and reviews
    assert all(a['agent'].get('acp') for a in authors)
    assert all(not a['agent'].get('acp') for a in reviews)
    assert set(a['agent']['sessionId'] for a in authors).isdisjoint(
        a['agent']['sessionId'] for a in reviews)
    assert state['checkReceipts'], 'completed without actual check receipt'
    if repair:
        assert len(authors)==2 and len(reviews)==2, attempts
        assert authors[0]['report']['evidence']!=authors[1]['report']['evidence']
        assert reviews[0]['report']['review']['decision']!='approved'
        assert reviews[1]['report']['review']['decision']=='approved'
        assert len({r['commit'] for r in state['checkReceipts']})>=1
    commit = command(['git','rev-parse','HEAD'],workspace)
    assert command(['git','show',f'{commit}:result.txt'],workspace)=='correct'
    (output/'delivery-observation.json').write_text(json.dumps(finished,indent=2)+'\n')

    return finished
