use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    Command::cargo_bin("rust_toy_agent").unwrap()
}

#[test]
fn help_prints_usage() {
    bin()
        .arg("--help")
        .assert()
        .success()
        .stderr(predicate::str::contains("Usage"));
}

#[test]
fn test_mode_unknown_name_errors() {
    bin()
        .args(["--test", "definitely_not_a_real_test"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found").or(predicate::str::contains("Error")));
}

#[test]
fn repl_empty_stdin_exits_cleanly() {
    bin()
        .env("ANTHROPIC_API_KEY", "fake-key-for-test")
        .env("MODEL_ID", "claude-opus-4-6")
        .write_stdin("")
        .timeout(std::time::Duration::from_secs(5))
        .assert()
        .success();
}

#[test]
fn test_mode_missing_arg() {
    bin()
        .arg("--test")
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires"));
}
