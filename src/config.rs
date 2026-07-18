use anyhow::{Context, Result, bail};
use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

const ENV_FILE_VARIABLE: &str = "MCP_KALI_ENV_FILE";

/// Loads the shared MCP Kali env file before Clap resolves environment-backed
/// options. Existing shell variables win because dotenv does not overwrite
/// them; explicit CLI arguments are parsed afterwards and win over both.
pub fn load_env_file() -> Result<Option<PathBuf>> {
    let cli_path = env_file_from_args(env::args_os())?;
    let environment_path = env::var_os(ENV_FILE_VARIABLE).map(PathBuf::from);
    let explicit = cli_path.is_some() || environment_path.is_some();
    let path = cli_path.or(environment_path).or_else(default_env_file);

    let Some(path) = path.map(expand_home) else {
        return Ok(None);
    };
    if !path.exists() {
        if explicit {
            bail!("environment file does not exist: {}", path.display());
        }
        return Ok(None);
    }
    if !path.is_file() {
        bail!("environment file is not a regular file: {}", path.display());
    }
    warn_if_permissions_are_broad(&path)?;
    dotenvy::from_path(&path)
        .with_context(|| format!("load environment file {}", path.display()))?;
    Ok(Some(path))
}

fn env_file_from_args(args: impl IntoIterator<Item = OsString>) -> Result<Option<PathBuf>> {
    let mut args = args.into_iter().skip(1);
    while let Some(argument) = args.next() {
        if argument == "--env-file" {
            let value = args.next().context("--env-file requires a path")?;
            return Ok(Some(PathBuf::from(value)));
        }
        if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--env-file="))
        {
            if value.is_empty() {
                bail!("--env-file requires a path");
            }
            return Ok(Some(PathBuf::from(value)));
        }
    }
    Ok(None)
}

fn default_env_file() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".envs/.env_mcp-kali"))
}

fn expand_home(path: PathBuf) -> PathBuf {
    let value = path.to_string_lossy();
    if value == "~" {
        return env::var_os("HOME").map(PathBuf::from).unwrap_or(path);
    }
    if let Some(rest) = value.strip_prefix("~/") {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    path
}

#[cfg(unix)]
fn warn_if_permissions_are_broad(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(path)?.permissions().mode();
    if mode & 0o077 != 0 {
        eprintln!(
            "warning: environment file {} is accessible by group or other users; use chmod 600",
            path.display()
        );
    }
    Ok(())
}

#[cfg(not(unix))]
fn warn_if_permissions_are_broad(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_both_env_file_forms() {
        let split = env_file_from_args(["binary".into(), "--env-file".into(), "/tmp/a.env".into()])
            .unwrap();
        assert_eq!(split, Some(PathBuf::from("/tmp/a.env")));

        let joined = env_file_from_args(["binary".into(), "--env-file=/tmp/b.env".into()]).unwrap();
        assert_eq!(joined, Some(PathBuf::from("/tmp/b.env")));
    }
}
