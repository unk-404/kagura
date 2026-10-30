use kagura_bindgen::{Bindings, Host, HostCall, HostError, HostFunctionId};
use kagura_value::Value;

pub struct Test262Host {
    bindings: Bindings,
}

impl Default for Test262Host {
    fn default() -> Self {
        let mut bindings = Bindings::new();
        bindings.bind("assert.sameValue", same_value);
        bindings.bind("assert.notSameValue", not_same_value);
        bindings.bind("assert.fail", fail);
        bindings.bind("$DONOTEVALUATE", do_not_evaluate);
        Self { bindings }
    }
}

impl Host for Test262Host {
    fn resolve(&self, path: &str) -> Option<HostFunctionId> {
        self.bindings.resolve(path)
    }

    fn invoke(&mut self, id: HostFunctionId, call: &mut HostCall<'_>) -> Result<Value, HostError> {
        self.bindings.invoke(id, call)
    }
}

fn same_value(call: &mut HostCall<'_>) -> Result<Value, HostError> {
    let actual = call
        .args()
        .first()
        .copied()
        .ok_or_else(|| HostError::new("assert.sameValue requires two arguments"))?;
    let expected = call
        .args()
        .get(1)
        .copied()
        .ok_or_else(|| HostError::new("assert.sameValue requires two arguments"))?;
    if !call.heap().strict_eq(actual, expected) {
        return Err(HostError::new("assert.sameValue failed"));
    }
    Ok(Value::undefined())
}

fn not_same_value(call: &mut HostCall<'_>) -> Result<Value, HostError> {
    let actual = call
        .args()
        .first()
        .copied()
        .ok_or_else(|| HostError::new("assert.notSameValue requires two arguments"))?;
    let expected = call
        .args()
        .get(1)
        .copied()
        .ok_or_else(|| HostError::new("assert.notSameValue requires two arguments"))?;
    if call.heap().strict_eq(actual, expected) {
        return Err(HostError::new("assert.notSameValue failed"));
    }
    Ok(Value::undefined())
}

fn fail(call: &mut HostCall<'_>) -> Result<Value, HostError> {
    let message = call
        .args()
        .first()
        .map(|value| call.heap().to_js_string(*value))
        .unwrap_or_else(|| "assert.fail".into());
    Err(HostError::new(message))
}

fn do_not_evaluate(_call: &mut HostCall<'_>) -> Result<Value, HostError> {
    Err(HostError::new("$DONOTEVALUATE"))
}
