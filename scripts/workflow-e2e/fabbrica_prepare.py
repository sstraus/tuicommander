"""Prepare real, disposable C1 state on rb; no worker or GitHub access."""
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import time
import urllib.request


def run(argv, **kwargs):
    return subprocess.run(argv, check=True, capture_output=True, text=True,
                          timeout=180, **kwargs).stdout.strip()


def main():
    os.umask(0o077)
    root = Path.home() / 'Gits/.tmp' / f'workflow-e2e-2707-fabbrica-{int(time.time())}'
    root.mkdir(mode=0o700)
    for directory in ('panel', 'panel/state', 'panel/operator', 'node', 'node/identity', 'bin'):
        (root / directory).mkdir(mode=0o700)
    panel = root / 'panel'
    node = root / 'node'
    run(['go', 'build', '-o', str(root/'bin/panel'), './cmd/fabbrica-panel'])
    run(['go', 'build', '-o', str(root/'bin/node'), './cmd/fabbrica-node'])
    for name in ('ca', 'server'):
        run(['openssl', 'genpkey', '-algorithm', 'RSA', '-pkeyopt', 'rsa_keygen_bits:2048',
             '-out', str(panel/f'{name}-key.pem')])
    run(['openssl', 'req', '-new', '-x509', '-key', str(panel/'ca-key.pem'),
         '-out', str(panel/'ca.pem'), '-days', '2', '-subj', '/CN=TUIC 2707 disposable CA',
         '-addext', 'basicConstraints=critical,CA:TRUE',
         '-addext', 'keyUsage=critical,keyCertSign,cRLSign'])
    run(['openssl', 'req', '-new', '-key', str(panel/'server-key.pem'),
         '-out', str(panel/'server.csr'), '-subj', '/CN=localhost'])
    extensions = panel/'server.ext'
    extensions.write_text('subjectAltName=DNS:localhost,IP:127.0.0.1\n'
                          'extendedKeyUsage=serverAuth\nkeyUsage=digitalSignature,keyEncipherment\n')
    run(['openssl', 'x509', '-req', '-in', str(panel/'server.csr'), '-CA', str(panel/'ca.pem'),
         '-CAkey', str(panel/'ca-key.pem'), '-CAcreateserial', '-out', str(panel/'server.pem'),
         '-days', '1', '-extfile', str(extensions)])
    # Kernel allocates a private loopback port; no public listener or production state.
    with socket.socket() as reservation:
        reservation.bind(('127.0.0.1', 0))
        port = reservation.getsockname()[1]
    admin = panel/'operator/admin.sock'
    processes = []
    try:
        with (root/'panel.log').open('w') as log:
            proc = subprocess.Popen([str(root/'bin/panel'), '--listen', f'127.0.0.1:{port}',
                '--tls-cert', str(panel/'server.pem'), '--tls-key', str(panel/'server-key.pem'),
                '--ca-cert', str(panel/'ca.pem'), '--ca-key', str(panel/'ca-key.pem'),
                '--state-dir', str(panel/'state'), '--admin-socket', str(admin)],
                stdout=log, stderr=log)
        processes.append(proc)
        # Setup readiness, separate from any workflow behaviour deadline.
        deadline = time.monotonic()+60
        while not admin.exists():
            if proc.poll() is not None or time.monotonic() >= deadline:
                raise RuntimeError('isolated panel did not create its operator socket')
            time.sleep(0.1)
        raw = run(['curl', '--fail', '--silent', '--unix-socket', str(admin),
                   '-X', 'POST', 'http://localhost/v1/join'])
        token = json.loads(raw)['token']
        token_file = node/'join-token'
        token_file.write_text(token)
        del token, raw
        with (root/'node.log').open('w') as log:
            proc = subprocess.Popen([str(root/'bin/node'), '--panel', f'https://localhost:{port}',
                '--server-name', 'localhost', '--ca-cert', str(panel/'ca.pem'),
                '--identity-dir', str(node/'identity'), '--join-token-file', str(token_file)],
                stdout=log, stderr=log)
        processes.append(proc)
        deadline = time.monotonic()+60
        while True:
            machines = json.loads(run(['curl', '--fail', '--silent', '--unix-socket', str(admin),
                                      'http://localhost/v1/machines']))
            records = machines if isinstance(machines, list) else machines.get('machines', [])
            if records and any(m.get('connected') for m in records):
                break
            if proc.poll() is not None or time.monotonic() >= deadline:
                raise RuntimeError('real node enrollment/link did not become connected')
            time.sleep(0.2)
        token_file.unlink()
        receipt = {'root': str(root), 'toolchain': run(['go', 'version']),
                   'caSha256': hashlib.sha256((panel/'ca.pem').read_bytes()).hexdigest(),
                   'machines': records, 'joinToken': 'issued and consumed; value excluded',
                   'worker': 'not started', 'github': 'not contacted',
                   'model': 'not invoked', 'preparedUtcMs': int(time.time()*1000)}
        (root/'preparation.json').write_text(json.dumps(receipt, indent=2)+'\n')
    finally:
        exits=[]
        for proc in reversed(processes):
            proc.terminate()
            proc.wait(timeout=30)
            exits.append({'pid':proc.pid,'exit':proc.returncode})
        (root/'owned-process-exits.json').write_text(json.dumps(exits,indent=2)+'\n')
    print(json.dumps({'prepared':str(root),'publicCA':str(panel/'ca.pem'),
                      'stateRetained':True,'processesStopped':True}))


if __name__ == '__main__':
    main()
