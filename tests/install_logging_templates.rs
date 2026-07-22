#[test]
fn system_templates_define_private_split_logs_and_rotation() {
    let service = include_str!("../systemd/mcp-kali.service.in");
    assert!(service.contains("LogsDirectory=mcp-kali"));
    assert!(service.contains("LogsDirectoryMode=0700"));
    assert!(service.contains("UMask=0077"));

    let rotation = include_str!("../systemd/mcp-kali.logrotate.in");
    assert!(rotation.contains("/var/log/mcp-kali/mcp-kali.jsonl"));
    assert!(rotation.contains("/var/log/mcp-kali/mcp-kali.error.jsonl"));
    assert!(rotation.contains("rotate 30"));
    assert!(rotation.contains("create 0600 @MCP_KALI_USER@ @MCP_KALI_GROUP@"));
    assert!(rotation.contains("--signal=SIGHUP mcp-kali.service"));
}

#[test]
fn installed_configuration_templates_declare_log_directories() {
    let default_config = include_str!("../examples/mcp-kali.conf");
    let reference = include_str!("../examples/mcp-kali.conf.example");
    assert!(default_config.contains("MCP_KALI_LOG_DIR=@MCP_KALI_LOG_DIR@"));
    assert!(default_config.contains("MCP_KALI_PROJECTS_DIR=@MCP_KALI_PROJECTS_DIR@"));
    assert!(reference.contains("MCP_KALI_MAX_CONCURRENCY=4"));
    assert!(reference.contains("MCP_KALI_DEFAULT_TIMEOUT=432000"));
    assert!(reference.contains("MCP_KALI_PROJECTS_DIR=~/projects"));
    assert!(reference.contains("RUST_LOG controls normal tracing verbosity"));
}
