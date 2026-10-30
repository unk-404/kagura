use kagura_kmem::Arena;

use crate::display::ValueDisplay;
use crate::value::{SMI_MAX, SMI_MIN, Value};

pub struct Heap {
    arena: Arena,
}

impl Heap {
    pub fn new(capacity: usize) -> Self {
        Self {
            arena: Arena::new(capacity),
        }
    }

    pub fn used(&self) -> usize {
        self.arena.used()
    }

    pub fn capacity(&self) -> usize {
        self.arena.capacity()
    }

    pub fn display(&self, value: Value) -> ValueDisplay<'_> {
        ValueDisplay::new(self, value)
    }

    pub fn number(&mut self, value: f64) -> Value {
        if let Some(smi) = smi_from_f64(value) {
            return Value::smi(smi);
        }
        let offset = self
            .arena
            .alloc_f64(value)
            .expect("value heap exhausted while allocating a number");
        Value::number(offset)
    }

    pub fn string(&mut self, value: &str) -> Value {
        let offset = self
            .arena
            .alloc_str(value)
            .expect("value heap exhausted while allocating a string");
        Value::string(offset)
    }

    pub fn as_number(&self, value: Value) -> Option<f64> {
        if let Some(smi) = value.as_smi() {
            return Some(smi as f64);
        }
        if value.is_heap_number() {
            return self.arena.read_f64(value.heap_offset()?);
        }
        None
    }

    pub fn as_str(&self, value: Value) -> Option<&str> {
        if value.is_string() {
            return self.arena.read_str(value.heap_offset()?);
        }
        None
    }

    pub fn to_js_string(&self, value: Value) -> String {
        if let Some(smi) = value.as_smi() {
            return smi.to_string();
        }
        if let Some(number) = self.as_number(value) {
            return format_number(number);
        }
        if let Some(text) = self.as_str(value) {
            return text.to_owned();
        }
        if value.is_object() {
            return "[object Object]".into();
        }
        match value.special() {
            Some(0) => "undefined".into(),
            Some(1) => "null".into(),
            Some(2) => "true".into(),
            Some(3) => "false".into(),
            _ => "undefined".into(),
        }
    }

    pub fn truthy(&self, value: Value) -> bool {
        if let Some(smi) = value.as_smi() {
            return smi != 0;
        }
        if let Some(number) = self.as_number(value) {
            return number != 0.0 && !number.is_nan();
        }
        if let Some(text) = self.as_str(value) {
            return !text.is_empty();
        }
        if value.is_object() {
            return true;
        }
        value.as_boolean().unwrap_or(false)
    }

    pub fn strict_eq(&self, left: Value, right: Value) -> bool {
        if left.is_number() && right.is_number() {
            return match (self.as_number(left), self.as_number(right)) {
                (Some(left), Some(right)) => !left.is_nan() && !right.is_nan() && left == right,
                _ => false,
            };
        }
        if left.is_string() && right.is_string() {
            return match (self.as_str(left), self.as_str(right)) {
                (Some(left), Some(right)) => left == right,
                _ => false,
            };
        }
        left == right
    }

    pub fn to_number(&self, value: Value) -> Option<f64> {
        if let Some(number) = self.as_number(value) {
            return Some(number);
        }
        if let Some(text) = self.as_str(value) {
            let text = text.trim();
            if text.is_empty() {
                return Some(0.0);
            }
            return text.parse::<f64>().ok();
        }
        if let Some(boolean) = value.as_boolean() {
            return Some(if boolean { 1.0 } else { 0.0 });
        }
        if value.is_null() {
            return Some(0.0);
        }
        Some(f64::NAN)
    }
}

fn smi_from_f64(value: f64) -> Option<i32> {
    if !value.is_finite() || value.fract() != 0.0 {
        return None;
    }
    if value == 0.0 && value.is_sign_negative() {
        return None;
    }
    let value = value as i64;
    if value < SMI_MIN as i64 || value > SMI_MAX as i64 {
        return None;
    }
    Some(value as i32)
}

fn format_number(value: f64) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-Infinity".into()
        } else {
            "Infinity".into()
        };
    }
    if value.fract() == 0.0 && value.abs() < 1e21 {
        return format!("{value:.0}");
    }
    format!("{value}")
}
