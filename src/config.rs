use anyhow::{Context, Result, bail};
use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

const HOME_VARIABLE: &str = "MCP_KALI_HOME";
const SYSTEM_CONFIG_FILE: &str = "/etc/mcp-kali/mcp-kali.config";
const LEGACY_SYSTEM_CONFIG_FILE: &str = "/etc/mcp-kali/mcp-kali.conf";

/// Loads the shared MCP Kali configuration file before Clap resolves
/// environment-backed options. The file uses a simple `KEY=VALUE` format so
/// its values can provide normal CLI defaults. Existing shell variables win
/// because dotenv does not overwrite them; explicit CLI arguments win over both.
pub fn load_config_file() -> Result<Option<PathBuf>> {
    let cli_path = config_file_from_args(env::args_os())?;
    let explicit = cli_path.is_some();
    let path = cli_path.or_else(default_config_file);

    let Some(path) = path.map(expand_home) else {
        return Ok(None);
    };
    if !path.exists() {
        if explicit {
            bail!("configuration file does not exist: {}", path.display());
        }
        return Ok(None);
    }
    if !path.is_file() {
        bail!(
            "configuration file is not a regular file: {}",
            path.display()
        );
    }
    dotenvy::from_path(&path)
        .with_context(|| format!("load configuration file {}", path.display()))?;
    Ok(Some(path))
}

/// Reads one value from a configuration file without mutating the process
/// environment. This supports runtime reloads of selected settings.
pub fn read_config_value(path: &Path, key: &str) -> Result<Option<String>> {
    let iterator = dotenvy::from_path_iter(path)
        .with_context(|| format!("read configuration file {}", path.display()))?;
    for item in iterator {
        let (candidate, value) =
            item.with_context(|| format!("parse configuration file {}", path.display()))?;
        if candidate == key {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn config_file_from_args(args: impl IntoIterator<Item = OsString>) -> Result<Option<PathBuf>> {
    let mut args = args.into_iter().skip(1);
    while let Some(argument) = args.next() {
        if argument == "--config-file" {
            let value = args.next().context("--config-file requires a path")?;
            return Ok(Some(PathBuf::from(value)));
        }
        let value = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--config-file="));
        if let Some(value) = value {
            if value.is_empty() {
                bail!("--config-file requires a path");
            }
            return Ok(Some(PathBuf::from(value)));
        }
    }
    Ok(None)
}

/// Returns the self-contained per-user MCP Kali directory. `MCP_KALI_HOME`
/// makes a relocated user installation explicit without changing individual
/// state, configuration, or data paths.
pub fn default_mcp_kali_home() -> Option<PathBuf> {
    env::var_os(HOME_VARIABLE)
        .map(PathBuf::from)
        .map(expand_home)
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".mcp-kali"))
        })
}

pub fn default_state_dir() -> PathBuf {
    default_mcp_kali_home()
        .map(|home| home.join("var/lib/jobs"))
        .unwrap_or_else(|| PathBuf::from("/var/lib/mcp-kali/jobs"))
}

pub fn default_system_data_dir() -> PathBuf {
    default_mcp_kali_home()
        .map(|home| home.join("share"))
        .unwrap_or_else(|| PathBuf::from("/usr/lib/mcp-kali"))
}

pub fn default_config_dir() -> PathBuf {
    default_mcp_kali_home()
        .map(|home| home.join("etc"))
        .unwrap_or_else(|| PathBuf::from("/etc/mcp-kali"))
}

fn default_config_file() -> Option<PathBuf> {
    preferred_config_file(
        PathBuf::from(SYSTEM_CONFIG_FILE),
        default_mcp_kali_home().map(|home| home.join("etc/mcp-kali.config")),
    )
    .or_else(|| {
        preferred_config_file(
            PathBuf::from(LEGACY_SYSTEM_CONFIG_FILE),
            default_mcp_kali_home().map(|home| home.join("etc/mcp-kali.conf")),
        )
    })
}

fn preferred_config_file(system: PathBuf, user: Option<PathBuf>) -> Option<PathBuf> {
    if system.is_file() { Some(system) } else { user }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_config_file_forms() {
        let split = config_file_from_args([
            "binary".into(),
            "--config-file".into(),
            "/tmp/a.conf".into(),
        ])
        .unwrap();
        assert_eq!(split, Some(PathBuf::from("/tmp/a.conf")));

        let joined =
            config_file_from_args(["binary".into(), "--config-file=/tmp/b.conf".into()]).unwrap();
        assert_eq!(joined, Some(PathBuf::from("/tmp/b.conf")));
    }

    #[test]
    fn reads_a_value_without_changing_the_environment() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mcp-kali.conf");
        std::fs::write(&path, "MCP_KALI_MAX_CONCURRENCY=4\n").unwrap();
        assert_eq!(
            read_config_value(&path, "MCP_KALI_MAX_CONCURRENCY").unwrap(),
            Some("4".into())
        );
        assert_eq!(read_config_value(&path, "MISSING").unwrap(), None);
    }

    #[test]
    fn system_config_wins_when_present() {
        let directory = tempfile::tempdir().unwrap();
        let system = directory.path().join("system.conf");
        let user = directory.path().join("user.conf");
        std::fs::write(&system, "KEY=VALUE\n").unwrap();
        assert_eq!(
            preferred_config_file(system.clone(), Some(user.clone())),
            Some(system.clone())
        );
        std::fs::remove_file(&system).unwrap();
        assert_eq!(
            preferred_config_file(system, Some(user.clone())),
            Some(user)
        );
    }

    #[test]
    fn canonical_config_is_preferred_over_legacy_config() {
        let directory = tempfile::tempdir().unwrap();
        let canonical = directory.path().join("mcp-kali.config");
        let legacy = directory.path().join("mcp-kali.conf");
        std::fs::write(&canonical, "KEY=CANONICAL\n").unwrap();
        std::fs::write(&legacy, "KEY=LEGACY\n").unwrap();
        assert_eq!(
            preferred_config_file(canonical.clone(), Some(legacy)),
            Some(canonical)
        );
    }
}
