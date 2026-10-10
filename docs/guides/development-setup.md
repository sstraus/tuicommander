# Development Setup

## Prerequisites

The macOS desktop app supports macOS 12 Monterey or later and requires system WebKit updated to Safari 17.4 or later for Mermaid diagrams. The bundle declares macOS 12.0 as its minimum; this does not guarantee that Safari updates are installed. The Rust deployment target stays at 10.15 for binary compilation. Windows uses the Evergreen WebView2 runtime; Linux builds use WebKitGTK 4.1 API packages.

- **Node.js 24+** (the repository version is pinned in [`.nvmrc`](https://github.com/sstraus/tuicommander/blob/main/.nvmrc))
- **Rust** (stable toolchain via rustup)
- **Tauri CLI** (`cargo install tauri-cli`)
- **git** and **gh** (GitHub CLI) for git/GitHub features

> **Windows users:** See the [Windows-specific prerequisites](#windows-prerequisites) section below before proceeding.

## Install Dependencies

```bash
pnpm install
```

### TypeScript target

The app uses `ES2024` for the TypeScript target and standard library, with
`DOM` and `DOM.Iterable` types. Vite keeps its existing `esnext` build target;
it does not lower ES2024 syntax to ES2021. This configuration does not add
runtime polyfills.

### Paired Tauri dependencies

Update each Tauri Rust plugin and its npm counterpart together. Notification stays
on `~2.4.0` and updater on `~2.11.0` in both manifests until their next minor
versions are validated together. The direct `rfd` dependency must match the
version and features used by `tauri-plugin-dialog`; dialog 2.8.1 uses `rfd 0.16`.

### Temporary dependency audit exception

`pnpm-workspace.yaml` configures `auditConfig.ignoreGhsas` for
[GHSA-vfj7-8cjw-p6xm](https://github.com/advisories/GHSA-vfj7-8cjw-p6xm) only.
Boss approved this exception on 2026-10-04; it expires on **2026-11-04**.
At approval time, `braces` 3.0.3 had no patched version on npm (`3.0.4`
returned 404). Its only dependency path is build-only:
`vite-plugin-purgecss → purgecss → fast-glob → micromatch → braces`.
The advisory concerns stack exhaustion from deeply nested patterns. This path
processes repository build inputs, rather than runtime terminal or Markdown
content. Remove the exception when a patch becomes available, and review it
by the expiry date; pnpm does not enforce the date automatically.
No other advisory is ignored. `pnpm-workspace.yaml` pins DOMPurify 3.4.16
for the application and Mermaid, and the development-only `qs` to 6.16.0.
Rechecked on 2026-10-09: npm still publishes 3.0.3 as latest, and the advisory
still lists no patch. The dependency tree still has only the build path above;
retain the existing exception and its expiry. The frontend audit now requires
Seroval >=1.6.3 and source-map-js >=1.2.2. KaTeX 0.16.47 remains under
Mermaid 11; its patched >=0.18.2 line requires the separate Mermaid upgrade
and browser compatibility review. No KaTeX advisory is ignored.
pnpm 11 ignores the legacy `pnpm.auditConfig` field in `package.json`.

## Development

### Native Tauri App

```bash
make dev
```

Starts the Vite dev server and Tauri app. Frontend files use Vite HMR; Rust changes require restarting the development process.
On macOS, `make dev` requires Python 3 and passes a [Cargo target runner](https://doc.rust-lang.org/cargo/reference/config.html#targetcfgrunner) to Tauri's `cargo run`. The runner copies the desktop executable and its `tuic-remote`, `tuic-bridge`, and `tuic` siblings into `~/Library/Application Support/com.tuic.commander/dev-bin/` before launch. These are real copies, not links into Cargo or mbx targets. Target cleanup can no longer remove the running desktop executable. The path stays the same across runs; files are replaced, not accumulated, and remain after exit. A second launch at that path is rejected while the runner holds its process lock. `TUIC_DEV_BIN_DIR` overrides the directory for isolated script checks. Direct `pnpm tauri dev` does not use this protection. Linux and Windows keep the normal Cargo launch.

The existing MCP bridge installation still verifies and preserves its own revisions; the dev runner keeps its adjacent source available. The bridge installer runs inside the application, so it cannot protect the desktop executable before launch.

The macOS application firewall uses code signing for initial and tracking decisions ([Apple TN2206](https://developer.apple.com/library/archive/technotes/tn2206/)). Its UI also selects applications by filesystem path ([Apple firewall settings](https://support.apple.com/guide/mac-help/block-connections-to-your-mac-with-a-firewall-mh34041/mac)). Both the executable path and signing identity matter: a stable existing path contains the missing-executable failure, but does not promise unchanged firewall permission after a rebuild changes the signature. Confirm LAN and tailnet access after the next launch; no firewall setting is changed by the runner.
Vite excludes backend files and repository tooling output such as `.tmp/`, `target/`, `dist/`, coverage, reports, plans, stories, and docs from its watcher. Mutation testing keeps its disposable checkout under `.tmp/`; its HTML files must not reload the live WebView. Changes to this watch configuration require restarting the Vite dev server.
The watch allowlist and Tauri version are resolved from the directory containing `vite.config.ts`, so starting Vite from another working directory still uses this checkout's inputs.

### Browser Mode

When the MCP server is enabled in settings, the frontend can run standalone:

```bash
pnpm dev
```

Connects to the Rust HTTP server via WebSocket/REST.

> **Note:** `pnpm dev` runs `scripts/dev-server.mjs`, not `vite` directly. The dev server is pinned to port 1421 (Tauri's `devUrl`), so the launcher checks the port first: if this checkout is already serving there it prints `reusing it` and exits 0 — the second Tauri app attaches to the running server. Starting a second Vite would wipe the shared `node_modules/.vite/deps` cache and kill hot reload for the session already running. If the port is held by another checkout or a stray process, the launcher fails with an explicit message instead of serving the wrong sources.

## Build

```bash
pnpm tauri build
```

Produces platform-specific installers:
- macOS: `.dmg` and `.app`
- Windows: `.nsis` (`.exe` setup installer)
- Linux: `.deb` and `.AppImage`

> **Note:** The `.msi` bundle may fail on Windows due to WiX tooling issues. Use `--bundles nsis` to produce a working `.exe` installer:
> ```bash
> cargo tauri build --bundles nsis
> ```

## Testing

Rust socket tests use `tuic-test-support::short_socket_test_temp_root()`. If the
checkout's test path exceeds the Unix socket limit, the helper uses a short
checkout-specific directory under the nearest short `Gits/.tmp/s<checkout-hash>`.
Outside `Gits`, or when all such ancestors are too long, it uses
`/tmp/tuic-s<checkout-hash>`. It measures every candidate against the socket
budget, including the full mdkb staging filename and eight bytes of margin. The
`scripts/with-test-tmp.sh` wrapper removes abandoned Gits socket directories and
test-run directories under `~/Gits/.tmp/tuic-tests` and the checkout's
`.tmp/tuic-tests` after they have been unused for more than seven days. Run
standalone Rust tests through that wrapper.

### Live peer-mail wake canary

After a Rust rebuild, run this against the isolated test instance, not the
orchestrator instance. The script starts a disposable managed peer through MCP
`agent action=spawn`, waits for its idle composer, sends mail from a separate
MCP identity, checks that `PEER_MAIL_WAKE` appears in the PTY within 20 seconds,
then closes the PTY and its MCP connections. It exits with an error if readiness,
delivery, or cleanup fails.

```bash
TUIC_APP_INSTANCE=peer-mail-canary make dev  # in a separate terminal; use its reported HTTP port
TUIC_CANARY_URL=http://127.0.0.1:9877 python3 scripts/canary-peer-mail-wake.py claude
TUIC_CANARY_URL=http://127.0.0.1:9877 python3 scripts/canary-peer-mail-wake.py claude --capacity
```

`--capacity` sends 100 messages to the disposable agent, reads them through a
second MCP connection bound to that agent, then checks that the next send and
read succeed. Use `codex` instead of `claude` for the optional Codex check. The
agent CLI must already be installed and authenticated. Run the script only
after the test instance is listening on the URL you supply.

```bash
pnpm test              # Run all tests
pnpm test:coverage     # Coverage report
```

**Test tiers:**
- Tier 1: Pure functions (utils, type transformations)
- Tier 2: Store logic (state management)
- Tier 3: Component rendering
- Tier 4: Integration (hooks + stores)

**Framework:** Vitest + SolidJS Testing Library + happy-dom

**Coverage:** ~80%+

### Rust build features

Plain application Cargo builds enable `desktop` but omit native speech. Add
`--features dictation` for Whisper/WebRTC and the voice adapters. Tauri dev and
release builds read `build.features = ["dictation"]` from `tauri.conf.json`, so
`make dev` and `make build` retain voice support. Desktop CI tests that feature;
headless daemon checks use `--no-default-features`. Workspace tests also select
the `tuic-dictation` crate explicitly, and therefore compile its native stack.

TLS uses the existing ring provider. Provider-free reqwest/axum-server features
avoid compiling AWS-LC while retaining HTTPS and WSS.

### Rust tests in linked worktrees

Run standalone Rust tests through `scripts/with-test-tmp.sh`. In a linked
worktree, the wrapper clears an inherited `CARGO_TARGET_DIR`, so Cargo selects
that checkout's artifacts instead of a target supplied by the parent
TUICommander process. The worktrees' `tuic-terminal` builds have the same Cargo
fingerprint key, while [Cargo checks path sources by file mtime](https://doc.rust-lang.org/stable/nightly-rustc/cargo/core/compiler/fingerprint/index.html);
sharing one target can therefore make an older checkout look fresh against an
artifact from another checkout. The wrapper also keeps test scratch files under
`~/Gits`.

### TypeScript mutation testing

Run Stryker on the changed source files and their relevant Vitest files. Keep both
lists narrow; a mutation run executes the selected tests for each mutant.

```bash
node scripts/ts-mutants.mjs \
  'src/utils/pathUtils.ts:65-65' -- \
  'src/__tests__/utils/pathUtils.test.ts'
```

List the changed source files or line ranges before `--`, and the tests that
exercise them after it. Read
`reports/mutation/mutation.json` and the console verdicts. A known behavior
change, such as inverting the `pathBasename` condition above, must be reported
`Killed` before treating the other verdicts as evidence. Install this checkout's
dependencies with `pnpm install --offline --frozen-lockfile` when its local
`node_modules/.bin/stryker` is missing; linked worktrees do not share the main
checkout's executables. The checked-in Stryker config copies frontend inputs,
Rust `.rs` files under `src-tauri/src/`, the terminal-grid source, and the generated
`command_table_paths.txt` into its sandbox. Add any new files read directly by a
Vitest test to that copy policy. It leaves the worktree source untouched and runs
Vitest as a separate command for each mutant. Stryker 10's
Vitest runner reports false survivors with this repo's Vitest 5, so it is not
used. Stryker's TypeScript preprocessor is pointed at an absent file because
this repo's TypeScript 7 does not expose the compiler API it calls; Vitest still
transforms and runs the TypeScript tests.

## Project Structure

See [Architecture Overview](../architecture/overview.md) for full directory structure.

## Key Files

| File | Purpose |
|------|---------|
| `src/App.tsx` | Central orchestrator |
| `src-tauri/src/lib.rs` | Rust app setup, command registration |
| `src-tauri/src/pty.rs` | PTY session management |
| `src/hooks/useAppInit.ts` | App initialization |
| `src/stores/terminals.ts` | Terminal state |
| `src/stores/repositories.ts` | Repository state |
| `SPEC.md` | Feature specification |
| `ideas/index.md` | Feature concepts under evaluation |

## Configuration

App config stored in platform config directory:
- macOS: `~/Library/Application Support/tuicommander/`
- Linux: `~/.config/tuicommander/`
- Windows: `%APPDATA%/tuicommander/`

See [Configuration docs](../backend/config.md) for all config files.

## Makefile Targets

```bash
make dev         # Tauri dev mode
make build       # Production build
make test        # Run tests
make lint        # Run linter
make docs        # Build this documentation + search index into docs/book
make docs-serve  # …and serve it at http://127.0.0.1:8123
make clean       # Clean build artifacts
```

---

## Documentation Site

The book you are reading is [mdBook](https://rust-lang.github.io/mdBook/), built
by `scripts/build-docs.sh` and deployed to GitHub Pages by
`.github/workflows/website.yml`. The script is the single source of truth — CI
runs the same one, so a local build matches the deployed site.

```bash
make docs-serve     # build + serve; open http://127.0.0.1:8123
```

Requires `mdbook` (`brew install mdbook` or `cargo install mdbook`) and `npx`.

Search is [Pagefind](https://pagefind.app/), generated after the mdBook build,
which is why `file://` previews have no search — the index is fetched over HTTP.
mdBook's own elasticlunr search is disabled in `book.toml`.

Adding a page means adding it to `SUMMARY.md`: mdBook only renders — and
Pagefind only indexes — chapters listed there.

---

## Windows Prerequisites

Building on Windows requires a few extra tools beyond the standard prerequisites. Install them in this order.

### 1. Visual Studio Build Tools (C++ compiler)

Download and install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) and select the **"Desktop development with C++"** workload. VS Build Tools 2019 or later is fine.

Or via winget:
```powershell
winget install Microsoft.VisualStudio.2022.BuildTools
```

### 2. Rust

```powershell
winget install Rustlang.Rustup
```

Restart your terminal after installation, then verify:
```powershell
rustc --version
cargo --version
```

### 3. Node.js

```powershell
winget install OpenJS.NodeJS.LTS
```

### 4. CMake

Required to compile `whisper-rs` (the on-device dictation library).

```powershell
winget install Kitware.CMake
```

### 5. LLVM 18 (libclang — required for whisper-rs bindings)

`whisper-rs` uses `bindgen` to generate Rust bindings for `whisper.cpp`, which requires `libclang`. **Use LLVM 18** — LLVM 19+ produces broken bindings for this crate on Windows.

Download the LLVM 18 installer from [GitHub releases](https://github.com/llvm/llvm-project/releases/tag/llvmorg-18.1.8) (`LLVM-18.1.8-win64.exe`) and install it. Then set the environment variable so `bindgen` can find it:

```powershell
# Add to your PowerShell profile or set permanently in System Environment Variables
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
```

> If you have LLVM 22+ installed (e.g. from winget), install LLVM 18 to a separate directory and point `LIBCLANG_PATH` there instead.

### 6. Tauri CLI

```powershell
cargo install tauri-cli --version "^2"
```

### Full Windows Build Command

Always set `LIBCLANG_PATH` before building:

```powershell
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"   # adjust path if LLVM 18 is elsewhere
cargo tauri build --bundles nsis
```

The installer will be at:
```
src-tauri\target\release\bundle\nsis\TUICommander_0.x.x_x64-setup.exe
```

### Windows Known Issues

| Symptom | Cause | Fix |
|---|---|---|
| `whisper-rs-sys` build fails with "couldn't find libclang" | LLVM not installed or `LIBCLANG_PATH` not set | Install LLVM 18 and set `LIBCLANG_PATH` |
| `whisper-rs-sys` compile error: `attempt to compute 1_usize - 296_usize` | LLVM 19+ generates broken bindings for this crate | Use LLVM 18 specifically |
| WiX `.msi` bundle fails | WiX `light.exe` tooling issue | Use `--bundles nsis` instead |
| App window opens but shows a **black screen** | Navigation guard in `lib.rs` blocked `http://tauri.localhost/` (Windows' internal Tauri URL) | Fixed in current code — `tauri.localhost` is explicitly allowed |
| `Update check failed: windows-x86_64-nsis not found` | Custom local build isn't listed in official release manifest | Harmless — auto-update simply won't trigger |
