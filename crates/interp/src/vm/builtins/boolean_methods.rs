use kagura_support::Span;
use kagura_value::Value;

use crate::error::RuntimeError;

use super::Interpreter;
use super::method_id::BuiltinMethodId;

impl<'a> Interpreter<'a> {
    pub(crate) fn call_boolean_builtin(
        &mut self,
        boolean: bool,
        method: BuiltinMethodId,
        _args: &[Value],
        _span: Span,
    ) -> Result<Option<Value>, RuntimeError> {
        Ok(Some(match method {
            BuiltinMethodId::ValueOf => Value::boolean(boolean),
            BuiltinMethodId::ToString | BuiltinMethodId::ToLocaleString => {
                self.heap.string(if boolean { "true" } else { "false" })
            }
            _ => return Ok(None),
        }))
    }
}
