// Run with agent-browser eval --stdin on dotPulse.html served by its Vite config.
// Catches a dot/space pulse changing wrapped body positions or block height.
(() => {
  const results = [];
  const snapshot = () => {
    const line = document.querySelector("pre > div");
    const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT);
    const nodes = [];
    let node;
    let text = "";
    while ((node = walker.nextNode())) {
      nodes.push({ node, start: text.length });
      text += node.textContent;
    }
    const begin = text.indexOf("Bash(");
    if (begin < 0) throw new Error("Recorded tool header was not rendered");
    const chars = [];
    for (let i = begin; i < text.length; i++) {
      if (text[i] === " ") continue;
      const n = nodes.findLast((entry) => entry.start <= i);
      const range = document.createRange();
      range.setStart(n.node, i - n.start);
      range.setEnd(n.node, i - n.start + 1);
      const rect = range.getBoundingClientRect();
      chars.push([rect.x, rect.y]);
    }
    const rowStarts = [...new Set(chars.map(([, y]) => y))].map((y) => chars.find((char) => char[1] === y)[0]);
    return {
      rowCount: rowStarts.length,
      bodyX: chars[0][0],
      continuationXs: rowStarts.slice(1),
      height: line.getBoundingClientRect().height,
      afterY: document.querySelector("pre").lastElementChild.getBoundingClientRect().y,
      chars,
    };
  };
  const body = window.dotSource;
  window.setDotFrame("on", body);
  const maxRows = snapshot().rowCount;
  if (innerWidth < 600 && maxRows < 3) throw new Error("Phone fixture must exercise at least three rows");
  const cases = [];
  // Select excerpts by measured DOM rows, not canvas font guesses. A partial
  // last word can move a wrap, so measure each candidate with the actual CSS.
  for (const expectedRows of [1, 2, 3]) {
    if (expectedRows > maxRows) continue;
    let selected;
    for (let end = 12; end <= body.length; end++) {
      const text = body.slice(0, end);
      window.setDotFrame("on", text);
      if (snapshot().rowCount === expectedRows) {
        selected = text;
        break;
      }
    }
    if (!selected) throw new Error(`Cannot select a ${expectedRows}-row excerpt`);
    cases.push({ name: `${expectedRows} rows`, text: selected, expectedRows });
  }
  cases.push({ name: "recorded tool", text: body, expectedRows: maxRows });
  for (const { name, text, expectedRows } of cases) {
    const frames = [];
    for (const state of ["on", "off", "green"]) {
      window.setDotFrame(state, text);
      frames.push(snapshot());
    }
    const stable = frames.every((frame) => JSON.stringify(frame) === JSON.stringify(frames[0]));
    const correctRows = frames.every((frame) => frame.rowCount === expectedRows);
    // Absolute geometry: continuation starts must align with Bash, not the dot.
    const aligned = frames.every((frame) => frame.continuationXs.every((x) => Math.abs(x - frame.bodyX) < 0.02));
    results.push({
      name, expectedRows, rows: frames.map((frame) => frame.rowCount),
      heights: frames.map((frame) => frame.height),
      bodyXs: frames.map((frame) => frame.bodyX),
      continuationXs: frames.map((frame) => frame.continuationXs),
      stable, correctRows, aligned,
    });
  }
  const report = {
    ua: navigator.userAgent,
    webdriver: typeof navigator.webdriver,
    touch: navigator.maxTouchPoints,
    viewport: [innerWidth, innerHeight],
    skippedRows: [1, 2, 3].filter((rows) => rows > maxRows),
    results,
  };
  if (results.some((result) => !result.stable || !result.correctRows || !result.aligned)) {
    throw new Error("Dot pulse moved mobile body: " + JSON.stringify(report));
  }
  return report;
})()
