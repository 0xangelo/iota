//! Run a Move package's unit tests in-process, optionally printing their
//! instruction coverage.
//!
//! Tests run the way `iota move test` runs them: same natives, unit-test gas
//! schedule and pass/fail rules. With `--coverage`, it prints what `iota move
//! coverage --dev summary` would.
//!
//! Coverage comes from the in-memory hook behind `move-vm-runtime`'s `coverage`
//! feature, not from the CLI's per-instruction trace file. Pushing a
//! `move-test-runner-v<version>` tag publishes the binary as a release.

use std::{num::NonZeroUsize, path::PathBuf, process::ExitCode};

use clap::Parser;

use crate::run::Config;

mod compile;
mod env;
mod exec;
mod run;

type Result<T, E = Error> = std::result::Result<T, E>;

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Failure to run a package's tests; failing tests are reported instead.
#[derive(thiserror::Error, Debug)]
enum Error {
    /// Resolving or compiling the package failed.
    #[error("building the Move package: {0}")]
    Build(BoxError),
    /// The package has a `#[random_test]`, which this runner doesn't support.
    #[error("`#[random_test]` is not supported: {0}")]
    RandomTest(String),
    /// The compiled modules couldn't be loaded into the tests' storage.
    #[error("loading modules into test storage: {0}")]
    Storage(BoxError),
    /// The test threads couldn't be started.
    #[error("starting the test threads: {0}")]
    Threads(BoxError),
}

/// Run a Move package's unit tests, optionally printing their coverage summary.
///
/// Exits with 1 if a test fails and 2 if the tests can't run.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Directory of the Move package.
    package: PathBuf,
    /// Gas budget of each test, as `iota move test --gas-limit`.
    #[arg(long)]
    gas_limit: Option<u64>,
    /// How many tests run at once. Defaults to the available parallelism.
    #[arg(long)]
    threads: Option<NonZeroUsize>,
    /// If every test passes, print what `iota move coverage --dev summary`
    /// would.
    #[arg(long)]
    coverage: bool,
    /// Summarize each function too, as `--summarize-functions`.
    #[arg(long, requires = "coverage")]
    summarize_functions: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let mut config = Config::default().coverage(args.coverage);
    if let Some(gas_limit) = args.gas_limit {
        config = config.gas_limit(gas_limit);
    }
    if let Some(threads) = args.threads {
        config = config.threads(threads);
    }

    let report = match run::run(&args.package, &config) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(2);
        }
    };
    for failure in report.failures() {
        eprintln!("{failure}");
    }
    eprintln!(
        "{} passed, {} failed",
        report.passed().len(),
        report.failures().len()
    );
    if !report.failures().is_empty() {
        return ExitCode::FAILURE;
    }
    if let Some(coverage) = report.coverage() {
        print!("{}", coverage.summary(args.summarize_functions));
    }
    ExitCode::SUCCESS
}
