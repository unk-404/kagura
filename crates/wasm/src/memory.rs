//! Linear memory: a byte-buffer MVP implementation with a 32/64-bit-ready
//! address model.
//!
//! All page counts, byte sizes, and offsets are canonical `u64` values. Host
//! `usize` conversion happens only at allocation and bounds-check boundaries.
//! The current storage commits pages eagerly into a `Vec<u8>`; the
//! [`MemoryReservation`] interface is the replacement point for the future
//! KMem reserve/commit implementation, including 64-bit address spaces.

use std::fmt;

use thiserror::Error;

use crate::value::ValType;

/// Size of a WebAssembly page in bytes.
pub const WASM_PAGE_SIZE: u64 = 65_536;

/// Maximum number of pages in a 32-bit linear memory (4 GiB).
pub const MAX_PAGES_32: u64 = 65_536;

/// Maximum number of pages in a 64-bit linear memory (2^48 pages).
pub const MAX_PAGES_64: u64 = 1 << 48;

/// Default 32-bit per-memory budget: 1 GiB.
pub const DEFAULT_MAX_PAGES_32: u64 = 16_384;

/// Default 64-bit per-memory budget: 4 GiB.
pub const DEFAULT_MAX_PAGES_64: u64 = 65_536;

/// Address width of a linear memory.
///
/// `W32` is the v1 default for wasm32 memories. `W64` represents memory64 and
/// is a first-class future target even though the v1 interpreter executes a
/// 32-bit-default subset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AddressWidth {
    /// 32-bit memory index (`i32` addresses); at most 4 GiB.
    W32,
    /// 64-bit memory index (`i64` addresses); up to 2^48 pages.
    W64,
}

impl AddressWidth {
    /// Returns the index type used by this address width.
    pub const fn index_type(self) -> ValType {
        match self {
            Self::W32 => ValType::I32,
            Self::W64 => ValType::I64,
        }
    }

    /// Returns the address width in bits.
    pub const fn bits(self) -> u32 {
        match self {
            Self::W32 => 32,
            Self::W64 => 64,
        }
    }

    /// Returns the maximum page count addressable by this width.
    pub const fn max_pages(self) -> u64 {
        match self {
            Self::W32 => MAX_PAGES_32,
            Self::W64 => MAX_PAGES_64,
        }
    }

    /// Returns the maximum byte count addressable by this width.
    ///
    /// The 64-bit result is `2^64`, which is exactly one past `u64::MAX`; use
    /// [`MemoryCap`] for a byte budget that must fit in `u64`.
    pub const fn max_bytes(self) -> u128 {
        match self {
            Self::W32 => 1u128 << 32,
            Self::W64 => 1u128 << 64,
        }
    }
}

impl fmt::Display for AddressWidth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::W32 => "wasm32",
            Self::W64 => "wasm64",
        })
    }
}

/// An upper bound on the number of bytes a single linear memory may commit.
///
/// The cap carries an explicit [`AddressWidth`] so a 64-bit memory cannot be
/// constrained by an accidentally narrower 32-bit budget. Unlike the address
/// width's theoretical range, the cap itself is a `u64` byte budget; the
/// 2^64-byte top of the memory64 address space is not representable and will
/// be reported as a size overflow rather than silently truncated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryCap {
    max_bytes: u64,
    address_width: AddressWidth,
}

impl MemoryCap {
    /// Returns the default budget for an address width.
    ///
    /// `W32` gets the 1 GiB 32-bit-first default; `W64` gets the 4 GiB
    /// default and can be raised explicitly.
    pub const fn for_address_width(address_width: AddressWidth) -> Self {
        let max_bytes = match address_width {
            AddressWidth::W32 => DEFAULT_MAX_PAGES_32 * WASM_PAGE_SIZE,
            AddressWidth::W64 => DEFAULT_MAX_PAGES_64 * WASM_PAGE_SIZE,
        };
        Self {
            max_bytes,
            address_width,
        }
    }

    /// Creates a cap from a byte budget in the given address space.
    ///
    /// 32-bit caps are clamped to 4 GiB; 64-bit caps may be 4 GiB or larger.
    pub const fn from_bytes(bytes: u64, address_width: AddressWidth) -> Self {
        let max_bytes = match address_width {
            AddressWidth::W32 if bytes > (1u64 << 32) => 1u64 << 32,
            _ => bytes,
        };
        Self {
            max_bytes,
            address_width,
        }
    }

    /// Creates a cap from a page budget in the given address space.
    ///
    /// For `W64`, a budget that would reach the unreachable 2^64-byte ceiling
    /// saturates at `u64::MAX` bytes.
    pub const fn from_pages(pages: u64, address_width: AddressWidth) -> Self {
        let requested = (pages as u128) * (WASM_PAGE_SIZE as u128);
        let limit = match address_width {
            AddressWidth::W32 => 1u128 << 32,
            AddressWidth::W64 => u64::MAX as u128,
        };
        let max_bytes = if requested > limit {
            limit as u64
        } else {
            requested as u64
        };
        Self {
            max_bytes,
            address_width,
        }
    }

    /// Returns the capped byte count.
    pub const fn bytes(self) -> u64 {
        self.max_bytes
    }

    /// Returns the capped page count (byte budget rounded down).
    pub const fn pages(self) -> u64 {
        self.max_bytes / WASM_PAGE_SIZE
    }

    /// Returns the address width this cap applies to.
    pub const fn address_width(self) -> AddressWidth {
        self.address_width
    }
}

impl Default for MemoryCap {
    fn default() -> Self {
        Self::for_address_width(AddressWidth::W32)
    }
}

/// The limits declared for a linear memory in a module.
///
/// Page counts are canonical `u64` values so wasm32 and memory64 memories use
/// the same type. `u64` page counts can express the full memory64 limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryType {
    /// Initial number of pages.
    pub min_pages: u64,
    /// Optional declared maximum number of pages.
    pub max_pages: Option<u64>,
    /// Address width declared by the module.
    pub address_width: AddressWidth,
}

impl MemoryType {
    /// Creates a 32-bit linear memory type.
    pub const fn new(min_pages: u64, max_pages: Option<u64>) -> Self {
        Self::with_address_width(min_pages, max_pages, AddressWidth::W32)
    }

    /// Creates a linear memory type with an explicit address width.
    pub const fn with_address_width(
        min_pages: u64,
        max_pages: Option<u64>,
        address_width: AddressWidth,
    ) -> Self {
        Self {
            min_pages,
            max_pages,
            address_width,
        }
    }

    /// Returns the initially committed byte size, if it fits in `u64`.
    ///
    /// Only the absolute top of the memory64 address space (`2^48` pages) can
    /// overflow `u64`; admission control rejects it before allocation.
    pub const fn min_bytes(self) -> Option<u64> {
        self.min_pages.checked_mul(WASM_PAGE_SIZE)
    }

    /// Returns the declared maximum byte size, if present and representable.
    pub const fn max_bytes(self) -> Option<u64> {
        match self.max_pages {
            Some(pages) => pages.checked_mul(WASM_PAGE_SIZE),
            None => None,
        }
    }
}

/// Error returned when linear memory cannot be reserved or committed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MemoryError {
    /// The memory's address width is wider than the cap's address width.
    #[error("linear memory address width {memory} exceeds the cap address width {cap}")]
    AddressWidthMismatch {
        /// Address width declared by the memory.
        memory: AddressWidth,
        /// Address width of the configured cap.
        cap: AddressWidth,
    },
    /// The declared page count exceeds the address width's page limit.
    #[error(
        "linear memory size of {pages} pages exceeds the {address_width} limit of {limit} pages"
    )]
    PageLimitExceeded {
        /// Page count that was rejected.
        pages: u64,
        /// Maximum page count for the address width.
        limit: u64,
        /// Address width whose limit was exceeded.
        address_width: AddressWidth,
    },
    /// The initial memory size exceeds the configured per-memory cap.
    #[error("linear memory minimum of {min_pages} pages exceeds the {cap_pages}-page cap")]
    MinimumExceedsCap {
        /// Initial page count declared by the module.
        min_pages: u64,
        /// Page count allowed by the cap.
        cap_pages: u64,
    },
    /// The declared maximum is smaller than the declared minimum.
    #[error("linear memory maximum of {max_pages} pages is below the minimum of {min_pages} pages")]
    MaximumBelowMinimum {
        /// Initial page count declared by the module.
        min_pages: u64,
        /// Declared maximum page count.
        max_pages: u64,
    },
    /// The byte size does not fit in the host address space.
    #[error("linear memory size overflows the host address space")]
    SizeOverflow,
    /// The initial memory allocation failed.
    #[error("linear memory allocation of {bytes} bytes failed")]
    AllocationFailed {
        /// Number of bytes that could not be allocated.
        bytes: u64,
    },
}

/// A draft linear-memory reservation.
///
/// The KMem-backed design reserves virtual address space up front (32-bit or
/// 64-bit) and commits pages as the module grows. This foundation keeps the
/// validated limits, cap, and admission result in the reservation;
/// [`MemoryReservation::commit`] then allocates the initial pages in a byte
/// buffer. Replacing the storage with a KMem `Reservation` is intentionally a
/// local change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryReservation {
    limits: MemoryType,
    cap: MemoryCap,
    max_pages: u64,
    initial_bytes: u64,
}

impl MemoryReservation {
    /// Validates limits against a cap without committing any pages.
    pub fn new(limits: MemoryType, cap: MemoryCap) -> Result<Self, MemoryError> {
        // 32 位预算不得约束 64 位 memory；反向（64 位预算约束 wasm32 memory）允许，
        // 因为同一 target 可以同时承载 wasm32 与 memory64 模块。
        if cap.address_width().bits() < limits.address_width.bits() {
            return Err(MemoryError::AddressWidthMismatch {
                memory: limits.address_width,
                cap: cap.address_width(),
            });
        }
        let width_limit = limits.address_width.max_pages();
        if limits.min_pages > width_limit {
            return Err(MemoryError::PageLimitExceeded {
                pages: limits.min_pages,
                limit: width_limit,
                address_width: limits.address_width,
            });
        }
        if let Some(max_pages) = limits.max_pages {
            if max_pages < limits.min_pages {
                return Err(MemoryError::MaximumBelowMinimum {
                    min_pages: limits.min_pages,
                    max_pages,
                });
            }
            if max_pages > width_limit {
                return Err(MemoryError::PageLimitExceeded {
                    pages: max_pages,
                    limit: width_limit,
                    address_width: limits.address_width,
                });
            }
        }
        let initial_bytes = limits.min_bytes().ok_or(MemoryError::SizeOverflow)?;
        if initial_bytes > cap.bytes() {
            return Err(MemoryError::MinimumExceedsCap {
                min_pages: limits.min_pages,
                cap_pages: cap.pages(),
            });
        }
        let max_pages = limits
            .max_pages
            .unwrap_or(width_limit)
            .min(cap.pages())
            .min(width_limit);
        Ok(Self {
            limits,
            cap,
            max_pages,
            initial_bytes,
        })
    }

    /// Returns the declared limits.
    pub const fn limits(self) -> MemoryType {
        self.limits
    }

    /// Returns the configured cap.
    pub const fn cap(self) -> MemoryCap {
        self.cap
    }

    /// Returns the effective maximum page count after applying the cap.
    pub const fn max_pages(self) -> u64 {
        self.max_pages
    }

    /// Returns the number of bytes the initial commit will allocate.
    pub const fn committed_bytes(self) -> u64 {
        self.initial_bytes
    }

    /// Commits the initial pages and produces a usable linear memory.
    pub fn commit(self) -> Result<Memory, MemoryError> {
        Memory::allocate(self.limits, self.cap, self.max_pages, self.initial_bytes)
    }
}

/// A WebAssembly linear memory backed by a zero-initialized byte buffer.
///
/// The buffer only covers committed pages. `grow` appends zeroed pages up to
/// the effective maximum, which is the minimum of the module maximum, the
/// configured [`MemoryCap`], and the address width's limit.
pub struct Memory {
    bytes: Vec<u8>,
    cap: MemoryCap,
    max_pages: u64,
    address_width: AddressWidth,
}

impl Memory {
    /// Creates a linear memory, committing the declared initial pages.
    pub fn new(limits: MemoryType, cap: MemoryCap) -> Result<Self, MemoryError> {
        MemoryReservation::new(limits, cap)?.commit()
    }

    /// Reserves linear memory limits under a cap without committing pages.
    pub fn reserve(limits: MemoryType, cap: MemoryCap) -> Result<MemoryReservation, MemoryError> {
        MemoryReservation::new(limits, cap)
    }

    // 分配已提交页；调用方必须已经通过 MemoryReservation 的 admission 校验。
    fn allocate(
        limits: MemoryType,
        cap: MemoryCap,
        max_pages: u64,
        initial_bytes: u64,
    ) -> Result<Self, MemoryError> {
        let initial_len = usize::try_from(initial_bytes).map_err(|_| MemoryError::SizeOverflow)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(initial_len)
            .map_err(|_| MemoryError::AllocationFailed {
                bytes: initial_bytes,
            })?;
        bytes.resize(initial_len, 0);
        Ok(Self {
            bytes,
            cap,
            max_pages,
            address_width: limits.address_width,
        })
    }

    /// Returns the configured memory cap.
    pub const fn cap(&self) -> MemoryCap {
        self.cap
    }

    /// Returns the memory's address width.
    pub const fn address_width(&self) -> AddressWidth {
        self.address_width
    }

    /// Returns the effective maximum page count.
    pub const fn max_pages(&self) -> u64 {
        self.max_pages
    }

    /// Returns the currently committed page count.
    pub fn pages(&self) -> u64 {
        self.bytes.len() as u64 / WASM_PAGE_SIZE
    }

    /// Returns the committed bytes as an immutable slice.
    pub fn data(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the committed bytes as a mutable slice.
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }

    /// Returns a committed byte range, or `None` if it is out of bounds.
    ///
    /// `offset` and `len` are canonical `u64` values; the host `usize`
    /// conversion is checked here so wasm64 modules do not silently truncate.
    pub fn read_bytes(&self, offset: u64, len: u64) -> Option<&[u8]> {
        let start = usize::try_from(offset).ok()?;
        let len = usize::try_from(len).ok()?;
        let end = start.checked_add(len)?;
        self.bytes.get(start..end)
    }

    /// Writes bytes into the committed range, returning `false` if out of bounds.
    pub fn write_bytes(&mut self, offset: u64, bytes: &[u8]) -> bool {
        let Ok(start) = usize::try_from(offset) else {
            return false;
        };
        let Some(end) = start.checked_add(bytes.len()) else {
            return false;
        };
        let Some(destination) = self.bytes.get_mut(start..end) else {
            return false;
        };
        destination.copy_from_slice(bytes);
        true
    }

    /// Reads a little-endian `u32`, returning `None` if it is out of bounds.
    pub fn read_u32_le(&self, offset: u64) -> Option<u32> {
        let bytes: [u8; 4] = self.read_bytes(offset, 4)?.try_into().ok()?;
        Some(u32::from_le_bytes(bytes))
    }

    /// Writes a little-endian `u32`, returning `false` if it is out of bounds.
    pub fn write_u32_le(&mut self, offset: u64, value: u32) -> bool {
        self.write_bytes(offset, &value.to_le_bytes())
    }

    /// Reads a little-endian `u64`, returning `None` if it is out of bounds.
    pub fn read_u64_le(&self, offset: u64) -> Option<u64> {
        let bytes: [u8; 8] = self.read_bytes(offset, 8)?.try_into().ok()?;
        Some(u64::from_le_bytes(bytes))
    }

    /// Writes a little-endian `u64`, returning `false` if it is out of bounds.
    pub fn write_u64_le(&mut self, offset: u64, value: u64) -> bool {
        self.write_bytes(offset, &value.to_le_bytes())
    }

    /// Grows the memory by `delta_pages`, returning the previous page count.
    ///
    /// Mirrors `memory.grow`: `None` is the WebAssembly `-1` result when the
    /// new size would exceed the effective maximum or the host address space,
    /// or when allocation fails.
    pub fn grow(&mut self, delta_pages: u64) -> Option<u64> {
        let previous_pages = self.pages();
        if delta_pages == 0 {
            return Some(previous_pages);
        }
        let new_pages = previous_pages.checked_add(delta_pages)?;
        if new_pages > self.max_pages {
            return None;
        }
        let new_bytes = new_pages.checked_mul(WASM_PAGE_SIZE)?;
        let new_len = usize::try_from(new_bytes).ok()?;
        let additional = new_len.checked_sub(self.bytes.len())?;
        if self.bytes.try_reserve_exact(additional).is_err() {
            return None;
        }
        self.bytes.resize(new_len, 0);
        Some(previous_pages)
    }
}

impl fmt::Debug for Memory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Memory")
            .field("pages", &self.pages())
            .field("max_pages", &self.max_pages)
            .field("cap", &self.cap)
            .field("address_width", &self.address_width)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AddressWidth, DEFAULT_MAX_PAGES_32, DEFAULT_MAX_PAGES_64, MAX_PAGES_32, Memory, MemoryCap,
        MemoryError, MemoryReservation, MemoryType, WASM_PAGE_SIZE,
    };

    #[test]
    fn caps_are_address_width_aware_and_64_bit_ready() {
        assert_eq!(
            MemoryCap::for_address_width(AddressWidth::W32).bytes(),
            DEFAULT_MAX_PAGES_32 * WASM_PAGE_SIZE
        );
        assert_eq!(
            MemoryCap::for_address_width(AddressWidth::W64).bytes(),
            DEFAULT_MAX_PAGES_64 * WASM_PAGE_SIZE
        );
        assert_eq!(MemoryCap::default().pages(), DEFAULT_MAX_PAGES_32);

        // 8 GiB 预算必须可表示，不能把 4 GiB 当成上限。
        let cap = MemoryCap::from_bytes(8 * 1024 * 1024 * 1024, AddressWidth::W64);
        assert_eq!(cap.bytes(), 8 * 1024 * 1024 * 1024);
        assert_eq!(cap.pages(), 131_072);
        assert_eq!(cap.address_width(), AddressWidth::W64);

        assert_eq!(
            MemoryCap::from_pages(MAX_PAGES_32 + 1, AddressWidth::W32).pages(),
            MAX_PAGES_32
        );
        assert_eq!(
            MemoryCap::from_bytes(3 * 1024 * 1024, AddressWidth::W32).pages(),
            48
        );
    }

    #[test]
    fn memory64_reservation_accepts_4_gib_and_beyond_without_allocating() {
        // 4 GiB 初始 + 8 GiB 最大：只做 admission，不 commit。
        let limits = MemoryType::with_address_width(65_536, Some(131_072), AddressWidth::W64);
        let cap = MemoryCap::from_bytes(8 * 1024 * 1024 * 1024, AddressWidth::W64);
        let reservation = MemoryReservation::new(limits, cap).expect("64-bit limits accepted");
        assert_eq!(reservation.committed_bytes(), 4 * 1024 * 1024 * 1024);
        assert_eq!(reservation.max_pages(), 131_072);
        assert_eq!(reservation.cap().pages(), 131_072);

        // 32 位 cap 不能约束 64 位 memory。
        assert!(matches!(
            MemoryReservation::new(limits, MemoryCap::default()),
            Err(MemoryError::AddressWidthMismatch {
                memory: AddressWidth::W64,
                cap: AddressWidth::W32
            })
        ));

        // 64 位 cap 可以约束 wasm32 memory（同一 64 位 target 上两种模块共存）。
        assert!(
            MemoryReservation::new(
                MemoryType::new(1, Some(4)),
                MemoryCap::for_address_width(AddressWidth::W64)
            )
            .is_ok()
        );

        // wasm32 的 4 GiB 页上限仍然生效。
        assert!(matches!(
            MemoryReservation::new(
                MemoryType::with_address_width(MAX_PAGES_32 + 1, None, AddressWidth::W32),
                MemoryCap::from_pages(MAX_PAGES_32, AddressWidth::W32)
            ),
            Err(MemoryError::PageLimitExceeded { .. })
        ));
    }

    #[test]
    fn reservation_validates_limits_without_committing() {
        let reservation = Memory::reserve(
            MemoryType::new(1, Some(4)),
            MemoryCap::from_pages(3, AddressWidth::W32),
        )
        .expect("limits fit the cap");
        assert_eq!(reservation.committed_bytes(), WASM_PAGE_SIZE);
        assert_eq!(reservation.cap().pages(), 3);
        assert_eq!(
            Memory::reserve(
                MemoryType::new(8, None),
                MemoryCap::from_pages(4, AddressWidth::W32)
            )
            .unwrap_err(),
            MemoryError::MinimumExceedsCap {
                min_pages: 8,
                cap_pages: 4,
            }
        );
        assert_eq!(
            Memory::reserve(
                MemoryType::new(2, Some(1)),
                MemoryCap::from_pages(4, AddressWidth::W32)
            )
            .unwrap_err(),
            MemoryError::MaximumBelowMinimum {
                min_pages: 2,
                max_pages: 1,
            }
        );
        let memory = MemoryReservation::new(
            MemoryType::new(1, Some(4)),
            MemoryCap::from_pages(3, AddressWidth::W32),
        )
        .unwrap()
        .commit()
        .unwrap();
        assert_eq!(memory.pages(), 1);
        assert_eq!(memory.address_width(), AddressWidth::W32);
    }

    #[test]
    fn grow_respects_declared_maximum_and_cap() {
        let mut memory = Memory::new(
            MemoryType::new(1, Some(3)),
            MemoryCap::from_pages(2, AddressWidth::W32),
        )
        .unwrap();
        assert_eq!(memory.max_pages(), 2);
        assert_eq!(memory.grow(1), Some(1));
        assert_eq!(memory.pages(), 2);
        assert_eq!(memory.grow(1), None);
        assert_eq!(memory.pages(), 2);

        let mut memory = Memory::new(MemoryType::new(1, Some(3)), MemoryCap::default()).unwrap();
        assert_eq!(memory.grow(2), Some(1));
        assert_eq!(memory.grow(1), None);
    }

    #[test]
    fn reads_and_writes_use_u64_offsets_and_are_bounds_checked() {
        let mut memory = Memory::new(
            MemoryType::new(1, None),
            MemoryCap::from_pages(1, AddressWidth::W32),
        )
        .unwrap();
        assert_eq!(memory.pages(), 1);
        assert_eq!(memory.data().len() as u64, WASM_PAGE_SIZE);
        assert!(memory.write_u32_le(0, 0xdead_beef));
        assert_eq!(memory.read_u32_le(0), Some(0xdead_beef));
        assert!(memory.write_u64_le(8, 0x0123_4567_89ab_cdef));
        assert_eq!(memory.read_u64_le(8), Some(0x0123_4567_89ab_cdef));
        assert_eq!(memory.read_u32_le(WASM_PAGE_SIZE - 4), Some(0));
        assert_eq!(memory.read_u32_le(WASM_PAGE_SIZE - 3), None);
        assert!(!memory.write_u32_le(WASM_PAGE_SIZE - 2, 1));
        assert_eq!(memory.read_bytes(WASM_PAGE_SIZE, 1), None);
    }

    #[test]
    fn grow_zero_pages_is_a_no_op() {
        let mut memory = Memory::new(MemoryType::new(1, None), MemoryCap::default()).unwrap();
        assert_eq!(memory.grow(0), Some(1));
        assert_eq!(memory.pages(), 1);
    }
}
