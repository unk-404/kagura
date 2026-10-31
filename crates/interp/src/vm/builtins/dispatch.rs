use kagura_object::ObjectId;
use kagura_support::Span;
use kagura_value::Value;

use crate::error::RuntimeError;

use super::Interpreter;
use super::date;
use super::method_id::{BuiltinMethodId, MethodOwner};
use super::{math_operation, parse_float_prefix, parse_int_value};

impl<'a> Interpreter<'a> {
    pub(crate) fn invoke_builtin_method(
        &mut self,
        id: BuiltinMethodId,
        owner: MethodOwner,
        object: Value,
        args: &[Value],
        span: Span,
        roots: &[Value],
    ) -> Result<Value, RuntimeError> {
        if owner == MethodOwner::Array
            && let Some(object_id) = object.object_offset().map(ObjectId::new)
        {
            return self
                .call_array_builtin(object, object_id, id, args, span, roots)?
                .ok_or_else(|| RuntimeError::type_error("method is not supported", span));
        }
        if owner == MethodOwner::String && object.is_string() {
            let text = self.heap.as_str(object).unwrap_or_default().to_owned();
            return self
                .call_string_builtin(&text, id, args, span)?
                .ok_or_else(|| RuntimeError::type_error("method is not supported", span));
        }
        if owner == MethodOwner::Number
            && let Some(number) = self.heap.as_number(object)
        {
            return self
                .call_number_builtin(number, id, args, span)?
                .ok_or_else(|| RuntimeError::type_error("method is not supported", span));
        }
        if owner == MethodOwner::Boolean
            && let Some(boolean) = object.as_boolean()
        {
            return self
                .call_boolean_builtin(boolean, id, args, span)?
                .ok_or_else(|| RuntimeError::type_error("method is not supported", span));
        }
        if matches!(owner, MethodOwner::Object | MethodOwner::Function)
            && let Some(object_id) = object.object_offset().map(ObjectId::new)
            && let Some(value) =
                self.call_object_builtin(object, object_id, id, args, span, roots)?
        {
            return Ok(value);
        }
        self.call_builtin(object, id.name(), args, span, roots)?
            .ok_or_else(|| RuntimeError::type_error("method is not supported", span))
    }

    pub(crate) fn call_builtin(
        &mut self,
        object: Value,
        key: &str,
        args: &[Value],
        span: Span,
        roots: &[Value],
    ) -> Result<Option<Value>, RuntimeError> {
        if object.object_offset().is_some() {
            if let Some(wrapper) = self.get_own_data_property(object, "__wrapper")?
                && self.heap.as_str(wrapper).is_some()
                && let Some(primitive) = self.get_own_data_property(object, "__primitive")?
            {
                if key == "valueOf" {
                    return Ok(Some(primitive));
                }
                if key == "toString" {
                    let text = self.heap.to_js_string(primitive);
                    return Ok(Some(self.heap.string(&text)));
                }
                return self.call_builtin(primitive, key, args, span, roots);
            }
            let marker = self.get_property(object, "__date", span)?;
            if marker.as_boolean() == Some(true)
                && let Some(value) = self.call_date_method(object, key, args, span)?
            {
                return Ok(Some(value));
            }
        }
        if let Some(boolean) = object.as_boolean() {
            let Some(method) = BuiltinMethodId::from_name(key) else {
                return Ok(None);
            };
            return self.call_boolean_builtin(boolean, method, args, span);
        }
        if let Some(number) = self.heap.as_number(object) {
            let Some(method) = BuiltinMethodId::from_name(key) else {
                return Ok(None);
            };
            return self.call_number_builtin(number, method, args, span);
        }
        if object.is_string() {
            let text = self.heap.as_str(object).unwrap_or_default().to_owned();
            let Some(method) = BuiltinMethodId::from_name(key) else {
                return Ok(None);
            };
            return self.call_string_builtin(&text, method, args, span);
        }
        let Ok(object_id) = self.object_id(object, span) else {
            return Ok(None);
        };
        let Some(method) = BuiltinMethodId::from_name(key) else {
            return Ok(None);
        };
        if let Some(value) =
            self.call_object_builtin(object, object_id, method, args, span, roots)?
        {
            return Ok(Some(value));
        }

        if !self
            .objects
            .is_array(object_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Ok(None);
        }
        let Some(method) = BuiltinMethodId::from_name(key) else {
            return Ok(None);
        };
        self.call_array_builtin(object, object_id, method, args, span, roots)
    }

    pub(crate) fn flatten_array(
        &mut self,
        array: ObjectId,
        depth: u32,
        out: &mut Vec<Value>,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let length = self
            .objects
            .array_length(array)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        for index in 0..length {
            if !self
                .objects
                .array_has(array, index)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            {
                continue;
            }
            let value = self
                .objects
                .array_get(array, index)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            if depth > 0
                && let Some(value_id) = value.object_offset().map(ObjectId::new)
                && self
                    .objects
                    .is_array(value_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            {
                self.flatten_array(value_id, depth - 1, out, span)?;
            } else {
                out.push(value);
            }
        }
        Ok(())
    }

    pub(crate) fn call_builtin_id(
        &mut self,
        id: kagura_bytecode::builtins::BuiltinId,
        args: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        use kagura_bytecode::builtins::BuiltinId;
        match id {
            BuiltinId::JsonParse => {
                let text = self
                    .heap
                    .to_js_string(args.first().copied().unwrap_or_else(Value::undefined));
                self.json_parse_native(&text, span)
            }
            BuiltinId::JsonStringify => {
                let value = args.first().copied().unwrap_or_else(Value::undefined);
                match self.json_stringify(value, span)? {
                    Some(text) => Ok(self.heap.string(&text)),
                    None => Ok(Value::undefined()),
                }
            }
            BuiltinId::ObjectProtoToStringCall => {
                let value = args.first().copied().unwrap_or_else(Value::undefined);
                let tag = self.object_to_string_tag(value, span)?;
                Ok(self.heap.string(&format!("[object {tag}]")))
            }
            BuiltinId::ObjectProtoHasOwnCall => {
                let target = args.first().copied().unwrap_or_else(Value::undefined);
                let key = self
                    .heap
                    .to_js_string(args.get(1).copied().unwrap_or_else(Value::undefined));
                let own = self.has_own_property(target, &key, span)?;
                Ok(Value::boolean(own))
            }
            BuiltinId::MathAbs
            | BuiltinId::MathAcos
            | BuiltinId::MathAsin
            | BuiltinId::MathAtan
            | BuiltinId::MathAtan2
            | BuiltinId::MathCeil
            | BuiltinId::MathClz32
            | BuiltinId::MathCos
            | BuiltinId::MathExp
            | BuiltinId::MathFloor
            | BuiltinId::MathFround
            | BuiltinId::MathHypot
            | BuiltinId::MathImul
            | BuiltinId::MathLog
            | BuiltinId::MathLog10
            | BuiltinId::MathLog2
            | BuiltinId::MathMax
            | BuiltinId::MathMin
            | BuiltinId::MathPow
            | BuiltinId::MathRound
            | BuiltinId::MathSign
            | BuiltinId::MathSin
            | BuiltinId::MathSqrt
            | BuiltinId::MathTan
            | BuiltinId::MathTrunc
            | BuiltinId::MathCbrt => {
                let operation = math_operation(id).expect("math builtin operation");
                self.call_math(operation, args, span)
            }
            BuiltinId::ParseInt => {
                let parsed = parse_int_value(self.heap, args);
                Ok(self.heap.number(parsed))
            }
            BuiltinId::ParseFloat => {
                let text = self
                    .heap
                    .to_js_string(args.first().copied().unwrap_or_else(Value::undefined));
                Ok(self.heap.number(parse_float_prefix(&text)))
            }
            BuiltinId::IsNaN => {
                let number = self
                    .heap
                    .to_number(args.first().copied().unwrap_or_else(Value::undefined))
                    .unwrap_or(f64::NAN);
                Ok(Value::boolean(number.is_nan()))
            }
            BuiltinId::IsFinite => {
                let number = self
                    .heap
                    .to_number(args.first().copied().unwrap_or_else(Value::undefined))
                    .unwrap_or(f64::NAN);
                Ok(Value::boolean(number.is_finite()))
            }
            BuiltinId::DateNow => Ok(self.heap.number(date::now_millis())),
            BuiltinId::DateParse => {
                let text = self
                    .heap
                    .to_js_string(args.first().copied().unwrap_or_else(Value::undefined));
                Ok(self.heap.number(date::parse(&text)))
            }
            BuiltinId::DateUtc => Ok(self.heap.number(date::utc(args, self.heap))),
        }
    }
}
