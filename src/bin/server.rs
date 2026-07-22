use anyhow::{Result, bail};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};
use mcp_kali::{
    analysis::AnalysisRoot,
    config::{
        default_config_dir, default_projects_dir, default_state_dir, default_system_data_dir,
    },
    jobs::{Scheduler, default_archive_root},
    plugins::{PluginRegistry, PrivilegeElevation},
    references::{ReferenceImport, import_reference},
};
use std::{env, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// Kali-side scheduler, API, and job-control dashboard.
#[derive(Parser)]
#[command(
    name = "mcp-kali",
    version,
    about = "Kali-side job scheduler, HTTP API, and dashboard"
)]
struct Cli {
    /// Load defaults from this configuration file. Shell variables and CLI flags override it.
    #[arg(long, global = true, value_name = "PATH")]
    config_file: Option<PathBuf>,

    /// Address for the local HTTP API and dashboard.
    #[arg(long, env = "MCP_KALI_BIND", default_value = "127.0.0.1:5000")]
    bind: SocketAddr,

    /// Durable job metadata and output directory.
    #[arg(
        long,
        env = "MCP_KALI_STATE_DIR",
        default_value_os_t = default_state_dir()
    )]
    state_dir: PathBuf,

    /// Root for projects, including operator-managed evidence and notes plus
    /// MCP Kali exports and native scanner artifacts.
    #[arg(
        long = "projects-dir",
        env = "MCP_KALI_PROJECTS_DIR",
        default_value_os_t = default_projects_dir()
    )]
    projects_dir: PathBuf,

    /// Directory for recoverably archived terminal job records.
    #[arg(long, env = "MCP_KALI_JOB_ARCHIVE_DIR", value_name = "PATH")]
    job_archive_dir: Option<PathBuf>,

    /// Archive terminal jobs at least this many minutes old when SIGUSR1 is received.
    #[arg(long, env = "MCP_KALI_JOB_ARCHIVE_AFTER_MINUTES", default_value_t = 60)]
    job_archive_after_minutes: u64,

    /// Maximum scanner processes running at once.
    #[arg(long, env = "MCP_KALI_MAX_CONCURRENCY", default_value_t = 4)]
    max_concurrency: usize,

    /// Default per-job wall-clock timeout in seconds.
    #[arg(long, env = "MCP_KALI_DEFAULT_TIMEOUT", default_value_t = 432_000)]
    default_timeout: u64,

    /// Show otherwise-redacted passwords and sensitive arguments in the job API,
    /// dashboard, and completion webhooks.
    #[arg(long, env = "MCP_KALI_REVEAL_SENSITIVE_DATA", default_value_t = false)]
    reveal_sensitive_data: bool,

    /// Read-only packaged plugins and base capability catalog directory.
    #[arg(long, env = "MCP_KALI_SYSTEM_DATA_DIR", default_value_os_t = default_system_data_dir())]
    system_data_dir: PathBuf,

    /// Administrator plugin and capability-catalog overlay directory.
    #[arg(long, env = "MCP_KALI_CONFIG_DIR", default_value_os_t = default_config_dir())]
    config_dir: PathBuf,

    /// Disable the privileged Core Plugin execute_command escape hatch.
    #[arg(
        long,
        env = "MCP_KALI_DISABLE_EXECUTE_COMMAND",
        default_value_t = false
    )]
    disable_execute_command: bool,

    /// Automatically use non-interactive sudo for declarative tools that declare
    /// requirements.privilege: root. Use none to run them as the server user.
    #[arg(long, env = "MCP_KALI_PRIVILEGE_ELEVATION", default_value = "auto")]
    privilege_elevation: PrivilegeElevation,

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
    /// Manage operator reference documents.
    References {
        #[command(subcommand)]
        command: ReferenceCommands,
    },
}

#[derive(Subcommand)]
enum ReferenceCommands {
    /// Import a Markdown guide into the administrator reference overlay.
    Import {
        /// Markdown file to import. Symlinks and files over 256 KiB are rejected.
        file: PathBuf,
        /// Stable lowercase reference ID, for example nmap.internal-discovery.
        #[arg(long)]
        id: String,
        /// Plugin that owns this guidance.
        #[arg(long)]
        plugin: String,
        /// Human-readable reference title.
        #[arg(long)]
        title: String,
        /// Short description shown by MCP clients and the dashboard.
        #[arg(long)]
        description: String,
        /// Search tag; repeat for multiple values.
        #[arg(long = "tag")]
        tags: Vec<String>,
        /// Related declarative tool; repeat for multiple values.
        #[arg(long = "related-tool")]
        related_tools: Vec<String>,
        /// Related capability ID; repeat for multiple values.
        #[arg(long = "related-capability")]
        related_capabilities: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let max_concurrency_from_environment = env::var_os("MCP_KALI_MAX_CONCURRENCY").is_some();
    let max_concurrency_from_cli = has_cli_option("--max-concurrency");
    let loaded_config_file = mcp_kali::config::load_config_file()?;
    let cli = Cli::parse();
    let _ = &cli.config_file;
    if let Some(command) = &cli.command {
        match command {
            Commands::Completions { shell } => {
                generate(
                    *shell,
                    &mut Cli::command(),
                    "mcp-kali",
                    &mut std::io::stdout(),
                );
                return Ok(());
            }
            Commands::References {
                command:
                    ReferenceCommands::Import {
                        file,
                        id,
                        plugin,
                        title,
                        description,
                        tags,
                        related_tools,
                        related_capabilities,
                    },
            } => {
                let destination = import_reference(ReferenceImport {
                    source: file,
                    config_dir: &cli.config_dir,
                    id,
                    plugin,
                    title,
                    description,
                    tags: tags.clone(),
                    related_tools: related_tools.clone(),
                    related_capabilities: related_capabilities.clone(),
                })?;
                println!("Imported reference: {}", destination.display());
                return Ok(());
            }
        }
    }
    let logging = mcp_kali::logging::init()?;
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
    let job_archive_dir = cli
        .job_archive_dir
        .unwrap_or_else(|| default_archive_root(&cli.state_dir));
    let projects = AnalysisRoot::open(&cli.projects_dir)?;
    let scheduler = Scheduler::open_with_archive_and_analysis(
        cli.state_dir.clone(),
        job_archive_dir,
        projects,
        cli.max_concurrency,
        cli.default_timeout,
        cli.job_archive_after_minutes,
        cli.reveal_sensitive_data,
    )
    .await?;
    let registry = PluginRegistry::load_with_privilege_elevation(
        &cli.system_data_dir,
        &cli.config_dir,
        !cli.disable_execute_command,
        cli.privilege_elevation,
    );
    log_registry_diagnostics(&registry);
    tracing::info!(
        plugins = registry.plugins().len(),
        tools = registry.tools().len(),
        references = registry.references().len(),
        diagnostics = registry.diagnostics().len() + registry.reference_diagnostics().len(),
        projects_dir = %scheduler.projects_root().display(),
        "plugin registry loaded"
    );
    let registry = Arc::new(RwLock::new(registry));
    let shutdown = CancellationToken::new();
    let signal_task = tokio::spawn(signal_loop(
        scheduler.clone(),
        registry.clone(),
        shutdown.clone(),
        ReloadSettings {
            config_file: loaded_config_file,
            system_data_dir: cli.system_data_dir,
            config_dir: cli.config_dir,
            execute_enabled: !cli.disable_execute_command,
            privilege_elevation: cli.privilege_elevation,
            reload_max_concurrency: !max_concurrency_from_environment && !max_concurrency_from_cli,
        },
        logging,
    ));
    let result =
        mcp_kali::api::serve(cli.bind, scheduler.clone(), registry, shutdown.clone()).await;
    shutdown.cancel();
    scheduler.shutdown().await;
    let _ = signal_task.await;
    result
}

struct ReloadSettings {
    config_file: Option<PathBuf>,
    system_data_dir: PathBuf,
    config_dir: PathBuf,
    execute_enabled: bool,
    privilege_elevation: PrivilegeElevation,
    reload_max_concurrency: bool,
}

async fn signal_loop(
    scheduler: Scheduler,
    registry: Arc<RwLock<PluginRegistry>>,
    shutdown: CancellationToken,
    settings: ReloadSettings,
    logging: mcp_kali::logging::LoggingHandle,
) {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let Ok(mut terminate) = signal(SignalKind::terminate()) else {
            tracing::error!("could not register SIGTERM handler");
            shutdown.cancel();
            return;
        };
        let Ok(mut interrupt) = signal(SignalKind::interrupt()) else {
            tracing::error!("could not register SIGINT handler");
            shutdown.cancel();
            return;
        };
        let Ok(mut reload) = signal(SignalKind::hangup()) else {
            tracing::error!("could not register SIGHUP handler");
            shutdown.cancel();
            return;
        };
        let Ok(mut archive) = signal(SignalKind::user_defined1()) else {
            tracing::error!("could not register SIGUSR1 handler");
            shutdown.cancel();
            return;
        };
        loop {
            tokio::select! {
                _ = terminate.recv() => {
                    tracing::info!("received SIGTERM; shutting down gracefully");
                    scheduler.begin_shutdown().await;
                    shutdown.cancel();
                    break;
                }
                _ = interrupt.recv() => {
                    tracing::info!("received SIGINT; shutting down gracefully");
                    scheduler.begin_shutdown().await;
                    shutdown.cancel();
                    break;
                }
                _ = reload.recv() => {
                    match logging.reopen() {
                        Ok(true) => tracing::info!("log files reopened after SIGHUP"),
                        Ok(false) => {}
                        Err(error) => tracing::error!(%error, "could not reopen configured log files; using stdout"),
                    }
                    reload_runtime(&scheduler, &registry, &settings).await;
                }
                _ = archive.recv() => {
                    let older_than_minutes = scheduler.archive_after_minutes();
                    match scheduler.archive_terminal_jobs(older_than_minutes).await {
                        Ok(result) => tracing::info!(
                            older_than_minutes,
                            matched = result.matched,
                            archived = result.archived,
                            failed = result.failed,
                            bytes_archived = result.bytes_archived,
                            "archived terminal jobs after SIGUSR1"
                        ),
                        Err(error) => tracing::error!(%error, "SIGUSR1 job archive failed"),
                    }
                }
                _ = shutdown.cancelled() => return,
            }
        }
        tokio::select! {
            _ = terminate.recv() => {
                tracing::warn!("received second SIGTERM; force-killing active job process groups");
                scheduler.force_kill_active().await;
            }
            _ = interrupt.recv() => {
                tracing::warn!("received second SIGINT; force-killing active job process groups");
                scheduler.force_kill_active().await;
            }
            _ = scheduler.wait_for_shutdown() => return,
        }
        scheduler.wait_for_shutdown().await;
    }
    #[cfg(not(unix))]
    {
        let _ = (scheduler, registry, settings, logging);
        shutdown.cancelled().await;
    }
}

async fn reload_runtime(
    scheduler: &Scheduler,
    registry: &Arc<RwLock<PluginRegistry>>,
    settings: &ReloadSettings,
) {
    let max_concurrency = match (
        settings.reload_max_concurrency,
        settings.config_file.as_deref(),
    ) {
        (false, _) => None,
        (true, Some(path)) => {
            match mcp_kali::config::read_config_value(path, "MCP_KALI_MAX_CONCURRENCY") {
                Ok(Some(value)) => match value.parse::<usize>() {
                    Ok(value) if (1..=256).contains(&value) => Some(value),
                    _ => {
                        tracing::warn!(path = %path.display(), "reload rejected: MCP_KALI_MAX_CONCURRENCY must be between 1 and 256; keeping last-known-good runtime");
                        return;
                    }
                },
                Ok(None) => None,
                Err(error) => {
                    tracing::warn!(path = %path.display(), %error, "reload rejected: could not read configuration; keeping last-known-good runtime");
                    return;
                }
            }
        }
        (true, None) => None,
    };
    let replacement = PluginRegistry::load_with_privilege_elevation(
        &settings.system_data_dir,
        &settings.config_dir,
        settings.execute_enabled,
        settings.privilege_elevation,
    );
    if !replacement.diagnostics().is_empty() || !replacement.reference_diagnostics().is_empty() {
        tracing::warn!(
            diagnostics =
                replacement.diagnostics().len() + replacement.reference_diagnostics().len(),
            "reload rejected: Plugin or reference diagnostics were found; keeping last-known-good runtime"
        );
        log_registry_diagnostics(&replacement);
        return;
    }
    if let Some(max_concurrency) = max_concurrency {
        if let Err(error) = scheduler.set_max_concurrency(max_concurrency).await {
            tracing::warn!(%error, "reload rejected: could not update scheduler concurrency; keeping last-known-good runtime");
            return;
        }
    }
    log_registry_diagnostics(&replacement);
    let plugins = replacement.plugins().len();
    let tools = replacement.tools().len();
    let references = replacement.references().len();
    *registry.write().await = replacement;
    tracing::info!(plugins, tools, references, max_concurrency = ?max_concurrency, "runtime reloaded after SIGHUP");
}

fn has_cli_option(option: &str) -> bool {
    env::args_os().skip(1).any(|argument| {
        argument == option
            || argument
                .to_string_lossy()
                .starts_with(&format!("{option}="))
    })
}

fn log_registry_diagnostics(registry: &PluginRegistry) {
    for diagnostic in registry.diagnostics() {
        tracing::warn!(
            layer = %diagnostic.layer,
            path = %diagnostic.path,
            "plugin diagnostic"
        );
    }
    for diagnostic in registry.reference_diagnostics() {
        tracing::warn!(
            layer = %diagnostic.layer,
            path = %diagnostic.path,
            "reference diagnostic"
        );
    }
}
