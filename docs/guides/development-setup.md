# Development Setup

## Prerequisites

- **Node.js** (LTS)
- **Rust** (stable toolchain via rustup)
- **Tauri CLI** (`cargo install tauri-cli`)
- **git** and **gh** (GitHub CLI) for git/GitHub features

## Install Dependencies

```bash
npm install
```

## Development

### Native Tauri App

```bash
npm run tauri dev
```

Starts Vite dev server + Tauri app with hot reload.

### Browser Mode

When the MCP server is enabled in settings, the frontend can run standalone:

```bash
npm run dev
```

Connects to the Rust HTTP server via WebSocket/REST.

## Build

```bash
npm run tauri build
```

Produces platform-specific installers:
- macOS: `.dmg` and `.app`
- Windows: `.msi` and `.exe`
- Linux: `.deb` and `.AppImage`

## Testing

```bash
npm test              # Run all tests
npm run test:watch    # Watch mode
npm run test:coverage # Coverage report
```

**Test tiers:**
- Tier 1: Pure functions (utils, type transformations)
- Tier 2: Store logic (state management)
- Tier 3: Component rendering
- Tier 4: Integration (hooks + stores)

**Framework:** Vitest + SolidJS Testing Library + happy-dom

**Coverage:** ~80%+ (830 tests)

## Project Structure

See [Architecture Overview](../architecture/overview.md) for full directory structure.

## Key Files

| File | Purpose |
|------|---------|
| `src/App.tsx` | Central orchestrator (829 lines) |
| `src-tauri/src/lib.rs` | Rust app setup, command registration |
| `src-tauri/src/pty.rs` | PTY session management |
| `src/hooks/useAppInit.ts` | App initialization |
| `src/stores/terminals.ts` | Terminal state |
| `src/stores/repositories.ts` | Repository state |
| `SPEC.md` | Feature specification |
| `IDEAS.md` | Feature concepts under evaluation |

## Configuration

App config stored in platform config directory:
- macOS: `~/Library/Application Support/tui-commander/`
- Linux: `~/.config/tui-commander/`
- Windows: `%APPDATA%/tui-commander/`

See [Configuration docs](../backend/config.md) for all config files.

## Makefile Targets

```bash
make dev      # Tauri dev mode
make build    # Production build
make test     # Run tests
make lint     # Run linter
make clean    # Clean build artifacts
```
