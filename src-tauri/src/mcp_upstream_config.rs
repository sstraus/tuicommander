//! Configuration schema and persistence for upstream MCP servers.
//!
//! Each upstream server is identified by a unique name and can use either
//! HTTP (Streamable HTTP) or stdio (spawned process) transport. Configuration
//! lives in `mcp-upstreams.json`, separate from the main `AppConfig`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::config::{load_json_config, save_json_config};

const UPSTREAMS_FILE: &str = "mcp-upstreams.json";

/// Top-level wrapper for the upstream config file.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct UpstreamMcpConfig {
    #[serde(default)]
    pub(crate) servers: Vec<UpstreamMcpServer>,
}

/// A single upstream MCP server configuration.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct UpstreamMcpServer {
    /// Unique identifier (UUID).
    pub(crate) id: String,
    /// Human-readable name, also used as the tool namespace prefix.
    /// Must be unique, non-empty, and contain only `[a-z0-9_-]`.
    pub(crate) name: String,
    /// Transport configuration.
    pub(crate) transport: UpstreamTransport,
    /// Whether this upstream is active.
    #[serde(default = "default_true")]
    pub(crate) enabled: bool,
    /// Timeout in seconds for tool calls (0 = no timeout).
    #[serde(default = "default_timeout")]
    pub(crate) timeout_secs: u32,
    /// Optional tool filter (allow/deny list).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tool_filter: Option<ToolFilter>,
}

/// Transport type for connecting to an upstream MCP server.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum UpstreamTransport {
    Http {
        url: String,
    },
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
    },
}

/// Tool filter: allow or deny specific tools by glob pattern.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct ToolFilter {
    pub(crate) mode: FilterMode,
    pub(crate) patterns: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FilterMode {
    Allow,
    Deny,
}

fn default_true() -> bool {
    true
}

fn default_timeout() -> u32 {
    30
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validation errors for upstream config.
#[derive(Debug, PartialEq)]
pub(crate) enum UpstreamConfigError {
    EmptyName(String),
    InvalidName(String),
    DuplicateName(String),
    SelfReferentialUrl(String),
    EmptyUrl(String),
    EmptyCommand(String),
}

impl std::fmt::Display for UpstreamConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyName(id) => write!(f, "Server '{id}' has an empty name"),
            Self::InvalidName(name) => write!(
                f,
                "Name '{name}' is invalid: must contain only lowercase letters, digits, hyphens, and underscores"
            ),
            Self::DuplicateName(name) => write!(f, "Duplicate server name: '{name}'"),
            Self::SelfReferentialUrl(url) => {
                write!(f, "URL '{url}' points to this TUIC instance (circular proxy)")
            }
            Self::EmptyUrl(id) => write!(f, "Server '{id}' has an empty HTTP URL"),
            Self::EmptyCommand(id) => write!(f, "Server '{id}' has an empty stdio command"),
        }
    }
}

/// Validate the upstream config. Returns all errors found (not just the first).
pub(crate) fn validate_upstream_config(
    config: &UpstreamMcpConfig,
    self_port: u16,
) -> Vec<UpstreamConfigError> {
    let mut errors = Vec::new();
    let mut seen_names = std::collections::HashSet::new();
    let name_re = regex::Regex::new(r"^[a-z0-9_-]+$").unwrap();

    for server in &config.servers {
        // Empty name
        if server.name.is_empty() {
            errors.push(UpstreamConfigError::EmptyName(server.id.clone()));
            continue;
        }

        // Invalid name characters
        if !name_re.is_match(&server.name) {
            errors.push(UpstreamConfigError::InvalidName(server.name.clone()));
        }

        // Duplicate name
        if !seen_names.insert(&server.name) {
            errors.push(UpstreamConfigError::DuplicateName(server.name.clone()));
        }

        // Transport-specific validation
        match &server.transport {
            UpstreamTransport::Http { url } => {
                if url.is_empty() {
                    errors.push(UpstreamConfigError::EmptyUrl(server.id.clone()));
                } else if is_self_referential(url, self_port) {
                    errors.push(UpstreamConfigError::SelfReferentialUrl(url.clone()));
                }
            }
            UpstreamTransport::Stdio { command, .. } => {
                if command.is_empty() {
                    errors.push(UpstreamConfigError::EmptyCommand(server.id.clone()));
                }
            }
        }
    }

    errors
}

/// Check if a URL points to this TUIC instance (circular proxy).
fn is_self_referential(url: &str, self_port: u16) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or("");
    let port = parsed.port().unwrap_or(if parsed.scheme() == "https" { 443 } else { 80 });
    let is_localhost = matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]" | "0.0.0.0");
    is_localhost && port == self_port
}

// ---------------------------------------------------------------------------
// Persistence (Tauri commands)
// ---------------------------------------------------------------------------

#[tauri::command]
pub(crate) fn load_mcp_upstreams() -> UpstreamMcpConfig {
    load_json_config(UPSTREAMS_FILE)
}

#[tauri::command]
pub(crate) fn save_mcp_upstreams(
    config: UpstreamMcpConfig,
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<(), String> {
    let self_port = state.config.read().mcp_port;
    let errors = validate_upstream_config(&config, self_port);
    if !errors.is_empty() {
        let msgs: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        return Err(msgs.join("; "));
    }
    save_json_config(UPSTREAMS_FILE, &config)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn http_server(name: &str, url: &str) -> UpstreamMcpServer {
        UpstreamMcpServer {
            id: format!("id-{name}"),
            name: name.to_string(),
            transport: UpstreamTransport::Http {
                url: url.to_string(),
            },
            enabled: true,
            timeout_secs: 30,
            tool_filter: None,
        }
    }

    fn stdio_server(name: &str, command: &str) -> UpstreamMcpServer {
        UpstreamMcpServer {
            id: format!("id-{name}"),
            name: name.to_string(),
            transport: UpstreamTransport::Stdio {
                command: command.to_string(),
                args: vec!["-y".to_string(), "@modelcontextprotocol/server-filesystem".to_string()],
                env: HashMap::new(),
            },
            enabled: true,
            timeout_secs: 30,
            tool_filter: None,
        }
    }

    // -- Serialization round-trip --

    #[test]
    fn http_server_round_trip() {
        let server = http_server("github", "http://localhost:8080/mcp");
        let json = serde_json::to_string_pretty(&server).unwrap();
        let parsed: UpstreamMcpServer = serde_json::from_str(&json).unwrap();
        assert_eq!(server, parsed);
    }

    #[test]
    fn stdio_server_round_trip() {
        let server = stdio_server("filesystem", "npx");
        let json = serde_json::to_string_pretty(&server).unwrap();
        let parsed: UpstreamMcpServer = serde_json::from_str(&json).unwrap();
        assert_eq!(server, parsed);
    }

    #[test]
    fn config_round_trip() {
        let config = UpstreamMcpConfig {
            servers: vec![
                http_server("github", "http://localhost:8080/mcp"),
                stdio_server("filesystem", "npx"),
            ],
        };
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: UpstreamMcpConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn empty_config_round_trip() {
        let config = UpstreamMcpConfig::default();
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: UpstreamMcpConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, parsed);
        assert!(parsed.servers.is_empty());
    }

    #[test]
    fn defaults_applied_on_deserialize() {
        // Minimal JSON without optional fields
        let json = r#"{
            "id": "abc",
            "name": "test",
            "transport": { "type": "http", "url": "http://example.com/mcp" }
        }"#;
        let server: UpstreamMcpServer = serde_json::from_str(json).unwrap();
        assert!(server.enabled); // default_true
        assert_eq!(server.timeout_secs, 30); // default_timeout
        assert!(server.tool_filter.is_none());
    }

    #[test]
    fn tool_filter_round_trip() {
        let server = UpstreamMcpServer {
            id: "id-filtered".to_string(),
            name: "filtered".to_string(),
            transport: UpstreamTransport::Http {
                url: "http://localhost:9000/mcp".to_string(),
            },
            enabled: true,
            timeout_secs: 60,
            tool_filter: Some(ToolFilter {
                mode: FilterMode::Deny,
                patterns: vec!["dangerous_*".to_string(), "admin_*".to_string()],
            }),
        };
        let json = serde_json::to_string_pretty(&server).unwrap();
        let parsed: UpstreamMcpServer = serde_json::from_str(&json).unwrap();
        assert_eq!(server, parsed);
        let filter = parsed.tool_filter.unwrap();
        assert_eq!(filter.mode, FilterMode::Deny);
        assert_eq!(filter.patterns.len(), 2);
    }

    // -- Validation: valid configs --

    #[test]
    fn valid_config_passes() {
        let config = UpstreamMcpConfig {
            servers: vec![
                http_server("github", "http://remote-host:8080/mcp"),
                stdio_server("filesystem", "npx"),
            ],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert!(errors.is_empty(), "Expected no errors, got: {errors:?}");
    }

    #[test]
    fn valid_single_char_name() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("a", "http://remote:8080/mcp")],
        };
        assert!(validate_upstream_config(&config, 3845).is_empty());
    }

    #[test]
    fn valid_name_with_hyphens_underscores_digits() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("my-server_2", "http://remote:8080/mcp")],
        };
        assert!(validate_upstream_config(&config, 3845).is_empty());
    }

    // -- Validation: empty name --

    #[test]
    fn empty_name_rejected() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("", "http://remote:8080/mcp")],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], UpstreamConfigError::EmptyName(_)));
    }

    // -- Validation: invalid name characters --

    #[test]
    fn uppercase_name_rejected() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("GitHub", "http://remote:8080/mcp")],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], UpstreamConfigError::InvalidName(_)));
    }

    #[test]
    fn name_with_spaces_rejected() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("my server", "http://remote:8080/mcp")],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], UpstreamConfigError::InvalidName(_)));
    }

    #[test]
    fn name_with_dots_rejected() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("my.server", "http://remote:8080/mcp")],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], UpstreamConfigError::InvalidName(_)));
    }

    // -- Validation: duplicate names --

    #[test]
    fn duplicate_names_rejected() {
        let config = UpstreamMcpConfig {
            servers: vec![
                http_server("github", "http://host-a:8080/mcp"),
                http_server("github", "http://host-b:9090/mcp"),
            ],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], UpstreamConfigError::DuplicateName(_)));
    }

    // -- Validation: self-referential URL --

    #[test]
    fn self_referential_localhost_rejected() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("bad", "http://localhost:3845/mcp")],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0],
            UpstreamConfigError::SelfReferentialUrl(_)
        ));
    }

    #[test]
    fn self_referential_127_rejected() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("bad", "http://127.0.0.1:3845/mcp")],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0],
            UpstreamConfigError::SelfReferentialUrl(_)
        ));
    }

    #[test]
    fn self_referential_ipv6_rejected() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("bad", "http://[::1]:3845/mcp")],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0],
            UpstreamConfigError::SelfReferentialUrl(_)
        ));
    }

    #[test]
    fn different_port_not_self_referential() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("ok", "http://localhost:9999/mcp")],
        };
        assert!(validate_upstream_config(&config, 3845).is_empty());
    }

    #[test]
    fn remote_host_not_self_referential() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("ok", "http://remote-host:3845/mcp")],
        };
        assert!(validate_upstream_config(&config, 3845).is_empty());
    }

    // -- Validation: empty URL / command --

    #[test]
    fn empty_url_rejected() {
        let config = UpstreamMcpConfig {
            servers: vec![http_server("bad", "")],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], UpstreamConfigError::EmptyUrl(_)));
    }

    #[test]
    fn empty_command_rejected() {
        let mut server = stdio_server("bad", "npx");
        if let UpstreamTransport::Stdio { ref mut command, .. } = server.transport {
            *command = String::new();
        }
        let config = UpstreamMcpConfig {
            servers: vec![server],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], UpstreamConfigError::EmptyCommand(_)));
    }

    // -- Validation: multiple errors --

    #[test]
    fn multiple_errors_collected() {
        let config = UpstreamMcpConfig {
            servers: vec![
                http_server("", "http://remote:8080/mcp"),  // empty name
                http_server("BAD", ""),                      // invalid name + empty url
                http_server("ok", "http://localhost:3845/mcp"), // self-ref
            ],
        };
        let errors = validate_upstream_config(&config, 3845);
        assert!(errors.len() >= 3, "Expected at least 3 errors, got: {errors:?}");
    }

    // -- is_self_referential edge cases --

    #[test]
    fn invalid_url_not_self_referential() {
        assert!(!is_self_referential("not-a-url", 3845));
    }

    #[test]
    fn https_localhost_with_matching_port() {
        assert!(is_self_referential("https://localhost:3845/mcp", 3845));
    }

    #[test]
    fn zero_zero_zero_zero_is_localhost() {
        assert!(is_self_referential("http://0.0.0.0:3845/mcp", 3845));
    }

    // -- Persistence (file I/O) --

    #[test]
    fn load_nonexistent_returns_default() {
        // load_json_config returns Default for missing files
        let config: UpstreamMcpConfig = load_json_config("nonexistent-mcp-upstreams-test.json");
        assert!(config.servers.is_empty());
    }

    #[test]
    fn save_and_load_round_trip_via_file() {
        use tempfile::TempDir;

        // We can't easily override config_dir() in tests, so we test the
        // serialization format is compatible with load_json_config's expectations.
        let config = UpstreamMcpConfig {
            servers: vec![
                http_server("github", "http://remote:8080/mcp"),
                stdio_server("filesystem", "npx"),
            ],
        };
        let json = serde_json::to_string_pretty(&config).unwrap();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mcp-upstreams.json");
        std::fs::write(&path, &json).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let loaded: UpstreamMcpConfig = serde_json::from_str(&content).unwrap();
        assert_eq!(config, loaded);
    }
}
