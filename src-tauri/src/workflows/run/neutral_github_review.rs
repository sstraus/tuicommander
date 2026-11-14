//! Installed GitHub pull request operations for neutral workflows: capture the
//! exact head/base and publish one review pinned to that head. The neutral run
//! owner keeps scheduling, overlap and uncertainty; this adapter only reads the
//! current PR and writes the one review its effect owns.
use super::neutral_effects::{EffectObservation, EffectOutcome, EffectPreparation, NeutralEffect};
use super::neutral_execution::NeutralResult;
use super::neutral_runtime::{NeutralHost, NeutralInvocation, ObservationFuture};
use crate::github_pr_api::{PrApi, RepoApi};
use crate::workflows::catalog::{
    ActivityDescriptor, ActivityInstance, DescriptorId, FieldSchema, HandlerRef, ObjectSchema,
    ValueType,
};
use crate::workflows::neutral_admission::{HostAdmission, HostContext, HostRegistry};
use crate::workflows::neutral_publication::NeutralPublication;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Bound on the review context: PR text plus the exact base...head patch.
const MAX_TEXT_BYTES: usize = 400_000;
const MAX_BODY_BYTES: usize = 60_000;
/// Reconciliation reads at most this many pages of 100 reviews.
const MAX_REVIEW_PAGES: u32 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Operation {
    Snapshot,
    Review,
}
pub(crate) const OPERATIONS: [Operation; 2] = [Operation::Snapshot, Operation::Review];

impl Operation {
    fn name(self) -> &'static str {
        match self {
            Self::Snapshot => "pull_request.snapshot",
            Self::Review => "pull_request.review",
        }
    }
    fn mutation(self) -> bool {
        self == Self::Review
    }
}

fn field(value_type: ValueType, required: bool) -> FieldSchema {
    FieldSchema {
        value_type,
        required,
        default: None,
    }
}

pub(crate) fn descriptor(op: Operation) -> ActivityDescriptor {
    use ValueType::{Integer, String};
    let mut inputs = BTreeMap::from([
        ("repository".into(), field(String, true)),
        ("number".into(), field(Integer, true)),
        ("head".into(), field(String, true)),
        ("base".into(), field(String, true)),
    ]);
    let (purpose, outputs, outcomes) = match op {
        Operation::Snapshot => (
            "Read one open GitHub pull request at its exact head/base, with the base...head patch",
            BTreeMap::from([
                ("title".into(), field(String, false)),
                ("text".into(), field(String, false)),
            ]),
            ["ok", "stale"],
        ),
        Operation::Review => {
            inputs.insert("body".into(), field(String, true));
            inputs.insert("event".into(), field(String, true));
            (
                "Publish one GitHub pull request review pinned to the exact head commit",
                BTreeMap::from([
                    ("review_id".into(), field(Integer, false)),
                    ("url".into(), field(String, false)),
                ]),
                ["published", "stale"],
            )
        }
    };
    ActivityDescriptor {
        id: DescriptorId {
            namespace: "github".into(),
            name: op.name().into(),
            version: 1,
        },
        purpose: purpose.into(),
        handler: HandlerRef {
            name: format!("installed.github.{}", op.name()),
            version: 1,
        },
        target: None,
        config: ObjectSchema::default(),
        inputs: ObjectSchema { fields: inputs },
        subject: None,
        outputs: ObjectSchema { fields: outputs },
        outcomes: outcomes.into_iter().map(str::to_owned).collect(),
        required_capabilities: BTreeSet::new(),
        execution_policies: BTreeSet::from(["local".into()]),
    }
}

fn operation(descriptor: &ActivityDescriptor) -> Result<Operation, String> {
    OPERATIONS
        .into_iter()
        .find(|op| descriptor_matches(*op, descriptor))
        .ok_or_else(|| "GitHub descriptor differs from installed operation".into())
}

fn descriptor_matches(op: Operation, descriptor: &ActivityDescriptor) -> bool {
    descriptor == &self::descriptor(op)
}

/// Typed request after schema and value checks.
#[derive(Debug)]
struct Request<'a> {
    repository: String,
    number: u64,
    head: &'a str,
    base: &'a str,
}

fn sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn text<'a>(inputs: &'a BTreeMap<String, Value>, name: &str) -> Result<&'a str, String> {
    inputs
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("GitHub {name} requires text"))
}

fn request(op: Operation, inputs: &BTreeMap<String, Value>) -> Result<Request<'_>, String> {
    descriptor(op).inputs.resolve(inputs, "GitHub inputs")?;
    let repository = crate::workflows::github_trigger::repository(text(inputs, "repository")?)?;
    let number = inputs
        .get("number")
        .and_then(Value::as_u64)
        .filter(|n| *n > 0)
        .ok_or("GitHub number requires a positive integer")?;
    let (head, base) = (text(inputs, "head")?, text(inputs, "base")?);
    if !sha(head) || !sha(base) {
        return Err("GitHub head/base require full lowercase commit SHAs".into());
    }
    if op == Operation::Review {
        let body = text(inputs, "body")?;
        if body.trim().is_empty() || body.len() > MAX_BODY_BYTES {
            return Err("GitHub review body is empty or too large".into());
        }
        review_state(text(inputs, "event")?)?;
    }
    Ok(Request {
        repository,
        number,
        head,
        base,
    })
}

/// The read-back state of each event GitHub accepts on a review.
fn review_state(event: &str) -> Result<&'static str, String> {
    match event {
        "COMMENT" => Ok("COMMENTED"),
        "APPROVE" => Ok("APPROVED"),
        "REQUEST_CHANGES" => Ok("CHANGES_REQUESTED"),
        _ => Err("GitHub review event must be COMMENT, APPROVE or REQUEST_CHANGES".into()),
    }
}

/// The precommitted correlation that reconciliation looks up; never a new write.
fn marked_body(body: &str, digest: &str) -> String {
    format!("{body}\n\n<!-- tuic-effect:{digest} -->")
}

fn completed(outcome: &str, outputs: BTreeMap<String, Value>) -> EffectObservation {
    EffectObservation {
        stopped: true,
        outcome: EffectOutcome::Completed {
            result: NeutralResult::Completed {
                outcome: outcome.into(),
                outputs,
            },
        },
    }
}

fn failed(diagnostic: String) -> EffectObservation {
    EffectObservation {
        stopped: true,
        outcome: EffectOutcome::Completed {
            result: NeutralResult::Failed { diagnostic },
        },
    }
}

/// The PR is current only while it is open at the exact captured head and base.
async fn current(api: &impl PrApi, r: &Request<'_>) -> Result<(bool, Value), String> {
    let pr = api
        .rest("GET", &format!("pulls/{}", r.number), Value::Null)
        .await?;
    let fresh = pr["state"] == "open" && pr["head"]["sha"] == r.head && pr["base"]["sha"] == r.base;
    Ok((fresh, pr))
}

fn review_text(pr: &Value, r: &Request<'_>, compare: &Value) -> String {
    let mut text = format!(
        "Pull request #{} in {}: {}\nHead: {}\nBase: {}\n\n{}\n\nChanged files (base...head):\n",
        r.number,
        r.repository,
        pr["title"].as_str().unwrap_or(""),
        r.head,
        r.base,
        pr["body"].as_str().unwrap_or("")
    );
    for file in compare["files"].as_array().into_iter().flatten() {
        text.push_str(&format!(
            "\n--- {} ({})\n{}\n",
            file["filename"].as_str().unwrap_or("?"),
            file["status"].as_str().unwrap_or("?"),
            file["patch"].as_str().unwrap_or("(no textual patch)")
        ));
    }
    if text.len() > MAX_TEXT_BYTES {
        let mut end = MAX_TEXT_BYTES;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("\n[patch truncated]");
    }
    text
}

/// Execute one owned effect against an account-bound API for `repository`.
/// Errors before the review POST are definite failures; after it they stay
/// uncertain so the owner reconciles through the effect marker.
pub(super) async fn execute(
    api: &impl PrApi,
    repository: &str,
    effect: &NeutralEffect,
) -> Result<EffectObservation, String> {
    let op = operation(&effect.invocation.descriptor)?;
    let r = match request(op, &effect.invocation.inputs) {
        Ok(r) => r,
        Err(error) => return Ok(failed(error)),
    };
    if !repository.eq_ignore_ascii_case(&r.repository) {
        return Ok(failed(
            "GitHub workflow repository differs from the project repository".into(),
        ));
    }
    let (fresh, pr) = match current(api, &r).await {
        Ok(found) => found,
        Err(error) => return Ok(failed(format!("GitHub PR read failed: {error}"))),
    };
    if !fresh {
        // A newer head or a closed PR never receives the captured review.
        return Ok(completed("stale", BTreeMap::new()));
    }
    match op {
        Operation::Snapshot => {
            let compare = match api
                .rest(
                    "GET",
                    &format!("compare/{}...{}", r.base, r.head),
                    Value::Null,
                )
                .await
            {
                Ok(compare) => compare,
                Err(error) => return Ok(failed(format!("GitHub compare read failed: {error}"))),
            };
            Ok(completed(
                "ok",
                BTreeMap::from([
                    ("title".into(), pr["title"].clone()),
                    ("text".into(), json!(review_text(&pr, &r, &compare))),
                ]),
            ))
        }
        Operation::Review => {
            let event = text(&effect.invocation.inputs, "event")?;
            let body = marked_body(text(&effect.invocation.inputs, "body")?, &effect.key.digest);
            // commit_id pins the review to the captured head even when the head
            // moves between the read and this POST.
            let created = api
                .rest(
                    "POST",
                    &format!("pulls/{}/reviews", r.number),
                    json!({"commit_id": r.head, "body": body, "event": event}),
                )
                .await?;
            let id = created["id"]
                .as_u64()
                .filter(|id| *id > 0)
                .ok_or("GitHub review id missing; reconcile the original effect")?;
            let observed = api
                .rest(
                    "GET",
                    &format!("pulls/{}/reviews/{id}", r.number),
                    Value::Null,
                )
                .await?;
            published(&observed, &r, &body, review_state(event)?)
        }
    }
}

fn published(
    observed: &Value,
    r: &Request<'_>,
    body: &str,
    state: &str,
) -> Result<EffectObservation, String> {
    if observed["commit_id"] != r.head || observed["body"] != body || observed["state"] != state {
        return Err("GitHub exact-head review read-back differs".into());
    }
    Ok(completed(
        "published",
        BTreeMap::from([
            ("review_id".into(), observed["id"].clone()),
            ("url".into(), observed["html_url"].clone()),
        ]),
    ))
}

/// Look up the original review by its precommitted marker. A complete listing
/// without it proves that GitHub holds no review from this effect.
pub(super) async fn reconcile(
    api: &impl PrApi,
    effect: &NeutralEffect,
) -> Result<EffectObservation, String> {
    let op = operation(&effect.invocation.descriptor)?;
    if op != Operation::Review {
        return Err("GitHub read operations have no effect to reconcile".into());
    }
    let r = request(op, &effect.invocation.inputs)?;
    let event = text(&effect.invocation.inputs, "event")?;
    let body = marked_body(text(&effect.invocation.inputs, "body")?, &effect.key.digest);
    for page in 1..=MAX_REVIEW_PAGES {
        let reviews = api
            .rest(
                "GET",
                &format!("pulls/{}/reviews?per_page=100&page={page}", r.number),
                Value::Null,
            )
            .await?;
        let reviews = reviews
            .as_array()
            .ok_or("GitHub review list is not an array")?;
        if let Some(found) = reviews
            .iter()
            .find(|review| review["body"] == body.as_str())
        {
            return published(found, &r, &body, review_state(event)?);
        }
        if reviews.len() < 100 {
            return Ok(failed(
                "GitHub holds no review from this effect; nothing was published".into(),
            ));
        }
    }
    Err("GitHub review list exceeds the reconciliation bound".into())
}

#[derive(Default)]
pub(crate) struct GitHubContext;
impl HostContext for GitHubContext {}
pub(crate) type GitHubAuthority = dyn Fn() -> Result<(), String> + Send + Sync;

/// Account-bound adapter. The project path selects the account and repository
/// through the existing PR client; workflow data never names credentials.
pub(crate) struct GitHubHost {
    pub state: Arc<crate::AppState>,
    pub authority: Arc<GitHubAuthority>,
}

impl GitHubHost {
    fn check(
        &self,
        descriptor: &ActivityDescriptor,
        instance: &ActivityInstance,
        inputs: Option<&BTreeMap<String, Value>>,
    ) -> Result<Operation, String> {
        let op = operation(descriptor)?;
        if instance.descriptor != descriptor.id
            || instance.execution_policy != "local"
            || !instance.config.is_empty()
        {
            return Err("GitHub instance differs from installed operation".into());
        }
        (self.authority)()?;
        if let Some(inputs) = inputs {
            request(op, inputs)?;
        }
        Ok(op)
    }
}

impl HostAdmission for GitHubHost {
    fn publication(
        &self,
        _: &dyn HostContext,
        _: &NeutralPublication,
        descriptor: &ActivityDescriptor,
        instance: &ActivityInstance,
    ) -> Result<(), String> {
        self.check(descriptor, instance, None).map(|_| ())
    }
    fn dispatch(
        &self,
        _: &dyn HostContext,
        _: &NeutralPublication,
        descriptor: &ActivityDescriptor,
        instance: &ActivityInstance,
        inputs: &BTreeMap<String, Value>,
        _: Option<&BTreeMap<String, Value>>,
    ) -> Result<(), String> {
        self.check(descriptor, instance, Some(inputs)).map(|_| ())
    }
}

impl NeutralHost for GitHubHost {
    fn context(&self, _: &NeutralInvocation) -> Result<Box<dyn HostContext>, String> {
        (self.authority)()?;
        Ok(Box::new(GitHubContext))
    }
    fn registry(&self) -> HostRegistry<'_> {
        let mut registry = HostRegistry::default();
        registry.handlers = OPERATIONS
            .into_iter()
            .map(|op| (descriptor(op).handler, self as &dyn HostAdmission))
            .collect();
        registry
    }
    fn prepare(&self, invocation: &NeutralInvocation) -> Result<EffectPreparation, String> {
        prepare(invocation)
    }
    fn invoke_effect(&self, effect: NeutralEffect) -> ObservationFuture {
        let ready = prepare(&effect.invocation).and_then(|prepared| {
            if !effect.dispatched || !effect.approved() || prepared != effect.preparation {
                return Err("GitHub effect lacks current dispatch ownership".into());
            }
            (self.authority)()
        });
        let state = self.state.clone();
        Box::pin(async move {
            ready?;
            let api = match RepoApi::resolve(&state, &effect.invocation.publication.project).await {
                Ok(api) => api,
                // No request was sent: the account or repository is unresolved.
                Err(error) => return Ok(failed(format!("GitHub account unavailable: {error}"))),
            };
            let repository = format!("{}/{}", api.owner, api.repo);
            execute(&api, &repository, &effect).await
        })
    }
    fn reconcile(&self, effect: NeutralEffect) -> ObservationFuture {
        let state = self.state.clone();
        Box::pin(async move {
            let api = RepoApi::resolve(&state, &effect.invocation.publication.project).await?;
            let repository = format!("{}/{}", api.owner, api.repo);
            let r = request(
                operation(&effect.invocation.descriptor)?,
                &effect.invocation.inputs,
            )?;
            if !repository.eq_ignore_ascii_case(&r.repository) {
                return Err(
                    "GitHub workflow repository differs from the project repository".into(),
                );
            }
            reconcile(&api, &effect).await
        })
    }
}

/// The review owns the PR resource while it writes; the snapshot only reads.
pub(super) fn prepare(invocation: &NeutralInvocation) -> Result<EffectPreparation, String> {
    let op = operation(&invocation.descriptor)?;
    let r = request(op, &invocation.inputs)?;
    Ok(EffectPreparation {
        mutation: op.mutation(),
        resources: if op.mutation() {
            BTreeSet::from([format!("github:{}/pull_request/{}", r.repository, r.number)])
        } else {
            BTreeSet::new()
        },
        // The graph decides when an operator answers; a review needs no second gate.
        approval: None,
        model_tools: None,
    })
}
