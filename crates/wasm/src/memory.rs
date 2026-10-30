//! Linear memory: `u64` canonical sizes/offsets, explicit 32/64-bit address
//! width, eager `Vec<u8>` storage until KMem reserve/commit replaces it.

use std::fmt;

use thiserror::Error;

use crate::value::ValType;

pub const WASM_PAGE_SIZE: u64 = 65_536;

pub const MAX_PAGES_32: u64 = 65_536;

pub const MAX_PAGES_64: u64 = 1 << 48;

pub const DEFAULT_MAX_PAGES_32: u64 = 16_384;

pub const DEFAULT_MAX_PAGES_64: u64 = 65_536;

/// Address width of a linear memory. `W32` is the v1 default; `W64` is
/// memory64, a first-class future target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AddressWidth {
    W32,
    W64,
}

impl AddressWidth {
    pub const fn index_type(self) -> ValType {
        match self {
            Self::W32 => ValType::I32,
            Self::W64 => ValType::I64,
        }
    }

    pub const fn bits(self) -> u32 {
        match self {
            Self::W32 => 32,
            Self::W64 => 64,
        }
    }

    pub const fn max_pages(self) -> u64 {
        match self {
            Self::W32 => MAX_PAGES_32,
            Self::W64 => MAX_PAGES_64,
        }
    }

    /// Maximum byte count, as `u128` because the W64 result is `2^64`.
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

/// Admission budget for one linear memory. `u64` bytes plus an explicit
/// [`AddressWidth`], so a W32 cap can never constrain a W64 memory.
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

    pub const fn bytes(self) -> u64 {
        self.max_bytes
    }

    pub const fn pages(self) -> u64 {
        self.max_bytes / WASM_PAGE_SIZE
    }

    pub const fn address_width(self) -> AddressWidth {
        self.address_width
    }
}

impl Default for MemoryCap {
    fn default() -> Self {
        Self::for_address_width(AddressWidth::W32)
    }
}

/// Memory limits declared in a module. Page counts are `u64` for both widths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryType {
    pub min_pages: u64,
    pub max_pages: Option<u64>,
    pub address_width: AddressWidth,
}

impl MemoryType {
    pub const fn new(min_pages: u64, max_pages: Option<u64>) -> Self {
        Self::with_address_width(min_pages, max_pages, AddressWidth::W32)
    }

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

    pub const fn max_bytes(self) -> Option<u64> {
        match self.max_pages {
            Some(pages) => pages.checked_mul(WASM_PAGE_SIZE),
            None => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MemoryError {
    #[error("linear memory address width {memory} exceeds the cap address width {cap}")]
    AddressWidthMismatch {
        memory: AddressWidth,
        cap: AddressWidth,
    },
    #[error(
        "linear memory size of {pages} pages exceeds the {address_width} limit of {limit} pages"
    )]
    PageLimitExceeded {
        pages: u64,
        limit: u64,
        address_width: AddressWidth,
    },
    #[error("linear memory minimum of {min_pages} pages exceeds the {cap_pages}-page cap")]
    MinimumExceedsCap { min_pages: u64, cap_pages: u64 },
    #[error("linear memory maximum of {max_pages} pages is below the minimum of {min_pages} pages")]
    MaximumBelowMinimum { min_pages: u64, max_pages: u64 },
    #[error("linear memory size overflows the host address space")]
    SizeOverflow,
    #[error("linear memory allocation of {bytes} bytes failed")]
    AllocationFailed { bytes: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryReservation {
    limits: MemoryType,
    cap: MemoryCap,
    max_pages: u64,
    initial_bytes: u64,
}

impl MemoryReservation {
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

    pub const fn limits(self) -> MemoryType {
        self.limits
    }

    pub const fn cap(self) -> MemoryCap {
        self.cap
    }

    pub const fn max_pages(self) -> u64 {
        self.max_pages
    }

    pub const fn committed_bytes(self) -> u64 {
        self.initial_bytes
    }

    pub fn commit(self) -> Result<Memory, MemoryError> {
        Memory::allocate(self.limits, self.cap, self.max_pages, self.initial_bytes)
    }
}

pub struct Memory {
    bytes: Vec<u8>,
    cap: MemoryCap,
    max_pages: u64,
    address_width: AddressWidth,
}

impl Memory {
    pub fn new(limits: MemoryType, cap: MemoryCap) -> Result<Self, MemoryError> {
        MemoryReservation::new(limits, cap)?.commit()
    }

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

    pub const fn cap(&self) -> MemoryCap {
        self.cap
    }

    pub const fn address_width(&self) -> AddressWidth {
        self.address_width
    }

    pub const fn max_pages(&self) -> u64 {
        self.max_pages
    }

    pub fn pages(&self) -> u64 {
        self.bytes.len() as u64 / WASM_PAGE_SIZE
    }

    pub fn data(&self) -> &[u8] {
        &self.bytes
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }

    pub fn read_bytes(&self, offset: u64, len: u64) -> Option<&[u8]> {
        let start = usize::try_from(offset).ok()?;
        let len = usize::try_from(len).ok()?;
        let end = start.checked_add(len)?;
        self.bytes.get(start..end)
    }

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

    pub fn read_u32_le(&self, offset: u64) -> Option<u32> {
        let bytes: [u8; 4] = self.read_bytes(offset, 4)?.try_into().ok()?;
        Some(u32::from_le_bytes(bytes))
    }

    pub fn write_u32_le(&mut self, offset: u64, value: u32) -> bool {
        self.write_bytes(offset, &value.to_le_bytes())
    }

    pub fn read_u64_le(&self, offset: u64) -> Option<u64> {
        let bytes: [u8; 8] = self.read_bytes(offset, 8)?.try_into().ok()?;
        Some(u64::from_le_bytes(bytes))
    }

    pub fn write_u64_le(&mut self, offset: u64, value: u64) -> bool {
        self.write_bytes(offset, &value.to_le_bytes())
    }

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
