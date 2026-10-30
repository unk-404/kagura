use std::fmt;

use crate::{Heap, Value};

pub struct ValueDisplay<'a> {
    heap: &'a Heap,
    value: Value,
}

impl<'a> ValueDisplay<'a> {
    pub(crate) fn new(heap: &'a Heap, value: Value) -> Self {
        Self { heap, value }
    }
}

impl fmt::Display for ValueDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(text) = self.heap.as_str(self.value) {
            return write!(f, "\"{}\"", text.escape_debug());
        }
        f.write_str(&self.heap.to_js_string(self.value))
    }
}

impl fmt::Debug for ValueDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
