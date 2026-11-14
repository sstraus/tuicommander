//! Closed connector delegation; the installed NativeHost remains the only provider.
use super::native_config::FamilyBinding;
use super::native_operations::NativeContext;
use super::neutral_effects::EffectQuestion;
use super::neutral_github_review::GitHubHost;
use super::neutral_gitlab::{GitLabContext, GitLabHost};
use super::neutral_http::{HttpConnector, HttpContext, HttpHost};
use super::neutral_jira::JiraHost;
use super::neutral_runtime::NeutralHost;
use super::neutral_telegram::TelegramHost;
use crate::agent_capabilities::Capability;
use crate::state::AppState;
use crate::workflows::catalog::{ActivityDescriptor, ReusableTarget};
use crate::workflows::neutral_admission::HostAdmission;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub(super) enum ConnectorAdapter {
    Http(HttpHost),
    Jira(JiraHost),
    GitLab(Box<GitLabHost>),
    Telegram(TelegramHost),
    GitHub(GitHubHost),
}

impl ConnectorAdapter {
    pub fn admission(&self) -> &dyn HostAdmission {
        match self {
            Self::Http(h) => h,
            Self::Jira(h) => h,
            Self::GitLab(h) => h.as_ref(),
            Self::Telegram(h) => h,
            Self::GitHub(h) => h,
        }
    }
    pub fn host(&self) -> &dyn NeutralHost {
        match self {
            Self::Http(h) => h,
            Self::Jira(h) => h,
            Self::GitLab(h) => h.as_ref(),
            Self::Telegram(h) => h,
            Self::GitHub(h) => h,
        }
    }

    /// None is reserved for bounded authoring checks and pure preparation.
    /// Runtime dispatch always supplies its backend-captured caller context.
    pub fn new(
        state: Arc<AppState>,
        descriptor: &ActivityDescriptor,
        binding: Option<FamilyBinding>,
        caller: Option<NativeContext>,
        approval: Option<EffectQuestion>,
    ) -> Result<Self, String> {
        let grant = move || {
            if let Some(caller) = &caller {
                caller.current(Some(Capability::Operator))?;
            }
            Ok::<_, String>(())
        };
        if let Some(ReusableTarget::Http {
            connector,
            plugin,
            base,
        }) = &descriptor.target
        {
            let connector = HttpConnector::new(
                descriptor.clone(),
                connector.clone(),
                plugin.clone(),
                base.clone(),
            )?;
            let id = match &descriptor.target {
                Some(ReusableTarget::Http { connector, .. }) => connector.id.clone(),
                _ => unreachable!(),
            };
            return Ok(Self::Http(HttpHost {
                state,
                approval,
                connectors: BTreeMap::from([(descriptor.handler.clone(), connector)]),
                authority: Arc::new(move |_| {
                    grant()?;
                    Ok(http_grants(&id))
                }),
            }));
        }
        match binding.ok_or("configured connector unavailable")? {
            FamilyBinding::Jira(binding) => {
                let id = binding.contract.id.clone();
                Ok(Self::Jira(JiraHost {
                    state,
                    approval,
                    bindings: BTreeMap::from([(descriptor.handler.clone(), *binding)]),
                    authority: Arc::new(move |_| {
                        grant()?;
                        Ok(http_grants(&id))
                    }),
                }))
            }
            FamilyBinding::GitLab(install) => {
                let id = install.connector.id.clone();
                Ok(Self::GitLab(Box::new(GitLabHost {
                    state,
                    approval,
                    install: *install,
                    authority: Arc::new(move |_| {
                        grant()?;
                        Ok(GitLabContext {
                            reads: BTreeSet::from([id.clone()]),
                            writes: BTreeSet::from([id.clone()]),
                        })
                    }),
                })))
            }
            FamilyBinding::Telegram(bot) => {
                Ok(Self::Telegram(super::neutral_telegram::telegram_host(bot)?))
            }
            FamilyBinding::GitHub => Ok(Self::GitHub(GitHubHost {
                state,
                authority: Arc::new(grant),
            })),
            FamilyBinding::Model | FamilyBinding::Approval => {
                Err("activity is not a connector".into())
            }
        }
    }
}

fn http_grants(id: &str) -> HttpContext {
    HttpContext {
        network: BTreeSet::from([id.into()]),
        writes: BTreeSet::from([id.into()]),
    }
}
