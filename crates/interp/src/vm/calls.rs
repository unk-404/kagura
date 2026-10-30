use kagura_object::ObjectId;
use kagura_support::Span;
use kagura_value::Value;

use crate::error::{RuntimeError, RuntimeErrorKind};
use crate::host::{HostCall, HostFunctionId};

use super::{FrameContext, Interpreter, MAX_CALL_DEPTH, TryHandler};

impl<'a> Interpreter<'a> {
    pub(super) fn call(
        &mut self,
        index: usize,
        args: &[Value],
        caller_roots: &[Value],
        closure_env: Option<ObjectId>,
        self_binding: Option<Value>,
        receiver: Option<Value>,
    ) -> Result<Value, RuntimeError> {
        if self.depth >= MAX_CALL_DEPTH {
            return Err(RuntimeError::range(
                "maximum call depth exceeded",
                Span::empty(0),
            ));
        }
        let function = self.module.functions.get(index).cloned().ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorKind::Runtime,
                "missing function",
                Span::empty(0),
            )
        })?;
        let mut locals = vec![Value::undefined(); function.local_count as usize];
        let mut defined = vec![false; function.local_count as usize];
        for (slot, arg) in args.iter().take(function.param_count as usize).enumerate() {
            locals[slot] = *arg;
            defined[slot] = true;
        }
        defined[..function.param_count as usize].fill(true);

        let env = if function.uses_env {
            let env = self
                .objects
                .allocate_array(self.shapes.root())
                .map_err(|error| RuntimeError::runtime(error.to_string(), Span::empty(0)))?;
            for (slot, value) in locals.iter().enumerate() {
                let value = if defined[slot] { *value } else { Value::hole() };
                self.objects
                    .array_push(env, value)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), Span::empty(0)))?;
            }
            let parent = closure_env
                .map(|parent| Value::object(parent.index()))
                .unwrap_or_else(Value::undefined);
            self.objects
                .array_push(env, parent)
                .map_err(|error| RuntimeError::runtime(error.to_string(), Span::empty(0)))?;
            Some(env)
        } else {
            None
        };
        if let (Some(slot), Some(value)) = (function.self_slot, self_binding) {
            let slot = slot as usize;
            if slot < locals.len() {
                locals[slot] = value;
                defined[slot] = true;
                if let Some(env) = env {
                    self.objects
                        .array_set(env, slot as u32, value)
                        .map_err(|error| {
                            RuntimeError::runtime(error.to_string(), Span::empty(0))
                        })?;
                }
            }
        }

        self.depth += 1;
        let frame = FrameContext {
            caller_roots,
            env,
            closure_env,
            self_value: receiver,
        };
        let result = self.execute(index, &function, &mut locals, &mut defined, &frame);
        self.depth -= 1;
        result
    }

    pub(super) fn call_name(
        &mut self,
        name: &str,
        index: usize,
        args: &[Value],
        span: Span,
        caller_roots: &[Value],
    ) -> Result<Value, RuntimeError> {
        if let Some(function) = self.module.find_function(name) {
            return self.call(function, args, caller_roots, None, None, None);
        }
        let id = self.resolve_host(name, index, span)?;
        self.invoke_host(id, args, span)
    }

    pub(super) fn resolve_host(
        &mut self,
        path: &str,
        index: usize,
        span: Span,
    ) -> Result<HostFunctionId, RuntimeError> {
        if let Some(Some(id)) = self.host_cache.get(index) {
            return Ok(*id);
        }
        let id = self
            .host
            .resolve(path)
            .ok_or_else(|| RuntimeError::reference(format!("{path} is not defined"), span))?;
        if let Some(slot) = self.host_cache.get_mut(index) {
            *slot = Some(id);
        }
        Ok(id)
    }

    pub(super) fn invoke_host(
        &mut self,
        id: HostFunctionId,
        args: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let mut call = HostCall::new(&mut *self.heap, args);
        self.host
            .invoke(id, &mut call)
            .map_err(|error| RuntimeError::runtime(error.message, span))
    }

    pub(super) fn catch_error(
        &mut self,
        handlers: &mut Vec<TryHandler>,
        stack: &mut Vec<Value>,
        error: RuntimeError,
    ) -> Result<usize, RuntimeError> {
        let Some(handler) = handlers.pop() else {
            return Err(error);
        };
        stack.truncate(handler.stack_depth);
        let value = error
            .thrown
            .unwrap_or_else(|| self.heap.string(&error.message));
        stack.push(value);
        Ok(handler.catch_pc)
    }

    pub(super) fn capture_env(
        &mut self,
        mut env: ObjectId,
        level: u16,
        span: Span,
    ) -> Result<ObjectId, RuntimeError> {
        if level == 0 {
            return Err(RuntimeError::reference("invalid capture level", span));
        }
        for _ in 1..level {
            let length = self
                .objects
                .array_length(env)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            if length == 0 {
                return Err(RuntimeError::reference("broken closure environment", span));
            }
            let parent = self
                .objects
                .array_get(env, length - 1)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            env = self.object_id(parent, span)?;
        }
        Ok(env)
    }
}
