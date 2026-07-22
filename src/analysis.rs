use anyhow::{Context, Result, bail};
use std::{
    env,
    path::{Component, Path, PathBuf},
    sync::Arc,
};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
};

const MAX_ANALYSIS_PATH_BYTES: usize = 4096;

/// A configured output tree for operator-requested analysis artifacts.
///
/// The root is canonical and every resolved destination is constrained beneath
/// it. Existing symlinks in destination paths are rejected.
#[derive(Clone, Debug)]
pub struct AnalysisRoot {
    root: Arc<PathBuf>,
}

impl AnalysisRoot {
    /// Opens a projects root beneath the running user's HOME directory.
    pub fn open(configured: &Path) -> Result<Self> {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .context("HOME must be set to configure analysis outputs")?;
        Self::open_with_home(configured, &home)
    }

    fn open_with_home(configured: &Path, home: &Path) -> Result<Self> {
        let configured_home = home.to_path_buf();
        let home = home
            .canonicalize()
            .with_context(|| format!("resolve running user home {}", home.display()))?;
        let configured = expand_home(configured, &configured_home);
        let relative = if configured.is_absolute() {
            configured
                .strip_prefix(&configured_home)
                .or_else(|_| configured.strip_prefix(&home))
                .with_context(|| {
                    format!(
                        "projects directory {} must be within the running user home {}",
                        configured.display(),
                        home.display()
                    )
                })?
        } else {
            configured.as_path()
        };
        if relative.as_os_str().is_empty() {
            bail!("projects directory must be a folder beneath the running user home");
        }
        validate_relative_path(relative, "projects directory")?;
        let root = ensure_directories(&home, relative)?;
        if !root.starts_with(&home) || root == home {
            bail!("projects directory escaped the running user home");
        }
        Ok(Self {
            root: Arc::new(root),
        })
    }

    /// Opens an isolated root without applying the process HOME policy for
    /// compatibility scheduler constructors and tests inside this crate.
    pub(crate) fn open_isolated(configured: &Path) -> Result<Self> {
        if configured.as_os_str().is_empty() || configured.parent().is_none() {
            bail!("projects directory must not be empty or a filesystem root");
        }
        std::fs::create_dir_all(configured)
            .with_context(|| format!("create projects directory {}", configured.display()))?;
        set_private_directory(configured)?;
        let root = configured
            .canonicalize()
            .with_context(|| format!("resolve projects directory {}", configured.display()))?;
        Ok(Self {
            root: Arc::new(root),
        })
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    /// Resolves a relative path, or an absolute path already beneath the root,
    /// and creates private parent directories as needed.
    pub fn resolve_file(&self, requested: &str) -> Result<PathBuf> {
        if requested.is_empty()
            || requested.len() > MAX_ANALYSIS_PATH_BYTES
            || requested.chars().any(char::is_control)
        {
            bail!(
                "analysis output path must be 1 to {MAX_ANALYSIS_PATH_BYTES} bytes without control characters"
            );
        }
        let requested = Path::new(requested);
        let relative = if requested.is_absolute() {
            requested.strip_prefix(self.path()).with_context(|| {
                format!(
                    "analysis output {} must remain within {}",
                    requested.display(),
                    self.path().display()
                )
            })?
        } else {
            requested
        };
        validate_relative_path(relative, "analysis output path")?;
        let file_name = relative
            .file_name()
            .context("analysis output path must name a file")?;
        let parent = relative.parent().unwrap_or_else(|| Path::new(""));
        let directory = ensure_directories(self.path(), parent)?;
        let destination = directory.join(file_name);
        match std::fs::symlink_metadata(&destination) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!(
                    "analysis output must not be a symlink: {}",
                    destination.display()
                )
            }
            Ok(metadata) if metadata.is_dir() => {
                bail!(
                    "analysis output must name a file: {}",
                    destination.display()
                )
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("inspect analysis output destination"),
        }
        Ok(destination)
    }

    /// Copies a captured job stream to an already-resolved analysis file while
    /// refusing a symlink at the final destination.
    pub async fn copy_file(&self, source: &Path, destination: &Path) -> Result<u64> {
        let destination_text = destination
            .to_str()
            .context("analysis output destination is not UTF-8")?;
        let destination = self.resolve_file(destination_text)?;
        let mut input = fs::File::open(source)
            .await
            .with_context(|| format!("open captured output {}", source.display()))?;
        let mut options = fs::OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let mut output = options
            .open(&destination)
            .await
            .with_context(|| format!("open analysis output {}", destination.display()))?;
        #[cfg(unix)]
        output
            .set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))
            .await?;
        let mut copied = 0u64;
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let read = input.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            output.write_all(&buffer[..read]).await?;
            copied = copied.saturating_add(read as u64);
        }
        output.flush().await?;
        Ok(copied)
    }
}

fn expand_home(path: &Path, home: &Path) -> PathBuf {
    let value = path.to_string_lossy();
    if value == "~" {
        return home.to_path_buf();
    }
    value
        .strip_prefix("~/")
        .map(|rest| home.join(rest))
        .unwrap_or_else(|| path.to_path_buf())
}

fn validate_relative_path(path: &Path, label: &str) -> Result<()> {
    if path.as_os_str().is_empty() {
        return Ok(());
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("{label} must not contain '.', '..', a root, or a platform prefix");
    }
    Ok(())
}

fn ensure_directories(root: &Path, relative: &Path) -> Result<PathBuf> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            bail!("projects directory contains an unsafe path component");
        };
        current.push(name);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!(
                    "projects directory must not traverse a symlink: {}",
                    current.display()
                )
            }
            Ok(metadata) if !metadata.is_dir() => {
                bail!(
                    "projects directory component is not a directory: {}",
                    current.display()
                )
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&current)
                    .with_context(|| format!("create projects directory {}", current.display()))?;
                set_private_directory(&current)?;
            }
            Err(error) => return Err(error).context("inspect projects directory"),
        }
    }
    current
        .canonicalize()
        .with_context(|| format!("resolve projects directory {}", current.display()))
}

fn set_private_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_and_outputs_stay_beneath_home() {
        let home = tempfile::tempdir().unwrap();
        let analysis = AnalysisRoot::open_with_home(Path::new("scans"), home.path()).unwrap();
        let output = analysis.resolve_file("customer-a/scan.xml").unwrap();
        assert!(output.starts_with(home.path().canonicalize().unwrap()));
        assert!(output.ends_with("scans/customer-a/scan.xml"));
        assert!(analysis.resolve_file("../outside").is_err());
        assert!(analysis.resolve_file("/tmp/outside").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_traversal() {
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = home.path().join("scans");
        std::fs::create_dir(&root).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("escape")).unwrap();
        let analysis = AnalysisRoot::open_with_home(&root, home.path()).unwrap();
        assert!(analysis.resolve_file("escape/result.txt").is_err());
    }
}
