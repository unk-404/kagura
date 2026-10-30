use std::cell::RefCell;
use std::rc::Rc;

use kagura_bindgen::{Bindings, Host, HostCall, HostError, HostFunctionId};
use kagura_value::Value;

pub struct Test262Host {
    bindings: Bindings,
    output: Rc<RefCell<String>>,
}

impl Default for Test262Host {
    fn default() -> Self {
        let mut bindings = Bindings::new();
        let output = Rc::new(RefCell::new(String::new()));
        bindings.bind("assert.sameValue", same_value);
        bindings.bind("assert.notSameValue", not_same_value);
        bindings.bind("assert.fail", fail);
        bindings.bind("$DONOTEVALUATE", do_not_evaluate);
        let console_output = output.clone();
        bindings.bind("console.log", move |call| {
            record_output(call, &console_output)
        });
        let print_output = output.clone();
        bindings.bind("print", move |call| record_output(call, &print_output));
        Self { bindings, output }
    }
}

impl Test262Host {
    pub fn output(&self) -> String {
        self.output.borrow().clone()
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
    if !same_value_impl(call.heap(), actual, expected) {
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
    if same_value_impl(call.heap(), actual, expected) {
        return Err(HostError::new("assert.notSameValue failed"));
    }
    Ok(Value::undefined())
}

fn same_value_impl(heap: &kagura_value::Heap, left: Value, right: Value) -> bool {
    if left.is_number() || right.is_number() {
        let (Some(left), Some(right)) = (heap.as_number(left), heap.as_number(right)) else {
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
    heap.strict_eq(left, right)
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

fn record_output(call: &mut HostCall<'_>, output: &RefCell<String>) -> Result<Value, HostError> {
    let text = call
        .args()
        .iter()
        .map(|value| call.heap().to_js_string(*value))
        .collect::<Vec<_>>()
        .join(" ");
    let mut output = output.borrow_mut();
    output.push_str(&text);
    output.push('\n');
    Ok(Value::undefined())
}
