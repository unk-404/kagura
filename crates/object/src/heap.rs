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
const ELEMENTS_OFFSET: u32 = PROPERTIES_OFFSET + 4;
const RECORD_SIZE: u32 = ELEMENTS_OFFSET + 4;

const BLOCK_HEADER: u32 = 12;
const BLOCK_LENGTH_OFFSET: u32 = 4;
const BLOCK_CAPACITY_OFFSET: u32 = 8;

const FLAG_ARRAY: u32 = 1;
const FLAG_MARK: u32 = 2;
const FLAG_FUNCTION: u32 = 4;
const FLAG_BOUND: u32 = 8;
const NO_BLOCK: u32 = u32::MAX;
const MIN_ARRAY_CAPACITY: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockKind {
    Object,
    Array,
}

#[derive(Clone, Copy, Debug)]
struct Allocation {
    offset: u32,
    size: u32,
    kind: BlockKind,
}

#[derive(Clone, Copy, Debug)]
struct FreeBlock {
    offset: u32,
    size: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GcStats {
    pub live_bytes_before: u32,
    pub live_bytes_after: u32,
    pub freed_objects: u32,
    pub freed_arrays: u32,
}

#[derive(Debug, Error)]
pub enum ObjectHeapError {
    #[error("object heap exhausted")]
    OutOfMemory,
    #[error("invalid object reference")]
    InvalidObject,
    #[error("object slot out of range")]
    InvalidSlot,
    #[error("invalid array reference")]
    InvalidArray,
    #[error("value is not an array")]
    NotArray,
}

pub struct ObjectHeap {
    arena: Arena,
    allocations: Vec<Allocation>,
    free: Vec<FreeBlock>,
}

impl ObjectHeap {
    pub fn new(capacity: usize) -> Self {
        Self {
            arena: Arena::new(capacity),
            allocations: Vec::new(),
            free: Vec::new(),
        }
    }

    pub fn allocate(&mut self, shape: ShapeId) -> Result<ObjectId, ObjectHeapError> {
        let offset = self.alloc_block(RECORD_SIZE, BlockKind::Object)?;
        write_u32(&mut self.arena, offset, shape.index());
        write_u32(&mut self.arena, offset + FLAGS_OFFSET, 0);
        for slot in 0..MAX_IN_OBJECT_SLOTS {
            write_u32(
                &mut self.arena,
                slot_offset(offset, slot),
                Value::undefined().raw(),
            );
        }
        write_u32(&mut self.arena, offset + PROPERTIES_OFFSET, NO_BLOCK);
        write_u32(&mut self.arena, offset + ELEMENTS_OFFSET, NO_BLOCK);
        Ok(ObjectId::new(offset))
    }

    pub fn allocate_array(&mut self, shape: ShapeId) -> Result<ObjectId, ObjectHeapError> {
        let object = self.allocate(shape)?;
        write_u32(&mut self.arena, object.index() + FLAGS_OFFSET, FLAG_ARRAY);
        Ok(object)
    }

    pub fn allocate_function(
        &mut self,
        shape: ShapeId,
        function_index: u32,
        env: Value,
        captured_this: Value,
    ) -> Result<ObjectId, ObjectHeapError> {
        let object = self.allocate(shape)?;
        write_u32(
            &mut self.arena,
            object.index() + FLAGS_OFFSET,
            FLAG_FUNCTION,
        );
        self.set_slot(object, 0, Value::smi(function_index as i32))?;
        self.set_slot(object, 1, env)?;
        self.set_slot(object, 2, captured_this)?;
        Ok(object)
    }

    pub fn is_function(&self, object: ObjectId) -> Result<bool, ObjectHeapError> {
        let flags = read_u32(&self.arena, object.index() + FLAGS_OFFSET)
            .ok_or(ObjectHeapError::InvalidObject)?;
        Ok(flags & FLAG_FUNCTION != 0)
    }

    pub fn function_data(&self, object: ObjectId) -> Result<(u32, Value), ObjectHeapError> {
        if !self.is_function(object)? {
            return Err(ObjectHeapError::InvalidObject);
        }
        let index = self
            .get_slot(object, 0)?
            .as_smi()
            .ok_or(ObjectHeapError::InvalidObject)? as u32;
        Ok((index, self.get_slot(object, 1)?))
    }

    pub fn function_this(&self, object: ObjectId) -> Result<Value, ObjectHeapError> {
        if !self.is_function(object)? {
            return Err(ObjectHeapError::InvalidObject);
        }
        self.get_slot(object, 2)
    }

    pub fn allocate_bound_function(
        &mut self,
        shape: ShapeId,
        function_index: u32,
        env: Value,
        bound_this: Value,
        bound_args: Value,
        target: Value,
    ) -> Result<ObjectId, ObjectHeapError> {
        let object = self.allocate(shape)?;
        write_u32(
            &mut self.arena,
            object.index() + FLAGS_OFFSET,
            FLAG_FUNCTION | FLAG_BOUND,
        );
        self.set_slot(object, 0, Value::smi(function_index as i32))?;
        self.set_slot(object, 1, env)?;
        self.set_slot(object, 2, bound_this)?;
        self.set_slot(object, 3, bound_args)?;
        self.set_slot(object, 4, target)?;
        Ok(object)
    }

    pub fn is_bound(&self, object: ObjectId) -> Result<bool, ObjectHeapError> {
        let flags = read_u32(&self.arena, object.index() + FLAGS_OFFSET)
            .ok_or(ObjectHeapError::InvalidObject)?;
        Ok(flags & (FLAG_FUNCTION | FLAG_BOUND) == (FLAG_FUNCTION | FLAG_BOUND))
    }

    pub fn bound_data(&self, object: ObjectId) -> Result<(Value, Value, Value), ObjectHeapError> {
        if !self.is_bound(object)? {
            return Err(ObjectHeapError::InvalidObject);
        }
        Ok((
            self.get_slot(object, 2)?,
            self.get_slot(object, 3)?,
            self.get_slot(object, 4)?,
        ))
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

    pub fn is_array(&self, object: ObjectId) -> Result<bool, ObjectHeapError> {
        let flags = read_u32(&self.arena, object.index() + FLAGS_OFFSET)
            .ok_or(ObjectHeapError::InvalidObject)?;
        Ok(flags & FLAG_ARRAY != 0)
    }

    pub fn get_slot(&self, object: ObjectId, slot: u16) -> Result<Value, ObjectHeapError> {
        if slot >= MAX_IN_OBJECT_SLOTS {
            return Err(ObjectHeapError::InvalidSlot);
        }
        let raw = read_u32(&self.arena, slot_offset(object.index(), slot))
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
        let offset = slot_offset(object.index(), slot);
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
        let block = read_u32(&self.arena, object.index() + PROPERTIES_OFFSET)
            .ok_or(ObjectHeapError::InvalidObject)?;
        if block == NO_BLOCK {
            return Ok(Value::undefined());
        }
        let length = read_u32(&self.arena, block + BLOCK_LENGTH_OFFSET)
            .ok_or(ObjectHeapError::InvalidArray)?;
        if index >= length {
            return Ok(Value::undefined());
        }
        let raw = read_u32(&self.arena, block + BLOCK_HEADER + index * 4)
            .ok_or(ObjectHeapError::InvalidArray)?;
        Ok(Value::from_raw(raw))
    }

    pub fn set_property_array(
        &mut self,
        object: ObjectId,
        index: u32,
        value: Value,
    ) -> Result<(), ObjectHeapError> {
        let required = index.checked_add(1).ok_or(ObjectHeapError::OutOfMemory)?;
        let block = self.ensure_block(object, PROPERTIES_OFFSET, required)?;
        let length = read_u32(&self.arena, block + BLOCK_LENGTH_OFFSET).unwrap_or(0);
        write_u32(
            &mut self.arena,
            block + BLOCK_LENGTH_OFFSET,
            length.max(required),
        );
        write_u32(
            &mut self.arena,
            block + BLOCK_HEADER + index * 4,
            value.raw(),
        );
        Ok(())
    }

    pub fn array_length(&self, object: ObjectId) -> Result<u32, ObjectHeapError> {
        if !self.is_array(object)? {
            return Err(ObjectHeapError::NotArray);
        }
        let block = read_u32(&self.arena, object.index() + ELEMENTS_OFFSET)
            .ok_or(ObjectHeapError::InvalidObject)?;
        if block == NO_BLOCK {
            return Ok(0);
        }
        read_u32(&self.arena, block + BLOCK_LENGTH_OFFSET).ok_or(ObjectHeapError::InvalidArray)
    }

    pub fn set_array_length(
        &mut self,
        object: ObjectId,
        length: u32,
    ) -> Result<(), ObjectHeapError> {
        if !self.is_array(object)? {
            return Err(ObjectHeapError::NotArray);
        }
        if length == 0 {
            let block = read_u32(&self.arena, object.index() + ELEMENTS_OFFSET)
                .ok_or(ObjectHeapError::InvalidObject)?;
            if block != NO_BLOCK {
                write_u32(&mut self.arena, block + BLOCK_LENGTH_OFFSET, 0);
            }
            return Ok(());
        }
        let block = self.ensure_block(object, ELEMENTS_OFFSET, length)?;
        write_u32(&mut self.arena, block + BLOCK_LENGTH_OFFSET, length);
        Ok(())
    }

    pub fn array_get(&self, object: ObjectId, index: u32) -> Result<Value, ObjectHeapError> {
        let value = self.array_get_raw(object, index)?;
        if value.is_hole() {
            Ok(Value::undefined())
        } else {
            Ok(value)
        }
    }

    pub fn array_pop(&mut self, object: ObjectId) -> Result<Value, ObjectHeapError> {
        let length = self.array_length(object)?;
        if length == 0 {
            return Ok(Value::undefined());
        }
        let value = self.array_get(object, length - 1)?;
        self.set_array_length(object, length - 1)?;
        Ok(value)
    }

    pub fn array_get_raw(&self, object: ObjectId, index: u32) -> Result<Value, ObjectHeapError> {
        if !self.is_array(object)? {
            return Err(ObjectHeapError::NotArray);
        }
        let block = read_u32(&self.arena, object.index() + ELEMENTS_OFFSET)
            .ok_or(ObjectHeapError::InvalidObject)?;
        if block == NO_BLOCK {
            return Ok(Value::undefined());
        }
        let length = read_u32(&self.arena, block + BLOCK_LENGTH_OFFSET)
            .ok_or(ObjectHeapError::InvalidArray)?;
        if index >= length {
            return Ok(Value::undefined());
        }
        let raw = read_u32(&self.arena, block + BLOCK_HEADER + index * 4)
            .ok_or(ObjectHeapError::InvalidArray)?;
        Ok(Value::from_raw(raw))
    }

    pub fn array_has(&self, object: ObjectId, index: u32) -> Result<bool, ObjectHeapError> {
        if !self.is_array(object)? {
            return Err(ObjectHeapError::NotArray);
        }
        let block = read_u32(&self.arena, object.index() + ELEMENTS_OFFSET)
            .ok_or(ObjectHeapError::InvalidObject)?;
        if block == NO_BLOCK {
            return Ok(false);
        }
        let length = read_u32(&self.arena, block + BLOCK_LENGTH_OFFSET)
            .ok_or(ObjectHeapError::InvalidArray)?;
        if index >= length {
            return Ok(false);
        }
        let raw = read_u32(&self.arena, block + BLOCK_HEADER + index * 4)
            .ok_or(ObjectHeapError::InvalidArray)?;
        Ok(!Value::from_raw(raw).is_hole())
    }

    pub fn array_set(
        &mut self,
        object: ObjectId,
        index: u32,
        value: Value,
    ) -> Result<(), ObjectHeapError> {
        if !self.is_array(object)? {
            return Err(ObjectHeapError::NotArray);
        }
        let required = index.checked_add(1).ok_or(ObjectHeapError::OutOfMemory)?;
        let block = self.ensure_block(object, ELEMENTS_OFFSET, required)?;
        let length = read_u32(&self.arena, block + BLOCK_LENGTH_OFFSET).unwrap_or(0);
        write_u32(
            &mut self.arena,
            block + BLOCK_LENGTH_OFFSET,
            length.max(required),
        );
        write_u32(
            &mut self.arena,
            block + BLOCK_HEADER + index * 4,
            value.raw(),
        );
        Ok(())
    }

    pub fn array_push(&mut self, object: ObjectId, value: Value) -> Result<(), ObjectHeapError> {
        if !self.is_array(object)? {
            return Err(ObjectHeapError::NotArray);
        }
        let length = self.array_length(object)?;
        let required = length.checked_add(1).ok_or(ObjectHeapError::OutOfMemory)?;
        let block = self.ensure_block(object, ELEMENTS_OFFSET, required)?;
        write_u32(&mut self.arena, block + BLOCK_LENGTH_OFFSET, required);
        write_u32(
            &mut self.arena,
            block + BLOCK_HEADER + length * 4,
            value.raw(),
        );
        Ok(())
    }

    fn ensure_block(
        &mut self,
        object: ObjectId,
        pointer_offset: u32,
        required: u32,
    ) -> Result<u32, ObjectHeapError> {
        let mut block = read_u32(&self.arena, object.index() + pointer_offset)
            .ok_or(ObjectHeapError::InvalidObject)?;
        if block == NO_BLOCK {
            block = self.allocate_array_block(MIN_ARRAY_CAPACITY.max(required))?;
            write_u32(&mut self.arena, object.index() + pointer_offset, block);
        }
        let capacity = read_u32(&self.arena, block + BLOCK_CAPACITY_OFFSET)
            .ok_or(ObjectHeapError::InvalidArray)?;
        if required > capacity {
            let new_capacity = capacity
                .max(required)
                .checked_next_power_of_two()
                .ok_or(ObjectHeapError::OutOfMemory)?;
            let new_block = self.allocate_array_block(new_capacity)?;
            let length = read_u32(&self.arena, block + BLOCK_LENGTH_OFFSET)
                .ok_or(ObjectHeapError::InvalidArray)?;
            for slot in 0..length {
                let value = read_u32(&self.arena, block + BLOCK_HEADER + slot * 4)
                    .ok_or(ObjectHeapError::InvalidArray)?;
                write_u32(&mut self.arena, new_block + BLOCK_HEADER + slot * 4, value);
            }
            write_u32(&mut self.arena, new_block + BLOCK_LENGTH_OFFSET, length);
            write_u32(&mut self.arena, object.index() + pointer_offset, new_block);
            block = new_block;
        }
        Ok(block)
    }

    fn allocate_array_block(&mut self, capacity: u32) -> Result<u32, ObjectHeapError> {
        let size = BLOCK_HEADER
            .checked_add(
                capacity
                    .checked_mul(4)
                    .ok_or(ObjectHeapError::OutOfMemory)?,
            )
            .ok_or(ObjectHeapError::OutOfMemory)?;
        let offset = self.alloc_block(size, BlockKind::Array)?;
        write_u32(&mut self.arena, offset, 0);
        write_u32(&mut self.arena, offset + BLOCK_LENGTH_OFFSET, 0);
        write_u32(&mut self.arena, offset + BLOCK_CAPACITY_OFFSET, capacity);
        for slot in 0..capacity {
            write_u32(
                &mut self.arena,
                offset + BLOCK_HEADER + slot * 4,
                Value::undefined().raw(),
            );
        }
        Ok(offset)
    }

    fn alloc_block(&mut self, size: u32, kind: BlockKind) -> Result<u32, ObjectHeapError> {
        if let Some(position) = self.free.iter().position(|block| block.size >= size) {
            let block = self.free.swap_remove(position);
            if block.size > size {
                self.free.push(FreeBlock {
                    offset: block.offset + size,
                    size: block.size - size,
                });
            }
            self.allocations.push(Allocation {
                offset: block.offset,
                size,
                kind,
            });
            return Ok(block.offset);
        }
        let offset = self
            .arena
            .alloc(size as usize, 4)
            .ok_or(ObjectHeapError::OutOfMemory)?;
        self.allocations.push(Allocation { offset, size, kind });
        Ok(offset)
    }

    pub fn collect(&mut self, roots: &[Value]) -> GcStats {
        let live_bytes_before = self.live_bytes();
        let mut work = Vec::new();
        for root in roots {
            if let Some(offset) = root.object_offset() {
                self.mark(offset, &mut work);
            }
        }
        while let Some(offset) = work.pop() {
            self.trace(offset, &mut work);
        }

        let mut live = Vec::new();
        let mut freed_objects = 0;
        let mut freed_arrays = 0;
        for allocation in std::mem::take(&mut self.allocations) {
            if self.is_marked(&allocation) {
                self.clear_mark(&allocation);
                live.push(allocation);
            } else {
                match allocation.kind {
                    BlockKind::Object => freed_objects += 1,
                    BlockKind::Array => freed_arrays += 1,
                }
                self.free.push(FreeBlock {
                    offset: allocation.offset,
                    size: allocation.size,
                });
            }
        }
        self.allocations = live;
        GcStats {
            live_bytes_before,
            live_bytes_after: self.live_bytes(),
            freed_objects,
            freed_arrays,
        }
    }

    fn mark(&mut self, offset: u32, work: &mut Vec<u32>) {
        let Some(allocation) = self.allocation(offset) else {
            return;
        };
        if self.is_marked(&allocation) {
            return;
        }
        self.set_mark(&allocation);
        work.push(offset);
    }

    fn trace(&mut self, offset: u32, work: &mut Vec<u32>) {
        let Some(allocation) = self.allocation(offset) else {
            return;
        };
        match allocation.kind {
            BlockKind::Object => {
                for slot in 0..MAX_IN_OBJECT_SLOTS {
                    if let Some(value) = read_u32(&self.arena, slot_offset(offset, slot)) {
                        self.mark_value(value, work);
                    }
                }
                for pointer_offset in [PROPERTIES_OFFSET, ELEMENTS_OFFSET] {
                    if let Some(block) = read_u32(&self.arena, offset + pointer_offset)
                        && block != NO_BLOCK
                    {
                        self.mark(block, work);
                    }
                }
            }
            BlockKind::Array => {
                let length = read_u32(&self.arena, offset + BLOCK_LENGTH_OFFSET).unwrap_or(0);
                for index in 0..length {
                    if let Some(value) = read_u32(&self.arena, offset + BLOCK_HEADER + index * 4) {
                        self.mark_value(value, work);
                    }
                }
            }
        }
    }

    fn mark_value(&mut self, raw: u32, work: &mut Vec<u32>) {
        if let Some(offset) = Value::from_raw(raw).object_offset() {
            self.mark(offset, work);
        }
    }

    fn allocation(&self, offset: u32) -> Option<Allocation> {
        self.allocations
            .iter()
            .find(|allocation| allocation.offset == offset)
            .copied()
    }

    fn is_marked(&self, allocation: &Allocation) -> bool {
        let offset = match allocation.kind {
            BlockKind::Object => allocation.offset + FLAGS_OFFSET,
            BlockKind::Array => allocation.offset,
        };
        read_u32(&self.arena, offset)
            .map(|flags| flags & FLAG_MARK != 0)
            .unwrap_or(false)
    }

    fn set_mark(&mut self, allocation: &Allocation) {
        let offset = match allocation.kind {
            BlockKind::Object => allocation.offset + FLAGS_OFFSET,
            BlockKind::Array => allocation.offset,
        };
        if let Some(flags) = read_u32(&self.arena, offset) {
            write_u32(&mut self.arena, offset, flags | FLAG_MARK);
        }
    }

    fn clear_mark(&mut self, allocation: &Allocation) {
        let offset = match allocation.kind {
            BlockKind::Object => allocation.offset + FLAGS_OFFSET,
            BlockKind::Array => allocation.offset,
        };
        if let Some(flags) = read_u32(&self.arena, offset) {
            write_u32(&mut self.arena, offset, flags & !FLAG_MARK);
        }
    }

    pub fn live_bytes(&self) -> u32 {
        self.allocations
            .iter()
            .map(|allocation| allocation.size)
            .sum()
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
}

fn slot_offset(object: u32, slot: u16) -> u32 {
    object + SLOTS_OFFSET + slot as u32 * TAGGED_BYTES_32 as u32
}

fn read_u32(arena: &Arena, offset: u32) -> Option<u32> {
    let bytes = arena.read_bytes(offset, 4)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

fn write_u32(arena: &mut Arena, offset: u32, value: u32) {
    let _ = arena.write_bytes(offset, &value.to_le_bytes());
}
