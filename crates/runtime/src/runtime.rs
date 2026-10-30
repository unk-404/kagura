use kagura_bytecode::{Module, compile};
use kagura_interp::{Host, Interpreter};
use kagura_object::{ObjectHeap, ShapeTable};
use kagura_parser::parse;
use kagura_value::{Heap, Value};
use tracing::instrument;

use crate::error::RunError;

pub struct Runtime {
    heap: Heap,
    objects: ObjectHeap,
    shapes: ShapeTable,
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
        }
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
        let mut interpreter = Interpreter::new(
            module,
            &mut self.heap,
            &mut self.objects,
            &mut self.shapes,
            host,
        );
        let value = interpreter.run()?;
        #[cfg(feature = "debug")]
        tracing::debug!(
            heap_used = self.heap.used(),
            heap_capacity = self.heap.capacity(),
            object_used = self.objects.used(),
            object_capacity = self.objects.capacity(),
            "runtime heap usage"
        );
        Ok(value)
    }
}
