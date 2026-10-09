use std::{num::NonZeroUsize, path::PathBuf};

use move_test_runner::{Config, Error};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/move")
        .join(name)
}

#[test]
fn coverage_summary() {
    let report =
        move_test_runner::run(fixture("basics"), &Config::default().coverage(true)).unwrap();

    assert!(report.failures().is_empty(), "{:#?}", report.failures());
    assert_eq!(
        report.passed(),
        [
            "basics::math_tests::clamp_below_max",
            "basics::math_tests::divide_by_zero"
        ]
    );
    insta::assert_snapshot!(report.coverage().unwrap().summary(true));
}

#[test]
fn coverage_is_independent_of_thread_count() {
    let summary = |threads| {
        let config = Config::default().coverage(true).threads(threads);
        move_test_runner::run(fixture("basics"), &config)
            .unwrap()
            .coverage()
            .unwrap()
            .summary(true)
    };

    assert_eq!(
        summary(NonZeroUsize::MIN),
        summary(NonZeroUsize::new(4).unwrap())
    );
}

#[test]
fn failure_kinds() {
    let report =
        move_test_runner::run(fixture("failures"), &Config::default().gas_limit(10_000)).unwrap();

    assert_eq!(
        report.passed(),
        [
            "failures::cases::passes",
            "failures::cases::raw_tagged_code"
        ]
    );
    let failed: Vec<_> = report.failures().iter().map(|f| f.name()).collect();
    assert_eq!(
        failed,
        [
            "failures::cases::aborts",
            "failures::cases::does_not_abort",
            "failures::cases::spins",
            "failures::cases::wrong_code",
        ]
    );
    assert!(report.coverage().is_none());
}

#[test]
fn compile_errors_are_returned() {
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

    let err = move_test_runner::run(package.path(), &Config::default()).unwrap_err();

    assert!(matches!(err, Error::Build(_)), "{err}");
    assert!(
        err.to_string().contains("Invalid return expression"),
        "{err}"
    );
}
