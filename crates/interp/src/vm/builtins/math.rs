use kagura_support::Span;
use kagura_value::Value;

use crate::error::RuntimeError;

use super::Interpreter;

impl<'a> Interpreter<'a> {
    pub(crate) fn call_math(
        &mut self,
        operation: &str,
        args: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let number = |heap: &kagura_value::Heap, index: usize| {
            heap.to_number(args.get(index).copied().unwrap_or_else(Value::undefined))
                .unwrap_or(f64::NAN)
        };
        let value = match operation {
            "random" => {
                let mut state = self.rng_state;
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                self.rng_state = state;
                (state >> 11) as f64 / (1u64 << 53) as f64
            }
            "abs" => number(self.heap, 0).abs(),
            "floor" => number(self.heap, 0).floor(),
            "ceil" => number(self.heap, 0).ceil(),
            "trunc" => number(self.heap, 0).trunc(),
            "round" => (number(self.heap, 0) + 0.5).floor(),
            "sqrt" => number(self.heap, 0).sqrt(),
            "pow" => number(self.heap, 0).powf(number(self.heap, 1)),
            "acos" => number(self.heap, 0).acos(),
            "asin" => number(self.heap, 0).asin(),
            "atan" => number(self.heap, 0).atan(),
            "atan2" => number(self.heap, 0).atan2(number(self.heap, 1)),
            "cos" => number(self.heap, 0).cos(),
            "sin" => number(self.heap, 0).sin(),
            "tan" => number(self.heap, 0).tan(),
            "exp" => number(self.heap, 0).exp(),
            "log" => number(self.heap, 0).ln(),
            "log2" => number(self.heap, 0).log2(),
            "log10" => number(self.heap, 0).log10(),
            "cbrt" => number(self.heap, 0).cbrt(),
            "fround" => number(self.heap, 0) as f32 as f64,
            "clz32" => (number(self.heap, 0) as u32).leading_zeros() as f64,
            "imul" => {
                let left = number(self.heap, 0) as i32;
                let right = number(self.heap, 1) as i32;
                left.wrapping_mul(right) as f64
            }
            "hypot" => {
                let mut sum = 0.0;
                for index in 0..args.len() {
                    let value = number(self.heap, index);
                    sum += value * value;
                }
                sum.sqrt()
            }
            "min" | "max" => {
                let mut result = if operation == "min" {
                    f64::INFINITY
                } else {
                    f64::NEG_INFINITY
                };
                for index in 0..args.len() {
                    let value = number(self.heap, index);
                    if value.is_nan() {
                        result = f64::NAN;
                        break;
                    }
                    result = if operation == "min" {
                        result.min(value)
                    } else {
                        result.max(value)
                    };
                }
                result
            }
            _ => {
                return Err(RuntimeError::runtime(
                    format!("Math.{operation} is not supported"),
                    span,
                ));
            }
        };
        Ok(self.heap.number(value))
    }
}
