pub const HEADER_BYTES_32: usize = 8;
pub const TAGGED_BYTES_32: usize = 4;
pub const MAX_IN_OBJECT_SLOTS: u16 = 8;

pub const fn object_size_32(in_object_slots: u16) -> u16 {
    HEADER_BYTES_32 as u16 + in_object_slots * TAGGED_BYTES_32 as u16
}
