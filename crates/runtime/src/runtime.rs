use kagura_bytecode::{Module, compile};
use kagura_interp::{Host, Interpreter};
use kagura_object::{GcStats, ObjectHeap, ShapeTable};
use kagura_parser::parse;
use kagura_value::{Heap, Value};
use tracing::instrument;

use crate::error::RunError;

pub struct Runtime {
    heap: Heap,
    objects: ObjectHeap,
    shapes: ShapeTable,
    step_limit: u64,
    last_property_ic: (u64, u64),
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new(1 << 20)
    }
}

impl Runtime {
    pub fn new(heap_capacity: usize) -> Self {
        Self {
            heap: Heap::new(heap_capacity),
            objects: ObjectHeap::new(heap_capacity),
            shapes: ShapeTable::new(),
            step_limit: 1_000_000,
            last_property_ic: (0, 0),
        }
    }

    pub fn set_step_limit(&mut self, step_limit: u64) {
        self.step_limit = step_limit;
    }

    pub fn property_ic_stats(&self) -> (u64, u64) {
        self.last_property_ic
    }

    pub fn heap_used(&self) -> usize {
        self.heap.used()
    }

    pub fn heap_capacity(&self) -> usize {
        self.heap.capacity()
    }

    pub fn object_used(&self) -> usize {
        self.objects.used()
    }

    pub fn object_capacity(&self) -> usize {
        self.objects.capacity()
    }

    pub fn object_live_bytes(&self) -> u32 {
        self.objects.live_bytes()
    }

    pub fn collect_garbage(&mut self, roots: &[Value]) -> GcStats {
        self.objects.collect(roots)
    }

    #[instrument(skip(self, source, host), fields(bytes = source.len()))]
    pub fn run(&mut self, source: &str, host: &mut dyn Host) -> Result<Value, RunError> {
        let program = parse(source)?;
        let module = compile(&program)?;
        self.run_module(&module, host)
    }

    #[instrument(skip(self, bytes, host), fields(bytes = bytes.len()))]
    pub fn run_bytecode(&mut self, bytes: &[u8], host: &mut dyn Host) -> Result<Value, RunError> {
        let module = Module::decode(bytes)?;
        self.run_module(&module, host)
    }

    #[instrument(skip(self, module, host), fields(version = module.version))]
    pub fn run_module(&mut self, module: &Module, host: &mut dyn Host) -> Result<Value, RunError> {
        let (value, property_ic) = {
            let mut interpreter = Interpreter::new(
                module,
                &mut self.heap,
                &mut self.objects,
                &mut self.shapes,
                host,
                self.step_limit,
            );
            let value = interpreter.run()?;
            (value, interpreter.property_ic_stats())
        };
        self.last_property_ic = property_ic;
        self.objects.collect(&[value]);
        #[cfg(feature = "debug")]
        tracing::debug!(
            heap_used = self.heap.used(),
            heap_capacity = self.heap.capacity(),
            object_used = self.objects.used(),
            object_capacity = self.objects.capacity(),
            object_live_bytes = self.objects.live_bytes(),
            "runtime heap usage"
        );
        Ok(value)
    }
}
