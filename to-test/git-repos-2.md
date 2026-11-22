# Git and repositories (2)

## Delete a squash-merged worktree — Rust restart required

- [ ] After the backend restart, squash-merge a worktree branch into main (GitHub squash or `git merge --squash`), fetch, then open Delete on the worktree. The commit check must read "All changes are in … (squash merge)", Delete must be enabled and remove folder and branch. A merge-commit/fast-forward branch must still read "All commits are in …"; a branch with an unmerged commit must stay blocked. Unit coverage: `archive_checklist_allows_delete_after_squash_merge`.
