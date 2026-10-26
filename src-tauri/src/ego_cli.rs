//! Running ego's own command line, for the configuration ACP does not carry.
//!
//! ACP has no provider surface and must not grow one: which model a person
//! defaults to, and whether a provider has a credential at all, are ego's
//! configuration rather than a property of a session. So the Providers tab reads
//! and writes that configuration by running ego exactly as a person would, and
//! TUICommander stores no key of its own and speaks to no provider.
//!
//! Four rules this module exists to keep, each of them a thing that goes wrong
//! quietly when it is left to a caller:
//!
//! * **The binary is the configured one.** No argument names a program, so no
//!   caller over IPC or HTTP can choose what this host launches — the same rule
//!   `acp_commands` keeps for the ACP spawn.
//! * **Keys belong to the owner.** The UI keeps dedicated model/perimeter
//!   operations; the settings adapter admits only its typed managed-key schema.
//!   Neither path accepts arbitrary commands or edits Ego configuration files.
//! * **A value may not look like a flag.** The arguments never touch a shell, so
//!   there is nothing to quote; what is left is a value starting with `-`, which
//!   clap would read as an option. Refused here, and `--` is passed as well.
//! * **A failure carries what ego printed.** Swallowing stderr turns "no
//!   credential for anthropic" into "something went wrong", which is the one
//!   thing a person looking at a Providers tab cannot act on.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::AppState;

mod context;
pub(crate) mod perimeter;
pub(crate) use context::ConfigContext;

/// How much of a child's output is kept when reporting a failure.
///
/// Enough to hold a stack trace or a usage message, small enough that a
/// runaway child cannot push megabytes through IPC into a toast.
const MAX_CAPTURED: usize = 4000;

/// The longest model id this host will hand to `ego config set`.
///
/// Not a protocol limit — ego decides what is a model. It bounds the argument
/// so a caller cannot make the command line itself the payload.
const MAX_MODEL_LEN: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EgoCliErrorCode {
    /// No ego binary is configured. Nothing was run.
    NotConfigured,
    /// The value a caller asked to write is not one this host will pass on.
    InvalidInput,
    /// The binary could not be started at all — missing, not executable, wrong
    /// architecture. Distinct from a refusal, because the fix is the setting.
    LaunchFailed,
    /// ego ran and refused. `stderr` holds its own words.
    CommandFailed,
    /// ego ran, reported success, and printed something this host cannot read.
    UnreadableOutput,
}

/// A failure of the ego command line, carrying what it printed.
///
/// `stdout` is kept alongside `stderr` because the two commands that fail most
/// usefully fail differently: a refusal goes to stderr, while output this host
/// cannot parse *is* the stdout. Both are clipped, never summarised.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EgoCliError {
    pub code: EgoCliErrorCode,
    /// What this host was doing, in its own words. Never a paraphrase of ego.
    pub message: String,
    /// The command line that was run, without the program's directory.
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

impl EgoCliError {
    fn plain(code: EgoCliErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            command: String::new(),
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
        }
    }

    pub(crate) fn not_configured() -> Self {
        Self::plain(
            EgoCliErrorCode::NotConfigured,
            "no ego executable is configured; set it in Settings before reading ego's configuration",
        )
    }

    fn invalid_input(message: impl Into<String>) -> Self {
        Self::plain(EgoCliErrorCode::InvalidInput, message)
    }
}

impl std::fmt::Display for EgoCliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if !self.stderr.is_empty() {
            write!(f, ": {}", self.stderr)?;
        }
        Ok(())
    }
}

impl std::error::Error for EgoCliError {}

/// Keep at most [`MAX_CAPTURED`] characters, saying so when there were more.
///
/// Counted in characters rather than bytes so the clip cannot land inside a
/// multi-byte sequence and produce a replacement character that looks like the
/// child printed one.
fn clip(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    let trimmed = text.trim_end();
    if trimmed.chars().count() <= MAX_CAPTURED {
        return trimmed.to_string();
    }
    let kept: String = trimmed.chars().take(MAX_CAPTURED).collect();
    format!("{kept}\n[... truncated]")
}

/// The command line as it should be read back to a person.
///
/// The program is shown by file name: the full path is the person's own setting
/// and repeating it in every error adds a line without adding a fact.
fn spell(program: &Path, args: &[String]) -> String {
    let name = program
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| program.to_string_lossy().into_owned());
    if args.is_empty() {
        name
    } else {
        format!("{name} {}", args.join(" "))
    }
}

/// Run `program` with `args` and return its stdout, or a failure that quotes it.
///
/// Takes the program rather than reading it from configuration so the runner
/// itself is testable against a stock shell: the rule that only the configured
/// binary may be launched lives in [`ConfigContext::configured`], which every caller here goes
/// through, and not in a parameter a test has to work around.
///
/// Stderr is dropped on success on purpose. Every caller asks for `--json` and
/// reads stdout; a warning ego printed alongside a correct answer is not
/// something a Providers tab can act on, and carrying it would invite a caller
/// to render it as though the command had half-failed.
#[cfg(test)]
pub(crate) async fn run(program: &Path, args: &[String]) -> Result<String, EgoCliError> {
    run_at(program, args, None).await
}

async fn run_at(
    program: &Path,
    args: &[String],
    cwd: Option<&Path>,
) -> Result<String, EgoCliError> {
    let mut command = tokio::process::Command::new(program);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd).env_remove("EGO_PROFILE");
    }
    let output = command.output().await.map_err(|err| EgoCliError {
        code: EgoCliErrorCode::LaunchFailed,
        message: format!("could not run the configured ego executable: {err}"),
        command: spell(program, args),
        stdout: String::new(),
        stderr: String::new(),
        exit_code: None,
    })?;

    if !output.status.success() {
        return Err(EgoCliError {
            code: EgoCliErrorCode::CommandFailed,
            message: "ego refused the command".to_string(),
            command: spell(program, args),
            stdout: clip(&output.stdout),
            stderr: clip(&output.stderr),
            exit_code: output.status.code(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string())
}

/// Refuse a model id this host will not put on a command line.
///
/// Deliberately not a check that the model exists: ego owns that answer, and
/// asking twice would race a catalogue that can change between the two calls.
/// What is checked is only what makes the *argument* unsafe or unreadable — an
/// empty value, a value that clap would read as a flag, whitespace or control
/// characters that would silently split or corrupt it, and a length that turns
/// the command line into the payload.
pub(crate) fn validate_model(model: &str) -> Result<&str, EgoCliError> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return Err(EgoCliError::invalid_input(
            "a model id is required; ego names a model as provider/model",
        ));
    }
    if trimmed.chars().count() > MAX_MODEL_LEN {
        return Err(EgoCliError::invalid_input(format!(
            "that model id is longer than {MAX_MODEL_LEN} characters"
        )));
    }
    if trimmed.starts_with('-') {
        return Err(EgoCliError::invalid_input(
            "a model id may not start with '-'; ego would read it as an option",
        ));
    }
    if trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(EgoCliError::invalid_input(
            "a model id may not contain spaces or control characters",
        ));
    }
    // The value is spelled into a TOML string, `model="…"`. A quote or a
    // backslash inside it would end that string early or escape the character
    // after it, so ego would store something other than what was asked for —
    // or refuse with a TOML parse error that names neither.
    if trimmed.contains(['"', '\\']) {
        return Err(EgoCliError::invalid_input(
            "a model id may not contain a quote or a backslash",
        ));
    }
    Ok(trimmed)
}

// ---------------------------------------------------------------------------
// What ego prints. Only the fields this host reads are declared, so a field ego
// adds is ignored rather than fatal — and a `status` or a `section` ego invents
// lands on a variant that says "unknown" instead of failing the whole tab.
// ---------------------------------------------------------------------------

/// `ego config ls --json` — `{"v":1,"scope":…,"profile":…,"values":{…}}`.
#[derive(Deserialize)]
struct ConfigListing {
    #[serde(default)]
    values: serde_json::Map<String, serde_json::Value>,
}

/// `ego models --json` — the shared model catalogue plus one row per source.
#[derive(Deserialize)]
struct CatalogueSnapshot {
    #[serde(default)]
    models: Vec<CatalogueRow>,
    #[serde(default)]
    sources: Vec<SourceRow>,
}

#[derive(Deserialize)]
struct CatalogueRow {
    /// `provider/model`, as one string. ego splits on the **first** separator
    /// only, because the model half may itself contain slashes.
    slug: String,
    availability: Availability,
}

#[derive(Deserialize)]
struct SourceRow {
    provider: String,
}

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Availability {
    Available,
    Unavailable {
        #[serde(default)]
        message: String,
    },
    /// A status this build does not know. Kept rather than refused: an ego that
    /// grows a third status must not empty the Providers tab.
    #[serde(other)]
    Unknown,
}

/// `ego doctor --json` — the health snapshot, of which one section is read.
#[derive(Deserialize)]
struct HealthSnapshot {
    #[serde(default)]
    sections: Vec<HealthSectionReport>,
}

#[derive(Deserialize)]
struct HealthSectionReport {
    /// Read as text rather than as an enum so a section ego adds is skipped
    /// instead of failing the parse of the section this host needs.
    section: String,
    #[serde(default)]
    items: Vec<HealthItem>,
}

#[derive(Deserialize)]
struct HealthItem {
    id: HealthItemId,
    status: String,
    #[serde(default)]
    detail: String,
}

#[derive(Deserialize)]
struct HealthItemId {
    probe: String,
    #[serde(default)]
    subject: Option<String>,
}

// ---------------------------------------------------------------------------
// The projection the Providers tab renders.
// ---------------------------------------------------------------------------

/// Whether a provider can be used, as `ego doctor` reported it.
///
/// Four states rather than a boolean because the three unhappy ones want
/// different words on screen and different next steps: a stored credential that
/// expired is refreshed by ego on its next run and needs nothing from a person,
/// a missing one needs `ego auth login`, and a store doctor could not read is
/// not evidence that anything is missing at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum EgoCredential {
    Stored { detail: String },
    Expired { detail: String },
    Missing,
    Unknown { detail: String },
}

/// One model ego knows about, under the provider that offers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EgoModel {
    /// The whole `provider/model` id — what `ego config set model` takes.
    pub slug: String,
    /// The half after the first separator, for a list already grouped by
    /// provider. Never re-derived on the frontend: the split is on the first
    /// `/` and a second opinion about that would mangle `openrouter/z-ai/…`.
    pub name: String,
    pub available: bool,
    /// Why not, in ego's own words, when it is not.
    pub unavailable: Option<String>,
}

/// One provider, its credential, and the models it offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EgoProvider {
    pub name: String,
    pub credential: EgoCredential,
    pub models: Vec<EgoModel>,
}

/// Everything the Providers tab renders, assembled here rather than there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EgoProviders {
    /// The value of ego's `model` key, when one is set. This is the default a
    /// run starts from; the in-chat switch is a session option and does not
    /// touch it.
    pub default_model: Option<String>,
    /// Sorted by provider name, so two reads of an unchanged installation
    /// render identically.
    pub providers: Vec<EgoProvider>,
}

/// Read `key` out of `ego config ls --json`.
fn config_string(listing: &ConfigListing, key: &str) -> Option<String> {
    listing.values.get(key)?.as_str().map(str::to_owned)
}

/// The provider half of a slug, split on the first separator only.
fn provider_of(slug: &str) -> Option<&str> {
    match slug.split_once('/') {
        Some((provider, model)) if !provider.is_empty() && !model.is_empty() => Some(provider),
        _ => None,
    }
}

/// Turn doctor's credentials section into one state per provider.
///
/// Two shapes come out of that section and they mean opposite things. One item
/// per provider (`credentials.entry:<provider>`) is a populated store. A single
/// `credentials.store` item is the store speaking about itself — either "no
/// provider credentials are stored", which makes every provider `Missing`, or a
/// failure to read it at all, which makes every provider `Unknown`. Reading the
/// second shape as the first would report every provider as missing a
/// credential because doctor could not open the file.
fn credentials(
    snapshot: &HealthSnapshot,
) -> (
    std::collections::BTreeMap<String, EgoCredential>,
    EgoCredential,
) {
    let mut per_provider = std::collections::BTreeMap::new();
    let mut fallback = EgoCredential::Missing;

    let items = snapshot
        .sections
        .iter()
        .filter(|section| section.section == "credentials")
        .flat_map(|section| section.items.iter());

    for item in items {
        match (item.id.probe.as_str(), item.id.subject.as_deref()) {
            ("credentials.entry", Some(provider)) => {
                let state = match item.status.as_str() {
                    "ok" => EgoCredential::Stored {
                        detail: item.detail.clone(),
                    },
                    "warning" => EgoCredential::Expired {
                        detail: item.detail.clone(),
                    },
                    _ => EgoCredential::Unknown {
                        detail: item.detail.clone(),
                    },
                };
                per_provider.insert(provider.to_string(), state);
            }
            // The store itself. `not_checked` is doctor saying it looked and
            // the store is empty; anything else is doctor saying it could not
            // look, which is not the same as "nothing is there".
            ("credentials.store" | "credentials.home", _) if item.status != "not_checked" => {
                fallback = EgoCredential::Unknown {
                    detail: item.detail.clone(),
                };
            }
            _ => {}
        }
    }

    (per_provider, fallback)
}

/// Assemble the three answers into the one thing the tab renders.
///
/// Pure, and separate from the three processes that produce its inputs, so the
/// join — which provider a model belongs to, which credential a provider has,
/// what happens to a provider ego lists but says nothing about — is testable
/// without an ego on the machine.
fn project(
    listing: ConfigListing,
    catalogue: CatalogueSnapshot,
    health: HealthSnapshot,
) -> EgoProviders {
    let (per_provider, fallback) = credentials(&health);

    let mut grouped: std::collections::BTreeMap<String, Vec<EgoModel>> =
        std::collections::BTreeMap::new();

    // A source with no usable model still gets a row: "anthropic is configured
    // and has no credential" is the answer a person came to the tab for, and an
    // empty list would read as "ego does not know about anthropic".
    for source in &catalogue.sources {
        grouped.entry(source.provider.clone()).or_default();
    }

    for row in catalogue.models {
        let Some(provider) = provider_of(&row.slug) else {
            continue;
        };
        let name = row
            .slug
            .split_once('/')
            .map(|(_, model)| model.to_string())
            .unwrap_or_else(|| row.slug.clone());
        let (available, unavailable) = match row.availability {
            Availability::Available => (true, None),
            Availability::Unavailable { message } => (false, Some(message)),
            Availability::Unknown => (false, None),
        };
        grouped
            .entry(provider.to_string())
            .or_default()
            .push(EgoModel {
                slug: row.slug,
                name,
                available,
                unavailable,
            });
    }

    let providers = grouped
        .into_iter()
        .map(|(name, models)| EgoProvider {
            credential: per_provider.get(&name).cloned().unwrap_or(fallback.clone()),
            name,
            models,
        })
        .collect();

    EgoProviders {
        default_model: config_string(&listing, "model"),
        providers,
    }
}

/// Say which command a parse failure was about, keeping what it printed.
fn unreadable(command: &str, stdout: String, error: serde_json::Error) -> EgoCliError {
    EgoCliError {
        code: EgoCliErrorCode::UnreadableOutput,
        message: format!("could not read what `{command}` printed: {error}"),
        command: command.to_string(),
        stdout,
        stderr: String::new(),
        exit_code: None,
    }
}

/// Everything the Providers tab shows, read from ego in three calls.
///
/// All three must succeed. A partial answer would have to render as a tab that
/// is silently missing one of the three things it exists to show — which model
/// is the default, which models exist, whether a provider has a credential —
/// and none of those is optional to the question being asked. Whichever call
/// fails is reported with what ego printed.
///
/// `refresh` is passed straight to `ego models`, which is the only thing here
/// that reaches a provider over the network, and it is ego that reaches it.
/// Off by default: a tab that refreshed on every open would turn opening
/// Settings into a round of provider requests.
pub(crate) async fn providers(
    state: &AppState,
    refresh: bool,
) -> Result<EgoProviders, EgoCliError> {
    providers_in(&ConfigContext::configured(state)?, refresh).await
}

pub(crate) async fn providers_in(
    context: &ConfigContext,
    refresh: bool,
) -> Result<EgoProviders, EgoCliError> {
    let raw = context.command("ls", &["--json"]).await?;
    let mut listing: ConfigListing = serde_json::from_str(&raw)
        .map_err(|error| unreadable("ego config ls --json", raw, error))?;
    // A profile without its own model inherits the base model of the same layer.
    if context.profile.is_some() && !listing.values.contains_key("model") {
        let args = ["config".into(), "ls".into(), "--json".into()];
        let raw = context.run(&args).await?;
        let base: ConfigListing = serde_json::from_str(&raw)
            .map_err(|error| unreadable("ego config ls --json", raw, error))?;
        if let Some(model) = base.values.get("model") {
            listing.values.insert("model".into(), model.clone());
        }
    }
    let mut models_args = vec!["models".into(), "--json".into()];
    if refresh {
        models_args.push("--refresh".into());
    }
    let raw = context.run(&models_args).await?;
    let catalogue: CatalogueSnapshot =
        serde_json::from_str(&raw).map_err(|error| unreadable("ego models --json", raw, error))?;
    let raw = context.run(&["doctor".into(), "--json".into()]).await?;
    let health: HealthSnapshot =
        serde_json::from_str(&raw).map_err(|error| unreadable("ego doctor --json", raw, error))?;
    Ok(project(listing, catalogue, health))
}

/// Write ego's default model, then read the whole tab back.
///
/// Reading back rather than trusting the write is the point: what the tab then
/// shows is what ego persisted, so a value ego stored differently — or did not
/// store at all — cannot be rendered as though it had been accepted.
///
/// The assignment is one argument, `model="…"`, because that is the grammar
/// `ego config set` takes; the quotes are part of the TOML value and no shell
/// ever sees them.
pub(crate) async fn set_default_model(
    state: &AppState,
    model: String,
) -> Result<EgoProviders, EgoCliError> {
    let context = ConfigContext::configured(state)?;
    let validated = validate_model(&model)?;
    let assignment = format!("model=\"{validated}\"");
    context.command("set", &[&assignment]).await?;
    providers_in(&context, false).await
}

// ---------------------------------------------------------------------------
// Tauri commands. One per operation, each calling the same core the HTTP route
// calls, so the desktop and a browser cannot answer differently.
// ---------------------------------------------------------------------------

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn ego_providers(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    refresh: Option<bool>,
) -> Result<EgoProviders, EgoCliError> {
    providers(&state, refresh.unwrap_or(false)).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn ego_set_default_model(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    model: String,
) -> Result<EgoProviders, EgoCliError> {
    set_default_model(&state, model).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fail_with_stderr_script, host_shell};
    use std::path::PathBuf;

    /// `sh -c <script>` / `cmd /C <script>`, as the runner's argument vector.
    fn shell(script: String) -> (PathBuf, Vec<String>) {
        let (program, flag) = host_shell();
        (PathBuf::from(program), vec![flag.to_string(), script])
    }

    #[tokio::test]
    async fn returns_what_the_command_printed() {
        let (program, args) = shell("echo hello".to_string());
        let stdout = run(&program, &args).await.expect("the shell must succeed");
        assert_eq!(stdout, "hello");
    }

    // A successful answer is parsed, not displayed: `ego models --json` prints
    // ~10 KB, and clipping it to the failure-report cap broke the JSON.
    #[tokio::test]
    async fn a_long_successful_answer_is_returned_whole() {
        let long = "x".repeat(MAX_CAPTURED * 3);
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let answer = dir.path().join("answer.txt");
        std::fs::write(&answer, &long).unwrap();
        // cmd.exe has an 8191-character command-line limit; the output does not.
        let (program, args) = shell(if cfg!(windows) {
            // The repository test temp path has no spaces; avoid cmd quote re-escaping.
            tuic_test_support::print_file_script(&answer.display().to_string())
        } else {
            tuic_test_support::print_file_script(&format!("\"{}\"", answer.display()))
        });
        let stdout = run(&program, &args).await.expect("the shell must succeed");
        assert_eq!(stdout, long);
    }

    // The criterion this module exists for: a failure is reported with what ego
    // printed. A message that said only "ego failed" would leave a person with a
    // Providers tab and nothing to act on.
    #[tokio::test]
    async fn a_refusal_carries_stderr_and_the_exit_code() {
        let (program, args) = shell(fail_with_stderr_script("no-credential-for-anthropic", 3));
        let err = run(&program, &args)
            .await
            .expect_err("a non-zero exit must be a failure");
        assert_eq!(err.code, EgoCliErrorCode::CommandFailed);
        assert!(
            err.stderr.contains("no-credential-for-anthropic"),
            "stderr was swallowed: {err:?}"
        );
        assert_eq!(err.exit_code, Some(3));
    }

    // A missing binary is not the same failure as a refusal: the fix is the
    // setting, not the request, and the tab says so differently.
    #[tokio::test]
    async fn a_binary_that_cannot_start_is_not_a_refusal() {
        let program = PathBuf::from("/nonexistent/ego-that-is-not-there");
        let err = run(&program, &[])
            .await
            .expect_err("a missing program must fail");
        assert_eq!(err.code, EgoCliErrorCode::LaunchFailed);
        assert_eq!(err.exit_code, None);
    }

    // The error names the command without repeating the person's own path back
    // at them on every line.
    #[test]
    fn the_command_is_spelled_by_file_name() {
        let spelled = spell(
            Path::new("/opt/ego/bin/ego"),
            &["config".to_string(), "ls".to_string()],
        );
        assert_eq!(spelled, "ego config ls");
    }

    #[test]
    fn output_is_clipped_rather_than_summarised() {
        let long = "x".repeat(MAX_CAPTURED + 50);
        let clipped = clip(long.as_bytes());
        assert!(clipped.starts_with(&"x".repeat(MAX_CAPTURED)));
        assert!(clipped.ends_with("[... truncated]"));
    }

    // Clipping counts characters, not bytes, so it can never land inside a
    // multi-byte sequence and invent a replacement character the child never
    // printed.
    #[test]
    fn clipping_never_splits_a_character() {
        let long = "è".repeat(MAX_CAPTURED + 10);
        let clipped = clip(long.as_bytes());
        assert!(!clipped.contains('\u{FFFD}'));
    }

    #[test]
    fn a_model_id_is_trimmed_and_kept() {
        assert_eq!(
            validate_model("  anthropic/claude-opus-5  ").expect("a plain id is fine"),
            "anthropic/claude-opus-5"
        );
    }

    // The arguments never touch a shell, so quoting is not the risk; a value
    // clap reads as an option is.
    #[test]
    fn a_model_id_may_not_look_like_a_flag() {
        let err = validate_model("--config=/etc/ego.toml").expect_err("a flag must be refused");
        assert_eq!(err.code, EgoCliErrorCode::InvalidInput);
    }

    #[test]
    fn a_model_id_may_not_be_empty() {
        assert_eq!(
            validate_model("   ")
                .expect_err("an empty id must be refused")
                .code,
            EgoCliErrorCode::InvalidInput
        );
    }

    #[test]
    fn a_model_id_may_not_carry_whitespace_or_control_characters() {
        assert!(validate_model("anthropic/claude opus").is_err());
        assert!(validate_model("anthropic/claude\nopus").is_err());
        assert!(validate_model("anthropic/claude\u{0}opus").is_err());
    }

    #[test]
    fn a_model_id_may_not_be_the_payload() {
        let long = "a".repeat(MAX_MODEL_LEN + 1);
        assert_eq!(
            validate_model(&long)
                .expect_err("an overlong id must be refused")
                .code,
            EgoCliErrorCode::InvalidInput
        );
    }

    // The value is written into a TOML string. A quote would close it early and
    // ego would store a different model, or refuse with a parse error naming
    // neither the key nor the value that caused it.
    #[test]
    fn a_model_id_may_not_break_out_of_the_toml_string() {
        assert!(validate_model("anthropic/opus\",bare=true").is_err());
        assert!(validate_model("anthropic/opus\\").is_err());
    }

    // -----------------------------------------------------------------------
    // The projection: which provider a model belongs to, which credential a
    // provider has, and what happens to a provider ego mentions only once.
    // -----------------------------------------------------------------------

    fn listing(json: &str) -> ConfigListing {
        serde_json::from_str(json).expect("the config listing fixture must parse")
    }

    fn catalogue(json: &str) -> CatalogueSnapshot {
        serde_json::from_str(json).expect("the catalogue fixture must parse")
    }

    fn health(json: &str) -> HealthSnapshot {
        serde_json::from_str(json).expect("the health fixture must parse")
    }

    const NO_CONFIG: &str = r#"{"v":1,"scope":"user","profile":null,"values":{}}"#;
    const NO_CREDENTIALS: &str = r#"{"v":1,"sections":[]}"#;

    #[test]
    fn the_default_model_comes_from_the_config_listing() {
        let projected = project(
            listing(
                r#"{"v":1,"scope":"user","profile":null,
                    "values":{"model":"anthropic/claude-opus-5","bare":true}}"#,
            ),
            catalogue(r#"{"v":2,"models":[],"sources":[]}"#),
            health(NO_CREDENTIALS),
        );
        assert_eq!(
            projected.default_model.as_deref(),
            Some("anthropic/claude-opus-5")
        );
    }

    #[test]
    fn no_default_model_is_absent_rather_than_empty() {
        let projected = project(
            listing(NO_CONFIG),
            catalogue(r#"{"v":2,"models":[],"sources":[]}"#),
            health(NO_CREDENTIALS),
        );
        assert_eq!(projected.default_model, None);
    }

    // ego splits a slug on the FIRST separator only, because the model half may
    // contain slashes of its own. Splitting on the last one would file
    // `openrouter/z-ai/glm-4.7` under a provider called `openrouter/z-ai`.
    #[test]
    fn a_slug_is_split_on_the_first_separator_only() {
        let projected = project(
            listing(NO_CONFIG),
            catalogue(
                r#"{"v":2,"sources":[],"models":[
                    {"slug":"openrouter/z-ai/glm-4.7","availability":{"status":"available"}}]}"#,
            ),
            health(NO_CREDENTIALS),
        );
        let provider = &projected.providers[0];
        assert_eq!(provider.name, "openrouter");
        assert_eq!(provider.models[0].name, "z-ai/glm-4.7");
        assert_eq!(provider.models[0].slug, "openrouter/z-ai/glm-4.7");
    }

    // A source ego lists but has no model for is still a row. An empty list
    // reads as "ego does not know about this provider", which is the opposite
    // of what an unreachable source means.
    #[test]
    fn a_source_with_no_model_is_still_a_provider() {
        let projected = project(
            listing(NO_CONFIG),
            catalogue(
                r#"{"v":2,"models":[],"sources":[
                    {"provider":"anthropic","availability":{"status":"available"}}]}"#,
            ),
            health(NO_CREDENTIALS),
        );
        assert_eq!(projected.providers.len(), 1);
        assert_eq!(projected.providers[0].name, "anthropic");
        assert_eq!(
            projected.providers[0].models,
            [] as [crate::ego_cli::EgoModel; 0]
        );
    }

    #[test]
    fn an_unavailable_model_keeps_egos_own_words() {
        let projected = project(
            listing(NO_CONFIG),
            catalogue(
                r#"{"v":2,"sources":[],"models":[
                    {"slug":"openai/gpt-5","availability":
                     {"status":"unavailable","cause":"missing_credential",
                      "message":"no credential for openai"}}]}"#,
            ),
            health(NO_CREDENTIALS),
        );
        let model = &projected.providers[0].models[0];
        assert!(!model.available);
        assert_eq!(
            model.unavailable.as_deref(),
            Some("no credential for openai")
        );
    }

    // An availability status a later ego invents must not empty the tab.
    #[test]
    fn an_unknown_availability_status_is_survived() {
        let projected = project(
            listing(NO_CONFIG),
            catalogue(
                r#"{"v":2,"sources":[],"models":[
                    {"slug":"openai/gpt-5","availability":{"status":"deprecated"}}]}"#,
            ),
            health(NO_CREDENTIALS),
        );
        assert!(!projected.providers[0].models[0].available);
    }

    #[test]
    fn a_stored_credential_is_reported_per_provider() {
        let projected = project(
            listing(NO_CONFIG),
            catalogue(
                r#"{"v":2,"models":[],"sources":[{"provider":"anthropic"},{"provider":"openai"}]}"#,
            ),
            health(
                r#"{"v":1,"sections":[{"section":"credentials","items":[
                    {"id":{"probe":"credentials.entry","subject":"anthropic"},
                     "status":"ok","detail":"oauth credential stored","source":"the credential store"}]}]}"#,
            ),
        );
        assert_eq!(
            projected.providers[0].credential,
            EgoCredential::Stored {
                detail: "oauth credential stored".to_string()
            }
        );
        // openai was never mentioned by doctor, so nothing is stored for it.
        assert_eq!(projected.providers[1].credential, EgoCredential::Missing);
    }

    // Expired is its own state: ego refreshes it on the next run, so telling a
    // person to log in again would send them to do work nobody needs.
    #[test]
    fn an_expired_credential_is_not_a_missing_one() {
        let projected = project(
            listing(NO_CONFIG),
            catalogue(r#"{"v":2,"models":[],"sources":[{"provider":"anthropic"}]}"#),
            health(
                r#"{"v":1,"sections":[{"section":"credentials","items":[
                    {"id":{"probe":"credentials.entry","subject":"anthropic"},
                     "status":"warning","detail":"oauth credential expired","source":"s"}]}]}"#,
            ),
        );
        assert_eq!(
            projected.providers[0].credential,
            EgoCredential::Expired {
                detail: "oauth credential expired".to_string()
            }
        );
    }

    // The failure this join exists to avoid. A store doctor could not READ is
    // not a store that is empty, and reporting every provider as missing a
    // credential because a lock file was busy would send a person to re-run
    // every login they already did.
    #[test]
    fn a_store_doctor_could_not_read_is_not_a_missing_credential() {
        let projected = project(
            listing(NO_CONFIG),
            catalogue(r#"{"v":2,"models":[],"sources":[{"provider":"anthropic"}]}"#),
            health(
                r#"{"v":1,"sections":[{"section":"credentials","items":[
                    {"id":{"probe":"credentials.store"},
                     "status":"error","detail":"credential store is unreadable","source":"s"}]}]}"#,
            ),
        );
        assert_eq!(
            projected.providers[0].credential,
            EgoCredential::Unknown {
                detail: "credential store is unreadable".to_string()
            }
        );
    }

    // The other shape of the same item: doctor looked, and there is nothing.
    #[test]
    fn an_empty_store_leaves_every_provider_missing() {
        let projected = project(
            listing(NO_CONFIG),
            catalogue(r#"{"v":2,"models":[],"sources":[{"provider":"anthropic"}]}"#),
            health(
                r#"{"v":1,"sections":[{"section":"credentials","items":[
                    {"id":{"probe":"credentials.store"},
                     "status":"not_checked","detail":"no provider credentials are stored","source":"s"}]}]}"#,
            ),
        );
        assert_eq!(projected.providers[0].credential, EgoCredential::Missing);
    }

    // Only the credentials section answers this question. A warning in
    // `catalogue` or `mcp` says nothing about whether a provider can be used.
    #[test]
    fn another_section_does_not_decide_a_credential() {
        let projected = project(
            listing(NO_CONFIG),
            catalogue(r#"{"v":2,"models":[],"sources":[{"provider":"anthropic"}]}"#),
            health(
                r#"{"v":1,"sections":[{"section":"mcp","items":[
                    {"id":{"probe":"credentials.store"},
                     "status":"error","detail":"an unrelated failure","source":"s"}]}]}"#,
            ),
        );
        assert_eq!(projected.providers[0].credential, EgoCredential::Missing);
    }

    #[test]
    fn providers_are_sorted_so_two_reads_render_alike() {
        let projected = project(
            listing(NO_CONFIG),
            catalogue(
                r#"{"v":2,"models":[],"sources":[
                    {"provider":"openai"},{"provider":"anthropic"},{"provider":"google"}]}"#,
            ),
            health(NO_CREDENTIALS),
        );
        let names: Vec<&str> = projected
            .providers
            .iter()
            .map(|provider| provider.name.as_str())
            .collect();
        assert_eq!(names, ["anthropic", "google", "openai"]);
    }

    // A field ego adds must be ignored, not fatal: the tab would otherwise go
    // blank on an ego upgrade that changed nothing this host reads.
    #[test]
    fn a_field_this_host_does_not_read_is_ignored() {
        let projected = project(
            listing(NO_CONFIG),
            catalogue(
                r#"{"v":2,"sources":[],"models":[
                    {"slug":"anthropic/opus","availability":{"status":"available"},
                     "capabilities":{"vision":true},"metering":"tokens",
                     "provenance":{"discovered":{"provider":"anthropic"}},
                     "freshness":"cached","somethingNew":42}]}"#,
            ),
            health(NO_CREDENTIALS),
        );
        assert_eq!(projected.providers[0].models[0].slug, "anthropic/opus");
    }
}
