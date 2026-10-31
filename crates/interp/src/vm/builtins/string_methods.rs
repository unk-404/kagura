use kagura_support::Span;
use kagura_value::Value;

use crate::error::RuntimeError;

use super::Interpreter;
use super::method_id::BuiltinMethodId;

impl<'a> Interpreter<'a> {
    pub(crate) fn call_string_builtin(
        &mut self,
        text: &str,
        method: BuiltinMethodId,
        args: &[Value],
        span: Span,
    ) -> Result<Option<Value>, RuntimeError> {
        let arg_index = |args: &[Value], index: usize| {
            args.get(index)
                .and_then(|value| value.as_smi())
                .unwrap_or(0)
                .max(0) as usize
        };
        let text = text.to_owned();
        Ok(Some(match method {
            BuiltinMethodId::CharAt => text
                .chars()
                .nth(arg_index(args, 0))
                .map(|ch| self.heap.string(&ch.to_string()))
                .unwrap_or_else(Value::undefined),
            BuiltinMethodId::CharCodeAt => text
                .encode_utf16()
                .nth(arg_index(args, 0))
                .map(|unit| self.heap.number(unit as f64))
                .unwrap_or_else(Value::undefined),
            BuiltinMethodId::IndexOf => {
                let needle = args
                    .first()
                    .map(|value| self.heap.to_js_string(*value))
                    .unwrap_or_default();
                let from = arg_index(args, 1);
                let found = text
                    .char_indices()
                    .enumerate()
                    .find(|(index, (byte, _))| *index >= from && text[*byte..].starts_with(&needle))
                    .map(|(index, _)| index as f64)
                    .unwrap_or(-1.0);
                self.heap.number(found)
            }
            BuiltinMethodId::Includes => {
                let needle = args
                    .first()
                    .map(|value| self.heap.to_js_string(*value))
                    .unwrap_or_default();
                Value::boolean(text.contains(&needle))
            }
            BuiltinMethodId::Slice => {
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
            BuiltinMethodId::ToUpperCase => self.heap.string(&text.to_uppercase()),
            BuiltinMethodId::ToLowerCase => self.heap.string(&text.to_lowercase()),
            BuiltinMethodId::Trim => self.heap.string(text.trim()),
            BuiltinMethodId::TrimStart => self.heap.string(text.trim_start()),
            BuiltinMethodId::TrimEnd => self.heap.string(text.trim_end()),
            BuiltinMethodId::Replace | BuiltinMethodId::ReplaceAll => {
                let search = args
                    .first()
                    .map(|value| self.heap.to_js_string(*value))
                    .unwrap_or_default();
                let replacement = args
                    .get(1)
                    .map(|value| self.heap.to_js_string(*value))
                    .unwrap_or_default();
                let value = if matches!(method, BuiltinMethodId::Replace) {
                    text.replacen(&search, &replacement, 1)
                } else {
                    text.replace(&search, &replacement)
                };
                self.heap.string(&value)
            }
            BuiltinMethodId::StartsWith => {
                let needle = args
                    .first()
                    .map(|value| self.heap.to_js_string(*value))
                    .unwrap_or_default();
                let position = arg_index(args, 1);
                let suffix: String = text.chars().skip(position).collect();
                Value::boolean(suffix.starts_with(&needle))
            }
            BuiltinMethodId::EndsWith => {
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
            BuiltinMethodId::Repeat => {
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
            BuiltinMethodId::Substring => {
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
            BuiltinMethodId::Substr => {
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
            BuiltinMethodId::At => {
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
            BuiltinMethodId::CodePointAt => text
                .chars()
                .nth(arg_index(args, 0))
                .map(|ch| self.heap.number(ch as u32 as f64))
                .unwrap_or_else(Value::undefined),
            BuiltinMethodId::Concat => {
                let mut result = text.clone();
                for value in args {
                    result.push_str(&self.heap.to_js_string(*value));
                }
                self.heap.string(&result)
            }
            BuiltinMethodId::ToString | BuiltinMethodId::ValueOf => self.heap.string(&text),
            BuiltinMethodId::LocaleCompare => {
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
            BuiltinMethodId::PadStart | BuiltinMethodId::PadEnd => {
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
                    self.heap
                        .string(&if matches!(method, BuiltinMethodId::PadStart) {
                            format!("{padding}{text}")
                        } else {
                            format!("{text}{padding}")
                        })
                }
            }
            BuiltinMethodId::Split => {
                let limit = args
                    .get(1)
                    .and_then(|value| value.as_smi())
                    .map(|value| value.max(0) as usize);
                let separator =
                    if args.is_empty() || args.first().is_some_and(|value| value.is_undefined()) {
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
        }))
    }
}
