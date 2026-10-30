const TAG_MASK: u32 = 0b11;
const TAG_SMI: u32 = 0b00;
const TAG_STRING: u32 = 0b01;
const TAG_NUMBER: u32 = 0b10;
const TAG_SPECIAL: u32 = 0b11;

const SPECIAL_SHIFT: u32 = 2;
const SPECIAL_UNDEFINED: u32 = 0;
const SPECIAL_NULL: u32 = 1;
const SPECIAL_TRUE: u32 = 2;
const SPECIAL_FALSE: u32 = 3;

pub const SMI_MIN: i32 = -(1 << 29);
pub const SMI_MAX: i32 = (1 << 29) - 1;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Value(u32);

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(value) = self.as_smi() {
            return write!(f, "smi({value})");
        }
        if self.is_string() {
            return write!(f, "string(ref {})", self.0 >> 2);
        }
        if self.is_heap_number() {
            return write!(f, "number(ref {})", self.0 >> 2);
        }
        match self.special() {
            Some(0) => f.write_str("undefined"),
            Some(1) => f.write_str("null"),
            Some(2) => f.write_str("true"),
            Some(3) => f.write_str("false"),
            _ => f.write_str("<invalid>"),
        }
    }
}

impl Value {
    pub const fn smi(value: i32) -> Self {
        debug_assert!(value >= SMI_MIN && value <= SMI_MAX);
        Self(((value as u32) << 2) & !TAG_MASK)
    }

    pub const fn string(offset: u32) -> Self {
        Self((offset << 2) | TAG_STRING)
    }

    pub const fn number(offset: u32) -> Self {
        Self((offset << 2) | TAG_NUMBER)
    }

    pub const fn undefined() -> Self {
        Self((SPECIAL_UNDEFINED << SPECIAL_SHIFT) | TAG_SPECIAL)
    }

    pub const fn null() -> Self {
        Self((SPECIAL_NULL << SPECIAL_SHIFT) | TAG_SPECIAL)
    }

    pub const fn boolean(value: bool) -> Self {
        let special = if value { SPECIAL_TRUE } else { SPECIAL_FALSE };
        Self((special << SPECIAL_SHIFT) | TAG_SPECIAL)
    }

    pub const fn is_smi(self) -> bool {
        self.0 & TAG_MASK == TAG_SMI
    }

    pub const fn is_string(self) -> bool {
        self.0 & TAG_MASK == TAG_STRING
    }

    pub const fn is_heap_number(self) -> bool {
        self.0 & TAG_MASK == TAG_NUMBER
    }

    pub const fn is_number(self) -> bool {
        self.is_smi() || self.is_heap_number()
    }

    pub const fn is_special(self) -> bool {
        self.0 & TAG_MASK == TAG_SPECIAL
    }

    pub fn as_smi(self) -> Option<i32> {
        self.is_smi().then_some((self.0 as i32) >> 2)
    }

    pub fn heap_offset(self) -> Option<u32> {
        match self.0 & TAG_MASK {
            TAG_STRING | TAG_NUMBER => Some(self.0 >> 2),
            _ => None,
        }
    }

    pub fn special(self) -> Option<u32> {
        self.is_special().then_some(self.0 >> SPECIAL_SHIFT)
    }

    pub fn is_undefined(self) -> bool {
        self == Self::undefined()
    }

    pub fn is_null(self) -> bool {
        self == Self::null()
    }

    pub fn as_boolean(self) -> Option<bool> {
        match self.special()? {
            SPECIAL_TRUE => Some(true),
            SPECIAL_FALSE => Some(false),
            _ => None,
        }
    }
}
