# Release & Tag Checklist

When Boss asks to tag a release:

1. **Update version**: run `make bump V=x.y.z` (updates `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `package.json`)
2. **Update SPEC.md** header version and date
3. **Update CHANGELOG.md** — move Unreleased items under the new version heading
4. **Commit** with message `chore: bump version to vX.Y.Z`
5. **Tag** with `git tag vX.Y.Z`
6. **GitHub release** — create via `gh release create vX.Y.Z --generate-notes`
7. **Milestone** — close the matching milestone if one exists, create the next one

## GitHub Issue Management

- **Labels**: Use `type:`, `P0-P3:`, `area:`, `effort:` prefixes. Apply `needs triage` to new issues.
- **Milestones**: Assign issues to version milestones (v0.4.0, v1.0.0, etc.)
- **Issue templates**: Bug reports and feature requests use `.github/ISSUE_TEMPLATE/*.yml` forms
- **Token for project ops**: Use `GH_TOKEN=$GH_STRAUS gh ...` when commands need the `project` scope (the default `gh auth` token only has `repo` + `workflow`)
