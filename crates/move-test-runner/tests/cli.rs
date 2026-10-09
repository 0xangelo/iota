use std::{path::PathBuf, process::Command};

use move_test_runner::Config;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/move")
        .join(name)
}

fn runner() -> Command {
    Command::new(env!("CARGO_BIN_EXE_move-test-runner"))
}

#[test]
fn prints_the_library_summary() {
    let output = runner()
        .args(["--coverage", "--summarize-functions"])
        .arg(fixture("basics"))
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    let report =
        move_test_runner::run(fixture("basics"), &Config::default().coverage(true)).unwrap();
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        report.coverage().unwrap().summary(true)
    );
}

#[test]
fn failing_tests_exit_with_1_and_no_summary() {
    let output = runner()
        .args(["--coverage", "--gas-limit", "10000"])
        .arg(fixture("failures"))
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("failures::cases::aborts")
    );
}

#[test]
fn unbuildable_packages_exit_with_2() {
    let output = runner().arg(fixture("missing")).output().unwrap();

    assert_eq!(output.status.code(), Some(2), "{output:?}");
}
