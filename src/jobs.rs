use crate::{
    analysis::AnalysisRoot,
    models::{
        Job, JobArchiveFailure, JobArchivePreview, JobArchiveResult, JobState, OutputPage,
        SubmitJob,
    },
};
use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use flate2::{Compression, write::GzEncoder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs as stdfs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    process::Command,
    sync::{Mutex, Notify},
    time::timeout,
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

const MAX_OUTPUT_PAGE: usize = 1024 * 1024;
const MAX_ARG_COUNT: usize = 1024;
const MAX_ARG_BYTES: usize = 64 * 1024;
const MAX_COMMAND_BYTES: usize = 256 * 1024;
const MAX_TOOL_BYTES: usize = 128;
const MAX_ARCHIVE_MINUTES: u64 = 10 * 365 * 24 * 60;
const JOB_TERMINATION_GRACE: Duration = Duration::from_secs(5);
const SHUTDOWN_TERMINATION_GRACE: Duration = Duration::from_secs(10);
const INTEGRITY_FILE: &str = "integrity.json";
const INTEGRITY_ALGORITHM: &str = "sha256";

#[derive(Serialize, Deserialize)]
struct JobIntegrityManifest {
    version: u8,
    algorithm: String,
    generated_at: chrono::DateTime<Utc>,
    job_id: Uuid,
    files: Vec<JobIntegrityFile>,
}

#[derive(Serialize, Deserialize)]
struct JobIntegrityFile {
    path: String,
    bytes: u64,
    sha256: String,
}

pub fn default_archive_root(job_root: &Path) -> PathBuf {
    job_root
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("archive/jobs")
}

fn validate_archive_minutes(minutes: u64) -> Result<()> {
    if minutes == 0 || minutes > MAX_ARCHIVE_MINUTES {
        bail!("older_than_minutes must be between 1 and {MAX_ARCHIVE_MINUTES}");
    }
    Ok(())
}

fn archive_cutoff(minutes: u64) -> Result<chrono::DateTime<Utc>> {
    let minutes = i64::try_from(minutes).context("archive minute threshold is too large")?;
    Ok(Utc::now() - chrono::Duration::minutes(minutes))
}

async fn job_directory_size(path: &Path) -> Result<u64> {
    let mut entries = fs::read_dir(path)
        .await
        .with_context(|| format!("read job directory {}", path.display()))?;
    let mut bytes = 0u64;
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_file() {
            bytes = bytes.saturating_add(entry.metadata().await?.len());
        }
    }
    Ok(bytes)
}

#[derive(Serialize, Deserialize)]
struct PrivateJobSpec {
    argv: Vec<String>,
    #[serde(default)]
    webhook_url: Option<String>,
    #[serde(default)]
    stdout_export_path: Option<PathBuf>,
    #[serde(default)]
    stderr_export_path: Option<PathBuf>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StoredJobSpec {
    Current(PrivateJobSpec),
    Legacy(Vec<String>),
}

#[derive(Clone)]
pub struct Scheduler {
    inner: Arc<Inner>,
}

struct Inner {
    root: PathBuf,
    archive_root: PathBuf,
    analysis: AnalysisRoot,
    jobs: Mutex<HashMap<Uuid, Job>>,
    archive_lock: Mutex<()>,
    cancellations: Mutex<HashMap<Uuid, CancellationToken>>,
    process_ids: Mutex<HashMap<Uuid, i32>>,
    dispatch: Mutex<DispatchState>,
    notify: Notify,
    accepting: AtomicBool,
    default_timeout: u64,
    archive_after_minutes: u64,
    reveal_sensitive_data: bool,
    webhook_client: reqwest::Client,
}

struct DispatchState {
    max_concurrency: usize,
    running: usize,
}

impl Scheduler {
    pub async fn open(root: PathBuf, max_concurrency: usize, default_timeout: u64) -> Result<Self> {
        let archive_root = default_archive_root(&root);
        Self::open_with_archive(
            root,
            archive_root,
            max_concurrency,
            default_timeout,
            60,
            false,
        )
        .await
    }

    /// Starts a scheduler with explicit control over public command redaction.
    /// The execution specification remains private on disk in either mode.
    pub async fn open_with_sensitive_data(
        root: PathBuf,
        max_concurrency: usize,
        default_timeout: u64,
        reveal_sensitive_data: bool,
    ) -> Result<Self> {
        let archive_root = default_archive_root(&root);
        Self::open_with_archive(
            root,
            archive_root,
            max_concurrency,
            default_timeout,
            60,
            reveal_sensitive_data,
        )
        .await
    }

    pub async fn open_with_archive(
        root: PathBuf,
        archive_root: PathBuf,
        max_concurrency: usize,
        default_timeout: u64,
        archive_after_minutes: u64,
        reveal_sensitive_data: bool,
    ) -> Result<Self> {
        let analysis_root = root.join(".analysis");
        let analysis = AnalysisRoot::open_isolated(&analysis_root)?;
        Self::open_with_archive_and_analysis(
            root,
            archive_root,
            analysis,
            max_concurrency,
            default_timeout,
            archive_after_minutes,
            reveal_sensitive_data,
        )
        .await
    }

    pub async fn open_with_archive_and_analysis(
        root: PathBuf,
        archive_root: PathBuf,
        analysis: AnalysisRoot,
        max_concurrency: usize,
        default_timeout: u64,
        archive_after_minutes: u64,
        reveal_sensitive_data: bool,
    ) -> Result<Self> {
        if max_concurrency == 0 {
            bail!("max_concurrency must be greater than zero");
        }
        validate_archive_minutes(archive_after_minutes)?;
        if archive_root.as_os_str().is_empty() || archive_root.parent().is_none() {
            bail!("job archive directory must not be empty or a filesystem root");
        }
        if archive_root == root || archive_root.starts_with(&root) {
            bail!("job archive directory must be outside the active job state directory");
        }
        fs::create_dir_all(&root)
            .await
            .context("create job state directory")?;
        #[cfg(unix)]
        {
            fs::set_permissions(&root, std::os::unix::fs::PermissionsExt::from_mode(0o700))
                .await
                .context("secure job state directory")?;
        }
        let scheduler = Self {
            inner: Arc::new(Inner {
                root,
                archive_root,
                analysis,
                jobs: Mutex::new(HashMap::new()),
                archive_lock: Mutex::new(()),
                cancellations: Mutex::new(HashMap::new()),
                process_ids: Mutex::new(HashMap::new()),
                dispatch: Mutex::new(DispatchState {
                    max_concurrency,
                    running: 0,
                }),
                notify: Notify::new(),
                accepting: AtomicBool::new(true),
                default_timeout,
                archive_after_minutes,
                reveal_sensitive_data,
                webhook_client: reqwest::Client::new(),
            }),
        };
        scheduler.load().await?;
        let dispatcher = scheduler.clone();
        tokio::spawn(async move { dispatcher.dispatch().await });
        scheduler.inner.notify.notify_one();
        Ok(scheduler)
    }

    pub fn projects_root(&self) -> &Path {
        self.inner.analysis.path()
    }

    pub fn resolve_analysis_file(&self, requested: &str) -> Result<PathBuf> {
        self.inner.analysis.resolve_file(requested)
    }

    pub fn archive_after_minutes(&self) -> u64 {
        self.inner.archive_after_minutes
    }

    pub async fn preview_archive(&self, older_than_minutes: u64) -> Result<JobArchivePreview> {
        validate_archive_minutes(older_than_minutes)?;
        let cutoff = archive_cutoff(older_than_minutes)?;
        let candidates = self.archive_candidates(cutoff).await;
        let mut bytes = 0u64;
        for job in &candidates {
            bytes = bytes.saturating_add(job_directory_size(&self.job_dir(job.id)).await?);
        }
        Ok(JobArchivePreview {
            older_than_minutes,
            cutoff,
            matched: candidates.len(),
            bytes,
        })
    }

    pub async fn archive_terminal_jobs(&self, older_than_minutes: u64) -> Result<JobArchiveResult> {
        validate_archive_minutes(older_than_minutes)?;
        let _archive_guard = self.inner.archive_lock.lock().await;
        fs::create_dir_all(&self.inner.archive_root)
            .await
            .context("create job archive directory")?;
        #[cfg(unix)]
        fs::set_permissions(
            &self.inner.archive_root,
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .await
        .context("secure job archive directory")?;
        let cutoff = archive_cutoff(older_than_minutes)?;
        let candidates = self.archive_candidates(cutoff).await;
        let matched = candidates.len();
        let mut archive_jobs = Vec::new();
        let mut bytes_archived = 0u64;
        let mut failures = Vec::new();

        for job in candidates {
            let source = self.job_dir(job.id);
            if let Err(error) = ensure_integrity_manifest(&self.inner.root, &job).await {
                failures.push(JobArchiveFailure {
                    job_id: job.id,
                    error: format!("terminal job integrity verification failed: {error}"),
                });
                continue;
            }
            let bytes = match job_directory_size(&source).await {
                Ok(bytes) => bytes,
                Err(error) => {
                    failures.push(JobArchiveFailure {
                        job_id: job.id,
                        error: error.to_string(),
                    });
                    continue;
                }
            };
            bytes_archived = bytes_archived.saturating_add(bytes);
            archive_jobs.push(job);
        }

        let mut archive_file = None;
        if !archive_jobs.is_empty() {
            let destination = self.archive_destination(&archive_jobs).await?;
            let sources = archive_jobs
                .iter()
                .map(|job| (job.id, self.job_dir(job.id)))
                .collect::<Vec<_>>();
            let create_result = tokio::task::spawn_blocking({
                let destination = destination.clone();
                move || create_gzip_archive(&destination, &sources)
            })
            .await
            .context("archive compression task panicked")?;
            if let Err(error) = create_result {
                for job in &archive_jobs {
                    failures.push(JobArchiveFailure {
                        job_id: job.id,
                        error: format!("could not create compressed archive: {error}"),
                    });
                }
                bytes_archived = 0;
            } else {
                archive_file = destination
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_owned);
            }
        }

        let mut archived = 0usize;
        if archive_file.is_some() {
            for job in archive_jobs {
                match fs::remove_dir_all(self.job_dir(job.id)).await {
                    Ok(()) => {
                        self.inner.jobs.lock().await.remove(&job.id);
                        archived += 1;
                    }
                    Err(error) => failures.push(JobArchiveFailure {
                        job_id: job.id,
                        error: format!(
                            "compressed archive was created but the active job directory could not be removed: {error}"
                        ),
                    }),
                }
            }
        }

        for failure in &failures {
            warn!(
                id = %failure.job_id,
                error = %failure.error,
                "could not archive terminal job"
            );
        }

        Ok(JobArchiveResult {
            older_than_minutes,
            cutoff,
            matched,
            archived,
            failed: failures.len(),
            bytes_archived,
            archive_file,
            failures,
        })
    }

    async fn archive_destination(&self, jobs: &[Job]) -> Result<PathBuf> {
        let oldest_started = jobs
            .iter()
            .map(|job| job.started_at.unwrap_or(job.created_at))
            .min()
            .context("archive requires at least one job")?;
        let newest_finished = jobs
            .iter()
            .filter_map(|job| job.finished_at)
            .max()
            .context("terminal jobs must have a finished timestamp")?;
        let base = format!(
            "jobs_{}_to_{}_{}.tar.gz",
            oldest_started.format("%Y%m%dT%H%M%SZ"),
            newest_finished.format("%Y%m%dT%H%M%SZ"),
            jobs.len()
        );
        let destination = self.inner.archive_root.join(&base);
        if !fs::try_exists(&destination).await? {
            return Ok(destination);
        }
        Ok(self.inner.archive_root.join(format!(
            "jobs_{}_to_{}_{}_{}.tar.gz",
            oldest_started.format("%Y%m%dT%H%M%SZ"),
            newest_finished.format("%Y%m%dT%H%M%SZ"),
            jobs.len(),
            Uuid::new_v4()
        )))
    }

    async fn archive_candidates(&self, cutoff: chrono::DateTime<Utc>) -> Vec<Job> {
        self.inner
            .jobs
            .lock()
            .await
            .values()
            .filter(|job| {
                job.state.is_terminal()
                    && job.finished_at.is_some_and(|finished| finished <= cutoff)
            })
            .cloned()
            .collect()
    }

    async fn load(&self) -> Result<()> {
        let mut entries = fs::read_dir(&self.inner.root).await?;
        let mut jobs = self.inner.jobs.lock().await;
        while let Some(entry) = entries.next_entry().await? {
            if !entry.file_type().await?.is_dir() {
                continue;
            }
            let metadata = entry.path().join("job.json");
            let Ok(bytes) = fs::read(&metadata).await else {
                continue;
            };
            let Ok(mut job) = serde_json::from_slice::<Job>(&bytes) else {
                warn!(path = %metadata.display(), "ignoring invalid job metadata");
                continue;
            };
            match fs::read(entry.path().join("command.json")).await {
                Ok(bytes) => match serde_json::from_slice::<StoredJobSpec>(&bytes) {
                    Ok(StoredJobSpec::Current(spec)) => {
                        job.argv = spec.argv;
                        job.webhook_url = spec.webhook_url;
                        job.stdout_export_path = spec.stdout_export_path;
                        job.stderr_export_path = spec.stderr_export_path;
                    }
                    Ok(StoredJobSpec::Legacy(argv)) => job.argv = argv,
                    Err(_) => job.argv.clear(),
                },
                Err(_) => job.argv.clear(),
            }
            job.webhook_configured = job.webhook_url.is_some();
            let mut changed = false;
            let displayed_command = display_command(&job.argv, self.inner.reveal_sensitive_data);
            if job.command != displayed_command {
                job.command = displayed_command;
                changed = true;
            }
            if job.argv.is_empty() && job.state == JobState::Queued {
                job.state = JobState::Interrupted;
                job.finished_at = Some(Utc::now());
                job.error = Some("private execution specification is missing".into());
                changed = true;
            }
            if job.state == JobState::Running {
                job.state = JobState::Interrupted;
                job.finished_at = Some(Utc::now());
                job.error = Some("server restarted while job was running".into());
                changed = true;
            }
            if changed {
                if job.state.is_terminal() {
                    persist_terminal_at(&self.inner.root, &job).await?;
                } else {
                    persist_at(&self.inner.root, &job).await?;
                }
            } else if job.state.is_terminal()
                && !fs::try_exists(entry.path().join(INTEGRITY_FILE)).await?
            {
                // Upgrade pre-integrity terminal records without rewriting a
                // manifest that already exists and may signal tampering.
                write_integrity_manifest(&self.inner.root, &job).await?;
            }
            jobs.insert(job.id, job);
        }
        Ok(())
    }

    pub async fn submit(&self, request: SubmitJob) -> Result<Job> {
        if !self.inner.accepting.load(Ordering::Acquire) {
            bail!("scheduler is shutting down");
        }
        if request.argv.is_empty() || request.argv[0].is_empty() {
            bail!("argv must contain an executable");
        }
        if request.argv.len() > MAX_ARG_COUNT {
            bail!("argv must contain at most {MAX_ARG_COUNT} arguments");
        }
        let mut command_bytes = 0usize;
        for argument in &request.argv {
            if argument.len() > MAX_ARG_BYTES {
                bail!("each argument must be at most {MAX_ARG_BYTES} bytes");
            }
            command_bytes = command_bytes
                .checked_add(argument.len())
                .context("command size overflow")?;
        }
        if command_bytes > MAX_COMMAND_BYTES {
            bail!("combined argument data must be at most {MAX_COMMAND_BYTES} bytes");
        }
        let timeout_seconds = request
            .timeout_seconds
            .unwrap_or(self.inner.default_timeout);
        if timeout_seconds == 0 || timeout_seconds > 7 * 24 * 60 * 60 {
            bail!("timeout_seconds must be between 1 and 604800");
        }
        if let Some(url) = &request.webhook_url {
            let parsed = reqwest::Url::parse(url).context("invalid webhook_url")?;
            if !parsed.username().is_empty() || parsed.password().is_some() {
                bail!("webhook_url must not contain credentials");
            }
            if parsed.fragment().is_some() {
                bail!("webhook_url must not contain a fragment");
            }
            if parsed.scheme() != "https"
                && !parsed
                    .host_str()
                    .is_some_and(|h| h == "127.0.0.1" || h == "localhost")
            {
                bail!("webhook_url must use HTTPS (HTTP is allowed only for localhost)");
            }
        }
        let tool = request.tool.unwrap_or_else(|| request.argv[0].clone());
        if tool.is_empty() || tool.len() > MAX_TOOL_BYTES || tool.chars().any(char::is_control) {
            bail!("tool must be 1 to {MAX_TOOL_BYTES} bytes without control characters");
        }
        let webhook_configured = request.webhook_url.is_some();
        let mut destinations = request
            .analysis_artifacts
            .iter()
            .map(|artifact| artifact.path.as_str())
            .collect::<Vec<_>>();
        destinations.sort_unstable();
        if destinations.windows(2).any(|pair| pair[0] == pair[1]) {
            bail!("analysis output destinations must be unique");
        }
        let job = Job {
            id: Uuid::new_v4(),
            tool,
            command: display_command(&request.argv, self.inner.reveal_sensitive_data),
            argv: request.argv,
            state: JobState::Queued,
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
            timeout_seconds,
            return_code: None,
            error: None,
            webhook_configured,
            webhook_url: request.webhook_url,
            analysis_artifacts: request.analysis_artifacts,
            analysis_export_error: None,
            stdout_export_path: request.stdout_export_path,
            stderr_export_path: request.stderr_export_path,
        };
        persist_at(&self.inner.root, &job).await?;
        self.inner.jobs.lock().await.insert(job.id, job.clone());
        self.inner.notify.notify_one();
        Ok(job)
    }

    pub async fn get(&self, id: Uuid) -> Option<Job> {
        self.inner.jobs.lock().await.get(&id).cloned()
    }

    pub async fn list(&self) -> Vec<Job> {
        let mut jobs: Vec<_> = self.inner.jobs.lock().await.values().cloned().collect();
        jobs.sort_by_key(|j| std::cmp::Reverse(j.created_at));
        jobs
    }

    pub async fn counts(&self) -> (usize, usize, usize) {
        let jobs = self.inner.jobs.lock().await;
        let queued = jobs
            .values()
            .filter(|j| j.state == JobState::Queued)
            .count();
        let running = jobs
            .values()
            .filter(|j| j.state == JobState::Running)
            .count();
        let max_concurrency = self.inner.dispatch.lock().await.max_concurrency;
        (queued, running, max_concurrency)
    }

    /// Updates the dispatch ceiling without interrupting running jobs.
    pub async fn set_max_concurrency(&self, max_concurrency: usize) -> Result<()> {
        if max_concurrency == 0 || max_concurrency > 256 {
            bail!("max_concurrency must be between 1 and 256");
        }
        self.inner.dispatch.lock().await.max_concurrency = max_concurrency;
        self.inner.notify.notify_one();
        Ok(())
    }

    /// Stops new submissions, cancels queued work, and waits for active job
    /// process groups to terminate.
    pub async fn shutdown(&self) {
        self.begin_shutdown().await;
        self.wait_for_shutdown().await;
    }

    /// Starts graceful shutdown once. Subsequent calls are harmless.
    pub async fn begin_shutdown(&self) {
        if !self.inner.accepting.swap(false, Ordering::AcqRel) {
            return;
        }
        let running = {
            let mut jobs = self.inner.jobs.lock().await;
            let mut running = Vec::new();
            for job in jobs.values_mut() {
                match job.state {
                    JobState::Queued => {
                        job.state = JobState::Cancelled;
                        job.finished_at = Some(Utc::now());
                        job.error = Some("server shutting down".into());
                        if let Err(error) = persist_terminal_at(&self.inner.root, job).await {
                            error!(id = %job.id, %error, "could not persist cancelled job during shutdown");
                        }
                    }
                    JobState::Running | JobState::Paused => running.push(job.id),
                    _ => {}
                }
            }
            running
        };
        let cancellations = self.inner.cancellations.lock().await;
        for id in running {
            if let Some(token) = cancellations.get(&id) {
                token.cancel();
            }
        }
        drop(cancellations);
    }

    /// Waits for active job runners to reach terminal states.
    pub async fn wait_for_shutdown(&self) {
        while self
            .inner
            .jobs
            .lock()
            .await
            .values()
            .any(|job| matches!(job.state, JobState::Running | JobState::Paused))
        {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// Immediately kills active job process groups. Used only when shutdown is
    /// explicitly escalated by a second termination signal.
    pub async fn force_kill_active(&self) {
        self.begin_shutdown().await;
        let process_ids = self
            .inner
            .process_ids
            .lock()
            .await
            .values()
            .copied()
            .collect::<Vec<_>>();
        for pid in process_ids {
            if let Err(error) = signal_process_group(pid, libc::SIGKILL) {
                warn!(%pid, %error, "could not force-kill job process group during shutdown");
            }
        }
        for token in self.inner.cancellations.lock().await.values() {
            token.cancel();
        }
    }

    pub async fn cancel(&self, id: Uuid) -> Result<Job> {
        let state = self
            .get(id)
            .await
            .ok_or_else(|| anyhow!("job not found"))?
            .state;
        match state {
            JobState::Queued => {
                let mut jobs = self.inner.jobs.lock().await;
                let job = jobs.get_mut(&id).ok_or_else(|| anyhow!("job not found"))?;
                job.state = JobState::Cancelled;
                job.finished_at = Some(Utc::now());
                persist_terminal_at(&self.inner.root, job).await?;
            }
            JobState::Running | JobState::Paused => self
                .inner
                .cancellations
                .lock()
                .await
                .get(&id)
                .ok_or_else(|| anyhow!("job is transitioning to running; retry cancellation"))?
                .cancel(),
            _ => bail!("job is already terminal"),
        }
        self.get(id).await.ok_or_else(|| anyhow!("job not found"))
    }

    pub async fn pause(&self, id: Uuid) -> Result<Job> {
        self.signal(id, libc::SIGSTOP, JobState::Running, JobState::Paused)
            .await
    }

    pub async fn resume(&self, id: Uuid) -> Result<Job> {
        self.signal(id, libc::SIGCONT, JobState::Paused, JobState::Running)
            .await
    }

    pub async fn kill(&self, id: Uuid) -> Result<Job> {
        let state = self
            .get(id)
            .await
            .ok_or_else(|| anyhow!("job not found"))?
            .state;
        if state == JobState::Queued {
            return self.cancel(id).await;
        }
        if !matches!(state, JobState::Running | JobState::Paused) {
            bail!("job is already terminal");
        }
        let pid = self
            .inner
            .process_ids
            .lock()
            .await
            .get(&id)
            .copied()
            .ok_or_else(|| anyhow!("job is starting; retry force-kill"))?;
        signal_process_group(pid, libc::SIGKILL)?;
        if let Some(token) = self.inner.cancellations.lock().await.get(&id) {
            token.cancel();
        }
        self.get(id).await.ok_or_else(|| anyhow!("job not found"))
    }

    async fn signal(
        &self,
        id: Uuid,
        signal: i32,
        expected: JobState,
        next: JobState,
    ) -> Result<Job> {
        let state = self
            .get(id)
            .await
            .ok_or_else(|| anyhow!("job not found"))?
            .state;
        if state != expected {
            bail!("job must be {expected:?} to perform this action (currently {state:?})");
        }
        let pid = self
            .inner
            .process_ids
            .lock()
            .await
            .get(&id)
            .copied()
            .ok_or_else(|| anyhow!("job is starting; retry this action"))?;
        signal_process_group(pid, signal)?;
        let mut jobs = self.inner.jobs.lock().await;
        let job = jobs.get_mut(&id).ok_or_else(|| anyhow!("job not found"))?;
        job.state = next;
        persist_at(&self.inner.root, job).await?;
        Ok(job.clone())
    }

    pub async fn output(
        &self,
        id: Uuid,
        stream: &str,
        offset: u64,
        limit: usize,
    ) -> Result<OutputPage> {
        if self.get(id).await.is_none() {
            bail!("job not found");
        }
        if !["stdout", "stderr"].contains(&stream) {
            bail!("stream must be stdout or stderr");
        }
        let limit = limit.clamp(1, MAX_OUTPUT_PAGE);
        let path = self.job_dir(id).join(format!("{stream}.log"));
        let mut file = match fs::File::open(path).await {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(OutputPage {
                    job_id: id,
                    stream: stream.into(),
                    offset,
                    next_offset: offset,
                    truncated: false,
                    data: String::new(),
                });
            }
            Err(error) => return Err(error.into()),
        };
        let size = file.metadata().await?.len();
        let start = offset.min(size);
        file.seek(std::io::SeekFrom::Start(start)).await?;
        let mut bytes = vec![0; limit];
        let read = file.read(&mut bytes).await?;
        bytes.truncate(read);
        Ok(OutputPage {
            job_id: id,
            stream: stream.into(),
            offset: start,
            next_offset: start + read as u64,
            truncated: start + (read as u64) < size,
            data: String::from_utf8_lossy(&bytes).into_owned(),
        })
    }

    /// Opens a log for streaming to an authorized local API caller. The file can
    /// continue growing while a running job writes to it.
    pub async fn open_log(&self, id: Uuid, stream: &str) -> Result<Option<fs::File>> {
        if self.get(id).await.is_none() {
            bail!("job not found");
        }
        if !["stdout", "stderr"].contains(&stream) {
            bail!("stream must be stdout or stderr");
        }
        match fs::File::open(self.job_dir(id).join(format!("{stream}.log"))).await {
            Ok(file) => Ok(Some(file)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn tail(&self, id: Uuid, stream: &str, lines: usize) -> Result<String> {
        if self.get(id).await.is_none() {
            bail!("job not found");
        }
        if !["stdout", "stderr"].contains(&stream) {
            bail!("stream must be stdout or stderr");
        }
        let path = self.job_dir(id).join(format!("{stream}.log"));
        let mut file = match fs::File::open(path).await {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
            Err(error) => return Err(error.into()),
        };
        let size = file.metadata().await?.len();
        let offset = size.saturating_sub(MAX_OUTPUT_PAGE as u64);
        file.seek(std::io::SeekFrom::Start(offset)).await?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).await?;
        let text = String::from_utf8_lossy(&bytes);
        let lines = lines.clamp(1, 500);
        let mut tail: Vec<&str> = text.lines().rev().take(lines).collect();
        tail.reverse();
        Ok(tail.join("\n"))
    }

    fn job_dir(&self, id: Uuid) -> PathBuf {
        self.inner.root.join(id.to_string())
    }

    async fn dispatch(&self) {
        loop {
            self.inner.notify.notified().await;
            loop {
                if !self.inner.accepting.load(Ordering::Acquire) {
                    break;
                }
                let can_dispatch = {
                    let mut dispatch = self.inner.dispatch.lock().await;
                    if dispatch.running >= dispatch.max_concurrency {
                        false
                    } else {
                        dispatch.running += 1;
                        true
                    }
                };
                if !can_dispatch {
                    break;
                }
                let Some(id) = self.next_queued().await else {
                    self.dispatch_finished().await;
                    break;
                };
                let scheduler = self.clone();
                tokio::spawn(async move {
                    if let Err(error) = scheduler.run(id).await {
                        error!(%id, %error, "job runner failed");
                    }
                    scheduler.dispatch_finished().await;
                    scheduler.inner.notify.notify_one();
                });
            }
        }
    }

    async fn dispatch_finished(&self) {
        let mut dispatch = self.inner.dispatch.lock().await;
        dispatch.running = dispatch.running.saturating_sub(1);
    }

    async fn next_queued(&self) -> Option<Uuid> {
        if !self.inner.accepting.load(Ordering::Acquire) {
            return None;
        }
        let mut jobs = self.inner.jobs.lock().await;
        let id = jobs
            .values()
            .filter(|j| j.state == JobState::Queued)
            .min_by_key(|j| j.created_at)?
            .id;
        // Reserving as running here prevents dispatching a queued job twice.
        let job = jobs.get_mut(&id)?;
        job.state = JobState::Running;
        job.started_at = Some(Utc::now());
        self.inner
            .cancellations
            .lock()
            .await
            .insert(id, CancellationToken::new());
        if let Err(error) = persist_at(&self.inner.root, job).await {
            error!(%id, %error, "could not persist running state");
            job.state = JobState::Failed;
            job.error = Some(error.to_string());
            return None;
        }
        Some(id)
    }

    async fn run(&self, id: Uuid) -> Result<()> {
        let job = self
            .get(id)
            .await
            .ok_or_else(|| anyhow!("job disappeared"))?;
        let dir = self.job_dir(id);
        let stdout = create_private_log(&dir.join("stdout.log"))?;
        let stderr = create_private_log(&dir.join("stderr.log"))?;
        let mut command = Command::new(&job.argv[0]);
        command
            .args(&job.argv[1..])
            .stdin(std::process::Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .kill_on_drop(true);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.as_std_mut().process_group(0);
        }
        let token = self
            .inner
            .cancellations
            .lock()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow!("job cancellation token disappeared"))?;
        info!(%id, tool = %job.tool, "job started");

        let outcome = match command.spawn() {
            Err(error) => (
                JobState::Failed,
                None,
                Some(format!("failed to start {}: {error}", job.argv[0])),
            ),
            Ok(mut child) => {
                if let Some(pid) = child.id() {
                    self.inner.process_ids.lock().await.insert(id, pid as i32);
                }
                tokio::select! {
                    _ = token.cancelled() => {
                        let grace = if self.inner.accepting.load(Ordering::Acquire) {
                            JOB_TERMINATION_GRACE
                        } else {
                            SHUTDOWN_TERMINATION_GRACE
                        };
                        terminate(&mut child, grace).await;
                        (JobState::Cancelled, None, None)
                    }
                    result = timeout(Duration::from_secs(job.timeout_seconds), child.wait()) => match result {
                        Err(_) => {
                            terminate(&mut child, JOB_TERMINATION_GRACE).await;
                            (JobState::TimedOut, None, Some(format!("timed out after {} seconds", job.timeout_seconds)))
                        }
                        Ok(Err(error)) => (JobState::Failed, None, Some(error.to_string())),
                        Ok(Ok(status)) if status.success() => (JobState::Succeeded, status.code(), None),
                        Ok(Ok(status)) => (JobState::Failed, status.code(), None),
                    }
                }
            }
        };
        self.inner.cancellations.lock().await.remove(&id);
        self.inner.process_ids.lock().await.remove(&id);
        let analysis_export_error = self
            .export_streams(&job)
            .await
            .err()
            .map(|error| error.to_string());
        let completed = {
            let mut jobs = self.inner.jobs.lock().await;
            let job = jobs
                .get_mut(&id)
                .ok_or_else(|| anyhow!("job disappeared"))?;
            job.state = outcome.0;
            job.return_code = outcome.1;
            job.error = outcome.2;
            job.analysis_export_error = analysis_export_error;
            job.finished_at = Some(Utc::now());
            persist_terminal_at(&self.inner.root, job).await?;
            job.clone()
        };
        let duration_ms = completed
            .started_at
            .zip(completed.finished_at)
            .map(|(started, finished)| (finished - started).num_milliseconds().max(0));
        info!(
            %id,
            tool = %completed.tool,
            state = ?completed.state,
            duration_ms,
            "job finished"
        );
        self.send_webhook(&completed).await;
        Ok(())
    }

    async fn export_streams(&self, job: &Job) -> Result<()> {
        let mut failures = Vec::new();
        for (stream, destination) in [
            ("stdout", job.stdout_export_path.as_ref()),
            ("stderr", job.stderr_export_path.as_ref()),
        ] {
            let Some(destination) = destination else {
                continue;
            };
            let source = self.job_dir(job.id).join(format!("{stream}.log"));
            if let Err(error) = self.inner.analysis.copy_file(&source, destination).await {
                failures.push(format!("{stream}: {error}"));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            bail!("{}", failures.join("; "))
        }
    }

    async fn send_webhook(&self, job: &Job) {
        let Some(url) = &job.webhook_url else { return };
        match self
            .inner
            .webhook_client
            .post(url)
            .json(job)
            .timeout(Duration::from_secs(10))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                info!(id = %job.id, "webhook delivered")
            }
            Ok(response) => warn!(id = %job.id, status = %response.status(), "webhook rejected"),
            Err(error) => {
                let error = error.without_url();
                warn!(id = %job.id, %error, "webhook delivery failed")
            }
        }
    }
}

async fn terminate(child: &mut tokio::process::Child, grace: Duration) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // The child starts its own process group so scanner descendants do not
        // survive cancellation or timeout as orphaned processes.
        unsafe { libc::kill(-(pid as i32), libc::SIGTERM) };
        if timeout(grace, child.wait()).await.is_err() {
            unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
            let _ = child.wait().await;
        }
        return;
    }
    let _ = child.kill().await;
}

fn signal_process_group(pid: i32, signal: i32) -> Result<()> {
    #[cfg(unix)]
    {
        // Every child starts a new process group, so job controls include scanner
        // subprocesses rather than leaving descendants behind.
        if unsafe { libc::kill(-pid, signal) } == 0 {
            return Ok(());
        }
        Err(std::io::Error::last_os_error().into())
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, signal);
        bail!("pause, resume, and force-kill require Unix process groups")
    }
}

fn display_command(argv: &[String], reveal_sensitive_data: bool) -> String {
    let mut redact_next = false;
    argv.iter()
        .map(|argument| {
            let value = if reveal_sensitive_data {
                argument.clone()
            } else if redact_next {
                redact_next = false;
                "[REDACTED]".to_owned()
            } else if secret_flag(argv.first().map(String::as_str), argument) {
                redact_next = true;
                argument.clone()
            } else if argument.starts_with("--password=") || argument.starts_with("--data=") {
                format!(
                    "{}[REDACTED]",
                    argument
                        .split_once('=')
                        .map(|(prefix, _)| format!("{prefix}="))
                        .unwrap_or_default()
                )
            } else {
                argument.clone()
            };
            shell_quote(&value)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn secret_flag(program: Option<&str>, argument: &str) -> bool {
    const SECRET_FLAGS: &[&str] = &["-P", "--password", "--password-file", "--data", "-x"];
    SECRET_FLAGS.contains(&argument)
        || (argument == "-p"
            && program
                .and_then(|program| Path::new(program).file_name())
                .and_then(|program| program.to_str())
                .is_some_and(|program| matches!(program, "hydra" | "medusa")))
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_+-./:=@".contains(c))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

async fn persist_terminal_at(root: &Path, job: &Job) -> Result<()> {
    persist_at(root, job).await?;
    write_integrity_manifest(root, job).await
}

async fn write_integrity_manifest(root: &Path, job: &Job) -> Result<()> {
    let dir = root.join(job.id.to_string());
    let mut files = Vec::new();
    for name in ["job.json", "command.json", "stdout.log", "stderr.log"] {
        let path = dir.join(name);
        if !fs::try_exists(&path).await? {
            continue;
        }
        let (bytes, sha256) = sha256_file(&path).await?;
        files.push(JobIntegrityFile {
            path: name.into(),
            bytes,
            sha256,
        });
    }
    if !files.iter().any(|file| file.path == "job.json")
        || !files.iter().any(|file| file.path == "command.json")
    {
        bail!("terminal job {} is missing required evidence files", job.id);
    }
    let manifest = JobIntegrityManifest {
        version: 1,
        algorithm: INTEGRITY_ALGORITHM.into(),
        generated_at: Utc::now(),
        job_id: job.id,
        files,
    };
    write_private(
        &dir.join(INTEGRITY_FILE),
        &serde_json::to_vec_pretty(&manifest)?,
    )
    .await
}

async fn ensure_integrity_manifest(root: &Path, job: &Job) -> Result<()> {
    let path = root.join(job.id.to_string()).join(INTEGRITY_FILE);
    if !fs::try_exists(&path).await? {
        // Compatibility for terminal jobs created before integrity manifests.
        write_integrity_manifest(root, job).await?;
    }
    let bytes = fs::read(&path).await?;
    let manifest = serde_json::from_slice::<JobIntegrityManifest>(&bytes)
        .context("read integrity manifest")?;
    if manifest.version != 1
        || manifest.algorithm != INTEGRITY_ALGORITHM
        || manifest.job_id != job.id
    {
        bail!("integrity manifest metadata is invalid");
    }
    if manifest.files.is_empty() {
        bail!("integrity manifest contains no evidence files");
    }
    let dir = root.join(job.id.to_string());
    let mut seen = std::collections::HashSet::new();
    for file in manifest.files {
        if !matches!(
            file.path.as_str(),
            "job.json" | "command.json" | "stdout.log" | "stderr.log"
        ) || !seen.insert(file.path.clone())
        {
            bail!("integrity manifest contains an invalid file entry");
        }
        let (bytes, sha256) = sha256_file(&dir.join(&file.path)).await?;
        if bytes != file.bytes || sha256 != file.sha256 {
            bail!("checksum mismatch for {}", file.path);
        }
    }
    if !seen.contains("job.json") || !seen.contains("command.json") {
        bail!("integrity manifest omits required evidence files");
    }
    Ok(())
}

async fn sha256_file(path: &Path) -> Result<(u64, String)> {
    let mut file = fs::File::open(path)
        .await
        .with_context(|| format!("open {} for hashing", path.display()))?;
    let mut hasher = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        bytes = bytes.saturating_add(read as u64);
    }
    Ok((bytes, format!("{:x}", hasher.finalize())))
}

fn create_gzip_archive(destination: &Path, sources: &[(Uuid, PathBuf)]) -> Result<()> {
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .context("archive destination must have a UTF-8 filename")?;
    let temporary = destination.with_file_name(format!(".{file_name}.{}.tmp", Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut options = stdfs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let output = options.open(&temporary)?;
        let encoder = GzEncoder::new(output, Compression::default());
        let mut archive = tar::Builder::new(encoder);
        for (id, source) in sources {
            archive.append_dir(id.to_string(), source)?;
            let mut entries =
                stdfs::read_dir(source)?.collect::<std::result::Result<Vec<_>, _>>()?;
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let file_type = entry.file_type()?;
                if !file_type.is_file() {
                    bail!(
                        "refusing non-regular archive entry {}",
                        entry.path().display()
                    );
                }
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| anyhow!("archive entry name is not UTF-8"))?;
                archive.append_path_with_name(entry.path(), format!("{id}/{name}"))?;
            }
        }
        let output = archive.into_inner()?.finish()?;
        output.sync_all()?;
        stdfs::rename(&temporary, destination)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = stdfs::remove_file(&temporary);
    }
    result
}

async fn persist_at(root: &Path, job: &Job) -> Result<()> {
    let dir = root.join(job.id.to_string());
    fs::create_dir_all(&dir).await?;
    #[cfg(unix)]
    fs::set_permissions(&dir, std::os::unix::fs::PermissionsExt::from_mode(0o700)).await?;
    let final_path = dir.join("job.json");
    let temporary = dir.join("job.json.tmp");
    write_private(&temporary, &serde_json::to_vec_pretty(job)?).await?;
    fs::rename(temporary, final_path).await?;
    let private = PrivateJobSpec {
        argv: job.argv.clone(),
        webhook_url: job.webhook_url.clone(),
        stdout_export_path: job.stdout_export_path.clone(),
        stderr_export_path: job.stderr_export_path.clone(),
    };
    write_private(&dir.join("command.json"), &serde_json::to_vec(&private)?).await?;
    Ok(())
}

async fn write_private(path: &Path, contents: &[u8]) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path).await?;
    #[cfg(unix)]
    file.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))
        .await?;
    file.write_all(contents).await?;
    file.flush().await?;
    Ok(())
}

fn create_private_log(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let file = options.open(path)?;
    #[cfg(unix)]
    file.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn executes_and_pages_output() {
        let temp = tempfile::tempdir().unwrap();
        let scheduler = Scheduler::open(temp.path().into(), 1, 10).await.unwrap();
        let job = scheduler
            .submit(SubmitJob {
                tool: None,
                argv: vec!["printf".into(), "hello".into()],
                timeout_seconds: None,
                webhook_url: None,
                stdout_export_path: None,
                stderr_export_path: None,
                analysis_artifacts: Vec::new(),
            })
            .await
            .unwrap();
        for _ in 0..100 {
            if scheduler.get(job.id).await.unwrap().state.is_terminal() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            scheduler.get(job.id).await.unwrap().state,
            JobState::Succeeded
        );
        assert_eq!(
            scheduler
                .output(job.id, "stdout", 0, 100)
                .await
                .unwrap()
                .data,
            "hello"
        );
        let mut log = scheduler.open_log(job.id, "stdout").await.unwrap().unwrap();
        let mut downloaded = String::new();
        log.read_to_string(&mut downloaded).await.unwrap();
        assert_eq!(downloaded, "hello");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(temp.path()), 0o700);
            assert_eq!(mode(&scheduler.job_dir(job.id)), 0o700);
            assert_eq!(mode(&scheduler.job_dir(job.id).join("job.json")), 0o600);
            assert_eq!(mode(&scheduler.job_dir(job.id).join("command.json")), 0o600);
            assert_eq!(mode(&scheduler.job_dir(job.id).join("stdout.log")), 0o600);
            assert_eq!(mode(&scheduler.job_dir(job.id).join("stderr.log")), 0o600);
        }
    }

    #[tokio::test]
    async fn exports_captured_stream_without_removing_job_output() {
        let temp = tempfile::tempdir().unwrap();
        let scheduler = Scheduler::open(temp.path().into(), 1, 10).await.unwrap();
        let destination = scheduler.resolve_analysis_file("scans/stdout.txt").unwrap();
        let job = scheduler
            .submit(SubmitJob {
                tool: None,
                argv: vec!["printf".into(), "saved output".into()],
                timeout_seconds: None,
                webhook_url: None,
                stdout_export_path: Some(destination.clone()),
                stderr_export_path: None,
                analysis_artifacts: vec![crate::models::AnalysisArtifact {
                    kind: "stdout_export".into(),
                    path: destination.display().to_string(),
                }],
            })
            .await
            .unwrap();
        for _ in 0..100 {
            if scheduler.get(job.id).await.unwrap().state.is_terminal() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let completed = scheduler.get(job.id).await.unwrap();
        assert_eq!(completed.state, JobState::Succeeded);
        assert!(completed.analysis_export_error.is_none());
        assert_eq!(
            fs::read_to_string(destination).await.unwrap(),
            "saved output"
        );
        assert_eq!(
            scheduler
                .output(job.id, "stdout", 0, 100)
                .await
                .unwrap()
                .data,
            "saved output"
        );
    }

    #[tokio::test]
    async fn archives_only_old_terminal_jobs_and_preserves_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let job_root = temp.path().join("jobs");
        let archive_root = temp.path().join("archive/jobs");
        let scheduler =
            Scheduler::open_with_archive(job_root.clone(), archive_root.clone(), 1, 10, 60, false)
                .await
                .unwrap();
        let terminal = scheduler
            .submit(SubmitJob {
                tool: None,
                argv: vec!["printf".into(), "archive-me".into()],
                timeout_seconds: None,
                webhook_url: None,
                stdout_export_path: None,
                stderr_export_path: None,
                analysis_artifacts: Vec::new(),
            })
            .await
            .unwrap();
        for _ in 0..100 {
            if scheduler
                .get(terminal.id)
                .await
                .unwrap()
                .state
                .is_terminal()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        {
            let mut jobs = scheduler.inner.jobs.lock().await;
            let job = jobs.get_mut(&terminal.id).unwrap();
            job.finished_at = Some(Utc::now() - chrono::Duration::minutes(2));
            persist_terminal_at(&job_root, job).await.unwrap();
        }

        let active = scheduler
            .submit(SubmitJob {
                tool: None,
                argv: vec!["sleep".into(), "1".into()],
                timeout_seconds: None,
                webhook_url: None,
                stdout_export_path: None,
                stderr_export_path: None,
                analysis_artifacts: Vec::new(),
            })
            .await
            .unwrap();
        for _ in 0..100 {
            if scheduler.get(active.id).await.unwrap().state == JobState::Running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let preview = scheduler.preview_archive(1).await.unwrap();
        assert_eq!(preview.matched, 1);
        assert!(preview.bytes > 0);
        let result = scheduler.archive_terminal_jobs(1).await.unwrap();
        assert_eq!(result.matched, 1);
        assert_eq!(result.archived, 1);
        assert_eq!(result.failed, 0);
        assert!(result.bytes_archived > 0);
        assert!(scheduler.get(terminal.id).await.is_none());
        assert!(scheduler.get(active.id).await.is_some());
        assert!(!job_root.join(terminal.id.to_string()).exists());
        let archive_name = result.archive_file.as_ref().unwrap();
        assert!(archive_name.starts_with("jobs_"));
        assert!(archive_name.ends_with(".tar.gz"));
        let archive_path = archive_root.join(archive_name);
        assert!(archive_path.is_file());
        let archive_file = std::fs::File::open(archive_path).unwrap();
        let decoder = flate2::read::GzDecoder::new(archive_file);
        let mut archive = tar::Archive::new(decoder);
        let paths = archive
            .entries()
            .unwrap()
            .map(|entry| entry.unwrap().path().unwrap().into_owned())
            .collect::<Vec<_>>();
        let prefix = terminal.id.to_string();
        for name in [
            "job.json",
            "command.json",
            "stdout.log",
            "stderr.log",
            INTEGRITY_FILE,
        ] {
            assert!(paths.contains(&PathBuf::from(format!("{prefix}/{name}"))));
        }
        assert!(scheduler.preview_archive(0).await.is_err());

        scheduler.cancel(active.id).await.unwrap();
        scheduler.shutdown().await;
    }

    #[tokio::test]
    async fn archive_refuses_a_terminal_job_with_modified_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let job_root = temp.path().join("jobs");
        let archive_root = temp.path().join("archive/jobs");
        let scheduler =
            Scheduler::open_with_archive(job_root.clone(), archive_root, 1, 10, 60, false)
                .await
                .unwrap();
        let job = scheduler
            .submit(SubmitJob {
                tool: None,
                argv: vec!["printf".into(), "original".into()],
                timeout_seconds: None,
                webhook_url: None,
                stdout_export_path: None,
                stderr_export_path: None,
                analysis_artifacts: Vec::new(),
            })
            .await
            .unwrap();
        for _ in 0..100 {
            if scheduler.get(job.id).await.unwrap().state.is_terminal() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        {
            let mut jobs = scheduler.inner.jobs.lock().await;
            let terminal = jobs.get_mut(&job.id).unwrap();
            terminal.finished_at = Some(Utc::now() - chrono::Duration::minutes(2));
            persist_terminal_at(&job_root, terminal).await.unwrap();
        }
        fs::write(scheduler.job_dir(job.id).join("stdout.log"), b"modified")
            .await
            .unwrap();

        let result = scheduler.archive_terminal_jobs(1).await.unwrap();
        assert_eq!(result.matched, 1);
        assert_eq!(result.archived, 0);
        assert_eq!(result.failed, 1);
        assert!(result.archive_file.is_none());
        assert!(scheduler.get(job.id).await.is_some());

        scheduler.shutdown().await;
    }

    #[tokio::test]
    async fn queued_job_can_be_cancelled_without_starting() {
        let temp = tempfile::tempdir().unwrap();
        let scheduler = Scheduler::open(temp.path().into(), 1, 10).await.unwrap();
        let first = scheduler
            .submit(SubmitJob {
                tool: None,
                argv: vec!["sleep".into(), "0.2".into()],
                timeout_seconds: None,
                webhook_url: None,
                stdout_export_path: None,
                stderr_export_path: None,
                analysis_artifacts: Vec::new(),
            })
            .await
            .unwrap();
        let second = scheduler
            .submit(SubmitJob {
                tool: None,
                argv: vec!["printf".into(), "must-not-run".into()],
                timeout_seconds: None,
                webhook_url: None,
                stdout_export_path: None,
                stderr_export_path: None,
                analysis_artifacts: Vec::new(),
            })
            .await
            .unwrap();
        for _ in 0..100 {
            if scheduler.get(first.id).await.unwrap().state == JobState::Running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(
            scheduler.get(second.id).await.unwrap().state,
            JobState::Queued
        );
        scheduler.cancel(second.id).await.unwrap();
        assert_eq!(
            scheduler.get(second.id).await.unwrap().state,
            JobState::Cancelled
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn running_job_can_be_paused_resumed_and_force_killed() {
        let temp = tempfile::tempdir().unwrap();
        let scheduler = Scheduler::open(temp.path().into(), 1, 10).await.unwrap();
        let job = scheduler
            .submit(SubmitJob {
                tool: None,
                argv: vec!["sleep".into(), "10".into()],
                timeout_seconds: None,
                webhook_url: None,
                stdout_export_path: None,
                stderr_export_path: None,
                analysis_artifacts: Vec::new(),
            })
            .await
            .unwrap();
        for _ in 0..100 {
            if scheduler.get(job.id).await.unwrap().state == JobState::Running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        for _ in 0..100 {
            if scheduler.pause(job.id).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(scheduler.get(job.id).await.unwrap().state, JobState::Paused);
        scheduler.resume(job.id).await.unwrap();
        assert_eq!(
            scheduler.get(job.id).await.unwrap().state,
            JobState::Running
        );
        scheduler.kill(job.id).await.unwrap();
        for _ in 0..100 {
            if scheduler.get(job.id).await.unwrap().state.is_terminal() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            scheduler.get(job.id).await.unwrap().state,
            JobState::Cancelled
        );
    }

    #[tokio::test]
    async fn concurrency_can_increase_without_interrupting_running_jobs() {
        let temp = tempfile::tempdir().unwrap();
        let scheduler = Scheduler::open(temp.path().into(), 1, 10).await.unwrap();
        let first = scheduler
            .submit(SubmitJob {
                tool: None,
                argv: vec!["sleep".into(), "1".into()],
                timeout_seconds: None,
                webhook_url: None,
                stdout_export_path: None,
                stderr_export_path: None,
                analysis_artifacts: Vec::new(),
            })
            .await
            .unwrap();
        let second = scheduler
            .submit(SubmitJob {
                tool: None,
                argv: vec!["sleep".into(), "1".into()],
                timeout_seconds: None,
                webhook_url: None,
                stdout_export_path: None,
                stderr_export_path: None,
                analysis_artifacts: Vec::new(),
            })
            .await
            .unwrap();
        for _ in 0..100 {
            if scheduler.get(first.id).await.unwrap().state == JobState::Running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(
            scheduler.get(second.id).await.unwrap().state,
            JobState::Queued
        );
        scheduler.set_max_concurrency(2).await.unwrap();
        for _ in 0..100 {
            if scheduler.get(second.id).await.unwrap().state == JobState::Running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(
            scheduler.get(first.id).await.unwrap().state,
            JobState::Running
        );
        assert_eq!(
            scheduler.get(second.id).await.unwrap().state,
            JobState::Running
        );
        assert_eq!(scheduler.counts().await.2, 2);
        scheduler.shutdown().await;
    }

    #[tokio::test]
    async fn lowering_concurrency_waits_for_running_jobs_to_drain() {
        let temp = tempfile::tempdir().unwrap();
        let scheduler = Scheduler::open(temp.path().into(), 2, 10).await.unwrap();
        let first = scheduler
            .submit(SubmitJob {
                tool: None,
                argv: vec!["sleep".into(), "0.2".into()],
                timeout_seconds: None,
                webhook_url: None,
                stdout_export_path: None,
                stderr_export_path: None,
                analysis_artifacts: Vec::new(),
            })
            .await
            .unwrap();
        let second = scheduler
            .submit(SubmitJob {
                tool: None,
                argv: vec!["sleep".into(), "1".into()],
                timeout_seconds: None,
                webhook_url: None,
                stdout_export_path: None,
                stderr_export_path: None,
                analysis_artifacts: Vec::new(),
            })
            .await
            .unwrap();
        for _ in 0..100 {
            if scheduler.get(first.id).await.unwrap().state == JobState::Running
                && scheduler.get(second.id).await.unwrap().state == JobState::Running
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        scheduler.set_max_concurrency(1).await.unwrap();
        let third = scheduler
            .submit(SubmitJob {
                tool: None,
                argv: vec!["sleep".into(), "1".into()],
                timeout_seconds: None,
                webhook_url: None,
                stdout_export_path: None,
                stderr_export_path: None,
                analysis_artifacts: Vec::new(),
            })
            .await
            .unwrap();
        for _ in 0..100 {
            if scheduler.get(first.id).await.unwrap().state.is_terminal() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(
            scheduler.get(second.id).await.unwrap().state,
            JobState::Running
        );
        assert_eq!(
            scheduler.get(third.id).await.unwrap().state,
            JobState::Queued
        );
        scheduler.shutdown().await;
    }

    #[tokio::test]
    async fn shutdown_cancels_queued_and_running_jobs_and_rejects_submissions() {
        let temp = tempfile::tempdir().unwrap();
        let scheduler = Scheduler::open(temp.path().into(), 1, 10).await.unwrap();
        let running = scheduler
            .submit(SubmitJob {
                tool: None,
                argv: vec!["sleep".into(), "10".into()],
                timeout_seconds: None,
                webhook_url: None,
                stdout_export_path: None,
                stderr_export_path: None,
                analysis_artifacts: Vec::new(),
            })
            .await
            .unwrap();
        let queued = scheduler
            .submit(SubmitJob {
                tool: None,
                argv: vec!["sleep".into(), "10".into()],
                timeout_seconds: None,
                webhook_url: None,
                stdout_export_path: None,
                stderr_export_path: None,
                analysis_artifacts: Vec::new(),
            })
            .await
            .unwrap();
        for _ in 0..100 {
            if scheduler.get(running.id).await.unwrap().state == JobState::Running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        scheduler.shutdown().await;
        assert_eq!(
            scheduler.get(running.id).await.unwrap().state,
            JobState::Cancelled
        );
        assert_eq!(
            scheduler.get(queued.id).await.unwrap().state,
            JobState::Cancelled
        );
        assert!(
            scheduler
                .submit(SubmitJob {
                    tool: None,
                    argv: vec!["true".into()],
                    timeout_seconds: None,
                    webhook_url: None,
                    stdout_export_path: None,
                    stderr_export_path: None,
                    analysis_artifacts: Vec::new(),
                })
                .await
                .is_err()
        );
    }

    #[test]
    fn command_display_redacts_by_default_and_can_reveal() {
        let argv = vec!["hydra".into(), "-p".into(), "super-secret".into()];
        assert_eq!(display_command(&argv, false), "hydra -p '[REDACTED]'");
        assert_eq!(display_command(&argv, true), "hydra -p super-secret");

        let nmap = vec!["nmap".into(), "-p".into(), "22".into()];
        assert_eq!(display_command(&nmap, false), "nmap -p 22");
    }

    #[tokio::test]
    async fn rejects_oversized_job_submissions() {
        let temp = tempfile::tempdir().unwrap();
        let scheduler = Scheduler::open(temp.path().into(), 1, 10).await.unwrap();
        let result = scheduler
            .submit(SubmitJob {
                tool: None,
                argv: vec!["printf".into(), "x".repeat(MAX_ARG_BYTES + 1)],
                timeout_seconds: None,
                webhook_url: None,
                stdout_export_path: None,
                stderr_export_path: None,
                analysis_artifacts: Vec::new(),
            })
            .await;
        assert!(result.is_err());
        let error = result.err().unwrap();
        assert!(error.to_string().contains("each argument"));
    }
}
