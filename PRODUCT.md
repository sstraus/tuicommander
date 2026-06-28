# Product

## Register

product

## Users

Developers running multiple AI coding agents (Claude Code, Codex, Aider, Cursor, Gemini CLI) in parallel. Users who need to monitor agent status (rate limits, questions, idle/working), review diffs, merge PRs, and track usage without switching windows. Power users who value precision and efficiency over hand-holding.

## Product Purpose

TUICommander is an AI-native IDE for multi-agent development. It orchestrates parallel AI agents on isolated Git worktrees, provides real-time observability (rate limits, question detection, activity status), and integrates the full dev feedback loop (terminal output, diffs, PRs, CI status, file editor) in one workspace. Success means users can run 5+ agents simultaneously, spot problems immediately (rate-limited agent, awaiting-input prompt), and review/merge work without leaving the app.

## Platform

Desktop only (macOS, Linux, Windows). Built with Tauri v2 + SolidJS. No mobile web, no responsive breakpoints, no touch-target concerns. The companion mobile app (`src/mobile/`) is a separate Capacitor build with its own design constraints; it is NOT part of the desktop product's design scope.

## Brand Personality

Precise, no-nonsense, earned confidence. The tool for people who know what they're doing. Direct labels, zero marketing fluff, professional utility. TUICommander doesn't sell you on AI agents; it assumes you're already running them and gives you the observability layer the terminal doesn't provide. Voice is technical, exact, and unadorned.

## Anti-references

- **VS Code / generic Electron**: Avoid the standard sidebar-left + tabs-top + panels-bottom layout cliché. TUICommander's layout is distinct; don't converge toward the Electron-IDE reflex.
- **Dense terminal nostalgia (ncurses aesthetic)**: Not retro ASCII-art chrome or a throwback to 1980s CUI. Modern native UI with clean typography and clarity.
- **Slack / chatty SaaS polish**: No rounded-everything, emoji reactions, or consumer-app friendliness. This is a developer tool, not a social app.

## Design Principles

1. **Practice what you preach** — TUICommander is built for AI agents; its own UI must be agent-readable and scriptable (HTTP API, MCP server, CLI companion). Design decisions that make the UI harder to automate contradict the product's purpose.

2. **Observability, not decoration** — Every UI element must answer "what's happening?" or "what can I do?" Status indicators (rate limit countdowns, question badges, activity dots) are signal, not noise. Avoid decoration that doesn't inform.

3. **Density earns its place** — Information-dense UI is correct when users need to monitor many parallel sessions at once. But density without hierarchy is chaos; use typography scale, spacing rhythm, and color sparingly to guide the eye.

4. **Expert confidence** — Users chose TUICommander because they outgrew tmux + manual window juggling. The UI can assume technical fluency. Don't hide power behind "simple mode" or tutorial overlays. Show the full interface, label it clearly, trust users to learn.

5. **No lost threads** — The core problem is context loss across many terminal windows. Every design decision that increases "where was that again?" friction (hidden panels, deeply nested menus, unclear tab states) undermines the product.

## Accessibility & Inclusion

WCAG AA baseline: accessible contrast ratios (4.5:1 for text, 3:1 for UI components), full keyboard navigation, semantic HTML where applicable (Tauri webview). No extra constraints beyond standard web accessibility.
