//! The IOTA unit-test environment `iota move test` sets up, from `iota-move`'s
//! `unit_test.rs`.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::{Arc, LazyLock},
};

use iota_move_natives::{
    NativesCostTable, authentication_context::AuthenticationContext, object_runtime::ObjectRuntime,
    protocol_config::ProtocolConfigTestOverrides, test_scenario::InMemoryTestStore,
    transaction_context::TransactionContext,
};
use iota_protocol_config::ProtocolConfig;
use iota_sdk_types::{Address, TransactionDigest};
use iota_types::{
    auth_context::AuthContext, base_types::TxContext,
    in_memory_storage::InMemoryStorage as ObjectStorage, metrics::LimitsMetrics,
};
use move_binary_format::binary_config::BinaryConfig;
use move_bytecode_utils::Modules;
use move_compiler::unit_test::TestPlan;
use move_vm_runtime::{
    move_vm::MoveVM, native_extensions::NativeContextExtensions,
    native_functions::NativeFunctionTable,
};
use move_vm_test_utils::InMemoryStorage;

use crate::{Error, Result};

static PROTOCOL_CONFIG: LazyLock<ProtocolConfig> =
    LazyLock::new(ProtocolConfig::get_for_max_version_UNSAFE);

thread_local! {
    // Like the CLI, objects created by `test_scenario` live per thread and aren't reset between
    // tests.
    static OBJECTS: RefCell<ObjectStorage> = RefCell::new(ObjectStorage::default());
}

static TEST_STORE: InMemoryTestStore = InMemoryTestStore(&OBJECTS);

pub(crate) fn natives() -> NativeFunctionTable {
    iota_move_natives::all_natives(false, &PROTOCOL_CONFIG)
}

pub(crate) fn vm(natives: &NativeFunctionTable) -> MoveVM {
    let config = move_vm_config::runtime::VMConfig {
        binary_config: BinaryConfig::new_unpublishable(),
        ..Default::default()
    };
    MoveVM::new_with_config(natives.clone(), config).expect("the IOTA natives are well-formed")
}

/// Every compiled module, root package and dependencies, published in
/// dependency order.
pub(crate) fn storage(plan: &TestPlan) -> Result<InMemoryStorage> {
    let modules = plan
        .module_info
        .values()
        .map(|info| &info.module)
        .chain(&plan.bytecode_deps_modules);
    let mut storage = InMemoryStorage::new();
    for module in Modules::new(modules)
        .compute_topological_order()
        .map_err(|e| Error::Storage(format!("{e:#}").into()))?
    {
        let mut bytes = Vec::new();
        module
            .serialize_with_version(module.version, &mut bytes)
            .map_err(|e| Error::Storage(format!("{e:#}").into()))?;
        storage.publish_or_overwrite_module(module.self_id(), bytes);
    }
    Ok(storage)
}

pub(crate) fn extensions<'a>() -> NativeContextExtensions<'a> {
    let metrics = Arc::new(LimitsMetrics::new(&prometheus_filtered::Registry::new()));
    let tx_context = TxContext::new_from_components(
        &Address::ZERO,
        &TransactionDigest::default(),
        &0,
        0,
        0,
        0,
        0,
        None,
        &PROTOCOL_CONFIG,
    );
    let mut extensions = NativeContextExtensions::default();
    extensions.add(ObjectRuntime::new(
        &TEST_STORE,
        BTreeMap::new(),
        false,
        &PROTOCOL_CONFIG,
        metrics,
        0,
    ));
    extensions.add(NativesCostTable::from_protocol_config(&PROTOCOL_CONFIG));
    extensions.add(ProtocolConfigTestOverrides::default());
    extensions.add(AuthenticationContext::new_for_testing(Rc::new(
        RefCell::new(AuthContext::new_for_testing()),
    )));
    extensions.add(TransactionContext::new_for_testing(Rc::new(RefCell::new(
        tx_context,
    ))));
    extensions.add(&TEST_STORE);
    extensions
}
