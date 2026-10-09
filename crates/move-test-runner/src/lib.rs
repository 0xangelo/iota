//! Run a Move package's unit tests in-process, optionally collecting
//! instruction coverage.
//!
//! Tests run the way `iota move test` runs them: same natives, unit-test gas
//! schedule and pass/fail rules. [`Coverage::summary`] prints what `iota move
//! coverage --dev summary` would.
//!
//! Coverage comes from the in-memory hook behind `move-vm-runtime`'s `coverage`
//! feature, not from the CLI's per-instruction trace file. Other repositories
//! use the binary, published as a release by pushing a
//! `move-test-runner-v<version>` tag.

use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroUsize,
    path::Path,
    sync::{Mutex, Once},
};

use move_binary_format::CompiledModule;
use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use move_coverage::coverage_map::ExecCoverageMap;
use rayon::prelude::*;

mod compile;
mod env;
mod exec;

type Result<T, E = Error> = std::result::Result<T, E>;

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// `iota move test`'s per-test gas limit when `--gas-limit` isn't given.
const DEFAULT_GAS_LIMIT: u64 = 1_000_000;

/// Failure to run a package's tests; failing tests are reported in [`Report`]
/// instead.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
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

/// How [`run`] runs the tests.
#[derive(Clone, Debug)]
pub struct Config {
    gas_limit: u64,
    threads: Option<NonZeroUsize>,
    coverage: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            gas_limit: DEFAULT_GAS_LIMIT,
            threads: None,
            coverage: false,
        }
    }
}

impl Config {
    /// Gas budget of each test under the unit-test cost schedule, as `iota move
    /// test --gas-limit`.
    #[must_use]
    pub const fn gas_limit(mut self, gas_limit: u64) -> Self {
        self.gas_limit = gas_limit;
        self
    }

    /// How many tests run at once. Defaults to the available parallelism.
    #[must_use]
    pub const fn threads(mut self, threads: NonZeroUsize) -> Self {
        self.threads = Some(threads);
        self
    }

    /// Record the instructions the tests execute, for [`Report::coverage`].
    #[must_use]
    pub const fn coverage(mut self, coverage: bool) -> Self {
        self.coverage = coverage;
        self
    }
}

/// Outcome of [`run`].
#[derive(Debug)]
pub struct Report {
    passed: Vec<String>,
    failures: Vec<Failure>,
    coverage: Option<Coverage>,
}

impl Report {
    /// Names of the tests that passed, as `<address>::<module>::<function>`,
    /// sorted.
    pub fn passed(&self) -> &[String] {
        &self.passed
    }

    /// The tests that failed, sorted by name.
    pub fn failures(&self) -> &[Failure] {
        &self.failures
    }

    /// Instruction coverage over the package's own modules, if
    /// [`Config::coverage`] was set.
    pub const fn coverage(&self) -> Option<&Coverage> {
        self.coverage.as_ref()
    }
}

/// A test that failed.
#[derive(Debug)]
pub struct Failure {
    name: String,
    message: String,
}

impl Failure {
    /// The test's name, as `<address>::<module>::<function>`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Why it failed, as `iota move test` reports it, without colour.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, self.message)
    }
}

/// Which instructions of the package's own modules the tests executed.
#[derive(Debug)]
pub struct Coverage {
    hits: ExecCoverageMap,
    modules: Vec<CompiledModule>,
}

impl Coverage {
    /// What `iota move coverage --dev summary` prints, plus
    /// `--summarize-functions` if `per_function`.
    pub fn summary(&self, per_function: bool) -> String {
        let mut out = Vec::new();
        move_coverage::format_human_summary(
            &self.modules,
            &self.hits,
            move_coverage::summary::summarize_inst_cov,
            &mut out,
            per_function,
        );
        String::from_utf8_lossy(&out).into_owned()
    }
}

/// Compile the package at `package` in test mode and run all of its unit tests.
///
/// Nothing is written into the package directory; build artifacts go to a
/// temporary directory.
pub fn run(package: impl AsRef<Path>, config: &Config) -> Result<Report> {
    static PACKAGE_HOOKS: Once = Once::new();
    PACKAGE_HOOKS.call_once(|| {
        move_package::package_hooks::register_package_hooks(Box::new(
            iota_move_build::IotaPackageHooks,
        ));
    });

    let package = package.as_ref();
    let build_dir = tempfile::tempdir().map_err(|e| Error::Build(e.into()))?;
    let plan = compile::test_plan(package, build_dir.path())?;
    let tests = exec::Tests::new(&plan, config.gas_limit)?;

    let _recording = config.coverage.then(move_vm_runtime::coverage::enable);
    let threads = config
        .threads
        .or_else(|| std::thread::available_parallelism().ok())
        .map_or(1, NonZeroUsize::get);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .map_err(|e| Error::Threads(e.into()))?;

    let hits = Mutex::new(Hits::new());
    let outcomes: Vec<_> = pool.install(|| {
        tests
            .names()
            .into_par_iter()
            .with_max_len(1)
            .map(|(module, name)| {
                let outcome = tests.run(module, name);
                if config.coverage {
                    let mut hits = hits
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    for function in move_vm_runtime::coverage::take() {
                        hits.entry((
                            *function.module.address(),
                            function.module.name().to_owned(),
                            function.function,
                        ))
                        .or_default()
                        .extend(function.pcs);
                    }
                }
                outcome
            })
            .collect()
    });

    let mut passed = Vec::new();
    let mut failures = Vec::new();
    for outcome in outcomes {
        match outcome {
            exec::Outcome::Passed(name) => passed.push(name),
            exec::Outcome::Failed(name, message) => failures.push(Failure { name, message }),
        }
    }
    passed.sort();
    failures.sort_by(|a, b| a.name.cmp(&b.name));

    let coverage = if config.coverage {
        let modules = compile::root_modules(package, build_dir.path())?;
        let mut map = ExecCoverageMap::new(String::new());
        for ((address, module, function), pcs) in hits
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
        {
            for pc in pcs {
                map.insert(address, module.clone(), function.clone(), pc.into());
            }
        }
        Some(Coverage { hits: map, modules })
    } else {
        None
    };

    Ok(Report {
        passed,
        failures,
        coverage,
    })
}

type Hits = BTreeMap<(AccountAddress, Identifier, Identifier), BTreeSet<u16>>;
