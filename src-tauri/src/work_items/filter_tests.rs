use super::{demo::demo_provider, model::*, protocol::*};
fn query(limit: u32) -> WorkItemQuery {
    WorkItemQuery {
        limit,
        ..Default::default()
    }
}
fn reference(id: &str) -> WorkItemRef {
    WorkItemRef {
        provider_id: "demo".into(),
        account_id: "local".into(),
        id: id.into(),
    }
}

// Catches: label/user filters compare display names or require an exact complete array.
#[test]
fn multi_value_field_edits_update_header_and_filter_by_stable_ids() {
    let mut provider = demo_provider();
    let label = WorkItemOption {
        id: "bug".into(),
        name: "Bug".into(),
    };
    provider
        .act(
            Some(&reference("1")),
            WorkItemAction::EditField(WorkItemEdit {
                field_id: "labels".into(),
                value: Some(WorkItemFieldValue::Labels(vec![
                    label,
                    WorkItemOption {
                        id: "ui".into(),
                        name: "UI".into(),
                    },
                ])),
            }),
        )
        .unwrap();
    assert_eq!(
        provider.detail(&reference("1")).unwrap().item.labels.len(),
        2
    );
    let filter = WorkItemFilter {
        field_id: "labels".into(),
        values: vec![WorkItemFieldValue::Labels(vec![WorkItemOption {
            id: "bug".into(),
            name: "Renamed label".into(),
        }])],
    };
    let page = provider
        .list(&WorkItemQuery {
            filters: vec![filter],
            ..query(50)
        })
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].key, "DEMO-1");
    provider
        .act(
            Some(&reference("1")),
            WorkItemAction::EditField(WorkItemEdit {
                field_id: "labels".into(),
                value: None,
            }),
        )
        .unwrap();
    assert_eq!(
        provider.detail(&reference("1")).unwrap().item.labels,
        [] as [crate::work_items::model::WorkItemOption; 0]
    );
}

// Catches: board/group projection omits cards or grants moves to invalid targets.
#[test]
fn board_and_grouped_pages_project_authoritative_lanes() {
    let mut provider = demo_provider();
    let board = provider.board("priority", &query(50)).unwrap();
    assert_eq!(
        board.lanes[0]
            .cards
            .iter()
            .map(|c| c.item.key.as_str())
            .collect::<Vec<_>>(),
        ["DEMO-1", "DEMO-3"]
    );
    assert_eq!(board.lanes[1].cards[0].item.key, "DEMO-2");
    assert_eq!(
        board.lanes[0].cards[0]
            .moves
            .iter()
            .map(|m| m.column_id.as_str())
            .collect::<Vec<_>>(),
        ["high", "low"]
    );
    provider.boards[0].columns[1].action = Some(WorkItemAction::EditField(WorkItemEdit {
        field_id: "priority".into(),
        value: Some(WorkItemFieldValue::Enum("missing".into())),
    }));
    assert_eq!(
        provider.board("priority", &query(50)).unwrap().lanes[0].cards[0]
            .moves
            .iter()
            .map(|m| m.column_id.as_str())
            .collect::<Vec<_>>(),
        ["high"]
    );
    let groups = provider
        .grouped(&WorkItemQuery {
            group_by: Some("priority".into()),
            ..query(50)
        })
        .unwrap();
    assert_eq!(groups.groups[0].name, "High");
    assert_eq!(groups.groups[0].items.len(), 2);
    assert_eq!(groups.groups[1].items[0].key, "DEMO-2");
    assert!(provider.grouped(&query(50)).is_err());
}
