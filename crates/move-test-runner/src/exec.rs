//! Running one test and judging it, following `move-unit-test`'s
//! `test_runner.rs`.

use std::{cell::RefCell, collections::BTreeMap, time::Instant};

use move_binary_format::errors::{Location, VMError};
use move_command_line_common::error_bitset::ErrorBitset;
use move_compiler::{
    compiled_unit::NamedCompiledModule,
    unit_test::{ExpectedFailure, ExpectedMoveError, MoveErrorType, TestArgument, TestPlan},
};
use move_core_types::{
    identifier::IdentStr,
    language_storage::ModuleId,
    runtime_value::{MoveValue, serialize_values},
    vm_status::StatusCode,
};
use move_unit_test::test_reporter::{FailureReason, TestFailure, TestRunInfo};
use move_vm_runtime::{move_vm::MoveVM, native_functions::NativeFunctionTable};
use move_vm_test_utils::{
    InMemoryStorage,
    gas_schedule::{CostTable, Gas, GasStatus},
};

use crate::{Error, Result, env};

pub(crate) enum Outcome {
    Passed(String),
    Failed(String, String),
}

/// Everything shared by the tests of one [`run`](crate::run).
pub(crate) struct Tests<'a> {
    plan: &'a TestPlan,
    storage: InMemoryStorage,
    natives: NativeFunctionTable,
    cost_table: CostTable,
    gas_limit: u64,
}

thread_local! {
    // One VM per thread and run: its loader caches the run's modules, so the tests a thread runs
    // don't each reload and re-verify them, which the CLI does.
    static VM: RefCell<Option<MoveVM>> = const { RefCell::new(None) };
}

impl<'a> Tests<'a> {
    pub(crate) fn new(plan: &'a TestPlan, gas_limit: u64) -> Result<Self> {
        for (module, tests) in &plan.module_tests {
            for (name, test) in &tests.tests {
                if test
                    .arguments
                    .iter()
                    .any(|arg| matches!(arg, TestArgument::Generate { .. }))
                {
                    return Err(Error::RandomTest(test_name(plan, module, name)));
                }
            }
        }
        Ok(Self {
            plan,
            storage: env::storage(plan)?,
            natives: env::natives(),
            cost_table: iota_types::gas_model::tables::initial_cost_schedule_for_unit_tests(),
            gas_limit,
        })
    }

    pub(crate) fn names(&self) -> Vec<(&'a ModuleId, &'a str)> {
        self.plan
            .module_tests
            .iter()
            .flat_map(|(module, tests)| tests.tests.keys().map(move |name| (module, name.as_str())))
            .collect()
    }

    pub(crate) fn run(&self, module: &ModuleId, name: &str) -> Outcome {
        VM.with_borrow_mut(|vm| {
            let vm = vm.get_or_insert_with(|| env::vm(&self.natives));
            self.run_with(vm, module, name)
        })
    }

    fn run_with(&self, vm: &MoveVM, module: &ModuleId, name: &str) -> Outcome {
        let test = &self.plan.module_tests[module].tests[name];
        let arguments: Vec<MoveValue> = test
            .arguments
            .iter()
            .map(|arg| match arg {
                TestArgument::Value(value) => value.clone(),
                TestArgument::Generate { .. } => unreachable!("rejected in `Tests::new`"),
            })
            .collect();

        let mut session = vm.new_session_with_extensions(&self.storage, env::extensions());
        let mut gas = GasStatus::new(&self.cost_table, Gas::new(self.gas_limit));
        let started = Instant::now();
        let result = session.execute_function_bypass_visibility(
            module,
            IdentStr::new(name).expect("test names are identifiers"),
            vec![],
            serialize_values(&arguments),
            &mut gas,
            None,
        );
        let info = TestRunInfo::new(
            started.elapsed(),
            Gas::new(self.gas_limit)
                .checked_sub(gas.remaining_gas())
                .map_or(0, u64::from),
            None,
        );
        drop(session.finish_with_extensions());

        let full_name = test_name(self.plan, module, name);
        let failure = match (result, &test.expected_failure) {
            (Ok(_), None) => None,
            (Ok(_), Some(_)) => Some((FailureReason::no_error(), None)),
            (Err(err), expected) => {
                let actual = self.actual_error(&err);
                match expected {
                    Some(ExpectedFailure::Expected) => None,
                    Some(ExpectedFailure::ExpectedWithError(expected)) if *expected == actual => {
                        None
                    }
                    Some(ExpectedFailure::ExpectedWithCodeDEPRECATED(code))
                        if actual.0 == StatusCode::ABORTED && actual.1.as_ref() == Some(code) =>
                    {
                        None
                    }
                    Some(ExpectedFailure::ExpectedWithError(expected)) => Some((
                        FailureReason::wrong_error(expected.clone(), actual),
                        Some(err),
                    )),
                    Some(ExpectedFailure::ExpectedWithCodeDEPRECATED(code)) => Some((
                        FailureReason::wrong_abort_deprecated(code.clone(), actual),
                        Some(err),
                    )),
                    None if err.major_status() == StatusCode::OUT_OF_GAS => {
                        Some((FailureReason::timeout(), Some(err)))
                    }
                    None => Some((FailureReason::unexpected_error(actual), Some(err))),
                }
            }
        };
        match failure {
            None => Outcome::Passed(full_name),
            Some((reason, err)) => {
                // `render_error` colours its output when stdout is a terminal.
                let message = TestFailure::new(reason, info, err, None).render_error(self.plan);
                Outcome::Failed(
                    full_name,
                    anstream::adapter::strip_str(&message).to_string(),
                )
            }
        }
    }

    fn actual_error(&self, err: &VMError) -> ExpectedMoveError {
        let sub_status = err
            .sub_status()
            .and_then(|code| clever_error(code, err.location(), &self.plan.module_info));
        ExpectedMoveError(err.major_status(), sub_status, err.location().clone())
    }
}

/// Resolve a clever abort code to the name of the constant it encodes.
fn clever_error(
    code: u64,
    location: &Location,
    modules: &BTreeMap<ModuleId, NamedCompiledModule>,
) -> Option<MoveErrorType> {
    let Some(bitset) = ErrorBitset::from_u64(code) else {
        return Some(MoveErrorType::Code(code));
    };
    let Location::Module(module) = location else {
        return None;
    };
    let module = &modules.get(module)?.module;
    let constant = module
        .constant_pool
        .get(usize::from(bitset.identifier_index()?))?;
    let name: Vec<u8> = bcs::from_bytes(&constant.data).ok()?;
    Some(MoveErrorType::ConstantName(String::from_utf8(name).ok()?))
}

fn test_name(plan: &TestPlan, module: &ModuleId, name: &str) -> String {
    plan.module_info
        .get(module)
        .and_then(NamedCompiledModule::address_name)
        .map_or_else(
            || format!("{}::{name}", module.short_str_lossless()),
            |address| format!("{address}::{}::{name}", module.name()),
        )
}
