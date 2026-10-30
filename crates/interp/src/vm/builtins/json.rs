use kagura_support::Span;
use kagura_value::Value;

use crate::error::RuntimeError;

use super::super::Interpreter;

impl Interpreter<'_> {
    pub(super) fn json_parse_native(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let value: serde_json::Value = serde_json::from_str(text)
            .map_err(|error| RuntimeError::syntax_error(format!("JSON.parse: {error}"), span))?;
        self.json_from_value(value, span)
    }

    fn json_from_value(
        &mut self,
        value: serde_json::Value,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        match value {
            serde_json::Value::Null => Ok(Value::null()),
            serde_json::Value::Bool(value) => Ok(Value::boolean(value)),
            serde_json::Value::Number(number) => {
                let value = number
                    .as_i64()
                    .map(|value| value as f64)
                    .or_else(|| number.as_u64().map(|value| value as f64))
                    .or_else(|| number.as_f64())
                    .unwrap_or(f64::NAN);
                Ok(self.heap.number(value))
            }
            serde_json::Value::String(text) => Ok(self.heap.string(&text)),
            serde_json::Value::Array(values) => {
                let shape = self
                    .array_shape(span)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let array = self
                    .objects
                    .allocate_array(shape)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                for value in values {
                    let value = self.json_from_value(value, span)?;
                    self.objects
                        .array_push(array, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                Ok(Value::object(array.index()))
            }
            serde_json::Value::Object(values) => {
                let shape = self
                    .object_shape(span)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let object = self
                    .objects
                    .allocate(shape)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let object = Value::object(object.index());
                for (key, value) in values {
                    let value = self.json_from_value(value, span)?;
                    self.set_property(object, &key, value, span)?;
                }
                Ok(object)
            }
        }
    }
}
