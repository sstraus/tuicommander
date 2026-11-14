use super::neutral_effects::{EffectCommand, EffectPreparation, EffectQuestion, NeutralEffect};
use super::neutral_execution::NeutralExecution;
use super::neutral_runtime::{NeutralHost, NeutralInvocation};
use super::neutral_telegram::{TelegramHost, telegram_host};
use super::neutral_telegram_contract::ActionReference;
use super::runtime::RuntimeOwner;
use crate::workflows::{
    WorkflowStore,
    catalog::*,
    data_binding::DataSource,
    neutral_control::{ControlGraph, ControlNode},
};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(super) struct Fixture {
    pub dir: tempfile::TempDir,
    pub owner: Arc<RuntimeOwner>,
    pub target: NeutralEffect,
    pub notification: NeutralEffect,
    pub action: ActionReference,
    pub host: TelegramHost,
}

pub(super) fn invocation(run: &NeutralExecution) -> NeutralInvocation {
    let (activity, attempt) = run.pending_invocations()[0];
    let descriptor = run.descriptor(activity).unwrap().clone();
    let mut instance = run.publication.definition.instances[activity].clone();
    instance.config = descriptor
        .config
        .resolve(&instance.config, "config")
        .unwrap();
    NeutralInvocation {
        fence: attempt.fence.clone(),
        publication: run.publication.clone(),
        descriptor,
        instance,
        inputs: attempt.inputs.clone(),
        subject: attempt.subject.clone(),
    }
}

pub(super) fn fixture() -> Fixture {
    let (dir, owner, run, _) = super::neutral_execution_tests::fixture(30, false);
    let (run, _) = owner.drive_neutral_turn(&run.run.id).unwrap();
    let target = NeutralEffect::prepare(
        invocation(&run),
        EffectPreparation {
            mutation: true,
            resources: BTreeSet::from(["file:exact".into()]),
            model_tools: None,
            approval: Some(EffectQuestion {
                revision: 1,
                title: "Write exact value?".into(),
                deadline_ms: run.deadline_ms,
            }),
        },
    )
    .unwrap();
    owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Prepare {
                effect: Box::new(target.clone()),
            },
        )
        .unwrap();
    let action = ActionReference {
        effect: target.key.clone(),
        revision: 1,
        approved: true,
        label: "Approve".into(),
    };
    let host = telegram_host("test-bot".into()).unwrap();
    let descriptor = host
        .descriptors
        .values()
        .find(|d| d.id.name == "send")
        .unwrap()
        .clone();
    let mut catalog = WorkflowCatalog::new();
    catalog.register(descriptor.clone()).unwrap();
    let mut definition = run.publication.definition.clone();
    definition.control = ControlGraph {
        nodes: BTreeMap::from([
            ("start".into(), ControlNode::Start),
            ("send".into(), ControlNode::Activity { activity: 0 }),
            (
                "end".into(),
                ControlNode::End {
                    outcome: "done".into(),
                },
            ),
        ]),
        edges: BTreeMap::from([
            (
                "start".into(),
                BTreeMap::from([("next".into(), "send".into())]),
            ),
            (
                "send".into(),
                BTreeMap::from([("complete".into(), "end".into())]),
            ),
        ]),
    };
    definition.instances = vec![ActivityInstance {
        descriptor: descriptor.id.clone(),
        config: BTreeMap::new(),
        execution_policy: "local".into(),
    }];
    definition.conditions.clear();
    definition.outputs = descriptor.outputs.clone();
    definition.outcomes = BTreeSet::from(["done".into()]);
    definition.input_bindings = BTreeMap::from([(
        0,
        BTreeMap::from([
            (
                "chat".into(),
                DataSource::Constant {
                    value: json!(1111111),
                },
            ),
            (
                "text".into(),
                DataSource::Constant {
                    value: json!("Write exact value?"),
                },
            ),
            (
                "actions".into(),
                DataSource::Constant {
                    value: json!([action]),
                },
            ),
        ]),
    )]);
    definition.output_bindings = BTreeMap::from([(
        "notification".into(),
        DataSource::ActivityOutput {
            activity: 0,
            field: "notification".into(),
        },
    )]);
    let workflows = WorkflowStore::open_at(&dir.path().join("definitions.sqlite3")).unwrap();
    let handlers = BTreeSet::from([descriptor.handler.clone()]);
    let publication = workflows
        .save_neutral_publication("/project", "notify", 0, definition, &catalog, &handlers)
        .unwrap();
    let mut start = run.run.request.clone();
    start.request_id = "notify".into();
    start.publication.id = publication.id;
    start.publication.revision = publication.revision;
    let notification_run = owner
        .start_neutral(&workflows, &start, &catalog, &handlers)
        .unwrap();
    let (notification_run, _) = owner.drive_neutral_turn(&notification_run.id).unwrap();
    let invocation = invocation(&notification_run);
    let notification =
        NeutralEffect::prepare(invocation.clone(), host.prepare(&invocation).unwrap()).unwrap();
    owner
        .neutral_effect_command(
            &notification.key.fence.run,
            EffectCommand::Prepare {
                effect: Box::new(notification.clone()),
            },
        )
        .unwrap();
    let snapshot = owner
        .neutral_effect_command(
            &notification.key.fence.run,
            EffectCommand::Dispatch {
                key: notification.key.clone(),
            },
        )
        .unwrap();
    let notification = snapshot.effect(&notification.key).unwrap().clone();
    Fixture {
        dir,
        owner,
        target,
        notification,
        action,
        host,
    }
}

pub(super) fn receipt(f: &Fixture) {
    f.owner.neutral_effect_command(&f.notification.key.fence.run, EffectCommand::Observe {
        key: f.notification.key.clone(), observation: super::neutral_http::completed(BTreeMap::from([("notification".into(), json!({"effect": f.notification.key, "message": {"bot": "test-bot", "chat": 1111111, "message": 17}}))])),
    }).unwrap();
}
