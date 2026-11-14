"""Recorded HTTP operations against one explicitly isolated Unix socket."""
import http.client
import json
import socket
import re
import time
import subprocess
import urllib.parse
from pathlib import Path


class UnixHTTP(http.client.HTTPConnection):
    def __init__(self, path, timeout=30):
        super().__init__('localhost', timeout=timeout)
        self.path = str(path)

    def connect(self):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(self.timeout)
        self.sock.connect(self.path)


class Client:
    def __init__(self, path, evidence):
        self.socket = path
        self.evidence = Path(evidence)
        self.counter = 0
        self.session = None
        self.project = None
        self.started_runs = []
        self.theme_selected = set()
        self.import_prompt_seen = {}

    def request(self, method, path, body=None, expect_error=False):
        self.counter += 1
        connection = UnixHTTP(self.socket)
        headers = {'Content-Type': 'application/json',
                   'Accept': 'application/json, text/event-stream'}
        if self.session:
            headers['Mcp-Session-Id'] = self.session
        encoded = json.dumps(body) if body is not None else None
        connection.request(method, path, encoded, headers)
        response = connection.getresponse()
        self.session = response.getheader('Mcp-Session-Id') or self.session
        raw = response.read().decode()
        connection.close()
        if raw.startswith('event:') or raw.startswith('data:'):
            raw = next(line[6:] for line in raw.splitlines() if line.startswith('data: '))
        try:
            value = json.loads(raw)
        except json.JSONDecodeError:
            value = {'raw': raw}
        record = {'method': method, 'path': path, 'request': body,
                  'status': response.status, 'response': value}
        (self.evidence / f'http-{self.counter:04}.json').write_text(
            json.dumps(record, indent=2) + '\n')
        if response.status >= 400 and not expect_error:
            raise RuntimeError(f'{path}: HTTP {response.status}: {value}')
        return value

    def action(self, service, action, **fields):
        query = urllib.parse.urlencode({'path': str(self.project)})
        reply = self.request('POST', f'/{service}/action?{query}',
                             {'action': action, **fields})
        if 'error' in reply:
            raise RuntimeError(str(reply['error']))
        return reply.get('value', reply)

    def tool(self, name, arguments):
        if not self.session:
            self.request('POST', '/mcp', {'jsonrpc': '2.0', 'id': 1,
                         'method': 'initialize', 'params': {
                             'protocolVersion': '2024-11-05', 'capabilities': {},
                             'clientInfo': {'name': 'workflow-e2e', 'version': '1'}}})
        reply = self.request('POST', '/mcp', {'jsonrpc': '2.0', 'id': self.counter + 1,
                             'method': 'tools/call',
                             'params': {'name': name, 'arguments': arguments}})
        result = reply.get('result', {})
        if 'error' in reply or result.get('isError'):
            raise RuntimeError(str(reply))
        return result

    def story(self, action, **fields):
        query = urllib.parse.urlencode({'path': str(self.project)})
        reply = self.request('POST', f'/stories/action?{query}',
                             {'action': {'action': action, **fields}})
        return reply.get('value', reply)

    def definition(self, action, **fields):
        return self.action('workflows/definition', action, **fields)

    def run(self, action, **fields):
        return self.action('workflows/run', action, **fields)

    def await_run(self, run_id, predicate, timeout=600, allow_questions=False):
        # Subscribe before inspection so a transition cannot be lost in between.
        events = UnixHTTP(self.socket, timeout=timeout)
        events.request('GET', '/events', headers={'Accept': 'text/event-stream'})
        response = events.getresponse()
        deadline = time.monotonic() + timeout
        try:
            while True:
                current = self.run('get', run_id=run_id)
                for attempt in current.get('attempts', []):
                    agent=attempt.get('agent')
                    if agent and not agent.get('acp') and attempt['state']=='running':
                        session=agent['sessionId']
                        terminal=self.request('GET','/sessions/'+session+'/output')
                        text=re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]','',terminal.get('data',''))
                        compact=re.sub(r'\s+','',text)
                        notices=[('theme','Choosethetextstyle' in compact),
                                 ('security','promptinjectionrisks' in compact and 'continue' in compact)]
                        for kind,visible in notices:
                            key=(session,kind)
                            if visible and key not in self.theme_selected:
                                self.request('POST','/sessions/'+session+'/write',{'data':'\r'})
                                self.theme_selected.add(key)
                        if 'Quicksafetycheck' in compact and 'Yes,Itrustthisfolder' in compact:
                            raise RuntimeError('owned test reviewer awaits workspace trust; no report yet')
                        if 'AllowexternalCLAUDE.mdfileimports?' in compact:
                            key=(session,'disable-external-imports')
                            if key not in self.theme_selected:
                                # Default No keeps ambient imports out of this isolated test profile.
                                self.request('POST','/sessions/'+session+'/write',{'data':'\r'})
                                self.theme_selected.add(key)
                                self.import_prompt_seen[session]=(time.monotonic(),terminal.get('data',''))
                            observed=self.import_prompt_seen.get(session)
                            if observed and time.monotonic()-observed[0]>30 and terminal.get('data','')==observed[1]:
                                raise RuntimeError('isolated external-import dialog did not advance after default No')
                        if 'Signintoyouraccount' in compact:
                            raise RuntimeError('owned test reviewer requires manual authentication')
                if predicate(current):
                    return current
                graphs=current.get('graphExecutions', [])
                if graphs and all(g['completed'] for g in graphs):
                    raise RuntimeError('graph completed before the expected scenario effect')
                pending = [q for q in current.get('questions', []) if q['state']=='pending']
                if pending and not allow_questions:
                    raise RuntimeError(f'unexpected operator question: {pending}')
                if current['status'] == 'paused' and not pending:
                    phases = [(g['id'], g['completed']) for g in current.get('graphExecutions', [])]
                    raise RuntimeError(f"run paused before expected effect; graph phases={phases}")
                while time.monotonic() < deadline:
                    line = response.readline()
                    if not line:
                        raise RuntimeError('event stream disconnected')
                    if line.startswith(b'data:'):
                        break
                else:
                    raise TimeoutError(f'run {run_id} did not reach expected effect')
        finally:
            events.close()

    def close_owned(self):
        inventory=subprocess.check_output(['ps','-axo','pid=,ppid=,lstart=,comm='],text=True)
        test_apps=[line.strip() for line in inventory.splitlines()
                   if any(Path(word).name.startswith(('tuicommander-agent-test','tuic-agent-test'))
                          for word in line.split())]
        (self.evidence/'close-test-app-inventory.json').write_text(json.dumps({
            'utcMs':int(time.time()*1000),'processes':test_apps},indent=2)+'\n')
        outcomes = []
        for run_id in self.started_runs:
            state = self.run('get', run_id=run_id)
            for race in range(10):
                if state['status'] in ('cancelled', 'completed'):
                    break
                try:
                    fresh_command(self,run_id,f'cleanup:{run_id}',{'action':'cancel'})
                    break
                except RuntimeError as error:
                    # A live run advances between get and cancel (E03-live-10). Cancel
                    # carries no answer, so a re-read sequence is safe to retry.
                    if 'stale workflow sequence' not in str(error) or race == 9:
                        raise
                    time.sleep(0.5)
                    state = self.run('get', run_id=run_id)
            state = self.run('get', run_id=run_id)
            for attempt in state['attempts']:
                if not attempt.get('agent'):
                    continue
                deadline = time.monotonic() + 120
                retry = 0
                while True:
                    try:
                        result = self.run('close_agent', run_id=run_id,
                                          attempt_id=attempt['id'], request_id=f"close:{attempt['id']}" + (f':{retry}' if retry else ''),
                                          expected_sequence=state['sequence'], successor_attempt_id=None)
                        # A PTY agent may still finish its turn after its typed report
                        # (E05-live-11). Wait inside the same bound; a persistent refusal fails.
                        if (result.get('state') == 'refused' and result.get('reason') ==
                                'owned process has live work, a draft, a question or undelivered input'
                                and time.monotonic() < deadline):
                            # Refusals are stored per request id; a retry is a new request.
                            retry += 1
                            outcomes.append(result)
                            time.sleep(2)
                            continue
                        break
                    except RuntimeError as error:
                        if ('managed launch/close is active; retry the same request' not in str(error)
                                or time.monotonic() >= deadline):
                            raise
                        time.sleep(0.5)
                outcomes.append(result)
                if result.get('state') != 'closed':
                    (self.evidence / 'owned-close.json').write_text(json.dumps(outcomes, indent=2)+'\n')
                    raise RuntimeError(f'owned process close not proven: {result}')
        (self.evidence / 'owned-close.json').write_text(json.dumps(outcomes, indent=2)+'\n')
        return outcomes


def fresh_command(client, run_id, command_id, command):
    # Only explicit preflight races may be retried; question revision/digest stays fixed.
    for attempt in range(5):
        current=client.run('get',run_id=run_id)
        sequence=current['sequence']
        try:
            receipt=client.run('command',run_id=run_id,command_id=command_id,
                               expected_sequence=sequence,command=command)
            return receipt,sequence
        except RuntimeError as error:
            if 'workflow or story revisions changed during command preflight; retry' not in str(error) or attempt==4:
                raise
            time.sleep(0.25)


def command(argv, cwd=None):
    result = subprocess.run(argv, cwd=cwd, text=True, capture_output=True,
                            timeout=60, check=True)
    return result.stdout.strip()


def new_project(client, directory):
    directory.mkdir()
    command(['git', 'init', '-b', 'main', str(directory)])
    command(['git', 'config', 'user.name', 'Workflow E2E'], directory)
    command(['git', 'config', 'user.email', 'workflow-e2e@localhost'], directory)
    command(['git', 'config', 'commit.gpgsign', 'false'], directory)
    (directory / '.gitignore').write_text('.mdkb/\n')
    (directory / 'plan.md').write_text('# Workflow evidence\n\nCreate the requested artifacts.\n')
    (directory / 'sibling.md').write_text('# Independent sibling plan\n')
    command(['git', 'add', '.gitignore', 'plan.md', 'sibling.md'], directory)
    command(['git', 'commit', '-m',
             'test: seed disposable workflow repository'], directory)
    client.tool('repo', {'action': 'add', 'path': str(directory)})
    client.project = directory
    return client.story('create_plan', title='Workflow evidence', source='plan.md')


def new_story(client, plan, title, criteria, scope=None):
    return client.story('create_story', input={
        'planId': plan['id'], 'title': title, 'criteria': criteria,
        'priority': 2, 'origin': {'type': 'native'}, 'fileScope': scope or ['result.txt']})


def transition(client, story, value):
    return client.story('transition', story_id=story['id'],
                        expected_revision=story['revision'], command=value)


def publish(client, graph, kind='story'):
    draft = client.definition('create_draft', name='Workflow e2e', kind=kind, graph=graph)
    if not any(n['kind']['type'] == 'return' for n in graph['nodes']):
        draft = client.definition('update_checks', id=draft['id'],
                                  expected_revision=draft['draftRevision'], checks=[{
                                      'id': 'integrity', 'argv': ['git', 'fsck'], 'timeoutSecs': 30}])
    return client.definition('publish', id=draft['id'],
                              expected_revision=draft['draftRevision'])


def start(client, story, definition, key):
    problems = client.run('start_readiness', definition_id=definition['id'],
                          definition_revision=definition['revision'])
    if problems:
        raise ValueError(f'exact execution route unavailable: {problems}')
    run = client.run('start_graph', target={'type': 'story', 'id': story['id']},
                      expected_revision=story['revision'], definition_id=definition['id'],
                      definition_revision=definition['revision'], request_id=key,
                      limits={'maxLoops': 2, 'maxStoryCreations': 8, 'maxSpawns': 8,
                              'maxParallelStories': 2, 'maxDurationSecs': 600})
    client.started_runs.append(run['id'])
    return run


def graph_agent(prompt, execution=None):
    return {'nodes': [
        {'id': 'start', 'kind': {'type': 'start'}},
        {'id': 'implement', 'kind': {'type': 'agent', 'role': 'implementer',
                                  'capabilities': ['story_read', 'story_report'],
                                  'prompt_template': prompt, 'execution': execution}},
        {'id': 'end', 'kind': {'type': 'end'}}],
        'edges': [{'from': 'start', 'to': 'implement', 'outcome': None},
                  {'from': 'implement', 'to': 'end', 'outcome': None}]}


def graph_completed(run):
    return bool(run.get('graphExecutions')) and run['graphExecutions'][0]['completed']
