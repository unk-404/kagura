//! WebAssembly value types and runtime values.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValType {
    I32,
    I64,
    F32,
    F64,
    FuncRef,
    ExternRef,
}

impl ValType {
    pub const fn is_numeric(self) -> bool {
        matches!(self, Self::I32 | Self::I64 | Self::F32 | Self::F64)
    }

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    I32(i32),
    I64(i64),
    F32(u32),
    F64(u64),
    FuncRef(Option<u32>),
    ExternRef(Option<u32>),
}

impl Value {
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

    pub const fn from_f32(value: f32) -> Self {
        Self::F32(value.to_bits())
    }

    pub const fn from_f64(value: f64) -> Self {
        Self::F64(value.to_bits())
    }

    pub const fn as_i32(self) -> Option<i32> {
        match self {
            Self::I32(value) => Some(value),
            _ => None,
        }
    }

    pub const fn as_i64(self) -> Option<i64> {
        match self {
            Self::I64(value) => Some(value),
            _ => None,
        }
    }

    pub const fn as_f32(self) -> Option<f32> {
        match self {
            Self::F32(bits) => Some(f32::from_bits(bits)),
            _ => None,
        }
    }

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuncType {
    params: Box<[ValType]>,
    results: Box<[ValType]>,
}

impl FuncType {
    pub fn new(params: Vec<ValType>, results: Vec<ValType>) -> Self {
        Self {
            params: params.into_boxed_slice(),
            results: results.into_boxed_slice(),
        }
    }

    pub fn params(&self) -> &[ValType] {
        &self.params
    }

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
