use kagura_object::{ObjectId, PropertyKey, own_property_keys};
use kagura_support::Span;
use kagura_value::Value;

use crate::error::RuntimeError;

use super::Interpreter;

impl<'a> Interpreter<'a> {
    pub(super) fn call_global(
        &mut self,
        path: &str,
        args: &[Value],
        span: Span,
    ) -> Result<Option<Value>, RuntimeError> {
        if path == "Object.create" {
            let Some(prototype) = args.first().copied() else {
                return Err(RuntimeError::type_error(
                    "Object.create requires a prototype",
                    span,
                ));
            };
            let prototype = if prototype.is_null() {
                None
            } else {
                Some(self.object_id(prototype, span)?)
            };
            let shape = self
                .shapes
                .with_prototype(self.shapes.root(), prototype)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let object = self
                .objects
                .allocate(shape)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            return Ok(Some(Value::object(object.index())));
        }
        if path == "Object.getPrototypeOf" {
            let Some(target) = args.first().copied() else {
                return Err(RuntimeError::type_error(
                    "Object.getPrototypeOf requires an object",
                    span,
                ));
            };
            let object = self.object_id(target, span)?;
            let shape = self
                .objects
                .shape(object)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let prototype = self
                .shapes
                .get(shape)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                .prototype;
            return Ok(Some(
                prototype
                    .map(|prototype| Value::object(prototype.index()))
                    .unwrap_or_else(Value::null),
            ));
        }
        if path == "Object.setPrototypeOf" {
            let target = args.first().copied().ok_or_else(|| {
                RuntimeError::type_error("Object.setPrototypeOf requires an object", span)
            })?;
            let prototype = args.get(1).copied().ok_or_else(|| {
                RuntimeError::type_error("Object.setPrototypeOf requires a prototype", span)
            })?;
            let object = self.object_id(target, span)?;
            let prototype = if prototype.is_null() {
                None
            } else {
                Some(self.object_id(prototype, span)?)
            };
            let shape = self
                .objects
                .shape(object)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let shape = self
                .shapes
                .with_prototype(shape, prototype)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            self.objects
                .set_shape(object, shape)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            return Ok(Some(target));
        }
        let mode = match path {
            "Object.keys" => 0,
            "Object.values" => 1,
            "Object.entries" => 2,
            _ => return Ok(None),
        };
        let Some(target) = args.first().copied() else {
            return Err(RuntimeError::type_error(
                "Object method requires an argument",
                span,
            ));
        };
        let mut pairs = Vec::new();
        if let Some(text) = self.heap.as_str(target).map(|text| text.to_owned()) {
            for (index, ch) in text.chars().enumerate() {
                pairs.push((index.to_string(), self.heap.string(&ch.to_string())));
            }
        } else {
            let object = self.object_id(target, span)?;
            if self
                .objects
                .is_array(object)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            {
                let length = self
                    .objects
                    .array_length(object)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                for index in 0..length {
                    if !self
                        .objects
                        .array_has(object, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        continue;
                    }
                    let value = self
                        .objects
                        .array_get(object, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    pairs.push((index.to_string(), value));
                }
            } else {
                let shape = self
                    .objects
                    .shape(object)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let keys = own_property_keys(self.shapes, shape)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                for key in keys {
                    let descriptor = self
                        .shapes
                        .lookup(shape, &key)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    if !descriptor.is_some_and(|descriptor| descriptor.attributes.enumerable) {
                        continue;
                    }
                    let key_string = match &key {
                        PropertyKey::Index(index) => index.to_string(),
                        PropertyKey::String(key) => key.clone(),
                        PropertyKey::Symbol(_) => continue,
                    };
                    let value = self.get_property(target, &key_string, span)?;
                    pairs.push((key_string, value));
                }
            }
        }
        let result = self
            .objects
            .allocate_array(self.shapes.root())
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        for (key, value) in pairs {
            match mode {
                0 => {
                    let key = self.heap.string(&key);
                    self.objects
                        .array_push(result, key)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                1 => {
                    self.objects
                        .array_push(result, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                _ => {
                    let entry = self
                        .objects
                        .allocate_array(self.shapes.root())
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let key = self.heap.string(&key);
                    self.objects
                        .array_push(entry, key)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    self.objects
                        .array_push(entry, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    self.objects
                        .array_push(result, Value::object(entry.index()))
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
            }
        }
        Ok(Some(Value::object(result.index())))
    }

    pub(super) fn invoke_callable(
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
        let (function, env_value) = self
            .objects
            .function_data(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let closure_env = env_value.object_offset().map(ObjectId::new);
        self.call(
            function as usize,
            args,
            caller_roots,
            closure_env,
            Some(callee),
            None,
        )
    }

    pub(super) fn invoke_method(
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
        let (function, env_value) = self
            .objects
            .function_data(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let closure_env = env_value.object_offset().map(ObjectId::new);
        self.call(
            function as usize,
            args,
            caller_roots,
            closure_env,
            Some(callee),
            Some(receiver),
        )
    }

    pub(super) fn call_builtin(
        &mut self,
        object: Value,
        key: &str,
        args: &[Value],
        span: Span,
        roots: &[Value],
    ) -> Result<Option<Value>, RuntimeError> {
        let arg_index = |args: &[Value], index: usize| {
            args.get(index)
                .and_then(|value| value.as_smi())
                .unwrap_or(0)
                .max(0) as usize
        };
        if object.is_string() {
            let text = self.heap.as_str(object).unwrap_or_default().to_owned();
            return Ok(Some(match key {
                "charAt" => text
                    .chars()
                    .nth(arg_index(args, 0))
                    .map(|ch| self.heap.string(&ch.to_string()))
                    .unwrap_or_else(Value::undefined),
                "charCodeAt" => text
                    .encode_utf16()
                    .nth(arg_index(args, 0))
                    .map(|unit| self.heap.number(unit as f64))
                    .unwrap_or_else(Value::undefined),
                "indexOf" => {
                    let needle = args
                        .first()
                        .map(|value| self.heap.to_js_string(*value))
                        .unwrap_or_default();
                    let from = arg_index(args, 1);
                    let found = text
                        .char_indices()
                        .enumerate()
                        .find(|(index, (byte, _))| {
                            *index >= from && text[*byte..].starts_with(&needle)
                        })
                        .map(|(index, _)| index as f64)
                        .unwrap_or(-1.0);
                    self.heap.number(found)
                }
                "includes" => {
                    let needle = args
                        .first()
                        .map(|value| self.heap.to_js_string(*value))
                        .unwrap_or_default();
                    Value::boolean(text.contains(&needle))
                }
                "slice" => {
                    let length = text.chars().count();
                    let start = arg_index(args, 0).min(length);
                    let end = args
                        .get(1)
                        .and_then(|value| value.as_smi())
                        .map(|value| value.max(0) as usize)
                        .unwrap_or(length)
                        .min(length)
                        .max(start);
                    let value: String = text.chars().skip(start).take(end - start).collect();
                    self.heap.string(&value)
                }
                "toUpperCase" => self.heap.string(&text.to_uppercase()),
                "toLowerCase" => self.heap.string(&text.to_lowercase()),
                "trim" => self.heap.string(text.trim()),
                _ => return Ok(None),
            }));
        }
        let Ok(object_id) = self.object_id(object, span) else {
            return Ok(None);
        };
        if !self
            .objects
            .is_array(object_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Ok(None);
        }
        Ok(Some(match key {
            "push" => {
                for value in args {
                    self.objects
                        .array_push(object_id, *value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                self.heap.number(length as f64)
            }
            "pop" => self
                .objects
                .array_pop(object_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?,
            "join" => {
                let separator = args
                    .first()
                    .map(|value| self.heap.to_js_string(*value))
                    .unwrap_or_else(|| ",".into());
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let mut parts = Vec::with_capacity(length as usize);
                for index in 0..length {
                    let value = self
                        .objects
                        .array_get(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    if value.is_undefined() || value.is_null() {
                        parts.push(String::new());
                    } else {
                        parts.push(self.heap.to_js_string(value));
                    }
                }
                self.heap.string(&parts.join(&separator))
            }
            "indexOf" | "includes" => {
                let needle = args.first().copied().unwrap_or_else(Value::undefined);
                let from = arg_index(args, 1) as u32;
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let mut found = None;
                for index in from..length {
                    let has = self
                        .objects
                        .array_has(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    if !has && key == "indexOf" {
                        continue;
                    }
                    let value = if has {
                        self.objects
                            .array_get(object_id, index)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    } else {
                        Value::undefined()
                    };
                    if self.heap.strict_eq(value, needle) {
                        found = Some(index);
                        break;
                    }
                }
                if key == "includes" {
                    Value::boolean(found.is_some())
                } else {
                    found
                        .map(|index| self.heap.number(index as f64))
                        .unwrap_or_else(|| self.heap.number(-1.0))
                }
            }
            "slice" => {
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    as i32;
                let start = args
                    .first()
                    .and_then(|value| value.as_smi())
                    .unwrap_or(0)
                    .clamp(0, length);
                let end = args
                    .get(1)
                    .and_then(|value| value.as_smi())
                    .unwrap_or(length)
                    .clamp(start, length);
                let array = self
                    .objects
                    .allocate_array(self.shapes.root())
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                for index in start..end {
                    let has = self
                        .objects
                        .array_has(object_id, index as u32)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let value = if has {
                        self.objects
                            .array_get(object_id, index as u32)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    } else {
                        Value::hole()
                    };
                    self.objects
                        .array_push(array, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                Value::object(array.index())
            }
            "map" | "filter" | "forEach" => {
                let callback = args.first().copied().unwrap_or_else(Value::undefined);
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let result = if key == "forEach" {
                    None
                } else {
                    Some(
                        self.objects
                            .allocate_array(self.shapes.root())
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?,
                    )
                };
                let mut roots = roots.to_vec();
                if let Some(result) = result {
                    roots.push(Value::object(result.index()));
                }
                for index in 0..length {
                    let has = self
                        .objects
                        .array_has(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    if !has {
                        if key == "map"
                            && let Some(result) = result
                        {
                            self.objects
                                .array_push(result, Value::hole())
                                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                        }
                        continue;
                    }
                    let value = self
                        .objects
                        .array_get(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let callback_args = [value, self.heap.number(index as f64), object];
                    let mapped = self.invoke_callable(callback, &callback_args, &roots, span)?;
                    match key {
                        "map" => {
                            if let Some(result) = result {
                                self.objects.array_push(result, mapped).map_err(|error| {
                                    RuntimeError::runtime(error.to_string(), span)
                                })?;
                                roots.push(mapped);
                            }
                        }
                        "filter" => {
                            if let Some(result) = result
                                && self.heap.truthy(mapped)
                            {
                                self.objects.array_push(result, value).map_err(|error| {
                                    RuntimeError::runtime(error.to_string(), span)
                                })?;
                            }
                        }
                        _ => {}
                    }
                }
                result.map_or_else(Value::undefined, |array| Value::object(array.index()))
            }
            "some" | "every" | "find" => {
                let callback = args.first().copied().unwrap_or_else(Value::undefined);
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let roots = roots.to_vec();
                for index in 0..length {
                    let has = self
                        .objects
                        .array_has(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    if !has {
                        continue;
                    }
                    let value = self
                        .objects
                        .array_get(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let callback_args = [value, self.heap.number(index as f64), object];
                    let result = self.invoke_callable(callback, &callback_args, &roots, span)?;
                    let truthy = self.heap.truthy(result);
                    match key {
                        "some" if truthy => return Ok(Some(Value::boolean(true))),
                        "every" if !truthy => return Ok(Some(Value::boolean(false))),
                        "find" if truthy => return Ok(Some(value)),
                        _ => {}
                    }
                }
                if key == "some" {
                    Value::boolean(false)
                } else if key == "every" {
                    Value::boolean(true)
                } else {
                    Value::undefined()
                }
            }
            "reduce" => {
                let callback = args.first().copied().unwrap_or_else(Value::undefined);
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let mut index = 0;
                let mut accumulator = if let Some(initial) = args.get(1) {
                    *initial
                } else {
                    let mut first = None;
                    while index < length {
                        if self
                            .objects
                            .array_has(object_id, index)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                        {
                            first = Some(index);
                            break;
                        }
                        index += 1;
                    }
                    let Some(first) = first else {
                        return Err(RuntimeError::type_error(
                            "reduce of empty array with no initial value",
                            span,
                        ));
                    };
                    index = first + 1;
                    self.objects
                        .array_get(object_id, first)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                };
                let mut roots = roots.to_vec();
                roots.push(accumulator);
                for current in index..length {
                    if !self
                        .objects
                        .array_has(object_id, current)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        continue;
                    }
                    let value = self
                        .objects
                        .array_get(object_id, current)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let callback_args =
                        [accumulator, value, self.heap.number(current as f64), object];
                    accumulator = self.invoke_callable(callback, &callback_args, &roots, span)?;
                    roots.push(accumulator);
                }
                accumulator
            }
            _ => return Ok(None),
        }))
    }
}
