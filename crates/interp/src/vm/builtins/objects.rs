use kagura_object::{
    ObjectId, PropertyAttributes, PropertyKey, PropertyLocation, own_property_keys,
};
use kagura_support::Span;
use kagura_value::Value;

use crate::error::RuntimeError;

use super::Interpreter;

impl<'a> Interpreter<'a> {
    pub(crate) fn same_value(&self, left: Value, right: Value) -> bool {
        if left.is_number() || right.is_number() {
            let (Some(left), Some(right)) = (self.heap.as_number(left), self.heap.as_number(right))
            else {
                return false;
            };
            if left.is_nan() && right.is_nan() {
                return true;
            }
            if left == 0.0 && right == 0.0 {
                return left.is_sign_negative() == right.is_sign_negative();
            }
            return left == right;
        }
        if let (Some(left), Some(right)) = (left.as_boolean(), right.as_boolean()) {
            return left == right;
        }
        self.heap.strict_eq(left, right)
    }

    pub(crate) fn assign_property(
        &mut self,
        target: Value,
        key: &str,
        value: Value,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let target_id = self.object_id(target, span)?;
        if self
            .objects
            .is_array(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            && let Ok(index) = key.parse::<u32>()
        {
            return self
                .objects
                .array_set(target_id, index, value)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span));
        }
        self.set_property(target, key, value, span)
    }

    pub(crate) fn has_own_property(
        &mut self,
        target: Value,
        key: &str,
        span: Span,
    ) -> Result<bool, RuntimeError> {
        let object = self.object_id(target, span)?;
        let is_array = self
            .objects
            .is_array(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if is_array && key == "length" {
            return Ok(true);
        }
        if is_array && let Ok(index) = key.parse::<u32>() {
            return self
                .objects
                .array_has(object, index)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span));
        }
        let shape = self
            .objects
            .shape(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        Ok(self
            .shapes
            .lookup(shape, &PropertyKey::from_string(key))
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            .is_some())
    }

    pub(crate) fn object_to_string_tag(
        &mut self,
        value: Value,
        span: Span,
    ) -> Result<&'static str, RuntimeError> {
        if value.is_undefined() || value.is_hole() {
            Ok("Undefined")
        } else if value.is_null() {
            Ok("Null")
        } else if value.as_boolean().is_some() {
            Ok("Boolean")
        } else if value.is_number() {
            Ok("Number")
        } else if value.is_string() {
            Ok("String")
        } else if let Some(offset) = value.object_offset() {
            let id = ObjectId::new(offset);
            if self
                .objects
                .is_array(id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            {
                Ok("Array")
            } else if self
                .objects
                .is_function(id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            {
                Ok("Function")
            } else {
                Ok("Object")
            }
        } else {
            Ok("Object")
        }
    }

    pub(crate) fn assign_source(
        &mut self,
        target: Value,
        source: Value,
        span: Span,
    ) -> Result<(), RuntimeError> {
        if let Some(text) = self.heap.as_str(source).map(|text| text.to_owned()) {
            for (index, ch) in text.chars().enumerate() {
                let value = self.heap.string(&ch.to_string());
                self.assign_property(target, &index.to_string(), value, span)?;
            }
            return Ok(());
        }
        let Some(source_id) = source.object_offset().map(ObjectId::new) else {
            return Ok(());
        };
        if self
            .objects
            .is_array(source_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
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
                self.assign_property(target, &index.to_string(), value, span)?;
            }
            return Ok(());
        }
        let shape = self
            .objects
            .shape(source_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let keys = own_property_keys(self.shapes, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        for key in keys {
            let descriptor = self
                .shapes
                .lookup(shape, &key)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            if !descriptor.is_some_and(|descriptor| descriptor.attributes.enumerable) {
                continue;
            }
            let key_string = match &key {
                PropertyKey::Index(index) => index.to_string(),
                PropertyKey::String(key) => key.clone(),
                PropertyKey::Symbol(_) => continue,
            };
            let value = self.get_property(source, &key_string, span)?;
            self.assign_property(target, &key_string, value, span)?;
        }
        Ok(())
    }

    pub(crate) fn make_descriptor(
        &mut self,
        value: Value,
        writable: bool,
        enumerable: bool,
        configurable: bool,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let descriptor = self
            .objects
            .allocate(self.shapes.root())
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let object = Value::object(descriptor.index());
        self.set_property(object, "value", value, span)?;
        self.set_property(object, "writable", Value::boolean(writable), span)?;
        self.set_property(object, "enumerable", Value::boolean(enumerable), span)?;
        self.set_property(object, "configurable", Value::boolean(configurable), span)?;
        Ok(object)
    }

    pub(crate) fn own_property_descriptor(
        &mut self,
        target: Value,
        key: &str,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let target_id = self.object_id(target, span)?;
        if self
            .objects
            .is_array(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            if key == "length" {
                let length = self
                    .objects
                    .array_length(target_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let length = self.heap.number(length as f64);
                return self.make_descriptor(length, true, false, false, span);
            }
            if let Ok(index) = key.parse::<u32>()
                && self
                    .objects
                    .array_has(target_id, index)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            {
                let value = self
                    .objects
                    .array_get(target_id, index)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                return self.make_descriptor(value, true, true, true, span);
            }
        }
        let shape = self
            .objects
            .shape(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let property = PropertyKey::from_string(key);
        let Some(descriptor) = self
            .shapes
            .lookup(shape, &property)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        else {
            return Ok(Value::undefined());
        };
        let attributes = descriptor.attributes;
        let value = self.get_property(target, key, span)?;
        self.make_descriptor(
            value,
            attributes.writable,
            attributes.enumerable,
            attributes.configurable,
            span,
        )
    }

    pub(crate) fn define_property(
        &mut self,
        target: Value,
        key: &str,
        descriptor: Value,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let target_id = self.object_id(target, span)?;
        self.object_id(descriptor, span)?;
        let writable = self.get_property(descriptor, "writable", span)?;
        let writable = self.heap.truthy(writable);
        let enumerable = self.get_property(descriptor, "enumerable", span)?;
        let enumerable = self.heap.truthy(enumerable);
        let configurable = self.get_property(descriptor, "configurable", span)?;
        let configurable = self.heap.truthy(configurable);
        let value = self.get_property(descriptor, "value", span)?;
        let shape = self
            .objects
            .shape(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let property = PropertyKey::from_string(key);
        let existing_descriptor = self
            .shapes
            .lookup(shape, &property)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            .map(|descriptor| descriptor.attributes);
        let existing = existing_descriptor.is_some();
        let attributes = PropertyAttributes {
            writable,
            enumerable,
            configurable,
        };
        if let Some(old_attributes) = existing_descriptor {
            let old_value = self.get_property(target, key, span)?;
            if !old_attributes.configurable
                && (old_attributes != attributes || !self.same_value(old_value, value))
            {
                return Err(RuntimeError::type_error(
                    "cannot redefine a non-configurable property",
                    span,
                ));
            }
        }
        let shape = if existing {
            self.shapes
                .redescribe(shape, &property, attributes)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        } else {
            self.shapes
                .add_property(shape, property.clone(), attributes)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        };
        self.objects
            .set_shape(target_id, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let location = self
            .shapes
            .lookup(shape, &property)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            .expect("defined property has descriptor")
            .location;
        match location {
            PropertyLocation::InObject(slot) => self
                .objects
                .set_slot(target_id, slot, value)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span)),
            PropertyLocation::PropertyArray(index) => self
                .objects
                .set_property_array(target_id, index, value)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span)),
        }
    }

    pub(crate) fn set_non_enumerable_readonly_property(
        &mut self,
        target: Value,
        key: &str,
        value: Value,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let descriptor = self
            .objects
            .allocate(self.shapes.root())
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let descriptor = Value::object(descriptor.index());
        self.set_property(descriptor, "value", value, span)?;
        self.set_property(descriptor, "writable", Value::boolean(false), span)?;
        self.set_property(descriptor, "enumerable", Value::boolean(false), span)?;
        self.set_property(descriptor, "configurable", Value::boolean(true), span)?;
        self.define_property(target, key, descriptor, span)
    }

    pub(crate) fn set_non_enumerable_property(
        &mut self,
        target: Value,
        key: &str,
        value: Value,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let descriptor = self
            .objects
            .allocate(self.shapes.root())
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let descriptor = Value::object(descriptor.index());
        self.set_property(descriptor, "value", value, span)?;
        self.set_property(descriptor, "writable", Value::boolean(true), span)?;
        self.set_property(descriptor, "enumerable", Value::boolean(false), span)?;
        self.set_property(descriptor, "configurable", Value::boolean(true), span)?;
        self.define_property(target, key, descriptor, span)
    }

    pub(crate) fn get_own_property_names(
        &mut self,
        target: Value,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let target_id = self.object_id(target, span)?;
        let array = self
            .objects
            .allocate_array(self.shapes.root())
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let push = |heap: &mut kagura_value::Heap,
                    objects: &mut kagura_object::ObjectHeap,
                    text: &str|
         -> Result<(), RuntimeError> {
            let value = heap.string(text);
            objects
                .array_push(array, value)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))
        };
        if self
            .objects
            .is_array(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            let length = self
                .objects
                .array_length(target_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            for index in 0..length {
                if self
                    .objects
                    .array_has(target_id, index)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                {
                    push(self.heap, self.objects, &index.to_string())?;
                }
            }
            push(self.heap, self.objects, "length")?;
        }
        let shape = self
            .objects
            .shape(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let keys = own_property_keys(self.shapes, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        for key in keys {
            let key_string = match &key {
                PropertyKey::Index(index) => index.to_string(),
                PropertyKey::String(key) => key.clone(),
                PropertyKey::Symbol(_) => continue,
            };
            push(self.heap, self.objects, &key_string)?;
        }
        Ok(Value::object(array.index()))
    }

    pub(crate) fn define_properties(
        &mut self,
        target: Value,
        properties: Value,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let properties_id = self.object_id(properties, span)?;
        let shape = self
            .objects
            .shape(properties_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let keys = own_property_keys(self.shapes, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        for key in keys {
            let descriptor = self
                .shapes
                .lookup(shape, &key)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            if !descriptor.is_some_and(|descriptor| descriptor.attributes.enumerable) {
                continue;
            }
            let key_string = match &key {
                PropertyKey::Index(index) => index.to_string(),
                PropertyKey::String(key) => key.clone(),
                PropertyKey::Symbol(_) => continue,
            };
            let descriptor = self.get_property(properties, &key_string, span)?;
            self.define_property(target, &key_string, descriptor, span)?;
        }
        Ok(())
    }

    pub(crate) fn instance_of(
        &mut self,
        value: Value,
        constructor: Value,
        span: Span,
    ) -> Result<bool, RuntimeError> {
        let Some(value_id) = value.object_offset().map(ObjectId::new) else {
            return Ok(false);
        };
        let constructor_id = self.object_id(constructor, span)?;
        if !self
            .objects
            .is_function(constructor_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Err(RuntimeError::type_error(
                "right-hand side of instanceof is not callable",
                span,
            ));
        }
        let name = self.get_property(constructor, "name", span)?;
        let name = self.heap.as_str(name).unwrap_or_default().to_owned();
        if name == "Object" {
            return Ok(true);
        }
        if name == "Array" {
            return self
                .objects
                .is_array(value_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span));
        }
        if name == "Function" {
            return self
                .objects
                .is_function(value_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span));
        }
        if matches!(
            name.as_str(),
            "Error"
                | "TypeError"
                | "RangeError"
                | "ReferenceError"
                | "SyntaxError"
                | "EvalError"
                | "URIError"
                | "Test262Error"
        ) {
            let actual = self.get_property(value, "name", span)?;
            let actual = self.heap.as_str(actual).unwrap_or_default();
            if name == "Error" {
                return Ok(matches!(
                    actual,
                    "Error"
                        | "TypeError"
                        | "RangeError"
                        | "ReferenceError"
                        | "SyntaxError"
                        | "EvalError"
                        | "URIError"
                ));
            }
            return Ok(actual == name);
        }
        let prototype = self.get_property(constructor, "prototype", span)?;
        let Some(prototype_id) = prototype.object_offset().map(ObjectId::new) else {
            return Ok(false);
        };
        let mut current = Some(value_id);
        while let Some(current_id) = current {
            if current_id == prototype_id {
                return Ok(true);
            }
            let shape = self
                .objects
                .shape(current_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            current = self
                .shapes
                .get(shape)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                .prototype;
        }
        Ok(false)
    }

    pub(crate) fn for_in_keys_inner(
        &mut self,
        value: Value,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let array = self
            .objects
            .allocate_array(self.shapes.root())
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let mut seen = kagura_support::FastHashSet::default();
        let mut push = |this: &mut Self, key: String| -> Result<(), RuntimeError> {
            if seen.insert(key.clone()) {
                let value = this.heap.string(&key);
                this.objects
                    .array_push(array, value)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            }
            Ok(())
        };
        if let Some(text) = self.heap.as_str(value).map(str::to_owned) {
            for (index, _) in text.chars().enumerate() {
                push(self, index.to_string())?;
            }
            return Ok(Value::object(array.index()));
        }
        let Some(mut id) = value.object_offset().map(ObjectId::new) else {
            return Ok(Value::object(array.index()));
        };
        let mut visited = kagura_support::FastHashSet::default();
        while visited.insert(id.index()) {
            if self
                .objects
                .is_array(id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            {
                let length = self
                    .objects
                    .array_length(id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                for index in 0..length {
                    if self
                        .objects
                        .array_has(id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        push(self, index.to_string())?;
                    }
                }
            }
            let shape = self
                .objects
                .shape(id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let keys = own_property_keys(self.shapes, shape)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            for key in keys {
                let descriptor = self
                    .shapes
                    .lookup(shape, &key)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                if !descriptor.is_some_and(|descriptor| descriptor.attributes.enumerable) {
                    continue;
                }
                let key = match key {
                    PropertyKey::String(key) => key,
                    PropertyKey::Index(index) => index.to_string(),
                    PropertyKey::Symbol(_) => continue,
                };
                push(self, key)?;
            }
            let Some(prototype) = self
                .shapes
                .get(shape)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                .prototype
            else {
                break;
            };
            id = prototype;
        }
        Ok(Value::object(array.index()))
    }

    pub(crate) fn for_in_keys(&mut self, value: Value, span: Span) -> Result<Value, RuntimeError> {
        self.for_in_keys_inner(value, span)
    }

    pub(crate) fn is_sealed(&mut self, target: Value, span: Span) -> Result<bool, RuntimeError> {
        let target_id = self.object_id(target, span)?;
        if self
            .objects
            .is_sealed(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Ok(true);
        }
        let shape = self
            .objects
            .shape(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if !self
            .shapes
            .is_non_extensible(shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Ok(false);
        }
        let keys = own_property_keys(self.shapes, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        for key in keys {
            let Some(descriptor) = self
                .shapes
                .lookup(shape, &key)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            else {
                continue;
            };
            if descriptor.attributes.configurable {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(crate) fn seal_object(&mut self, target: Value, span: Span) -> Result<(), RuntimeError> {
        let target_id = self.object_id(target, span)?;
        if self
            .objects
            .is_array(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            self.objects
                .mark_sealed(target_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            return Ok(());
        }
        let mut shape = self
            .objects
            .shape(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let keys = own_property_keys(self.shapes, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        for key in keys {
            let attributes = self
                .shapes
                .lookup(shape, &key)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                .map(|descriptor| descriptor.attributes)
                .ok_or_else(|| RuntimeError::runtime("missing property", span))?;
            shape = self
                .shapes
                .redescribe(
                    shape,
                    &key,
                    PropertyAttributes {
                        writable: attributes.writable,
                        enumerable: attributes.enumerable,
                        configurable: false,
                    },
                )
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            self.objects
                .set_shape(target_id, shape)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        }
        shape = self
            .shapes
            .set_non_extensible(shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        self.objects
            .set_shape(target_id, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        self.objects
            .mark_sealed(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))
    }

    pub(crate) fn get_own_property_descriptors(
        &mut self,
        target: Value,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let names = self.get_own_property_names(target, span)?;
        let names_id = self.object_id(names, span)?;
        let length = self
            .objects
            .array_length(names_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let result = self
            .objects
            .allocate(self.shapes.root())
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let result = Value::object(result.index());
        for index in 0..length {
            let key = self
                .objects
                .array_get(names_id, index)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let key = self.heap.as_str(key).unwrap_or_default().to_owned();
            let descriptor = self.own_property_descriptor(target, &key, span)?;
            self.set_property(result, &key, descriptor, span)?;
        }
        Ok(result)
    }

    pub(crate) fn is_extensible(
        &mut self,
        target: Value,
        span: Span,
    ) -> Result<bool, RuntimeError> {
        let target_id = self.object_id(target, span)?;
        let shape = self
            .objects
            .shape(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        self.shapes
            .is_non_extensible(shape)
            .map(|non_extensible| !non_extensible)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))
    }

    pub(crate) fn prevent_extensions(
        &mut self,
        target: Value,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let target_id = self.object_id(target, span)?;
        let shape = self
            .objects
            .shape(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let shape = self
            .shapes
            .set_non_extensible(shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        self.objects
            .set_shape(target_id, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))
    }

    pub(crate) fn is_frozen(&mut self, target: Value, span: Span) -> Result<bool, RuntimeError> {
        let target_id = self.object_id(target, span)?;
        if self
            .objects
            .is_frozen(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Ok(true);
        }
        let shape = self
            .objects
            .shape(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if !self
            .shapes
            .is_non_extensible(shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Ok(false);
        }
        let keys = own_property_keys(self.shapes, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        for key in keys {
            let Some(descriptor) = self
                .shapes
                .lookup(shape, &key)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            else {
                continue;
            };
            if descriptor.attributes.writable || descriptor.attributes.configurable {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(crate) fn freeze_object(&mut self, target: Value, span: Span) -> Result<(), RuntimeError> {
        let target_id = self.object_id(target, span)?;
        if self
            .objects
            .is_array(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return self
                .objects
                .mark_frozen(target_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span));
        }
        let mut shape = self
            .objects
            .shape(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let keys = own_property_keys(self.shapes, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        for key in keys {
            let attributes = self
                .shapes
                .lookup(shape, &key)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                .map(|descriptor| descriptor.attributes)
                .ok_or_else(|| RuntimeError::runtime("missing property", span))?;
            shape = self
                .shapes
                .redescribe(
                    shape,
                    &key,
                    PropertyAttributes {
                        writable: false,
                        enumerable: attributes.enumerable,
                        configurable: false,
                    },
                )
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            self.objects
                .set_shape(target_id, shape)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        }
        shape = self
            .shapes
            .set_non_extensible(shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        self.objects
            .set_shape(target_id, shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        self.objects
            .mark_frozen(target_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))
    }
}
