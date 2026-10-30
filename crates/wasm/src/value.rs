//! WebAssembly value types and runtime values.

use std::fmt;

/// A WebAssembly value type supported by the v1 foundation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValType {
    /// 32-bit integer.
    I32,
    /// 64-bit integer.
    I64,
    /// 32-bit IEEE-754 float.
    F32,
    /// 64-bit IEEE-754 float.
    F64,
    /// Nullable function reference.
    FuncRef,
    /// Nullable host reference.
    ExternRef,
}

impl ValType {
    /// Returns `true` for the four numeric value types.
    pub const fn is_numeric(self) -> bool {
        matches!(self, Self::I32 | Self::I64 | Self::F32 | Self::F64)
    }

    /// Returns `true` for reference value types.
    pub const fn is_reference(self) -> bool {
        matches!(self, Self::FuncRef | Self::ExternRef)
    }
}

impl fmt::Display for ValType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::FuncRef => "funcref",
            Self::ExternRef => "externref",
        })
    }
}

/// A WebAssembly runtime value.
///
/// Floating-point variants store raw IEEE-754 bits so NaN payloads survive a
/// round trip through the interpreter unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    /// A 32-bit integer.
    I32(i32),
    /// A 64-bit integer.
    I64(i64),
    /// A 32-bit float stored as raw bits.
    F32(u32),
    /// A 64-bit float stored as raw bits.
    F64(u64),
    /// A nullable function reference; `Some` holds a function index.
    FuncRef(Option<u32>),
    /// A nullable host reference; `Some` holds an opaque host handle.
    ExternRef(Option<u32>),
}

impl Value {
    /// Returns the value type of this value.
    pub const fn ty(self) -> ValType {
        match self {
            Self::I32(_) => ValType::I32,
            Self::I64(_) => ValType::I64,
            Self::F32(_) => ValType::F32,
            Self::F64(_) => ValType::F64,
            Self::FuncRef(_) => ValType::FuncRef,
            Self::ExternRef(_) => ValType::ExternRef,
        }
    }

    /// Creates the zero value for a value type.
    ///
    /// Numeric types map to `0`; reference types map to `null`.
    pub const fn zero(ty: ValType) -> Self {
        match ty {
            ValType::I32 => Self::I32(0),
            ValType::I64 => Self::I64(0),
            ValType::F32 => Self::F32(0),
            ValType::F64 => Self::F64(0),
            ValType::FuncRef => Self::FuncRef(None),
            ValType::ExternRef => Self::ExternRef(None),
        }
    }

    /// Wraps a Rust `f32` as a WebAssembly value.
    pub const fn from_f32(value: f32) -> Self {
        Self::F32(value.to_bits())
    }

    /// Wraps a Rust `f64` as a WebAssembly value.
    pub const fn from_f64(value: f64) -> Self {
        Self::F64(value.to_bits())
    }

    /// Returns the `i32` payload, if this is an `i32` value.
    pub const fn as_i32(self) -> Option<i32> {
        match self {
            Self::I32(value) => Some(value),
            _ => None,
        }
    }

    /// Returns the `i64` payload, if this is an `i64` value.
    pub const fn as_i64(self) -> Option<i64> {
        match self {
            Self::I64(value) => Some(value),
            _ => None,
        }
    }

    /// Returns the `f32` payload, if this is an `f32` value.
    pub const fn as_f32(self) -> Option<f32> {
        match self {
            Self::F32(bits) => Some(f32::from_bits(bits)),
            _ => None,
        }
    }

    /// Returns the `f64` payload, if this is an `f64` value.
    pub const fn as_f64(self) -> Option<f64> {
        match self {
            Self::F64(bits) => Some(f64::from_bits(bits)),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::I32(value) => write!(f, "i32:{value}"),
            Self::I64(value) => write!(f, "i64:{value}"),
            Self::F32(bits) => write!(f, "f32:{:?}", f32::from_bits(*bits)),
            Self::F64(bits) => write!(f, "f64:{:?}", f64::from_bits(*bits)),
            Self::FuncRef(Some(index)) => write!(f, "funcref:{index}"),
            Self::FuncRef(None) => f.write_str("funcref:null"),
            Self::ExternRef(Some(handle)) => write!(f, "externref:{handle}"),
            Self::ExternRef(None) => f.write_str("externref:null"),
        }
    }
}

/// A WebAssembly function signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuncType {
    params: Box<[ValType]>,
    results: Box<[ValType]>,
}

impl FuncType {
    /// Creates a function type from parameter and result lists.
    pub fn new(params: Vec<ValType>, results: Vec<ValType>) -> Self {
        Self {
            params: params.into_boxed_slice(),
            results: results.into_boxed_slice(),
        }
    }

    /// Returns the parameter types.
    pub fn params(&self) -> &[ValType] {
        &self.params
    }

    /// Returns the result types.
    pub fn results(&self) -> &[ValType] {
        &self.results
    }
}

#[cfg(test)]
mod tests {
    use super::{FuncType, ValType, Value};

    #[test]
    fn float_values_round_trip_through_bits() {
        let value = Value::from_f32(f32::from_bits(0x7fc0_0001));
        assert_eq!(value, Value::F32(0x7fc0_0001));
        assert_eq!(value.as_f32().unwrap().to_bits(), 0x7fc0_0001);
        assert_eq!(Value::from_f64(-0.0).as_f64(), Some(-0.0));
    }

    #[test]
    fn zero_values_match_their_type() {
        for ty in [
            ValType::I32,
            ValType::I64,
            ValType::F32,
            ValType::F64,
            ValType::FuncRef,
            ValType::ExternRef,
        ] {
            assert_eq!(Value::zero(ty).ty(), ty);
        }
        assert_eq!(Value::zero(ValType::FuncRef), Value::FuncRef(None));
    }

    #[test]
    fn func_type_exposes_signature() {
        let ty = FuncType::new(vec![ValType::I32, ValType::I64], vec![ValType::F32]);
        assert_eq!(ty.params(), &[ValType::I32, ValType::I64]);
        assert_eq!(ty.results(), &[ValType::F32]);
    }
}
