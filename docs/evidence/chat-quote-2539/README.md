# Chat quote reply and feedback — evidence (story 2539-6c53)

`shots/` holds before/after captures in Commander (dark) and Paper (light), desktop (1100 px) and phone (390 px) widths.
The host (`host.tsx`) mounts the real `Transcript` — the renderer Terminal Chat and AI Chat share — above a draft box on the real prompt-draft store.
`before-menu-*`: no host quote hook, as Terminal Chat was before this change (menu has Copy and raw only).
`after-menu-*`: quote reply, Approve and Disapprove appear. `after-drafted-*`: after Disapprove, the quote and verdict are in the draft; nothing is sent.

Reproduce: `./node_modules/.bin/vite --config docs/evidence/chat-quote-2539/vite.config.ts`, then open
`http://127.0.0.1:18440/docs/evidence/chat-quote-2539/index.html?theme=dark|light[&before]`.
