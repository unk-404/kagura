use kagura_value::{Heap, Value};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HostFunctionId(u32);

impl HostFunctionId {
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    pub const fn index(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct HostError {
    pub message: String,
}

impl HostError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub struct HostCall<'a> {
    heap: &'a mut Heap,
    args: &'a [Value],
}

impl<'a> HostCall<'a> {
    pub fn new(heap: &'a mut Heap, args: &'a [Value]) -> Self {
        Self { heap, args }
    }

    pub fn args(&self) -> &[Value] {
        self.args
    }

    pub fn heap(&self) -> &Heap {
        self.heap
    }

    pub fn heap_mut(&mut self) -> &mut Heap {
        self.heap
    }
}

pub trait Host {
    fn resolve(&self, path: &str) -> Option<HostFunctionId>;
    fn invoke(&mut self, id: HostFunctionId, call: &mut HostCall<'_>) -> Result<Value, HostError>;
}
