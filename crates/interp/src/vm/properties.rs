use kagura_object::{PropertyAttributes, PropertyKey, PropertyLocation};
use kagura_support::Span;
use kagura_value::Value;

use crate::error::RuntimeError;

use super::Interpreter;

impl<'a> Interpreter<'a> {
    pub(super) fn get_property(
        &mut self,
        object: Value,
        key: &str,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        if key == "length"
            && let Some(length) = self
                .heap
                .as_str(object)
                .map(|text| text.encode_utf16().count() as f64)
        {
            return Ok(self.heap.number(length));
        }
        let id = self.object_id(object, span)?;
        if key == "length"
            && self
                .objects
                .is_array(id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            let length = self
                .objects
                .array_length(id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            return Ok(self.heap.number(length as f64));
        }
        let mut shape = self
            .objects
            .shape(id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let mut owner = id;
        let key = PropertyKey::from_string(key);
        loop {
            let descriptor = self
                .shapes
                .lookup(shape, &key)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            if let Some(descriptor) = descriptor {
                return match descriptor.location {
                    PropertyLocation::InObject(slot) => self
                        .objects
                        .get_slot(owner, slot)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span)),
                    PropertyLocation::PropertyArray(index) => self
                        .objects
                        .get_property_array(owner, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span)),
                };
            }
            let prototype = self
                .shapes
                .get(shape)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                .prototype;
            let Some(prototype) = prototype else {
                return Ok(Value::undefined());
            };
            owner = prototype;
            shape = self
                .objects
                .shape(prototype)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        }
    }

    pub(super) fn set_property(
        &mut self,
        object: Value,
        key: &str,
        value: Value,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let id = self.object_id(object, span)?;
        if key == "length"
            && self
                .objects
                .is_array(id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            let length = self.index_value(value, span)?;
            self.objects
                .set_array_length(id, length)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            return Ok(());
        }
        let shape = self
            .objects
            .shape(id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let key = PropertyKey::from_string(key);
        let existing = self
            .shapes
            .lookup(shape, &key)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            .map(|descriptor| descriptor.location);
        let location = match existing {
            Some(location) => location,
            None => {
                let new_shape = self
                    .shapes
                    .add_property(shape, key.clone(), PropertyAttributes::data())
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                self.objects
                    .set_shape(id, new_shape)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                self.shapes
                    .lookup(new_shape, &key)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    .expect("new shape has descriptor")
                    .location
            }
        };
        match location {
            PropertyLocation::InObject(slot) => self
                .objects
                .set_slot(id, slot, value)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span)),
            PropertyLocation::PropertyArray(index) => self
                .objects
                .set_property_array(id, index, value)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span)),
        }
    }

    pub(super) fn has_property(
        &mut self,
        key: Value,
        target: Value,
        span: Span,
    ) -> Result<bool, RuntimeError> {
        if target.is_string() {
            return Err(RuntimeError::type_error(
                "right-hand side of 'in' is not an object",
                span,
            ));
        }
        let id = self.object_id(target, span)?;
        let key = self.heap.to_js_string(key);
        if self
            .objects
            .is_array(id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            if key == "length" {
                return Ok(true);
            }
            if let Ok(index) = key.parse::<u32>() {
                return self
                    .objects
                    .array_has(id, index)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span));
            }
        }
        let mut shape = self
            .objects
            .shape(id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let key = PropertyKey::from_string(key);
        loop {
            if self
                .shapes
                .lookup(shape, &key)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                .is_some()
            {
                return Ok(true);
            }
            let prototype = self
                .shapes
                .get(shape)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                .prototype;
            let Some(prototype) = prototype else {
                return Ok(false);
            };
            shape = self
                .objects
                .shape(prototype)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        }
    }
}
