use super::{Operation, field};
use crate::workflows::catalog::{ObjectSchema, ValueType};
use std::collections::BTreeMap;
impl Operation {
    /// Subject fields stay individually bindable; the full receipt is retained.
    /// Optional fields preserve absence on pending/stale outcomes.
    pub(crate) fn outputs(self) -> ObjectSchema {
        use ValueType::{Integer, String};
        let mut fields = BTreeMap::from([("receipt".into(), field(ValueType::Object))]);
        let mut names = vec![
            ("instance", String),
            ("project", Integer),
            ("effect", String),
        ];
        match self {
            Self::Issue => names.push(("issue", Integer)),
            Self::MergeRequest => {
                names.extend([("mr", Integer), ("head", String), ("base", String)])
            }
            Self::DiffVersion => names.extend([
                ("mr", Integer),
                ("version", Integer),
                ("head", String),
                ("base", String),
                ("start", String),
            ]),
            Self::Pipeline => names.extend([
                ("mr", Integer),
                ("pipeline", Integer),
                ("head", String),
                ("base", String),
                ("status", String),
            ]),
            Self::ReviewNote => names.extend([
                ("mr", Integer),
                ("head", String),
                ("base", String),
                ("note", Integer),
            ]),
        }
        for (name, ty) in names {
            let mut value = field(ty);
            value.required = false;
            fields.insert(name.into(), value);
        }
        ObjectSchema { fields }
    }
}
