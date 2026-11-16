// Independent terminal oracle. The ED2 retention adapter is the only TUIC policy
// adjustment; cursor/editing/wrapping/Unicode semantics remain xterm's own.
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');
// The oracle install lives outside the checkout: TUIC_TERMINAL_INTEGRITY_DIR, else
// <temp dir>/tuic-tests/terminal-integrity (same default as ansi_integrity.py).
const integrityDir = process.env.TUIC_TERMINAL_INTEGRITY_DIR
  || path.join(require('node:os').tmpdir(), 'tuic-tests', 'terminal-integrity');
const { Terminal } = require(path.join(integrityDir, 'oracle/node_modules/@xterm/headless'));

async function render(payload, rows, cols, retainCleared = false) {
  const terminal = new Terminal({cols, rows, scrollback: 10000, allowProposedApi: true});
  const retained = [];
  if (retainCleared) {
    terminal.parser.registerCsiHandler({final: 'J'}, (params) => {
      if (params[0] === 2 && terminal.buffer.active.type === 'normal') {
        const buffer = terminal.buffer.active;
        const cleared = Array.from({length: rows}, (_, y) =>
          buffer.getLine(buffer.baseY + y).translateToString(true));
        while (cleared.length && cleared.at(-1) === '') cleared.pop();
        retained.push({at: buffer.baseY, rows: cleared});
      }
      return false; // xterm still handles the actual erase and cursor semantics.
    });
  }
  await new Promise(resolve => terminal.write(payload, resolve));
  const buffer = terminal.buffer.active;
  const result = [];
  for (let index = 0; index < buffer.length; index++) {
    for (const extra of retained) if (extra.at === index) result.push(...extra.rows);
    result.push(buffer.getLine(index).translateToString(true));
  }
  terminal.dispose();
  return result.slice(-(10000 + rows));
}

async function main() {
  if (process.argv[2] === '--server') {
    for await (const line of readline.createInterface({input: process.stdin})) {
      const request = JSON.parse(line);
      const result = await render(Buffer.from(request.payload, 'base64'), request.rows, request.cols, true);
      process.stdout.write(JSON.stringify(result) + '\n');
    }
    return;
  }
  const dir = process.argv[2];
  if (!dir) throw new Error('usage: node check_xterm.cjs <failure-directory> | --server');
  const meta = JSON.parse(fs.readFileSync(path.join(dir, 'meta.json')));
  const rows = await render(fs.readFileSync(path.join(dir, 'raw.bin')), meta.rows, meta.cols);
  fs.writeFileSync(path.join(dir, 'xterm.json'), JSON.stringify(rows, null, 2));
  const actual = JSON.parse(fs.readFileSync(path.join(dir, 'actual.json')));
  const normalize = rows => rows.map(row => row.replaceAll('\t', ' ').normalize('NFC').trimEnd());
  console.log(JSON.stringify({case: meta.name,
    xtermMatchesTuic: JSON.stringify(normalize(rows)) === JSON.stringify(normalize(actual))}));
}
main().catch(error => { console.error(error); process.exitCode = 1; });
