//! Per-agent capabilities, fixed when TUIC spawns a managed agent (story 2713).
//!
//! The set is written once, by the spawn that creates the session, and removed
//! with its PTY. Nothing that carries agent text — mail, prompts, later tool
//! arguments — writes it, so text sent to an agent cannot widen what it may do.
//!
//! `children` and `progress` are MCP calls and TUIC refuses them itself.
//! `land`, `push` and `tag` have no MCP surface: TUIC exports the set as
//! [`ENV_VAR`] and the repository's pre-push hook refuses pushes and tag
//! pushes outside it. In a repository without that hook, a `git` shim on the
//! agent's `PATH` ([`apply_git_shim`]) refuses them. Both are advisory
//! (`git push --no-verify` or an absolute path to git skips them).

use std::collections::BTreeSet;

/// Environment variable that carries a capped agent's set, comma-separated.
/// Absent for an uncapped session; empty for a child with no capabilities.
pub(crate) const ENV_VAR: &str = "TUIC_CAPABILITIES";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Capability {
    Land,
    Push,
    Tag,
    Children,
    Progress,
    Operator,
}

impl Capability {
    pub(crate) const NAMES: [&'static str; 6] =
        ["land", "push", "tag", "children", "progress", "operator"];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Land => "land",
            Self::Push => "push",
            Self::Tag => "tag",
            Self::Children => "children",
            Self::Progress => "progress",
            Self::Operator => "operator",
        }
    }

    const ALL: [Self; 6] = [
        Self::Land,
        Self::Push,
        Self::Tag,
        Self::Children,
        Self::Progress,
        Self::Operator,
    ];

    fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|capability| capability.as_str() == name)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CapabilitySet(BTreeSet<Capability>);

impl CapabilitySet {
    pub(crate) fn automation() -> Self {
        Self(
            Capability::ALL
                .into_iter()
                .filter(|cap| *cap != Capability::Operator)
                .collect(),
        )
    }

    /// Parse the `capabilities` spawn argument: an array of capability names.
    pub(crate) fn from_spawn_arg(value: &serde_json::Value) -> Result<Self, String> {
        let names = value
            .as_array()
            .ok_or("capabilities must be an array of capability names")?;
        names
            .iter()
            .map(|name| {
                name.as_str().and_then(Capability::parse).ok_or_else(|| {
                    format!(
                        "Unknown capability {name}; expected one of: {}",
                        Capability::NAMES.join(", ")
                    )
                })
            })
            .collect::<Result<BTreeSet<_>, _>>()
            .map(Self)
    }

    pub(crate) fn contains(&self, capability: Capability) -> bool {
        self.0.contains(&capability)
    }

    /// The first capability of `self` that `holder` lacks, if any.
    pub(crate) fn first_outside(&self, holder: &Self) -> Option<Capability> {
        self.0.difference(&holder.0).next().copied()
    }

    /// The [`ENV_VAR`] value: names joined by commas, empty for none.
    pub(crate) fn env_value(&self) -> String {
        self.0
            .iter()
            .map(|capability| capability.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }
}

fn capability_path(data_dir: &std::path::Path, identity: &str) -> std::path::PathBuf {
    use sha2::{Digest, Sha256};
    let filename = if uuid::Uuid::parse_str(identity).is_ok() {
        identity.to_owned()
    } else {
        format!(
            "nonuuid-{}",
            hex::encode(Sha256::digest(identity.as_bytes()))
        )
    };
    data_dir
        .join("agent-capabilities")
        .join(format!("{filename}.json"))
}

/// Persist before launch so a restored identity cannot regain capabilities.
pub(crate) fn save(
    data_dir: &std::path::Path,
    identity: &str,
    set: &CapabilitySet,
) -> Result<(), String> {
    let dir = data_dir.join("agent-capabilities");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = capability_path(data_dir, identity);
    let names: Vec<_> = set.0.iter().map(|cap| cap.as_str()).collect();
    let bytes = serde_json::to_vec(&names).map_err(|e| e.to_string())?;
    save_bytes(data_dir, identity, &path, &bytes)
}

pub(crate) fn save_root(data_dir: &std::path::Path, identity: &str) -> Result<(), String> {
    let dir = data_dir.join("agent-capabilities");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = capability_path(data_dir, identity);
    save_bytes(data_dir, identity, &path, b"null")
}

fn save_bytes(
    _data_dir: &std::path::Path,
    _identity: &str,
    path: &std::path::Path,
    bytes: &[u8],
) -> Result<(), String> {
    use std::io::Write;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            file.write_all(bytes)
                .and_then(|_| file.sync_all())
                .map_err(|e| e.to_string())?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if std::fs::read(path).map_err(|e| e.to_string())? != bytes {
                return Err("capability identity already has a different set".into());
            }
        }
        Err(error) => return Err(error.to_string()),
    }
    Ok(())
}

/// A missing or damaged legacy record never grants the new operator capability.
pub(crate) fn load(data_dir: &std::path::Path, identity: &str) -> Option<CapabilitySet> {
    let path = capability_path(data_dir, identity);
    match std::fs::read(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Some(CapabilitySet::automation())
        }
        Err(_) => Some(CapabilitySet::default()),
        Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(serde_json::Value::Null) => None,
            Ok(value) => Some(CapabilitySet::from_spawn_arg(&value).unwrap_or_default()),
            Err(_) => Some(CapabilitySet::default()),
        },
    }
}

/// The capability an MCP call needs, if any. `children` covers every call
/// that starts a managed agent; plain terminal sessions are not children.
pub(crate) fn required_for(tool: &str, action: Option<&str>) -> Option<Capability> {
    match (tool, action) {
        ("agent", Some("spawn" | "launch")) => Some(Capability::Children),
        ("progress", _) => Some(Capability::Progress),
        _ => None,
    }
}

/// `git` shim for a capped agent. The pre-push hook exists only in repositories
/// that install it, so a push elsewhere needs this: TUIC puts this directory
/// first on the capped agent's `PATH` and the shim refuses a push (or tag push)
/// the agent's set does not allow, then runs the real git. Like the hook it is
/// advisory: an absolute path to git skips it.
#[cfg(unix)]
const GIT_SHIM: &str = r#"#!/bin/sh
# TUIC capability shim: refuse a push outside TUIC_CAPABILITIES, else run git.
self=${0%/*}
real=
set -f
old_ifs=$IFS
IFS=:
for dir in $PATH; do
  [ -n "$dir" ] || dir=.
  [ "$dir" = "$self" ] && continue
  if [ -f "$dir/git" ] && [ -x "$dir/git" ]; then real=$dir/git; break; fi
done
IFS=$old_ifs
set +f
if [ -z "$real" ]; then
  printf 'tuic: git: command not found\n' >&2
  exit 127
fi
[ -n "${TUIC_CAPABILITIES+set}" ] || exec "$real" "$@"
sub=
skip=0
for arg in "$@"; do
  if [ "$skip" = 1 ]; then skip=0; continue; fi
  case $arg in
    -C|-c|--git-dir|--work-tree|--namespace|--exec-path|--super-prefix|--config-env) skip=1;;
    -*) ;;
    *) sub=$arg; break;;
  esac
done
[ "$sub" = push ] || exec "$real" "$@"
needed=push
for arg in "$@"; do
  case $arg in
    --tags|--follow-tags|--mirror|tag|*refs/tags/*) needed=tag;;
  esac
done
for cap in $needed; do
  case ",$TUIC_CAPABILITIES," in
    *",$cap,"*) ;;
    *)
      printf 'tuic: git push refused: this agent lacks the %s capability.\n' "$cap" >&2
      printf 'TUIC fixed its capabilities at spawn: [%s]. Ask the agent that spawned you.\n' "$TUIC_CAPABILITIES" >&2
      exit 1;;
  esac
done
exec "$real" "$@"
"#;

/// Directory under the data dir that holds the shim.
#[cfg(unix)]
const SHIM_DIR: &str = "capability-shim";

/// Directory under the data dir that holds the hooks, one file per hook name.
#[cfg(unix)]
const HOOKS_DIR: &str = "capability-hooks";

/// One script installed under every client-side hook name. A login shell
/// (macOS `path_helper`, Claude Code's shell snapshot) moves the shim behind
/// `/usr/bin`, but it keeps the environment, so `core.hooksPath` set through
/// `GIT_CONFIG_*` still reaches git. Each hook runs the repository's own hook of
/// that name (found without our override), so pointing git here costs a
/// repository none of its hooks. `pre-push` first refuses a push or tag push
/// outside `TUIC_CAPABILITIES`.
#[cfg(unix)]
const HOOK_SCRIPT: &str = r#"#!/bin/sh
# TUIC capability hook: refuse a push outside TUIC_CAPABILITIES, then run the
# repository's own hook of the same name.
name=${0##*/}
self=${0%/*}
if [ "$name" = pre-push ]; then
  refs=$(cat)
  if [ -n "${TUIC_CAPABILITIES+set}" ]; then
    printf '%s\n' "$refs" | while read -r _ _ remote_ref _; do
      [ -n "$remote_ref" ] || continue
      case $remote_ref in refs/tags/*) needed=tag;; *) needed=push;; esac
      case ",$TUIC_CAPABILITIES," in
        *",$needed,"*) ;;
        *)
          printf 'tuic: git push refused: this agent lacks the %s capability (%s).\n' "$needed" "$remote_ref" >&2
          printf 'TUIC fixed its capabilities at spawn: [%s]. Ask the agent that spawned you.\n' "$TUIC_CAPABILITIES" >&2
          exit 1;;
      esac
    done || exit 1
  fi
fi
orig=$(env -u GIT_CONFIG_COUNT git rev-parse --git-path hooks 2>/dev/null) || exit 0
case $orig in /*) ;; *) orig=$PWD/$orig;; esac
[ "$orig" = "$self" ] && exit 0
[ -f "$orig/$name" ] && [ -x "$orig/$name" ] || exit 0
if [ "$name" = pre-push ]; then
  printf '%s\n' "$refs" | "$orig/$name" "$@"
else
  exec "$orig/$name" "$@"
fi
"#;

/// Hooks git runs on the client side; each gets a copy of [`HOOK_SCRIPT`].
#[cfg(unix)]
const HOOK_NAMES: [&str; 17] = [
    "applypatch-msg",
    "pre-applypatch",
    "post-applypatch",
    "pre-commit",
    "pre-merge-commit",
    "prepare-commit-msg",
    "commit-msg",
    "post-commit",
    "pre-rebase",
    "post-checkout",
    "post-merge",
    "pre-push",
    "post-rewrite",
    "reference-transaction",
    "push-to-checkout",
    "pre-auto-gc",
    "post-index-change",
];

/// Write an executable `content` at `path` unless it is already there. The
/// write is atomic (rename), so concurrent spawns are safe.
#[cfg(unix)]
fn install_script(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if std::fs::read_to_string(path).ok().as_deref() != Some(content) {
        let staging = path.with_extension(uuid::Uuid::new_v4().to_string());
        std::fs::write(&staging, content)?;
        std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o755))?;
        std::fs::rename(&staging, path)?;
    }
    Ok(())
}

/// Write the shim under `data_dir` and return its directory.
#[cfg(unix)]
pub(crate) fn install_git_shim(data_dir: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    let dir = data_dir.join(SHIM_DIR);
    std::fs::create_dir_all(&dir)?;
    install_script(&dir.join("git"), GIT_SHIM)?;
    Ok(dir)
}

/// Write the hooks under `data_dir` and return their directory.
#[cfg(unix)]
fn install_git_hooks(data_dir: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    let dir = data_dir.join(HOOKS_DIR);
    std::fs::create_dir_all(&dir)?;
    for name in HOOK_NAMES {
        install_script(&dir.join(name), HOOK_SCRIPT)?;
    }
    Ok(dir)
}

/// Enforce a capped agent's push capabilities in any repository: the shim first
/// on `PATH` (catches `--no-verify`), and `core.hooksPath` through the
/// environment (survives a login shell that reorders `PATH`). Windows has none
/// of this: there the repository hook alone applies.
pub(crate) fn apply_git_shim(
    cmd: &mut portable_pty::CommandBuilder,
    data_dir: &std::path::Path,
) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let dir = install_git_shim(data_dir)?;
        let path = cmd
            .get_env("PATH")
            .map(|path| path.to_os_string())
            .or_else(|| std::env::var_os("PATH"))
            .unwrap_or_default();
        let mut entries = vec![dir.clone()];
        entries.extend(std::env::split_paths(&path).filter(|entry| entry != &dir));
        let joined = std::env::join_paths(entries)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;
        cmd.env("PATH", joined);

        let hooks = install_git_hooks(data_dir)?;
        let count_var = "GIT_CONFIG_COUNT";
        let count = match cmd
            .get_env(count_var)
            .map(|value| value.to_os_string())
            .or_else(|| std::env::var_os(count_var))
        {
            None => 0,
            Some(value) => value
                .to_str()
                .and_then(|value| value.parse::<usize>().ok())
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("{count_var} is not a number"),
                    )
                })?,
        };
        cmd.env(format!("GIT_CONFIG_KEY_{count}"), "core.hooksPath");
        cmd.env(format!("GIT_CONFIG_VALUE_{count}"), hooks);
        cmd.env(count_var, (count + 1).to_string());
    }
    #[cfg(not(unix))]
    let _ = (cmd, data_dir);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::process::Command;

    // Catches: a client-provided non-UUID session cannot be restored as an uncapped root.
    #[test]
    fn non_uuid_root_identity_survives_restore() {
        let root = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        save_root(root.path(), "client/id").unwrap();
        assert!(load(root.path(), "client/id").is_none());
        assert!(
            !root
                .path()
                .join("agent-capabilities/client/id.json")
                .exists()
        );
    }

    // Catches: the schema enum (NAMES) drifts from the names spawn accepts.
    #[test]
    fn schema_names_are_exactly_the_parsed_capabilities() {
        let parsed: Vec<&str> = Capability::NAMES
            .iter()
            .map(|name| Capability::parse(name).map_or("<unparsed>", Capability::as_str))
            .collect();
        assert_eq!(parsed, Capability::NAMES);
        assert_eq!(Capability::ALL.len(), Capability::NAMES.len());
    }

    /// Real git in a repo with no hooks, a bare remote, and the shim first on PATH.
    #[cfg(unix)]
    fn push_through_shim(capabilities: Option<&str>, push_args: &[&str]) -> (bool, String, bool) {
        let root = tempfile::TempDir::new_in(tuic_test_support::test_temp_root()).unwrap();
        let git = |dir: &std::path::Path, args: &[&str]| {
            let out = Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
                .args(args)
                .current_dir(dir)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}: {out:?}");
        };
        let remote = root.path().join("remote.git");
        let repo = root.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(root.path(), &["init", "-q", "--bare", "remote.git"]);
        git(&repo, &["init", "-q"]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "initial"]);
        git(&repo, &["tag", "v1"]);
        git(
            &repo,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        let shim = install_git_shim(&root.path().join("data")).unwrap();
        let path = format!(
            "{}:{}",
            shim.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut command = Command::new("git");
        command
            .args(["-C", repo.to_str().unwrap(), "push"])
            .args(push_args)
            .env("PATH", path)
            .env_remove("TUIC_CAPABILITIES");
        if let Some(set) = capabilities {
            command.env("TUIC_CAPABILITIES", set);
        }
        let out = command.output().unwrap();
        let pushed = Command::new("git")
            .args(["--git-dir", remote.to_str().unwrap(), "for-each-ref"])
            .output()
            .unwrap();
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
            !pushed.stdout.is_empty(),
        )
    }

    /// A hookless repo with a bare remote; the capped agent's real spawn
    /// environment is taken from `apply_git_shim` and replayed in `shell`.
    #[cfg(target_os = "macos")]
    fn push_from_shell(shell: &str, capabilities: &str) -> (bool, String, bool) {
        let root = tempfile::TempDir::new_in(tuic_test_support::test_temp_root()).unwrap();
        let run = |dir: &std::path::Path, args: &[&str]| {
            let out = Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
                .args(args)
                .current_dir(dir)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}: {out:?}");
        };
        let remote = root.path().join("remote.git");
        let repo = root.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run(root.path(), &["init", "-q", "--bare", "remote.git"]);
        run(&repo, &["init", "-q"]);
        run(&repo, &["commit", "-q", "--allow-empty", "-m", "initial"]);
        run(
            &repo,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        let mut cmd = portable_pty::CommandBuilder::new("true");
        apply_git_shim(&mut cmd, &root.path().join("data")).unwrap();
        let mut command = Command::new(shell);
        command
            .args(["-lc", "git push origin HEAD:refs/heads/x"])
            .current_dir(&repo)
            .env(ENV_VAR, capabilities);
        for (key, value) in cmd.iter_extra_env_as_str() {
            command.env(key, value);
        }
        let out = command.output().unwrap();
        let pushed = Command::new("git")
            .args(["--git-dir", remote.to_str().unwrap(), "for-each-ref"])
            .output()
            .unwrap();
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
            !pushed.stdout.is_empty(),
        )
    }

    // Catches: a login shell (macOS path_helper, which Claude Code's shell
    // snapshot runs) moves the shim behind /usr/bin, and the real git then
    // pushes in a repository without the TUIC hook.
    #[cfg(target_os = "macos")]
    #[test]
    fn capped_push_is_refused_inside_a_login_shell() {
        for shell in ["/bin/zsh", "/bin/bash"] {
            let (ok, stderr, reached_remote) = push_from_shell(shell, "");
            assert!(!ok && !reached_remote, "{shell} -l pushed: {stderr}");
            assert!(stderr.contains("lacks the push capability"), "{stderr}");
        }
    }

    // Catches: pointing git at the TUIC hooks drops the repository's own hooks
    // (a pre-commit or pre-push the repository relies on stops running), or a
    // refusing repository hook no longer refuses.
    #[cfg(unix)]
    #[test]
    fn tuic_hooks_still_run_the_repository_hooks() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::TempDir::new_in(tuic_test_support::test_temp_root()).unwrap();
        let repo = root.path().join("repo");
        let remote = root.path().join("remote.git");
        let git = |dir: &std::path::Path, args: &[&str], cmd: &portable_pty::CommandBuilder| {
            let mut command = Command::new("git");
            command
                .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
                .args(args)
                .current_dir(dir);
            for (key, value) in cmd.iter_extra_env_as_str() {
                command.env(key, value);
            }
            command.output().unwrap()
        };
        let plain = portable_pty::CommandBuilder::new("true");
        std::fs::create_dir_all(&repo).unwrap();
        git(root.path(), &["init", "-q", "--bare", "remote.git"], &plain);
        git(&repo, &["init", "-q"], &plain);
        git(
            &repo,
            &["remote", "add", "origin", remote.to_str().unwrap()],
            &plain,
        );
        let hooks = repo.join(".git/hooks");
        for (name, body) in [
            ("pre-commit", "touch ran-pre-commit"),
            ("pre-push", "exit 3"),
        ] {
            let path = hooks.join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let mut capped = portable_pty::CommandBuilder::new("true");
        capped.env(ENV_VAR, "push");
        apply_git_shim(&mut capped, &root.path().join("data")).unwrap();
        let commit = git(
            &repo,
            &["commit", "-q", "--allow-empty", "-m", "m"],
            &capped,
        );
        assert!(commit.status.success(), "{commit:?}");
        assert!(
            repo.join("ran-pre-commit").exists(),
            "repository pre-commit skipped"
        );
        let push = git(&repo, &["push", "origin", "HEAD:refs/heads/x"], &capped);
        assert_eq!(
            push.status.code(),
            Some(1),
            "git reports a refused push as 1: {push:?}"
        );
        let refs = git(
            &repo,
            &["--git-dir", remote.to_str().unwrap(), "for-each-ref"],
            &plain,
        );
        assert!(
            refs.stdout.is_empty(),
            "the repository pre-push exit 3 did not refuse"
        );
    }

    // Catches: enforcement that depends on a per-repository hook.
    #[cfg(unix)]
    #[test]
    fn capped_agent_push_is_refused_without_repo_hook() {
        let (ok, stderr, reached_remote) =
            push_through_shim(Some(""), &["origin", "HEAD:refs/heads/x"]);
        assert!(!ok);
        assert!(stderr.contains("lacks the push capability"), "{stderr}");
        assert!(!reached_remote);
    }

    // Catches: the push capability also lets a tag push through.
    #[cfg(unix)]
    #[test]
    fn push_capability_does_not_push_tags_without_repo_hook() {
        let (ok, stderr, reached_remote) =
            push_through_shim(Some("push"), &["origin", "refs/tags/v1"]);
        assert!(!ok);
        assert!(stderr.contains("lacks the tag capability"), "{stderr}");
        assert!(!reached_remote);
        let (ok, _, reached_remote) = push_through_shim(Some("push"), &["--tags", "origin"]);
        assert!(!ok && !reached_remote);
    }

    // Catches: the shim blocks a granted push, or a capped agent's other git calls.
    #[cfg(unix)]
    #[test]
    fn granted_and_uncapped_sessions_push_as_before() {
        let (ok, stderr, reached_remote) =
            push_through_shim(Some("progress,push"), &["origin", "HEAD:refs/heads/x"]);
        assert!(ok && reached_remote, "{stderr}");
        let (ok, stderr, reached_remote) =
            push_through_shim(None, &["origin", "HEAD:refs/heads/x"]);
        assert!(ok && reached_remote, "{stderr}");
        let (ok, stderr, reached_remote) =
            push_through_shim(Some("land,tag"), &["origin", "refs/tags/v1"]);
        assert!(ok && reached_remote, "{stderr}");
    }
}
