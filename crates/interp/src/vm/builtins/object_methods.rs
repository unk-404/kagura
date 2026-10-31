use kagura_object::{ObjectId, PropertyKey};
use kagura_support::Span;
use kagura_value::Value;

use crate::error::RuntimeError;

use super::Interpreter;
use super::method_id::BuiltinMethodId;

impl<'a> Interpreter<'a> {
    pub(crate) fn call_object_builtin(
        &mut self,
        object: Value,
        object_id: ObjectId,
        method: BuiltinMethodId,
        args: &[Value],
        span: Span,
        roots: &[Value],
    ) -> Result<Option<Value>, RuntimeError> {
        let is_function = self
            .objects
            .is_function(object_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if is_function && matches!(method, BuiltinMethodId::Bind) {
            let receiver = args.first().copied().unwrap_or_else(Value::undefined);
            let array = self
                .objects
                .allocate_array(self.shapes.root())
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            for argument in args.get(1..).unwrap_or_default() {
                self.objects
                    .array_push(array, *argument)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            }
            let (function, env_value) = self
                .objects
                .function_data(object_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let bound = self
                .objects
                .allocate_bound_function(
                    self.shapes.bound_function_shape(),
                    function,
                    env_value,
                    receiver,
                    Value::object(array.index()),
                    object,
                )
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            return Ok(Some(Value::object(bound.index())));
        }
        if is_function && matches!(method, BuiltinMethodId::Call | BuiltinMethodId::Apply) {
            let receiver = args.first().copied().unwrap_or_else(Value::undefined);
            let rest = if matches!(method, BuiltinMethodId::Call) {
                args.get(1..).unwrap_or_default().to_vec()
            } else {
                let values = args.get(1).copied().unwrap_or_else(Value::undefined);
                if values.is_undefined() {
                    Vec::new()
                } else {
                    let array = self.object_id(values, span)?;
                    if !self
                        .objects
                        .is_array(array)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        return Err(RuntimeError::type_error(
                            "apply expects an array-like argument list",
                            span,
                        ));
                    }
                    let length = self
                        .objects
                        .array_length(array)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let mut values = Vec::with_capacity(length as usize);
                    for index in 0..length {
                        values.push(
                            self.objects
                                .array_get(array, index)
                                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?,
                        );
                    }
                    values
                }
            };
            return self
                .invoke_method(object, &rest, roots, receiver, span)
                .map(Some);
        }
        if matches!(method, BuiltinMethodId::HasOwnProperty) {
            let key = self
                .heap
                .to_js_string(args.first().copied().unwrap_or_else(Value::undefined));
            return self
                .has_own_property(object, &key, span)
                .map(|own| Some(Value::boolean(own)));
        }
        if matches!(method, BuiltinMethodId::IsPrototypeOf) {
            let mut value = args.first().copied().unwrap_or_else(Value::undefined);
            while let Some(offset) = value.object_offset() {
                let id = ObjectId::new(offset);
                let shape = self
                    .objects
                    .shape(id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let Some(prototype) = self
                    .shapes
                    .get(shape)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    .prototype
                else {
                    break;
                };
                if prototype == object_id {
                    return Ok(Some(Value::boolean(true)));
                }
                value = Value::object(prototype.index());
            }
            return Ok(Some(Value::boolean(false)));
        }
        if matches!(method, BuiltinMethodId::PropertyIsEnumerable) {
            let key = self
                .heap
                .to_js_string(args.first().copied().unwrap_or_else(Value::undefined));
            let shape = self
                .objects
                .shape(object_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let descriptor = self
                .shapes
                .lookup(shape, &PropertyKey::from_string(&key))
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            return Ok(Some(Value::boolean(
                descriptor.is_some_and(|descriptor| descriptor.attributes.enumerable),
            )));
        }
        if matches!(method, BuiltinMethodId::ValueOf) {
            return Ok(Some(object));
        }
        if matches!(
            method,
            BuiltinMethodId::ToString | BuiltinMethodId::ToLocaleString
        ) {
            let tag = self.object_to_string_tag(object, span)?;
            return Ok(Some(self.heap.string(&format!("[object {tag}]"))));
        }
        Ok(None)
    }
}
