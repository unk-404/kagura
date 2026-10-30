//! Opaque identifiers used by the scheduler.
//!
//! Identifiers are intentionally small newtypes instead of raw integers: they
//! keep task, package, runner-unit, and job handles from being mixed up at API
//! boundaries, and they make logs/timelines unambiguous.

use std::fmt;

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident, $raw:ty) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name($raw);

        impl $name {
            /// Creates an identifier from its raw representation.
            #[must_use]
            pub const fn new(raw: $raw) -> Self {
                Self(raw)
            }

            /// Returns the raw representation.
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
    /// Identifies a [`crate::Task`], the smallest suspend/resume unit.
    TaskId, u64
}

define_id! {
    /// Identifies a [`crate::Tasks`] package, the scheduling and stealing unit.
    TasksId, u64
}

define_id! {
    /// Identifies a runner unit (RU), usually backed by one OS thread.
    RuId, u32
}

define_id! {
    /// Identifies one entry in the deterministic job or microtask queue.
    JobId, u64
}

define_id! {
    /// Identifies a packager-assigned context (object graph / module / effect domain).
    ContextId, u64
}

define_id! {
    /// Identifies a VM entry point (code object + offset) from which a task starts.
    EntryPoint, u64
}

define_id! {
    /// Identifies a saved VM continuation for a suspended task.
    ContinuationId, u64
}
