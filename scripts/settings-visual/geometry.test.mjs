import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import {runInNewContext} from 'node:vm';

const oracle = readFileSync(new URL('./geometry.js', import.meta.url), 'utf8');
// Recorded native WebKit measurements, 2026-10-19:
// the live-1789 run's results/live-1789/comparison.json, general.native_failures[0].boxes.
const recorded = {
  label: {x:917,y:2200.171875,right:944.234375,bottom:2218.171875,width:27.234375,height:18},
  lastLine: {x:917,y:2200.3388671875,right:944.2314453125,bottom:2218.0048828125,width:27.2314453125},
  badge: {x:956.234375,y:2201.984375,right:972.234375,bottom:2217.984375,width:16,height:16},
};

// Supply measured DOM inputs to the shipped capture expression, not a copy of
// its predicates. This tests the oracle, not browser layout or DOM rendering.
function checks({badge = recorded.badge, lastLine = recorded.lastLine, scrollWidth = 946, clientWidth = 946} = {}) {
  const label = {textContent:'ego', getBoundingClientRect:() => recorded.label};
  const button = {getBoundingClientRect:() => badge};
  const pair = {querySelector:selector => selector === 'label,h3' ? label : button};
  const content = {
    scrollWidth, clientWidth, innerText:'ego',
    getBoundingClientRect:() => recorded.label,
    querySelector:selector => selector === 'header' ? label : null,
    querySelectorAll:selector => selector === '[class*="helpLabel"], [class*="cardHeading"]' ? [pair] : [],
  };
  const document = {
    querySelector:selector => selector === '[data-settings-content]' ? content : null,
    createRange:() => ({selectNodeContents() {}, getClientRects:() => [lastLine]}),
  };
  return runInNewContext(oracle, {document, innerWidth:1400, location:{search:''}, URLSearchParams}).checks;
}

function passes(name, inputs) {
  const result = checks(inputs).find(check => check.name === name);
  assert.ok(result, `oracle did not emit ${name}`);
  return result.pass;
}

test('recorded WebKit Range rounding does not falsely reject the ego badge', () => {
  assert.equal(passes('badge beside ego'), true);
});

test('recorded smaller scrollWidth does not falsely report horizontal overflow', () => {
  // Recorded isolated native snapshot: scrollWidth937, clientWidth946.
  assert.equal(passes('no horizontal overflow', {scrollWidth:937, clientWidth:946}), true);
});

test('badge tolerance never hides a full pixel excess, overlap or a separate line', () => {
  for (const badge of [
    {...recorded.badge, x:recorded.lastLine.right + 13},
    {...recorded.badge, x:recorded.lastLine.right - 2},
    {...recorded.badge, y:recorded.lastLine.bottom},
    {...recorded.badge, bottom:recorded.lastLine.y},
  ]) assert.equal(passes('badge beside ego', {badge}), false, JSON.stringify(badge));
  assert.equal(passes('badge beside ego', {badge:{...recorded.badge, x:recorded.lastLine.right + 12}}), true);
});

test('horizontal overflow rejects even one pixel but accepts equal widths', () => {
  assert.equal(passes('no horizontal overflow', {scrollWidth:947}), false);
  assert.equal(passes('no horizontal overflow'), true);
});
