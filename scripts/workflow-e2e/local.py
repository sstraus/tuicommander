"""Local workflow cases. Every worker is launched by the real daemon."""
import copy
import json
import uuid
from client import graph_completed, graph_agent, new_project, new_story, publish, start, transition


def routing(client, manifest):
    base = client.request('GET', '/config/agents')
    config = copy.deepcopy(base)
    secondary = manifest['secondaryAgent']
    profile = manifest['secondaryProfile']
    model = manifest['egoModel']
    config['agents'].setdefault(secondary, {})['run_configs'] = [{
        'name': profile, 'command': manifest['secondary'],
        'model': manifest['secondaryModel'], 'args': manifest['secondaryArgs'],
        'env': manifest['secondaryEnv']}]
    client.request('PUT', '/config/agents', {'base': base, 'config': config})
    common = {'capabilities': ['story_read', 'story_report'],
              'scope': 'isolated_workspace', 'outputContractVersion': 1,
              'defaultForRole': True, 'effort': 'medium'}
    acp = {'transport':'acp','profile':'e2e','model':model,'effort':'medium'}
    runtime = {'executable':manifest['egoRuntime'],'machineProfile':''}
    pty = {'transport':'pty','profile':profile}
    routes = [{**common,'role':'implementer','model':model,
               'execution':acp,'acpRuntime':runtime},
              {**common,'role':'reviewer','model':manifest['secondaryModel'],'execution':pty},
              {**common,'role':'implementer','model':manifest['secondaryModel'],
               'defaultForRole':False,'execution':pty},
              {**common,'role':'reviewer','model':model,'defaultForRole':False,
               'execution':acp,'acpRuntime':runtime}]
    policy = client.definition('get_agent_policy')
    return client.definition('import_agent_policy',
                              expected_revision=policy['revision'] if policy else 0,
                              policy={'source': '2707 explicit isolated operator policy',
                                      'sourceRevision': manifest['candidate'], 'routes': routes})


def prompt(filename, content):
    return (f'Write {filename} containing exactly {content!r} followed by a newline. '
            'Commit the file in your assigned workspace. Do not launch agents, push, '
            'or change any other repository. Then submit the typed workflow report '
            'using the scoped context below. criterionResults is an array of objects with index (integer), '
            'satisfied (boolean), evidence (string); evidence is an array of strings. '
            'Do not call progress. Do not substitute a chat answer for a report.')


def review_contract():
    return (' Use mcp__tuicommander__workflow action=report with input containing the supplied '
            'contractVersion, runId, storyId, storyRevision, attemptId and generation. '
            'Read the tool schema; do not guess. outcome is completed for a completed review. '
            'criterionResults=[{index:0,satisfied:boolean,evidence:string}]; evidence=[string]. '
            'review={decision:approved OR changes_requested,artifactDigest:supplied_digest,findings:[]}. '
            'For rejection include finding {criterionIndex:0,severity:major,summary:string,evidence:string}. '
            'The enum is approved, not approve. No chat-only report, no progress. ')


def agent_story(client, plan, filename, content, execution=None):
    story = new_story(client, plan, f'Create {filename}', [f'{filename} contains {content}'], [filename])
    definition = publish(client, graph_agent(prompt(filename, content), execution))
    run = start(client, story, definition, str(uuid.uuid4()))
    return story, definition, run


# Catches: QA-only work launching an implementer or releasing a dependent before
# approval. The real-agent sibling establishes that unrelated workflow delivery
# is not globally paused; no hand-written reports or attempt bindings are used.
def qa_case(client, manifest, output):
    plan = new_project(client, output / 'repository')
    routing(client, manifest)
    story = new_story(client, plan, 'Manual observation', ['[MANUAL] Confirm retained sentinel'])
    definition = publish(client, graph_agent('Do not implement manual-only QA work.'))
    run = start(client, story, definition, str(uuid.uuid4()))
    state = client.await_run(run['id'], lambda r: r['status'] == 'paused')
    current = client.story('get_story', story_id=story['id'])
    assert current['status'] == 'qa', current
    assert not any(a.get('agent') for a in state['attempts']), state
    assert not any(s.get('worktreePath') for s in state['stories']), state
    sibling_plan = client.story('create_plan', title='Independent work', source='sibling.md')
    _, _, sibling = agent_story(client, sibling_plan, 'sibling.txt', 'independent delivery')
    finished = client.await_run(sibling['id'], graph_completed)
    paths = [s.get('worktreePath') for s in finished['stories']]
    # The file itself is the oracle, not the daemon's completed label.
    from pathlib import Path
    assert any(p and (Path(p) / 'sibling.txt').read_text() == 'independent delivery\n'
               for p in paths), paths
    current = transition(client, current, {'check_criterion': 0})
    approved = transition(client, current, 'approve')
    assert approved['status'] == 'done', approved
    (output / 'qa-observation.json').write_text(json.dumps({
        'paused': state, 'sibling': finished, 'approved': approved}, indent=2) + '\n')


def execute(case, client, manifest, output):
    if case == 'E22':
        from reconnect import reconnect_case
        reconnect_case(client, manifest, output)
        return
    if case == 'E05':
        from integration import integrate_case
        integrate_case(client, manifest, output)
        return
    if case == 'E01' and manifest['experiment'].get('closure') == 'automatic':
        from lifecycle import automatic_case
        automatic_case(client, manifest, output)
        return
    if case in ('E01', 'E02'):
        from lifecycle import delivery_case
        delivery_case(client, manifest, output, repair=case == 'E02')
        return
    if case in ('E03', 'E20'):
        from questions import question_case
        question_case(client, manifest, output, deadline=case == 'E20')
        return
    if case == 'E07':
        from calls import pinned_call
        pinned_call(client, manifest, output)
        return
    if case == 'E24':
        qa_case(client, manifest, output)
        return
    raise ValueError(f'unsupported local case {case}')
