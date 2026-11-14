use super::super::catalog::{ActivityDescriptor, ActivityInstance};
use super::super::neutral_publication::{NeutralDefinition, NeutralPublication};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum NeutralAction {
    TriggerSchema {
        source: super::super::trigger::TriggerSource,
        kind: String,
    },
    TriggerBindings {},
    /// Built-in definitions the Designer opens as a new draft.
    Templates {},
    SaveTriggerBinding {
        expected_revision: i64,
        draft: super::super::trigger::TriggerBindingDraft,
    },
    EnableTriggerBinding {
        id: String,
        expected_revision: i64,
        enabled: bool,
        acknowledged_overlaps: std::collections::BTreeMap<String, i64>,
    },
    PreviewTriggerMapping {
        draft: super::super::trigger::TriggerBindingDraft,
        snapshot: super::super::trigger::TriggerEvent,
    },
    ReplayTrigger {
        binding_id: String,
        resource: String,
        generation: String,
        replay_id: String,
    },
    Catalog {},
    CorrectionPolicyRegion {
        region: super::super::correction_policy_region::CorrectionPolicyRegion,
    },
    CorrectionRegion {
        region: super::super::correction_region::CorrectionRegion,
    },
    CreateDraft {
        name: String,
        #[serde(default)]
        editor: EditorMetadata,
        #[serde(deserialize_with = "definition_json")]
        definition: NeutralDefinition,
    },
    ListDrafts {},
    GetDraft {
        id: String,
    },
    UpdateDraft {
        id: String,
        expected_revision: i64,
        name: String,
        #[serde(default)]
        editor: EditorMetadata,
        #[serde(deserialize_with = "definition_json")]
        definition: NeutralDefinition,
    },
    Validate {
        #[serde(deserialize_with = "definition_json")]
        definition: NeutralDefinition,
    },
    Preview {
        #[serde(deserialize_with = "definition_json")]
        definition: NeutralDefinition,
        inputs: std::collections::BTreeMap<String, Value>,
        subject: Option<std::collections::BTreeMap<String, Value>>,
    },
    Publish {
        id: String,
        expected_revision: i64,
    },
    GetPublication {
        id: String,
        revision: i64,
    },
    ClonePublication {
        id: String,
        revision: i64,
        name: String,
    },
    SaveOperation {
        expected_version: u32,
        descriptor: ActivityDescriptor,
    },
    SaveChildDispatch {
        /// Absent: the cohort is a run-time input of stories this run created.
        #[serde(default)]
        cohort: Option<super::super::run::native_cohort::Cohort>,
        expected_version: u32,
        id: super::super::catalog::DescriptorId,
        publication: super::super::neutral_publication::PublicationPin,
    },
    SavePreset {
        id: String,
        expected_revision: i64,
        instance: ActivityInstance,
    },
    GetPreset {
        id: String,
        revision: i64,
    },
    Start {
        request: super::super::run::neutral::NeutralStartRequest,
    },
    /// Run the plan and story delivery workflow for one story; the same key
    /// resumes that run instead of starting another.
    StartDelivery {
        story: String,
        key: String,
    },
    DeliveryRuns {
        story: String,
    },
    Inspect {
        run_id: String,
        after_sequence: i64,
        limit: usize,
    },
    Events {
        run_id: String,
        after_sequence: i64,
        limit: usize,
    },
    Cancel {
        run_id: String,
        command_id: String,
        expected_sequence: i64,
    },
    OperatorAnswer {
        run_id: String,
        command_id: String,
        expected_sequence: i64,
        key: super::super::run::NeutralEffectKey,
        revision: u64,
        answer: super::super::run::native_operator::OperatorAnswer,
    },
    Answer {
        run_id: String,
        command_id: String,
        expected_sequence: i64,
        key: super::super::run::NeutralEffectKey,
        revision: u64,
        approved: bool,
    },
}

// Internally tagged enums buffer maps through serde Content. JSON object keys
// must be decoded as JSON again for activity ordinals in input_bindings.
fn definition_json<'de, D: serde::Deserializer<'de>>(d: D) -> Result<NeutralDefinition, D::Error> {
    serde_json::from_value(Value::deserialize(d)?).map_err(serde::de::Error::custom)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NeutralDraft {
    pub id: String,
    pub project: String,
    pub name: String,
    pub revision: i64,
    pub latest_published_revision: i64,
    pub definition: NeutralDefinition,
    pub editor: EditorMetadata,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct CatalogView {
    pub activities: Vec<ActivityDescriptor>,
    pub metanodes: Vec<PublicationView>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Diagnostic {
    pub path: String,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Validation {
    pub valid: bool,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub(crate) enum NeutralReply {
    CorrectionRegion(CorrectionRegionView),
    Schema(super::super::catalog::ObjectSchema),
    TriggerBindings(Vec<super::super::trigger::NeutralTriggerBinding>),
    Templates(Vec<super::templates::NeutralTemplate>),
    TriggerBinding(super::super::trigger::NeutralTriggerBinding),
    TriggerPreview(super::super::trigger::TriggerPreview),
    TriggerAdmission(Vec<super::super::trigger::TriggerAdmission>),
    Catalog(CatalogView),
    Draft(NeutralDraft),
    Drafts(Vec<NeutralDraft>),
    Validation(Validation),
    Preview(Value),
    Publication(PublicationView),
    Operation(ActivityDescriptor),
    Preset(super::super::activity_preset::ActivityPreset),
    Inspection(Value),
    Events(Value),
    DeliveryRuns(Vec<Value>),
}

/// Designer renders this boundary; publishing still uses the normal draft API.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct CorrectionRegionView {
    pub region: super::super::correction_region::CorrectionRegion,
    pub definition: NeutralDefinition,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditorMetadata {
    #[serde(default)]
    pub labels: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub presets: std::collections::BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct PublicationView {
    #[serde(flatten)]
    pub publication: NeutralPublication,
    pub editor: EditorMetadata,
}
