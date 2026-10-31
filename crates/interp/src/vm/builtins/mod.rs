use kagura_object::{ObjectId, PropertyKey, own_property_keys};
use kagura_support::Span;
use kagura_value::Value;

use crate::error::RuntimeError;

use super::Interpreter;

mod array_methods;
mod assertions;
mod boolean_methods;
mod calls;
mod constructors;
mod date;
mod dispatch;
mod globals;
mod json;
mod math;
pub(super) mod method_id;
mod number_methods;
mod object_methods;
mod objects;
mod string_methods;

use self::method_id::{BuiltinMethodId, MethodOwner};

impl<'a> Interpreter<'a> {
    pub(crate) fn allocate_static_builtin(
        &mut self,
        path: &str,
        arity: u16,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let object = self
            .objects
            .allocate_function(
                self.shapes.function_shape(),
                u32::MAX,
                Value::undefined(),
                Value::undefined(),
            )
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        self.builtin_statics.insert(object.index(), path.to_owned());
        let value = Value::object(object.index());
        let name = path.rsplit('.').next().unwrap_or(path);
        let name_value = self.heap.string(name);
        self.set_non_enumerable_readonly_property(value, "name", name_value, span)?;
        let length = self.heap.number(arity as f64);
        self.set_non_enumerable_readonly_property(value, "length", length, span)?;
        Ok(value)
    }

    pub(crate) fn allocate_builtin_method(
        &mut self,
        name: &str,
        owner: MethodOwner,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let object = self
            .objects
            .allocate_function(
                self.shapes.function_shape(),
                u32::MAX,
                Value::undefined(),
                Value::undefined(),
            )
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if let Some(id) = BuiltinMethodId::from_name(name) {
            self.builtin_methods.insert(object.index(), (id, owner));
        }
        let value = Value::object(object.index());
        let name_value = self.heap.string(name);
        self.set_non_enumerable_readonly_property(value, "name", name_value, span)?;
        Ok(value)
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
