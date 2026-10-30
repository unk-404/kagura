#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PropertyAttributes {
    pub writable: bool,
    pub enumerable: bool,
    pub configurable: bool,
}

impl PropertyAttributes {
    pub const fn data() -> Self {
        Self {
            writable: true,
            enumerable: true,
            configurable: true,
        }
    }

    pub const fn readonly() -> Self {
        Self {
            writable: false,
            enumerable: true,
            configurable: false,
        }
    }
}
