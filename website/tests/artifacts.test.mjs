import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// Catches SAL hiding public content permanently when JavaScript is disabled.
test('website reveal content has a no-JavaScript fallback', () => {
  const dist = fileURLToPath(new URL('../dist/', import.meta.url));
  let revealPages = 0;
  for (const entry of readdirSync(dist, { recursive: true })) {
    if (!entry.endsWith('.html')) continue;
    const html = readFileSync(resolve(dist, entry), 'utf8');
    if (!/\bdata-sal\s*=/.test(html)) continue;
    revealPages++;
    const fallback = [...html.matchAll(/<noscript\b[^>]*>([\s\S]*?)<\/noscript>/gi)]
      .map((match) => match[1]).join('\n');
    assert.match(fallback, /<style\b[^>]*>[\s\S]*\[data-sal\]\s*\{[^}]*opacity\s*:\s*1\s*;[^}]*transform\s*:\s*none\s*;/, entry);
  }
  assert.ok(revealPages > 0, 'the built site must exercise reveal content');
});

import { resolve } from 'node:path';
const root = fileURLToPath(new URL('../../', import.meta.url));
const read = p => readFileSync(resolve(root, p), 'utf8');
// Catches unresolved shared partials or dropped multipage entries after trunk integration.
test('website build retains area pages, published posts, shared markup and local assets', () => {
  const pages = ['index.html', '2.0.html', 'download.html', 'blog.html', ...['workspace', 'orchestration', 'git-and-review', 'planning', 'anywhere', 'extend'].map(p => `features/${p}.html`), ...JSON.parse(read('website/blog/index.json')).posts.filter(p => p.status === 'published').map(p => `blog/${p.slug}.html`)];
  const sitemap = read('website/dist/sitemap.xml');
  for (const page of pages) {
    const html = read(`website/dist/${page}`);
    assert.doesNotMatch(html, /<!-- @include|%DISCORD_URL%|%FILM_SRC%/, page);
    assert.match(html, /<nav\b/, page);
    assert.match(html, /<footer\b/, page);
    assert.ok(sitemap.includes(`https://tuicommander.com/${page === 'index.html' ? '' : page}`), page);
    for (const match of html.matchAll(/(?:src|href)="(\/(?:assets|shots)\/[^"?#]+)"/g)) {
      assert.ok(existsSync(resolve(root, 'website/dist', `.${match[1]}`)), `${page}: ${match[1]}`);
    }
  }
  // Catch the home film reverting to the 2.0 cut or replacing the chaptered 2.0 film.
  assert.match(read('website/dist/index.html'), /<source src="https:\/\/dl\.straus\.it\/tuicommander\/tuicommander-2\.1-home\.mp4" type="video\/mp4"/);
  assert.match(read('website/dist/2.0.html'), /<source src="\/video\/tuicommander-2\.0\.mp4" type="video\/mp4"/);
  assert.match(read('website/dist/download.html'), /data-download="macos"/);
  assert.match(read('website/dist/download.html'), /data-download="windows"/);
  assert.match(read('website/dist/download.html'), /data-download="linux"/);
  console.log(`Verified ${pages.length} generated pages and their local assets`);
});
