// Measurement-only injection. Never included in the application bundle.
(() => {
  let pending;
  let serial = 0;
  const report = (value) => {
    fetch('/__1770/event', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(value) }).catch(() => {});
  };
  const info = () => ({ standalone: navigator.standalone === true, displayStandalone: matchMedia('(display-mode: standalone)').matches, userAgent: navigator.userAgent, width: innerWidth, height: innerHeight });
  report({ kind: 'load', ...info() });
  // Split transport/body decoding from synchronous transcript work. The wrapper
  // preserves the application's response and JSON parsing paths.
  const originalFetch = window.fetch;
  window.fetch = async function (...args) {
    const url = String(args[0]);
    if (!url.includes('/chat-view')) return originalFetch.apply(this, args);
    const start = performance.now();
    const response = await originalFetch.apply(this, args);
    const headersAt = performance.now();
    const originalText = response.text.bind(response);
    response.text = async () => {
      const text = await originalText();
      const bodyAt = performance.now();
      report({ kind: 'snapshot-transfer', url, headersMs: headersAt - start, bodyMs: bodyAt - headersAt, elapsedMs: bodyAt - start, bytes: text.length });
      return text;
    };
    return response;
  };
  let layoutMs = 0;
  let layoutReads = 0;
  const scrollHeight = Object.getOwnPropertyDescriptor(Element.prototype, 'scrollHeight');
  if (scrollHeight?.get && scrollHeight.configurable) {
    Object.defineProperty(Element.prototype, 'scrollHeight', { ...scrollHeight, get() {
      if (this.getAttribute('aria-label') !== 'Chat transcript') return scrollHeight.get.call(this);
      const start = performance.now();
      const height = scrollHeight.get.call(this);
      layoutMs += performance.now() - start;
      layoutReads++;
      return height;
    } });
  }
  const originalParse = JSON.parse;
  JSON.parse = function (text, ...args) {
    if (typeof text !== 'string' || !text.includes('SIM1770_ENTRY_')) return originalParse.call(this, text, ...args);
    const start = performance.now();
    const value = originalParse.call(this, text, ...args);
    const end = performance.now();
    report({ kind: 'snapshot-parse', elapsedMs: end - start, updates: value.updates?.length });
    layoutMs = 0;
    layoutReads = 0;
    // Includes synchronous snapshot replay/render and its queued highlight and
    // follow microtasks. scrollHeight measures their forced style/layout reads.
    setTimeout(() => report({ kind: 'snapshot-consumer', elapsedMs: performance.now() - end, layoutMs, layoutReads }), 0);
    return value;
  };
  function start(kind) {
    const run = ++serial;
    pending = { kind, start: performance.now(), run };
    requestAnimationFrame(function check() {
      if (!pending || pending.run !== run) return;
      const transcript = document.querySelector('[aria-label="Chat transcript"]');
      const cli = kind === 'cli';
      let last;
      if (transcript) {
        const walker = document.createTreeWalker(transcript, NodeFilter.SHOW_TEXT);
        for (let node; (node = walker.nextNode());) if (node.textContent.includes('SIM1770_LAST_ENTRY')) last = node.parentElement;
      }
      const bounds = last?.getBoundingClientRect();
      const visible = bounds && bounds.height > 0 && bounds.bottom > 0 && bounds.top < innerHeight;
      if ((cli && !transcript) || (!cli && visible)) {
        requestAnimationFrame(() => {
          if (!pending || pending.run !== run) return;
          const elapsed = performance.now() - pending.start;
          const text = transcript?.textContent ?? '';
          const entryCount = new Set(text.match(/SIM1770_ENTRY_\d+/g) ?? []).size;
          report({ kind, elapsedMs: elapsed, entryCount, lastBounds: bounds?.toJSON(), ...info() });
          let ready = document.getElementById('sim1770-ready');
          if (!ready) { ready = document.createElement('output'); ready.id = 'sim1770-ready'; ready.style.cssText = 'position:fixed;top:0;left:0;font-size:1px;pointer-events:none;z-index:2147483647'; document.body.append(ready); }
          ready.textContent = `SIM1770_READY_${kind}_${run}`;
          pending = undefined;
        });
      } else if (performance.now() - pending.start < 90000) requestAnimationFrame(check);
      else { report({ kind: 'timeout', transition: kind, ...info() }); pending = undefined; }
    });
  }
  document.addEventListener('click', (event) => {
    const button = event.target.closest('button');
    if (!button || !event.isTrusted || !button.closest('[aria-label="Terminal view"]')) return;
    const label = button.textContent.trim();
    report({ kind: 'tap', label, trusted: event.isTrusted });
    if (label === 'Chat') start('chat');
    if (label === 'CLI') start('cli');
  }, true);
  document.addEventListener('visibilitychange', () => {
    report({ kind: 'visibility', state: document.visibilityState });
    if (document.visibilityState === 'visible' && document.querySelector('[aria-label="Chat transcript"]')) start('resume');
  });
})();
