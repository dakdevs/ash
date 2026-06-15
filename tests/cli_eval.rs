use std::process::Command;

use tempfile::tempdir;

#[test]
fn eval_executes_native_command_mode_line() {
    let tempdir = tempdir().expect("tempdir");
    let context_db = tempdir.path().join("context.db");
    let binary = env!("CARGO_BIN_EXE_ash");

    let output = Command::new(binary)
        .arg("--no-ashrc")
        .arg("--context-db")
        .arg(&context_db)
        .arg("--mode")
        .arg("command")
        .arg("--eval")
        .arg("printf ash")
        .output()
        .expect("run ash");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "ash");
    assert!(context_db.exists());
}
