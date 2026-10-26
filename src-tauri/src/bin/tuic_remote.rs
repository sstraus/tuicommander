#[cfg(any(not(feature = "desktop"), test))]
struct ParsedArgs {
    instance: Option<String>,
    set_password: bool,
    build_info: bool,
    relay_pair: Option<tuicommander_lib::RelayPairOptions>,
    remote: tuicommander_lib::RemoteOptions,
}

#[cfg(any(not(feature = "desktop"), test))]
fn parse_args<I, S>(args: I) -> anyhow::Result<ParsedArgs>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut instance = None;
    let mut set_password = false;
    let mut build_info = false;
    let mut relay_pair: Option<tuicommander_lib::RelayPairOptions> = None;
    let mut relay_register_seen = false;
    let mut relay_out_seen = false;
    let mut relay_pair_seen = false;
    let mut bind_seen = false;
    let mut survive_seen = false;
    let mut no_agent_configs_seen = false;
    let mut supervised_seen = false;
    let mut wait_for_restart_seen = false;
    let mut remote = tuicommander_lib::RemoteOptions::default();
    let mut args = args.into_iter().map(Into::into);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--instance" => {
                if instance.is_some() {
                    anyhow::bail!("--instance may only be specified once");
                }
                let id = args
                    .next()
                    .filter(|value| !value.starts_with("--"))
                    .ok_or_else(|| anyhow::anyhow!("--instance requires an identifier"))?;
                instance = Some(id);
            }
            "--set-password" => {
                if set_password {
                    anyhow::bail!("--set-password may only be specified once");
                }
                set_password = true;
            }
            "--build-info" => {
                if build_info {
                    anyhow::bail!("--build-info may only be specified once");
                }
                build_info = true;
            }
            "--relay-pair" => {
                if relay_pair_seen {
                    anyhow::bail!("--relay-pair may only be specified once");
                }
                relay_pair_seen = true;
                let url = args
                    .next()
                    .filter(|value| !value.starts_with("--"))
                    .ok_or_else(|| anyhow::anyhow!("--relay-pair requires the relay URL"))?;
                relay_pair.get_or_insert_with(Default::default).relay_url = url;
            }
            "--relay-register" => {
                if relay_register_seen {
                    anyhow::bail!("--relay-register may only be specified once");
                }
                relay_register_seen = true;
                relay_pair.get_or_insert_with(Default::default).register = true;
            }
            "--relay-out" => {
                if relay_out_seen {
                    anyhow::bail!("--relay-out may only be specified once");
                }
                relay_out_seen = true;
                let path = args
                    .next()
                    .filter(|value| !value.starts_with("--"))
                    .ok_or_else(|| anyhow::anyhow!("--relay-out requires a file path"))?;
                relay_pair.get_or_insert_with(Default::default).out = Some(path.into());
            }
            "--bind" => {
                if bind_seen {
                    anyhow::bail!("--bind may only be specified once");
                }
                bind_seen = true;
                let value = args
                    .next()
                    .filter(|value| !value.starts_with("--"))
                    .ok_or_else(|| anyhow::anyhow!("--bind requires an IP address"))?;
                remote.bind = value
                    .parse()
                    .map_err(|e| anyhow::anyhow!("invalid --bind address {value:?}: {e}"))?;
            }
            "--survive-secs" => {
                if survive_seen {
                    anyhow::bail!("--survive-secs may only be specified once");
                }
                survive_seen = true;
                let value = args
                    .next()
                    .filter(|value| !value.starts_with("--"))
                    .ok_or_else(|| anyhow::anyhow!("--survive-secs requires a duration"))?;
                remote.survive_secs =
                    Some(value.parse().map_err(|e| {
                        anyhow::anyhow!("invalid --survive-secs value {value:?}: {e}")
                    })?);
            }
            "--no-agent-configs" => {
                if no_agent_configs_seen {
                    anyhow::bail!("--no-agent-configs may only be specified once");
                }
                no_agent_configs_seen = true;
                remote.agent_configs = false;
            }
            "--supervised" => {
                if supervised_seen {
                    anyhow::bail!("--supervised may only be specified once");
                }
                supervised_seen = true;
                remote.supervised = true;
            }
            "--wait-for-restart" => {
                if wait_for_restart_seen {
                    anyhow::bail!("--wait-for-restart may only be specified once");
                }
                wait_for_restart_seen = true;
                remote.wait_for_restart = true;
            }
            _ => anyhow::bail!("Unknown argument: {arg}"),
        }
    }

    if relay_pair.as_ref().is_some_and(|o| o.relay_url.is_empty()) {
        anyhow::bail!("--relay-register and --relay-out need --relay-pair <url>");
    }
    Ok(ParsedArgs {
        instance,
        set_password,
        build_info,
        relay_pair,
        remote,
    })
}

#[cfg(any(not(feature = "desktop"), test))]
fn take_pairing_token() -> Option<String> {
    let token = std::env::var("TUIC_PAIRING_TOKEN").ok();
    // SAFETY: the non-desktop entry point calls this before constructing the
    // Tokio runtime, so no other thread can read the process environment.
    unsafe { std::env::remove_var("TUIC_PAIRING_TOKEN") };
    token
}

#[cfg(not(feature = "desktop"))]
fn main() -> anyhow::Result<()> {
    let mut parsed = parse_args(std::env::args().skip(1))?;

    if parsed.build_info {
        println!(
            "{}",
            tuicommander_lib::remote_build_info_json().map_err(anyhow::Error::msg)?
        );
        return Ok(());
    }

    // `--instance` wins; otherwise honour TUIC_APP_INSTANCE like the desktop
    // binary, so a dev/test daemon never lands on the production config dir.
    match parsed.instance.as_deref() {
        Some(id) => tuicommander_lib::app_instance::select_app_instance(Some(id)),
        None => tuicommander_lib::app_instance::select_app_instance_from_env(),
    }
    .map_err(anyhow::Error::msg)?;

    if parsed.set_password {
        return tuicommander_lib::set_password_interactive();
    }

    if let Some(options) = parsed.relay_pair.take() {
        return tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(tuicommander_lib::relay_pair(options));
    }

    parsed.remote.port = match std::env::var("TUIC_PORT") {
        Ok(val) => val.parse().unwrap_or_else(|e| {
            eprintln!("warning: TUIC_PORT={val:?} is not a valid port ({e}), using default 9877");
            9877
        }),
        Err(_) => 9877,
    };
    parsed.remote.set_pairing_token(take_pairing_token());

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(tuicommander_lib::run_remote(parsed.remote))
}

#[cfg(feature = "desktop")]
fn main() {
    eprintln!("tuic-remote requires --no-default-features (desktop feature must be disabled)");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn remote_options_parse_deploy_flags_with_existing_flags() {
        let parsed = parse_args([
            "--instance",
            "worker-a",
            "--bind",
            "127.0.0.1",
            "--survive-secs",
            "1800",
            "--no-agent-configs",
            "--set-password",
        ])
        .expect("valid options");

        assert_eq!(parsed.instance.as_deref(), Some("worker-a"));
        assert!(parsed.set_password);
        assert_eq!(parsed.remote.bind, IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(parsed.remote.survive_secs, Some(1800));
        assert!(!parsed.remote.agent_configs);
    }

    #[test]
    fn relay_pair_flags_parse_and_reject_orphans() {
        let parsed = parse_args([
            "--relay-pair",
            "wss://relay.example",
            "--relay-register",
            "--relay-out",
            "bundle.txt",
        ])
        .expect("valid relay pairing");
        let options = parsed.relay_pair.expect("relay options");
        assert_eq!(options.relay_url, "wss://relay.example");
        assert!(options.register);
        assert_eq!(
            options.out.as_deref(),
            Some(std::path::Path::new("bundle.txt"))
        );
        assert!(parse_args(["--relay-register"]).is_err());
        assert!(parse_args(["--relay-out", "x"]).is_err());
        assert!(parse_args(["--relay-pair"]).is_err());
    }

    #[test]
    fn remote_restart_flags_are_accepted_together() {
        let parsed = parse_args(["--supervised", "--wait-for-restart"]);
        assert!(
            parsed.is_ok(),
            "restart flags must survive a daemon relaunch"
        );
    }

    #[test]
    fn build_info_flag_is_read_only_and_rejects_duplicates() {
        let parsed = parse_args(["--build-info"]).expect("build identity query");
        assert!(parsed.build_info);
        assert!(parse_args(["--build-info", "--build-info"]).is_err());
        let identity: serde_json::Value =
            serde_json::from_str(&tuicommander_lib::remote_build_info_json().unwrap()).unwrap();
        assert_eq!(identity["target"], env!("TUIC_TARGET_TRIPLE"));
        assert_eq!(identity["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(identity["sha256"].as_str().unwrap().len(), 64);
    }

    #[test]
    fn remote_options_reject_unknown_duplicate_and_missing_values() {
        for args in [
            vec!["--unknown"],
            vec!["--bind", "127.0.0.1", "--bind", "0.0.0.0"],
            vec!["--survive-secs", "1", "--survive-secs", "2"],
            vec!["--no-agent-configs", "--no-agent-configs"],
            vec!["--bind"],
            vec!["--survive-secs", "nope"],
        ] {
            assert!(parse_args(args.clone()).is_err(), "accepted {args:?}");
        }
    }

    #[test]
    #[serial_test::serial]
    fn pairing_token_is_consumed_from_the_environment() {
        // SAFETY: this serial test owns the process environment while it runs.
        unsafe { std::env::set_var("TUIC_PAIRING_TOKEN", "pair-once") };
        assert_eq!(take_pairing_token().as_deref(), Some("pair-once"));
        assert!(std::env::var_os("TUIC_PAIRING_TOKEN").is_none());
    }
}
