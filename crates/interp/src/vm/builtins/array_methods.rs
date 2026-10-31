use kagura_object::ObjectId;
use kagura_support::Span;
use kagura_value::Value;

use crate::error::RuntimeError;

use super::Interpreter;
use super::method_id::BuiltinMethodId;
use super::relative_array_index;

impl<'a> Interpreter<'a> {
    pub(crate) fn call_array_builtin(
        &mut self,
        object: Value,
        object_id: ObjectId,
        method: BuiltinMethodId,
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
        Ok(Some(match method {
            BuiltinMethodId::ToString | BuiltinMethodId::ToLocaleString => {
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
                self.heap.string(&parts.join(","))
            }
            BuiltinMethodId::Push => {
                let frozen = self
                    .objects
                    .is_frozen(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let sealed = self
                    .objects
                    .is_sealed(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                if !frozen && !sealed {
                    for value in args {
                        self.objects
                            .array_push(object_id, *value)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                }
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                self.heap.number(length as f64)
            }
            BuiltinMethodId::Pop => {
                if self
                    .objects
                    .is_frozen(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    || self
                        .objects
                        .is_sealed(object_id)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                {
                    Value::undefined()
                } else {
                    self.objects
                        .array_pop(object_id)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                }
            }
            BuiltinMethodId::Shift => {
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                if length == 0 {
                    Value::undefined()
                } else {
                    let first = self
                        .objects
                        .array_get(object_id, 0)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    for index in 1..length {
                        let value = self
                            .objects
                            .array_get(object_id, index)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                        self.objects
                            .array_set(object_id, index - 1, value)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                    self.objects
                        .array_pop(object_id)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    first
                }
            }
            BuiltinMethodId::Unshift => {
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let count = args.len() as u32;
                if count == 0 {
                    self.heap.number(length as f64)
                } else {
                    for _ in 0..count {
                        self.objects
                            .array_push(object_id, Value::hole())
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                    for index in (0..length).rev() {
                        let value = self
                            .objects
                            .array_get(object_id, index)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                        self.objects
                            .array_set(object_id, index + count, value)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                    for (index, value) in args.iter().enumerate() {
                        self.objects
                            .array_set(object_id, index as u32, *value)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                    self.heap.number((length + count) as f64)
                }
            }
            BuiltinMethodId::Splice => {
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let start_value = args.first().and_then(|value| value.as_smi()).unwrap_or(0);
                let start = if start_value < 0 {
                    (length as i64 + start_value as i64).max(0) as u32
                } else {
                    (start_value as u32).min(length)
                };
                let delete_count = if args.len() < 2 {
                    length - start
                } else {
                    args.get(1)
                        .and_then(|value| value.as_smi())
                        .unwrap_or(0)
                        .max(0) as u32
                }
                .min(length - start);
                let result = self
                    .objects
                    .allocate_array(self.shapes.root())
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                for index in 0..delete_count {
                    let value = self
                        .objects
                        .array_get(object_id, start + index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    self.objects
                        .array_push(result, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                let mut tail = Vec::new();
                for index in start + delete_count..length {
                    tail.push(
                        self.objects
                            .array_get(object_id, index)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?,
                    );
                }
                while self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    > start
                {
                    self.objects
                        .array_pop(object_id)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                for value in args.iter().skip(2) {
                    self.objects
                        .array_push(object_id, *value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                for value in tail {
                    self.objects
                        .array_push(object_id, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                Value::object(result.index())
            }
            BuiltinMethodId::Join => {
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
            BuiltinMethodId::IndexOf | BuiltinMethodId::Includes => {
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
                    if !has && matches!(method, BuiltinMethodId::IndexOf) {
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
                if matches!(method, BuiltinMethodId::Includes) {
                    Value::boolean(found.is_some())
                } else {
                    found
                        .map(|index| self.heap.number(index as f64))
                        .unwrap_or_else(|| self.heap.number(-1.0))
                }
            }
            BuiltinMethodId::Slice => {
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
            BuiltinMethodId::Reverse => {
                if self
                    .objects
                    .is_frozen(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                {
                    return Ok(Some(Value::object(object_id.index())));
                }
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let mut low = 0u32;
                let mut high = length;
                while low + 1 < high {
                    high -= 1;
                    let has_low = self
                        .objects
                        .array_has(object_id, low)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let has_high = self
                        .objects
                        .array_has(object_id, high)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let low_value = if has_low {
                        self.objects
                            .array_get(object_id, low)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    } else {
                        Value::hole()
                    };
                    let high_value = if has_high {
                        self.objects
                            .array_get(object_id, high)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    } else {
                        Value::hole()
                    };
                    self.objects
                        .array_set(object_id, low, high_value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    self.objects
                        .array_set(object_id, high, low_value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    low += 1;
                }
                Value::object(object_id.index())
            }
            BuiltinMethodId::Concat => {
                let array = self
                    .objects
                    .allocate_array(self.shapes.root())
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                for index in 0..length {
                    let value = if self
                        .objects
                        .array_has(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        self.objects
                            .array_get(object_id, index)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    } else {
                        Value::hole()
                    };
                    self.objects
                        .array_push(array, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                for argument in args {
                    let source = argument.object_offset().map(ObjectId::new);
                    if let Some(source) = source
                        && self
                            .objects
                            .is_array(source)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        let length = self
                            .objects
                            .array_length(source)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                        for index in 0..length {
                            let value =
                                if self.objects.array_has(source, index).map_err(|error| {
                                    RuntimeError::runtime(error.to_string(), span)
                                })? {
                                    self.objects.array_get(source, index).map_err(|error| {
                                        RuntimeError::runtime(error.to_string(), span)
                                    })?
                                } else {
                                    Value::hole()
                                };
                            self.objects
                                .array_push(array, value)
                                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                        }
                    } else {
                        self.objects
                            .array_push(array, *argument)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                }
                Value::object(array.index())
            }
            BuiltinMethodId::FindLast | BuiltinMethodId::FindLastIndex => {
                let callback = args.first().copied().unwrap_or_else(Value::undefined);
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let roots = roots.to_vec();
                let mut index = length;
                while index > 0 {
                    index -= 1;
                    if !self
                        .objects
                        .array_has(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        continue;
                    }
                    let value = self
                        .objects
                        .array_get(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let callback_args = [value, self.heap.number(index as f64), object];
                    let result = self.invoke_callable(callback, &callback_args, &roots, span)?;
                    if self.heap.truthy(result) {
                        return Ok(Some(if matches!(method, BuiltinMethodId::FindLast) {
                            value
                        } else {
                            self.heap.number(index as f64)
                        }));
                    }
                }
                if matches!(method, BuiltinMethodId::FindLast) {
                    Value::undefined()
                } else {
                    self.heap.number(-1.0)
                }
            }
            BuiltinMethodId::FindIndex => {
                let callback = args.first().copied().unwrap_or_else(Value::undefined);
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let roots = roots.to_vec();
                for index in 0..length {
                    if !self
                        .objects
                        .array_has(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        continue;
                    }
                    let value = self
                        .objects
                        .array_get(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let callback_args = [value, self.heap.number(index as f64), object];
                    let result = self.invoke_callable(callback, &callback_args, &roots, span)?;
                    if self.heap.truthy(result) {
                        return Ok(Some(self.heap.number(index as f64)));
                    }
                }
                self.heap.number(-1.0)
            }
            BuiltinMethodId::Fill => {
                let value = args.first().copied().unwrap_or_else(Value::undefined);
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let start = relative_array_index(self.heap, args.get(1).copied(), length, 0);
                let end = relative_array_index(self.heap, args.get(2).copied(), length, length);
                for index in start..end {
                    self.objects
                        .array_set(object_id, index, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                Value::object(object_id.index())
            }
            BuiltinMethodId::At => {
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let raw = args
                    .first()
                    .and_then(|value| self.heap.as_number(*value))
                    .unwrap_or(0.0) as i64;
                let index = if raw < 0 { length as i64 + raw } else { raw };
                if index < 0 || index >= length as i64 {
                    Value::undefined()
                } else {
                    self.objects
                        .array_get(object_id, index as u32)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                }
            }
            BuiltinMethodId::LastIndexOf => {
                let needle = args.first().copied().unwrap_or_else(Value::undefined);
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let start = args
                    .get(1)
                    .and_then(|value| self.heap.as_number(*value))
                    .map(|value| {
                        if value < 0.0 {
                            (length as f64 + value).max(-1.0) as i64
                        } else {
                            value.min(length as f64 - 1.0) as i64
                        }
                    })
                    .unwrap_or(length as i64 - 1);
                let mut index = start;
                while index >= 0 {
                    let index_u32 = index as u32;
                    if self
                        .objects
                        .array_has(object_id, index_u32)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        let value = self
                            .objects
                            .array_get(object_id, index_u32)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                        if self.heap.strict_eq(value, needle) {
                            return Ok(Some(self.heap.number(index as f64)));
                        }
                    }
                    index -= 1;
                }
                self.heap.number(-1.0)
            }
            BuiltinMethodId::Sort => {
                if self
                    .objects
                    .is_frozen(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                {
                    return Ok(Some(Value::object(object_id.index())));
                }
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let mut values = Vec::new();
                for index in 0..length {
                    if self
                        .objects
                        .array_has(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        values.push(
                            self.objects
                                .array_get(object_id, index)
                                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?,
                        );
                    }
                }
                let comparator = args.first().copied().unwrap_or_else(Value::undefined);
                if comparator.is_undefined() {
                    values.sort_by(|left, right| {
                        self.heap
                            .to_js_string(*left)
                            .cmp(&self.heap.to_js_string(*right))
                    });
                } else {
                    let Some(comparator_id) = comparator.object_offset().map(ObjectId::new) else {
                        return Err(RuntimeError::type_error(
                            "sort comparator must be a function",
                            span,
                        ));
                    };
                    if !self
                        .objects
                        .is_function(comparator_id)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        return Err(RuntimeError::type_error(
                            "sort comparator must be a function",
                            span,
                        ));
                    }
                    let mut roots = roots.to_vec();
                    roots.extend_from_slice(&values);
                    for index in 1..values.len() {
                        let mut current = index;
                        while current > 0 {
                            let order = self
                                .invoke_callable(
                                    comparator,
                                    &[values[current - 1], values[current]],
                                    &roots,
                                    span,
                                )
                                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                            let order = self.heap.as_number(order).unwrap_or(f64::NAN);
                            if order > 0.0 {
                                values.swap(current - 1, current);
                                current -= 1;
                            } else {
                                break;
                            }
                        }
                    }
                }
                for index in 0..length {
                    let value = if (index as usize) < values.len() {
                        values[index as usize]
                    } else {
                        Value::hole()
                    };
                    self.objects
                        .array_set(object_id, index, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                Value::object(object_id.index())
            }
            BuiltinMethodId::Map | BuiltinMethodId::Filter | BuiltinMethodId::ForEach => {
                let callback = args.first().copied().unwrap_or_else(Value::undefined);
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let result = if matches!(method, BuiltinMethodId::ForEach) {
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
                        if matches!(method, BuiltinMethodId::Map)
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
                    match method {
                        BuiltinMethodId::Map => {
                            if let Some(result) = result {
                                self.objects.array_push(result, mapped).map_err(|error| {
                                    RuntimeError::runtime(error.to_string(), span)
                                })?;
                                roots.push(mapped);
                            }
                        }
                        BuiltinMethodId::Filter => {
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
            BuiltinMethodId::Some | BuiltinMethodId::Every | BuiltinMethodId::Find => {
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
                    match method {
                        BuiltinMethodId::Some if truthy => return Ok(Some(Value::boolean(true))),
                        BuiltinMethodId::Every if !truthy => {
                            return Ok(Some(Value::boolean(false)));
                        }
                        BuiltinMethodId::Find if truthy => return Ok(Some(value)),
                        _ => {}
                    }
                }
                if matches!(method, BuiltinMethodId::Some) {
                    Value::boolean(false)
                } else if matches!(method, BuiltinMethodId::Every) {
                    Value::boolean(true)
                } else {
                    Value::undefined()
                }
            }
            BuiltinMethodId::Reduce => {
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
            BuiltinMethodId::ReduceRight => {
                let callback = args.first().copied().unwrap_or_else(Value::undefined);
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let mut index = length as i64 - 1;
                let mut accumulator = if let Some(initial) = args.get(1) {
                    *initial
                } else {
                    let mut first = None;
                    while index >= 0 {
                        if self
                            .objects
                            .array_has(object_id, index as u32)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                        {
                            first = Some(index);
                            break;
                        }
                        index -= 1;
                    }
                    let Some(first) = first else {
                        return Err(RuntimeError::type_error(
                            "reduceRight of empty array with no initial value",
                            span,
                        ));
                    };
                    index = first - 1;
                    self.objects
                        .array_get(object_id, first as u32)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                };
                let mut roots = roots.to_vec();
                roots.push(accumulator);
                while index >= 0 {
                    let current = index as u32;
                    if self
                        .objects
                        .array_has(object_id, current)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        let value = self
                            .objects
                            .array_get(object_id, current)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                        let callback_args =
                            [accumulator, value, self.heap.number(current as f64), object];
                        accumulator =
                            self.invoke_callable(callback, &callback_args, &roots, span)?;
                        roots.push(accumulator);
                    }
                    index -= 1;
                }
                accumulator
            }
            BuiltinMethodId::CopyWithin => {
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let target = relative_array_index(self.heap, args.first().copied(), length, 0);
                let start = relative_array_index(self.heap, args.get(1).copied(), length, 0);
                let end = relative_array_index(self.heap, args.get(2).copied(), length, length);
                let count = end.saturating_sub(start).min(length.saturating_sub(target));
                let mut values = Vec::with_capacity(count as usize);
                for index in 0..count {
                    let source = start + index;
                    let value = if self
                        .objects
                        .array_has(object_id, source)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        self.objects
                            .array_get(object_id, source)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    } else {
                        Value::hole()
                    };
                    values.push(value);
                }
                for (index, value) in values.into_iter().enumerate() {
                    self.objects
                        .array_set(object_id, target + index as u32, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                Value::object(object_id.index())
            }
            BuiltinMethodId::Flat => {
                let depth = args
                    .first()
                    .and_then(|value| self.heap.to_number(*value))
                    .unwrap_or(1.0)
                    .max(0.0) as u32;
                let result = self
                    .objects
                    .allocate_array(self.shapes.root())
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let mut values = Vec::new();
                self.flatten_array(object_id, depth, &mut values, span)?;
                for value in values {
                    self.objects
                        .array_push(result, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                Value::object(result.index())
            }
            BuiltinMethodId::FlatMap => {
                let callback = args.first().copied().unwrap_or_else(Value::undefined);
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let result = self
                    .objects
                    .allocate_array(self.shapes.root())
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let mut callback_roots = roots.to_vec();
                callback_roots.push(Value::object(result.index()));
                for index in 0..length {
                    if !self
                        .objects
                        .array_has(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        continue;
                    }
                    let value = self
                        .objects
                        .array_get(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let callback_args = [value, self.heap.number(index as f64), object];
                    let mapped =
                        self.invoke_callable(callback, &callback_args, &callback_roots, span)?;
                    if let Some(mapped_id) = mapped.object_offset().map(ObjectId::new)
                        && self
                            .objects
                            .is_array(mapped_id)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        let mut flat = Vec::new();
                        self.flatten_array(mapped_id, 1, &mut flat, span)?;
                        for value in flat {
                            self.objects
                                .array_push(result, value)
                                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                        }
                    } else {
                        self.objects
                            .array_push(result, mapped)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                }
                Value::object(result.index())
            }
            _ => return Ok(None),
        }))
    }
}
