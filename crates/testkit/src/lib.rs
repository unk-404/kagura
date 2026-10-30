use std::cell::RefCell;
use std::rc::Rc;

use kagura_bindgen::{Bindings, Host, HostCall, HostError};
use kagura_value::Value;

pub struct CaptureHost {
    bindings: Bindings,
    output: Rc<RefCell<String>>,
}

impl Default for CaptureHost {
    fn default() -> Self {
        let output = Rc::new(RefCell::new(String::new()));
        let mut bindings = Bindings::new();
        bind_console(&mut bindings, "console.log", output.clone());
        bind_console(&mut bindings, "print", output.clone());
        Self { bindings, output }
    }
}

impl CaptureHost {
    pub fn output(&self) -> String {
        self.output.borrow().clone()
    }

    pub fn clear(&self) {
        self.output.borrow_mut().clear();
    }
}

impl Host for CaptureHost {
    fn resolve(&self, path: &str) -> Option<kagura_bindgen::HostFunctionId> {
        self.bindings.resolve(path)
    }

    fn invoke(
        &mut self,
        id: kagura_bindgen::HostFunctionId,
        call: &mut HostCall<'_>,
    ) -> Result<Value, HostError> {
        self.bindings.invoke(id, call)
    }
}

fn bind_console(bindings: &mut Bindings, path: &str, output: Rc<RefCell<String>>) {
    bindings.bind(path, move |call: &mut HostCall<'_>| {
        let text = call
            .args()
            .iter()
            .map(|value| call.heap().to_js_string(*value))
            .collect::<Vec<_>>()
            .join(" ");
        output.borrow_mut().push_str(&text);
        output.borrow_mut().push('\n');
        Ok(Value::undefined())
    });
}
