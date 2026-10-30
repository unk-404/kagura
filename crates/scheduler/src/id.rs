//! Opaque scheduler identifiers.

use std::fmt;

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident, $raw:ty) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name($raw);

        impl $name {
            #[must_use]
            pub const fn new(raw: $raw) -> Self {
                Self(raw)
            }

            #[must_use]
            pub const fn get(self) -> $raw {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}#{}", stringify!($name), self.0)
            }
        }
    };
}

define_id! {
    TaskId, u64
}

define_id! {
    TasksId, u64
}

define_id! {
    RuId, u32
}

define_id! {
    /// Identifies one entry in the deterministic job or microtask queue.
    JobId, u64
}

define_id! {
    ContextId, u64
}

define_id! {
    EntryPoint, u64
}

define_id! {
    ContinuationId, u64
}
