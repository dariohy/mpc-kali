#![cfg(unix)]

use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Server(Option<Child>);

impl Server {
    fn child(&mut self) -> &mut Child {
        self.0.as_mut().unwrap()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            unsafe {
                libc::kill(child.id() as libc::pid_t, libc::SIGKILL);
            }
            let _ = child.wait();
        }
    }
}

#[test]
fn splits_json_logs_and_reopens_both_files_after_sighup() {
    let root = tempfile::tempdir().unwrap();
    let log_dir = root.path().join("logs");
    let system_data = root.path().join("system-data");
    let config_dir = root.path().join("config");
    fs::create_dir(&log_dir).unwrap();
    fs::create_dir(&system_data).unwrap();
    fs::create_dir(&config_dir).unwrap();
    let config_file = root.path().join("mcp-kali.conf");
    fs::write(
        &config_file,
        format!(
            "MCP_KALI_LOG_DIR={}\nRUST_LOG=mcp_kali=info,tower_http=info\n",
            log_dir.display()
        ),
    )
    .unwrap();

    let child = Command::new(env!("CARGO_BIN_EXE_mcp-kali"))
        .args([
            "--config-file",
            config_file.to_str().unwrap(),
            "--bind",
            "0.0.0.0:0",
            "--allow-remote-bind",
            "--state-dir",
            root.path().join("jobs").to_str().unwrap(),
            "--system-data-dir",
            system_data.to_str().unwrap(),
            "--config-dir",
            config_dir.to_str().unwrap(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("HOME", root.path())
        .env("RUST_LOG", "mcp_kali=info,tower_http=info")
        .spawn()
        .unwrap();
    let mut server = Server(Some(child));
    let main = log_dir.join("mcp-kali.jsonl");
    let error = log_dir.join("mcp-kali.error.jsonl");
    wait_for(&main, "HTTP server listening");
    wait_for(&error, "remote bind enabled");

    let initial_main = read_json_lines(&main);
    let initial_error = read_json_lines(&error);
    assert!(
        initial_main
            .iter()
            .all(|record| { matches!(record["level"].as_str(), Some("TRACE" | "DEBUG" | "INFO")) })
    );
    assert!(
        initial_error
            .iter()
            .all(|record| matches!(record["level"].as_str(), Some("WARN" | "ERROR")))
    );

    let rotated_main = log_dir.join("mcp-kali.jsonl.rotated");
    let rotated_error = log_dir.join("mcp-kali.error.jsonl.rotated");
    fs::rename(&main, &rotated_main).unwrap();
    fs::rename(&error, &rotated_error).unwrap();
    unsafe {
        assert_eq!(
            libc::kill(server.child().id() as libc::pid_t, libc::SIGHUP),
            0
        );
    }
    wait_for(&main, "log files reopened after SIGHUP");
    assert!(error.is_file());

    unsafe {
        assert_eq!(
            libc::kill(server.child().id() as libc::pid_t, libc::SIGTERM),
            0
        );
    }
    let status = server.child().wait().unwrap();
    assert!(status.success());
    let mut stdout = String::new();
    let mut stderr = String::new();
    use std::io::Read;
    server
        .child()
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    server
        .child()
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(stdout.is_empty(), "unexpected stdout: {stdout}");
    assert!(stderr.is_empty(), "unexpected stderr: {stderr}");
    server.0.take();
}

#[test]
fn unavailable_configured_directory_falls_back_to_stdout() {
    let root = tempfile::tempdir().unwrap();
    let config_file = root.path().join("mcp-kali.conf");
    fs::write(
        &config_file,
        format!(
            "MCP_KALI_LOG_DIR={}\nRUST_LOG=mcp_kali=info\n",
            root.path().join("missing").display()
        ),
    )
    .unwrap();
    let system_data = root.path().join("system-data");
    let config_dir = root.path().join("config");
    fs::create_dir(&system_data).unwrap();
    fs::create_dir(&config_dir).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_mcp-kali"))
        .args([
            "--config-file",
            config_file.to_str().unwrap(),
            "--bind",
            "127.0.0.1:0",
            "--state-dir",
            root.path().join("jobs").to_str().unwrap(),
            "--system-data-dir",
            system_data.to_str().unwrap(),
            "--config-dir",
            config_dir.to_str().unwrap(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("HOME", root.path())
        .env("RUST_LOG", "mcp_kali=info")
        .spawn()
        .unwrap();
    let mut server = Server(Some(child));
    use std::io::{BufRead, BufReader, Read};
    let stdout_pipe = server.child().stdout.take().unwrap();
    let mut stdout_reader = BufReader::new(stdout_pipe);
    let mut stdout = String::new();
    loop {
        let mut line = String::new();
        assert_ne!(stdout_reader.read_line(&mut line).unwrap(), 0);
        let listening = line.contains("HTTP server listening");
        stdout.push_str(&line);
        if listening {
            break;
        }
    }
    unsafe {
        libc::kill(server.child().id() as libc::pid_t, libc::SIGTERM);
    }
    let status = server.child().wait().unwrap();
    stdout_reader.read_to_string(&mut stdout).unwrap();
    let mut stderr = String::new();
    server
        .child()
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(status.success());
    assert!(
        stdout.contains("using stdout"),
        "unexpected stdout: {stdout}"
    );
    assert!(stderr.is_empty(), "unexpected stderr: {stderr}");
    server.0.take();
}

fn wait_for(path: &Path, needle: &str) {
    let started = Instant::now();
    loop {
        if fs::read_to_string(path).is_ok_and(|contents| contents.contains(needle)) {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "timed out waiting for {needle} in {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(25));
    }
}

fn read_json_lines(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
