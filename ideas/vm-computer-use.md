# VM / Computer Use — A Linux Desktop an Agent Can Drive (OpenMausBot as Reference)

**Status:** `concept` — parked, no decision (Boss, 2026-10-10: "save it in the ideas, then we see how it ends")
**Priority:** TBD
**Category:** Agents / Execution targets
**Date:** 2026-10-10

## Request

Boss wants the OpenMausBot (OMB) VM and computer-use study kept as an idea, to
decide later. Nothing is approved. No code, plan, or story exists.

Study (full evidence, file:line references, effort table):
`/Users/stefano.straus/Gits/personal/orchestrator/research/competitors/omb-vm-computer-use.md`
(Italian twin: `omb-vm-computer-use_it.md` in the same folder). The study is a
source review only: nothing was run. It expires 2026-11-09.

## What it is

OMB's "Local VM" is not a hypervisor. It is a Linux desktop (XFCE) in a
container, started through the Docker, Podman, or Apple `container` CLI that is
already installed. Apple `container` adds a lightweight VM below that CLI.

- **Agent control:** Cua Driver runs in the container (`cua-driver mcp`) and
  gives the agent screenshot, click, and type tools over MCP. It is not noVNC.
- **Human view:** a noVNC (RFB) panel shows the live desktop. The person can
  take control; the agent's tool calls are then refused, not queued.
- **Other targets in OMB:** direct host control, a container on a user VPS over
  SSH, and a paid cloud computer ("Boat"). Each is a separate adapter.

## Why it could matter for TUIC

- Agents get a GUI they can break without touching the host: browser, desktop
  apps, installers, UI tests of a build.
- TUIC already aggregates MCP servers (`upstream__tool`) and has an Axum WS
  server, so the control path is mostly reuse.
- Complements Design Mode (a separate headed Chrome) with a full desktop.

## Relation to Agent Sandbox

`ideas/agent-sandbox.md` (local file; `ideas/` is gitignored, so it is not on
every branch) is about restricting the agent's own subprocesses on the host
with a kernel sandbox (Seatbelt, Landlock, AppContainer). This idea is about a
separate desktop the agent drives. They overlap only in "isolation":

- Agent Sandbox protects the host from the agent's shell commands (security).
- VM / computer use gives the agent a GUI to operate (capability). The
  container limits (memory, CPU, PIDs, dropped capabilities) are a side effect.
  OMB does not prove that outbound network is blocked.

Neither one replaces the other. If both are built, they share nothing in code.

## Feasibility on Tauri + Solid (inference from the study)

Feasible. Electron is only the host for the viewer and the native helper; it is
not needed for the isolation.

- Solid panel with `@novnc/novnc` (`onMount`/`onCleanup`), RFB relayed by the
  existing Axum WS + tokio-tungstenite.
- Lifecycle in Rust with `tokio::process::Command` (Docker, Podman, Apple CLI).
- Agent control: spawn `cua-driver mcp` in the container and reuse the MCP hub.
- Host control stays on MacControl. Do not write a new input engine.

The study estimates 2–4 weeks for one integrated prototype (Docker/Podman, Cua
MCP, noVNC panel, human handover) and 6–10 weeks for a release on three host
systems. These are planning ranges, not measurements.

## Main costs and risks

- Needs a working container runtime on the host. TUIC would ask the user to
  install it, or own the setup flow.
- Image pin, build, and cleanup. Wrong-target deletion is the main lifecycle bug.
- Lease and handover: no agent action after the person takes control.
- noVNC in WKWebView / WebView2 / WebKitGTK: modifiers, IME, DPI, clipboard,
  focus, hidden panels. Untested.
- Release: Cua driver redistribution terms, signing, notarization per target.
- Apple `container` needs macOS 26 and Apple Silicon. Linux guest only; no
  managed macOS or Windows guest.
- No snapshot or checkpoint of the VM in OMB. Persistence is a bind-mounted
  host workspace.

## Open questions

- Is a GUI sandbox a real need for TUIC users, or is the PTY-only agent enough?
- Cua licence and driver versions (host 0.28.2, Linux host 0.19.3, container
  0.20.0 differ in OMB): can TUIC ship or pin it?
- Does the Cua driver embed cleanly as a sidecar, or must the MCP process stay
  separate (the Rust side cannot import the JS SDK)?
- Live FPS and input latency over RFB: unknown; measure before any video stream.
- Own the runtime install, or require it? Podman vs Docker default?
- Should VPS or cloud targets exist, or only local?
- Where does it sit relative to `tuic-remote` and the remote browser mode?

## Next step

None. If Boss picks it up: run the study's prototype scope first (one Linux
desktop, installed runtime, Cua MCP, Solid noVNC panel, human handover), and
re-check the study if it is past its 2026-11-09 expiry.
