use kagura_object::{ObjectId, PropertyKey, own_property_keys};
use kagura_support::Span;
use kagura_value::Value;

use crate::error::RuntimeError;

use super::Interpreter;
use super::date;
use super::{parse_float_prefix, parse_int_value};

impl<'a> Interpreter<'a> {
    pub(crate) fn load_builtin(&mut self, name: &str, span: Span) -> Result<Value, RuntimeError> {
        if name == "globalThis" {
            return self.global_object(span);
        }
        if let Some(value) = self.builtin_cache.get(name).copied() {
            return Ok(value);
        }
        let value = self.build_builtin(name, span)?;
        self.builtin_cache.insert(name.to_owned(), value);
        Ok(value)
    }

    pub(crate) fn global_object(&mut self, span: Span) -> Result<Value, RuntimeError> {
        if let Some(global) = self.global_object {
            return Ok(Value::object(global.index()));
        }
        let shape = self.object_shape(span)?;
        let object = self
            .objects
            .allocate(shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let value = Value::object(object.index());
        self.global_object = Some(object);
        for name in [
            "Object",
            "Array",
            "String",
            "Number",
            "Boolean",
            "Function",
            "Math",
            "JSON",
            "Date",
            "Error",
            "TypeError",
            "RangeError",
            "ReferenceError",
            "SyntaxError",
            "EvalError",
            "URIError",
            "NaN",
            "Infinity",
            "parseInt",
            "parseFloat",
            "isNaN",
            "isFinite",
        ] {
            let builtin = self.load_builtin(name, span)?;
            self.set_non_enumerable_property(value, name, builtin, span)?;
        }
        self.set_non_enumerable_property(value, "undefined", Value::undefined(), span)?;
        self.set_non_enumerable_property(value, "globalThis", value, span)?;
        Ok(value)
    }

    pub(crate) fn call_global(
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
        if path == "Function" {
            return self.anonymous_function(span).map(Some);
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
        if path == "String.fromCharCode" {
            let mut text = String::new();
            for value in args {
                let unit = self.heap.to_number(*value).unwrap_or(0.0) as u32 & 0xFFFF;
                text.push(char::from_u32(unit).unwrap_or('\u{FFFD}'));
            }
            return Ok(Some(self.heap.string(&text)));
        }
        if path == "String.fromCodePoint" {
            let mut text = String::new();
            for value in args {
                let number = self.heap.to_number(*value).unwrap_or(f64::NAN);
                let Some(ch) = (number.is_finite() && number >= 0.0 && number <= 0x10FFFF as f64)
                    .then_some(number as u32)
                    .and_then(char::from_u32)
                else {
                    return Err(RuntimeError::range("invalid code point", span));
                };
                text.push(ch);
            }
            return Ok(Some(self.heap.string(&text)));
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
        if path == "perform" || path == "performance.now" {
            return Ok(Some(self.heap.number(date::now_millis())));
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
            if let Some(text) = self.heap.as_str(source).map(str::to_owned) {
                for ch in text.chars() {
                    let value = self.heap.string(&ch.to_string());
                    self.objects
                        .array_push(array, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
            } else if let Some(source_id) = source.object_offset().map(ObjectId::new) {
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
        } else if target.is_null() || target.is_undefined() {
            return Err(RuntimeError::type_error(
                "Object.keys requires a coercible value",
                span,
            ));
        } else if self.heap.as_number(target).is_none() && target.as_boolean().is_none() {
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
                    if key_string == "length" || pairs.iter().any(|(key, _)| *key == key_string) {
                        continue;
                    }
                    let value = self.get_property(target, &key_string, span)?;
                    pairs.push((key_string, value));
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
}
