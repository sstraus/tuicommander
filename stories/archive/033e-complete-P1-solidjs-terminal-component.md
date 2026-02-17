---
id: "033e"
title: "SolidJS Terminal component with xterm.js"
status: pending
priority: P1
created: 2026-02-04T13:00:00.000Z
updated: 2026-02-04T13:00:00.000Z
dependencies: ["033a", "033b", "033c", "033d"]
blocks: ["033f", "033g"]
---

# SolidJS Terminal component with xterm.js

## Problem Statement

The terminal component is the most complex - it wraps xterm.js, handles PTY I/O, zoom, and resize. This is the core component that must work correctly.

## Current Implementation Analysis

```typescript
// From main.ts - terminal creation
function createTerminalInstance() {
  const terminal = new Terminal({ fontSize, fontFamily, theme, ... });
  const fitAddon = new FitAddon();
  terminal.loadAddon(fitAddon);
  terminal.loadAddon(new WebLinksAddon());
  return { terminal, fitAddon };
}

// PTY I/O
terminal.onData(data => invoke("write_pty", { sessionId, data }));
terminal.onResize(({ rows, cols }) => invoke("resize_pty", { ... }));

// Focus handling
terminal.textarea?.addEventListener("focus", () => setActiveTerminal(id));
```

## SolidJS Component Design

```typescript
interface TerminalProps {
  id: string;
  sessionId: string | null;
  fontSize: number;
  fontFamily: string;
  isActive: boolean;
  onFocus: () => void;
  onData: (data: string) => void;
  onResize: (rows: number, cols: number) => void;
}

function Terminal(props: TerminalProps) {
  let containerRef: HTMLDivElement;
  let terminal: XTerminal;
  let fitAddon: FitAddon;

  onMount(() => {
    terminal = new XTerminal({ ... });
    fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(containerRef);
    fitAddon.fit();

    terminal.onData(props.onData);
    terminal.onResize(({ rows, cols }) => props.onResize(rows, cols));
  });

  onCleanup(() => terminal.dispose());

  createEffect(() => {
    terminal.options.fontSize = props.fontSize;
    fitAddon.fit();
  });

  return <div ref={containerRef} class="terminal-container" />;
}
```

## Acceptance Criteria

- [ ] Create src/components/Terminal/Terminal.tsx
- [ ] Integrate xterm.js using ref and onMount
- [ ] Handle PTY data events via props callbacks
- [ ] Reactive fontSize/fontFamily via createEffect
- [ ] ResizeObserver for container size changes
- [ ] Proper cleanup on unmount (dispose terminal)
- [ ] Focus management syncs with store
- [ ] Maintain Tokyo Night theme

## Technical Challenges

1. **xterm.js DOM integration**: Must use ref + onMount, can't be reactive
2. **Resize handling**: Need ResizeObserver + fitAddon.fit()
3. **Event cleanup**: Must remove listeners on unmount
4. **Data flow**: Props down (fontSize), events up (onData, onResize)

## Files

- src/components/Terminal/Terminal.tsx
- src/components/Terminal/index.ts
- src/components/Terminal/theme.ts (extract theme config)
