/// Defines the stable builtin identifiers and the compile-time path/id maps in
/// one place. The numeric values are part of the bytecode ABI and must never
/// be reused.
macro_rules! define_builtins {
    ($( $id:literal => $variant:ident, $path:literal ),* $(,)?) => {
        #[repr(u16)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum BuiltinId {
            $( $variant = $id, )*
        }

        static PATHS: phf::Map<&'static str, BuiltinId> = phf::phf_map! {
            $( $path => BuiltinId::$variant, )*
        };

        impl BuiltinId {
            pub fn from_path(path: &str) -> Option<Self> {
                PATHS.get(path).copied()
            }

            pub fn from_u16(value: u16) -> Option<Self> {
                Some(match value {
                    $( $id => Self::$variant, )*
                    _ => return None,
                })
            }
        }
    };
}

define_builtins! {
    1 => JsonParse, "JSON.parse",
    2 => JsonStringify, "JSON.stringify",
    3 => ObjectProtoToStringCall, "Object.prototype.toString.call",
    4 => ObjectProtoHasOwnCall, "Object.prototype.hasOwnProperty.call",
    5 => ParseInt, "parseInt",
    6 => ParseFloat, "parseFloat",
    7 => IsNaN, "isNaN",
    8 => IsFinite, "isFinite",

    20 => MathAbs, "Math.abs",
    21 => MathAcos, "Math.acos",
    22 => MathAsin, "Math.asin",
    23 => MathAtan, "Math.atan",
    24 => MathAtan2, "Math.atan2",
    25 => MathCeil, "Math.ceil",
    26 => MathClz32, "Math.clz32",
    27 => MathCos, "Math.cos",
    28 => MathExp, "Math.exp",
    29 => MathFloor, "Math.floor",
    30 => MathFround, "Math.fround",
    31 => MathHypot, "Math.hypot",
    32 => MathImul, "Math.imul",
    33 => MathLog, "Math.log",
    34 => MathLog10, "Math.log10",
    35 => MathLog2, "Math.log2",
    36 => MathMax, "Math.max",
    37 => MathMin, "Math.min",
    38 => MathPow, "Math.pow",
    39 => MathRound, "Math.round",
    40 => MathSign, "Math.sign",
    41 => MathSin, "Math.sin",
    42 => MathSqrt, "Math.sqrt",
    43 => MathTan, "Math.tan",
    44 => MathTrunc, "Math.trunc",
    45 => MathCbrt, "Math.cbrt",
}
