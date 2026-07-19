use anyhow::{Result, bail};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};
use reqwest::Url;
use std::{net::IpAddr, path::PathBuf};
use tracing_subscriber::EnvFilter;

/// Local stdio MCP bridge that talks to an mcp-kali instance.
#[derive(Parser)]
#[command(
    name = "mcp-kali-bridge",
    version,
    about = "Local stdio MCP bridge for mcp-kali"
)]
struct Cli {
    /// Load defaults from this configuration file. Shell variables and CLI flags override it.
    #[arg(long, env = "MCP_KALI_CONFIG_FILE", global = true, value_name = "PATH")]
    config_file: Option<PathBuf>,

    /// Base URL of the Kali-side mcp-kali API.
    #[arg(long, env = "MCP_KALI_SERVER", default_value = "http://127.0.0.1:5000")]
    server: Url,

    /// Permit cleartext HTTP to a non-loopback server. Prefer HTTPS or an SSH
    /// tunnel because job commands and output may be sensitive.
    #[arg(long, env = "MCP_KALI_ALLOW_INSECURE_HTTP", default_value_t = false)]
    allow_insecure_http: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a shell completion script on stdout.
    #[command(hide = true)]
    Completions { shell: Shell },
}

#[tokio::main]
async fn main() -> Result<()> {
    mcp_kali::config::load_config_file()?;
    let cli = Cli::parse();
    let _ = &cli.config_file;
    if let Some(Commands::Completions { shell }) = cli.command {
        generate(
            shell,
            &mut Cli::command(),
            "mcp-kali-bridge",
            &mut std::io::stdout(),
        );
        return Ok(());
    }
    validate_server_url(&cli.server, cli.allow_insecure_http)?;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "mcp_kali=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    mcp_kali::mcp::run(cli.server).await
}

fn validate_server_url(server: &Url, allow_insecure_http: bool) -> Result<()> {
    if !matches!(server.scheme(), "http" | "https") {
        bail!("--server must use http or https");
    }
    if !server.username().is_empty() || server.password().is_some() {
        bail!("--server must not contain credentials; use network-layer access controls");
    }
    if server.query().is_some() || server.fragment().is_some() {
        bail!("--server must not contain a query string or fragment");
    }
    if !matches!(server.path(), "" | "/") {
        bail!("--server must be an origin URL without a path");
    }
    let local = server.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    });
    if server.scheme() == "http" && !local && !allow_insecure_http {
        bail!(
            "refusing cleartext HTTP to a non-loopback server; use HTTPS, an SSH tunnel, or --allow-insecure-http"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_transport_protection_for_remote_servers() {
        assert!(validate_server_url(&"http://127.0.0.1:5000".parse().unwrap(), false).is_ok());
        assert!(validate_server_url(&"https://kali.example".parse().unwrap(), false).is_ok());
        assert!(validate_server_url(&"http://kali.example".parse().unwrap(), false).is_err());
        assert!(validate_server_url(&"http://kali.example".parse().unwrap(), true).is_ok());
    }
}
