"""Request one restart from the outer owner of the isolated native daemon."""
import hashlib
import json
from pathlib import Path
import socket
import time


def restart(client, manifest):
    path = Path(manifest['lifecycleSocket']).resolve()
    if not path.is_relative_to(Path.home()/'Gits/.tmp'):
        raise ValueError('restart control socket must be owned test state under Gits/.tmp')
    request = {'action':'restart', 'instance':manifest['environment']['TUIC_APP_INSTANCE'],
               'candidate':manifest['candidate']}
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
        connection.settimeout(650)
        connection.connect(str(path))
        connection.sendall((json.dumps(request)+'\n').encode())
        response = json.loads(connection.makefile('r').readline())
    (client.evidence/'daemon-restart.json').write_text(json.dumps(response,indent=2)+'\n')
    if response.get('status')!='ready':
        raise RuntimeError(f'owned daemon restart failed: {response}')
    client.session = None
    health = client.request('GET','/health')
    expected = hashlib.sha256(Path(manifest['binary']).read_bytes()).hexdigest()
    assert health['build']['sha256']==expected
    assert health['socket_path']==str(client.socket)
    return health
