use std::alloc::{Layout, alloc, dealloc};
use std::ptr::NonNull;

pub struct Arena {
    ptr: NonNull<u8>,
    layout: Layout,
    capacity: usize,
    used: usize,
}

impl Arena {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "arena capacity must be positive");
        let layout = Layout::from_size_align(capacity, 8).expect("valid arena layout");
        let ptr = unsafe { alloc(layout) };
        let ptr = NonNull::new(ptr).expect("arena allocation failed");
        Self {
            ptr,
            layout,
            capacity,
            used: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn used(&self) -> usize {
        self.used
    }

    pub fn alloc(&mut self, size: usize, align: usize) -> Option<u32> {
        if size == 0 || !align.is_power_of_two() {
            return None;
        }
        let start = self.used.checked_add(align - 1)? & !(align - 1);
        let end = start.checked_add(size)?;
        if end > self.capacity || end > u32::MAX as usize {
            return None;
        }
        self.used = end;
        Some(start as u32)
    }

    pub fn write_bytes(&mut self, offset: u32, bytes: &[u8]) -> bool {
        let start = offset as usize;
        let Some(end) = start.checked_add(bytes.len()) else {
            return false;
        };
        if end > self.used {
            return false;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                self.ptr.as_ptr().add(start),
                bytes.len(),
            );
        }
        true
    }

    pub fn read_bytes(&self, offset: u32, len: usize) -> Option<&[u8]> {
        let start = offset as usize;
        let end = start.checked_add(len)?;
        if end > self.used {
            return None;
        }
        Some(unsafe { std::slice::from_raw_parts(self.ptr.as_ptr().add(start), len) })
    }

    pub fn alloc_str(&mut self, value: &str) -> Option<u32> {
        let len = value.len();
        if len > u32::MAX as usize {
            return None;
        }
        let offset = self.alloc(4 + len, 4)?;
        self.write_bytes(offset, &(len as u32).to_ne_bytes());
        self.write_bytes(offset + 4, value.as_bytes());
        Some(offset)
    }

    pub fn read_str(&self, offset: u32) -> Option<&str> {
        let len_bytes = self.read_bytes(offset, 4)?;
        let len = u32::from_ne_bytes(len_bytes.try_into().ok()?) as usize;
        let bytes = self.read_bytes(offset.checked_add(4)?, len)?;
        std::str::from_utf8(bytes).ok()
    }

    pub fn alloc_f64(&mut self, value: f64) -> Option<u32> {
        let offset = self.alloc(8, 8)?;
        self.write_bytes(offset, &value.to_ne_bytes());
        Some(offset)
    }

    pub fn read_f64(&self, offset: u32) -> Option<f64> {
        let bytes = self.read_bytes(offset, 8)?;
        Some(f64::from_ne_bytes(bytes.try_into().ok()?))
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        unsafe { dealloc(self.ptr.as_ptr(), self.layout) }
    }
}
