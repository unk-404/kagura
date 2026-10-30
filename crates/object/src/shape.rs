use kagura_support::FastHashMap;
use thiserror::Error;

use crate::attributes::PropertyAttributes;
use crate::elements::ElementsKind;
use crate::key::{PropertyKey, ordered_keys};
use crate::layout::{MAX_IN_OBJECT_SLOTS, object_size_32};
use crate::object_id::ObjectId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ShapeId(u32);

impl ShapeId {
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    pub const fn index(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertyLocation {
    InObject(u16),
    PropertyArray(u32),
}

#[derive(Clone, Debug)]
pub struct Descriptor {
    pub key: PropertyKey,
    pub location: PropertyLocation,
    pub attributes: PropertyAttributes,
}

#[derive(Clone, Debug)]
pub struct Shape {
    pub id: ShapeId,
    pub parent: Option<ShapeId>,
    pub prototype: Option<ObjectId>,
    pub flags: u16,
    pub instance_size: u16,
    pub in_object_slots: u16,
    pub elements_kind: ElementsKind,
    pub descriptors: Vec<Descriptor>,
    pub transitions: FastHashMap<PropertyKey, ShapeId>,
    pub is_dictionary: bool,
}

#[derive(Debug, Error)]
pub enum ShapeError {
    #[error("unknown shape")]
    UnknownShape,
    #[error("property already exists on shape")]
    DuplicateProperty,
}

pub struct ShapeTable {
    shapes: Vec<Shape>,
}

impl Default for ShapeTable {
    fn default() -> Self {
        Self::new()
    }
}

impl ShapeTable {
    pub fn new() -> Self {
        let root = Shape {
            id: ShapeId::new(0),
            parent: None,
            prototype: None,
            flags: 0,
            instance_size: object_size_32(0),
            in_object_slots: 0,
            elements_kind: ElementsKind::Smi,
            descriptors: Vec::new(),
            transitions: FastHashMap::default(),
            is_dictionary: false,
        };
        Self { shapes: vec![root] }
    }

    pub fn root(&self) -> ShapeId {
        ShapeId::new(0)
    }

    pub fn with_prototype(
        &mut self,
        shape: ShapeId,
        prototype: Option<ObjectId>,
    ) -> Result<ShapeId, ShapeError> {
        let mut next = self.get(shape)?.clone();
        next.id = ShapeId::new(self.shapes.len() as u32);
        next.prototype = prototype;
        next.transitions = FastHashMap::default();
        let id = next.id;
        self.shapes.push(next);
        Ok(id)
    }

    pub fn redescribe(
        &mut self,
        shape: ShapeId,
        key: &PropertyKey,
        attributes: PropertyAttributes,
    ) -> Result<ShapeId, ShapeError> {
        let mut next = self.get(shape)?.clone();
        let descriptor = next
            .descriptors
            .iter_mut()
            .find(|descriptor| &descriptor.key == key)
            .ok_or(ShapeError::UnknownShape)?;
        descriptor.attributes = attributes;
        next.id = ShapeId::new(self.shapes.len() as u32);
        next.parent = Some(shape);
        next.transitions = FastHashMap::default();
        let id = next.id;
        self.shapes.push(next);
        Ok(id)
    }

    pub fn set_non_extensible(&mut self, shape: ShapeId) -> Result<ShapeId, ShapeError> {
        let mut next = self.get(shape)?.clone();
        next.id = ShapeId::new(self.shapes.len() as u32);
        next.parent = Some(shape);
        next.flags |= 1;
        next.transitions = FastHashMap::default();
        let id = next.id;
        self.shapes.push(next);
        Ok(id)
    }

    pub fn remove_property(
        &mut self,
        shape: ShapeId,
        key: &PropertyKey,
    ) -> Result<ShapeId, ShapeError> {
        let mut next = self.get(shape)?.clone();
        let index = next
            .descriptors
            .iter()
            .position(|descriptor| &descriptor.key == key)
            .ok_or(ShapeError::UnknownShape)?;
        next.descriptors.remove(index);
        next.id = ShapeId::new(self.shapes.len() as u32);
        next.parent = Some(shape);
        next.transitions = FastHashMap::default();
        let id = next.id;
        self.shapes.push(next);
        Ok(id)
    }

    pub fn is_non_extensible(&self, shape: ShapeId) -> Result<bool, ShapeError> {
        Ok(self.get(shape)?.flags & 1 != 0)
    }

    pub fn get(&self, id: ShapeId) -> Result<&Shape, ShapeError> {
        self.shapes
            .get(id.index() as usize)
            .ok_or(ShapeError::UnknownShape)
    }

    pub fn add_property(
        &mut self,
        shape: ShapeId,
        key: PropertyKey,
        attributes: PropertyAttributes,
    ) -> Result<ShapeId, ShapeError> {
        let parent = self.get(shape)?;
        if let Some(id) = parent.transitions.get(&key) {
            return Ok(*id);
        }
        if parent
            .descriptors
            .iter()
            .any(|descriptor| descriptor.key == key)
        {
            return Err(ShapeError::DuplicateProperty);
        }

        let location = if parent.in_object_slots < MAX_IN_OBJECT_SLOTS {
            PropertyLocation::InObject(parent.in_object_slots)
        } else {
            let overflow = parent
                .descriptors
                .len()
                .saturating_sub(MAX_IN_OBJECT_SLOTS as usize);
            PropertyLocation::PropertyArray(overflow as u32)
        };
        let in_object_slots = if matches!(location, PropertyLocation::InObject(_)) {
            parent.in_object_slots + 1
        } else {
            parent.in_object_slots
        };
        let mut descriptors = parent.descriptors.clone();
        descriptors.push(Descriptor {
            key: key.clone(),
            location,
            attributes,
        });

        let id = ShapeId::new(self.shapes.len() as u32);
        let created = Shape {
            id,
            parent: Some(shape),
            prototype: parent.prototype,
            flags: parent.flags,
            instance_size: object_size_32(in_object_slots),
            in_object_slots,
            elements_kind: parent.elements_kind,
            descriptors,
            transitions: FastHashMap::default(),
            is_dictionary: false,
        };
        self.shapes.push(created);
        self.shapes[shape.index() as usize]
            .transitions
            .insert(key, id);
        Ok(id)
    }

    pub fn lookup(
        &self,
        shape: ShapeId,
        key: &PropertyKey,
    ) -> Result<Option<&Descriptor>, ShapeError> {
        Ok(self
            .get(shape)?
            .descriptors
            .iter()
            .find(|descriptor| &descriptor.key == key))
    }

    pub fn len(&self) -> usize {
        self.shapes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }

    pub fn keys(&self, shape: ShapeId) -> Result<Vec<PropertyKey>, ShapeError> {
        Ok(self
            .get(shape)?
            .descriptors
            .iter()
            .map(|descriptor| descriptor.key.clone())
            .collect())
    }
}

pub fn own_property_keys(
    shapes: &ShapeTable,
    shape: ShapeId,
) -> Result<Vec<PropertyKey>, ShapeError> {
    Ok(ordered_keys(&shapes.keys(shape)?))
}
