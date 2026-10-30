use kagura_support::FastHashMap;
use kagura_value::Value;

pub use kagura_interp::{Host, HostCall, HostError, HostFunctionId};

type HostFunction = Box<dyn for<'a> FnMut(&mut HostCall<'a>) -> Result<Value, HostError>>;

#[derive(Default)]
pub struct Bindings {
    names: FastHashMap<String, HostFunctionId>,
    functions: Vec<HostFunction>,
}

impl Bindings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind<F>(&mut self, path: impl Into<String>, function: F) -> HostFunctionId
    where
        F: for<'a> FnMut(&mut HostCall<'a>) -> Result<Value, HostError> + 'static,
    {
        let id = HostFunctionId::new(self.functions.len() as u32);
        self.functions.push(Box::new(function));
        self.names.insert(path.into(), id);
        id
    }

    pub fn resolve(&self, path: &str) -> Option<HostFunctionId> {
        self.names.get(path).copied()
    }

    pub fn invoke(
        &mut self,
        id: HostFunctionId,
        call: &mut HostCall<'_>,
    ) -> Result<Value, HostError> {
        let function = self
            .functions
            .get_mut(id.index() as usize)
            .ok_or_else(|| HostError::new("invalid host function id"))?;
        function(call)
    }

    pub fn len(&self) -> usize {
        self.functions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
    }
}

impl Host for Bindings {
    fn resolve(&self, path: &str) -> Option<HostFunctionId> {
        Bindings::resolve(self, path)
    }

    fn invoke(&mut self, id: HostFunctionId, call: &mut HostCall<'_>) -> Result<Value, HostError> {
        Bindings::invoke(self, id, call)
    }
}

pub trait CallExt {
    fn arg_count(&self) -> usize;
    fn arg(&self, index: usize) -> Option<Value>;
    fn arg_number(&self, index: usize) -> Result<f64, HostError>;
    fn arg_string(&self, index: usize) -> Result<String, HostError>;
    fn arg_boolean(&self, index: usize) -> Result<bool, HostError>;
}

impl CallExt for HostCall<'_> {
    fn arg_count(&self) -> usize {
        self.args().len()
    }

    fn arg(&self, index: usize) -> Option<Value> {
        self.args().get(index).copied()
    }

    fn arg_number(&self, index: usize) -> Result<f64, HostError> {
        let value = self
            .arg(index)
            .ok_or_else(|| HostError::new("missing numeric argument"))?;
        self.heap()
            .to_number(value)
            .filter(|number| !number.is_nan())
            .ok_or_else(|| HostError::new("argument is not a number"))
    }

    fn arg_string(&self, index: usize) -> Result<String, HostError> {
        let value = self
            .arg(index)
            .ok_or_else(|| HostError::new("missing string argument"))?;
        self.heap()
            .as_str(value)
            .map(str::to_owned)
            .ok_or_else(|| HostError::new("argument is not a string"))
    }

    fn arg_boolean(&self, index: usize) -> Result<bool, HostError> {
        let value = self
            .arg(index)
            .ok_or_else(|| HostError::new("missing boolean argument"))?;
        value
            .as_boolean()
            .ok_or_else(|| HostError::new("argument is not a boolean"))
    }
}
