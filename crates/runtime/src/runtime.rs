use kagura_bytecode::{Module, compile};
use kagura_interp::{GcObservability, Host, Interpreter, SafepointStats};
use kagura_object::{GcStats, ObjectHeap, ShapeTable};
use kagura_parser::parse;
use kagura_value::{Heap, Value};
use tracing::instrument;

use crate::error::RunError;
use crate::handles::{Handle, HandleTable, WeakHandle, WeakHandleTable};

pub struct Runtime {
    heap: Heap,
    objects: ObjectHeap,
    shapes: ShapeTable,
    handles: HandleTable,
    weak_handles: WeakHandleTable,
    step_limit: u64,
    last_property_ic: (u64, u64),
    last_keyed_ic: (u64, u64),
    last_keyed_store_ic: (u64, u64),
    last_call_ic: (u64, u64),
    last_builtin_method_hits: u64,
    last_safepoints: SafepointStats,
    last_gc: GcObservability,
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
            handles: HandleTable::new(),
            weak_handles: WeakHandleTable::new(),
            step_limit: 1_000_000,
            last_property_ic: (0, 0),
            last_keyed_ic: (0, 0),
            last_keyed_store_ic: (0, 0),
            last_call_ic: (0, 0),
            last_builtin_method_hits: 0,
            last_safepoints: SafepointStats::default(),
            last_gc: GcObservability::default(),
        }
    }

    pub fn set_step_limit(&mut self, step_limit: u64) {
        self.step_limit = step_limit;
    }

    pub fn property_ic_stats(&self) -> (u64, u64) {
        self.last_property_ic
    }

    pub fn keyed_ic_stats(&self) -> (u64, u64) {
        self.last_keyed_ic
    }

    pub fn keyed_store_ic_stats(&self) -> (u64, u64) {
        self.last_keyed_store_ic
    }

    pub fn call_ic_stats(&self) -> (u64, u64) {
        self.last_call_ic
    }

    pub fn safepoint_stats(&self) -> &SafepointStats {
        &self.last_safepoints
    }

    pub fn builtin_method_hits(&self) -> u64 {
        self.last_builtin_method_hits
    }

    pub fn gc_observability(&self) -> &GcObservability {
        &self.last_gc
    }

    pub fn create_handle(&mut self, value: Value) -> Handle {
        self.handles.create(value)
    }

    pub fn drop_handle(&mut self, handle: Handle) -> Option<Value> {
        self.handles.drop_handle(handle)
    }

    pub fn handle_value(&self, handle: Handle) -> Option<Value> {
        self.handles.get(handle)
    }

    pub fn handle_count(&self) -> usize {
        self.handles.len()
    }

    pub fn create_weak_handle(&mut self, value: Value) -> WeakHandle {
        self.weak_handles.create(value)
    }

    pub fn weak_value(&self, handle: WeakHandle) -> Option<Value> {
        self.weak_handles
            .get(handle)
            .filter(|value| self.objects.is_live_value(*value))
    }

    pub fn drop_weak_handle(&mut self, handle: WeakHandle) {
        self.weak_handles.drop_handle(handle);
    }

    pub fn weak_handle_count(&self) -> usize {
        self.weak_handles.len()
    }

    pub fn take_cleared_weak_handles(&mut self) -> Vec<WeakHandle> {
        self.weak_handles.take_cleared()
    }

    /// Allocates a plain object owned by the embedder, not by a script.
    pub fn allocate_object(&mut self) -> Option<Value> {
        self.objects
            .allocate(self.shapes.root())
            .ok()
            .map(ObjectHeap::value)
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
        let mut all_roots = roots.to_vec();
        all_roots.extend(self.handles.roots());
        let stats = self.objects.collect(&all_roots);
        self.weak_handles
            .retain_live(|value| self.objects.is_live_value(value));
        stats
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
        let external_roots: Vec<Value> = self.handles.roots().collect();
        let (
            value,
            property_ic,
            keyed_ic,
            keyed_store_ic,
            call_ic,
            builtin_method_hits,
            safepoints,
            gc,
        ) = {
            let mut interpreter = Interpreter::new(
                module,
                &mut self.heap,
                &mut self.objects,
                &mut self.shapes,
                &external_roots,
                host,
                self.step_limit,
            );
            let value = interpreter.run()?;
            (
                value,
                interpreter.property_ic_stats(),
                interpreter.keyed_ic_stats(),
                interpreter.keyed_store_ic_stats(),
                interpreter.call_ic_stats(),
                interpreter.builtin_method_hits(),
                interpreter.safepoint_stats(),
                interpreter.gc_observability(),
            )
        };
        self.last_property_ic = property_ic;
        self.last_keyed_ic = keyed_ic;
        self.last_keyed_store_ic = keyed_store_ic;
        self.last_call_ic = call_ic;
        self.last_builtin_method_hits = builtin_method_hits;
        self.last_safepoints = safepoints;
        self.last_gc = gc;
        let mut roots = vec![value];
        roots.extend(self.handles.roots());
        self.objects.collect(&roots);
        self.weak_handles
            .retain_live(|value| self.objects.is_live_value(value));
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
