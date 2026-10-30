use kagura_kmem::Arena;
use kagura_value::Value;
use thiserror::Error;

use crate::layout::{MAX_IN_OBJECT_SLOTS, TAGGED_BYTES_32};
use crate::object_id::ObjectId;
use crate::shape::ShapeId;

const FLAGS_OFFSET: u32 = 4;
const SLOTS_OFFSET: u32 = 8;
const PROPERTIES_OFFSET: u32 =
    SLOTS_OFFSET + (TAGGED_BYTES_32 as u32) * (MAX_IN_OBJECT_SLOTS as u32);
const RECORD_SIZE: usize = PROPERTIES_OFFSET as usize + 4;
const PROPERTY_ARRAY_HEADER: u32 = 8;
const NO_PROPERTY_ARRAY: u32 = u32::MAX;
const MIN_PROPERTY_ARRAY_CAPACITY: u32 = 4;

#[derive(Debug, Error)]
pub enum ObjectHeapError {
    #[error("object heap exhausted")]
    OutOfMemory,
    #[error("invalid object reference")]
    InvalidObject,
    #[error("object slot out of range")]
    InvalidSlot,
    #[error("invalid property array reference")]
    InvalidPropertyArray,
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
        write_u32(
            &mut self.arena,
            offset + PROPERTIES_OFFSET,
            NO_PROPERTY_ARRAY,
        );
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

    pub fn get_property_array(
        &self,
        object: ObjectId,
        index: u32,
    ) -> Result<Value, ObjectHeapError> {
        let array = read_u32(&self.arena, object.index() + PROPERTIES_OFFSET)
            .ok_or(ObjectHeapError::InvalidObject)?;
        if array == NO_PROPERTY_ARRAY {
            return Ok(Value::undefined());
        }
        let length = read_u32(&self.arena, array).ok_or(ObjectHeapError::InvalidPropertyArray)?;
        if index >= length {
            return Ok(Value::undefined());
        }
        let raw = read_u32(&self.arena, array + PROPERTY_ARRAY_HEADER + index * 4)
            .ok_or(ObjectHeapError::InvalidPropertyArray)?;
        Ok(Value::from_raw(raw))
    }

    pub fn set_property_array(
        &mut self,
        object: ObjectId,
        index: u32,
        value: Value,
    ) -> Result<(), ObjectHeapError> {
        let required = index.checked_add(1).ok_or(ObjectHeapError::OutOfMemory)?;
        let mut array = read_u32(&self.arena, object.index() + PROPERTIES_OFFSET)
            .ok_or(ObjectHeapError::InvalidObject)?;
        if array == NO_PROPERTY_ARRAY {
            array = self.allocate_property_array(MIN_PROPERTY_ARRAY_CAPACITY.max(required))?;
            write_u32(&mut self.arena, object.index() + PROPERTIES_OFFSET, array);
        }

        let capacity =
            read_u32(&self.arena, array + 4).ok_or(ObjectHeapError::InvalidPropertyArray)?;
        if index >= capacity {
            let new_capacity = capacity
                .max(required)
                .checked_next_power_of_two()
                .ok_or(ObjectHeapError::OutOfMemory)?;
            let new_array = self.allocate_property_array(new_capacity)?;
            let length =
                read_u32(&self.arena, array).ok_or(ObjectHeapError::InvalidPropertyArray)?;
            for slot in 0..length {
                let value = read_u32(&self.arena, array + PROPERTY_ARRAY_HEADER + slot * 4)
                    .ok_or(ObjectHeapError::InvalidPropertyArray)?;
                write_u32(
                    &mut self.arena,
                    new_array + PROPERTY_ARRAY_HEADER + slot * 4,
                    value,
                );
            }
            write_u32(&mut self.arena, new_array, length);
            write_u32(
                &mut self.arena,
                object.index() + PROPERTIES_OFFSET,
                new_array,
            );
            array = new_array;
        }

        let length = read_u32(&self.arena, array).ok_or(ObjectHeapError::InvalidPropertyArray)?;
        let new_length = length.max(required);
        write_u32(&mut self.arena, array, new_length);
        write_u32(
            &mut self.arena,
            array + PROPERTY_ARRAY_HEADER + index * 4,
            value.raw(),
        );
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

    fn allocate_property_array(&mut self, capacity: u32) -> Result<u32, ObjectHeapError> {
        let size = PROPERTY_ARRAY_HEADER
            .checked_add(
                capacity
                    .checked_mul(4)
                    .ok_or(ObjectHeapError::OutOfMemory)?,
            )
            .ok_or(ObjectHeapError::OutOfMemory)?;
        let offset = self
            .arena
            .alloc(size as usize, 4)
            .ok_or(ObjectHeapError::OutOfMemory)?;
        write_u32(&mut self.arena, offset, 0);
        write_u32(&mut self.arena, offset + 4, capacity);
        Ok(offset)
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
