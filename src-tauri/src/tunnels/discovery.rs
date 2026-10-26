//! SSH host discovery: merges `~/.ssh/config` aliases with `~/.ssh/known_hosts`
//! names into one deduplicated list. Pure functions; file access lives in
//! `commands.rs`.

use std::collections::HashSet;

use serde::Serialize;

const DEFAULT_SSH_PORT: u16 = 22;
/// Listed known_hosts names are capped so a huge file cannot flood the panel.
pub(crate) const MAX_KNOWN_HOSTS: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HostSource {
    Config,
    KnownHosts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct DiscoveredHost {
    /// Name to show and to hand to `ssh`: the config alias, or the known_hosts name.
    pub(crate) host: String,
    /// Resolved machine (`HostName` for an alias, else the name itself). With
    /// `port` it is the entry's identity: two entries sharing a display name
    /// can still be different machines.
    pub(crate) target: String,
    pub(crate) user: Option<String>,
    pub(crate) port: Option<u16>,
    pub(crate) source: HostSource,
}

impl DiscoveredHost {
    /// Port to pass to `ssh -p`. A config alias carries its own port in the
    /// config, which ssh applies by itself.
    pub(crate) fn identity(&self) -> (String, u16) {
        dedupe_key(&self.target, self.port)
    }

    pub(crate) fn probe_port(&self) -> Option<u16> {
        match self.source {
            HostSource::Config => None,
            HostSource::KnownHosts => self.port,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub(crate) struct DiscoveredHosts {
    pub(crate) hosts: Vec<DiscoveredHost>,
    /// known_hosts lines whose name is hashed: present on disk, not listable.
    pub(crate) hashed_count: usize,
}

/// A non-wildcard `Host` alias from `~/.ssh/config` with the parameters that
/// identify the machine behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfigHost {
    pub(crate) alias: String,
    pub(crate) hostname: Option<String>,
    pub(crate) user: Option<String>,
    pub(crate) port: Option<u16>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct KnownHosts {
    pub(crate) hosts: Vec<(String, Option<u16>)>,
    pub(crate) hashed_count: usize,
}

pub(crate) fn is_wildcard(pattern: &str) -> bool {
    pattern.contains(['*', '?'])
}

/// Parse OpenSSH known_hosts text. Hashed names (`|1|…`) are counted, marker
/// lines (`@cert-authority`, `@revoked`) and wildcard/negated patterns are
/// skipped; `[host]:port` and comma-separated names are expanded. Names that
/// could reach `ssh` as an option or carry control characters, and ports that
/// are not 1-65535, are dropped.
pub(crate) fn parse_known_hosts(text: &str) -> KnownHosts {
    let mut known = KnownHosts::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('@') {
            continue;
        }
        let Some(names) = line.split_whitespace().next() else {
            continue;
        };
        if names.starts_with("|1|") {
            known.hashed_count += 1;
            continue;
        }
        for name in names.split(',') {
            if name.starts_with('!') || is_wildcard(name) {
                continue;
            }
            if let Some((host, port)) = split_host_port(name)
                && is_safe_host(&host)
            {
                known.hosts.push((host, port));
            }
        }
    }
    known
}

/// A host that is safe to pass to `ssh` as a destination: non-empty, no
/// leading `-`, and only ASCII letters, digits and `. - _ : %`. That excludes `@` and
/// `/` (ssh would read `user@host` or an `ssh://` URI), whitespace, brackets,
/// and non-ASCII characters (Cyrillic or full-width look-alikes, format and
/// control characters) that fake another name.
pub(crate) fn is_safe_host(host: &str) -> bool {
    !host.is_empty()
        && !host.starts_with('-')
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '%'))
}

/// `None` when the name is malformed (bad port, unbalanced bracket).
fn split_host_port(name: &str) -> Option<(String, Option<u16>)> {
    if let Some(rest) = name.strip_prefix('[') {
        let (host, tail) = rest.split_once(']')?;
        let port = match tail {
            "" => None,
            _ => Some(
                tail.strip_prefix(':')?
                    .parse::<u16>()
                    .ok()
                    .filter(|p| *p != 0)?,
            ),
        };
        return Some((host.to_string(), port));
    }
    Some((name.to_string(), None))
}

fn dedupe_key(host: &str, port: Option<u16>) -> (String, u16) {
    (host.to_ascii_lowercase(), port.unwrap_or(DEFAULT_SSH_PORT))
}

/// Config aliases first, then known_hosts names whose resolved (host, port)
/// no alias and no earlier line already covers.
pub(crate) fn merge_discovered(config: Vec<ConfigHost>, known: KnownHosts) -> DiscoveredHosts {
    let mut seen = HashSet::new();
    let mut hosts = Vec::new();
    for entry in config {
        let target = entry.hostname.unwrap_or_else(|| entry.alias.clone());
        if seen.insert(dedupe_key(&target, entry.port)) {
            hosts.push(DiscoveredHost {
                host: entry.alias,
                target,
                user: entry.user,
                port: entry.port,
                source: HostSource::Config,
            });
        }
    }
    let mut listed_known = 0;
    for (host, port) in known.hosts {
        if listed_known == MAX_KNOWN_HOSTS {
            break;
        }
        if seen.insert(dedupe_key(&host, port)) {
            listed_known += 1;
            hosts.push(DiscoveredHost {
                target: host.clone(),
                host,
                user: None,
                port,
                source: HostSource::KnownHosts,
            });
        }
    }
    DiscoveredHosts {
        hosts,
        hashed_count: known.hashed_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alias(alias: &str, hostname: Option<&str>, port: Option<u16>) -> ConfigHost {
        ConfigHost {
            alias: alias.to_string(),
            hostname: hostname.map(str::to_string),
            user: None,
            port,
        }
    }

    #[test]
    fn known_hosts_hashed_and_marker_lines_are_not_listed_as_hosts() {
        let text = "\
# comment
|1|c2FsdA==|aGFzaA== ssh-ed25519 AAAA
@cert-authority *.corp.example ssh-rsa AAAA
@revoked revoked.example ssh-rsa AAAA
plain.example ssh-ed25519 AAAA
";
        let known = parse_known_hosts(text);
        assert_eq!(known.hosts, vec![("plain.example".to_string(), None)]);
        assert_eq!(known.hashed_count, 1);
    }

    #[test]
    fn known_hosts_expands_ports_comma_names_and_skips_patterns() {
        let text = "\
[host.example]:2222 ssh-ed25519 AAAA
name.example,10.0.0.5 ssh-rsa AAAA
[::1]:2200 ssh-rsa AAAA
*.wild.example ssh-rsa AAAA
ok.example,!neg.example ssh-rsa AAAA
-oProxyCommand=evil ssh-rsa AAAA
";
        let known = parse_known_hosts(text);
        assert_eq!(
            known.hosts,
            vec![
                ("host.example".to_string(), Some(2222)),
                ("name.example".to_string(), None),
                ("10.0.0.5".to_string(), None),
                ("::1".to_string(), Some(2200)),
                ("ok.example".to_string(), None),
            ]
        );
        assert_eq!(known.hashed_count, 0);
    }

    #[test]
    fn a_host_in_config_and_known_hosts_is_listed_once() {
        let config = vec![alias("vps", Some("vps.example"), None)];
        let known = parse_known_hosts(
            "vps.example ssh-ed25519 AAAA\nVPS.example ssh-rsa BBBB\n[vps.example]:2222 ssh-rsa CCCC\n",
        );
        let merged = merge_discovered(config, known);
        let names: Vec<(&str, Option<u16>, HostSource)> = merged
            .hosts
            .iter()
            .map(|h| (h.host.as_str(), h.port, h.source))
            .collect();
        assert_eq!(
            names,
            vec![
                ("vps", None, HostSource::Config),
                ("vps.example", Some(2222), HostSource::KnownHosts),
            ],
            "same host+port must not repeat; a different port is a different machine entry"
        );
    }

    #[test]
    fn config_alias_without_hostname_dedupes_by_alias_itself() {
        let merged = merge_discovered(
            vec![alias("bare.example", None, None)],
            parse_known_hosts("bare.example ssh-ed25519 AAAA\n"),
        );
        assert_eq!(merged.hosts.len(), 1);
        assert_eq!(merged.hosts[0].source, HostSource::Config);
    }

    #[test]
    fn probe_port_is_only_passed_for_known_hosts_entries() {
        let merged = merge_discovered(
            vec![alias("a", Some("a.example"), Some(2200))],
            parse_known_hosts("[b.example]:2222 ssh-rsa AAAA\n"),
        );
        assert_eq!(merged.hosts[0].probe_port(), None);
        assert_eq!(merged.hosts[1].probe_port(), Some(2222));
    }
}

#[cfg(test)]
mod hostile_input_tests {
    use super::*;

    /// Catches: the `-` option guard running on the raw token, so a bracketed
    /// `[-oProxyCommand=…]:22` passes it and later reaches `ssh` as an option.
    #[test]
    fn bracketed_name_starting_with_dash_is_not_listed() {
        let known = parse_known_hosts(
            "[-oProxyCommand=touch$IFS/x]:22 ssh-ed25519 AAAA\n[-oFoo] ssh-rsa AAAA\n",
        );
        assert!(
            known.hosts.iter().all(|(h, _)| !h.starts_with('-')),
            "option-shaped host listed: {:?}",
            known.hosts
        );
    }

    /// Catches: tab separators and CRLF endings leaking `\r` into the host name.
    #[test]
    fn tab_separated_crlf_lines_yield_clean_host_names() {
        let known = parse_known_hosts(
            "a.example\tssh-ed25519\tAAAA\r\n[b.example]:2200\tssh-rsa\tBBBB\r\n",
        );
        assert_eq!(
            known.hosts,
            vec![
                ("a.example".to_string(), None),
                ("b.example".to_string(), Some(2200))
            ]
        );
    }

    /// Catches: an indented marker line being listed as a host.
    #[test]
    fn indented_marker_and_hashed_lines_are_handled() {
        let known = parse_known_hosts(
            "   @revoked gone.example ssh-rsa AAAA\n\t|1|c2FsdA==|aGFzaA== ssh-rsa AAAA\n",
        );
        assert_eq!(
            known.hosts,
            [] as [(std::string::String, std::option::Option<u16>); 0]
        );
        assert_eq!(known.hashed_count, 1);
    }

    /// Catches: `[h]:abc` parsing to port None (listed as 22, wrong dedupe key)
    /// and `[h]:0` or an unbalanced bracket being accepted as a host.
    #[test]
    fn malformed_ports_and_brackets_are_not_listed() {
        let known = parse_known_hosts(
            "[a.example]:abc ssh-rsa AAAA\n[b.example]:0 ssh-rsa AAAA\n[c.example ssh-rsa AAAA\n[d.example]: ssh-rsa AAAA\n[e.example]:99999 ssh-rsa AAAA\n[ok.example]:65535 ssh-rsa AAAA\n",
        );
        assert_eq!(known.hosts, vec![("ok.example".to_string(), Some(65535))]);
    }

    /// Catches: control characters or an embedded bracket inside a name reaching ssh.
    #[test]
    fn control_characters_in_a_name_are_not_listed() {
        let known = parse_known_hosts("bad\x07host ssh-rsa AAAA\nfine.example ssh-rsa AAAA\n");
        assert_eq!(known.hosts, vec![("fine.example".to_string(), None)]);
    }

    /// Catches: a known_hosts flood listing more than the cap, or the cap
    /// counting repeated lines (one per key type) instead of distinct hosts.
    #[test]
    fn listed_known_hosts_are_capped() {
        let text: String = (0..MAX_KNOWN_HOSTS + 50)
            .flat_map(|i| {
                ["ssh-rsa", "ssh-ed25519", "ecdsa-sha2-nistp256"]
                    .map(|kind| format!("h{i}.example {kind} AAAA\n"))
            })
            .collect();
        let merged = merge_discovered(Vec::new(), parse_known_hosts(&text));
        assert_eq!(merged.hosts.len(), MAX_KNOWN_HOSTS);
        assert_eq!(
            merged.hosts[MAX_KNOWN_HOSTS - 1].host,
            format!("h{}.example", MAX_KNOWN_HOSTS - 1)
        );
    }

    /// Catches: a config alias and a known_hosts name that resolve to different
    /// machines sharing the same (host, port) identity, so the UI matches the
    /// probe result of one entry onto the other (statuses carry no source).
    #[test]
    fn listed_entries_have_distinct_host_port_identities() {
        let config = vec![ConfigHost {
            alias: "db".into(),
            hostname: Some("10.0.0.9".into()),
            user: None,
            port: None,
        }];
        let merged = merge_discovered(config, parse_known_hosts("db ssh-ed25519 AAAA\n"));
        let mut ids: Vec<(String, u16)> = merged.hosts.iter().map(|h| h.identity()).collect();
        let total = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), total, "ambiguous entries: {:?}", merged.hosts);
    }
}

#[cfg(test)]
mod critic_r3_tests {
    use super::*;

    /// Catches: a known_hosts name that `ssh` reinterprets as `user@host` or an
    /// `ssh://` URI, so the machine contacted differs from the listed identity.
    #[test]
    fn names_ssh_would_parse_as_user_or_uri_are_not_listed() {
        let known = parse_known_hosts(
            "root@evil.example ssh-rsa AAAA\nssh://evil.example:2222 ssh-rsa AAAA\nfine.example ssh-rsa AAAA\n",
        );
        assert_eq!(known.hosts, vec![("fine.example".to_string(), None)]);
    }

    /// Catches: zero-width and bidi format characters (not `is_control`) letting
    /// two visually identical names coexist, or a reversed name be displayed.
    #[test]
    fn format_characters_in_a_name_are_not_listed() {
        let known = parse_known_hosts(
            "evil.example\u{200b} ssh-rsa AAAA\n\u{202e}moc.live ssh-rsa AAAA\nfine.example ssh-rsa AAAA\n",
        );
        assert_eq!(known.hosts, vec![("fine.example".to_string(), None)]);
    }

    /// Catches: the 2000 cap applied before dedupe, so repeated lines for one
    /// host (one per key type, or after key rotation) push real hosts out.
    #[test]
    fn duplicate_lines_do_not_consume_the_listing_cap() {
        let mut text: String = (0..MAX_KNOWN_HOSTS)
            .map(|_| "dup.example ssh-ed25519 AAAA\n")
            .collect();
        text.push_str("late.example ssh-ed25519 AAAA\n");
        let merged = merge_discovered(Vec::new(), parse_known_hosts(&text));
        let names: Vec<_> = merged.hosts.iter().map(|h| h.host.as_str()).collect();
        assert_eq!(names, vec!["dup.example", "late.example"]);
    }
}

#[cfg(test)]
mod critic_round5_tests {
    use super::*;

    /// Catches: `is_alphanumeric` admitting non-ASCII letters and digits, so a
    /// Cyrillic or full-width look-alike of a trusted name is listed and probed.
    #[test]
    fn non_ascii_look_alike_hosts_are_not_safe() {
        for host in [
            "exаmple.com",
            "ｅxample.com",
            "host٣.example",
            "ex\u{200b}ample.com",
        ] {
            assert!(!is_safe_host(host), "look-alike accepted: {host:?}");
        }
    }

    /// Catches: the allowlist rejecting a legitimate bracketed IPv6 name with a
    /// zone id, which known_hosts writes as `[fe80::1%en0]:2222`.
    #[test]
    fn bracketed_ipv6_with_zone_and_port_is_listed() {
        let known = parse_known_hosts("[fe80::1%en0]:2222 ssh-ed25519 AAAA\n");
        assert_eq!(known.hosts, vec![("fe80::1%en0".to_string(), Some(2222))]);
    }

    /// Catches: user@host and ssh:// forms surviving as one token, which ssh
    /// would split into a user and a different destination.
    #[test]
    fn user_and_uri_forms_are_not_listed() {
        let known = parse_known_hosts(
            "root@victim.example ssh-rsa AAAA\nssh://victim.example ssh-rsa AAAA\nok.example ssh-rsa AAAA\n",
        );
        assert_eq!(known.hosts, vec![("ok.example".to_string(), None)]);
    }

    /// Catches: known_hosts names that duplicate a config alias's target
    /// consuming the known-hosts cap, hiding real extra hosts.
    #[test]
    fn names_covered_by_config_do_not_consume_the_known_cap() {
        let config = vec![ConfigHost {
            alias: "a".to_string(),
            hostname: Some("h0.example".to_string()),
            user: None,
            port: None,
        }];
        let text = (0..=MAX_KNOWN_HOSTS).fold(String::new(), |mut text, i| {
            use std::fmt::Write as _;
            let _ = writeln!(text, "h{i}.example ssh-rsa AAAA");
            text
        });
        let merged = merge_discovered(config, parse_known_hosts(&text));
        let known = merged
            .hosts
            .iter()
            .filter(|h| matches!(h.source, HostSource::KnownHosts))
            .count();
        assert_eq!(known, MAX_KNOWN_HOSTS);
    }
}
