use kagura_object::ObjectId;
use kagura_support::Span;
use kagura_value::Value;

use crate::error::RuntimeError;

use super::Interpreter;

impl<'a> Interpreter<'a> {
    pub(crate) fn invoke_method(
        &mut self,
        callee: Value,
        args: &[Value],
        caller_roots: &[Value],
        receiver: Value,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let object = self.object_id(callee, span)?;
        if !self
            .objects
            .is_function(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Err(RuntimeError::type_error("value is not callable", span));
        }
        if self
            .objects
            .is_bound(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return self.invoke_bound(callee, args, caller_roots, span);
        }
        let (function, env_value) = self
            .objects
            .function_data(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let closure_env = env_value.object_offset().map(ObjectId::new);
        let receiver = self.callee_receiver(callee, function, Some(receiver), span)?;
        self.call(
            function as usize,
            args,
            caller_roots,
            closure_env,
            Some(callee),
            receiver,
        )
    }

    pub(crate) fn callee_receiver(
        &self,
        callee: Value,
        function_index: u32,
        receiver: Option<Value>,
        span: Span,
    ) -> Result<Option<Value>, RuntimeError> {
        if self
            .module
            .functions
            .get(function_index as usize)
            .is_some_and(|proto| proto.name == "<arrow>")
        {
            let object = self.object_id(callee, span)?;
            return self
                .objects
                .function_this(object)
                .map(Some)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span));
        }
        Ok(receiver)
    }

    pub(crate) fn invoke_callable(
        &mut self,
        callee: Value,
        args: &[Value],
        caller_roots: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let object = self.object_id(callee, span)?;
        if !self
            .objects
            .is_function(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Err(RuntimeError::type_error("value is not callable", span));
        }
        if self
            .objects
            .is_bound(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return self.invoke_bound(callee, args, caller_roots, span);
        }
        let (function, env_value) = self
            .objects
            .function_data(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let closure_env = env_value.object_offset().map(ObjectId::new);
        let receiver = self.callee_receiver(callee, function, None, span)?;
        self.call(
            function as usize,
            args,
            caller_roots,
            closure_env,
            Some(callee),
            receiver,
        )
    }

    pub(crate) fn call_with_this(
        &mut self,
        target: Value,
        this_value: Value,
        args: &[Value],
        caller_roots: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let object = self.object_id(target, span)?;
        if !self
            .objects
            .is_function(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Err(RuntimeError::type_error("value is not callable", span));
        }
        let (function, _) = self
            .objects
            .function_data(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if function == u32::MAX {
            if let Some((id, owner)) = self.builtin_methods.get(&object.index()).copied() {
                self.builtin_method_hits = self.builtin_method_hits.saturating_add(1);
                return self.invoke_builtin_method(id, owner, this_value, args, span, caller_roots);
            }
            let name = self.get_property(target, "name", span)?;
            let name = self
                .heap
                .as_str(name)
                .ok_or_else(|| RuntimeError::type_error("invalid builtin function", span))?
                .to_owned();
            return self
                .call_builtin(this_value, &name, args, span, caller_roots)?
                .ok_or_else(|| RuntimeError::type_error("method is not supported", span));
        }
        self.invoke_method(target, args, caller_roots, this_value, span)
    }

    pub(crate) fn invoke_bound(
        &mut self,
        callee: Value,
        args: &[Value],
        caller_roots: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let mut current = callee;
        let mut bound_this = Value::undefined();
        let mut levels = Vec::new();
        loop {
            let object = self.object_id(current, span)?;
            if !self
                .objects
                .is_bound(object)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            {
                break;
            }
            let (this_value, arguments, target) = self
                .objects
                .bound_data(object)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            bound_this = this_value;
            let arguments = self.object_id(arguments, span)?;
            let length = self
                .objects
                .array_length(arguments)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let mut level = Vec::with_capacity(length as usize);
            for index in 0..length {
                level.push(
                    self.objects
                        .array_get(arguments, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?,
                );
            }
            levels.push(level);
            current = target;
        }
        let object = self.object_id(current, span)?;
        let (function, env_value) = self
            .objects
            .function_data(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let closure_env = env_value.object_offset().map(ObjectId::new);
        let receiver = if self
            .module
            .functions
            .get(function as usize)
            .is_some_and(|proto| proto.name == "<arrow>")
        {
            Some(
                self.objects
                    .function_this(object)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?,
            )
        } else {
            Some(bound_this)
        };
        let mut all_args = Vec::new();
        for level in levels.into_iter().rev() {
            all_args.extend(level);
        }
        all_args.extend_from_slice(args);
        self.call(
            function as usize,
            &all_args,
            caller_roots,
            closure_env,
            Some(current),
            receiver,
        )
    }
}
