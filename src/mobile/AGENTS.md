# Mobile PWA

- The terminal is the focus of the session screen. Preserve its visible area when adding controls. Put search, progress, tasks, logos, attachments, and menus in the existing top bar, an overflow menu, or a transient overlay. If a change costs terminal rows, report the count at a 360×800 viewport and why no alternative works.
- `mobile.html` loads `src/mobile/index.tsx`, which mounts `MobileApp.tsx`. Session list and detail screens live in `screens/`; shared mobile controls live in `components/`.
- Focused mobile tests live in `src/mobile/__tests__/`. Run them from the repository root with `scripts/with-test-tmp.sh pnpm vitest run src/mobile/__tests__/<test>.test.tsx` and a narrow filter.
- Record live phone checks in the `to-test/mobile*.md` (index: repository root `to-test.md`). A desktop browser or component test does not prove the phone layout or touch behavior.
