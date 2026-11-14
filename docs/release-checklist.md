# Release & Tag Checklist

When Boss asks to tag a release:

1. **Update version**: run `make bump V=x.y.z` (updates all manifests, CHANGELOG, SPEC.md, and generates AI release notes with contributor extraction via `scripts/generate-release-notes.sh`)
2. **Review release notes** — the script shows AI-generated notes for approval (Y/edit/regenerate/quit). Ensure `### Community` section in CHANGELOG lists all external contributors with PR links
3. **Commit** with message `chore: bump version to vX.Y.Z`
4. **Prepare the public snapshot** — keep development `main` private. Create a reviewed JSON file with the groups `Breaking changes`, `New features`, and `Fixes` (arrays of public-facing sentences). Run `make github-release RELEASE_NOTES=/absolute/path/to/groups.json`. The target uses the shared orchestrator `tools/release/snapshot.py` tool, removes its configured private paths, refreshes public refs, and pushes only `release/<version>`. It prints the snapshot SHA. The existing generated `release-notes.json` is a different format; do not pass it as grouped notes.
5. **Integrate and tag** — open a PR from `release/<version>` to public `main`, wait for CI, then fast-forward public `main` to the reviewed snapshot. Run `make github-release RELEASE_COMMIT=<snapshot-sha>`. It refuses tagging until public `main` equals that commit and its release version matches. The shared guard checks the public tag target before the push. The `Release` workflow builds the signed installers, remote daemon, and `latest.json`, then publishes the release. Verify its assets; do not create a second release.
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
