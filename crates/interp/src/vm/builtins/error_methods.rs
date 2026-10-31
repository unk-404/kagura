use kagura_support::Span;
use kagura_value::Value;

use crate::error::RuntimeError;

use super::Interpreter;

impl<'a> Interpreter<'a> {
    pub(crate) fn call_error_to_string(
        &mut self,
        object: Value,
        _args: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let name = self.get_property(object, "name", span)?;
        let name = if name.is_undefined() {
            "Error".to_owned()
        } else {
            self.heap.to_js_string(name)
        };
        let message = self.get_property(object, "message", span)?;
        if message.is_undefined() {
            return Ok(self.heap.string(&name));
        }
        let message = self.heap.to_js_string(message);
        if message.is_empty() {
            return Ok(self.heap.string(&name));
        }
        Ok(self.heap.string(&format!("{name}: {message}")))
    }
}
