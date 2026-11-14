# Website redesign for TUICommander 2.0: proposal

- **Status:** proposal (phase 1), revised after the Fable review (`~/Gits/.tmp/results/consult-fable-20261025-182701-1792949369.md`) and the coordinator's decisions of 2026-10-25. No site code is changed yet.
- **Date:** 2026-10-25. **Branch:** `feat/website-2.0`.
- **Request (Boss, 2026-10-25):** redesign the site, put the new things in evidence, add the presentation video, restructure it and give room to more content.
- **Sources read:** `website/` (all files), `plans/archive/marketing-website.md`, `DESIGN.md`, `PRODUCT.md`, `CHANGELOG.md` headings for `[2.0.0-beta.1]`/`[2.0.0-beta.2]`, `docs/user-guide/*`, `docs/FEATURES.md`, `src/agents.ts`, `.github/workflows/website.yml`, `tuic-presentation/script-v3.md` (as-built table) and `tuic-presentation/docs/2.0-feature-analysis.md` (a scored map of the 2.0 changelog).

## 1. The current site and its gaps

### Site map today

| URL | Content |
|---|---|
| `/` (`index.html`, 885 lines) | Nav · hero (tagline, three pillars, Download/GitHub/Nightly, install tabs, screenshot with a YouTube facade for the 1.x video `YBtD9g-PFJs`) · agent grid · "The problem" · 12 feature blocks (parallel sessions, worktrees, observability, Progress, AI Chat, diffs, global workspace, remote, MCP + plugins, automations, customize, voice) · terminal features · 14 FAQ entries · bottom CTA · footer |
| `/blog.html` + `/blog/*.html` | 31 posts, hand-written list |
| `/docs/` | mdBook built from `docs/` by `scripts/build-docs.sh` in the same Pages workflow |
| `/install.sh`, `/install.ps1` | Installers |

Deploy: `.github/workflows/website.yml`, on push to `main` under `website/**` or `docs/**`, to GitHub Pages. The installers and the film use `tuicommander.com`, so a custom domain is set in the Pages settings (inferred: no `CNAME` file in the repo).

### Gaps

| # | Gap | Evidence |
|---|---|---|
| G1 | One page holds everything. Twelve features at equal weight, no hierarchy, no room to explain any of them. A 2.0 feature would be block 13. | `index.html` sections 213–671 |
| G2 | Nothing of 2.0 is visible: no Stories/Kanban, Workflow designer, Tasks (GitHub/Jira/GitLab), PR line review, CLI \| Chat, rich sidebar cards, `+` agent menu, Progress Flow, Design Mode, secrets, themes, settings search. | grep of `index.html` |
| G3 | The video is the 1.x YouTube cut. The 2.0 film (3:37) is not on the site. | `index.html:133-153`, script at the end |
| G4 | Agent grid shows 8 agents and claims "10". `src/agents.ts` has 11: Claude Code, Gemini CLI, OpenCode, Codex CLI, Amp, Cursor Agent, Goose, Grok, Droid, pi, ego. Grok, pi and ego are missing; `website/assets/agents/` still has `aider.svg`, `jules.svg`, `ona.svg`, `warp.svg`. | `index.html:158-195`, `src/agents.ts` |
| G5 | `og:image` and `twitter:image` point at `/og.png`, which does not exist: every shared link has no preview. | `ls website/public` |
| G6 | `sitemap.xml`, `robots.txt` and `og:url` use `sstraus.github.io/tuicommander/`; the installers and the film use `tuicommander.com`. The sitemap lists 2 of 31 posts. | `public/sitemap.xml`, `public/robots.txt` |
| G7 | Two "Docs" links disagree: nav → `/docs/`, footer → GitHub `docs/FEATURES.md`. | `index.html:49`, `:800` |
| G8 | The site breaks two `DESIGN.md` rules: `backdrop-filter: blur(12px)` on the nav (No-Glassmorphism) and a `radial-gradient` glow in the hero (no gradients). | `style.css:104`, `:171` |
| G9 | No download page: system requirements (macOS 12 minimum), beta channel, nightly and the per-OS assets are mixed into the hero. | `index.html:84-131` |
| G10 | Mobile: the nav links collapse with no menu below 768 px (to verify in phase 2). | `style.css:1132` |

## 2. New information architecture

Principle: the home page sells the idea in one scroll and routes to depth. Each product area gets its own page with room for screenshots, a short "how it works", and a link into the matching user guide. The docs stay the reference; the site does not copy them.

### Pages

| URL | Purpose | Main content |
|---|---|---|
| `/` | Pitch + routing | In this order (decided): hero with the film poster and a thin agent logo bar (11 logos, no grid) → before/after strip → five-step loop (plan → spawn → watch → review → ship) → **at most six bands**, one per area page → install → FAQ (6) → CTA |
| `/2.0.html` | The 2.0 release page | Full film with chapters · what changed since 1.7, grouped by area · "Not filmed but shipped" (resume after update, hibernation, tray, native scrollback, Windows CI, secrets) · upgrade notes · link to CHANGELOG |
| `/features/orchestration.html` | Agents and observability (opens with the film's "one window" shots) | `+` agent menu, rich sidebar cards, CLI \| Chat, question and rate-limit detection, Activity Dashboard, agents spawn agents, peer mail, Progress List/Flow, AI Chat (ego over ACP), resume after update |
| `/features/planning.html` | From idea to running work | Ideas, native Plans and Stories with List/Kanban, Workflow designer and runs, Automations (cron, templates, Run now, history) |
| `/features/git-and-review.html` | Ship the agent's work | Worktrees and archive/merge lifecycle, Git panel (diff, stage, smart commit), PR popover with CI, CI auto-heal, PR line review threads, Tasks: GitHub Issues/Projects, Jira and GitLab |
| `/features/workspace.html` | Everything around the terminal | File Browser (glob search, previews), Markdown panel (Edit · Live · Diff), Design Mode, editor, themes (Commander, Paper, Ink), settings search, terminal power features, keyboard-first navigation |
| `/features/anywhere.html` | Remote, mobile, voice | Mobile companion (CLI \| Chat, question cards, push), browser/PWA mode, SSH remote machines with `tuic-remote`, Telegram, push-to-talk and hands-free voice, private secret forms |
| `/features/extend.html` | Open platform | MCP Proxy Hub, plugins in JavaScript (and the plugins repo), HTTP API, `tuic` CLI, TUIC SDK, MDKB integration |
| `/download.html` | Install | Per-OS buttons from the latest release, Homebrew / shell / PowerShell, system requirements, beta and nightly channels, link to release notes |
| `/blog.html` | Unchanged content | Restyled with the shared layout only (no area tagging) |
| `/docs/` | Unchanged | mdBook |

Six area pages instead of one page per feature: about 40 features would give 40 thin pages to keep current. If one area grows too big later (Tasks is the likely one), it splits then.

### Navigation

- Desktop: `Features ▾` (the six area pages) · `2.0` · `Docs` · `Blog` · `GitHub` · **Download** button.
- Mobile (≤ 768 px): a menu button opens the same list in an opaque panel.
- Footer: the same links plus Plugins, Plugin API, Discord, Releases, Changelog, licence.
- Every feature section ends with "Read the guide →" pointing at the matching `docs/user-guide/*.md` page under `/docs/`.

### Shared layout

The nav and footer move into two partial files, injected at build time by a small `transformIndexHtml` step in `vite.config.ts` (the file already does this for `%DISCORD_URL%`). Without it, nine pages repeat the same nav and drift. Area pages are added as Vite entries like the blog posts are. Existing blog posts keep their own markup in this pass; only `style.css` changes under them.

## 2a. Narrative backbone: the 2.0 film (Boss, 2026-10-25)

The site tells the film's story, in the film's order, with its captions as headlines. The outlook is **"TUICommander gives you superpowers"**: one person runs ten projects and dozens of agents and only follows one orchestrator. Each band reuses a film still where it reads better than a new screenshot (stills from the 1080p master at caption-free frames, or stage stills of the same shot).

Decided (coordinator, 2026-10-25): **six area pages**, no `one-window` page. "One window" is the promise of the hero and the before/after strip; its sidebar, `+` menu and CLI \| Chat shots open the orchestration page. Home carries at most six bands; the full sequence of film beats lives only in the `/2.0.html` chapters. Times below are from the 4:43 cut (`script-v3.md` as-built table, 282.7 s); they move again with the coming re-render.

| Site section | Film beat (time, caption) |
|---|---|
| Hero (copy is an open decision, section 9) + film poster + agent logo bar | 1.1–2.1 (0:00–0:16), lockup 12.2 |
| Before/after strip: twelve loose windows vs "All ten. **One window.**" | 1.2, 2.2 (0:04–0:16) |
| Five-step loop: plan → spawn → watch → review → ship ("In **one day**.") | 10.4 (4:25–4:29) |
| Band 1 **Orchestrate** → `/features/orchestration.html`: "The sidebar tells you **everything**." · "Any agent. **One gesture.**" · "Read it like a **chat**." · "Agents spawn **agents**." · "Agents talk to **each other**." · "See **who said what**." · "It knows **which** agent." · AI Chat | 3.1–3.3 (0:16–0:32), 5.1–5.5 (1:29–1:54), 7.1–7.3 (2:36–2:55), 10.1–10.2 (3:55–4:10) |
| Band 2 **Plan** → `/features/planning.html`: "Ideas become **stories**." · "Your pipeline, as a **graph**." · "Agents on a **schedule**." | 4.1, 4.3–4.5 (0:32–0:42, 1:00–1:29) |
| Band 3 **Ship** → `/features/git-and-review.html`: "Your tracker, **next to your agents**." · "CI breaks. **The agent fixes it.**" · "Review the PR **in place**." · "From idea to **PR**." | 8.1–8.5 (2:55–3:43), 10.3 (4:10–4:25) |
| Band 4 **Your tools** → `/features/workspace.html`: Files · Markdown · Design Mode · themes · search | 6.1, 6.3, 6.4 (1:54–2:06, 2:14–2:27), 11 |
| Band 5 **Anywhere** → `/features/anywhere.html`: "Even when you're **out**." · "It pings you only when it **matters**." · "Plan your day **out loud**." | 9.1–9.2 (3:43–3:55), 4.2 (0:42–1:00) |
| Band 6 **Extend** → `/features/extend.html`: "Your own tools, in **JavaScript**." · "**MCP.** **Remote.** **Plugins.**" · memory | 6.2, 6.5 (2:06–2:14, 2:27–2:36), 11 (4:29–4:35) |
| Closing CTA: "Open source · macOS · Linux · Windows" · `brew install sstraus/tap/tuicommander` | 12.2 (4:35–4:43) |

Rules: the highlight word of each headline is bold Observation Blue (the film uses violet; the site keeps the app accent). **Never headline "50 agents. One Mac. No sweat."**: `PRODUCT.md` promises 5+ agents, and the film's number is staged. Features the film does not show (resume after update, hibernation, tray, secrets, Telegram, Windows CI) go under "Shipped, not filmed" on `/2.0.html` and in the matching area page, never in a headline. Keep the caveats of section 4 (Edge voices by default, Jira/GitLab as plugin providers, `tuic-remote` Beta).

## 3. The video

### Placement

- **Home hero:** the film replaces the 1.x YouTube facade. A poster frame and a play button; nothing loads before the click (`preload="none"`). The film has music, so it never autoplays.
- **`/2.0.html`:** the same film with a chapter list. Each chapter seeks the video and links to the area page.
- **Optional, not proposed:** a silent 10–15 s loop for the hero. It adds a second asset to maintain; skip unless Fable or Boss want it.

### Chapters (4:43 cut, from the as-built table in `script-v3.md`)

Provisional: the film gets one more render (unmeasured numbers removed, maybe new music). Re-read the as-built table after it.

| Time | Chapter | Area page |
|---|---|---|
| 0:00 | Ten projects, one window | home |
| 0:16 | Sidebar cards, `+` agent menu, CLI \| Chat | orchestration |
| 0:32 | Ideas | planning |
| 0:42 | Voice: push-to-talk and hands-free | anywhere |
| 1:00 | Stories, Kanban, Workflow designer, Automations | planning |
| 1:29 | Agents spawn agents, mail | orchestration |
| 1:46 | Progress Flow | orchestration |
| 1:54 | Files, plugins, Markdown, Design Mode, MDKB | workspace / extend |
| 2:36 | Questions, rate limits, Activity Dashboard | orchestration |
| 2:55 | Tasks: GitHub, Jira, GitLab | git-and-review |
| 3:22 | PR, CI auto-heal, line review | git-and-review |
| 3:43 | Phone | anywhere |
| 3:55 | AI Chat, ego session settings | orchestration |
| 4:10 | Git: diff, stage, commit | git-and-review |
| 4:29 | MCP, Remote, Prompts, Plugins, Themes, Search | extend |

### Web encode (measured on the 4:43 cut)

Source: 1920×1080, 60 fps, H.264 + AAC, 282.7 s, 129.2 MB. Encodes with `libx264 -preset slow -profile:v high -movflags +faststart`, AAC 96 kb/s, in `~/Gits/.tmp/website-video/` (not in the repo):

| Encode | 3:37 cut | 4:43 cut |
|---|---|---|
| 1280×720, 30 fps, CRF 26 | 19.2 MB | **21.3 MB** |
| 1920×1080, 30 fps, CRF 28 | 25.9 MB | — |
| 1920×1080, 30 fps, CRF 29 | — | 26.3 MB |

Legibility, checked on two frames (3:02 Tasks board, 4:12 Git panel file list): both encodes read cleanly, because the film's camera zooms in on the panels; 720p is slightly soft on 14 px file names, 1080p is crisp.

Decision rule (coordinator): 1080p CRF 29 if under 25 MB and readable, else 720p. 1080p CRF 29 is **26.3 MB, over the limit**, so the rule gives **720p30 CRF 26 (21.3 MB)**. The final encode is made only after the last render; the new length moves the size about linearly (about 75 KB per second at 720p). Poster: a JPEG of the lockup or of the wide "All ten. One window." frame, about 80–150 KB.

### Where the file lives — decision for Boss

| Option | For | Against |
|---|---|---|
| **A. `website/public/video/` in the repo** | Same origin, range requests and caching from Pages, no third party, no tracking, works offline in `vite preview`. One `<video>` tag. | +19 MB in git history, permanent; every re-encode adds again. Below the 50 MB GitHub warning. |
| B. YouTube (Boss uploads) | Adaptive bitrate, zero repo weight, the site already uses a YouTube facade. | Boss must upload; third-party embed and cookies; YouTube UI and recommendations around the film. |
| C. GitHub release asset | Zero repo weight. | The download URL redirects to `objects.githubusercontent.com` with an attachment disposition; seeking and Safari playback are not reliable for `<video>`. Not recommended. |

A and B combine well: self-host for the site, YouTube for sharing.

**Fable's variant (A′):** self-host same-origin but do not commit the mp4: `website.yml` downloads it from a GitHub release asset into `dist/video/` at build time. Same-origin playback, no permanent repo weight, no redirect problem at play time. The host stays an open decision for Boss.

## 4. Feature evidence

Each feature below was checked on this branch (base `670429cd7`, current `main`). Only these go on the site.

| Feature | Verified in |
|---|---|
| Plans and Stories, List/Kanban, Workflow designer, runs | `docs/user-guide/native-stories.md`, `src/components/WorkflowDesigner/` |
| Automations | `docs/user-guide/automations.md` |
| Progress List/Flow | `docs/user-guide/project-progress.md` |
| CLI \| Chat | `docs/user-guide/terminals.md` |
| Tasks with GitHub, Jira, GitLab | `docs/user-guide/github-integration.md`, `plugins.md:44,262` — **Jira and GitLab are plugin providers**: the copy says so |
| PR review threads, CI auto-heal | `docs/user-guide/github-integration.md`, `docs/FEATURES.md` §8.5 |
| Design Mode | `docs/user-guide/design-mode.md` |
| Secrets | `docs/user-guide/secrets.md` |
| tuic-remote, SSH remote machines, mobile | `docs/user-guide/remote-access.md` (tuic-remote is labelled **Beta**: the copy says so) |
| Suspend Tab, hibernation | `docs/user-guide/terminals.md` |
| Voice | `docs/user-guide/dictation.md`: transcription runs locally with Whisper. **Spoken replies default to Microsoft Edge voices (network); Pocket TTS is the local option** with six bundled languages. The film's "on-device" caption must not be repeated as a blanket claim. |
| MDKB | An external tool TUIC integrates with (launch playbook, dashboard plugin). Shown under "extend", not as a built-in. |

Items marked unverified until phase 2: the six theme names in the UI, the settings search box, Telegram reply flow wording.

## 5. Screenshots

Every area page needs 3–6 real 2.0 images (20–40 in total). Fable named this the main risk, so it was tested on 2026-10-25.

### Stage stills: tested, they work

The presentation stage (`tuic-presentation/stage`, dev server on `:5199`) mounts the real 2.0.0-beta.2 components over fictional Halcyon data. Each panel module registers a demo: `?demo=<name>` runs it on the bare seeded world, and **demo mode hides every caption and overlay** (`stage/src/Overlay.tsx:184`, `:330`). Demos: dashboards, flow, github, glance, media, mobile, montage, planning, prdetail, review, tasks, workflow; some take `&step=` (for example `tasks&step=detail`, `montage&step=voice`).

- `render.mjs --stills <ms,...> --dir <abs path>` exports clean 1920×1080 PNGs: no captions, no error banners, no page errors. It pins `deviceScaleFactor: 1` and frames the whole stage, so the app window sits inset on the wallpaper.
- For the site, a small Puppeteer script outside the presentation repo (`~/Gits/.tmp/website-stills-tool/still2x.mjs`, which uses the repo's `puppeteer-core` and Chrome read-only) seeks the same clock at `deviceScaleFactor: 2` and clips to `.stage-window`. Result: **3360×1984 window-only PNGs, crisp at 2×**, 300–900 KB before WebP. `tuic-presentation` is not edited (it belongs to promo-20b).
- Sample, one per area, in `~/Gits/.tmp/website-stills/sample/`:

| Area | Demo, time | Shows |
|---|---|---|
| Orchestration | `flow`, 5000 ms | Progress Flow: columns per agent, delegations, messages, done |
| Planning | `workflow`, 4000 ms | Plans and Stories, Kanban, story detail with revision history |
| Ship | `tasks&step=detail`, 2500 ms | Tasks board with the issue detail and comments |
| Workspace | `review`, 7000 ms | Markdown panel with a plan file, Edit · Live · Diff |
| Anywhere | `mobile`, 7000 ms | Phone session list (needs a full-viewport capture: the phone sits outside the app window, so the window clip cuts its top) |
| Extend | `dashboards`, 10500 ms | Settings → MCP upstream servers with status and call counts |

Other usable frames found: Activity Dashboard (`dashboards` 1400 ms), Settings → Manage plugins (`dashboards` 13500 ms; avoid it, it shows GitHub Work/GitLab/Jira as "Not listed in the plugin registry"), Paper theme (`montage`).

Limits: the terminal is a stage-drawn Claude replica and is empty in demo mode; split panes and mail packets are stage-drawn. Those shots come from film frames (captioned) or `agent-test`. Stage numbers are fictional (call counts, latencies); no headline may quote them.

### Order of sources

1. Stage stills at 2× (above), cropped to the panel that the section talks about.
2. Film frames for terminal-heavy and multi-pane shots (1080p, captions burnt in on most beats).
3. `make agent-test` with throwaway repositories for what neither covers (Design Mode, settings search on the current build).

Never Boss's live instance. Images ship as WebP, lazy-loaded, with width and height set.

## 6. Content to write

| Item | Size |
|---|---|
| Home copy: new hero line, the five-step loop, six highlight bands, "New in 2.0" strip | ~600 words |
| `/2.0.html`: release story, area summaries, "shipped but not filmed", upgrade notes | ~900 words |
| Six area pages, each 4–8 features with a heading, two-sentence "why", a screenshot and a guide link | ~500–800 words each |
| `/download.html` | ~250 words |
| FAQ: drop the stale entries, update the rest (Aider removed, agent count, voice engines, Windows) | 6 on home |
| `og.png` 1200×630 from the lockup frame; meta descriptions per page; sitemap with every page and post; `robots.txt` and `og:url` on `tuicommander.com` | — |

Hero line: **open decision for Boss.** Fable recommends H1 "Run ten projects. Follow one orchestrator." (the lockup) with "10 projects. Dozens of agents. One of you." as the sub-line, because the cold open reads as a complaint without its chaos animation; "The IDE that understands AI agents" then stays as meta description only. Marketing copy goes through `wiz:ls-style-editor`-style review rather than STE, per the repository language rule.

## 7. Visual direction

- Keep the brand: the `DESIGN.md` palette (Cool Stone / Slate neutrals, Observation Blue ≤ 10 % of a screen, semantic colours for status), JetBrains Mono for code and data, system sans for text, 3–4 px radii, 1 px Steel borders, flat surfaces.
- Fix the two rule breaks (G8): an opaque nav and no hero glow.
- The film highlights words in violet. The site keeps Observation Blue as its one accent so the site and the app look the same; violet stays the "merged" semantic colour.
- Marketing pages need a larger type scale than the app (the app's 12–14 px "no display sizes" rule is for the product UI). Proposal: a 16 px body, 40–56 px h1, 28 px h2, one weight step for emphasis, no gradient text.
- Layout: alternating text/screenshot bands on area pages; screenshots framed with a 1 px border and no shadow.
- Status dots and badges from the app appear in screenshots and in small inline "live state" chips, never as decoration.
- Accessibility: WCAG AA contrast, keyboard-reachable menu and video controls, captions are not needed (the film has no voice-over) but every chapter has a text label.

## 8. Phase 2 plan (after the go)

1. Shared layout partials, nav with mobile menu, CSS cleanup (G7, G8, G10).
2. Home and `/2.0.html` with the video (G1–G3).
3. Six area pages and `/download.html` (G9).
4. Agent grid from `src/agents.ts` (G4), `og.png`, sitemap, robots, meta (G5, G6).
5. Screenshots from the stage, then `agent-test` for the remainder, into `website/public/shots/`.
6. Checks: `pnpm build`; `vite preview` driven by agent-browser at 1440 px and 390 px; video plays and seeks; a link check over `dist/`; window screenshots into `website/.review/` (add it to `.gitignore`; it is not ignored today).
7. Local Conventional Commits on `feat/website-2.0`. No push, no deploy.

## 9. Decisions

Decided (coordinator, 2026-10-25, after the Fable review):

- Six area pages; no `one-window` page.
- Home order: hero → before/after strip → five-step loop → at most six bands → install → FAQ (6) → CTA. The agent grid becomes a thin logo bar.
- The film beats appear in full only in the `/2.0.html` chapters.
- Never headline "50 agents".
- No sticky in-page index on area pages; no blog tagging; no silent hero loop.
- Encode: 1080p CRF 29 if under 25 MB and readable, else 720p. Measured: 720p (section 3). The final encode is made after the last render.
- Screenshots: presentation stage stills first, `agent-test` for the rest (section 5).

Open, for Boss:

1. Video host: A (repo `public/`), A′ (release asset fetched into `dist/` by `website.yml`, Fable's recommendation), B (YouTube), or a combination.
2. Hero line (section 6).
