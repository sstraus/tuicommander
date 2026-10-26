import http from 'node:http';
import net from 'node:net';
import { readFileSync, writeFileSync, appendFileSync, mkdirSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';

const dir = process.env.EVIDENCE_DIR;
if (!dir) throw new Error('EVIDENCE_DIR is required');
const socketPath = process.env.AGENT_TEST_SOCKET;
if (!socketPath) throw new Error('AGENT_TEST_SOCKET is required');
const port = Number(process.env.PROBE_PORT ?? 11770);
mkdirSync(dir, { recursive: true });
const config = `${dir}/claude-config`;
const cwd = `${dir}/workspace`;
mkdirSync(cwd, { recursive: true });
mkdirSync(`${config}/sessions`, { recursive: true });
const project = `${config}/projects/${cwd.replace(/[^a-zA-Z0-9]/g, '-')}`;
mkdirSync(project, { recursive: true });
const uuid = randomUUID();
const count = Number(process.env.ENTRY_COUNT ?? 2000);
const transcript = Array.from({ length: count }, (_, i) => {
  const role = i % 2 === 0 ? 'user' : 'assistant';
  const text = `SIM1770_ENTRY_${i + 1} ${i === count - 1 ? 'SIM1770_LAST_ENTRY ' : ''}Synthetic measurement paragraph. This entry checks the existing transcript renderer without a model request.`;
  return JSON.stringify({ type: role, origin: { kind: role === 'user' ? 'human' : 'agent' }, uuid: randomUUID(), sessionId: uuid, cwd, timestamp: '2026-10-23T12:00:00.000Z', message: { id: `message-${i}`, role, content: role === 'user' ? text : [{ type: 'text', text }], stop_reason: 'end_turn' } });
}).join('\n') + '\n';
writeFileSync(`${project}/${uuid}.jsonl`, transcript);
// Compile synthetic-claude.c into this evidence directory before starting.

function api(path, method = 'GET', body) {
  return new Promise((resolve, reject) => {
    const request = http.request({ socketPath, path, method, headers: { 'content-type': 'application/json' } }, response => {
      let data = ''; response.on('data', chunk => data += chunk);
      response.on('end', () => response.statusCode < 300 ? resolve(JSON.parse(data)) : reject(new Error(`${path} ${response.statusCode} ${data}`)));
    });
    request.setTimeout(15000, () => request.destroy(new Error('Isolated backend request timed out')));
    request.on('error', reject); request.end(body ? JSON.stringify(body) : undefined);
  });
}
const session = await api('/sessions/agent', 'POST', { agent_type: 'claude', binary_path: `${dir}/synthetic-claude`, args: [], prompt: '', cwd, env: { CLAUDE_CONFIG_DIR: config }, rows: 30, cols: 80 });
writeFileSync(`${dir}/session.json`, JSON.stringify(session));
await api(`/sessions/${session.session_id}/name`, 'PUT', { name: '1770 measurement', isCustom: true });
const row = (await api('/sessions')).find(value => value.session_id === session.session_id);
writeFileSync(`${config}/sessions/${row.child_pid}.json`, JSON.stringify({ pid: row.child_pid, sessionId: uuid, cwd, version: 'synthetic-measurement' }));
try {
  const snapshot = await api(`/sessions/${session.session_id}/chat-view`);
  writeFileSync(`${dir}/snapshot.json`, JSON.stringify(snapshot));
  if (snapshot.updates.length !== count) throw new Error(`Expected ${count} updates, got ${snapshot.updates.length}`);
} catch (error) {
  await api(`/sessions/${session.session_id}`, 'DELETE').catch(() => {});
  throw error;
}
const probe = readFileSync(fileURLToPath(new URL('./probe.js', import.meta.url)), 'utf8');
const server = http.createServer((req, res) => {
  if (req.url === '/__1770/event') {
    let data = ''; req.on('data', chunk => data += chunk); req.on('end', () => { appendFileSync(`${dir}/events.jsonl`, data + '\n'); res.end('ok'); }); return;
  }
  const upstream = http.request({ socketPath, path: req.url, method: req.method, headers: { ...req.headers, host: 'localhost', 'accept-encoding': 'identity' } }, response => {
    if (response.headers['content-type']?.includes('text/html')) {
      let html = ''; response.on('data', chunk => html += chunk); response.on('end', () => {
        const headers = { ...response.headers }; delete headers['content-length']; delete headers['content-encoding']; delete headers['transfer-encoding'];
        res.writeHead(response.statusCode, headers); res.end(html.replace('</head>', `<script>${probe}</script></head>`));
      });
    } else { res.writeHead(response.statusCode, response.headers); response.pipe(res); }
  });
  upstream.on('error', error => { res.statusCode = 502; res.end(String(error)); }); req.pipe(upstream);
});
server.on('upgrade', (req, client, head) => {
  const upstream = net.connect({ path: socketPath }, () => {
    upstream.write(`${req.method} ${req.url} HTTP/${req.httpVersion}\r\n${Object.entries(req.headers).map(([key, value]) => `${key}: ${value}`).join('\r\n')}\r\n\r\n`);
    if (head.length) upstream.write(head); client.pipe(upstream); upstream.pipe(client);
  });
  upstream.on('error', () => client.destroy()); client.on('error', () => upstream.destroy());
});
server.listen(port, '127.0.0.1', () => console.log(`READY http://127.0.0.1:${port}/mobile ${count} updates`));
const stop = async () => { await api(`/sessions/${session.session_id}`, 'DELETE').catch(() => {}); server.closeAllConnections(); server.close(); process.exit(0); };
process.on('SIGTERM', stop); process.on('SIGINT', stop);
