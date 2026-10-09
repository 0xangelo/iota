// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// Modifications Copyright (c) 2025 IOTA Stiftung
// SPDX-License-Identifier: Apache-2.0

#[cfg(any(debug_assertions, feature = "tracing"))]
use ::{
    move_binary_format::file_format::Bytecode,
    move_vm_types::values::Locals,
    once_cell::sync::Lazy,
    std::{
        env,
        fs::{File, OpenOptions},
        io::Write,
        process,
        sync::Mutex,
        thread,
    },
};

#[cfg(any(debug_assertions, feature = "tracing"))]
use crate::debug::DebugContext;
#[cfg(any(debug_assertions, feature = "tracing"))]
use crate::{
    interpreter::Interpreter,
    loader::{Function, Loader},
};

#[cfg(any(debug_assertions, feature = "tracing"))]
const MOVE_VM_TRACING_ENV_VAR_NAME: &str = "MOVE_VM_TRACE";

#[cfg(any(debug_assertions, feature = "tracing"))]
const MOVE_VM_STEPPING_ENV_VAR_NAME: &str = "MOVE_VM_STEP";

#[cfg(any(debug_assertions, feature = "tracing"))]
static FILE_PATH: Lazy<String> = Lazy::new(|| {
    env::var(MOVE_VM_TRACING_ENV_VAR_NAME).unwrap_or_else(|_| "move_vm_trace.trace".to_string())
});

#[cfg(any(debug_assertions, feature = "tracing"))]
static TRACING_ENABLED: Lazy<bool> = Lazy::new(|| env::var(MOVE_VM_TRACING_ENV_VAR_NAME).is_ok());

#[cfg(any(debug_assertions, feature = "tracing"))]
static DEBUGGING_ENABLED: Lazy<bool> =
    Lazy::new(|| env::var(MOVE_VM_STEPPING_ENV_VAR_NAME).is_ok());

#[cfg(any(debug_assertions, feature = "tracing"))]
static LOGGING_FILE: Lazy<Mutex<File>> = Lazy::new(|| {
    Mutex::new(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&*FILE_PATH)
            .unwrap(),
    )
});

#[cfg(any(debug_assertions, feature = "tracing"))]
static DEBUG_CONTEXT: Lazy<Mutex<DebugContext>> = Lazy::new(|| Mutex::new(DebugContext::new()));

// Only include in debug builds
#[cfg(any(debug_assertions, feature = "tracing"))]
pub(crate) fn trace(
    function_desc: &Function,
    locals: &Locals,
    pc: u16,
    instr: &Bytecode,
    loader: &Loader,
    interp: &Interpreter,
) {
    if *TRACING_ENABLED {
        let f = &mut *LOGGING_FILE.lock().unwrap();
        writeln!(
            f,
            "{}-{:?},{},{},{:?}",
            process::id(),
            thread::current().id(),
            function_desc.pretty_string(),
            pc,
            instr,
        )
        .unwrap();
    }
    if *DEBUGGING_ENABLED {
        DEBUG_CONTEXT
            .lock()
            .unwrap()
            .debug_loop(function_desc, locals, pc, instr, loader, interp);
    }
}

#[macro_export]
macro_rules! trace {
    ($function_desc:expr, $locals:expr, $pc:expr, $instr:tt, $resolver:expr, $interp:expr) => {
        #[cfg(feature = "coverage")]
        $crate::tracing::coverage::record(&$function_desc, $pc);
        // Only include this code in debug releases
        #[cfg(any(debug_assertions, feature = "tracing"))]
        $crate::tracing::trace(
            &$function_desc,
            $locals,
            $pc,
            &$instr,
            $resolver.loader(),
            $interp,
        )
    };
}

/// In-memory instruction coverage, for callers that drive the VM themselves:
/// the same information as the `MOVE_VM_TRACE` file without serializing every
/// instruction through it.
///
/// Recording is per thread; drain it with [`take`](coverage::take) on the
/// thread that executed the code, before reusing that thread for an unrelated
/// set of modules.
#[cfg(feature = "coverage")]
pub mod coverage {
    use std::{
        cell::RefCell,
        collections::HashMap,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use move_binary_format::file_format::FunctionDefinitionIndex;
    use move_core_types::{identifier::Identifier, language_storage::ModuleId};

    use crate::loader::Function;

    static ENABLED: AtomicUsize = AtomicUsize::new(0);

    thread_local! {
        static RECORDER: RefCell<Recorder> = RefCell::default();
    }

    /// Keeps recording on, on every thread, while alive.
    #[must_use = "recording stops when the guard is dropped"]
    pub struct Enabled(());

    impl Drop for Enabled {
        fn drop(&mut self) {
            ENABLED.fetch_sub(1, Ordering::Relaxed);
        }
    }

    /// Start recording the instructions executed by any VM in this process.
    pub fn enable() -> Enabled {
        ENABLED.fetch_add(1, Ordering::Relaxed);
        Enabled(())
    }

    /// The distinct program counters executed in one function.
    #[derive(Debug)]
    pub struct FunctionHits {
        pub module: ModuleId,
        pub function: Identifier,
        pub pcs: Vec<u16>,
    }

    /// Drain the hits recorded on the calling thread.
    pub fn take() -> Vec<FunctionHits> {
        RECORDER
            .take()
            .entries
            .into_iter()
            .map(|entry| FunctionHits {
                module: entry.module,
                function: entry.function,
                pcs: (0..entry.pcs.len() * 64)
                    .filter(|pc| entry.pcs[pc / 64] & (1 << (pc % 64)) != 0)
                    .map(|pc| pc as u16)
                    .collect(),
            })
            .collect()
    }

    #[inline]
    pub(crate) fn record(function: &Function, pc: u16) {
        if ENABLED.load(Ordering::Relaxed) == 0 {
            return;
        }
        RECORDER.with_borrow_mut(|recorder| recorder.record(function, pc));
    }

    #[derive(Default)]
    struct Recorder {
        entries: Vec<Entry>,
        // Keyed by module and definition index rather than by `Function` address: a `Function` is
        // freed with its VM, and a later allocation may reuse the address.
        index: HashMap<ModuleId, HashMap<FunctionDefinitionIndex, usize>>,
        last: Option<usize>,
    }

    struct Entry {
        module: ModuleId,
        definition: FunctionDefinitionIndex,
        function: Identifier,
        pcs: Vec<u64>,
    }

    impl Recorder {
        fn record(&mut self, function: &Function, pc: u16) {
            let slot = match self.last {
                Some(slot)
                    if self.entries[slot].definition == function.index()
                        && &self.entries[slot].module == function.module_id() =>
                {
                    slot
                }
                _ => self.slot(function),
            };
            self.last = Some(slot);
            let pcs = &mut self.entries[slot].pcs;
            let word = pc as usize / 64;
            if word >= pcs.len() {
                pcs.resize(word + 1, 0);
            }
            pcs[word] |= 1 << (pc % 64);
        }

        fn slot(&mut self, function: &Function) -> usize {
            if !self.index.contains_key(function.module_id()) {
                self.index
                    .insert(function.module_id().clone(), HashMap::new());
            }
            let by_definition = self
                .index
                .get_mut(function.module_id())
                .expect("inserted above");
            *by_definition.entry(function.index()).or_insert_with(|| {
                self.entries.push(Entry {
                    module: function.module_id().clone(),
                    definition: function.index(),
                    function: Identifier::new(function.name())
                        .expect("loaded function names are valid"),
                    pcs: vec![0; function.code().len().div_ceil(64)],
                });
                self.entries.len() - 1
            })
        }
    }
}
