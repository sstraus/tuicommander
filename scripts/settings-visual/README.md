# Isolated Settings visual regressions

Run from a worktree with its own installed dependencies. The fixture mounts the real SettingsPanel, page components, theme JSON and controls. Vite aliases intercept RPC, events and logging before application imports. No native backend, user config or external service is used. Chromium blocks non-local requests.

Requirements: the bundled Chrome for Testing path in `capture.mjs`, Node with WebSocket support, free localhost port 15391, and an unlocked macOS display. A locked session pauses Chrome rendering, including headless screenshots and animation-frame checks; DOM access alone is not visual proof. The driver reads explicit `IOConsoleLocked` or `CGSSessionScreenIsLocked` booleans from `ioreg`; any locked marker, failed probe or missing state refuses capture. An unlocked console can omit the session-specific marker. The driver records `prerequisite-failure.json` and refuses capture when the system reports a locked session. Unlock the display before rerunning; never mark that refusal as a visual pass. The server uses `--strictPort`; the runner stops its own Vite and Chrome processes. Chrome diagnostics are retained, and a stalled CDP response has a separate 60-second harness bound.

Run a single regression with the normal build admission wrapper:

```sh
VISUAL_TEST=settings_text_contrast_on_themed_surfaces VISUAL_RUN=text \
  ~/Gits/personal/orchestrator/tools/build/build-slot.sh bash scripts/settings-visual/run.sh
```

Other named regressions:

- `settings_agent_initials_contrast`: catches white identity letters on bright agent fills.
- `settings_appearance_legend_light_text_contrast`: catches unreadable semantic marker text in the real legend/sidebar components.
- `settings_help_badges_share_label_line`: catches sibling help placed beneath its label.

Each checks Commander and Paper at 1400x900 and 390x844. Assertions and screenshots go under `.tmp/settings-visual/$VISUAL_RUN/`. Every run asserts shared row/help geometry, compact page selection, repository label order, shortcut spacing, and equal page-header widths. Results include the measured bounding boxes in `geometry-assertions.json` and the per-theme JSON reports. `VISUAL_TEST` adds the named contrast or help regression. The fixture also captures a dynamic plugin page in ready/loading/error/stale-revision states, the AI Chat folder editor, and a custom launcher editor. These are synthetic RPC fixtures, not live service or persistence evidence. `VISUAL_PAGES` restricts a capture to comma-separated tab IDs; `VISUAL_ELEMENTS=1` also crops label/help pairs, agent icons and PR markers.

Before capture, the driver waits for the requested document URL, mounted header, theme identity, body foreground, loaded fonts and painted frames. It also waits for the actual page heading and status badges to acquire their themed foregrounds: root tokens alone can be ready while initial inherited text is still black. Each result records the foregrounds measured before screenshots. Geometry checks cover the shared Sidebar Color row, breadcrumb-to-header spacing, nested MCP card spacing and heading size, and wrapped help adjacency. The color-target geometry check rejects stretched or shrunken swatches, and the upstream add-action check requires an 8px gap from its description. Shortcut key/action boxes are retained for review. The help-gap maximum is 12 CSS px plus 1/64 px (0.015625 px) to absorb the recorded WebKit Range/element-edge mismatch of 0.0029296875 px; a full 1 px excess still fails. Horizontal overflow fails only when `scrollWidth > clientWidth`, not when WebKit reports a smaller scroll width.

Run the geometry oracle regressions without a browser:

```sh
~/Gits/personal/orchestrator/tools/build/build-slot.sh scripts/with-test-tmp.sh node --test scripts/settings-visual/geometry.test.mjs
```

These tests execute the shipped capture expression with recorded native measurements and boundary inputs. They validate the oracle, not browser rendering or subjective visual approval.

For Settings catalogue parity, use `VISUAL_PAGES=general,all-settings VISUAL_BROWSER=1`. The inspector capture selects `app.shell` and waits for its descriptor-bound input and explicit synthetic owner-read status. Its read-only RPC fixture uses shipped descriptor defaults; mutation is unavailable. `VISUAL_BROWSER=1` keeps `isTauri=false` and does not simulate a native desktop runtime.

The fixture loads Commander and Paper from the shipped palette JSON through the same `loadThemes()` startup boundary as the app. Browser-mode captures must retain `isTauri=false`; Local tools is desktop-only and does not render an empty card. Theme options are synthetic catalogue responses, not live backend evidence.

The contrast oracle rasterizes CSS colors to resolve `color(srgb ...)` and composites translucent ancestor backgrounds. It uses raw ratios for the threshold, not the rounded report value. Help question buttons have a 3:1 non-text threshold; semantic question text has a 4.5:1 threshold. Reduced motion makes marker captures deterministic. It does not claim native WebKit rendering or subjective design approval. The Actions table's screen-reader-only heading is intentionally clipped.

Voice consistency evidence uses the opt-in `voice-audit=1` query parameter on the
real Dictation page. Its synthetic catalogue includes installed and missing
Whisper models plus a Pocket runtime, language and voice. The optional `engine`
query selects Pocket, Edge or an external command. This fixture does not contact
speech services, use the microphone or write user configuration. Without the
query parameter, the existing empty voice catalogue is unchanged.
