//! Plugin egress equivalence table (plan Step 5.1). Every row pins one trust
//! decision of the plugin HTTP perimeter. The table runs against an `Egress`
//! function; today that is `plugin_http_fetch_impl`, and Phase 5 binds the same
//! table to the gateway path without editing a row.
//!
//! Receivers are loopback servers and accounts are fake, kept in the isolated
//! test config. No row reads a real keychain.
use crate::accounts::AuthKind;
use crate::plugin_http::HttpResponse;
use crate::plugin_http::account_tests::{
    Seen, account, descriptor, isolated_config, load, server, write_manifest,
};
use crate::plugin_http::destination_tests::set_owner_destinations;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

const PLUGIN: &str = "egress-plugin";
const PROVIDER: &str = "egress-provider";
const TOKEN: &str = "egress-secret-token";

/// What the perimeter receives from a plugin.
struct EgressRequest {
    plugin: &'static str,
    url: String,
    headers: Option<HashMap<String, String>>,
    account: Option<String>,
}

type EgressFuture<'a> = Pin<Box<dyn Future<Output = Result<HttpResponse, String>> + 'a>>;

/// The egress under test: today `plugin_http_fetch_impl`, later the gateway.
type Egress = for<'a> fn(&'a Arc<crate::AppState>, EgressRequest) -> EgressFuture<'a>;

fn plugin_http_egress(state: &Arc<crate::AppState>, request: EgressRequest) -> EgressFuture<'_> {
    Box::pin(crate::plugin_http::plugin_http_fetch_impl(
        state,
        request.url,
        None,
        request.headers,
        None,
        request.account,
        request.plugin.into(),
    ))
}

/// A destination a row names. The three receivers answer on loopback; `Site`
/// serves under `/site` and redirects `/redirect` to `Target`.
#[derive(Clone, Copy)]
enum Dest {
    Site,
    /// The site origin, outside the account's `/site` API base.
    SiteRoot,
    Target,
    Other,
    /// A fixed URL prefix with no receiver behind it.
    Raw(&'static str),
}

struct World {
    state: Arc<crate::AppState>,
    site: String,
    target: String,
    other: String,
    seen: [Seen; 3],
}

impl World {
    fn origin(&self, dest: Dest) -> &str {
        match dest {
            Dest::Site | Dest::SiteRoot => &self.site,
            Dest::Target => &self.target,
            Dest::Other => &self.other,
            Dest::Raw(raw) => raw,
        }
    }

    /// The manifest `allowedUrls` or owner pattern that admits a destination.
    fn pattern(&self, dest: Dest) -> String {
        match dest {
            Dest::Raw(raw) => format!("{raw}*"),
            _ => format!("{}/*", self.origin(dest)),
        }
    }

    fn url(&self, dest: Dest, path: &str) -> String {
        let prefix = if matches!(dest, Dest::Site) {
            "/site"
        } else {
            ""
        };
        format!("{}{prefix}{path}", self.origin(dest))
    }

    fn hits(&self) -> [usize; 3] {
        self.seen.each_ref().map(|seen| seen.lock().unwrap().len())
    }
}

#[derive(Clone, Copy)]
enum Capability {
    Granted,
    /// Registered with another capability only.
    Missing,
    Unregistered,
}

#[derive(Clone, Copy)]
struct Row {
    name: &'static str,
    capability: Capability,
    allow: &'static [Dest],
    owner: &'static [Dest],
    /// The account kind to attach; the account is bound to `Dest::Site`.
    account: Option<AuthKind>,
    path_dest: Dest,
    path: &'static str,
    headers: &'static [(&'static str, &'static str)],
    /// `Err(fragment)` must refuse with that fragment; `Ok(())` must answer 200.
    expect: Result<(), &'static str>,
    /// Requests that reach `[Site, Target, Other]` after setup.
    hits: [usize; 3],
}

const ROW: Row = Row {
    name: "",
    capability: Capability::Granted,
    allow: &[],
    owner: &[],
    account: None,
    path_dest: Dest::Site,
    path: "/ok",
    headers: &[],
    expect: Ok(()),
    hits: [0, 0, 0],
};

const BEARER: Option<AuthKind> = Some(AuthKind::Bearer);

const ROWS: &[Row] = &[
    Row {
        name: "capability missing",
        capability: Capability::Missing,
        allow: &[Dest::Site],
        expect: Err("did not declare it"),
        ..ROW
    },
    Row {
        name: "plugin not registered",
        capability: Capability::Unregistered,
        allow: &[Dest::Site],
        expect: Err("is not registered"),
        ..ROW
    },
    Row {
        name: "allowed url admits the request",
        allow: &[Dest::Site],
        hits: [1, 0, 0],
        ..ROW
    },
    Row {
        name: "url outside allowed_urls",
        allow: &[Dest::Site],
        path_dest: Dest::Other,
        expect: Err("does not match any allowed URL pattern"),
        ..ROW
    },
    Row {
        name: "private IPv4 literal needs an explicit declaration",
        allow: &[Dest::Raw("http://")],
        path_dest: Dest::Raw("http://192.168.1.20"),
        expect: Err("Private network URLs require explicit allowedUrls"),
        ..ROW
    },
    Row {
        name: "loopback literal needs an explicit declaration",
        allow: &[Dest::Raw("http://")],
        path_dest: Dest::Other,
        expect: Err("Localhost URLs require explicit allowedUrls"),
        ..ROW
    },
    Row {
        name: "non-http scheme refused",
        allow: &[Dest::Raw("file://")],
        path_dest: Dest::Raw("file://"),
        path: "/etc/passwd",
        expect: Err("is not allowed; use http or https"),
        ..ROW
    },
    Row {
        name: "owner destination admits a url the manifest does not",
        allow: &[Dest::Raw("https://unrelated.example.test/")],
        owner: &[Dest::Other],
        path_dest: Dest::Other,
        hits: [0, 0, 1],
        ..ROW
    },
    Row {
        name: "owner destination does not widen an account request",
        allow: &[Dest::Site],
        owner: &[Dest::Other],
        account: BEARER,
        path_dest: Dest::Other,
        expect: Err("outside the account host"),
        ..ROW
    },
    Row {
        name: "account host mismatch",
        allow: &[Dest::Site, Dest::Other],
        account: BEARER,
        path_dest: Dest::Other,
        expect: Err("outside the account host"),
        ..ROW
    },
    Row {
        name: "account sibling path on the same origin",
        allow: &[Dest::Site],
        account: BEARER,
        path_dest: Dest::SiteRoot,
        path: "/sibling",
        expect: Err("outside the account host"),
        ..ROW
    },
    Row {
        name: "account request carries the injected credential",
        allow: &[Dest::Site],
        account: BEARER,
        path: "/ok",
        hits: [1, 0, 0],
        ..ROW
    },
    Row {
        name: "caller Authorization on an account request",
        allow: &[Dest::Site],
        account: BEARER,
        headers: &[("Authorization", "Bearer caller")],
        expect: Err("must not set Authorization"),
        ..ROW
    },
    Row {
        name: "redirect to a disallowed host",
        allow: &[Dest::Site],
        path: "/redirect",
        expect: Err("Localhost URLs require explicit allowedUrls"),
        hits: [1, 0, 0],
        ..ROW
    },
    Row {
        name: "account redirect to a disallowed host",
        allow: &[Dest::Site],
        account: BEARER,
        path: "/redirect",
        expect: Err("Localhost URLs require explicit allowedUrls"),
        hits: [1, 0, 0],
        ..ROW
    },
    Row {
        name: "cross-host redirect drops the credential",
        allow: &[Dest::Site, Dest::Target],
        account: BEARER,
        path: "/redirect",
        hits: [1, 1, 0],
        ..ROW
    },
    Row {
        name: "cross-host redirect to an owner destination drops the credential",
        allow: &[Dest::Site],
        owner: &[Dest::Target],
        account: BEARER,
        path: "/redirect",
        hits: [1, 1, 0],
        ..ROW
    },
    Row {
        name: "bearer token redacted from headers and body",
        allow: &[Dest::Site],
        account: BEARER,
        path: "/echo",
        hits: [1, 0, 0],
        ..ROW
    },
    Row {
        name: "basic credential redacted from headers and body",
        allow: &[Dest::Site],
        account: Some(AuthKind::Basic),
        path: "/echo",
        hits: [1, 0, 0],
        ..ROW
    },
];

async fn world(row: &Row, dir: &std::path::Path) -> World {
    let (target, target_seen) = server("", None).await;
    // `localhost` is not `127.0.0.1`: a hop to it changes the host.
    let target = target.replace("127.0.0.1", "localhost");
    let (site, site_seen) = server("/site", Some(format!("{target}/capture"))).await;
    let site = site.trim_end_matches("/site").to_string();
    let (other, other_seen) = server("", None).await;
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let world = World {
        state,
        site,
        target,
        other,
        seen: [site_seen, target_seen, other_seen],
    };
    let allowed: Vec<String> = row.allow.iter().map(|d| world.pattern(*d)).collect();
    write_manifest(dir, PLUGIN, descriptor(PROVIDER), &allowed);
    match row.capability {
        Capability::Granted => load(&world.state, PLUGIN),
        Capability::Missing => {
            world
                .state
                .loaded_plugins
                .insert(PLUGIN.into(), vec!["fs:read".into()]);
        }
        Capability::Unregistered => {}
    }
    let owner: Vec<String> = row.owner.iter().map(|d| world.pattern(*d)).collect();
    set_owner_destinations(&world.state, &owner);
    world
}

/// Run one row; the invariants hold for every row, allow or deny.
async fn run_row(egress: Egress, row: &Row) {
    let (dir, _guard) = isolated_config();
    let world = world(row, dir.path()).await;
    let account_id = match row.account {
        Some(kind) => {
            let base = world.url(Dest::Site, "");
            Some(account(PROVIDER, &base, kind, TOKEN).await)
        }
        None => None,
    };
    let before = world.hits();
    let request = EgressRequest {
        plugin: PLUGIN,
        url: world.url(row.path_dest, row.path),
        headers: (!row.headers.is_empty()).then(|| {
            row.headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        }),
        account: account_id,
    };
    let name = row.name;

    let result = egress(&world.state, request).await;

    match (row.expect, &result) {
        (Ok(()), Ok(reply)) => assert_eq!(reply.status, 200, "{name}: {reply:?}"),
        (Err(fragment), Err(err)) => assert!(err.contains(fragment), "{name}: {err}"),
        (expected, got) => panic!("{name}: expected {expected:?}, got {got:?}"),
    }
    let after = world.hits();
    let delta: Vec<usize> = (0..3).map(|i| after[i] - before[i]).collect();
    assert_eq!(
        delta, row.hits,
        "{name}: requests reaching [site, target, other]"
    );
    // No receiver other than the account site may ever see a credential, and the
    // site sees only the injected one.
    for (index, seen) in world.seen.iter().enumerate() {
        for (path, auth) in seen.lock().unwrap().iter().skip(before[index]) {
            let expected = (index == 0).then(|| format!("Bearer {TOKEN}"));
            if row.account == Some(AuthKind::Basic) && index == 0 {
                assert!(
                    auth.as_deref().is_some_and(|a| a.starts_with("Basic ")),
                    "{name}: {path}"
                );
            } else {
                assert_eq!(
                    auth,
                    &expected.filter(|_| row.account.is_some()),
                    "{name}: {path}"
                );
            }
        }
    }
    if let (Some(kind), Ok(reply)) = (row.account, &result) {
        use base64::Engine as _;
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(format!("user@example.test:{TOKEN}"));
        let wire = format!("{} {:?}", reply.body, reply.headers);
        assert!(
            !wire.contains(TOKEN),
            "{name}: token reached the plugin: {wire}"
        );
        assert!(
            !wire.contains(&encoded),
            "{name}: credential reached the plugin: {wire}"
        );
        if row.path == "/echo" {
            assert!(
                wire.contains("[redacted]"),
                "{name}: nothing was redacted ({kind:?}): {wire}"
            );
        }
    }
}

async fn run_table(egress: Egress) {
    for row in ROWS {
        run_row(egress, row).await;
    }
}

// Catches: a trust decision of the plugin HTTP perimeter that changes when the
// admission moves, or a row the current implementation does not honour.
#[tokio::test]
async fn plugin_http_fetch_meets_the_egress_table() {
    run_table(plugin_http_egress).await;
}
