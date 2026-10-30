use kagura_value::Value;

/// Opaque slot in the runtime handle table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handle {
    index: u32,
    generation: u32,
}

struct HandleSlot {
    value: Option<Value>,
    generation: u32,
}

/// External roots owned by the embedder. Values remain reachable until their
/// handle is dropped.
#[derive(Default)]
pub struct HandleTable {
    slots: Vec<HandleSlot>,
    free: Vec<u32>,
}

impl HandleTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create(&mut self, value: Value) -> Handle {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.value = Some(value);
            return Handle {
                index,
                generation: slot.generation,
            };
        }
        let index = self.slots.len() as u32;
        self.slots.push(HandleSlot {
            value: Some(value),
            generation: 0,
        });
        Handle {
            index,
            generation: 0,
        }
    }

    pub fn drop_handle(&mut self, handle: Handle) -> Option<Value> {
        let slot = self.slots.get_mut(handle.index as usize)?;
        if slot.generation != handle.generation {
            return None;
        }
        let value = slot.value.take();
        if value.is_some() {
            slot.generation = slot.generation.wrapping_add(1);
            self.free.push(handle.index);
        }
        value
    }

    pub fn get(&self, handle: Handle) -> Option<Value> {
        let slot = self.slots.get(handle.index as usize)?;
        if slot.generation != handle.generation {
            return None;
        }
        slot.value
    }

    pub fn roots(&self) -> impl Iterator<Item = Value> + '_ {
        self.slots.iter().filter_map(|slot| slot.value)
    }

    pub fn len(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.value.is_some())
            .count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use kagura_value::Value;

    use super::HandleTable;

    #[test]
    fn stale_handle_cannot_read_reused_slot() {
        let mut table = HandleTable::new();
        let first = table.create(Value::undefined());
        assert!(table.drop_handle(first).is_some());
        let second = table.create(Value::undefined());
        assert_eq!(table.get(first), None);
        assert_eq!(table.get(second), Some(Value::undefined()));
    }

    #[test]
    fn roots_skip_dropped_slots() {
        let mut table = HandleTable::new();
        let first = table.create(Value::undefined());
        let second = table.create(Value::undefined());
        table.drop_handle(first);
        assert_eq!(table.roots().count(), 1);
        assert!(table.get(second).is_some());
    }
}
