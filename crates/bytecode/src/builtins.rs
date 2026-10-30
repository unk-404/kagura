/// Stable identifiers for builtins that the compiler can resolve without a
/// runtime string lookup.
///
/// Values are encoded in bytecode and must never be reused. Unknown or
/// user-shadowed paths still use `CallHost`.
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinId {
    JsonParse = 1,
    JsonStringify = 2,
}

impl BuiltinId {
    pub fn from_path(path: &str) -> Option<Self> {
        Some(match path {
            "JSON.parse" => Self::JsonParse,
            "JSON.stringify" => Self::JsonStringify,
            _ => return None,
        })
    }

    pub fn from_u16(value: u16) -> Option<Self> {
        Some(match value {
            1 => Self::JsonParse,
            2 => Self::JsonStringify,
            _ => return None,
        })
    }
}
