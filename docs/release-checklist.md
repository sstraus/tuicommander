# Release & Tag Checklist

When Boss asks to tag a release:

1. **Update version**: run `make bump V=x.y.z` (updates all manifests, CHANGELOG, SPEC.md, and generates AI release notes with contributor extraction via `scripts/generate-release-notes.sh`)
2. **Review release notes** — the script shows AI-generated notes for approval (Y/edit/regenerate/quit). Ensure `### Community` section in CHANGELOG lists all external contributors with PR links
3. **Commit** with message `chore: bump version to vX.Y.Z`
4. **Prepare the public snapshot** — keep development `main` private. Create a reviewed JSON file with the groups `Breaking changes`, `New features`, and `Fixes` (arrays of public-facing sentences). Run `make github-release RELEASE_NOTES=/absolute/path/to/groups.json`. The target uses the shared orchestrator `tools/release/snapshot.py` tool, removes its configured private paths, refreshes public refs, and pushes only `release/<version>`. It prints the snapshot SHA. The existing generated `release-notes.json` is a different format; do not pass it as grouped notes.
5. **Integrate and tag** — open a PR from `release/<version>` to public `main`, wait for the successful `CI` push run in `sstraus/tuicommander` for the exact candidate SHA and for the required PR checks, then fast-forward public `main` to the reviewed snapshot. A push to `fix/release-*-public` also runs CI, so a replacement candidate can be tested without overwriting an existing release branch. Do not use a private repository for release CI. Run `make github-release RELEASE_COMMIT=<snapshot-sha>`. It refuses tagging until public `main` equals that commit and its release version matches. The shared guard checks the public tag target before the push. The `Release` workflow builds the signed installers, remote daemon, and `latest.json`, then publishes the release. Verify its assets; do not create a second release.
6. **Milestone** — close the matching milestone if one exists, create the next one

## Prereleases (betas)

After publication, `release.yml` calls `notify-tap.yml` directly with the published
tag and prerelease flag. It uses the existing `HOMEBREW_TAP_TOKEN` to dispatch
`release-published` to `sstraus/homebrew-tap` with `repo` and `version`. The notifier
accepts stable `v*` releases only; beta and nightly releases do not update the tap.
It also listens for releases published outside the release workflow.
The direct call is required because [GitHub suppresses workflow runs for release
events created with `GITHUB_TOKEN`](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow).

Use `make bump V=2.0.0-beta.1` and the same snapshot, PR, and post-integration tagging steps for `v2.0.0-beta.1`. `scripts/release-flags.sh` makes `release.yml` publish any version with a `-` suffix as a prerelease that is never "latest", and `notify-tap.yml` skips prereleases. The stable updater endpoint (`releases/latest/download/latest.json`) and the install scripts read "latest", so stable users stay on stable. After every completed versioned beta release, the finalization job copies its signed `latest.json` to the rolling `beta` prerelease (never latest). Installers remain on the immutable versioned release. Settings → General → Expert Mode → Update Channel → Beta checks this feed automatically and also checks stable, so testers receive the next beta and the final release. New beta installations default to Beta; an existing saved channel is preserved, so existing users who install a beta must choose Beta once. Beta updates open the versioned download page; final stable updates install in-app. Nightly behavior is unchanged. An older beta finishing later cannot replace a newer feed. No beta is published if the versioned release or its manifest is incomplete.

The macOS terminal installer preserves an installed Beta or Nightly channel when no version is given. Use `install.sh beta` or `install.sh nightly` to select one explicitly, or pass a version with or without a single `v` prefix to pin it. A default stable install resolves GitHub's latest stable tag. Beta downloads use the immutable tag named by the rolling manifest.

## GitHub Issue Management

- **Labels**: Use `type:`, `P0-P3:`, `area:`, `effort:` prefixes. Apply `needs triage` to new issues.
- **Milestones**: Assign issues to version milestones (v0.4.0, v1.0.0, etc.)
- **Issue templates**: Bug reports and feature requests use `.github/ISSUE_TEMPLATE/*.yml` forms
- **Token for project ops**: Use `GH_TOKEN=$GH_STRAUS gh ...` when commands need the `project` scope (the default `gh auth` token only has `repo` + `workflow`)

## Daily public nightlies

Run `make nightly` only for an approved private `main` commit. It snapshots
`refs/heads/main` even when invoked from another clean branch, creating one snapshot per
local calendar day, parented to public `origin/nightly` (public `origin/main` for
the first nightly), removes private paths through the shared policy, and pushes
only the `nightly` branch and its moving `nightly` tag. `tip` is no longer used.
A repeated or earlier date is refused. `NIGHTLY_DATE=YYYY-MM-DD` selects an
explicit date. The workflow builds the exact public snapshot across desktop,
daemon, and release-note jobs, including manual dispatches.

Both targets require Python 3 and the orchestrator snapshot tool at
`~/Gits/personal/orchestrator/tools/release/snapshot.py`; `SNAPSHOT_TOOL` can select
another installed copy. The tool owns exclusion policy, ancestry validation,
secret scanning, and public push checks. Neither target pushes development main.
Use `scripts/with-test-tmp.sh python3 scripts/test_release_targets.py` to exercise
these targets against disposable local bare remotes with the shared tool installed.


## 2.1.2 Linux ARM64 release gate

Story `3084-a4ba` adds the native Linux ARM64 desktop matrix for **2.1.2**, not a
2.1.1 correction. See [the CI platform contract](guides/ci.md#native-linux-arm64-desktop-packages-212).
Do not advertise verified ARM64 desktop support until these checks complete:

- [ ] Run the final release candidate on `ubuntu-22.04-arm`; preserve the package
  gate logs and `SHA256SUMS-Linux-ARM64.txt` (deb/rpm/AppImage).
- [ ] Confirm `latest.json` contains signed `linux-aarch64` and format-specific
  ARM64 URLs, alongside every default/format-specific x64/macOS/Windows target
  required by the finalization gate. Missing macOS/Windows entries must fail too,
  even when all Linux entries survived parallel uploads. No ARM64 URL may
  reference `amd64` or `x86_64`. Verify the signatures using the configured updater
  public key; a nonempty `.sig` alone does not prove cryptographic validity.
- [ ] On actual ARM64 Linux, record `uname -m`, `/etc/os-release`, artifact SHA256,
  package installation, and a desktop window with a working local terminal.
  Exercise update selection and CPU dictation fallback; record Vulkan device
  availability separately. Requires the produced package and a real desktop
  display. This remains unverified; headless daemon output is not a substitute.
- [ ] Reuse the final candidate's impact-selected checks and required batch gates
  for Linux x64, macOS ARM64 and Windows x64. Do not duplicate their full suites.

### Retained native evidence (2026-11-18)

The preserved **internal, unsigned 2.1.1** packages have native aarch64 runtime
evidence. This is prerequisite evidence, not acceptance of a signed 2.1.2 release:

| Format | Native environment | Verified result |
|---|---|---|
| DEB | Ubuntu 22.04.5 ARM64 | Normal dependency-resolving installation, resolved ELF libraries, mapped desktop and actual WebView DOM readiness |
| RPM | Fedora 44 ARM64 | Normal DNF installation, resolved ELF libraries, mapped desktop and actual WebView DOM readiness |
| AppImage | Ubuntu 22.04.5 ARM64 | Original artifact launched with `--appimage-extract-and-run`, mapped desktop and actual WebView DOM readiness |

Runs used disposable containers on native ARM64 hardware with Xvfb, isolated
user directories and read-only input packages. Artifact hashes stayed unchanged.
No host display, keyring, audio device or GPU was passed through. Missing keyring,
tray and portal services produced retained warnings; their behavior is not accepted
by the initial desktop smoke.

The release checks above remain open. These runs do **not** prove the final
2.1.2 pipeline, updater signatures or update selection, local-terminal interaction,
CPU dictation fallback, GPU acceleration, or default FUSE-mounted AppImage launch.
RPM evidence applies to Fedora 44, not every RPM-based distribution. Do not repeat
the internal-package smoke merely to substitute for final-candidate evidence.

For a task-local Linux ARM64 build through mbx, use the checksum-pinned upstream
1.29.1 tool from `bash scripts/stage-linux-arm64-mbx.sh <task>/tools`. Keep the
existing host tool and retained build artifacts unchanged. Version 1.29.1 fixes
loss of colon-bearing Cargo dependency metadata in shell launchers. Use a short,
whitespace-free managed target path and verify the generated launcher names the
pinned mbx directly, with a shebang no longer than 127 bytes. Longer paths fall
back to a shell and can still lose Tauri core metadata on dash.

A tool upgrade alone does not convert older multi-line shell launchers. The
focused native regression fixture is `python3 scripts/check-linux-build-script-env.py
<old-mbx> <fixed-mbx> <new-short-task-directory>`; run it through the build slot
with the native Rust toolchain. It preserves its old view, then changes only its
own build script to make Cargo regenerate the same launcher. It checks two
colon-bearing Tauri names and ordinary core/plugin controls through that actual
launcher. This is not package, installation, desktop or updater validation.
Regeneration of retained application launchers and the next package build need
separate authorization; do not clear a shared cache to force regeneration.
The fixed tool can discard managed incremental state from older tool versions.
The fixture confines this migration to its own cache and preserves its old view;
review the retained application's migration scope before another package run.

ARM64 asset names are `TUICommander_<version>_arm64.deb`,
`TUICommander-<version>-1.aarch64.rpm` and
`TUICommander_<version>_aarch64.AppImage`. Minimum runtime assumptions and package
limitations are in the CI guide above. The shell installer selects the native
architecture; a missing ARM64 asset is an error, never an x64 fallback.
