//! Closed route table: every provider URL maps to a template, rate-limit
//! bucket, TTL class, invalidation tags and effect. Anything the table does
//! not list is a `Generic` route (design "Buckets per provider").
//!
//! A GraphQL POST is classified as a `Write` so it is never cached by default;
//! the facade overrides the effect when the request carries a `GraphqlRead`.

use std::time::Duration;

use http::Method;
use percent_encoding::percent_decode_str;
use url::Url;

use crate::model::{AccountKey, BucketId, Effect, Provider};

/// Cache lifetime class of a route (design "Cache").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TtlClass {
    Volatile,
    Item,
    Meta,
    Identity,
    Search,
}

impl TtlClass {
    pub fn base_ttl(self) -> Duration {
        match self {
            Self::Volatile => Duration::from_secs(30),
            Self::Item | Self::Search => Duration::from_secs(60),
            Self::Meta => Duration::from_secs(3600),
            Self::Identity => Duration::from_secs(86_400),
        }
    }
}

/// Opaque invalidation tag, e.g. `pr:acct/o/r#7`. Constructors lowercase
/// GitHub owner/repo and uppercase Jira keys, so a signal adapter and the
/// route table always agree on the spelling.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Tag(String);

impl Tag {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Collection tag of a GitHub repository.
    pub fn repo(account: &str, owner: &str, repo: &str) -> Self {
        Self(format!(
            "repo:{account}/{}/{}",
            owner.to_lowercase(),
            repo.to_lowercase()
        ))
    }

    pub fn issue(account: &str, owner: &str, repo: &str, number: u64) -> Self {
        Self(format!(
            "issue:{account}/{}/{}#{number}",
            owner.to_lowercase(),
            repo.to_lowercase()
        ))
    }

    pub fn pr(account: &str, owner: &str, repo: &str, number: u64) -> Self {
        Self(format!(
            "pr:{account}/{}/{}#{number}",
            owner.to_lowercase(),
            repo.to_lowercase()
        ))
    }

    /// Collection tag of a GitLab project (decoded path or numeric id).
    pub fn gitlab_project(account: &str, project: &str) -> Self {
        Self(format!("gitlab:{account}/{project}"))
    }

    pub fn gitlab_issue(account: &str, project: &str, iid: u64) -> Self {
        Self(format!("gitlab:{account}/{project}#{iid}"))
    }

    /// Merge requests use `!` so an MR iid never collides with an issue iid.
    pub fn gitlab_mr(account: &str, project: &str, iid: u64) -> Self {
        Self(format!("gitlab:{account}/{project}!{iid}"))
    }

    /// A Jira issue key, a project key or a project id.
    pub fn jira(account: &str, key: &str) -> Self {
        Self(format!("jira:{account}/{}", key.to_uppercase()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub template: &'static str,
    pub bucket: BucketId,
    pub ttl: TtlClass,
    pub tags: Vec<Tag>,
    pub effect: Effect,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Verb {
    Get,
    Post,
    Put,
    Patch,
    Other,
}

impl Verb {
    fn of(method: &Method) -> Self {
        match method.as_str() {
            "GET" | "HEAD" => Self::Get,
            "POST" => Self::Post,
            "PUT" => Self::Put,
            "PATCH" => Self::Patch,
            _ => Self::Other,
        }
    }
}

fn route(
    template: &'static str,
    bucket: &str,
    ttl: TtlClass,
    tags: Vec<Tag>,
    effect: Effect,
) -> Route {
    Route {
        template,
        bucket: BucketId(bucket.into()),
        ttl,
        tags,
        effect,
    }
}

/// A read route; its tags are the cached entry's invalidation scope.
fn read(template: &'static str, bucket: &str, ttl: TtlClass, tags: Vec<Tag>) -> Route {
    route(template, bucket, ttl, tags, Effect::Read)
}

/// A write route; its tags are what the write invalidates. A write never caches.
fn write(template: &'static str, bucket: &str, tags: Vec<Tag>) -> Route {
    route(template, bucket, TtlClass::Item, tags, Effect::Write)
}

fn generic(method: &Method) -> Route {
    let effect = if Verb::of(method) == Verb::Get {
        Effect::Read
    } else {
        Effect::Write
    };
    route("*", "default", TtlClass::Volatile, vec![], effect)
}

/// Classify one request. The API base path (GHES `/api/v3`, a Jira context
/// path) is stripped first, so the table sees provider-relative paths.
pub fn classify(provider: Provider, account: &AccountKey, method: &Method, url: &Url) -> Route {
    let base = account.api_base.path().trim_end_matches('/');
    let path = url.path();
    let path = path
        .strip_prefix(base)
        .filter(|rest| rest.is_empty() || rest.starts_with('/'))
        .unwrap_or(path);
    let decoded: Vec<String> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| percent_decode_str(s).decode_utf8_lossy().into_owned())
        .collect();
    let segs: Vec<&str> = decoded.iter().map(String::as_str).collect();
    let (acct, verb) = (account.account_id.as_str(), Verb::of(method));
    let route = match provider {
        Provider::GitHub => github(acct, verb, &segs),
        Provider::GitLab => gitlab(acct, verb, &segs),
        Provider::Jira => jira(acct, verb, &segs),
        Provider::Generic => None,
    };
    route.unwrap_or_else(|| generic(method))
}

/// A positive decimal id; anything else is not an item route.
fn number(s: &str) -> Option<u64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

fn github(acct: &str, verb: Verb, segs: &[&str]) -> Option<Route> {
    use TtlClass::{Identity, Search, Volatile};
    use Verb::{Get, Post};
    let segs = match segs {
        ["api", "v3", rest @ ..] => rest,
        _ => segs,
    };
    Some(match (verb, segs) {
        (Get, ["user"]) => read("/user", "core", Identity, vec![]),
        (Post, ["graphql"]) => write("/graphql", "graphql", vec![]).with_ttl(Volatile),
        (Get, ["search", "code"]) => read("/search/code", "code_search", Search, vec![]),
        (Get, ["search", "issues"]) => read("/search/issues", "search", Search, vec![]),
        (Get, ["search", _]) => read("/search/{kind}", "search", Search, vec![]),
        (_, ["repos", owner, repo, rest @ ..]) => github_repo(acct, verb, owner, repo, rest)?,
        _ => return None,
    })
}

impl Route {
    fn with_ttl(mut self, ttl: TtlClass) -> Self {
        self.ttl = ttl;
        self
    }
}

fn github_repo(acct: &str, verb: Verb, owner: &str, repo: &str, rest: &[&str]) -> Option<Route> {
    use TtlClass::{Item, Meta, Volatile};
    use Verb::{Get, Patch, Post, Put};
    let repo_tag = Tag::repo(acct, owner, repo);
    let issue = |n: &str| number(n).map(|n| Tag::issue(acct, owner, repo, n));
    let pr = |n: &str| number(n).map(|n| Tag::pr(acct, owner, repo, n));
    // A pull request is an issue: a write through either number space also
    // invalidates the other, because reads of `issues/{n}` return PRs too.
    let issue_write = |n: &str| Some(vec![issue(n)?, pr(n)?, repo_tag.clone()]);
    let pr_write = |n: &str| Some(vec![pr(n)?, issue(n)?, repo_tag.clone()]);
    let r = vec![repo_tag.clone()];
    Some(match (verb, rest) {
        (Get, ["issues"]) => read("/repos/{owner}/{repo}/issues", "core", Volatile, r),
        (Post, ["issues"]) => write("/repos/{owner}/{repo}/issues", "core", r),
        (Get, ["pulls"]) => read("/repos/{owner}/{repo}/pulls", "core", Volatile, r),
        (Post, ["pulls"]) => write("/repos/{owner}/{repo}/pulls", "core", r),
        (Get, ["issues", n]) => read(
            "/repos/{owner}/{repo}/issues/{number}",
            "core",
            Item,
            vec![issue(n)?],
        ),
        (Patch, ["issues", n]) => write(
            "/repos/{owner}/{repo}/issues/{number}",
            "core",
            issue_write(n)?,
        ),
        (Get, ["issues", n, "comments"]) => read(
            "/repos/{owner}/{repo}/issues/{number}/comments",
            "core",
            Item,
            vec![issue(n)?],
        ),
        (Post, ["issues", n, "comments"]) => write(
            "/repos/{owner}/{repo}/issues/{number}/comments",
            "core",
            issue_write(n)?,
        ),
        (Get, ["pulls", n]) => read(
            "/repos/{owner}/{repo}/pulls/{number}",
            "core",
            Item,
            vec![pr(n)?],
        ),
        (Patch, ["pulls", n]) => {
            write("/repos/{owner}/{repo}/pulls/{number}", "core", pr_write(n)?)
        }
        (Put, ["pulls", n, "merge"]) => write(
            "/repos/{owner}/{repo}/pulls/{number}/merge",
            "core",
            pr_write(n)?,
        ),
        (Put, ["pulls", n, "update-branch"]) => write(
            "/repos/{owner}/{repo}/pulls/{number}/update-branch",
            "core",
            pr_write(n)?,
        ),
        (Get, ["pulls", n, "reviews"]) => read(
            "/repos/{owner}/{repo}/pulls/{number}/reviews",
            "core",
            Item,
            vec![pr(n)?],
        ),
        (Post, ["pulls", n, "reviews"]) => write(
            "/repos/{owner}/{repo}/pulls/{number}/reviews",
            "core",
            pr_write(n)?,
        ),
        (Get, ["actions", "runs"]) => {
            read("/repos/{owner}/{repo}/actions/runs", "core", Volatile, r)
        }
        (Get, ["actions", "runs", id, "jobs"]) if number(id).is_some() => read(
            "/repos/{owner}/{repo}/actions/runs/{id}/jobs",
            "core",
            Volatile,
            r,
        ),
        (Get, ["actions", "jobs", id, "logs"]) if number(id).is_some() => read(
            "/repos/{owner}/{repo}/actions/jobs/{id}/logs",
            "core",
            Item,
            r,
        ),
        (Get, ["labels"]) => read("/repos/{owner}/{repo}/labels", "core", Meta, r),
        (Get, ["milestones"]) => read("/repos/{owner}/{repo}/milestones", "core", Meta, r),
        (Get, ["assignees"]) => read("/repos/{owner}/{repo}/assignees", "core", Meta, r),
        _ => return None,
    })
}

fn gitlab(acct: &str, verb: Verb, segs: &[&str]) -> Option<Route> {
    use TtlClass::{Identity, Search, Volatile};
    use Verb::Get;
    let segs = match segs {
        ["api", "v4", rest @ ..] => rest,
        _ => segs,
    };
    Some(match (verb, segs) {
        (Get, ["user"]) => read("/user", "default", Identity, vec![]),
        (Get, ["issues"]) => read("/issues", "default", Volatile, vec![]),
        (Get, ["merge_requests"]) => read("/merge_requests", "default", Volatile, vec![]),
        (Get, ["todos"]) => read("/todos", "default", Volatile, vec![]),
        (Get, ["search"]) => read("/search", "search", Search, vec![]),
        (_, ["projects", project, rest @ ..]) => gitlab_project(acct, verb, project, rest)?,
        _ => return None,
    })
}

fn gitlab_project(acct: &str, verb: Verb, project: &str, rest: &[&str]) -> Option<Route> {
    use TtlClass::{Item, Meta, Search, Volatile};
    use Verb::{Get, Post, Put};
    let p = Tag::gitlab_project(acct, project);
    let issue = |n: &str| number(n).map(|n| Tag::gitlab_issue(acct, project, n));
    let mr = |n: &str| number(n).map(|n| Tag::gitlab_mr(acct, project, n));
    let issue_write = |n: &str| Some(vec![issue(n)?, p.clone()]);
    let mr_write = |n: &str| Some(vec![mr(n)?, p.clone()]);
    let d = "default";
    let pv = vec![p.clone()];
    const MR: &str = "/projects/{project}/merge_requests/{iid}";
    Some(match (verb, rest) {
        (Get, []) => read("/projects/{project}", d, Meta, pv),
        (Get, ["issues"]) => read("/projects/{project}/issues", d, Volatile, pv),
        (Post, ["issues"]) => write("/projects/{project}/issues", d, pv),
        (Get, ["merge_requests"]) => read("/projects/{project}/merge_requests", d, Volatile, pv),
        (Get, ["issues", n]) => read("/projects/{project}/issues/{iid}", d, Item, vec![issue(n)?]),
        (Put, ["issues", n]) => write("/projects/{project}/issues/{iid}", d, issue_write(n)?),
        (Get, ["issues", n, "discussions"]) => read(
            "/projects/{project}/issues/{iid}/discussions",
            d,
            Item,
            vec![issue(n)?],
        ),
        (Post, ["issues", n, "notes"]) => {
            write("/projects/{project}/issues/{iid}/notes", d, issue_write(n)?)
        }
        (Get, ["merge_requests", n]) => read(MR, d, Item, vec![mr(n)?]),
        (Get, ["merge_requests", n, "approvals"]) => read(
            "/projects/{project}/merge_requests/{iid}/approvals",
            d,
            Item,
            vec![mr(n)?],
        ),
        (Get, ["merge_requests", n, "discussions"]) => read(
            "/projects/{project}/merge_requests/{iid}/discussions",
            d,
            Item,
            vec![mr(n)?],
        ),
        (Get, ["merge_requests", n, "versions", v]) if number(v).is_some() => read(
            "/projects/{project}/merge_requests/{iid}/versions/{id}",
            d,
            Item,
            vec![mr(n)?],
        ),
        (Get, ["merge_requests", n, "pipelines"]) if number(n).is_some() => read(
            "/projects/{project}/merge_requests/{iid}/pipelines",
            d,
            Volatile,
            pv,
        ),
        (Put, ["merge_requests", n, "merge"]) => write(
            "/projects/{project}/merge_requests/{iid}/merge",
            d,
            mr_write(n)?,
        ),
        (Put, ["merge_requests", n, "rebase"]) => write(
            "/projects/{project}/merge_requests/{iid}/rebase",
            d,
            mr_write(n)?,
        ),
        (Post, ["merge_requests", n, "approve"]) => write(
            "/projects/{project}/merge_requests/{iid}/approve",
            d,
            mr_write(n)?,
        ),
        (Post, ["merge_requests", n, "unapprove"]) => write(
            "/projects/{project}/merge_requests/{iid}/unapprove",
            d,
            mr_write(n)?,
        ),
        (Post, ["merge_requests", n, "notes"]) => write(
            "/projects/{project}/merge_requests/{iid}/notes",
            d,
            mr_write(n)?,
        ),
        (Get, ["pipelines", id]) if number(id).is_some() => {
            read("/projects/{project}/pipelines/{id}", d, Volatile, pv)
        }
        (Get, ["pipelines", id, "jobs"]) if number(id).is_some() => {
            read("/projects/{project}/pipelines/{id}/jobs", d, Volatile, pv)
        }
        (Post, ["pipelines", id, "retry"]) if number(id).is_some() => {
            write("/projects/{project}/pipelines/{id}/retry", d, pv)
        }
        (Post, ["jobs", id, "retry"]) if number(id).is_some() => {
            write("/projects/{project}/jobs/{id}/retry", d, pv)
        }
        (Get, ["jobs", id, "trace"]) if number(id).is_some() => {
            read("/projects/{project}/jobs/{id}/trace", d, Item, pv)
        }
        (Get, ["labels"]) => read("/projects/{project}/labels", d, Meta, pv),
        (Get, ["members", "all"]) => read("/projects/{project}/members/all", d, Meta, pv),
        (Get, ["boards", id]) if number(id).is_some() => {
            read("/projects/{project}/boards/{id}", d, Meta, pv)
        }
        (Get, ["search"]) => read("/projects/{project}/search", "search", Search, pv),
        _ => return None,
    })
}

/// `ABC-12` → (`ABC-12`, `ABC`); the key is normalised to upper case.
fn jira_key(s: &str) -> Option<(String, String)> {
    let (project, n) = s.rsplit_once('-')?;
    let valid_project = project
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
        && project
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_');
    (valid_project && number(n).is_some()).then(|| (s.to_uppercase(), project.to_uppercase()))
}

fn jira(acct: &str, verb: Verb, segs: &[&str]) -> Option<Route> {
    let segs = match segs {
        ["rest", "api", "2" | "3", rest @ ..] | ["rest", "agile", "1.0" | "latest", rest @ ..] => {
            rest
        }
        _ => return None,
    };
    use TtlClass::{Identity, Item, Meta, Search};
    use Verb::{Get, Post, Put};
    // Reads draw on the per-route burst bucket; an issue write on its own per-issue bucket.
    let burst = |template: &'static str, ttl, tags: Vec<Tag>| {
        read(template, &format!("burst:{template}"), ttl, tags)
    };
    Some(match (verb, segs) {
        (Get, ["myself"]) => burst("/myself", Identity, vec![]),
        // Search and bulk comment fetch are POSTs, but they are Reads.
        (Get | Post, ["search", "jql"]) => burst("/search/jql", Search, vec![]),
        (Get | Post, ["search"]) => burst("/search", Search, vec![]),
        (Post, ["comment", "list"]) => burst("/comment/list", Item, vec![]),
        (Get, ["user", "search"]) => burst("/user/search", Search, vec![]),
        (Get, ["user", "assignable", "search"]) => burst("/user/assignable/search", Search, vec![]),
        (Get, ["board", id, "configuration"]) if number(id).is_some() => {
            burst("/board/{id}/configuration", Meta, vec![])
        }
        (Post, ["issue"]) => write("/issue", "burst:/issue", vec![]),
        (Get, ["issue", "createmeta", project, "issuetypes"]) => burst(
            "/issue/createmeta/{project}/issuetypes",
            Meta,
            vec![Tag::jira(acct, project)],
        ),
        (Get, ["issue", "createmeta", project, "issuetypes", id]) if number(id).is_some() => burst(
            "/issue/createmeta/{project}/issuetypes/{id}",
            Meta,
            vec![Tag::jira(acct, project)],
        ),
        (_, ["issue", key, rest @ ..]) => {
            let (key, project) = jira_key(key)?;
            let issue = Tag::jira(acct, &key);
            let writes = || vec![issue.clone(), Tag::jira(acct, &project)];
            let per_issue = format!("per-issue:{key}");
            let w = |template: &'static str| write(template, &per_issue, writes());
            match (verb, rest) {
                (Get, []) => burst("/issue/{key}", Item, vec![issue.clone()]),
                (Put, []) => w("/issue/{key}"),
                (Get, ["comment"]) => burst("/issue/{key}/comment", Item, vec![issue.clone()]),
                (Post, ["comment"]) => w("/issue/{key}/comment"),
                (Get, ["transitions"]) => {
                    burst("/issue/{key}/transitions", Item, vec![issue.clone()])
                }
                (Post, ["transitions"]) => w("/issue/{key}/transitions"),
                (Get, ["editmeta"]) => burst("/issue/{key}/editmeta", Meta, vec![issue.clone()]),
                _ => return None,
            }
        }
        _ => return None,
    })
}
