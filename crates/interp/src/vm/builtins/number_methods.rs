use kagura_support::Span;
use kagura_value::Value;

use crate::error::RuntimeError;

use super::Interpreter;
use super::method_id::BuiltinMethodId;
use super::{format_radix, normalize_exponent};

impl<'a> Interpreter<'a> {
    pub(crate) fn call_number_builtin(
        &mut self,
        number: f64,
        method: BuiltinMethodId,
        args: &[Value],
        _span: Span,
    ) -> Result<Option<Value>, RuntimeError> {
        let value = self.heap.number(number);
        Ok(Some(match method {
            BuiltinMethodId::ValueOf => value,
            BuiltinMethodId::ToString | BuiltinMethodId::ToLocaleString => {
                let radix = args
                    .first()
                    .and_then(|value| self.heap.to_number(*value))
                    .unwrap_or(10.0) as u32;
                if radix == 10 || !(2..=36).contains(&radix) {
                    self.heap.string(&self.heap.to_js_string(value))
                } else {
                    self.heap.string(&format_radix(number, radix))
                }
            }
            BuiltinMethodId::ToFixed => {
                let digits = args
                    .first()
                    .and_then(|value| self.heap.to_number(*value))
                    .unwrap_or(0.0)
                    .clamp(0.0, 100.0) as usize;
                self.heap.string(&format!("{number:.digits$}"))
            }
            BuiltinMethodId::ToExponential => {
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
            BuiltinMethodId::ToPrecision => {
                let Some(precision) = args.first().and_then(|value| self.heap.to_number(*value))
                else {
                    return Ok(Some(self.heap.string(&self.heap.to_js_string(value))));
                };
                let precision = precision.clamp(1.0, 100.0) as usize;
                self.heap.string(&format!("{number:.precision$}"))
            }
            _ => return Ok(None),
        }))
    }
}
