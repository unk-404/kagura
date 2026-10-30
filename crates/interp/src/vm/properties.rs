use kagura_object::{ObjectId, PropertyAttributes, PropertyKey, PropertyLocation};
use kagura_support::Span;
use kagura_value::Value;

use crate::error::RuntimeError;

use super::{Interpreter, PropertyFeedback};

impl<'a> Interpreter<'a> {
    pub(super) fn get_own_data_property(
        &mut self,
        object: Value,
        key: &str,
    ) -> Result<Option<Value>, RuntimeError> {
        let span = Span::empty(0);
        let id = self.object_id(object, span)?;
        let shape = self
            .objects
            .shape(id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let Some(descriptor) = self
            .shapes
            .lookup(shape, &PropertyKey::from_string(key))
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        else {
            return Ok(None);
        };
        let value = match descriptor.location {
            PropertyLocation::InObject(slot) => self
                .objects
                .get_slot(id, slot)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?,
            PropertyLocation::PropertyArray(index) => self
                .objects
                .get_property_array(id, index)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?,
        };
        Ok(Some(value))
    }

    pub(super) fn get_property_ic(
        &mut self,
        object: Value,
        function_index: usize,
        site: u32,
        key: &str,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        if !object.is_object() || key == "length" {
            self.property_ic_misses = self.property_ic_misses.saturating_add(1);
            return self.get_property(object, key, span);
        }
        let id = self.object_id(object, span)?;
        let shape = self
            .objects
            .shape(id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if let Some(PropertyFeedback::OwnData {
            shape: cached_shape,
            location,
            ..
        }) = self
            .property_feedback
            .get(function_index)
            .and_then(|sites| sites.get(site as usize))
            .copied()
            && cached_shape == shape
        {
            self.property_ic_hits = self.property_ic_hits.saturating_add(1);
            return self.load_property_location(id, location, span);
        }
        self.property_ic_misses = self.property_ic_misses.saturating_add(1);
        let value = self.get_property(object, key, span)?;
        let descriptor = self
            .shapes
            .lookup(shape, &PropertyKey::from_string(key))
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if let Some(slot) = self
            .property_feedback
            .get_mut(function_index)
            .and_then(|sites| sites.get_mut(site as usize))
        {
            *slot = match descriptor {
                Some(descriptor) => PropertyFeedback::OwnData {
                    shape,
                    location: descriptor.location,
                    writable: descriptor.attributes.writable,
                },
                None => PropertyFeedback::Empty,
            };
        }
        Ok(value)
    }

    pub(super) fn set_property_ic(
        &mut self,
        object: Value,
        function_index: usize,
        site: u32,
        key: &str,
        value: Value,
        span: Span,
    ) -> Result<(), RuntimeError> {
        if !object.is_object() || key == "length" {
            self.property_ic_misses = self.property_ic_misses.saturating_add(1);
            return self.set_property(object, key, value, span);
        }
        let id = self.object_id(object, span)?;
        if self
            .objects
            .is_frozen(id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            self.property_ic_misses = self.property_ic_misses.saturating_add(1);
            return self.set_property(object, key, value, span);
        }
        let shape = self
            .objects
            .shape(id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if let Some(PropertyFeedback::OwnData {
            shape: cached_shape,
            location,
            writable: true,
        }) = self
            .property_feedback
            .get(function_index)
            .and_then(|sites| sites.get(site as usize))
            .copied()
            && cached_shape == shape
        {
            self.property_ic_hits = self.property_ic_hits.saturating_add(1);
            return self.store_property_location(id, location, value, span);
        }
        self.property_ic_misses = self.property_ic_misses.saturating_add(1);
        self.set_property(object, key, value, span)?;
        let shape = self
            .objects
            .shape(id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let descriptor = self
            .shapes
            .lookup(shape, &PropertyKey::from_string(key))
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if let Some(slot) = self
            .property_feedback
            .get_mut(function_index)
            .and_then(|sites| sites.get_mut(site as usize))
        {
            *slot = match descriptor {
                Some(descriptor) if descriptor.attributes.writable => PropertyFeedback::OwnData {
                    shape,
                    location: descriptor.location,
                    writable: true,
                },
                _ => PropertyFeedback::Empty,
            };
        }
        Ok(())
    }

    fn load_property_location(
        &mut self,
        object: ObjectId,
        location: PropertyLocation,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        match location {
            PropertyLocation::InObject(slot) => self
                .objects
                .get_slot(object, slot)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span)),
            PropertyLocation::PropertyArray(index) => self
                .objects
                .get_property_array(object, index)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span)),
        }
    }

    fn store_property_location(
        &mut self,
        object: ObjectId,
        location: PropertyLocation,
        value: Value,
        span: Span,
    ) -> Result<(), RuntimeError> {
        match location {
            PropertyLocation::InObject(slot) => self
                .objects
                .set_slot(object, slot, value)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span)),
            PropertyLocation::PropertyArray(index) => self
                .objects
                .set_property_array(object, index, value)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span)),
        }
    }

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
        if let Some(wrapper) = self.get_own_data_property(object, "__wrapper")? {
            let wrapper = self.heap.as_str(wrapper).unwrap_or_default().to_owned();
            if wrapper == "string" {
                let primitive = self
                    .get_own_data_property(object, "__primitive")?
                    .and_then(|value| self.heap.as_str(value).map(str::to_owned));
                if let Some(text) = primitive {
                    if key == "length" {
                        return Ok(self.heap.number(text.encode_utf16().count() as f64));
                    }
                    if let Ok(index) = key.parse::<u32>()
                        && index.to_string() == key
                    {
                        let value = text
                            .encode_utf16()
                            .nth(index as usize)
                            .and_then(|unit| String::from_utf16(&[unit]).ok())
                            .map(|unit| self.heap.string(&unit))
                            .unwrap_or_else(Value::undefined);
                        return Ok(value);
                    }
                }
            }
        }
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
        if self
            .objects
            .is_frozen(id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Ok(());
        }
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
        let descriptor = self
            .shapes
            .lookup(shape, &key)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if let Some(descriptor) = descriptor {
            if !descriptor.attributes.writable {
                return Ok(());
            }
        } else if self
            .shapes
            .is_non_extensible(shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Ok(());
        }
        let existing = descriptor.map(|descriptor| descriptor.location);
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

    pub(super) fn delete_property(
        &mut self,
        object: Value,
        key: &str,
        span: Span,
    ) -> Result<bool, RuntimeError> {
        let id = self.object_id(object, span)?;
        if self
            .objects
            .is_frozen(id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            || self
                .objects
                .is_sealed(id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Ok(false);
        }
        let shape = self
            .objects
            .shape(id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let property = PropertyKey::from_string(key);
        let Some(descriptor) = self
            .shapes
            .lookup(shape, &property)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        else {
            return Ok(true);
        };
        if !descriptor.attributes.configurable {
            return Ok(false);
        }
        let shape = self
            .shapes
            .remove_property(shape, &property)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        self.objects
            .set_shape(id, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        Ok(true)
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

    pub(super) fn object_rest(
        &mut self,
        source: Value,
        excluded: &[String],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let shape = self.object_shape(span)?;
        let result = self
            .objects
            .allocate(shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let result = Value::object(result.index());
        let Some(source_id) = source.object_offset().map(kagura_object::ObjectId::new) else {
            return Ok(result);
        };
        let source_shape = self
            .objects
            .shape(source_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let keys = kagura_object::own_property_keys(self.shapes, source_shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        for key in keys {
            let enumerable = self
                .shapes
                .lookup(source_shape, &key)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                .is_some_and(|descriptor| descriptor.attributes.enumerable);
            if !enumerable {
                continue;
            }
            let key_string = match &key {
                PropertyKey::Index(index) => index.to_string(),
                PropertyKey::String(key) => key.clone(),
                PropertyKey::Symbol(_) => continue,
            };
            if excluded.iter().any(|excluded| excluded == &key_string) {
                continue;
            }
            if let Some(value) = self
                .get_own_data_property(source, &key_string)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            {
                self.set_property(result, &key_string, value, span)?;
            }
        }
        Ok(result)
    }

    pub(super) fn array_rest(
        &mut self,
        source: Value,
        start: u32,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let shape = self.array_shape(span)?;
        let result = self
            .objects
            .allocate_array(shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let Some(source_id) = source.object_offset().map(kagura_object::ObjectId::new) else {
            return Ok(Value::object(result.index()));
        };
        if !self
            .objects
            .is_array(source_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Ok(Value::object(result.index()));
        }
        let length = self
            .objects
            .array_length(source_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        for index in start..length {
            if self
                .objects
                .array_has(source_id, index)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            {
                let value = self
                    .objects
                    .array_get(source_id, index)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                self.objects
                    .array_push(result, value)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            }
        }
        Ok(Value::object(result.index()))
    }

    pub(super) fn spread_array_into(
        &mut self,
        target: Value,
        source: Value,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let target_id = target
            .object_offset()
            .map(kagura_object::ObjectId::new)
            .ok_or_else(|| {
                RuntimeError::type_error("array spread target is not an object", span)
            })?;
        let Some(source_id) = source.object_offset().map(kagura_object::ObjectId::new) else {
            return Err(RuntimeError::type_error(
                "spread value is not iterable",
                span,
            ));
        };
        if !self
            .objects
            .is_array(source_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Err(RuntimeError::type_error(
                "spread value is not iterable",
                span,
            ));
        }
        let length = self
            .objects
            .array_length(source_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        for index in 0..length {
            if !self
                .objects
                .array_has(source_id, index)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            {
                continue;
            }
            let value = self
                .objects
                .array_get(source_id, index)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            self.objects
                .array_push(target_id, value)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        }
        Ok(())
    }

    pub(super) fn copy_spread_properties(
        &mut self,
        target: Value,
        source: Value,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let Some(source_id) = source.object_offset().map(kagura_object::ObjectId::new) else {
            return Ok(());
        };
        let shape = self
            .objects
            .shape(source_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let keys = kagura_object::own_property_keys(self.shapes, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        for key in keys {
            let enumerable = self
                .shapes
                .lookup(shape, &key)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                .is_some_and(|descriptor| descriptor.attributes.enumerable);
            if !enumerable {
                continue;
            }
            let key_string = match &key {
                PropertyKey::Index(index) => index.to_string(),
                PropertyKey::String(key) => key.clone(),
                PropertyKey::Symbol(_) => continue,
            };
            if let Some(value) = self
                .get_own_data_property(source, &key_string)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            {
                self.set_property(target, &key_string, value, span)?;
            }
        }
        Ok(())
    }
}
