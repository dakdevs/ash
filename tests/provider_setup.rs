use std::{fs, process::Command};

use tempfile::tempdir;

fn ash() -> &'static str {
    env!("CARGO_BIN_EXE_ash")
}

#[test]
fn provider_add_writes_env_backed_provider_to_ashrc() {
    let dir = tempdir().expect("tempdir");
    let ashrc = dir.path().join(".ashrc");

    let output = Command::new(ash())
        .arg("--ashrc")
        .arg(&ashrc)
        .args(["provider", "add", "openai", "--env", "OPENAI_API_KEY"])
        .output()
        .expect("run ash provider add");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&ashrc).expect("ashrc"),
        "provider add openai kind openai env OPENAI_API_KEY\n"
    );
}

#[test]
fn provider_add_uses_default_auth_and_base_url() {
    let dir = tempdir().expect("tempdir");
    let ashrc = dir.path().join(".ashrc");

    let openai = Command::new(ash())
        .arg("--ashrc")
        .arg(&ashrc)
        .args(["provider", "add", "openai"])
        .output()
        .expect("run ash provider add openai");
    assert!(openai.status.success());

    let ollama = Command::new(ash())
        .arg("--ashrc")
        .arg(&ashrc)
        .args(["provider", "add", "ollama"])
        .output()
        .expect("run ash provider add ollama");
    assert!(ollama.status.success());

    assert_eq!(
        fs::read_to_string(&ashrc).expect("ashrc"),
        "provider add openai kind openai env OPENAI_API_KEY\nprovider add ollama kind ollama base-url http://localhost:11434\n"
    );
}

#[test]
fn provider_add_anthropic_uses_claude_code_auth_by_default() {
    let dir = tempdir().expect("tempdir");
    let ashrc = dir.path().join(".ashrc");

    let output = Command::new(ash())
        .arg("--ashrc")
        .arg(&ashrc)
        .args(["provider", "add", "anthropic"])
        .output()
        .expect("run ash provider add anthropic");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&ashrc).expect("ashrc"),
        "provider add anthropic kind anthropic auth claude-code\n"
    );
}

#[test]
fn provider_add_anthropic_allows_explicit_env_auth() {
    let dir = tempdir().expect("tempdir");
    let ashrc = dir.path().join(".ashrc");

    let output = Command::new(ash())
        .arg("--ashrc")
        .arg(&ashrc)
        .args(["provider", "add", "anthropic", "--env", "ANTHROPIC_API_KEY"])
        .output()
        .expect("run ash provider add anthropic");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&ashrc).expect("ashrc"),
        "provider add anthropic kind anthropic env ANTHROPIC_API_KEY\n"
    );
}

#[test]
fn provider_default_updates_existing_default() {
    let dir = tempdir().expect("tempdir");
    let ashrc = dir.path().join(".ashrc");
    fs::write(
        &ashrc,
        "provider add openai kind openai env OPENAI_API_KEY\nprovider default codex\n",
    )
    .expect("write ashrc");

    let output = Command::new(ash())
        .arg("--ashrc")
        .arg(&ashrc)
        .args(["provider", "default", "openai"])
        .output()
        .expect("run ash provider default");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&ashrc).expect("ashrc"),
        "provider add openai kind openai env OPENAI_API_KEY\nprovider default openai\n"
    );
}

#[test]
fn provider_list_marks_default_provider() {
    let dir = tempdir().expect("tempdir");
    let ashrc = dir.path().join(".ashrc");
    fs::write(
        &ashrc,
        "provider add openai kind openai env OPENAI_API_KEY\nprovider default openai\n",
    )
    .expect("write ashrc");

    let output = Command::new(ash())
        .arg("--ashrc")
        .arg(&ashrc)
        .args(["provider", "list"])
        .output()
        .expect("run ash provider list");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "* openai kind=openai auth=env:OPENAI_API_KEY\n"
    );
}

#[test]
fn provider_doctor_reports_missing_environment_secret() {
    let dir = tempdir().expect("tempdir");
    let ashrc = dir.path().join(".ashrc");
    fs::write(
        &ashrc,
        "provider add openai kind openai env ASH_TEST_MISSING_OPENAI_KEY\nprovider default openai\n",
    )
    .expect("write ashrc");

    let output = Command::new(ash())
        .arg("--ashrc")
        .arg(&ashrc)
        .args(["provider", "doctor"])
        .output()
        .expect("run ash provider doctor");

    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("openai: missing env ASH_TEST_MISSING_OPENAI_KEY")
    );
}

#[test]
fn auth_codex_dry_run_explains_setup_command() {
    let output = Command::new(ash())
        .args(["auth", "codex", "--dry-run"])
        .output()
        .expect("run ash auth codex");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "codex login\n");
}

#[test]
fn auth_anthropic_configures_anthropic_as_default_provider() {
    let dir = tempdir().expect("tempdir");
    let ashrc = dir.path().join(".ashrc");

    let output = Command::new(ash())
        .arg("--ashrc")
        .arg(&ashrc)
        .args(["auth", "anthropic"])
        .output()
        .expect("run ash auth anthropic");

    assert!(output.status.success());
    assert_eq!(
        fs::read_to_string(&ashrc).expect("ashrc"),
        "provider add anthropic kind anthropic auth claude-code\nprovider default anthropic\n"
    );
}

#[test]
fn auth_anthropic_dry_run_explains_provider_commands() {
    let output = Command::new(ash())
        .args(["auth", "anthropic", "--dry-run"])
        .output()
        .expect("run ash auth anthropic dry-run");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "ash provider add anthropic\nash provider default anthropic\n"
    );
}
