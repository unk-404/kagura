use kagura_object::{ObjectId, PropertyKey};
use kagura_support::Span;
use kagura_value::Value;

use crate::error::RuntimeError;

use super::Interpreter;
use super::date;
use super::method_id::{BuiltinMethodId, MethodOwner};
use super::{
    format_radix, math_operation, normalize_exponent, parse_float_prefix, parse_int_value,
};

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
        if let Some(boolean) = object.as_boolean() {
            return Ok(Some(match key {
                "valueOf" => object,
                "toString" | "toLocaleString" => {
                    self.heap.string(if boolean { "true" } else { "false" })
                }
                _ => return Ok(None),
            }));
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
