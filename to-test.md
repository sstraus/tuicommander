# To Test — index

Manual checks that need a live build, a phone, Windows or Boss's eye. The list is split by area so each file stays under the 28000-byte gate. Open items are the `- [ ]` rows in the area files; HUMAN and FAILED notes stay on the item.

**Add a new check** to the area file that matches its feature (add to the last file of that area when the area has several). Create a new `to-test/<area>-N.md` when the last file is near 26000 bytes. Keep every file under 28000 bytes.

**Record a verdict** in the area file that holds the item. A verified item leaves the open list: move its evidence to a new dated file under `to-test/archive/`, not back to this file. `[HUMAN]` and FAILED items stay with their note.

## AI Chat, ego and ACP

- [ai-chat-ego-1.md](to-test/ai-chat-ego-1.md) — 32 items
- [ai-chat-ego-2.md](to-test/ai-chat-ego-2.md) — 23 items
- [ai-chat-ego-3.md](to-test/ai-chat-ego-3.md) — 4 items

- [ ] After a manual rebuild/restart when losing current sessions is acceptable, ask Claude Code for an answer taller than the viewport and shrink the terminal height (and toggle a side panel to halve its width) while it streams. `session action=output format=log` must keep every answer row once. Rust does not hot-reload; no daily instance was restarted. _(FAILED 2026-11-05: repro (plain shell, throwaway PTY, not Claude): ./rows.sh prints ROW001..150 at 0.05s; resize 120c->10 rows, ->60c, ->30 rows/120c while streaming; GET /sessions/<id>/output?format=log lines show ROW102-ROW104 at idx 117-121 (cols 60) AND again at idx 166-168 (cols 120); ROW121-122 at 155-157 and 185-186: rows 102-122 logged twice. Other runs (height-only, 8 resizes) had each row once. Claude answer not tried (agent run).)_ _(2833-1cda: the width+height regrow now keeps pulled-back logged rows out of the log twice; after the rebuild re-run the plain-shell repro: every ROW once in format=log.)_
## Workflows, stories, plans and Design Mode

- [workflows-stories-1.md](to-test/workflows-stories-1.md) — 30 items
- [workflows-stories-2.md](to-test/workflows-stories-2.md) — 17 items
- [workflows-stories-3.md](to-test/workflows-stories-3.md) — 19 items
- [workflows-stories-4.md](to-test/workflows-stories-4.md) — 8 items
- [workflows-stories-5.md](to-test/workflows-stories-5.md) — 1 items

## Agents, hooks and PTY state

- [agents-pty-1.md](to-test/agents-pty-1.md) — 28 items
- [agents-pty-2.md](to-test/agents-pty-2.md) — 7 items

## Other areas

- [misc-1.md](to-test/misc-1.md) — 37 items
- [misc-2.md](to-test/misc-2.md) — 23 items
- [misc-3.md](to-test/misc-3.md) — 13 items
- [misc-4.md](to-test/misc-4.md) — 25 items
- [misc-5.md](to-test/misc-5.md) — 27 items
- [misc-6.md](to-test/misc-6.md) — 39 items
- [misc-7.md](to-test/misc-7.md) — 10 items

## Settings, themes, tabs, panels and terminal UI

- [settings-ui-1.md](to-test/settings-ui-1.md) — 24 items
- [settings-ui-2.md](to-test/settings-ui-2.md) — 32 items

## Mobile and phone PWA

- [mobile-1.md](to-test/mobile-1.md) — 36 items
- [mobile-2.md](to-test/mobile-2.md) — 29 items
- [mobile-3.md](to-test/mobile-3.md) — 12 items

## Windows lab and CI

- [windows.md](to-test/windows.md) — 17 items

## Voice, dictation, audio and Telegram

- [voice-telegram-1.md](to-test/voice-telegram-1.md) — 27 items
- [voice-telegram-2.md](to-test/voice-telegram-2.md) — 26 items
- [voice-telegram-3.md](to-test/voice-telegram-3.md) — 22 items
- [voice-telegram-4.md](to-test/voice-telegram-4.md) — 24 items
- [voice-telegram-5.md](to-test/voice-telegram-5.md) — 18 items

## Remote access, relay, daemon and MCP upstreams

- [remote-relay-1.md](to-test/remote-relay-1.md) — 29 items
- [remote-relay-2.md](to-test/remote-relay-2.md) — 8 items

## Git, GitHub and repositories

- [git-repos.md](to-test/git-repos.md) — 33 items

## Archive

Verified items with their evidence. Not open work.

- [2026-11-05-01.md](to-test/archive/2026-11-05-01.md)
- [2026-11-05-02.md](to-test/archive/2026-11-05-02.md)
- [2026-11-05-03.md](to-test/archive/2026-11-05-03.md)
- [2026-11-05-04.md](to-test/archive/2026-11-05-04.md)
- [2026-11-05-05.md](to-test/archive/2026-11-05-05.md)
- [2026-11-05-06.md](to-test/archive/2026-11-05-06.md)
- [2026-11-05-07.md](to-test/archive/2026-11-05-07.md)
- [2026-11-05-08.md](to-test/archive/2026-11-05-08.md)
- [2026-11-05-09.md](to-test/archive/2026-11-05-09.md)
- [2026-11-05-10.md](to-test/archive/2026-11-05-10.md)
