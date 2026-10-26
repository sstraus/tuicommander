import { readFileSync } from 'node:fs';

const events = readFileSync(process.argv[2], 'utf8').trim().split('\n').map(line => JSON.parse(line));
const groups = [];
let group;
for (const event of events) {
  if (event.kind === 'load') { group = []; groups.push(group); }
  if (group) group.push(event);
}
const runs = groups.filter(values => values.some(event => event.kind === 'chat'));
if (runs.length !== 5) throw new Error(`Expected five measured documents, got ${runs.length}`);
const samples = { firstOpen: [], foregroundResume: [], cliToChat: [], chatToCli: [], cliChatCliRenderSum: [] };
for (const values of runs) {
  const measured = values.filter(event => ['chat', 'resume', 'cli'].includes(event.kind));
  const taps = values.filter(event => event.kind === 'tap');
  if (measured.map(event => event.kind).join(',') !== 'chat,resume,cli,chat,cli') throw new Error('Incomplete transition sequence');
  if (taps.map(event => event.label).join(',') !== 'Chat,CLI,Chat,CLI' || taps.some(event => !event.trusted)) throw new Error('Missing native trusted taps');
  for (const event of measured) {
    if (!event.standalone || !event.displayStandalone) throw new Error('Not a standalone PWA');
    if (event.kind !== 'cli' && event.entryCount !== 2000) throw new Error('Not 2000 rendered entries');
  }
  const [first, resume, , reopen, cli] = measured;
  samples.firstOpen.push(first.elapsedMs);
  samples.foregroundResume.push(resume.elapsedMs);
  samples.cliToChat.push(reopen.elapsedMs);
  samples.chatToCli.push(cli.elapsedMs);
  samples.cliChatCliRenderSum.push(reopen.elapsedMs + cli.elapsedMs);
}
const round = value => Math.round(value * 1000) / 1000;
const nearestRank = (values, percentile) => round([...values].sort((a, b) => a - b)[Math.ceil(values.length * percentile) - 1]);
console.log(JSON.stringify({
  unit: 'milliseconds',
  percentileMethod: 'nearest rank; n=5 so p95 is the maximum, not a stable tail estimate',
  endpoint: 'Second requestAnimationFrame after last-entry visible geometry (or Chat removal for CLI); frame opportunity, not compositor or physical display proof',
  samples: Object.fromEntries(Object.entries(samples).map(([kind, values]) => [kind, { values: values.map(value => Math.round(value * 1000) / 1000), p50: nearestRank(values, 0.5), p95: nearestRank(values, 0.95) }])),
}, null, 2));
