use std::time::Duration;

use http::Method;
use url::Url;

use crate::model::{AccountKey, Effect, Provider};
use crate::routes::{Route, TtlClass, classify};

use Effect::{Read, Write};
use TtlClass::{Identity, Item, Meta, Search, Volatile};

const GH: &str = "https://api.github.com";
const GHES: &str = "https://ghe.example.com/api/v3";
const GL: &str = "https://gitlab.example.com/api/v4";
const JC: &str = "https://acme.atlassian.net";
const JDC: &str = "https://jira.example.com/jira";

fn run(provider: Provider, base: &str, method: &Method, path: &str) -> Route {
    let account = AccountKey {
        account_id: "acct".into(),
        api_base: Url::parse(base).unwrap(),
    };
    let url = Url::parse(base).unwrap().join(path).unwrap();
    classify(provider, &account, method, &url)
}

type Row = (
    Method,
    &'static str,
    &'static str,
    &'static str,
    TtlClass,
    &'static [&'static str],
    Effect,
);

fn check(provider: Provider, base: &str, rows: &[Row]) {
    for (method, path, template, bucket, ttl, tags, effect) in rows {
        let r = run(provider, base, method, path);
        let got_tags: Vec<&str> = r.tags.iter().map(|t| t.as_str()).collect();
        assert_eq!(
            (r.template, r.bucket.0.as_str(), r.ttl, got_tags, r.effect),
            (*template, *bucket, *ttl, tags.to_vec(), *effect),
            "{provider:?} {method} {path}"
        );
    }
}

const GET: Method = Method::GET;
const POST: Method = Method::POST;
const PUT: Method = Method::PUT;
const PATCH: Method = Method::PATCH;

const ACME_PR: &[&str] = &[
    "pr:acct/acme/widget#7",
    "issue:acct/acme/widget#7",
    "repo:acct/acme/widget",
];
const ACME_ISSUE: &[&str] = &[
    "issue:acct/acme/widget#42",
    "pr:acct/acme/widget#42",
    "repo:acct/acme/widget",
];
const MR5: &[&str] = &["gitlab:acct/grp/proj!5", "gitlab:acct/grp/proj"];
const ISSUE5: &[&str] = &["gitlab:acct/grp/proj#5", "gitlab:acct/grp/proj"];
const REPO: &[&str] = &["repo:acct/acme/widget"];
const PROJ: &[&str] = &["gitlab:acct/grp/proj"];
const ABC12: &[&str] = &["jira:acct/ABC-12", "jira:acct/ABC"];

#[test]
fn github_endpoints_map_to_their_routes() {
    const I: &str = "/repos/{owner}/{repo}/issues/{number}";
    const P: &str = "/repos/{owner}/{repo}/pulls/{number}";
    check(
        Provider::GitHub,
        GH,
        &[
            (GET, "/user", "/user", "core", Identity, &[], Read),
            (
                POST,
                "/graphql",
                "/graphql",
                "graphql",
                Volatile,
                &[],
                Write,
            ),
            (
                GET,
                "/search/issues?q=repo:o/r+is:pr",
                "/search/issues",
                "search",
                Search,
                &[],
                Read,
            ),
            (
                GET,
                "/search/code?q=x",
                "/search/code",
                "code_search",
                Search,
                &[],
                Read,
            ),
            // lists, owner/repo lowercased
            (
                GET,
                "/repos/Acme/Widget/issues?state=open",
                "/repos/{owner}/{repo}/issues",
                "core",
                Volatile,
                REPO,
                Read,
            ),
            (
                GET,
                "/repos/acme/widget/pulls?per_page=100",
                "/repos/{owner}/{repo}/pulls",
                "core",
                Volatile,
                REPO,
                Read,
            ),
            (
                POST,
                "/repos/acme/widget/issues",
                "/repos/{owner}/{repo}/issues",
                "core",
                Item,
                REPO,
                Write,
            ),
            (
                POST,
                "/repos/acme/widget/pulls",
                "/repos/{owner}/{repo}/pulls",
                "core",
                Item,
                REPO,
                Write,
            ),
            // issue number space: reads tag the addressed kind, writes touch both (a PR is an issue)
            (
                GET,
                "/repos/acme/widget/issues/42",
                I,
                "core",
                Item,
                &["issue:acct/acme/widget#42"],
                Read,
            ),
            (
                PATCH,
                "/repos/acme/widget/issues/42",
                I,
                "core",
                Item,
                ACME_ISSUE,
                Write,
            ),
            (
                GET,
                "/repos/acme/widget/issues/42/comments?page=2",
                "/repos/{owner}/{repo}/issues/{number}/comments",
                "core",
                Item,
                &["issue:acct/acme/widget#42"],
                Read,
            ),
            (
                POST,
                "/repos/acme/widget/issues/42/comments",
                "/repos/{owner}/{repo}/issues/{number}/comments",
                "core",
                Item,
                ACME_ISSUE,
                Write,
            ),
            // pull number space
            (
                GET,
                "/repos/acme/widget/pulls/7",
                P,
                "core",
                Item,
                &["pr:acct/acme/widget#7"],
                Read,
            ),
            (
                PATCH,
                "/repos/acme/widget/pulls/7",
                P,
                "core",
                Item,
                ACME_PR,
                Write,
            ),
            (
                PUT,
                "/repos/acme/widget/pulls/7/merge",
                "/repos/{owner}/{repo}/pulls/{number}/merge",
                "core",
                Item,
                ACME_PR,
                Write,
            ),
            (
                GET,
                "/repos/acme/widget/pulls/7/reviews?page=1",
                "/repos/{owner}/{repo}/pulls/{number}/reviews",
                "core",
                Item,
                &["pr:acct/acme/widget#7"],
                Read,
            ),
            (
                POST,
                "/repos/acme/widget/pulls/7/reviews",
                "/repos/{owner}/{repo}/pulls/{number}/reviews",
                "core",
                Item,
                ACME_PR,
                Write,
            ),
            (
                PUT,
                "/repos/acme/widget/pulls/7/update-branch",
                "/repos/{owner}/{repo}/pulls/{number}/update-branch",
                "core",
                Item,
                ACME_PR,
                Write,
            ),
            // github_ci_logs.rs
            (
                GET,
                "/repos/acme/widget/actions/runs?head_sha=abc&page=1",
                "/repos/{owner}/{repo}/actions/runs",
                "core",
                Volatile,
                REPO,
                Read,
            ),
            (
                GET,
                "/repos/acme/widget/actions/runs/99/jobs?filter=latest",
                "/repos/{owner}/{repo}/actions/runs/{id}/jobs",
                "core",
                Volatile,
                REPO,
                Read,
            ),
            (
                GET,
                "/repos/acme/widget/actions/jobs/5/logs",
                "/repos/{owner}/{repo}/actions/jobs/{id}/logs",
                "core",
                Item,
                REPO,
                Read,
            ),
            // github-work create metadata
            (
                GET,
                "/repos/acme/widget/labels?page=1",
                "/repos/{owner}/{repo}/labels",
                "core",
                Meta,
                REPO,
                Read,
            ),
            (
                GET,
                "/repos/acme/widget/milestones",
                "/repos/{owner}/{repo}/milestones",
                "core",
                Meta,
                REPO,
                Read,
            ),
            (
                GET,
                "/repos/acme/widget/assignees",
                "/repos/{owner}/{repo}/assignees",
                "core",
                Meta,
                REPO,
                Read,
            ),
        ],
    );
}

#[test]
fn ghes_base_path_is_stripped() {
    check(
        Provider::GitHub,
        GHES,
        &[(
            GET,
            "/api/v3/repos/acme/widget/pulls/7",
            "/repos/{owner}/{repo}/pulls/{number}",
            "core",
            Item,
            &["pr:acct/acme/widget#7"],
            Read,
        )],
    );
}

#[test]
fn gitlab_endpoints_map_to_their_routes() {
    const MR: &str = "/projects/{project}/merge_requests/{iid}";
    const IS: &str = "/projects/{project}/issues/{iid}";
    let d = "default";
    check(
        Provider::GitLab,
        GL,
        &[
            (GET, "/user", "/user", d, Identity, &[], Read),
            (GET, "/issues?scope=all", "/issues", d, Volatile, &[], Read),
            (
                GET,
                "/merge_requests?scope=all",
                "/merge_requests",
                d,
                Volatile,
                &[],
                Read,
            ),
            (
                GET,
                "/todos?state=pending",
                "/todos",
                d,
                Volatile,
                &[],
                Read,
            ),
            (
                GET,
                "/projects/grp%2Fproj/issues?page=1",
                "/projects/{project}/issues",
                d,
                Volatile,
                PROJ,
                Read,
            ),
            (
                GET,
                "/projects/grp%2Fproj/merge_requests",
                "/projects/{project}/merge_requests",
                d,
                Volatile,
                PROJ,
                Read,
            ),
            (
                POST,
                "/projects/grp%2Fproj/issues",
                "/projects/{project}/issues",
                d,
                Item,
                PROJ,
                Write,
            ),
            // issue iid and MR iid are different spaces: `#5` versus `!5`
            (
                GET,
                "/projects/grp%2Fproj/issues/5",
                IS,
                d,
                Item,
                &["gitlab:acct/grp/proj#5"],
                Read,
            ),
            (
                PUT,
                "/projects/grp%2Fproj/issues/5",
                IS,
                d,
                Item,
                ISSUE5,
                Write,
            ),
            (
                GET,
                "/projects/grp%2Fproj/issues/5/discussions?per_page=20",
                "/projects/{project}/issues/{iid}/discussions",
                d,
                Item,
                &["gitlab:acct/grp/proj#5"],
                Read,
            ),
            (
                POST,
                "/projects/grp%2Fproj/issues/5/notes",
                "/projects/{project}/issues/{iid}/notes",
                d,
                Item,
                ISSUE5,
                Write,
            ),
            (
                GET,
                "/projects/grp%2Fproj/merge_requests/5",
                MR,
                d,
                Item,
                &["gitlab:acct/grp/proj!5"],
                Read,
            ),
            (
                PUT,
                "/projects/grp%2Fproj/merge_requests/5/merge",
                "/projects/{project}/merge_requests/{iid}/merge",
                d,
                Item,
                MR5,
                Write,
            ),
            (
                PUT,
                "/projects/grp%2Fproj/merge_requests/5/rebase",
                "/projects/{project}/merge_requests/{iid}/rebase",
                d,
                Item,
                MR5,
                Write,
            ),
            (
                POST,
                "/projects/grp%2Fproj/merge_requests/5/approve",
                "/projects/{project}/merge_requests/{iid}/approve",
                d,
                Item,
                MR5,
                Write,
            ),
            (
                POST,
                "/projects/grp%2Fproj/merge_requests/5/unapprove",
                "/projects/{project}/merge_requests/{iid}/unapprove",
                d,
                Item,
                MR5,
                Write,
            ),
            (
                GET,
                "/projects/grp%2Fproj/merge_requests/5/approvals",
                "/projects/{project}/merge_requests/{iid}/approvals",
                d,
                Item,
                &["gitlab:acct/grp/proj!5"],
                Read,
            ),
            (
                GET,
                "/projects/grp%2Fproj/merge_requests/5/discussions",
                "/projects/{project}/merge_requests/{iid}/discussions",
                d,
                Item,
                &["gitlab:acct/grp/proj!5"],
                Read,
            ),
            (
                POST,
                "/projects/grp%2Fproj/merge_requests/5/notes",
                "/projects/{project}/merge_requests/{iid}/notes",
                d,
                Item,
                MR5,
                Write,
            ),
            (
                GET,
                "/projects/grp%2Fproj/merge_requests/5/pipelines?per_page=1",
                "/projects/{project}/merge_requests/{iid}/pipelines",
                d,
                Volatile,
                PROJ,
                Read,
            ),
            (
                GET,
                "/projects/grp%2Fproj/merge_requests/5/versions/3",
                "/projects/{project}/merge_requests/{iid}/versions/{id}",
                d,
                Item,
                &["gitlab:acct/grp/proj!5"],
                Read,
            ),
            (
                GET,
                "/projects/grp%2Fproj/pipelines/8",
                "/projects/{project}/pipelines/{id}",
                d,
                Volatile,
                PROJ,
                Read,
            ),
            (
                GET,
                "/projects/grp%2Fproj/pipelines/8/jobs?per_page=100",
                "/projects/{project}/pipelines/{id}/jobs",
                d,
                Volatile,
                PROJ,
                Read,
            ),
            (
                POST,
                "/projects/grp%2Fproj/pipelines/8/retry",
                "/projects/{project}/pipelines/{id}/retry",
                d,
                Item,
                PROJ,
                Write,
            ),
            (
                POST,
                "/projects/grp%2Fproj/jobs/4/retry",
                "/projects/{project}/jobs/{id}/retry",
                d,
                Item,
                PROJ,
                Write,
            ),
            (
                GET,
                "/projects/grp%2Fproj/jobs/4/trace",
                "/projects/{project}/jobs/{id}/trace",
                d,
                Item,
                PROJ,
                Read,
            ),
            (
                GET,
                "/projects/42",
                "/projects/{project}",
                d,
                Meta,
                &["gitlab:acct/42"],
                Read,
            ),
            (
                GET,
                "/projects/grp%2Fproj/labels?per_page=100",
                "/projects/{project}/labels",
                d,
                Meta,
                PROJ,
                Read,
            ),
            (
                GET,
                "/projects/grp%2Fproj/members/all",
                "/projects/{project}/members/all",
                d,
                Meta,
                PROJ,
                Read,
            ),
            (
                GET,
                "/projects/grp%2Fproj/boards/3",
                "/projects/{project}/boards/{id}",
                d,
                Meta,
                PROJ,
                Read,
            ),
            (
                GET,
                "/search?scope=issues&search=x",
                "/search",
                "search",
                Search,
                &[],
                Read,
            ),
            (
                GET,
                "/projects/grp%2Fproj/search?scope=blobs",
                "/projects/{project}/search",
                "search",
                Search,
                PROJ,
                Read,
            ),
        ],
    );
}

#[test]
fn jira_endpoints_map_to_their_routes() {
    const ISSUE: &str = "/issue/{key}";
    let key = &["jira:acct/ABC-12"][..];
    check(
        Provider::Jira,
        JC,
        &[
            (
                GET,
                "/rest/api/3/myself",
                "/myself",
                "burst:/myself",
                Identity,
                &[],
                Read,
            ),
            (
                POST,
                "/rest/api/3/search/jql",
                "/search/jql",
                "burst:/search/jql",
                Search,
                &[],
                Read,
            ),
            (
                POST,
                "/rest/api/3/search",
                "/search",
                "burst:/search",
                Search,
                &[],
                Read,
            ),
            (
                POST,
                "/rest/api/3/comment/list?expand=properties",
                "/comment/list",
                "burst:/comment/list",
                Item,
                &[],
                Read,
            ),
            (
                GET,
                "/rest/api/3/issue/ABC-12",
                ISSUE,
                "burst:/issue/{key}",
                Item,
                key,
                Read,
            ),
            (
                PUT,
                "/rest/api/3/issue/ABC-12",
                ISSUE,
                "per-issue:ABC-12",
                Item,
                ABC12,
                Write,
            ),
            (
                POST,
                "/rest/api/3/issue",
                "/issue",
                "burst:/issue",
                Item,
                &[],
                Write,
            ),
            (
                GET,
                "/rest/api/3/issue/ABC-12/comment?startAt=0",
                "/issue/{key}/comment",
                "burst:/issue/{key}/comment",
                Item,
                key,
                Read,
            ),
            (
                POST,
                "/rest/api/3/issue/ABC-12/comment",
                "/issue/{key}/comment",
                "per-issue:ABC-12",
                Item,
                ABC12,
                Write,
            ),
            (
                GET,
                "/rest/api/3/issue/ABC-12/transitions?expand=transitions.fields",
                "/issue/{key}/transitions",
                "burst:/issue/{key}/transitions",
                Item,
                key,
                Read,
            ),
            (
                POST,
                "/rest/api/3/issue/ABC-12/transitions",
                "/issue/{key}/transitions",
                "per-issue:ABC-12",
                Item,
                ABC12,
                Write,
            ),
            (
                GET,
                "/rest/api/3/issue/ABC-12/editmeta",
                "/issue/{key}/editmeta",
                "burst:/issue/{key}/editmeta",
                Meta,
                key,
                Read,
            ),
            (
                GET,
                "/rest/api/3/issue/createmeta/ABC/issuetypes?startAt=0",
                "/issue/createmeta/{project}/issuetypes",
                "burst:/issue/createmeta/{project}/issuetypes",
                Meta,
                &["jira:acct/ABC"],
                Read,
            ),
            (
                GET,
                "/rest/api/3/issue/createmeta/10001/issuetypes/10002",
                "/issue/createmeta/{project}/issuetypes/{id}",
                "burst:/issue/createmeta/{project}/issuetypes/{id}",
                Meta,
                &["jira:acct/10001"],
                Read,
            ),
            (
                GET,
                "/rest/api/3/user/search?query=bo",
                "/user/search",
                "burst:/user/search",
                Search,
                &[],
                Read,
            ),
            (
                GET,
                "/rest/api/3/user/assignable/search?project=ABC",
                "/user/assignable/search",
                "burst:/user/assignable/search",
                Search,
                &[],
                Read,
            ),
            (
                GET,
                "/rest/agile/1.0/board/7/configuration",
                "/board/{id}/configuration",
                "burst:/board/{id}/configuration",
                Meta,
                &[],
                Read,
            ),
        ],
    );
}

#[test]
fn jira_data_center_context_path_and_v2_are_handled() {
    check(
        Provider::Jira,
        JDC,
        &[
            (
                POST,
                "/jira/rest/api/2/search",
                "/search",
                "burst:/search",
                Search,
                &[],
                Read,
            ),
            (
                PUT,
                "/jira/rest/api/2/issue/XY_1-9",
                "/issue/{key}",
                "per-issue:XY_1-9",
                Item,
                &["jira:acct/XY_1-9", "jira:acct/XY_1"],
                Write,
            ),
        ],
    );
}

#[test]
fn jira_search_is_a_read_but_issue_post_is_a_write() {
    let search = run(Provider::Jira, JC, &POST, "/rest/api/3/search/jql");
    let create = run(Provider::Jira, JC, &POST, "/rest/api/3/issue");
    assert_eq!((search.effect, create.effect), (Read, Write));
}

#[test]
fn unknown_routes_fall_back_to_generic() {
    for (provider, base, path) in [
        (Provider::GitHub, GH, "/rate_limit"),
        (Provider::GitHub, GH, "/repos/acme/widget/branches"),
        (Provider::GitHub, GH, "/repos/acme/widget/pulls/7/files"),
        (Provider::GitLab, GL, "/version"),
        (Provider::Jira, JC, "/rest/api/3/serverInfo"),
        (Provider::Generic, "https://example.com", "/user"),
    ] {
        let r = run(provider, base, &GET, path);
        assert_eq!(
            (
                r.template,
                r.bucket.0.as_str(),
                r.ttl,
                r.tags.len(),
                r.effect
            ),
            ("*", "default", Volatile, 0, Read),
            "{path}"
        );
    }
}

#[test]
fn unknown_write_methods_stay_writes() {
    let r = run(
        Provider::GitHub,
        GH,
        &Method::DELETE,
        "/repos/acme/widget/branches/x",
    );
    assert_eq!(r.effect, Write);
}

#[test]
fn malformed_ids_do_not_produce_tags() {
    let r = run(Provider::GitHub, GH, &GET, "/repos/acme/widget/pulls/abc");
    assert_eq!((r.template, r.tags.len()), ("*", 0));
    let r = run(Provider::Jira, JC, &GET, "/rest/api/3/issue/not-a-key");
    assert_eq!((r.template, r.tags.len()), ("*", 0));
}

#[test]
fn ttl_classes_match_the_design_table() {
    assert_eq!(Volatile.base_ttl(), Duration::from_secs(30));
    assert_eq!(Item.base_ttl(), Duration::from_secs(60));
    assert_eq!(Meta.base_ttl(), Duration::from_secs(3600));
    assert_eq!(Identity.base_ttl(), Duration::from_secs(86_400));
    assert_eq!(Search.base_ttl(), Duration::from_secs(60));
}
