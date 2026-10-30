use kagura_kmem::Arena;
use kagura_value::Value;
use thiserror::Error;

use crate::layout::{MAX_IN_OBJECT_SLOTS, TAGGED_BYTES_32};
use crate::object_id::ObjectId;
use crate::shape::ShapeId;

const FLAGS_OFFSET: u32 = 4;
const SLOTS_OFFSET: u32 = 8;
const RECORD_SIZE: usize = SLOTS_OFFSET as usize + TAGGED_BYTES_32 * MAX_IN_OBJECT_SLOTS as usize;

#[derive(Debug, Error)]
pub enum ObjectHeapError {
    #[error("object heap exhausted")]
    OutOfMemory,
    #[error("invalid object reference")]
    InvalidObject,
    #[error("object slot out of range")]
    InvalidSlot,
}

pub struct ObjectHeap {
    arena: Arena,
}

impl ObjectHeap {
    pub fn new(capacity: usize) -> Self {
        Self {
            arena: Arena::new(capacity),
        }
    }

    pub fn allocate(&mut self, shape: ShapeId) -> Result<ObjectId, ObjectHeapError> {
        let offset = self
            .arena
            .alloc(RECORD_SIZE, 4)
            .ok_or(ObjectHeapError::OutOfMemory)?;
        write_u32(&mut self.arena, offset, shape.index());
        write_u32(&mut self.arena, offset + FLAGS_OFFSET, 0);
        for slot in 0..MAX_IN_OBJECT_SLOTS {
            write_u32(
                &mut self.arena,
                Self::slot_offset(offset, slot),
                Value::undefined().raw(),
            );
        }
        Ok(ObjectId::new(offset))
    }

    pub fn shape(&self, object: ObjectId) -> Result<ShapeId, ObjectHeapError> {
        let raw = read_u32(&self.arena, object.index()).ok_or(ObjectHeapError::InvalidObject)?;
        Ok(ShapeId::new(raw))
    }

    pub fn set_shape(&mut self, object: ObjectId, shape: ShapeId) -> Result<(), ObjectHeapError> {
        if !self
            .arena
            .write_bytes(object.index(), &shape.index().to_le_bytes())
        {
            return Err(ObjectHeapError::InvalidObject);
        }
        Ok(())
    }

    pub fn get_slot(&self, object: ObjectId, slot: u16) -> Result<Value, ObjectHeapError> {
        if slot >= MAX_IN_OBJECT_SLOTS {
            return Err(ObjectHeapError::InvalidSlot);
        }
        let raw = read_u32(&self.arena, Self::slot_offset(object.index(), slot))
            .ok_or(ObjectHeapError::InvalidObject)?;
        Ok(Value::from_raw(raw))
    }

    pub fn set_slot(
        &mut self,
        object: ObjectId,
        slot: u16,
        value: Value,
    ) -> Result<(), ObjectHeapError> {
        if slot >= MAX_IN_OBJECT_SLOTS {
            return Err(ObjectHeapError::InvalidSlot);
        }
        let offset = Self::slot_offset(object.index(), slot);
        if !self.arena.write_bytes(offset, &value.raw().to_le_bytes()) {
            return Err(ObjectHeapError::InvalidObject);
        }
        Ok(())
    }

    pub fn value(object: ObjectId) -> Value {
        Value::object(object.index())
    }

    pub fn used(&self) -> usize {
        self.arena.used()
    }

    pub fn capacity(&self) -> usize {
        self.arena.capacity()
    }

    const fn slot_offset(object: u32, slot: u16) -> u32 {
        object + SLOTS_OFFSET + slot as u32 * TAGGED_BYTES_32 as u32
    }
}

fn read_u32(arena: &Arena, offset: u32) -> Option<u32> {
    let bytes = arena.read_bytes(offset, 4)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

fn write_u32(arena: &mut Arena, offset: u32, value: u32) {
    let _ = arena.write_bytes(offset, &value.to_le_bytes());
}
