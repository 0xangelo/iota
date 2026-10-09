use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/move")
        .join(name)
}

fn run(package: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_move-test-runner"))
        .args(args)
        .arg(package)
        .output()
        .unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

#[test]
fn coverage_summary() {
    let output = run(&fixture("basics"), &["--coverage", "--summarize-functions"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).ends_with("2 passed, 0 failed\n"));
    insta::assert_snapshot!(String::from_utf8(output.stdout).unwrap());
}

#[test]
fn coverage_is_independent_of_thread_count() {
    let summary = |threads| {
        run(
            &fixture("basics"),
            &["--coverage", "--summarize-functions", "--threads", threads],
        )
        .stdout
    };

    assert_eq!(summary("1"), summary("4"));
}

#[test]
fn failing_tests_are_reported_and_exit_with_1() {
    let output = run(
        &fixture("failures"),
        &["--coverage", "--gas-limit", "10000"],
    );

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(output.stdout.is_empty());
    let stderr = stderr(&output);
    // Each failure starts a line with `<name>: `; the rendered reason may span
    // more lines.
    let failed: Vec<_> = stderr
        .lines()
        .filter_map(|line| line.split_once(": ").map(|(name, _)| name))
        .filter(|name| name.starts_with("failures::cases::"))
        .collect();
    assert_eq!(
        failed,
        [
            "failures::cases::aborts",
            "failures::cases::does_not_abort",
            "failures::cases::spins",
            "failures::cases::wrong_code",
        ]
    );
    assert!(stderr.ends_with("2 passed, 4 failed\n"), "{stderr}");
}

#[test]
fn compile_errors_exit_with_2() {
    let package = tempfile::tempdir().unwrap();
    std::fs::create_dir(package.path().join("sources")).unwrap();
    std::fs::write(
        package.path().join("Move.toml"),
        format!(
            "[package]\nedition = \"2024\"\nname = \"Broken\"\n\n[dependencies]\nMoveStdlib = {{ local = \"{}\" }}\n\n[addresses]\nbroken = \"0x0\"\n",
            fixture("std").display()
        ),
    )
    .unwrap();
    std::fs::write(
        package.path().join("sources/broken.move"),
        "module broken::broken;\n\npublic fun f(): u64 { true }\n",
    )
    .unwrap();

    let output = run(package.path(), &[]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("Invalid return expression"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn missing_packages_exit_with_2() {
    let output = run(&fixture("missing"), &[]);

    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
}
