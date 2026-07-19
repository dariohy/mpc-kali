use anyhow::{Context, Result, bail};
use std::{env, ffi::OsString, path::PathBuf};

const CONFIG_FILE_VARIABLE: &str = "MCP_KALI_CONFIG_FILE";
const HOME_VARIABLE: &str = "MCP_KALI_HOME";

/// Loads the shared MCP Kali configuration file before Clap resolves
/// environment-backed options. The file uses a simple `KEY=VALUE` format so
/// its values can provide normal CLI defaults. Existing shell variables win
/// because dotenv does not overwrite them; explicit CLI arguments win over both.
pub fn load_config_file() -> Result<Option<PathBuf>> {
    let cli_path = config_file_from_args(env::args_os())?;
    let environment_path = env::var_os(CONFIG_FILE_VARIABLE).map(PathBuf::from);
    let explicit = cli_path.is_some() || environment_path.is_some();
    let path = cli_path.or(environment_path).or_else(default_config_file);

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
        .map(|home| home.join("var/jobs"))
        .unwrap_or_else(|| PathBuf::from("/var/lib/mcp-kali/jobs"))
}

pub fn default_system_data_dir() -> PathBuf {
    default_mcp_kali_home()
        .map(|home| home.join("share"))
        .unwrap_or_else(|| PathBuf::from("/usr/local/share/mcp-kali"))
}

pub fn default_config_dir() -> PathBuf {
    default_mcp_kali_home()
        .map(|home| home.join("etc"))
        .unwrap_or_else(|| PathBuf::from("/etc/mcp-kali"))
}

fn default_config_file() -> Option<PathBuf> {
    default_mcp_kali_home().map(|home| home.join("etc/mcp-kali.conf"))
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
}
