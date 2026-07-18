use anyhow::{Result, bail};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};
use mcp_kali::jobs::Scheduler;
use std::{net::SocketAddr, path::PathBuf};
use tracing_subscriber::EnvFilter;

/// Kali-side scheduler, API, and job-control dashboard.
#[derive(Parser)]
#[command(
    name = "mcp-kali-server",
    version,
    about = "Kali-side job scheduler, HTTP API, and dashboard"
)]
struct Cli {
    /// Load defaults from this env file. Shell variables and CLI flags override it.
    #[arg(long, env = "MCP_KALI_ENV_FILE", global = true, value_name = "PATH")]
    env_file: Option<PathBuf>,

    /// Address for the local HTTP API and dashboard.
    #[arg(long, env = "MCP_KALI_BIND", default_value = "127.0.0.1:5000")]
    bind: SocketAddr,

    /// Durable job metadata and output directory.
    #[arg(
        long,
        env = "MCP_KALI_STATE_DIR",
        default_value = "/var/lib/mcp-kali/jobs"
    )]
    state_dir: PathBuf,

    /// Maximum scanner processes running at once.
    #[arg(long, env = "MCP_KALI_MAX_CONCURRENCY", default_value_t = 2)]
    max_concurrency: usize,

    /// Default per-job wall-clock timeout in seconds.
    #[arg(long, env = "MCP_KALI_DEFAULT_TIMEOUT", default_value_t = 1800)]
    default_timeout: u64,

    /// Show otherwise-redacted passwords and sensitive arguments in the job API,
    /// dashboard, and completion webhooks.
    #[arg(long, env = "MCP_KALI_REVEAL_SENSITIVE_DATA", default_value_t = false)]
    reveal_sensitive_data: bool,

    /// Permit binding to a non-loopback address. The server has no built-in
    /// authentication; protect remote access with a firewall and private
    /// tunnel or access-controlled TLS proxy.
    #[arg(long, env = "MCP_KALI_ALLOW_REMOTE_BIND", default_value_t = false)]
    allow_remote_bind: bool,

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
    mcp_kali::config::load_env_file()?;
    let cli = Cli::parse();
    let _ = &cli.env_file;
    if let Some(Commands::Completions { shell }) = cli.command {
        generate(
            shell,
            &mut Cli::command(),
            "mcp-kali-server",
            &mut std::io::stdout(),
        );
        return Ok(());
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mcp_kali=info,tower_http=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    if cli.max_concurrency == 0 || cli.max_concurrency > 256 {
        bail!("--max-concurrency must be between 1 and 256");
    }
    if cli.default_timeout == 0 || cli.default_timeout > 604_800 {
        bail!("--default-timeout must be between 1 and 604800 seconds");
    }
    if !cli.bind.ip().is_loopback() && !cli.allow_remote_bind {
        bail!(
            "refusing non-loopback bind {}; pass --allow-remote-bind only behind access controls",
            cli.bind
        );
    }
    if !cli.bind.ip().is_loopback() {
        tracing::warn!(
            bind = %cli.bind,
            "remote bind enabled; mcp-kali has no built-in authentication"
        );
    }
    let scheduler = Scheduler::open_with_sensitive_data(
        cli.state_dir,
        cli.max_concurrency,
        cli.default_timeout,
        cli.reveal_sensitive_data,
    )
    .await?;
    mcp_kali::api::serve(cli.bind, scheduler).await
}
