use crate::models::{Job, JobState, OutputPage, SubmitJob};
use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    process::Command,
    sync::{Mutex, Notify, Semaphore},
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

#[derive(Serialize, Deserialize)]
struct PrivateJobSpec {
    argv: Vec<String>,
    #[serde(default)]
    webhook_url: Option<String>,
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
    jobs: Mutex<HashMap<Uuid, Job>>,
    cancellations: Mutex<HashMap<Uuid, CancellationToken>>,
    process_ids: Mutex<HashMap<Uuid, i32>>,
    permits: Arc<Semaphore>,
    notify: Notify,
    default_timeout: u64,
    max_concurrency: usize,
    reveal_sensitive_data: bool,
    webhook_client: reqwest::Client,
}

impl Scheduler {
    pub async fn open(root: PathBuf, max_concurrency: usize, default_timeout: u64) -> Result<Self> {
        Self::open_with_sensitive_data(root, max_concurrency, default_timeout, false).await
    }

    /// Starts a scheduler with explicit control over public command redaction.
    /// The execution specification remains private on disk in either mode.
    pub async fn open_with_sensitive_data(
        root: PathBuf,
        max_concurrency: usize,
        default_timeout: u64,
        reveal_sensitive_data: bool,
    ) -> Result<Self> {
        if max_concurrency == 0 {
            bail!("max_concurrency must be greater than zero");
        }
        fs::create_dir_all(&root)
            .await
            .context("create job state directory")?;
        #[cfg(unix)]
        fs::set_permissions(&root, std::os::unix::fs::PermissionsExt::from_mode(0o700))
            .await
            .context("secure job state directory")?;
        let scheduler = Self {
            inner: Arc::new(Inner {
                root,
                jobs: Mutex::new(HashMap::new()),
                cancellations: Mutex::new(HashMap::new()),
                process_ids: Mutex::new(HashMap::new()),
                permits: Arc::new(Semaphore::new(max_concurrency)),
                notify: Notify::new(),
                default_timeout,
                max_concurrency,
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
                persist_at(&self.inner.root, &job).await?;
            }
            jobs.insert(job.id, job);
        }
        Ok(())
    }

    pub async fn submit(&self, request: SubmitJob) -> Result<Job> {
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
        (queued, running, self.inner.max_concurrency)
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
                persist_at(&self.inner.root, job).await?;
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
                let Ok(permit) = self.inner.permits.clone().acquire_owned().await else {
                    return;
                };
                let Some(id) = self.next_queued().await else {
                    drop(permit);
                    break;
                };
                let scheduler = self.clone();
                tokio::spawn(async move {
                    if let Err(error) = scheduler.run(id).await {
                        error!(%id, %error, "job runner failed");
                    }
                    drop(permit);
                    scheduler.inner.notify.notify_one();
                });
            }
        }
    }

    async fn next_queued(&self) -> Option<Uuid> {
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
                        terminate(&mut child).await;
                        (JobState::Cancelled, None, None)
                    }
                    result = timeout(Duration::from_secs(job.timeout_seconds), child.wait()) => match result {
                        Err(_) => {
                            terminate(&mut child).await;
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
        let completed = {
            let mut jobs = self.inner.jobs.lock().await;
            let job = jobs
                .get_mut(&id)
                .ok_or_else(|| anyhow!("job disappeared"))?;
            job.state = outcome.0;
            job.return_code = outcome.1;
            job.error = outcome.2;
            job.finished_at = Some(Utc::now());
            persist_at(&self.inner.root, job).await?;
            job.clone()
        };
        info!(%id, state = ?completed.state, "job finished");
        self.send_webhook(&completed).await;
        Ok(())
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

async fn terminate(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // The child starts its own process group so scanner descendants do not
        // survive cancellation or timeout as orphaned processes.
        unsafe { libc::kill(-(pid as i32), libc::SIGTERM) };
        if timeout(Duration::from_secs(5), child.wait()).await.is_err() {
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
    const SECRET_FLAGS: &[&str] = &["-p", "-P", "--password", "--password-file", "--data", "-x"];
    let mut redact_next = false;
    argv.iter()
        .map(|argument| {
            let value = if reveal_sensitive_data {
                argument.clone()
            } else if redact_next {
                redact_next = false;
                "[REDACTED]".to_owned()
            } else if SECRET_FLAGS.contains(&argument.as_str()) {
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
    async fn queued_job_can_be_cancelled_without_starting() {
        let temp = tempfile::tempdir().unwrap();
        let scheduler = Scheduler::open(temp.path().into(), 1, 10).await.unwrap();
        let first = scheduler
            .submit(SubmitJob {
                tool: None,
                argv: vec!["sleep".into(), "0.2".into()],
                timeout_seconds: None,
                webhook_url: None,
            })
            .await
            .unwrap();
        let second = scheduler
            .submit(SubmitJob {
                tool: None,
                argv: vec!["printf".into(), "must-not-run".into()],
                timeout_seconds: None,
                webhook_url: None,
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

    #[test]
    fn command_display_redacts_by_default_and_can_reveal() {
        let argv = vec!["hydra".into(), "-p".into(), "super-secret".into()];
        assert_eq!(display_command(&argv, false), "hydra -p '[REDACTED]'");
        assert_eq!(display_command(&argv, true), "hydra -p super-secret");
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
            })
            .await;
        assert!(result.is_err());
        let error = result.err().unwrap();
        assert!(error.to_string().contains("each argument"));
    }
}
