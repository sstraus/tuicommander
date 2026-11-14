//! Descriptors name native operations; only the installed host grants admission.
use super::catalog::{
    ActivityDescriptor, DescriptorId, FieldSchema, HandlerRef, ObjectSchema, SubjectSchema,
    ValueType, WorkflowCatalog,
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const OPERATIONS: &[(&str, &str)] = &[
    (
        "operator.wait",
        "Retain an exact typed operator decision and fixed continuation",
    ),
    (
        "operator.automatic_close",
        "Close through native story authority after independent current proof",
    ),
    (
        "review.bounded",
        "Review an exact native artifact under an immutable run-wide reviewer admission budget",
    ),
    (
        "agent.context.read",
        "Read bounded source-bound plan, story, member and artifact material without tool authority",
    ),
    (
        "agent.report.read",
        "Resolve a current source-bound role report from its original effect",
    ),
    (
        "story.create",
        "Create owned story proposals with durable review caps and receipts",
    ),
    (
        "issue.triage",
        "Evaluate the exact admitted issue through an installed scoped report owner",
    ),
    (
        "issue.handoff",
        "Dispatch an accepted issue to its retained exact child publication",
    ),
    (
        "issue.graph.receipt",
        "Compose source-bound issue receipts from the exact graph-owned returned story child",
    ),
    (
        "issue.publication",
        "Publish one exact issue-bound artifact with separate stopped disposition evidence",
    ),
    (
        "issue.prepare",
        "Prepare one accepted issue plan and problem-preserving graph story proposal",
    ),
    (
        "issue.read",
        "Read the exact admitted issue and its backend source pin",
    ),
    (
        "issue.effect",
        "Comment or add a label on the exact admitted issue generation",
    ),
    (
        "workspace.pin",
        "Observe the exact native checkout identity",
    ),
    (
        "artifact.pin",
        "Pin committed regular-file artifacts in that checkout",
    ),
    (
        "verification.run",
        "Run one host-pinned native verification definition",
    ),
    (
        "story.cohort.observe",
        "Observe every selected native story and retain owned completion evidence",
    ),
    (
        "workspace.stage",
        "Write and stage listed workspace files for the controlled implementer",
    ),
    (
        "story.read",
        "Read one story of this project: revision, criteria count and status",
    ),
    (
        "story.cohort.created",
        "Return the cohort of stories this run created under one review, in order",
    ),
    (
        "story.batch.pending",
        "Select the next bounded ordered batch or its armed FIX owner with current child revisions",
    ),
    (
        "story.batch.admit",
        "Admit the next ordered cohort member into the single sequential batch slot",
    ),
    (
        "story.batch.land",
        "Credit the slot holder with an owned native push made after its admission",
    ),
    (
        "story.batch.pin",
        "Pin the landed unbatched members at the clean artifact that contains them",
    ),
    (
        "story.batch.verdict",
        "Retain the single critic verdict of the latest batch and arm a FIX re-entry",
    ),
    (
        "story.transition",
        "Apply a native revision-checked story transition",
    ),
    (
        "review.change",
        "Review the exact staged artifact through the native reviewer",
    ),
    (
        "commit.create",
        "Commit the exact approved index through the native Git service",
    ),
    (
        "publication.github",
        "Publish exact approved artifacts through native account-bound GitHub actions",
    ),
    (
        "publication.push",
        "Push the exact approved ref to a local bare repository through the native Git service",
    ),
];

fn object(name: &str, kind: ValueType) -> ObjectSchema {
    ObjectSchema {
        fields: BTreeMap::from([(
            name.into(),
            FieldSchema {
                value_type: kind,
                required: true,
                default: None,
            },
        )]),
    }
}

pub(crate) fn native_operation_catalog() -> Result<WorkflowCatalog, String> {
    let mut catalog = WorkflowCatalog::new();
    for (name, purpose) in OPERATIONS {
        catalog.register(ActivityDescriptor {
            id: DescriptorId {
                namespace: "native".into(),
                name: (*name).into(),
                version: 1,
            },
            purpose: (*purpose).into(),
            handler: HandlerRef {
                name: format!("native.{name}"),
                version: 1,
            },
            target: None,
            config: if *name == "review.bounded" {
                object("policy", ValueType::Object)
            } else {
                ObjectSchema::default()
            },
            inputs: object("request", ValueType::Object),
            subject: Some(SubjectSchema {
                required: true,
                fields: object("workspace", ValueType::WorkspaceReference),
            }),
            outputs: object("receipt", ValueType::Object),
            outcomes: if *name == "issue.publication" {
                BTreeSet::from(["published".into(), "refused".into(), "uncertain".into()])
            } else if name.starts_with("operator.") {
                BTreeSet::from(["resumed".into(), "declined".into()])
            } else if *name == "review.bounded" {
                BTreeSet::from(["ok".into(), "exhausted".into()])
            } else if *name == "story.batch.verdict" {
                BTreeSet::from(["completed".into(), "continue".into(), "fix".into()])
            } else if *name == "story.cohort.observe" {
                BTreeSet::from(["completed".into(), "blocked".into()])
            } else {
                BTreeSet::from(["ok".into()])
            },
            required_capabilities: BTreeSet::new(),
            execution_policies: BTreeSet::from(["native".into()]),
        })?;
    }
    Ok(catalog)
}
