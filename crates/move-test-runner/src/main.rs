use std::{num::NonZeroUsize, path::PathBuf, process::ExitCode};

use clap::Parser;
use move_test_runner::Config;

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

    let report = match move_test_runner::run(&args.package, &config) {
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
