use std::path::Path;

use super::profile::{ForwardSpec, StrictHostKeyChecking, TunnelProfile};

/// Build the ssh argument vector for a tunnel profile.
pub fn build_ssh_args(profile: &TunnelProfile) -> Vec<String> {
    let mut args = Vec::new();

    // argv[0]
    args.push("ssh".to_string());

    // No shell, no stdin, no TTY
    args.push("-N".to_string());
    args.push("-n".to_string());
    args.push("-T".to_string());

    // No interactive prompts
    args.push("-o".to_string());
    args.push("BatchMode=yes".to_string());

    // Fail if any forward can't bind
    args.push("-o".to_string());
    args.push("ExitOnForwardFailure=yes".to_string());

    // Keep-alive
    args.push("-o".to_string());
    args.push(format!(
        "ServerAliveInterval={}",
        profile.options.server_alive_interval
    ));
    args.push("-o".to_string());
    args.push(format!(
        "ServerAliveCountMax={}",
        profile.options.server_alive_count_max
    ));

    // Host key policy
    let shk_value = match profile.options.strict_host_key_checking {
        StrictHostKeyChecking::Yes => "yes",
        StrictHostKeyChecking::AcceptNew => "accept-new",
    };
    args.push("-o".to_string());
    args.push(format!("StrictHostKeyChecking={shk_value}"));

    // Security policy: never forward the agent
    args.push("-o".to_string());
    args.push("ForwardAgent=no".to_string());

    // SSH port
    args.push("-p".to_string());
    args.push(profile.port.to_string());

    // Identity file (optional)
    if let Some(identity) = &profile.identity_file {
        args.push("-i".to_string());
        args.push(identity.to_string_lossy().into_owned());
    }

    // Port forwards
    for forward in &profile.forwards {
        match forward {
            ForwardSpec::Local {
                bind_port,
                remote_host,
                remote_port,
            } => {
                args.push("-L".to_string());
                args.push(format!("{bind_port}:{remote_host}:{remote_port}"));
            }
            ForwardSpec::Remote {
                bind_port,
                local_host,
                local_port,
            } => {
                args.push("-R".to_string());
                args.push(format!("{bind_port}:{local_host}:{local_port}"));
            }
        }
    }

    // Destination — must be last
    args.push(format!("{}@{}", profile.user, profile.host));

    args
}

/// Build environment variables for the ssh process.
/// Sets SSH_AUTH_SOCK if agent_socket is provided.
pub fn build_ssh_env(agent_socket: Option<&Path>) -> Vec<(String, String)> {
    let mut env = Vec::new();
    if let Some(socket) = agent_socket {
        env.push((
            "SSH_AUTH_SOCK".to_string(),
            socket.to_string_lossy().into_owned(),
        ));
    }
    env
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::tunnels::profile::{ProfileOptions, StrictHostKeyChecking, TunnelProfile};

    fn base_profile() -> TunnelProfile {
        TunnelProfile {
            id: uuid::Uuid::new_v4().to_string(),
            name: "test".to_string(),
            host: "example.com".to_string(),
            port: 22,
            user: "alice".to_string(),
            identity_file: None,
            forwards: Vec::new(),
            options: ProfileOptions::default(),
            auto_connect: false,
        }
    }

    // Helper: assert that a flag (possibly with a value) is absent in the arg vector.
    fn assert_flag_absent(args: &[String], flag: &str) {
        assert!(
            !args.iter().any(|a| a == flag),
            "flag {flag:?} must not appear in args: {args:?}"
        );
    }

    // Helper: find the value following a `-o` option prefix.
    fn find_option<'a>(args: &'a [String], prefix: &str) -> Option<&'a str> {
        args.windows(2)
            .find(|w| w[0] == "-o" && w[1].starts_with(prefix))
            .map(|w| w[1].as_str())
    }

    #[test]
    fn local_forward_only() {
        let mut profile = base_profile();
        profile.forwards = vec![ForwardSpec::Local {
            bind_port: 8080,
            remote_host: "internal.example.com".to_string(),
            remote_port: 80,
        }];

        let args = build_ssh_args(&profile);

        // Verify the -L flag and its value appear consecutively
        let l_pos = args
            .iter()
            .position(|a| a == "-L")
            .expect("-L must be present");
        assert_eq!(args[l_pos + 1], "8080:internal.example.com:80");
        // No -R flag
        assert_flag_absent(&args, "-R");
        // Last arg is user@host
        assert_eq!(args.last().unwrap(), "alice@example.com");
    }

    #[test]
    fn remote_forward_only() {
        let mut profile = base_profile();
        profile.forwards = vec![ForwardSpec::Remote {
            bind_port: 9090,
            local_host: "127.0.0.1".to_string(),
            local_port: 3000,
        }];

        let args = build_ssh_args(&profile);

        let r_pos = args
            .iter()
            .position(|a| a == "-R")
            .expect("-R must be present");
        assert_eq!(args[r_pos + 1], "9090:127.0.0.1:3000");
        assert_flag_absent(&args, "-L");
        assert_eq!(args.last().unwrap(), "alice@example.com");
    }

    #[test]
    fn mixed_forwards() {
        let mut profile = base_profile();
        profile.forwards = vec![
            ForwardSpec::Local {
                bind_port: 8080,
                remote_host: "internal.example.com".to_string(),
                remote_port: 80,
            },
            ForwardSpec::Remote {
                bind_port: 9090,
                local_host: "127.0.0.1".to_string(),
                local_port: 3000,
            },
        ];

        let args = build_ssh_args(&profile);

        assert!(args.iter().any(|a| a == "-L"), "-L must be present");
        assert!(args.iter().any(|a| a == "-R"), "-R must be present");

        let l_pos = args.iter().position(|a| a == "-L").unwrap();
        assert_eq!(args[l_pos + 1], "8080:internal.example.com:80");

        let r_pos = args.iter().position(|a| a == "-R").unwrap();
        assert_eq!(args[r_pos + 1], "9090:127.0.0.1:3000");
    }

    #[test]
    fn with_identity_file() {
        let mut profile = base_profile();
        profile.identity_file = Some(PathBuf::from("/home/alice/.ssh/id_ed25519"));

        let args = build_ssh_args(&profile);

        let i_pos = args
            .iter()
            .position(|a| a == "-i")
            .expect("-i must be present");
        assert_eq!(args[i_pos + 1], "/home/alice/.ssh/id_ed25519");
    }

    #[test]
    fn without_identity_file() {
        let profile = base_profile();
        let args = build_ssh_args(&profile);
        assert_flag_absent(&args, "-i");
    }

    #[test]
    fn ssh_auth_sock_override_in_env() {
        let socket = Path::new("/run/user/1000/ssh-agent.sock");
        let env = build_ssh_env(Some(socket));

        assert_eq!(env.len(), 1);
        assert_eq!(env[0].0, "SSH_AUTH_SOCK");
        assert_eq!(env[0].1, "/run/user/1000/ssh-agent.sock");
    }

    #[test]
    fn no_agent_socket_yields_empty_env() {
        let env = build_ssh_env(None);
        assert!(env.is_empty());
    }

    #[test]
    fn forward_agent_no_always_present() {
        let profile = base_profile();
        let args = build_ssh_args(&profile);

        let found = find_option(&args, "ForwardAgent=");
        assert_eq!(found, Some("ForwardAgent=no"), "ForwardAgent must be 'no'");
    }

    #[test]
    fn strict_host_key_checking_accept_new() {
        let mut profile = base_profile();
        profile.options.strict_host_key_checking = StrictHostKeyChecking::AcceptNew;

        let args = build_ssh_args(&profile);

        let found = find_option(&args, "StrictHostKeyChecking=");
        assert_eq!(
            found,
            Some("StrictHostKeyChecking=accept-new"),
            "AcceptNew must map to 'accept-new'"
        );
    }

    #[test]
    fn no_dash_a_flag_ever() {
        // -A enables agent forwarding — must never appear
        let mut profile = base_profile();
        profile.forwards = vec![ForwardSpec::Local {
            bind_port: 8080,
            remote_host: "host".to_string(),
            remote_port: 80,
        }];
        let args = build_ssh_args(&profile);
        assert_flag_absent(&args, "-A");
    }
}
