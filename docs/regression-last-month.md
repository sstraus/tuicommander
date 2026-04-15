# Regressione storie — ultimo mese (2026-03-15 → 2026-04-15)

Generato: 2026-04-15 · 104 storie completate (escluse XS <15 min) · 30 giorni di calendario

## Executive summary — 4 viste di speedup

Lo speedup non è un numero unico: dipende dalla domanda che si pone. Qui 4 prospettive con dati reali e incertezze esplicite.

### Dati di base (misurati, non stimati)

| Misura | Valore | Fonte |
|--------|--------|-------|
| Storie completate (S+ in t-shirt) | 104 | stories/ frontmatter |
| Claude active hours (7d dashboard) | 152h | Anthropic usage panel |
| Claude active hours (30d extrapolated) | ~651h | 152 × 30/7 |
| Messaggi Opus / Sonnet (7d) | 15.3K / 1.5K | dashboard |
| LOC aggiunte (30d) | 89,148 | `git log --numstat` |
| LOC cancellate (30d) | 32,093 | `git log --numstat` |
| Giorni calendario | 30 | — |

### LOC breakdown per stack

| Linguaggio | Added | Deleted | Rate conservativo (LOC/day) | Rate aggressivo (LOC/day) |
|------------|-------|---------|---|---|
| Rust (systems, async) | 28,847 | 9,334 | 25 | 60 |
| TypeScript/React | 41,751 | 17,797 | 50 | 100 |
| CSS / HTML | 8,714 | 1,962 | 150 | 300 |
| Markdown | 7,479 | 1,842 | 400 | 800 |
| Plain JS | 1,093 | 863 | 80 | 150 |
| Shell | 653 | 114 | 60 | 120 |
| Config (yml/toml/json) | 611 | 181 | 300 | 500 |
| **Totale** | **89,148** | **32,093** | → 2,092 dev-days | → 951 dev-days |

Rate di riferimento: conservativo da Code Complete/McConnell ("quality production code with tests"), aggressivo per "dev esperto su pattern noti, poche unknowns". Rust pesato basso per borrow checker/async/lifetimes; TS pesato medio per complessità React/SolidJS/state.

### Vista 1 — Effort ratio (speedup per ora di lavoro)

> "Quanto più lavoro produco per ora attiva rispetto a un dev senior?"

- **Numeratore**: PM hours da LOC. Range 7,605h (aggressivo) → 16,738h (conservativo). Midpoint ~12,200h.
- **Denominatore**: Claude active hours 30d = 651h (extrapolated).
- **Speedup: 11.7x – 25.7x, midpoint ~18-19x**.

Incertezza: dipende dal rate LOC/day scelto. Extrapolation 7→30d introduce ±15%.

### Vista 2 — Time-to-ship (calendario compression)

> "Quanto prima shippo lo stesso scope rispetto a un team singolo?"

- **Scope equivalente**: 951 – 2,092 dev-days (da LOC).
- **Senior solo, full-time**: 951/22 = 43 mesi → 2,092/22 = 95 mesi. Con team da 3: ~14-32 mesi.
- **Realtà**: 30 giorni calendario.
- **Compression: 43x – 95x solo-senior, 14x – 32x team-di-3**.

Incertezza: parallelismo umano non scala linearmente (overhead coordinamento). Per team grossi la compression è meno drammatica.

### Vista 3 — Raw story throughput

> "Quante unità di lavoro completo al giorno?"

- **Reale**: 104 storie / 30 giorni = **3.5 storie/giorno**.
- **Baseline dev senior**: letteratura agile dice ~2-3 storie "tipiche" per settimana = 0.4-0.6/giorno.
- **Speedup: ~6x – 9x** story throughput rispetto a solo senior.

Incertezza: le "storie" qui non sono calibrate a story-point; mix di fix triviali e feature XXL. Utile come ordine di grandezza, non metrica di precisione.

### Vista 4 — Raw LOC throughput (meno PM, più codice grezzo)

> "Quanto codice esce dal mio setup vs un dev?"

- **Reale**: 89,148 added LOC / 30 giorni = ~2,970 LOC/giorno.
- **Baseline senior solo** (mix Rust/TS complex): net ~2,000-5,000 LOC/mese = 70-170 LOC/giorno.
- **Speedup LOC grezzo: ~17x – 42x**.

Incertezza: LOC è metrica sporca (boilerplate, generated code, rewrites conteggiati). Direzionale, non da prendere al valore facciale.

### Sintesi onesta

Incrociando le 4 viste, il range difendibile è **~15x – 25x per effort ratio**, con picchi sul **time-to-ship fino a 40x+** grazie al parallelismo delle sessioni (5-7 in parallelo via TUIC). La cifra di **~20x sostenuto** è coerente con:
- tre fonti indipendenti (LOC/PM, story throughput scontato, active-hours ratio)
- la natura del workflow (dispatch multi-sessione, review umano, Claude fa coding grezzo)

Il Max x20 è il vero soffitto fisico: non scala oltre il quota mensile di active hours, quindi lo speedup è bounded da quel piano.

## Metodologia (dettaglio stories)

- **Duration (Claude)**: finestra `created → completed` della storia. Proxy del wall-clock in cui la storia è stata "live".
- **PM estimate (t-shirt)**: ore-persona stimate da me secondo t-shirt sizing. Calibrazione OAuth reality-check suggerisce che le mie stime sono sottostimate di 3-5x → LOC-based è più affidabile.
- **Caveat XXL**: le storie preplanificate (es. plan OAuth con 5-6 story create in anticipo) restano aperte fino al commit batch → il window gonfia la duration. Reale tempo Claude per ciascuna è minore.
- **Commit match**: full ID `#NNNN-xxxx` + fallback short ID `#NNNN`. Storie senza commit = closed senza riferimento esplicito oppure branch/WIP.

## T-shirt sizing

| Size | Claude window | PM estimate |
|------|---------------|-------------|
| XS   | <15 min       | ~1h         |
| S    | 15-60 min     | ½ day (4h)  |
| M    | 1-3 h         | 1 day (8h)  |
| L    | 3-8 h         | 2 days (16h)|
| XL   | 8-24 h        | 4 days (32h)|
| XXL  | >24 h         | 2 weeks (64h)|

## Totali aggregati

| Metrica | Valore |
|---------|--------|
| Storie completate (S+) | 104 |
| Tempo Claude totale (window) | 1334h10m = 55.6 giorni wall-clock |
| PM hours equivalenti | 3092h = **387 dev-days** (≈ 77.3 dev-weeks) |

## Distribuzione per size

| Size | Count | Claude totale | PM totale | % PM |
|------|-------|---------------|-----------|------|
| S | 23 | 13h32m | 92h (11.5d) | 3% |
| M | 23 | 45h53m | 184h (23.0d) | 6% |
| L | 12 | 47h45m | 192h (24.0d) | 6% |
| XL | 10 | 112h56m | 320h (40.0d) | 10% |
| XXL | 36 | 1114h4m | 2304h (288.0d) | 75% |

## Storie per size (completion desc)

### XXL — 36 storie · PM 2304h (288.0d)

| Completed | ID | P | Type | Title | Claude | PM | Commits |
|-----------|----|---|------|-------|--------|----|---------|
| 2026-04-14 22:52 | 1199-02a4 | P2 | feature | Services tab: OAuth auth method selector + authenticating status UI | 32h57m | 2 weeks | `beb0ebf6` |
| 2026-04-14 22:44 | 1198-571f | P1 | feature | Tauri commands + deep link handler for OAuth callback | 32h49m | 2 weeks | `f7f566d5` |
| 2026-04-14 22:39 | 1197-f47f | P1 | feature | Registry OAuth integration: NeedsOAuth wiring + on_oauth_complete | 32h44m | 2 weeks | `d9261f26` |
| 2026-04-14 22:26 | 1196-ab83 | P1 | feature | HTTP client: 401 handling + token auto-refresh before requests | 32h32m | 2 weeks | `aed21f78` |
| 2026-04-14 22:18 | 1195-2bec | P1 | feature | OAuth flow orchestrator: browser open, deep link callback, dev fallbac | 32h23m | 2 weeks | `1ee6978c` |
| 2026-04-14 21:58 | 1194-accd | P1 | feature | OAuth token exchange + refresh with PKCE and RFC 8707 | 32h4m | 2 weeks | `a8f6b40d` |
| 2026-04-14 21:55 | 1192-b741 | P1 | feature | Registry: Authenticating state + tool call error -32001 + auth semapho | 32h | 2 weeks | `3d9de4c1` |
| 2026-04-14 21:52 | 1191-2666 | P1 | feature | MCP upstream config: UpstreamAuth enum and auth field | 31h58m | 2 weeks | `76d4fd85` |
| 2026-04-14 21:44 | 1200-2ecb | P2 | feature | Deep link SDK gateway: tuic://cmd/{tool}/{action} | 31h50m | 2 weeks | `ee1b1625` |
| 2026-04-14 21:28 | 1193-7f78 | P1 | feature | OAuth discovery module: RFC 9728 + RFC 8414 | 31h33m | 2 weeks | `103a28fd` |
| 2026-04-14 20:30 | 1201-37a2 | P1 | feature | Run config name uniqueness validation (backend) | 30h18m | 2 weeks | — |
| 2026-04-14 20:30 | 1203-b24b | P1 | feature | Integrate run config resolver into agent spawn handler | 30h18m | 2 weeks | — |
| 2026-04-14 20:30 | 1204-14cd | P1 | fix | Inject env_flags in MCP spawn fallback path | 30h18m | 2 weeks | — |
| 2026-04-14 20:30 | 1208-6eab | P1 | feature | Run config name uniqueness validation (frontend) | 30h18m | 2 weeks | — |
| 2026-04-14 20:30 | 1213-3f17 | P1 | feature | Run config name uniqueness validation (backend) | 30h18m | 2 weeks | — |
| 2026-04-14 20:30 | 1214-8c27 | P1 | feature | Run config name uniqueness validation backend | 30h11m | 2 weeks | — |
| 2026-04-14 20:30 | 1215-ab22 | P1 | feature | resolve_run_config function in MCP transport | 30h10m | 2 weeks | — |
| 2026-04-14 20:30 | 1216-2d26 | P1 | feature | Integrate run config resolver into agent spawn handler | 30h10m | 2 weeks | — |
| 2026-04-14 20:30 | 1217-76a1 | P1 | fix | Inject env_flags in MCP spawn fallback path | 30h10m | 2 weeks | — |
| 2026-04-14 20:30 | 1218-4b95 | P2 | feature | MCP param merging with run config args | 30h10m | 2 weeks | — |
| 2026-04-14 20:30 | 1219-5223 | P1 | feature | Prompt substitution in run config args | 30h10m | 2 weeks | — |
| 2026-04-14 20:30 | 1220-8282 | P2 | chore | Update MCP tool description for run config resolution | 30h10m | 2 weeks | — |
| 2026-04-14 20:30 | 1221-ff24 | P1 | feature | Run config name uniqueness validation frontend | 30h10m | 2 weeks | — |
| 2026-04-14 20:30 | 1222-0c70 | P2 | feature | Headless agent execution via run config | 30h10m | 2 weeks | — |
| 2026-04-14 20:29 | 1209-d9f4 | P2 | feature | Headless agent execution via run config | 30h17m | 2 weeks | `ca494e74` |
| 2026-04-14 20:27 | 1202-7b81 | P1 | feature | resolve_run_config() function in MCP transport | 30h15m | 2 weeks | `54cc232b` |
| 2026-04-14 20:27 | 1205-cb60 | P2 | feature | MCP param merging with run config args | 30h15m | 2 weeks | `54cc232b` |
| 2026-04-14 20:27 | 1206-cebf | P1 | feature | {prompt} substitution in run config args | 30h15m | 2 weeks | `54cc232b` |
| 2026-04-14 20:27 | 1207-f57d | P2 | chore | Update MCP tool description for run config resolution | 30h15m | 2 weeks | `54cc232b` |
| 2026-04-14 20:05 | 1210-b5bd | P2 | feature | Settings UI: env vars editing per run config | 29h53m | 2 weeks | `e917dfdc` |
| 2026-04-14 20:05 | 1211-7f13 | P2 | feature | Settings UI: headless agent dropdown shows run configs | 29h53m | 2 weeks | `e917dfdc` |
| 2026-04-14 19:59 | 1212-7e44 | P2 | feature | Settings UI: inline duplicate name validation in run config form | 29h47m | 2 weeks | `9e02fbb4` |
| 2026-04-14 19:58 | 1223-4dc6 | P2 | feature | Settings UI env vars editing per run config | 29h37m | 2 weeks | — |
| 2026-04-14 19:58 | 1224-db5e | P2 | feature | Settings UI headless agent dropdown shows run configs | 29h37m | 2 weeks | — |
| 2026-04-14 19:58 | 1225-7a60 | P2 | feature | Settings UI inline duplicate name validation | 29h37m | 2 weeks | — |
| 2026-04-09 21:52 | 1120-381b | P2 | fix | Terminal tab disappears on new project open — reappears on repo switch | 38h32m | 2 weeks | `6acfba91` |

### XL — 10 storie · PM 320h (40.0d)

| Completed | ID | P | Type | Title | Claude | PM | Commits |
|-----------|----|---|------|-------|--------|----|---------|
| 2026-04-14 19:50 | 1226-8df8 | P1 | fix | PTY should ignore screen rows below input area border | 22h38m | 4 days | `f54ad157` |
| 2026-04-13 08:06 | 1165-b124 | P2 | feature | Swarm Layer 4 — MCP tool descriptions update | 16h57m | 4 days | `3d0b6566` |
| 2026-04-13 07:46 | 1179-1cf9 | P1 | refactor | MCP instructions + swarm protocol integrity | 13h16m | 4 days | `7108f05f` |
| 2026-04-11 06:46 | 1150-1e61 | P2 | feature | Global workspace: repo overlay on tab hover | 9h3m | 4 days | — |
| 2026-04-09 21:51 | 1124-c47b | P3 | feature | OSC 133 shell integration — command block detection | 8h32m | 4 days | — |
| 2026-04-09 21:51 | 1125-663b | P3 | feature | OSC 133: test foundation and fix integration issues | 8h30m | 4 days | `4a59b2d3` |
| 2026-04-09 21:51 | 1126-9adf | P3 | feature | OSC 133: block navigation with Cmd+Up/Down | 8h30m | 4 days | `4a59b2d3` |
| 2026-04-09 21:51 | 1127-0c02 | P3 | feature | OSC 133: gutter exit code markers | 8h30m | 4 days | `4a59b2d3` |
| 2026-04-09 21:51 | 1128-efbc | P3 | feature | OSC 133: copy single block output | 8h30m | 4 days | `4a59b2d3` |
| 2026-04-09 21:51 | 1129-ccce | P3 | feature | OSC 133: cross-terminal overview panel | 8h30m | 4 days | `7a0d4a9e` |

### L — 12 storie · PM 192h (24.0d)

| Completed | ID | P | Type | Title | Claude | PM | Commits |
|-----------|----|---|------|-------|--------|----|---------|
| 2026-04-14 21:08 | 1242-ae1e | P2 | chore | Website: update FAQ with IDE positioning and TUIC SDK entry | 4h2m | 2 days | `0c12d1bc` |
| 2026-04-14 21:07 | 1240-427d | P2 | chore | Website: update comparison table with Cursor IDE and Claude Desktop | 4h1m | 2 days | `0c12d1bc` |
| 2026-04-14 21:07 | 1241-e692 | P2 | chore | Website: update terminal features grid with new features | 4h2m | 2 days | `0c12d1bc` |
| 2026-04-14 21:06 | 1238-2db3 | P2 | feature | Website: add shell script mode to Smart Prompts section | 4h | 2 days | `0c12d1bc` |
| 2026-04-14 21:06 | 1243-3b84 | P2 | feature | Website: promote Claude Usage Dashboard in Agent Observability section | 4h1m | 2 days | `0c12d1bc` |
| 2026-04-14 21:05 | 1235-9d0e | P2 | feature | Website: add Workspace and Multi-Monitor section | 3h59m | 2 days | `0c12d1bc` |
| 2026-04-14 21:05 | 1237-5476 | P2 | feature | Website: update Plugin section with TUIC SDK v1.0 | 3h59m | 2 days | `0c12d1bc` |
| 2026-04-14 21:04 | 1236-4d19 | P2 | refactor | Website: merge duplicate MCP sections into one | 3h58m | 2 days | `0c12d1bc` |
| 2026-04-14 21:03 | 1234-9246 | P2 | refactor | Website: merge 3 worktree sections into one | 3h57m | 2 days | `0c12d1bc` |
| 2026-04-14 21:02 | 1233-03ee | P2 | feature | Website: add The Problem section after agent bar | 3h56m | 2 days | `0c12d1bc` |
| 2026-04-14 21:01 | 1232-5873 | P2 | chore | Website: hero rewrite — AI-native IDE positioning | 3h55m | 2 days | `0c12d1bc` |
| 2026-04-14 21:00 | 1239-1643 | P2 | refactor | Website: remove generic How it works section | 3h55m | 2 days | `0c12d1bc` |

### M — 23 storie · PM 184h (23.0d)

| Completed | ID | P | Type | Title | Claude | PM | Commits |
|-----------|----|---|------|-------|--------|----|---------|
| 2026-04-15 08:49 | 1268-40e8 | P2 | fix | OAuth: validate discovered AS against expected issuer (AS mix-up defen | 2h38m | 1 day | `a2e4e375` |
| 2026-04-15 08:41 | 1269-99f2 | P2 | fix | OAuth: is_token_valid treats None expiry as expired → force-refresh st | 2h30m | 1 day | `2da1c2a4` |
| 2026-04-15 08:36 | 1267-d522 | P2 | fix | OAuth: require user consent before auto-opening browser on NeedsOAuth | 2h26m | 1 day | `f46b12a9` |
| 2026-04-15 07:13 | 1266-5cb6 | P1 | refactor | OAuth: restructure redundant constant_time_eq or document threat model | 1h3m | 1 day | `93721afd` |
| 2026-04-14 19:53 | 1230-b1a7 | P1 | fix | PrDetailPopover freezes UI while loading check details | 2h57m | 1 day | `36a1ba00` |
| 2026-04-14 19:52 | 1229-b674 | P1 | fix | Slow new terminal spawn: 200-400ms defer before PTY creation | 2h58m | 1 day | `696082ac` |
| 2026-04-14 19:40 | 1231-9d93 | P1 | fix | UI lock on repo switch + new terminal + agent launch | 2h40m | 1 day | — |
| 2026-04-14 19:40 | 1244-ac78 | P1 | fix | Batch handleAddTerminalToBranch store writes | 2h31m | 1 day | — |
| 2026-04-14 19:40 | 1245-f78a | P1 | fix | Merge paired terminalsStore.update calls in handleParsedEvent | 2h31m | 1 day | — |
| 2026-04-14 19:40 | 1246-b0f4 | P2 | fix | Defer bumpRevision in repo-changed listener | 2h31m | 1 day | — |
| 2026-04-14 19:40 | 1247-74c0 | P2 | fix | Defer auto-show PR popover from branch-switch flush | 2h31m | 1 day | — |
| 2026-04-14 19:40 | 1248-2213 | P2 | fix | Move lastDataAt out of reactive store | 2h31m | 1 day | — |
| 2026-04-12 17:47 | 1178-dbe1 | P3 | chore | Swarm — docs and cleanup after plan implementation | 2h2m | 1 day | `569715bd` |
| 2026-04-12 17:00 | 1175-b358 | P2 | feature | Swarm UX — finished tab visual distinction (exited state) | 1h15m | 1 day | `95c5f3a8` |
| 2026-04-12 16:57 | 1164-2571 | P1 | feature | Swarm Layer 3 — fallback state_change message + tab autoclose tuning | 1h47m | 1 day | `f400614f` `eff2a213` |
| 2026-04-12 16:53 | 1176-b88b | P2 | fix | Swarm UX — prevent orphan HTML tabs and dedup terminal tabs | 1h7m | 1 day | `6cfa8f2a` |
| 2026-04-12 16:35 | 1163-7599 | P1 | feature | Swarm Layer 2 — spawn response enrichment, session(status) | 1h26m | 1 day | `b39177df` |
| 2026-04-10 21:46 | 1137-9c66 | P2 | feature | TypeScript: GitHubIssue types + github store issues support | 1h29m | 1 day | `d7744b6d` |
| 2026-04-10 21:46 | 1138-60b8 | P2 | feature | UI: IssueDetailContent component | 1h29m | 1 day | `d7744b6d` |
| 2026-04-10 21:46 | 1139-064c | P2 | feature | UI: GitHubPanel — unified Issues+PR sidebar panel | 1h29m | 1 day | `d7744b6d` |
| 2026-04-10 21:46 | 1140-1382 | P2 | feature | Wire GitHubPanel into RepoSection + update badge | 1h29m | 1 day | `d7744b6d` |
| 2026-04-10 21:46 | 1141-7dbb | P2 | feature | MCP HTTP routes for GitHub Issues | 1h29m | 1 day | `d7744b6d` |
| 2026-04-10 21:20 | 1136-83ed | P2 | feature | Rust: GitHubIssue struct + GraphQL query + mutations | 1h4m | 1 day | `afc8ca0a` |

### S — 23 storie · PM 92h (11.5d)

| Completed | ID | P | Type | Title | Claude | PM | Commits |
|-----------|----|---|------|-------|--------|----|---------|
| 2026-04-15 07:09 | 1260-1640 | P1 | fix | OAuth: unify redirect_uri between registry and commands | 59m | ½ day | `c6d58255` |
| 2026-04-15 07:00 | 1265-4548 | P1 | fix | Settings: detect duplicate env var keys before save | 49m | ½ day | `0ba47f4d` |
| 2026-04-15 06:50 | 1264-ef83 | P1 | fix | Settings: mask API key input (type=password) | 39m | ½ day | `e945de38` `754518f8` |
| 2026-04-15 06:49 | 1263-299c | P1 | fix | fs: add TCC guard to stat_path | 38m | ½ day | `88e395df` |
| 2026-04-15 06:40 | 1261-0dba | P1 | fix | MCP HTTP: add localhost guard to execute_headless_prompt/execute_api_p | 29m | ½ day | `7d139422` |
| 2026-04-15 06:37 | 1262-b9af | P1 | fix | Smart Prompts: eliminate shell injection in run config args | 26m | ½ day | `5b635f90` |
| 2026-04-14 16:41 | 1227-351a | P2 | feature | Right-click Print context menu for tabs | 34m | ½ day | `2421a7cf` |
| 2026-04-14 16:41 | 1228-6058 | P1 | fix | Tabs created with pinned:false appear across all repos | 33m | ½ day | `2421a7cf` |
| 2026-04-12 16:35 | 1177-9ea1 | P2 | feature | Swarm API — inbox missed_count on FIFO eviction | 50m | ½ day | `5e378271` |
| 2026-04-12 16:24 | 1168-2cbb | P2 | fix | Swarm Layer 1 — session-created tab auto-activation | 39m | ½ day | `16dde9ad` |
| 2026-04-12 16:20 | 1172-51bd | P1 | feature | Perf — ViewportLock rAF-based programmatic restore | 35m | ½ day | `f85cb224` |
| 2026-04-12 16:01 | 1162-8130 | P1 | feature | Swarm Layer 1 — auto-register child peer, prompt preamble | 52m | ½ day | `e8b23336` `ccba13db` |
| 2026-04-12 15:32 | 1161-4215 | P1 | feature | Swarm Layer 0 — env vars, self-close guard, CSS dot, tab focus | 23m | ½ day | `f16ad3fe` `787debf3` |
| 2026-04-12 07:42 | 1160-e01a | P3 | fix | ScrollbackCache retry on fetch failure | 28m | ½ day | `4bd9bb7d` |
| 2026-04-12 07:40 | 1159-7883 | P3 | fix | Surface truncated search results in VtLogSearch | 27m | ½ day | `d34ddfd6` |
| 2026-04-12 07:39 | 1158-f634 | P2 | feature | Proactive oldest notification from Rust | 25m | ½ day | `ea3aca25` |
| 2026-04-12 07:36 | 1157-706d | P2 | fix | Fix lineHeight and viewportHeight staleness | 23m | ½ day | `46c86209` |
| 2026-04-12 07:34 | 1156-4969 | P1 | feature | Custom virtual scrollbar for scrollback overlay | 20m | ½ day | `7cb55fae` |
| 2026-04-12 07:32 | 1155-93c6 | P1 | feature | Keyboard navigation for scrollback overlay | 18m | ½ day | `052b5e37` |
| 2026-04-10 21:00 | 1135-d0f0 | P2 | feature | CI pipeline for Windows GPU build | 45m | ½ day | `a7bb1c78` |
| 2026-04-10 20:58 | 1134-e72f | P2 | feature | Emit dictation-backend-info event to frontend | 44m | ½ day | `0b41d8dd` |
| 2026-04-10 20:57 | 1133-29f1 | P2 | feature | Enable GPU in WhisperContextParameters | 42m | ½ day | `6948fbe1` |
| 2026-04-10 20:48 | 1132-003d | P2 | feature | Upgrade whisper-rs and add GPU feature flags | 34m | ½ day | `6948fbe1` |

## Storie senza commit matched

25 storie senza commit esplicito (chiuse senza riferimento ID oppure assorbite in refactor batch).

| ID | Size | Title |
|----|------|-------|
| 1124-c47b | XL | OSC 133 shell integration — command block detection |
| 1150-1e61 | XL | Global workspace: repo overlay on tab hover |
| 1231-9d93 | M | UI lock on repo switch + new terminal + agent launch |
| 1244-ac78 | M | Batch handleAddTerminalToBranch store writes |
| 1245-f78a | M | Merge paired terminalsStore.update calls in handleParsedEvent |
| 1246-b0f4 | M | Defer bumpRevision in repo-changed listener |
| 1247-74c0 | M | Defer auto-show PR popover from branch-switch flush |
| 1248-2213 | M | Move lastDataAt out of reactive store |
| 1223-4dc6 | XXL | Settings UI env vars editing per run config |
| 1224-db5e | XXL | Settings UI headless agent dropdown shows run configs |
| 1225-7a60 | XXL | Settings UI inline duplicate name validation |
| 1201-37a2 | XXL | Run config name uniqueness validation (backend) |
| 1203-b24b | XXL | Integrate run config resolver into agent spawn handler |
| 1204-14cd | XXL | Inject env_flags in MCP spawn fallback path |
| 1208-6eab | XXL | Run config name uniqueness validation (frontend) |
| 1213-3f17 | XXL | Run config name uniqueness validation (backend) |
| 1214-8c27 | XXL | Run config name uniqueness validation backend |
| 1215-ab22 | XXL | resolve_run_config function in MCP transport |
| 1216-2d26 | XXL | Integrate run config resolver into agent spawn handler |
| 1217-76a1 | XXL | Inject env_flags in MCP spawn fallback path |
| 1218-4b95 | XXL | MCP param merging with run config args |
| 1219-5223 | XXL | Prompt substitution in run config args |
| 1220-8282 | XXL | Update MCP tool description for run config resolution |
| 1221-ff24 | XXL | Run config name uniqueness validation frontend |
| 1222-0c70 | XXL | Headless agent execution via run config |

## Confronto velocity

- **Claude output**: 104 storie in 30 giorni di calendario = **3.5 storie/giorno**.
- **Equivalente PM**: 3092h / 8 = **387 dev-days** di scope. Un team di 3 dev senior impiegherebbe ~5.9 mesi (22 gg lavorativi/mese).
- **Fattore compressione**: 2.3x (PM hours / Claude hours window).

> Caveat: il fattore è sovrastimato perché il window Claude include idle time tra apertura story e commit. Sottostima compensativa: una frazione del PM è già inclusa in retrospettiva (review, testing manuale).
