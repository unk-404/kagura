use kagura_object::{
    ObjectId, PropertyAttributes, PropertyKey, PropertyLocation, own_property_keys,
};
use kagura_support::Span;
use kagura_value::Value;

use crate::error::{RuntimeError, RuntimeErrorKind};

use super::Interpreter;

mod date;
mod json;

impl<'a> Interpreter<'a> {
    pub(super) fn call_global(
        &mut self,
        path: &str,
        args: &[Value],
        span: Span,
    ) -> Result<Option<Value>, RuntimeError> {
        if path == "NaN" || path == "Infinity" {
            return Err(RuntimeError::type_error(
                format!("{path} is not callable"),
                span,
            ));
        }
        if path == "Number.isNaN" {
            let value = args.first().copied().unwrap_or_else(Value::undefined);
            return Ok(Some(Value::boolean(
                self.heap.as_number(value).is_some_and(f64::is_nan),
            )));
        }
        if path == "Number.isFinite" {
            let value = args.first().copied().unwrap_or_else(Value::undefined);
            return Ok(Some(Value::boolean(
                self.heap.as_number(value).is_some_and(f64::is_finite),
            )));
        }
        if path == "Number.isInteger" || path == "Number.isSafeInteger" {
            let value = args.first().copied().unwrap_or_else(Value::undefined);
            let number = self.heap.as_number(value);
            let result = number.is_some_and(|number| {
                number.is_finite()
                    && number.fract() == 0.0
                    && (path == "Number.isInteger" || number.abs() <= 9_007_199_254_740_991.0)
            });
            return Ok(Some(Value::boolean(result)));
        }
        if path == "Number.parseInt" {
            let parsed = parse_int_value(self.heap, args);
            return Ok(Some(self.heap.number(parsed)));
        }
        if path == "Number.parseFloat" {
            let text = self
                .heap
                .to_js_string(args.first().copied().unwrap_or_else(Value::undefined));
            return Ok(Some(self.heap.number(parse_float_prefix(&text))));
        }
        if path == "Object" {
            let value = args.first().copied().unwrap_or_else(Value::undefined);
            if value.is_undefined() || value.is_null() {
                return self.new_name("Object", args, span).map(Some);
            }
            if value.is_object() {
                return Ok(Some(value));
            }
            let wrapper = if value.is_string() {
                "String"
            } else if value.is_number() {
                "Number"
            } else {
                "Boolean"
            };
            return self.new_name(wrapper, args, span).map(Some);
        }
        if matches!(
            path,
            "Error"
                | "TypeError"
                | "RangeError"
                | "ReferenceError"
                | "SyntaxError"
                | "EvalError"
                | "URIError"
                | "Test262Error"
                | "Array"
        ) {
            return self.new_name(path, args, span).map(Some);
        }
        if path == "Date.now" {
            return Ok(Some(self.heap.number(date::now_millis())));
        }
        if path == "Date.parse" {
            let text = self
                .heap
                .to_js_string(args.first().copied().unwrap_or_else(Value::undefined));
            return Ok(Some(self.heap.number(date::parse(&text))));
        }
        if path == "Date.UTC" {
            return Ok(Some(self.heap.number(date::utc(args, self.heap))));
        }
        if path == "Date" {
            let text = date::format_iso(date::now_millis());
            return Ok(Some(self.heap.string(&text)));
        }
        if path == "Array.isArray" {
            let array = args.first().copied().is_some_and(|value| {
                value.object_offset().is_some_and(|offset| {
                    self.objects
                        .is_array(ObjectId::new(offset))
                        .unwrap_or(false)
                })
            });
            return Ok(Some(Value::boolean(array)));
        }
        if path == "Object.is" {
            let left = args.first().copied().unwrap_or_else(Value::undefined);
            let right = args.get(1).copied().unwrap_or_else(Value::undefined);
            return Ok(Some(Value::boolean(self.same_value(left, right))));
        }
        if path == "Object.assign" {
            let Some(target) = args.first().copied() else {
                return Err(RuntimeError::type_error(
                    "Object.assign requires a target",
                    span,
                ));
            };
            self.object_id(target, span)?;
            for source in args.iter().skip(1) {
                if source.is_null() || source.is_undefined() {
                    continue;
                }
                self.assign_source(target, *source, span)?;
            }
            return Ok(Some(target));
        }
        if path == "Object.defineProperty" {
            let target = args.first().copied().ok_or_else(|| {
                RuntimeError::type_error("Object.defineProperty requires a target", span)
            })?;
            let key = self
                .heap
                .to_js_string(args.get(1).copied().unwrap_or_else(Value::undefined));
            let descriptor = args.get(2).copied().ok_or_else(|| {
                RuntimeError::type_error("Object.defineProperty requires a descriptor", span)
            })?;
            self.define_property(target, &key, descriptor, span)?;
            return Ok(Some(target));
        }
        if path == "Object.getOwnPropertyDescriptor" {
            let target = args.first().copied().ok_or_else(|| {
                RuntimeError::type_error("Object.getOwnPropertyDescriptor requires a target", span)
            })?;
            let key = self
                .heap
                .to_js_string(args.get(1).copied().unwrap_or_else(Value::undefined));
            return self.own_property_descriptor(target, &key, span).map(Some);
        }
        if path == "Object.freeze" {
            let target = args
                .first()
                .copied()
                .ok_or_else(|| RuntimeError::type_error("Object.freeze requires a target", span))?;
            self.freeze_object(target, span)?;
            return Ok(Some(target));
        }
        if path == "Object.isFrozen" {
            let target = args.first().copied().ok_or_else(|| {
                RuntimeError::type_error("Object.isFrozen requires a target", span)
            })?;
            return self
                .is_frozen(target, span)
                .map(|frozen| Some(Value::boolean(frozen)));
        }
        if path == "Object.seal" {
            let target = args
                .first()
                .copied()
                .ok_or_else(|| RuntimeError::type_error("Object.seal requires a target", span))?;
            self.seal_object(target, span)?;
            return Ok(Some(target));
        }
        if path == "Object.isSealed" {
            let target = args.first().copied().ok_or_else(|| {
                RuntimeError::type_error("Object.isSealed requires a target", span)
            })?;
            return self
                .is_sealed(target, span)
                .map(|sealed| Some(Value::boolean(sealed)));
        }
        if path == "Object.preventExtensions" {
            let target = args.first().copied().ok_or_else(|| {
                RuntimeError::type_error("Object.preventExtensions requires a target", span)
            })?;
            self.prevent_extensions(target, span)?;
            return Ok(Some(target));
        }
        if path == "Object.isExtensible" {
            let target = args.first().copied().ok_or_else(|| {
                RuntimeError::type_error("Object.isExtensible requires a target", span)
            })?;
            return self
                .is_extensible(target, span)
                .map(|extensible| Some(Value::boolean(extensible)));
        }
        if path == "Object.getOwnPropertyDescriptors" {
            let target = args.first().copied().ok_or_else(|| {
                RuntimeError::type_error("Object.getOwnPropertyDescriptors requires a target", span)
            })?;
            return self.get_own_property_descriptors(target, span).map(Some);
        }
        if path == "Object.hasOwn" {
            let target = args
                .first()
                .copied()
                .ok_or_else(|| RuntimeError::type_error("Object.hasOwn requires a target", span))?;
            let key = self
                .heap
                .to_js_string(args.get(1).copied().unwrap_or_else(Value::undefined));
            return self
                .has_own_property(target, &key, span)
                .map(|own| Some(Value::boolean(own)));
        }
        if path == "Object.defineProperties" {
            let target = args.first().copied().ok_or_else(|| {
                RuntimeError::type_error("Object.defineProperties requires a target", span)
            })?;
            let properties = args.get(1).copied().ok_or_else(|| {
                RuntimeError::type_error("Object.defineProperties requires a descriptor map", span)
            })?;
            self.define_properties(target, properties, span)?;
            return Ok(Some(target));
        }
        if path == "Object.getOwnPropertyNames" {
            let target = args.first().copied().ok_or_else(|| {
                RuntimeError::type_error("Object.getOwnPropertyNames requires a target", span)
            })?;
            return self.get_own_property_names(target, span).map(Some);
        }
        if path == "Array.of" {
            let array = self
                .objects
                .allocate_array(self.shapes.root())
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            for value in args {
                self.objects
                    .array_push(array, *value)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            }
            return Ok(Some(Value::object(array.index())));
        }
        if path == "Array.from" {
            let array = self
                .objects
                .allocate_array(self.shapes.root())
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let source = args.first().copied().unwrap_or_else(Value::undefined);
            if let Some(source_id) = source.object_offset().map(ObjectId::new) {
                if self
                    .objects
                    .is_array(source_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                {
                    let length = self
                        .objects
                        .array_length(source_id)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    for index in 0..length {
                        let value = self
                            .objects
                            .array_get(source_id, index)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                        self.objects
                            .array_push(array, value)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                } else {
                    let length = self.get_property(source, "length", span)?;
                    let length = self.heap.as_number(length).unwrap_or(0.0);
                    let length = length.max(0.0).min(u32::MAX as f64) as u32;
                    for index in 0..length {
                        let value = self.get_property(source, &index.to_string(), span)?;
                        self.objects
                            .array_push(array, value)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                }
            }
            return Ok(Some(Value::object(array.index())));
        }
        if path == "String" {
            let value = args.first().copied().unwrap_or_else(Value::undefined);
            let text = if value.is_undefined() {
                String::new()
            } else {
                self.heap.to_js_string(value)
            };
            return Ok(Some(self.heap.string(&text)));
        }
        if path == "Number" {
            let value = args.first().copied().unwrap_or_else(Value::undefined);
            let number = self.heap.to_number(value).unwrap_or(f64::NAN);
            return Ok(Some(self.heap.number(number)));
        }
        if path == "Boolean" {
            let value = args.first().copied().unwrap_or_else(Value::undefined);
            return Ok(Some(Value::boolean(self.heap.truthy(value))));
        }
        if path == "parseInt" {
            let text = self
                .heap
                .to_js_string(args.first().copied().unwrap_or_else(Value::undefined));
            let radix = args
                .get(1)
                .and_then(|value| self.heap.as_number(*value))
                .filter(|radix| *radix >= 2.0 && *radix <= 36.0)
                .unwrap_or(10.0) as u32;
            let text = text.trim_start();
            let (sign, digits) = if let Some(rest) = text.strip_prefix('-') {
                (-1.0, rest)
            } else if let Some(rest) = text.strip_prefix('+') {
                (1.0, rest)
            } else {
                (1.0, text)
            };
            let mut end = 0usize;
            for (index, ch) in digits.char_indices() {
                if ch.to_digit(radix).is_none() {
                    break;
                }
                end = index + ch.len_utf8();
            }
            if end == 0 {
                return Ok(Some(self.heap.number(f64::NAN)));
            }
            let parsed = i64::from_str_radix(&digits[..end], radix)
                .map(|value| value as f64 * sign)
                .unwrap_or(f64::NAN);
            return Ok(Some(self.heap.number(parsed)));
        }
        if path == "parseFloat" {
            let text = self
                .heap
                .to_js_string(args.first().copied().unwrap_or_else(Value::undefined));
            let parsed = parse_float_prefix(&text);
            return Ok(Some(self.heap.number(parsed)));
        }
        if path == "isNaN" {
            let value = args.first().copied().unwrap_or_else(Value::undefined);
            let number = self.heap.to_number(value).unwrap_or(f64::NAN);
            return Ok(Some(Value::boolean(number.is_nan())));
        }
        if path == "isFinite" {
            let value = args.first().copied().unwrap_or_else(Value::undefined);
            let number = self.heap.to_number(value).unwrap_or(f64::NAN);
            return Ok(Some(Value::boolean(number.is_finite())));
        }
        if let Some(operation) = path.strip_prefix("Math.") {
            return self.call_math(operation, args, span).map(Some);
        }
        if path == "JSON.stringify" {
            let value = args.first().copied().unwrap_or_else(Value::undefined);
            return match self.json_stringify(value, span)? {
                Some(text) => Ok(Some(self.heap.string(&text))),
                None => Ok(Some(Value::undefined())),
            };
        }
        if path == "JSON.parse" {
            let text = self
                .heap
                .to_js_string(args.first().copied().unwrap_or_else(Value::undefined));
            return self.json_parse_native(&text, span).map(Some);
        }
        if path == "JSON" {
            return Err(RuntimeError::type_error("JSON is not callable", span));
        }
        if path == "Object.prototype.toString.call" {
            let value = args.first().copied().unwrap_or_else(Value::undefined);
            let tag = self.object_to_string_tag(value, span)?;
            return Ok(Some(self.heap.string(&format!("[object {tag}]"))));
        }
        if path == "Object.prototype.hasOwnProperty.call" {
            let target = args.first().copied().unwrap_or_else(Value::undefined);
            let key = self
                .heap
                .to_js_string(args.get(1).copied().unwrap_or_else(Value::undefined));
            return self
                .has_own_property(target, &key, span)
                .map(|own| Some(Value::boolean(own)));
        }
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
            let object = Value::object(object.index());
            if let Some(properties) = args.get(1).copied()
                && !properties.is_undefined()
            {
                self.define_properties(object, properties, span)?;
            }
            return Ok(Some(object));
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

    pub(super) fn invoke_bound(
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

    pub(super) fn call_with_this(
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

    fn freeze_object(&mut self, target: Value, span: Span) -> Result<(), RuntimeError> {
        let target_id = self.object_id(target, span)?;
        if self
            .objects
            .is_array(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return self
                .objects
                .mark_frozen(target_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span));
        }
        let mut shape = self
            .objects
            .shape(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let keys = own_property_keys(self.shapes, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        for key in keys {
            let attributes = self
                .shapes
                .lookup(shape, &key)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                .map(|descriptor| descriptor.attributes)
                .ok_or_else(|| RuntimeError::runtime("missing property", span))?;
            shape = self
                .shapes
                .redescribe(
                    shape,
                    &key,
                    PropertyAttributes {
                        writable: false,
                        enumerable: attributes.enumerable,
                        configurable: false,
                    },
                )
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            self.objects
                .set_shape(target_id, shape)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        }
        shape = self
            .shapes
            .set_non_extensible(shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        self.objects
            .set_shape(target_id, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        self.objects
            .mark_frozen(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))
    }

    fn is_frozen(&mut self, target: Value, span: Span) -> Result<bool, RuntimeError> {
        let target_id = self.object_id(target, span)?;
        if self
            .objects
            .is_frozen(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Ok(true);
        }
        let shape = self
            .objects
            .shape(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if !self
            .shapes
            .is_non_extensible(shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Ok(false);
        }
        let keys = own_property_keys(self.shapes, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        for key in keys {
            let Some(descriptor) = self
                .shapes
                .lookup(shape, &key)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            else {
                continue;
            };
            if descriptor.attributes.writable || descriptor.attributes.configurable {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn prevent_extensions(&mut self, target: Value, span: Span) -> Result<(), RuntimeError> {
        let target_id = self.object_id(target, span)?;
        let shape = self
            .objects
            .shape(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let shape = self
            .shapes
            .set_non_extensible(shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        self.objects
            .set_shape(target_id, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))
    }

    fn is_extensible(&mut self, target: Value, span: Span) -> Result<bool, RuntimeError> {
        let target_id = self.object_id(target, span)?;
        let shape = self
            .objects
            .shape(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        self.shapes
            .is_non_extensible(shape)
            .map(|non_extensible| !non_extensible)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))
    }

    fn get_own_property_descriptors(
        &mut self,
        target: Value,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let names = self.get_own_property_names(target, span)?;
        let names_id = self.object_id(names, span)?;
        let length = self
            .objects
            .array_length(names_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let result = self
            .objects
            .allocate(self.shapes.root())
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let result = Value::object(result.index());
        for index in 0..length {
            let key = self
                .objects
                .array_get(names_id, index)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let key = self.heap.as_str(key).unwrap_or_default().to_owned();
            let descriptor = self.own_property_descriptor(target, &key, span)?;
            self.set_property(result, &key, descriptor, span)?;
        }
        Ok(result)
    }

    fn seal_object(&mut self, target: Value, span: Span) -> Result<(), RuntimeError> {
        let target_id = self.object_id(target, span)?;
        if self
            .objects
            .is_array(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            self.objects
                .mark_sealed(target_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            return Ok(());
        }
        let mut shape = self
            .objects
            .shape(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let keys = own_property_keys(self.shapes, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        for key in keys {
            let attributes = self
                .shapes
                .lookup(shape, &key)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                .map(|descriptor| descriptor.attributes)
                .ok_or_else(|| RuntimeError::runtime("missing property", span))?;
            shape = self
                .shapes
                .redescribe(
                    shape,
                    &key,
                    PropertyAttributes {
                        writable: attributes.writable,
                        enumerable: attributes.enumerable,
                        configurable: false,
                    },
                )
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            self.objects
                .set_shape(target_id, shape)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        }
        shape = self
            .shapes
            .set_non_extensible(shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        self.objects
            .set_shape(target_id, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        self.objects
            .mark_sealed(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))
    }

    fn is_sealed(&mut self, target: Value, span: Span) -> Result<bool, RuntimeError> {
        let target_id = self.object_id(target, span)?;
        if self
            .objects
            .is_sealed(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Ok(true);
        }
        let shape = self
            .objects
            .shape(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if !self
            .shapes
            .is_non_extensible(shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Ok(false);
        }
        let keys = own_property_keys(self.shapes, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        for key in keys {
            let Some(descriptor) = self
                .shapes
                .lookup(shape, &key)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            else {
                continue;
            };
            if descriptor.attributes.configurable {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn for_in_keys(&mut self, value: Value, span: Span) -> Result<Value, RuntimeError> {
        self.for_in_keys_inner(value, span)
    }

    fn for_in_keys_inner(&mut self, value: Value, span: Span) -> Result<Value, RuntimeError> {
        let array = self
            .objects
            .allocate_array(self.shapes.root())
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let mut seen = kagura_support::FastHashSet::default();
        let mut push = |this: &mut Self, key: String| -> Result<(), RuntimeError> {
            if seen.insert(key.clone()) {
                let value = this.heap.string(&key);
                this.objects
                    .array_push(array, value)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            }
            Ok(())
        };
        if let Some(text) = self.heap.as_str(value).map(str::to_owned) {
            for (index, _) in text.chars().enumerate() {
                push(self, index.to_string())?;
            }
            return Ok(Value::object(array.index()));
        }
        let Some(mut id) = value.object_offset().map(ObjectId::new) else {
            return Ok(Value::object(array.index()));
        };
        let mut visited = kagura_support::FastHashSet::default();
        while visited.insert(id.index()) {
            if self
                .objects
                .is_array(id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            {
                let length = self
                    .objects
                    .array_length(id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                for index in 0..length {
                    if self
                        .objects
                        .array_has(id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        push(self, index.to_string())?;
                    }
                }
            }
            let shape = self
                .objects
                .shape(id)
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
                let key = match key {
                    PropertyKey::String(key) => key,
                    PropertyKey::Index(index) => index.to_string(),
                    PropertyKey::Symbol(_) => continue,
                };
                push(self, key)?;
            }
            let Some(prototype) = self
                .shapes
                .get(shape)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                .prototype
            else {
                break;
            };
            id = prototype;
        }
        Ok(Value::object(array.index()))
    }

    pub(super) fn instance_of(
        &mut self,
        value: Value,
        constructor: Value,
        span: Span,
    ) -> Result<bool, RuntimeError> {
        let Some(value_id) = value.object_offset().map(ObjectId::new) else {
            return Ok(false);
        };
        let constructor_id = self.object_id(constructor, span)?;
        if !self
            .objects
            .is_function(constructor_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Err(RuntimeError::type_error(
                "right-hand side of instanceof is not callable",
                span,
            ));
        }
        let name = self.get_property(constructor, "name", span)?;
        let name = self.heap.as_str(name).unwrap_or_default().to_owned();
        if name == "Object" {
            return Ok(true);
        }
        if name == "Array" {
            return self
                .objects
                .is_array(value_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span));
        }
        if name == "Function" {
            return self
                .objects
                .is_function(value_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span));
        }
        if matches!(
            name.as_str(),
            "Error"
                | "TypeError"
                | "RangeError"
                | "ReferenceError"
                | "SyntaxError"
                | "EvalError"
                | "URIError"
                | "Test262Error"
        ) {
            let actual = self.get_property(value, "name", span)?;
            let actual = self.heap.as_str(actual).unwrap_or_default();
            if name == "Error" {
                return Ok(matches!(
                    actual,
                    "Error"
                        | "TypeError"
                        | "RangeError"
                        | "ReferenceError"
                        | "SyntaxError"
                        | "EvalError"
                        | "URIError"
                ));
            }
            return Ok(actual == name);
        }
        Ok(false)
    }

    fn json_stringify(&mut self, value: Value, span: Span) -> Result<Option<String>, RuntimeError> {
        if value.is_undefined() || value.is_hole() {
            return Ok(None);
        }
        if value.is_null() {
            return Ok(Some("null".into()));
        }
        if let Some(boolean) = value.as_boolean() {
            return Ok(Some(boolean.to_string()));
        }
        if let Some(number) = self.heap.as_number(value) {
            if number.is_nan() || number.is_infinite() {
                return Ok(Some("null".into()));
            }
            return Ok(Some(if number.fract() == 0.0 {
                format!("{number:.0}")
            } else {
                number.to_string()
            }));
        }
        if let Some(text) = self.heap.as_str(value) {
            return Ok(Some(json_quote(text)));
        }
        let Some(id) = value.object_offset().map(ObjectId::new) else {
            return Ok(None);
        };
        if self
            .objects
            .is_function(id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Ok(None);
        }
        if self
            .objects
            .is_array(id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            let length = self
                .objects
                .array_length(id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let mut parts = Vec::with_capacity(length as usize);
            for index in 0..length {
                let item = self
                    .objects
                    .array_get(id, index)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                parts.push(
                    self.json_stringify(item, span)?
                        .unwrap_or_else(|| "null".into()),
                );
            }
            return Ok(Some(format!("[{}]", parts.join(","))));
        }
        let shape = self
            .objects
            .shape(id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let keys = own_property_keys(self.shapes, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let mut parts = Vec::new();
        for key in keys {
            let descriptor = self
                .shapes
                .lookup(shape, &key)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            if !descriptor.is_some_and(|descriptor| descriptor.attributes.enumerable) {
                continue;
            }
            let key_text = match &key {
                PropertyKey::String(key) => key.clone(),
                PropertyKey::Index(index) => index.to_string(),
                PropertyKey::Symbol(_) => continue,
            };
            let item = self.get_property(value, &key_text, span)?;
            if let Some(text) = self.json_stringify(item, span)? {
                parts.push(format!("{}:{text}", json_quote(&key_text)));
            }
        }
        Ok(Some(format!("{{{}}}", parts.join(","))))
    }

    fn call_math(
        &mut self,
        operation: &str,
        args: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let number = |heap: &kagura_value::Heap, index: usize| {
            heap.to_number(args.get(index).copied().unwrap_or_else(Value::undefined))
                .unwrap_or(f64::NAN)
        };
        let value = match operation {
            "abs" => number(self.heap, 0).abs(),
            "floor" => number(self.heap, 0).floor(),
            "ceil" => number(self.heap, 0).ceil(),
            "trunc" => number(self.heap, 0).trunc(),
            "round" => (number(self.heap, 0) + 0.5).floor(),
            "sqrt" => number(self.heap, 0).sqrt(),
            "pow" => number(self.heap, 0).powf(number(self.heap, 1)),
            "acos" => number(self.heap, 0).acos(),
            "asin" => number(self.heap, 0).asin(),
            "atan" => number(self.heap, 0).atan(),
            "atan2" => number(self.heap, 0).atan2(number(self.heap, 1)),
            "cos" => number(self.heap, 0).cos(),
            "sin" => number(self.heap, 0).sin(),
            "tan" => number(self.heap, 0).tan(),
            "exp" => number(self.heap, 0).exp(),
            "log" => number(self.heap, 0).ln(),
            "log2" => number(self.heap, 0).log2(),
            "log10" => number(self.heap, 0).log10(),
            "cbrt" => number(self.heap, 0).cbrt(),
            "fround" => number(self.heap, 0) as f32 as f64,
            "clz32" => (number(self.heap, 0) as u32).leading_zeros() as f64,
            "imul" => {
                let left = number(self.heap, 0) as i32;
                let right = number(self.heap, 1) as i32;
                left.wrapping_mul(right) as f64
            }
            "hypot" => {
                let mut sum = 0.0;
                for index in 0..args.len() {
                    let value = number(self.heap, index);
                    sum += value * value;
                }
                sum.sqrt()
            }
            "min" | "max" => {
                let mut result = if operation == "min" {
                    f64::INFINITY
                } else {
                    f64::NEG_INFINITY
                };
                for index in 0..args.len() {
                    let value = number(self.heap, index);
                    if value.is_nan() {
                        result = f64::NAN;
                        break;
                    }
                    result = if operation == "min" {
                        result.min(value)
                    } else {
                        result.max(value)
                    };
                }
                result
            }
            _ => {
                return Err(RuntimeError::runtime(
                    format!("Math.{operation} is not supported"),
                    span,
                ));
            }
        };
        Ok(self.heap.number(value))
    }

    pub(super) fn construct(
        &mut self,
        callee: Value,
        args: &[Value],
        span: Span,
        roots: &[Value],
    ) -> Result<Value, RuntimeError> {
        let callee_id = self.object_id(callee, span)?;
        if !self
            .objects
            .is_function(callee_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Err(RuntimeError::type_error("value is not a constructor", span));
        }
        let (function_index, _) = self
            .objects
            .function_data(callee_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if function_index == u32::MAX {
            let name = self.get_property(callee, "name", span)?;
            let name = self
                .heap
                .as_str(name)
                .ok_or_else(|| RuntimeError::type_error("invalid constructor", span))?
                .to_owned();
            return self.new_name(&name, args, span);
        }
        let object = self
            .objects
            .allocate(self.shapes.root())
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let object = Value::object(object.index());
        let mut all_roots = roots.to_vec();
        all_roots.push(object);
        match self.call(
            function_index as usize,
            args,
            &all_roots,
            None,
            None,
            Some(object),
        ) {
            Ok(value) if value.object_offset().is_some() => Ok(value),
            Ok(_) => Ok(object),
            Err(error) => Err(error),
        }
    }

    pub(super) fn new_name(
        &mut self,
        name: &str,
        args: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        match name {
            "Error" | "TypeError" | "RangeError" | "ReferenceError" | "SyntaxError"
            | "EvalError" | "URIError" | "Test262Error" => {
                let object = self
                    .objects
                    .allocate(self.shapes.root())
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let object = Value::object(object.index());
                let name_value = self.heap.string(name);
                self.set_property(object, "name", name_value, span)?;
                let message = args
                    .first()
                    .copied()
                    .filter(|value| !value.is_undefined())
                    .map(|value| self.heap.to_js_string(value))
                    .unwrap_or_default();
                let message_value = self.heap.string(&message);
                self.set_property(object, "message", message_value, span)?;
                Ok(object)
            }
            "String" | "Number" | "Boolean" => {
                let primitive = match name {
                    "String" => {
                        if args.is_empty() {
                            self.heap.string("")
                        } else {
                            let text = self.heap.to_js_string(args[0]);
                            self.heap.string(&text)
                        }
                    }
                    "Number" => {
                        let value = if args.is_empty() {
                            0.0
                        } else {
                            self.heap.to_number(args[0]).unwrap_or(f64::NAN)
                        };
                        self.heap.number(value)
                    }
                    _ => {
                        let value = args.first().copied().unwrap_or_else(Value::undefined);
                        Value::boolean(self.heap.truthy(value))
                    }
                };
                let shape = self.object_shape(span)?;
                let object = self
                    .objects
                    .allocate(shape)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let object = Value::object(object.index());
                let wrapper = self.heap.string(&name.to_lowercase());
                self.set_non_enumerable_property(object, "__wrapper", wrapper, span)?;
                self.set_non_enumerable_property(object, "__primitive", primitive, span)?;
                Ok(object)
            }
            "Object" => {
                let shape = self.object_shape(span)?;
                let object = self
                    .objects
                    .allocate(shape)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                Ok(Value::object(object.index()))
            }
            "Array" => {
                let shape = self.array_shape(span)?;
                let array = self
                    .objects
                    .allocate_array(shape)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                if args.len() == 1
                    && let Some(length) = self.heap.as_number(args[0])
                    && length >= 0.0
                    && length.fract() == 0.0
                    && length <= 1024.0
                {
                    for _ in 0..length as u32 {
                        self.objects
                            .array_push(array, Value::hole())
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                } else {
                    for value in args {
                        self.objects
                            .array_push(array, *value)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                }
                Ok(Value::object(array.index()))
            }
            "Date" => self.date_construct(args, span),
            _ => Err(RuntimeError::type_error(
                format!("{name} is not a constructor"),
                span,
            )),
        }
    }

    pub(super) fn assert_compare_array(
        &mut self,
        args: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let actual = args.first().copied().ok_or_else(|| {
            RuntimeError::type_error("assert.compareArray requires two arrays", span)
        })?;
        let expected = args.get(1).copied().ok_or_else(|| {
            RuntimeError::type_error("assert.compareArray requires two arrays", span)
        })?;
        if self.compare_arrays(actual, expected, span)? {
            Ok(Value::undefined())
        } else {
            Err(RuntimeError::runtime("assert.compareArray failed", span))
        }
    }

    pub(super) fn assert_deep_equal(
        &mut self,
        args: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let actual = args.first().copied().ok_or_else(|| {
            RuntimeError::type_error("assert.deepEqual requires two values", span)
        })?;
        let expected = args.get(1).copied().ok_or_else(|| {
            RuntimeError::type_error("assert.deepEqual requires two values", span)
        })?;
        if self.deep_equal(actual, expected, span)? {
            Ok(Value::undefined())
        } else {
            Err(RuntimeError::runtime("assert.deepEqual failed", span))
        }
    }

    fn compare_arrays(
        &mut self,
        actual: Value,
        expected: Value,
        span: Span,
    ) -> Result<bool, RuntimeError> {
        let actual_id = self.object_id(actual, span)?;
        let expected_id = self.object_id(expected, span)?;
        if !self
            .objects
            .is_array(actual_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            || !self
                .objects
                .is_array(expected_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Ok(false);
        }
        let actual_length = self
            .objects
            .array_length(actual_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let expected_length = self
            .objects
            .array_length(expected_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if actual_length != expected_length {
            return Ok(false);
        }
        for index in 0..actual_length {
            let left = self
                .objects
                .array_get(actual_id, index)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let right = self
                .objects
                .array_get(expected_id, index)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            if !self.same_value(left, right) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn deep_equal(
        &mut self,
        actual: Value,
        expected: Value,
        span: Span,
    ) -> Result<bool, RuntimeError> {
        if self.same_value(actual, expected) {
            return Ok(true);
        }
        let Some(actual_id) = actual.object_offset().map(ObjectId::new) else {
            return Ok(false);
        };
        let Some(expected_id) = expected.object_offset().map(ObjectId::new) else {
            return Ok(false);
        };
        let actual_array = self
            .objects
            .is_array(actual_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let expected_array = self
            .objects
            .is_array(expected_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if actual_array || expected_array {
            return self.compare_arrays(actual, expected, span);
        }
        let actual_shape = self
            .objects
            .shape(actual_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let expected_shape = self
            .objects
            .shape(expected_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let actual_keys = own_property_keys(self.shapes, actual_shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let expected_keys = own_property_keys(self.shapes, expected_shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if actual_keys.len() != expected_keys.len() {
            return Ok(false);
        }
        for (actual_key, expected_key) in actual_keys.iter().zip(expected_keys.iter()) {
            if actual_key != expected_key {
                return Ok(false);
            }
            let key = match actual_key {
                PropertyKey::String(key) => key.clone(),
                PropertyKey::Index(index) => index.to_string(),
                PropertyKey::Symbol(_) => return Ok(false),
            };
            let left = self.get_property(actual, &key, span)?;
            let right = self.get_property(expected, &key, span)?;
            if !self.deep_equal(left, right, span)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn call_builtin_id(
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

    pub(super) fn load_builtin(&mut self, name: &str, span: Span) -> Result<Value, RuntimeError> {
        if name == "Math" {
            let object = self
                .objects
                .allocate(self.shapes.root())
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let object = Value::object(object.index());
            for (key, value) in [
                ("PI", std::f64::consts::PI),
                ("E", std::f64::consts::E),
                ("LN2", std::f64::consts::LN_2),
                ("LN10", std::f64::consts::LN_10),
                ("LOG2E", std::f64::consts::LOG2_E),
                ("LOG10E", std::f64::consts::LOG10_E),
                ("SQRT2", std::f64::consts::SQRT_2),
                ("SQRT1_2", std::f64::consts::FRAC_1_SQRT_2),
            ] {
                let value = self.heap.number(value);
                self.set_property(object, key, value, span)?;
            }
            return Ok(object);
        }
        if name == "NaN" {
            return Ok(self.heap.number(f64::NAN));
        }
        if name == "Infinity" {
            return Ok(self.heap.number(f64::INFINITY));
        }
        if name == "Number" {
            let object = self
                .objects
                .allocate_function(
                    self.shapes.function_shape(),
                    u32::MAX,
                    Value::undefined(),
                    Value::undefined(),
                )
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let object = Value::object(object.index());
            let name_value = self.heap.string("Number");
            self.set_property(object, "name", name_value, span)?;
            for (key, value) in [
                ("MAX_SAFE_INTEGER", 9_007_199_254_740_991.0),
                ("MIN_SAFE_INTEGER", -9_007_199_254_740_991.0),
                ("MAX_VALUE", f64::MAX),
                ("MIN_VALUE", f64::MIN_POSITIVE),
                ("EPSILON", f64::EPSILON),
                ("POSITIVE_INFINITY", f64::INFINITY),
                ("NEGATIVE_INFINITY", f64::NEG_INFINITY),
                ("NaN", f64::NAN),
            ] {
                let value = self.heap.number(value);
                self.set_non_enumerable_property(object, key, value, span)?;
            }
            for method in [
                "isNaN",
                "isFinite",
                "isInteger",
                "isSafeInteger",
                "parseInt",
                "parseFloat",
            ] {
                let function = self
                    .objects
                    .allocate_function(
                        self.shapes.function_shape(),
                        u32::MAX,
                        Value::undefined(),
                        Value::undefined(),
                    )
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let function = Value::object(function.index());
                let name_value = self.heap.string(method);
                self.set_property(function, "name", name_value, span)?;
                self.set_non_enumerable_property(object, method, function, span)?;
            }
            return Ok(object);
        }
        if name == "Object" || name == "Array" {
            if name == "Array" && self.object_prototype.is_none() {
                self.load_builtin("Object", span)?;
            }
            let prototype = if name == "Object" {
                self.object_prototype
            } else {
                self.array_prototype
            };
            let prototype = if let Some(prototype) = prototype {
                prototype
            } else {
                let shape = if name == "Array" {
                    if let Some(object_prototype) = self.object_prototype {
                        self.shapes
                            .with_prototype(self.shapes.root(), Some(object_prototype))
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    } else {
                        self.shapes.root()
                    }
                } else {
                    self.shapes.root()
                };
                let prototype = self
                    .objects
                    .allocate(shape)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                if name == "Object" {
                    self.object_prototype = Some(prototype);
                } else {
                    self.array_prototype = Some(prototype);
                }
                prototype
            };
            let prototype_value = Value::object(prototype.index());
            let methods: &[&str] = if name == "Array" {
                &[
                    "at",
                    "concat",
                    "every",
                    "fill",
                    "filter",
                    "find",
                    "findIndex",
                    "forEach",
                    "includes",
                    "indexOf",
                    "join",
                    "lastIndexOf",
                    "map",
                    "pop",
                    "push",
                    "reduce",
                    "reverse",
                    "slice",
                    "some",
                    "sort",
                ]
            } else {
                &[
                    "hasOwnProperty",
                    "isPrototypeOf",
                    "propertyIsEnumerable",
                    "toString",
                    "toLocaleString",
                    "valueOf",
                ]
            };
            for method in methods {
                let function = self
                    .objects
                    .allocate_function(
                        self.shapes.function_shape(),
                        u32::MAX,
                        Value::undefined(),
                        Value::undefined(),
                    )
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let function = Value::object(function.index());
                let name_value = self.heap.string(method);
                self.set_property(function, "name", name_value, span)?;
                self.set_non_enumerable_property(prototype_value, method, function, span)?;
            }
            let object = self
                .objects
                .allocate_function(
                    self.shapes.function_shape(),
                    u32::MAX,
                    Value::undefined(),
                    Value::undefined(),
                )
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let value = Value::object(object.index());
            let name_value = self.heap.string(name);
            self.set_property(value, "name", name_value, span)?;
            self.set_property(value, "prototype", prototype_value, span)?;
            self.set_non_enumerable_property(prototype_value, "constructor", value, span)?;
            return Ok(value);
        }
        if name == "JSON" {
            let object = self
                .objects
                .allocate(self.shapes.root())
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            return Ok(Value::object(object.index()));
        }
        if name == "Date" {
            let object = self
                .objects
                .allocate_function(
                    self.shapes.function_shape(),
                    u32::MAX,
                    Value::undefined(),
                    Value::undefined(),
                )
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let value = Value::object(object.index());
            let name_value = self.heap.string(name);
            self.set_property(value, "name", name_value, span)?;
            let prototype = self
                .objects
                .allocate(self.shapes.root())
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let prototype = Value::object(prototype.index());
            for method in [
                "getTime",
                "valueOf",
                "toISOString",
                "toString",
                "getYear",
                "getUTCFullYear",
                "getFullYear",
                "getUTCMonth",
                "getMonth",
                "getUTCDate",
                "getDate",
                "getUTCDay",
                "getDay",
                "getUTCHours",
                "getHours",
                "getUTCMinutes",
                "getMinutes",
                "getUTCSeconds",
                "getSeconds",
                "getUTCMilliseconds",
                "getMilliseconds",
                "getTimezoneOffset",
                "setTime",
            ] {
                let function = self
                    .objects
                    .allocate_function(
                        self.shapes.function_shape(),
                        u32::MAX,
                        Value::undefined(),
                        Value::undefined(),
                    )
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let function = Value::object(function.index());
                let name_value = self.heap.string(method);
                self.set_property(function, "name", name_value, span)?;
                self.set_non_enumerable_property(prototype, method, function, span)?;
            }
            self.set_property(value, "prototype", prototype, span)?;
            return Ok(value);
        }
        let object = self
            .objects
            .allocate_function(
                self.shapes.function_shape(),
                u32::MAX,
                Value::undefined(),
                Value::undefined(),
            )
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let value = Value::object(object.index());
        let name_value = self.heap.string(name);
        self.set_property(value, "name", name_value, span)?;
        Ok(value)
    }

    pub(super) fn assert_throws(
        &mut self,
        args: &[Value],
        span: Span,
        roots: &[Value],
    ) -> Result<Value, RuntimeError> {
        let expected = args.first().copied().ok_or_else(|| {
            RuntimeError::type_error("assert.throws requires a constructor", span)
        })?;
        let function = args
            .get(1)
            .copied()
            .ok_or_else(|| RuntimeError::type_error("assert.throws requires a function", span))?;
        let expected_name = self.function_name(expected, span)?;
        let function_id = self.object_id(function, span)?;
        if !self
            .objects
            .is_function(function_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Err(RuntimeError::type_error(
                "assert.throws requires a function",
                span,
            ));
        }
        let (function_index, _) = self
            .objects
            .function_data(function_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if function_index == u32::MAX {
            return Err(RuntimeError::type_error(
                "assert.throws cannot invoke a builtin stub",
                span,
            ));
        }
        match self.call(function_index as usize, &[], roots, None, None, None) {
            Ok(_) => Err(RuntimeError::runtime(
                format!("assert.throws expected {expected_name} but nothing was thrown"),
                span,
            )),
            Err(error) => {
                let actual = self.error_name(&error, span)?;
                if actual == expected_name {
                    Ok(Value::undefined())
                } else {
                    Err(RuntimeError::type_error(
                        format!("assert.throws expected {expected_name} but caught {actual}"),
                        span,
                    ))
                }
            }
        }
    }

    fn function_name(&mut self, value: Value, span: Span) -> Result<String, RuntimeError> {
        let name = self.get_property(value, "name", span)?;
        self.heap
            .as_str(name)
            .map(str::to_owned)
            .ok_or_else(|| RuntimeError::type_error("expected a constructor function", span))
    }

    fn error_name(&mut self, error: &RuntimeError, span: Span) -> Result<String, RuntimeError> {
        if let Some(thrown) = error.thrown
            && let Some(id) = thrown.object_offset()
        {
            let id = ObjectId::new(id);
            if self
                .objects
                .is_function(id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            {
                return Ok("function".into());
            }
            let name = self.get_property(thrown, "name", span)?;
            if let Some(name) = self.heap.as_str(name) {
                return Ok(name.to_owned());
            }
        }
        Ok(match error.kind {
            RuntimeErrorKind::Syntax => "SyntaxError",
            RuntimeErrorKind::Type => "TypeError",
            RuntimeErrorKind::Reference => "ReferenceError",
            RuntimeErrorKind::Range => "RangeError",
            RuntimeErrorKind::Runtime => "Error",
        }
        .into())
    }

    fn define_properties(
        &mut self,
        target: Value,
        properties: Value,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let properties_id = self.object_id(properties, span)?;
        let shape = self
            .objects
            .shape(properties_id)
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
            let descriptor = self.get_property(properties, &key_string, span)?;
            self.define_property(target, &key_string, descriptor, span)?;
        }
        Ok(())
    }

    fn get_own_property_names(&mut self, target: Value, span: Span) -> Result<Value, RuntimeError> {
        let target_id = self.object_id(target, span)?;
        let array = self
            .objects
            .allocate_array(self.shapes.root())
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let push = |heap: &mut kagura_value::Heap,
                    objects: &mut kagura_object::ObjectHeap,
                    text: &str|
         -> Result<(), RuntimeError> {
            let value = heap.string(text);
            objects
                .array_push(array, value)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))
        };
        if self
            .objects
            .is_array(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            let length = self
                .objects
                .array_length(target_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            for index in 0..length {
                if self
                    .objects
                    .array_has(target_id, index)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                {
                    push(self.heap, self.objects, &index.to_string())?;
                }
            }
            push(self.heap, self.objects, "length")?;
        }
        let shape = self
            .objects
            .shape(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let keys = own_property_keys(self.shapes, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        for key in keys {
            let key_string = match &key {
                PropertyKey::Index(index) => index.to_string(),
                PropertyKey::String(key) => key.clone(),
                PropertyKey::Symbol(_) => continue,
            };
            push(self.heap, self.objects, &key_string)?;
        }
        Ok(Value::object(array.index()))
    }

    fn set_non_enumerable_property(
        &mut self,
        target: Value,
        key: &str,
        value: Value,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let descriptor = self
            .objects
            .allocate(self.shapes.root())
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let descriptor = Value::object(descriptor.index());
        self.set_property(descriptor, "value", value, span)?;
        self.set_property(descriptor, "writable", Value::boolean(true), span)?;
        self.set_property(descriptor, "enumerable", Value::boolean(false), span)?;
        self.set_property(descriptor, "configurable", Value::boolean(true), span)?;
        self.define_property(target, key, descriptor, span)
    }

    fn define_property(
        &mut self,
        target: Value,
        key: &str,
        descriptor: Value,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let target_id = self.object_id(target, span)?;
        self.object_id(descriptor, span)?;
        let writable = self.get_property(descriptor, "writable", span)?;
        let writable = self.heap.truthy(writable);
        let enumerable = self.get_property(descriptor, "enumerable", span)?;
        let enumerable = self.heap.truthy(enumerable);
        let configurable = self.get_property(descriptor, "configurable", span)?;
        let configurable = self.heap.truthy(configurable);
        let value = self.get_property(descriptor, "value", span)?;
        let shape = self
            .objects
            .shape(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let property = PropertyKey::from_string(key);
        let existing_descriptor = self
            .shapes
            .lookup(shape, &property)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            .map(|descriptor| descriptor.attributes);
        let existing = existing_descriptor.is_some();
        let attributes = PropertyAttributes {
            writable,
            enumerable,
            configurable,
        };
        if let Some(old_attributes) = existing_descriptor {
            let old_value = self.get_property(target, key, span)?;
            if !old_attributes.configurable
                && (old_attributes != attributes || !self.same_value(old_value, value))
            {
                return Err(RuntimeError::type_error(
                    "cannot redefine a non-configurable property",
                    span,
                ));
            }
        }
        let shape = if existing {
            self.shapes
                .redescribe(shape, &property, attributes)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        } else {
            self.shapes
                .add_property(shape, property.clone(), attributes)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        };
        self.objects
            .set_shape(target_id, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let location = self
            .shapes
            .lookup(shape, &property)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            .expect("defined property has descriptor")
            .location;
        match location {
            PropertyLocation::InObject(slot) => self
                .objects
                .set_slot(target_id, slot, value)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span)),
            PropertyLocation::PropertyArray(index) => self
                .objects
                .set_property_array(target_id, index, value)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span)),
        }
    }

    fn own_property_descriptor(
        &mut self,
        target: Value,
        key: &str,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let target_id = self.object_id(target, span)?;
        if self
            .objects
            .is_array(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            if key == "length" {
                let length = self
                    .objects
                    .array_length(target_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let length = self.heap.number(length as f64);
                return self.make_descriptor(length, true, false, false, span);
            }
            if let Ok(index) = key.parse::<u32>()
                && self
                    .objects
                    .array_has(target_id, index)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            {
                let value = self
                    .objects
                    .array_get(target_id, index)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                return self.make_descriptor(value, true, true, true, span);
            }
        }
        let shape = self
            .objects
            .shape(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let property = PropertyKey::from_string(key);
        let Some(descriptor) = self
            .shapes
            .lookup(shape, &property)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        else {
            return Ok(Value::undefined());
        };
        let attributes = descriptor.attributes;
        let value = self.get_property(target, key, span)?;
        self.make_descriptor(
            value,
            attributes.writable,
            attributes.enumerable,
            attributes.configurable,
            span,
        )
    }

    fn make_descriptor(
        &mut self,
        value: Value,
        writable: bool,
        enumerable: bool,
        configurable: bool,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let descriptor = self
            .objects
            .allocate(self.shapes.root())
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let object = Value::object(descriptor.index());
        self.set_property(object, "value", value, span)?;
        self.set_property(object, "writable", Value::boolean(writable), span)?;
        self.set_property(object, "enumerable", Value::boolean(enumerable), span)?;
        self.set_property(object, "configurable", Value::boolean(configurable), span)?;
        Ok(object)
    }

    fn assign_source(
        &mut self,
        target: Value,
        source: Value,
        span: Span,
    ) -> Result<(), RuntimeError> {
        if let Some(text) = self.heap.as_str(source).map(|text| text.to_owned()) {
            for (index, ch) in text.chars().enumerate() {
                let value = self.heap.string(&ch.to_string());
                self.assign_property(target, &index.to_string(), value, span)?;
            }
            return Ok(());
        }
        let Some(source_id) = source.object_offset().map(ObjectId::new) else {
            return Ok(());
        };
        if self
            .objects
            .is_array(source_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            let length = self
                .objects
                .array_length(source_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            for index in 0..length {
                if !self
                    .objects
                    .array_has(source_id, index)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                {
                    continue;
                }
                let value = self
                    .objects
                    .array_get(source_id, index)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                self.assign_property(target, &index.to_string(), value, span)?;
            }
            return Ok(());
        }
        let shape = self
            .objects
            .shape(source_id)
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
            let value = self.get_property(source, &key_string, span)?;
            self.assign_property(target, &key_string, value, span)?;
        }
        Ok(())
    }

    fn object_to_string_tag(
        &mut self,
        value: Value,
        span: Span,
    ) -> Result<&'static str, RuntimeError> {
        if value.is_undefined() || value.is_hole() {
            Ok("Undefined")
        } else if value.is_null() {
            Ok("Null")
        } else if value.as_boolean().is_some() {
            Ok("Boolean")
        } else if value.is_number() {
            Ok("Number")
        } else if value.is_string() {
            Ok("String")
        } else if let Some(offset) = value.object_offset() {
            let id = ObjectId::new(offset);
            if self
                .objects
                .is_array(id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            {
                Ok("Array")
            } else if self
                .objects
                .is_function(id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            {
                Ok("Function")
            } else {
                Ok("Object")
            }
        } else {
            Ok("Object")
        }
    }

    fn has_own_property(
        &mut self,
        target: Value,
        key: &str,
        span: Span,
    ) -> Result<bool, RuntimeError> {
        let object = self.object_id(target, span)?;
        let is_array = self
            .objects
            .is_array(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if is_array && key == "length" {
            return Ok(true);
        }
        if is_array && let Ok(index) = key.parse::<u32>() {
            return self
                .objects
                .array_has(object, index)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span));
        }
        let shape = self
            .objects
            .shape(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        Ok(self
            .shapes
            .lookup(shape, &PropertyKey::from_string(key))
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            .is_some())
    }

    fn assign_property(
        &mut self,
        target: Value,
        key: &str,
        value: Value,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let target_id = self.object_id(target, span)?;
        if self
            .objects
            .is_array(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            && let Ok(index) = key.parse::<u32>()
        {
            return self
                .objects
                .array_set(target_id, index, value)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span));
        }
        self.set_property(target, key, value, span)
    }

    fn same_value(&self, left: Value, right: Value) -> bool {
        if left.is_number() || right.is_number() {
            let (Some(left), Some(right)) = (self.heap.as_number(left), self.heap.as_number(right))
            else {
                return false;
            };
            if left.is_nan() && right.is_nan() {
                return true;
            }
            if left == 0.0 && right == 0.0 {
                return left.is_sign_negative() == right.is_sign_negative();
            }
            return left == right;
        }
        if let (Some(left), Some(right)) = (left.as_boolean(), right.as_boolean()) {
            return left == right;
        }
        self.heap.strict_eq(left, right)
    }

    fn callee_receiver(
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

    fn flatten_array(
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
        if let Some(number) = self.heap.as_number(object) {
            return Ok(Some(match key {
                "valueOf" => object,
                "toString" | "toLocaleString" => {
                    let radix = args
                        .first()
                        .and_then(|value| self.heap.to_number(*value))
                        .unwrap_or(10.0) as u32;
                    if radix == 10 || !(2..=36).contains(&radix) {
                        self.heap.string(&self.heap.to_js_string(object))
                    } else {
                        let text = format_radix(number, radix);
                        self.heap.string(&text)
                    }
                }
                "toFixed" => {
                    let digits = args
                        .first()
                        .and_then(|value| self.heap.to_number(*value))
                        .unwrap_or(0.0)
                        .clamp(0.0, 100.0) as usize;
                    self.heap.string(&format!("{number:.digits$}"))
                }
                "toExponential" => {
                    let digits = args
                        .first()
                        .and_then(|value| self.heap.to_number(*value))
                        .unwrap_or(0.0)
                        .clamp(0.0, 100.0) as usize;
                    let text = if args.is_empty() {
                        format!("{number:e}")
                    } else {
                        format!("{number:.digits$e}")
                    };
                    self.heap.string(&normalize_exponent(&text))
                }
                "toPrecision" => {
                    let Some(precision) =
                        args.first().and_then(|value| self.heap.to_number(*value))
                    else {
                        return Ok(Some(self.heap.string(&self.heap.to_js_string(object))));
                    };
                    let precision = precision.clamp(1.0, 100.0) as usize;
                    self.heap.string(&format!("{number:.precision$}"))
                }
                _ => return Ok(None),
            }));
        }
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
                "trimStart" => self.heap.string(text.trim_start()),
                "trimEnd" => self.heap.string(text.trim_end()),
                "replace" | "replaceAll" => {
                    let search = args
                        .first()
                        .map(|value| self.heap.to_js_string(*value))
                        .unwrap_or_default();
                    let replacement = args
                        .get(1)
                        .map(|value| self.heap.to_js_string(*value))
                        .unwrap_or_default();
                    let value = if key == "replace" {
                        text.replacen(&search, &replacement, 1)
                    } else {
                        text.replace(&search, &replacement)
                    };
                    self.heap.string(&value)
                }
                "startsWith" => {
                    let needle = args
                        .first()
                        .map(|value| self.heap.to_js_string(*value))
                        .unwrap_or_default();
                    let position = arg_index(args, 1);
                    let suffix: String = text.chars().skip(position).collect();
                    Value::boolean(suffix.starts_with(&needle))
                }
                "endsWith" => {
                    let needle = args
                        .first()
                        .map(|value| self.heap.to_js_string(*value))
                        .unwrap_or_default();
                    let length = text.chars().count();
                    let end = args
                        .get(1)
                        .and_then(|value| value.as_smi())
                        .map(|value| value.clamp(0, length as i32) as usize)
                        .unwrap_or(length);
                    let prefix: String = text.chars().take(end).collect();
                    Value::boolean(prefix.ends_with(&needle))
                }
                "repeat" => {
                    let count = args
                        .first()
                        .and_then(|value| value.as_smi())
                        .unwrap_or(0)
                        .max(0) as usize;
                    if text.len().saturating_mul(count) > 1_000_000 {
                        return Err(RuntimeError::range("repeat result is too large", span));
                    }
                    self.heap.string(&text.repeat(count))
                }
                "substring" => {
                    let length = text.chars().count();
                    let start = arg_index(args, 0).min(length);
                    let end = args
                        .get(1)
                        .and_then(|value| value.as_smi())
                        .map(|value| value.clamp(0, length as i32) as usize)
                        .unwrap_or(length);
                    let (start, end) = if start <= end {
                        (start, end)
                    } else {
                        (end, start)
                    };
                    let value: String = text.chars().skip(start).take(end - start).collect();
                    self.heap.string(&value)
                }
                "substr" => {
                    let length = text.chars().count() as i64;
                    let start = args
                        .first()
                        .and_then(|value| self.heap.to_number(*value))
                        .unwrap_or(0.0) as i64;
                    let start = if start < 0 {
                        (length + start).max(0)
                    } else {
                        start.min(length)
                    };
                    let count = args
                        .get(1)
                        .and_then(|value| self.heap.to_number(*value))
                        .map(|value| value.max(0.0) as usize)
                        .unwrap_or(length as usize);
                    let value: String = text.chars().skip(start as usize).take(count).collect();
                    self.heap.string(&value)
                }
                "at" => {
                    let length = text.chars().count() as i64;
                    let mut index = args
                        .first()
                        .and_then(|value| self.heap.to_number(*value))
                        .unwrap_or(0.0) as i64;
                    if index < 0 {
                        index += length;
                    }
                    text.chars()
                        .nth(index.max(0) as usize)
                        .filter(|_| index >= 0 && index < length)
                        .map(|ch| self.heap.string(&ch.to_string()))
                        .unwrap_or_else(Value::undefined)
                }
                "codePointAt" => text
                    .chars()
                    .nth(arg_index(args, 0))
                    .map(|ch| self.heap.number(ch as u32 as f64))
                    .unwrap_or_else(Value::undefined),
                "concat" => {
                    let mut result = text.clone();
                    for value in args {
                        result.push_str(&self.heap.to_js_string(*value));
                    }
                    self.heap.string(&result)
                }
                "toString" | "valueOf" => self.heap.string(&text),
                "localeCompare" => {
                    let other = args
                        .first()
                        .map(|value| self.heap.to_js_string(*value))
                        .unwrap_or_default();
                    self.heap.number(match text.cmp(&other) {
                        std::cmp::Ordering::Less => -1.0,
                        std::cmp::Ordering::Equal => 0.0,
                        std::cmp::Ordering::Greater => 1.0,
                    })
                }
                "padStart" | "padEnd" => {
                    let target = args
                        .first()
                        .and_then(|value| self.heap.to_number(*value))
                        .unwrap_or(0.0)
                        .max(0.0) as usize;
                    let filler = args
                        .get(1)
                        .map(|value| self.heap.to_js_string(*value))
                        .unwrap_or_else(|| " ".into());
                    let current = text.chars().count();
                    if current >= target || filler.is_empty() {
                        self.heap.string(&text)
                    } else {
                        let needed = target - current;
                        let padding: String = filler.chars().cycle().take(needed).collect();
                        self.heap.string(&if key == "padStart" {
                            format!("{padding}{text}")
                        } else {
                            format!("{text}{padding}")
                        })
                    }
                }
                "split" => {
                    let limit = args
                        .get(1)
                        .and_then(|value| value.as_smi())
                        .map(|value| value.max(0) as usize);
                    let separator = if args.is_empty()
                        || args.first().is_some_and(|value| value.is_undefined())
                    {
                        None
                    } else {
                        Some(
                            args.first()
                                .map(|value| self.heap.to_js_string(*value))
                                .unwrap_or_default(),
                        )
                    };
                    let parts: Vec<String> = match separator {
                        None => vec![text.clone()],
                        Some(separator) if separator.is_empty() => {
                            text.chars().map(|ch| ch.to_string()).collect()
                        }
                        Some(separator) => text.split(&separator).map(str::to_owned).collect(),
                    };
                    let array = self
                        .objects
                        .allocate_array(self.shapes.root())
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    for part in parts.into_iter().take(limit.unwrap_or(usize::MAX)) {
                        let value = self.heap.string(&part);
                        self.objects
                            .array_push(array, value)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                    Value::object(array.index())
                }
                _ => return Ok(None),
            }));
        }
        let Ok(object_id) = self.object_id(object, span) else {
            return Ok(None);
        };
        let is_function = self
            .objects
            .is_function(object_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if is_function && key == "bind" {
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
        if is_function && (key == "call" || key == "apply") {
            let receiver = args.first().copied().unwrap_or_else(Value::undefined);
            let rest = if key == "call" {
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
        if key == "hasOwnProperty" {
            let key = self
                .heap
                .to_js_string(args.first().copied().unwrap_or_else(Value::undefined));
            return self
                .has_own_property(object, &key, span)
                .map(|own| Some(Value::boolean(own)));
        }
        if key == "isPrototypeOf" {
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
        if key == "propertyIsEnumerable" {
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
        if key == "valueOf" {
            return Ok(Some(object));
        }
        if key == "toString" || key == "toLocaleString" {
            let tag = self.object_to_string_tag(object, span)?;
            return Ok(Some(self.heap.string(&format!("[object {tag}]"))));
        }
        if !self
            .objects
            .is_array(object_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Ok(None);
        }
        Ok(Some(match key {
            "toString" | "toLocaleString" => {
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
            "push" => {
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
            "pop" => {
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
            "reverse" => {
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
            "concat" => {
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
            "findIndex" => {
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
            "fill" => {
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
            "at" => {
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
            "lastIndexOf" => {
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
            "sort" => {
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
            "reduceRight" => {
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
            "copyWithin" => {
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
            "flat" => {
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
            "flatMap" => {
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

fn relative_array_index(
    heap: &kagura_value::Heap,
    value: Option<Value>,
    length: u32,
    default: u32,
) -> u32 {
    let number = value
        .and_then(|value| heap.as_number(value))
        .unwrap_or(default as f64);
    if number.is_nan() {
        return 0;
    }
    let number = if number < 0.0 {
        length as f64 + number
    } else {
        number
    };
    number.clamp(0.0, length as f64) as u32
}

fn normalize_exponent(text: &str) -> String {
    if let Some(index) = text.find('e') {
        let (mantissa, exponent) = text.split_at(index);
        let exponent = &exponent[1..];
        if let Some(exponent) = exponent.strip_prefix('-') {
            return format!("{mantissa}e-{exponent}");
        }
        return format!("{mantissa}e+{exponent}");
    }
    text.to_owned()
}

fn format_radix(value: f64, radix: u32) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    let negative = value < 0.0;
    let mut integer = value.abs().trunc() as u64;
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut result = Vec::new();
    if integer == 0 {
        result.push(b'0');
    }
    while integer > 0 {
        result.push(digits[(integer % radix as u64) as usize]);
        integer /= radix as u64;
    }
    if negative {
        result.push(b'-');
    }
    result.reverse();
    String::from_utf8(result).unwrap_or_default()
}

fn parse_int_value(heap: &kagura_value::Heap, args: &[Value]) -> f64 {
    let text = heap.to_js_string(args.first().copied().unwrap_or_else(Value::undefined));
    let radix = args
        .get(1)
        .and_then(|value| heap.as_number(*value))
        .filter(|radix| *radix >= 2.0 && *radix <= 36.0)
        .unwrap_or(10.0) as u32;
    let text = text.trim_start();
    let (sign, digits) = if let Some(rest) = text.strip_prefix('-') {
        (-1.0, rest)
    } else if let Some(rest) = text.strip_prefix('+') {
        (1.0, rest)
    } else {
        (1.0, text)
    };
    let mut end = 0usize;
    for (index, ch) in digits.char_indices() {
        if ch.to_digit(radix).is_none() {
            break;
        }
        end = index + ch.len_utf8();
    }
    if end == 0 {
        return f64::NAN;
    }
    i64::from_str_radix(&digits[..end], radix)
        .map(|value| value as f64 * sign)
        .unwrap_or(f64::NAN)
}

fn math_operation(id: kagura_bytecode::builtins::BuiltinId) -> Option<&'static str> {
    use kagura_bytecode::builtins::BuiltinId;
    Some(match id {
        BuiltinId::MathAbs => "abs",
        BuiltinId::MathAcos => "acos",
        BuiltinId::MathAsin => "asin",
        BuiltinId::MathAtan => "atan",
        BuiltinId::MathAtan2 => "atan2",
        BuiltinId::MathCeil => "ceil",
        BuiltinId::MathClz32 => "clz32",
        BuiltinId::MathCos => "cos",
        BuiltinId::MathExp => "exp",
        BuiltinId::MathFloor => "floor",
        BuiltinId::MathFround => "fround",
        BuiltinId::MathHypot => "hypot",
        BuiltinId::MathImul => "imul",
        BuiltinId::MathLog => "log",
        BuiltinId::MathLog10 => "log10",
        BuiltinId::MathLog2 => "log2",
        BuiltinId::MathMax => "max",
        BuiltinId::MathMin => "min",
        BuiltinId::MathPow => "pow",
        BuiltinId::MathRound => "round",
        BuiltinId::MathSign => "sign",
        BuiltinId::MathSin => "sin",
        BuiltinId::MathSqrt => "sqrt",
        BuiltinId::MathTan => "tan",
        BuiltinId::MathTrunc => "trunc",
        BuiltinId::MathCbrt => "cbrt",
        _ => return None,
    })
}

fn parse_float_prefix(value: &str) -> f64 {
    let value = value.trim_start();
    if value.starts_with("Infinity") {
        return f64::INFINITY;
    }
    if value.starts_with("-Infinity") {
        return f64::NEG_INFINITY;
    }
    let bytes = value.as_bytes();
    let mut index = usize::from(matches!(bytes.first(), Some(b'+') | Some(b'-')));
    let mut digits = 0usize;
    let mut dot = false;
    let mut exponent = false;
    let mut end = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_digit() {
            digits += 1;
            end = index + 1;
            index += 1;
        } else if byte == b'.' && !dot && !exponent {
            dot = true;
            index += 1;
        } else if matches!(byte, b'e' | b'E') && digits > 0 && !exponent {
            exponent = true;
            end = index + 1;
            index += 1;
            if matches!(bytes.get(index), Some(b'+') | Some(b'-')) {
                index += 1;
            }
        } else {
            break;
        }
    }
    if digits == 0 {
        return f64::NAN;
    }
    value[..end].parse::<f64>().unwrap_or(f64::NAN)
}

fn json_quote(value: &str) -> String {
    let mut result = String::with_capacity(value.len() + 2);
    result.push('"');
    for ch in value.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            ch if (ch as u32) < 0x20 => {
                result.push_str(&format!("\\u{:04x}", ch as u32));
            }
            ch => result.push(ch),
        }
    }
    result.push('"');
    result
}
