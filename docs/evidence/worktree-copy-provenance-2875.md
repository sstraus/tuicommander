# Legacy copy inventory — 2875-7ecc

Read-only inventory, 2026-11-06. No historical copy receipt exists for these files. Current byte equality is not proof of origin. No retained checkout was changed.

The source ignores `docs/internals/` at `.gitignore:62`. The current warmer selects ignored directories with `git ls-files --others --ignored --exclude-standard --directory` in `cow.rs::ignored_directories`, then copies each absent destination in `warm_candidates`. The source ignore rule does not have to exist on the destination branch. This proves a viable copy path, not which historical process wrote these files.

| Checkout | Path | Current SHA-256 | Source comparison |
|---|---|---|---|
| feat-790-instruction-symlinks | `docs/internals/harness-watch/2026-11-02-claude-code-gap.md` | `a8e9235261f6814e73db3a09dafe0f6d23d41859dc122ea2ba0c71f460feb029` | equal, origin uncertain |
| feat-790-instruction-symlinks | `docs/internals/harness-watch/2026-11-02-codex-gap.md` | `4438f9f9264f68c1c3e41a103e4a8396497ac4013f167dda0f4bf5b7acf5eace` | equal, origin uncertain |
| feat-790-instruction-symlinks | `docs/internals/harness-watch/2026-11-02-gemini-grok-gap.md` | `774346fdbbd11c133e0a307aa17c0dc313b0922cd26aaa4bf1d8e49d3eaf21ce` | equal, origin uncertain |
| feat-790-instruction-symlinks | `docs/internals/harness-watch/2026-11-02-proposal.md` | `e7dff9078863031b7dd2dafff07432f9c58a849e7378306b18582479dfe74d0b` | different, preserve |
| feat-790-instruction-symlinks | `docs/internals/harness-watch/2026-11-02.md` | `dfdafa2efe048b5a1157b53512a2734cc24d58e9b2e612a9d4368debf42611c0` | equal, origin uncertain |
| feat-790-instruction-symlinks | `docs/internals/harness-watch/history/claude-code-0.2.21-2.1.107.md` | `47dcbce4c2d8fb67f86ca71f9bd4623974ad8f86db1b60b59ac19b05828f6d0a` | equal, origin uncertain |
| feat-790-instruction-symlinks | `docs/internals/harness-watch/history/claude-code-2.1.108-2.1.221.md` | `6032c7758473fe2bce981fef46417358def595a386fa363dfd08b3258aa2841e` | equal, origin uncertain |
| feat-790-instruction-symlinks | `docs/internals/harness-watch/history/claude-code-2.1.222-2.1.316.md` | `41bf532ceb290e8b398566efbbe9f3d24cf7a382b9bcdf2a123f3a85e0a4a02f` | equal, origin uncertain |
| feat-790-instruction-symlinks | `docs/internals/harness-watch/history/codex-full.md` | `d563ad5ca324383961e3fe24fe501fe3b0e918be279b82282d0d662c6556f7de` | equal, origin uncertain |
| feat-790-instruction-symlinks | `docs/internals/harness-watch/history/gemini-grok-full.md` | `5f07b117b3498dc77618eb8700ed773c6df2beb8a60e5f8fc7e9099829d10702` | equal, origin uncertain |

`feat-790-instruction-symlinks`: 10 non-ignored untracked files at observation time.

| Checkout | Path | Current SHA-256 | Source comparison |
|---|---|---|---|
| feat-792-credential-helper-failures | `docs/internals/harness-watch/2026-11-02-claude-code-gap.md` | `a8e9235261f6814e73db3a09dafe0f6d23d41859dc122ea2ba0c71f460feb029` | equal, origin uncertain |
| feat-792-credential-helper-failures | `docs/internals/harness-watch/2026-11-02-codex-gap.md` | `4438f9f9264f68c1c3e41a103e4a8396497ac4013f167dda0f4bf5b7acf5eace` | equal, origin uncertain |
| feat-792-credential-helper-failures | `docs/internals/harness-watch/2026-11-02-gemini-grok-gap.md` | `774346fdbbd11c133e0a307aa17c0dc313b0922cd26aaa4bf1d8e49d3eaf21ce` | equal, origin uncertain |
| feat-792-credential-helper-failures | `docs/internals/harness-watch/2026-11-02-proposal.md` | `e7dff9078863031b7dd2dafff07432f9c58a849e7378306b18582479dfe74d0b` | different, preserve |
| feat-792-credential-helper-failures | `docs/internals/harness-watch/2026-11-02.md` | `dfdafa2efe048b5a1157b53512a2734cc24d58e9b2e612a9d4368debf42611c0` | equal, origin uncertain |
| feat-792-credential-helper-failures | `docs/internals/harness-watch/history/claude-code-0.2.21-2.1.107.md` | `47dcbce4c2d8fb67f86ca71f9bd4623974ad8f86db1b60b59ac19b05828f6d0a` | equal, origin uncertain |
| feat-792-credential-helper-failures | `docs/internals/harness-watch/history/claude-code-2.1.108-2.1.221.md` | `6032c7758473fe2bce981fef46417358def595a386fa363dfd08b3258aa2841e` | equal, origin uncertain |
| feat-792-credential-helper-failures | `docs/internals/harness-watch/history/claude-code-2.1.222-2.1.316.md` | `41bf532ceb290e8b398566efbbe9f3d24cf7a382b9bcdf2a123f3a85e0a4a02f` | equal, origin uncertain |
| feat-792-credential-helper-failures | `docs/internals/harness-watch/history/codex-full.md` | `d563ad5ca324383961e3fe24fe501fe3b0e918be279b82282d0d662c6556f7de` | equal, origin uncertain |
| feat-792-credential-helper-failures | `docs/internals/harness-watch/history/gemini-grok-full.md` | `5f07b117b3498dc77618eb8700ed773c6df2beb8a60e5f8fc7e9099829d10702` | equal, origin uncertain |

`feat-792-credential-helper-failures`: 10 non-ignored untracked files at observation time.

These checkouts are read-only evidence, not cleanup targets. The current Rust copy settings (`copy_untracked_files`, `copy_ignored_files`) are persisted settings but are not consumed by the linked-worktree warmer. This task does not add a new copy mechanism or change the settings contract.

## Lifecycle evidence

- `src-tauri/src/mcp_http/worktree_routes.rs:177,276`: MCP/HTTP creation calls the existing allocator, then runs `cow::warm_worktree` through setup-then-warm.
- `src-tauri/src/worktree.rs:845`: desktop creation schedules the same warmer.
- `src-tauri/crates/tuic-git/src/cow.rs::ignored_directories`: Git selects ignored **source** directories, at any depth. `warm_candidates` skips an existing destination, snapshots non-ignored destination payloads before copying, then verifies and publishes copy proof. No current untracked-file setting is read here.
- `src-tauri/crates/tuic-git/src/worktree.rs::workspace_safety_at` and `dirty_files_at`: fresh lifecycle inspection and both final no-force removal checks share the same copy classification. The existing tip, submodule, lock, session and warm-token paths remain in use. Story 2864-74ab's fresh lifecycle call remains the automatic-archive entry point; no parallel candidate proof was added.

The historical writer of the twenty retained files cannot be proved from current bytes. The source ignore rule and destination inventory explain how the existing warmer can create this state. Acceptance must not convert this explanation into a claimed historical receipt.

## Receipt scope

Receipts cover payload files that are untracked and non-ignored in the destination after the copy. Ignored build caches retain normal Git behavior, are not added to the receipt and do not cause a full cache hash on every lifecycle request. A copied `.gitignore` is re-evaluated before proof is published. The original source and checkout paths, stable Git administrative identity, and regular-file SHA-256 values are stored outside the working tree. Administrative identity survives a registered archive move; pathname reuse does not transfer proof.

## Validation limits

The author used targeted rb tests, not a desktop instance. Native macOS validation was attempted after rb exit 78, but the installed `mbx` process was killed with signal 9 before Cargo started; `mbx --version` and `mbx doctor` were also killed. No tool installation or signing change was attempted. Native clonefile evidence remains unavailable. The coordinator owns the separate high-risk critic and pre-fix RED replay.
