use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    process::{Command, Stdio},
};

use tempfile::tempdir;

#[test]
fn interactive_agent_renders_json_events_and_passes_recent_context() {
    let dir = tempdir().expect("tempdir");
    let context_db = dir.path().join("context.db");
    let codex = dir.path().join("codex");
    fs::write(
        &codex,
        r#"#!/bin/sh
case "$*" in
  *again*remember*) text="context ok" ;;
  *remember*) text="stored" ;;
  *) text="default" ;;
esac
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' '{"type":"item.started","item":{"type":"command_execution","command":"/bin/zsh -lc '\''git status --short'\''","status":"in_progress"}}'
printf '%s\n' '{"type":"item.completed","item":{"type":"command_execution","command":"/bin/zsh -lc '\''git status --short'\''","aggregated_output":" M src/ui.rs\n","exit_code":0,"status":"completed"}}'
printf '%s\n' "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"$text\"}}"
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":2}}'
"#,
    )
    .expect("write fake codex");
    let mut permissions = fs::metadata(&codex).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&codex, permissions).expect("chmod");

    let mut child = Command::new(env!("CARGO_BIN_EXE_ash"))
        .arg("--no-ashrc")
        .arg("--context-db")
        .arg(&context_db)
        .env("PATH", path_with_fake_codex(dir.path()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ash");

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        stdin
            .write_all(b"remember cerulean\nagain\n")
            .expect("write prompts");
    }

    let output = child.wait_with_output().expect("ash output");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = strip_ansi(&String::from_utf8_lossy(&output.stdout));
    assert!(stdout.contains("╭─ assistant"));
    assert!(stdout.contains("thinking"));
    assert!(stdout.contains("tool\n│ $ /bin/zsh -lc 'git status --short'"));
    assert!(stdout.contains("output\n│"));
    assert!(stdout.contains("tool\n│ exit 0"));
    assert!(stdout.contains("usage\n│ in 1 · out 2"));
    assert!(stdout.contains("stored"));
    assert!(stdout.contains("context ok"));
}

fn path_with_fake_codex(fake_dir: &std::path::Path) -> String {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    format!("{}:{}", fake_dir.display(), existing.to_string_lossy())
}

fn strip_ansi(value: &str) -> String {
    let mut output = String::new();
    let mut chars = value.chars().peekable();

    while let Some(character) = chars.next() {
        if character == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for code in chars.by_ref() {
                if code.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        output.push(character);
    }

    output
}
