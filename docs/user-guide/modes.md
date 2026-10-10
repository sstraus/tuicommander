# TUICommander modes

TUICommander has one backend and several ways to connect to it. The available features depend on the client mode.

| Mode | How it is used | Best for | Main limitations |
|---|---|---|---|
| **Desktop app** | Launch TUICommander normally | Full local development workflow | None of the client-side limitations below |
| **Browser mode** | Open the local HTTP server in a browser | Remote control from a laptop or another desktop | Native dialogs, global hotkeys, updater, dictation, detached windows, and some file/clipboard integrations are desktop-only |
| **Mobile PWA** | Open the mobile endpoint from a phone/tablet | Monitoring agents and answering prompts | Deliberately reduced UI; not a replacement for the desktop workspace |
| **Remote daemon** | Run `tuic-remote` and connect through the remote-access flow | Hosting the backend on another machine | Requires separate remote configuration and network/security setup |

## Desktop app

The desktop app runs the Tauri shell and exposes the complete feature set: native file dialogs, IDE launchers, dictation, global hotkeys, detached panels, the complete Command Palette action set, and the native updater.

For local development, use `make dev` or `pnpm tauri dev`. Frontend files use Vite HMR; Rust changes require restarting the development process.

## Browser mode

The browser client uses the same backend over HTTP, WebSocket, and SSE. It is not a mock UI: sessions, terminal output, Git operations, settings, and most panels use the same backend as the desktop app.

Browser mode provides the Command Palette through the magnifying-glass toolbar button, with filename/content search over HTTP and only actions verified for the browser client. Native file pickers, global hotkeys, detached OS windows, dictation, IDE launchers, auto-updater, MCP/hooks configuration, and user-plugin installation from local files remain desktop-only.

See [Remote Access](remote-access.md) for setup and [Troubleshooting](troubleshooting.md) if the browser cannot connect.

## Mobile PWA

On iPad, the root URL opens the mobile interface by default. To use the full workspace, open Settings from the overflow menu and select **Open Desktop UI**. The device remembers your choice for future root visits. The desktop banner's **Switch** link returns to mobile and clears that choice. Both interfaces are within the installed PWA's scope; an older installation may need its manifest refreshed before the desktop link stays in the app window.

The mobile interface is optimized for observation and quick actions: inspect sessions, follow output, answer questions, and monitor agent activity. The Files tab lists configured repositories; long-press a repository path to see it in full, browse normal folders ahead of hidden ones, or search nested files. In a session, tap the folder button in the header to browse that session's repository directly; Back returns to the session with its output and unsent command intact. File editing uses the remaining screen height and wraps long lines. Use the desktop or full browser workspace for worktree management and complex Git operations.

## Remote daemon

`tuic-remote` provides a standalone backend process for remote access. It is useful when the machine running the repositories is different from the machine displaying the UI.

See [Remote Access](remote-access.md) for connection methods, TLS/relay options, and configuration details.
