const TAG_MASK: u32 = 0b111;
const TAG_SMI: u32 = 0b000;
const TAG_STRING: u32 = 0b001;
const TAG_NUMBER: u32 = 0b010;
const TAG_OBJECT: u32 = 0b011;
const TAG_SPECIAL: u32 = 0b100;

const SHIFT: u32 = 3;
const SPECIAL_UNDEFINED: u32 = 0;
const SPECIAL_NULL: u32 = 1;
const SPECIAL_TRUE: u32 = 2;
const SPECIAL_FALSE: u32 = 3;

pub const SMI_MIN: i32 = -(1 << 28);
pub const SMI_MAX: i32 = (1 << 28) - 1;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Value(u32);

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(value) = self.as_smi() {
            return write!(f, "smi({value})");
        }
        if self.is_string() {
            return write!(f, "string(ref {})", self.0 >> SHIFT);
        }
        if self.is_heap_number() {
            return write!(f, "number(ref {})", self.0 >> SHIFT);
        }
        if self.is_object() {
            return write!(f, "object(ref {})", self.0 >> SHIFT);
        }
        match self.special() {
            Some(SPECIAL_UNDEFINED) => f.write_str("undefined"),
            Some(SPECIAL_NULL) => f.write_str("null"),
            Some(SPECIAL_TRUE) => f.write_str("true"),
            Some(SPECIAL_FALSE) => f.write_str("false"),
            _ => f.write_str("<invalid>"),
        }
    }
}

impl Value {
    pub const fn raw(self) -> u32 {
        self.0
    }

    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn smi(value: i32) -> Self {
        debug_assert!(value >= SMI_MIN && value <= SMI_MAX);
        Self(((value as u32) << SHIFT) & !TAG_MASK)
    }

    pub const fn string(offset: u32) -> Self {
        Self((offset << SHIFT) | TAG_STRING)
    }

    pub const fn number(offset: u32) -> Self {
        Self((offset << SHIFT) | TAG_NUMBER)
    }

    pub const fn object(offset: u32) -> Self {
        Self((offset << SHIFT) | TAG_OBJECT)
    }

    pub const fn undefined() -> Self {
        Self((SPECIAL_UNDEFINED << SHIFT) | TAG_SPECIAL)
    }

    pub const fn null() -> Self {
        Self((SPECIAL_NULL << SHIFT) | TAG_SPECIAL)
    }

    pub const fn boolean(value: bool) -> Self {
        let special = if value { SPECIAL_TRUE } else { SPECIAL_FALSE };
        Self((special << SHIFT) | TAG_SPECIAL)
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

    pub const fn is_object(self) -> bool {
        self.0 & TAG_MASK == TAG_OBJECT
    }

    pub const fn is_number(self) -> bool {
        self.is_smi() || self.is_heap_number()
    }

    pub const fn is_special(self) -> bool {
        self.0 & TAG_MASK == TAG_SPECIAL
    }

    pub fn as_smi(self) -> Option<i32> {
        self.is_smi().then_some((self.0 as i32) >> SHIFT)
    }

    pub fn heap_offset(self) -> Option<u32> {
        match self.0 & TAG_MASK {
            TAG_STRING | TAG_NUMBER => Some(self.0 >> SHIFT),
            _ => None,
        }
    }

    pub fn object_offset(self) -> Option<u32> {
        self.is_object().then_some(self.0 >> SHIFT)
    }

    pub fn special(self) -> Option<u32> {
        self.is_special().then_some(self.0 >> SHIFT)
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
