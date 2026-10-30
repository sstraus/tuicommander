# Claude preview question (wide) — 2536-8e18

Source: real Claude Code PTY, recorded on 2026-10-30 through
`/diagnostics/capture` on an isolated debug `tuic-remote --instance`.
The agent capture driver used a throwaway cwd and its normal authenticated agent
HOME. No screenshot reconstruction or synthetic agent output was used.
The original capture is retained under `~/Gits/.tmp/chat-question-preview/live-wide/`.
The `.tcap` bytes are unchanged. Its header records the real geometry.

The native AskUserQuestion has three question tabs, a wrapped recommended label,
a right-hand bordered preview and an unnumbered Chat about this action.
Replay selects the last screen with the column-zero native selection footer,
not the later process-exit screen. The 65-column recording still uses side-by-side
panes. A stacked-layout regression derives its rows from that real screen by
moving the same preview below the same options; it is not a live stacked capture.
A separate 40-column live probe also retained side-by-side panes and truncated
its preview. No claim is made that this installed version emits a stacked pane.

SHA-256: `cca9d3aef18452171392312d87751f568282159a63abd92d7e4883412423f035`.
