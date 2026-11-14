use super::*;
use crate::workflows::catalog::*;
use crate::workflows::connector::*;
use crate::workflows::data_reference::DataReference;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub(crate) struct JiraBinding {
    pub descriptor: ActivityDescriptor,
    pub contract: ConnectorContract,
    pub base: String,
    pub plugin: String,
    pub account: String,
    pub project_id: String,
    pub operation: JiraOperation,
}
impl JiraBinding {
    /// Host installation pins account, project and operation; graph data cannot change them.
    pub(crate) fn new(
        base: String,
        plugin: String,
        account: String,
        project_id: String,
        operation: JiraOperation,
    ) -> Result<Self, String> {
        let url = url::Url::parse(&base).map_err(|_| "invalid Jira site")?;
        if !matches!(url.scheme(), "https" | "http")
            || !url.path().ends_with('/')
            || plugin.is_empty()
            || account.is_empty()
            || project_id.is_empty()
        {
            return Err("Jira binding requires site prefix, plugin, account and project".into());
        }
        let contract = ConnectorContract {
            id: format!("jira:{account}:{project_id}"),
            authentication: Some(DataReference::Credential {
                id: account.clone(),
            }),
            resources: schema(
                "project_id",
                ValueType::String,
                Some(serde_json::json!(project_id)),
            ),
            pagination: (operation == JiraOperation::Search).then(|| CursorPagination {
                query_parameter: "nextPageToken".into(),
                items_pointer: "/issues".into(),
                cursor_pointer: "/nextPageToken".into(),
                output: "issues".into(),
                max_pages: 1,
                max_items: 100,
            }),
            rate_limit: RateLimit::FailWithRetryAfter,
            callback: None,
            reconciliation: Reconciliation::ReadOnlyServiceObservation,
        };
        contract.validate()?;
        // Jira uses service-specific read-only observation; the shared HTTP callback
        // strategy is not an assertion that Jira implements that callback protocol.
        let installed = serde_json::json!({"contract":contract,"base":base,"plugin":plugin,"account":account,"project_id":project_id,"operation":operation});
        let descriptor = ActivityDescriptor {
            id: DescriptorId {
                namespace: "jira".into(),
                name: format!("{}.{}.{}", operation.name(), account, project_id),
                version: 1,
            },
            purpose: format!(
                "{} a Jira Cloud issue through the installed account",
                operation.name()
            ),
            handler: HandlerRef {
                name: format!("jira.{}.{}.{}", account, project_id, operation.name()),
                version: 1,
            },
            target: None,
            config: schema("connector", ValueType::Object, Some(installed)),
            inputs: schema("request", ValueType::Object, None),
            subject: None,
            outputs: schema("result", ValueType::Object, None),
            outcomes: BTreeSet::from(["complete".into()]),
            required_capabilities: BTreeSet::from(["jira.network".into()]),
            execution_policies: BTreeSet::from(["local".into()]),
        };
        Ok(Self {
            descriptor,
            contract,
            base,
            plugin,
            account,
            project_id,
            operation,
        })
    }
    pub(crate) fn request(&self, inputs: &BTreeMap<String, Value>) -> Result<JiraRequest, String> {
        self.descriptor.inputs.resolve(inputs, "Jira inputs")?;
        let request: JiraRequest =
            serde_json::from_value(inputs.get("request").ok_or("Jira request absent")?.clone())
                .map_err(|e| format!("invalid Jira request: {e}"))?;
        request.validate(&self.account)?;
        if request.operation() != self.operation {
            return Err("Jira operation differs from installed binding".into());
        }
        Ok(request)
    }
}
fn schema(name: &str, value_type: ValueType, default: Option<Value>) -> ObjectSchema {
    ObjectSchema {
        fields: BTreeMap::from([(
            name.into(),
            FieldSchema {
                value_type,
                required: true,
                default,
            },
        )]),
    }
}
