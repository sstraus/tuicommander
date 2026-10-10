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
    return {
      height: line.getBoundingClientRect().height,
      afterY: document.querySelector("pre").lastElementChild.getBoundingClientRect().y,
      chars,
    };
  };
  const body = window.dotSource;
  const pre = document.querySelector("pre");
  const ctx = document.createElement("canvas").getContext("2d");
  ctx.font = getComputedStyle(pre).font;
  const columns = Math.floor(pre.getBoundingClientRect().width / ctx.measureText(" ").width);
  for (const [name, text] of [
    ["unwrapped", body.slice(0, 20)],
    ["two rows", body.slice(0, columns + 12)],
    ["three rows", body.slice(0, columns * 2 + 12)],
    ["recorded tool", body],
  ]) {
    const frames = [];
    for (const state of ["on", "off", "green"]) {
      window.setDotFrame(state, text);
      frames.push(snapshot());
    }
    const stable = frames.every((frame) => JSON.stringify(frame) === JSON.stringify(frames[0]));
    results.push({ name, heights: frames.map((frame) => frame.height), stable });
  }
  const report = {
    ua: navigator.userAgent,
    webdriver: typeof navigator.webdriver,
    touch: navigator.maxTouchPoints,
    viewport: [innerWidth, innerHeight],
    results,
  };
  if (results.some((result) => !result.stable)) {
    throw new Error("Dot pulse moved mobile body: " + JSON.stringify(report));
  }
  return report;
})()
