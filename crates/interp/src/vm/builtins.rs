use kagura_object::{
    ObjectId, PropertyAttributes, PropertyKey, PropertyLocation, own_property_keys,
};
use kagura_support::Span;
use kagura_value::Value;

use crate::error::RuntimeError;

use super::Interpreter;

impl<'a> Interpreter<'a> {
    pub(super) fn call_global(
        &mut self,
        path: &str,
        args: &[Value],
        span: Span,
    ) -> Result<Option<Value>, RuntimeError> {
        if path == "Array.isArray" {
            let array = args.first().copied().is_some_and(|value| {
                value.object_offset().is_some_and(|offset| {
                    self.objects
                        .is_array(ObjectId::new(offset))
                        .unwrap_or(false)
                })
            });
            return Ok(Some(Value::boolean(array)));
        }
        if path == "Object.is" {
            let left = args.first().copied().unwrap_or_else(Value::undefined);
            let right = args.get(1).copied().unwrap_or_else(Value::undefined);
            return Ok(Some(Value::boolean(self.same_value(left, right))));
        }
        if path == "Object.assign" {
            let Some(target) = args.first().copied() else {
                return Err(RuntimeError::type_error(
                    "Object.assign requires a target",
                    span,
                ));
            };
            self.object_id(target, span)?;
            for source in args.iter().skip(1) {
                if source.is_null() || source.is_undefined() {
                    continue;
                }
                self.assign_source(target, *source, span)?;
            }
            return Ok(Some(target));
        }
        if path == "Object.defineProperty" {
            let target = args.first().copied().ok_or_else(|| {
                RuntimeError::type_error("Object.defineProperty requires a target", span)
            })?;
            let key = self
                .heap
                .to_js_string(args.get(1).copied().unwrap_or_else(Value::undefined));
            let descriptor = args.get(2).copied().ok_or_else(|| {
                RuntimeError::type_error("Object.defineProperty requires a descriptor", span)
            })?;
            self.define_property(target, &key, descriptor, span)?;
            return Ok(Some(target));
        }
        if path == "Object.getOwnPropertyDescriptor" {
            let target = args.first().copied().ok_or_else(|| {
                RuntimeError::type_error("Object.getOwnPropertyDescriptor requires a target", span)
            })?;
            let key = self
                .heap
                .to_js_string(args.get(1).copied().unwrap_or_else(Value::undefined));
            return self.own_property_descriptor(target, &key, span).map(Some);
        }
        if path == "Object.freeze" {
            let target = args
                .first()
                .copied()
                .ok_or_else(|| RuntimeError::type_error("Object.freeze requires a target", span))?;
            self.freeze_object(target, span)?;
            return Ok(Some(target));
        }
        if path == "Object.isFrozen" {
            let target = args.first().copied().ok_or_else(|| {
                RuntimeError::type_error("Object.isFrozen requires a target", span)
            })?;
            return self
                .is_frozen(target, span)
                .map(|frozen| Some(Value::boolean(frozen)));
        }
        if path == "Object.seal" {
            let target = args
                .first()
                .copied()
                .ok_or_else(|| RuntimeError::type_error("Object.seal requires a target", span))?;
            self.seal_object(target, span)?;
            return Ok(Some(target));
        }
        if path == "Object.isSealed" {
            let target = args.first().copied().ok_or_else(|| {
                RuntimeError::type_error("Object.isSealed requires a target", span)
            })?;
            return self
                .is_sealed(target, span)
                .map(|sealed| Some(Value::boolean(sealed)));
        }
        if path == "Object.create" {
            let Some(prototype) = args.first().copied() else {
                return Err(RuntimeError::type_error(
                    "Object.create requires a prototype",
                    span,
                ));
            };
            let prototype = if prototype.is_null() {
                None
            } else {
                Some(self.object_id(prototype, span)?)
            };
            let shape = self
                .shapes
                .with_prototype(self.shapes.root(), prototype)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let object = self
                .objects
                .allocate(shape)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            return Ok(Some(Value::object(object.index())));
        }
        if path == "Object.getPrototypeOf" {
            let Some(target) = args.first().copied() else {
                return Err(RuntimeError::type_error(
                    "Object.getPrototypeOf requires an object",
                    span,
                ));
            };
            let object = self.object_id(target, span)?;
            let shape = self
                .objects
                .shape(object)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let prototype = self
                .shapes
                .get(shape)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                .prototype;
            return Ok(Some(
                prototype
                    .map(|prototype| Value::object(prototype.index()))
                    .unwrap_or_else(Value::null),
            ));
        }
        if path == "Object.setPrototypeOf" {
            let target = args.first().copied().ok_or_else(|| {
                RuntimeError::type_error("Object.setPrototypeOf requires an object", span)
            })?;
            let prototype = args.get(1).copied().ok_or_else(|| {
                RuntimeError::type_error("Object.setPrototypeOf requires a prototype", span)
            })?;
            let object = self.object_id(target, span)?;
            let prototype = if prototype.is_null() {
                None
            } else {
                Some(self.object_id(prototype, span)?)
            };
            let shape = self
                .objects
                .shape(object)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let shape = self
                .shapes
                .with_prototype(shape, prototype)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            self.objects
                .set_shape(object, shape)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            return Ok(Some(target));
        }
        let mode = match path {
            "Object.keys" => 0,
            "Object.values" => 1,
            "Object.entries" => 2,
            _ => return Ok(None),
        };
        let Some(target) = args.first().copied() else {
            return Err(RuntimeError::type_error(
                "Object method requires an argument",
                span,
            ));
        };
        let mut pairs = Vec::new();
        if let Some(text) = self.heap.as_str(target).map(|text| text.to_owned()) {
            for (index, ch) in text.chars().enumerate() {
                pairs.push((index.to_string(), self.heap.string(&ch.to_string())));
            }
        } else {
            let object = self.object_id(target, span)?;
            if self
                .objects
                .is_array(object)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            {
                let length = self
                    .objects
                    .array_length(object)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                for index in 0..length {
                    if !self
                        .objects
                        .array_has(object, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        continue;
                    }
                    let value = self
                        .objects
                        .array_get(object, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    pairs.push((index.to_string(), value));
                }
            } else {
                let shape = self
                    .objects
                    .shape(object)
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
                    let value = self.get_property(target, &key_string, span)?;
                    pairs.push((key_string, value));
                }
            }
        }
        let result = self
            .objects
            .allocate_array(self.shapes.root())
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        for (key, value) in pairs {
            match mode {
                0 => {
                    let key = self.heap.string(&key);
                    self.objects
                        .array_push(result, key)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                1 => {
                    self.objects
                        .array_push(result, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                _ => {
                    let entry = self
                        .objects
                        .allocate_array(self.shapes.root())
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let key = self.heap.string(&key);
                    self.objects
                        .array_push(entry, key)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    self.objects
                        .array_push(entry, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    self.objects
                        .array_push(result, Value::object(entry.index()))
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
            }
        }
        Ok(Some(Value::object(result.index())))
    }

    pub(super) fn invoke_bound(
        &mut self,
        callee: Value,
        args: &[Value],
        caller_roots: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let mut current = callee;
        let mut bound_this = Value::undefined();
        let mut levels = Vec::new();
        loop {
            let object = self.object_id(current, span)?;
            if !self
                .objects
                .is_bound(object)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            {
                break;
            }
            let (this_value, arguments, target) = self
                .objects
                .bound_data(object)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            bound_this = this_value;
            let arguments = self.object_id(arguments, span)?;
            let length = self
                .objects
                .array_length(arguments)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let mut level = Vec::with_capacity(length as usize);
            for index in 0..length {
                level.push(
                    self.objects
                        .array_get(arguments, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?,
                );
            }
            levels.push(level);
            current = target;
        }
        let object = self.object_id(current, span)?;
        let (function, env_value) = self
            .objects
            .function_data(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let closure_env = env_value.object_offset().map(ObjectId::new);
        let receiver = if self
            .module
            .functions
            .get(function as usize)
            .is_some_and(|proto| proto.name == "<arrow>")
        {
            Some(
                self.objects
                    .function_this(object)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?,
            )
        } else {
            Some(bound_this)
        };
        let mut all_args = Vec::new();
        for level in levels.into_iter().rev() {
            all_args.extend(level);
        }
        all_args.extend_from_slice(args);
        self.call(
            function as usize,
            &all_args,
            caller_roots,
            closure_env,
            Some(current),
            receiver,
        )
    }

    pub(super) fn invoke_callable(
        &mut self,
        callee: Value,
        args: &[Value],
        caller_roots: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let object = self.object_id(callee, span)?;
        if !self
            .objects
            .is_function(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Err(RuntimeError::type_error("value is not callable", span));
        }
        if self
            .objects
            .is_bound(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return self.invoke_bound(callee, args, caller_roots, span);
        }
        let (function, env_value) = self
            .objects
            .function_data(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let closure_env = env_value.object_offset().map(ObjectId::new);
        let receiver = self.callee_receiver(callee, function, None, span)?;
        self.call(
            function as usize,
            args,
            caller_roots,
            closure_env,
            Some(callee),
            receiver,
        )
    }

    fn freeze_object(&mut self, target: Value, span: Span) -> Result<(), RuntimeError> {
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

    fn is_frozen(&mut self, target: Value, span: Span) -> Result<bool, RuntimeError> {
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

    fn seal_object(&mut self, target: Value, span: Span) -> Result<(), RuntimeError> {
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

    fn is_sealed(&mut self, target: Value, span: Span) -> Result<bool, RuntimeError> {
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

    fn define_property(
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
        let existing = self
            .shapes
            .lookup(shape, &property)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            .is_some();
        let attributes = PropertyAttributes {
            writable,
            enumerable,
            configurable,
        };
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

    fn own_property_descriptor(
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

    fn make_descriptor(
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

    fn assign_source(
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

    fn assign_property(
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

    fn same_value(&self, left: Value, right: Value) -> bool {
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

    fn callee_receiver(
        &self,
        callee: Value,
        function_index: u32,
        receiver: Option<Value>,
        span: Span,
    ) -> Result<Option<Value>, RuntimeError> {
        if self
            .module
            .functions
            .get(function_index as usize)
            .is_some_and(|proto| proto.name == "<arrow>")
        {
            let object = self.object_id(callee, span)?;
            return self
                .objects
                .function_this(object)
                .map(Some)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span));
        }
        Ok(receiver)
    }

    pub(super) fn invoke_method(
        &mut self,
        callee: Value,
        args: &[Value],
        caller_roots: &[Value],
        receiver: Value,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let object = self.object_id(callee, span)?;
        if !self
            .objects
            .is_function(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Err(RuntimeError::type_error("value is not callable", span));
        }
        if self
            .objects
            .is_bound(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return self.invoke_bound(callee, args, caller_roots, span);
        }
        let (function, env_value) = self
            .objects
            .function_data(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let closure_env = env_value.object_offset().map(ObjectId::new);
        let receiver = self.callee_receiver(callee, function, Some(receiver), span)?;
        self.call(
            function as usize,
            args,
            caller_roots,
            closure_env,
            Some(callee),
            receiver,
        )
    }

    pub(super) fn call_builtin(
        &mut self,
        object: Value,
        key: &str,
        args: &[Value],
        span: Span,
        roots: &[Value],
    ) -> Result<Option<Value>, RuntimeError> {
        let arg_index = |args: &[Value], index: usize| {
            args.get(index)
                .and_then(|value| value.as_smi())
                .unwrap_or(0)
                .max(0) as usize
        };
        if object.is_string() {
            let text = self.heap.as_str(object).unwrap_or_default().to_owned();
            return Ok(Some(match key {
                "charAt" => text
                    .chars()
                    .nth(arg_index(args, 0))
                    .map(|ch| self.heap.string(&ch.to_string()))
                    .unwrap_or_else(Value::undefined),
                "charCodeAt" => text
                    .encode_utf16()
                    .nth(arg_index(args, 0))
                    .map(|unit| self.heap.number(unit as f64))
                    .unwrap_or_else(Value::undefined),
                "indexOf" => {
                    let needle = args
                        .first()
                        .map(|value| self.heap.to_js_string(*value))
                        .unwrap_or_default();
                    let from = arg_index(args, 1);
                    let found = text
                        .char_indices()
                        .enumerate()
                        .find(|(index, (byte, _))| {
                            *index >= from && text[*byte..].starts_with(&needle)
                        })
                        .map(|(index, _)| index as f64)
                        .unwrap_or(-1.0);
                    self.heap.number(found)
                }
                "includes" => {
                    let needle = args
                        .first()
                        .map(|value| self.heap.to_js_string(*value))
                        .unwrap_or_default();
                    Value::boolean(text.contains(&needle))
                }
                "slice" => {
                    let length = text.chars().count();
                    let start = arg_index(args, 0).min(length);
                    let end = args
                        .get(1)
                        .and_then(|value| value.as_smi())
                        .map(|value| value.max(0) as usize)
                        .unwrap_or(length)
                        .min(length)
                        .max(start);
                    let value: String = text.chars().skip(start).take(end - start).collect();
                    self.heap.string(&value)
                }
                "toUpperCase" => self.heap.string(&text.to_uppercase()),
                "toLowerCase" => self.heap.string(&text.to_lowercase()),
                "trim" => self.heap.string(text.trim()),
                "startsWith" => {
                    let needle = args
                        .first()
                        .map(|value| self.heap.to_js_string(*value))
                        .unwrap_or_default();
                    let position = arg_index(args, 1);
                    let suffix: String = text.chars().skip(position).collect();
                    Value::boolean(suffix.starts_with(&needle))
                }
                "endsWith" => {
                    let needle = args
                        .first()
                        .map(|value| self.heap.to_js_string(*value))
                        .unwrap_or_default();
                    let length = text.chars().count();
                    let end = args
                        .get(1)
                        .and_then(|value| value.as_smi())
                        .map(|value| value.clamp(0, length as i32) as usize)
                        .unwrap_or(length);
                    let prefix: String = text.chars().take(end).collect();
                    Value::boolean(prefix.ends_with(&needle))
                }
                "repeat" => {
                    let count = args
                        .first()
                        .and_then(|value| value.as_smi())
                        .unwrap_or(0)
                        .max(0) as usize;
                    if text.len().saturating_mul(count) > 1_000_000 {
                        return Err(RuntimeError::range("repeat result is too large", span));
                    }
                    self.heap.string(&text.repeat(count))
                }
                "substring" => {
                    let length = text.chars().count();
                    let start = arg_index(args, 0).min(length);
                    let end = args
                        .get(1)
                        .and_then(|value| value.as_smi())
                        .map(|value| value.clamp(0, length as i32) as usize)
                        .unwrap_or(length);
                    let (start, end) = if start <= end {
                        (start, end)
                    } else {
                        (end, start)
                    };
                    let value: String = text.chars().skip(start).take(end - start).collect();
                    self.heap.string(&value)
                }
                "split" => {
                    let limit = args
                        .get(1)
                        .and_then(|value| value.as_smi())
                        .map(|value| value.max(0) as usize);
                    let separator = if args.is_empty()
                        || args.first().is_some_and(|value| value.is_undefined())
                    {
                        None
                    } else {
                        Some(
                            args.first()
                                .map(|value| self.heap.to_js_string(*value))
                                .unwrap_or_default(),
                        )
                    };
                    let parts: Vec<String> = match separator {
                        None => vec![text.clone()],
                        Some(separator) if separator.is_empty() => {
                            text.chars().map(|ch| ch.to_string()).collect()
                        }
                        Some(separator) => text.split(&separator).map(str::to_owned).collect(),
                    };
                    let array = self
                        .objects
                        .allocate_array(self.shapes.root())
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    for part in parts.into_iter().take(limit.unwrap_or(usize::MAX)) {
                        let value = self.heap.string(&part);
                        self.objects
                            .array_push(array, value)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                    Value::object(array.index())
                }
                _ => return Ok(None),
            }));
        }
        let Ok(object_id) = self.object_id(object, span) else {
            return Ok(None);
        };
        let is_function = self
            .objects
            .is_function(object_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if is_function && key == "bind" {
            let receiver = args.first().copied().unwrap_or_else(Value::undefined);
            let array = self
                .objects
                .allocate_array(self.shapes.root())
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            for argument in args.get(1..).unwrap_or_default() {
                self.objects
                    .array_push(array, *argument)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            }
            let (function, env_value) = self
                .objects
                .function_data(object_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let bound = self
                .objects
                .allocate_bound_function(
                    self.shapes.root(),
                    function,
                    env_value,
                    receiver,
                    Value::object(array.index()),
                    object,
                )
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            return Ok(Some(Value::object(bound.index())));
        }
        if is_function && (key == "call" || key == "apply") {
            let receiver = args.first().copied().unwrap_or_else(Value::undefined);
            let rest = if key == "call" {
                args.get(1..).unwrap_or_default().to_vec()
            } else {
                let values = args.get(1).copied().unwrap_or_else(Value::undefined);
                if values.is_undefined() {
                    Vec::new()
                } else {
                    let array = self.object_id(values, span)?;
                    if !self
                        .objects
                        .is_array(array)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        return Err(RuntimeError::type_error(
                            "apply expects an array-like argument list",
                            span,
                        ));
                    }
                    let length = self
                        .objects
                        .array_length(array)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let mut values = Vec::with_capacity(length as usize);
                    for index in 0..length {
                        values.push(
                            self.objects
                                .array_get(array, index)
                                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?,
                        );
                    }
                    values
                }
            };
            return self
                .invoke_method(object, &rest, roots, receiver, span)
                .map(Some);
        }
        if key == "hasOwnProperty" {
            let key = self
                .heap
                .to_js_string(args.first().copied().unwrap_or_else(Value::undefined));
            let is_array = self
                .objects
                .is_array(object_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            if is_array && key == "length" {
                return Ok(Some(Value::boolean(true)));
            }
            if is_array && let Ok(index) = key.parse::<u32>() {
                let own = self
                    .objects
                    .array_has(object_id, index)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                return Ok(Some(Value::boolean(own)));
            }
            let shape = self
                .objects
                .shape(object_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let own = self
                .shapes
                .lookup(shape, &PropertyKey::from_string(&key))
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                .is_some();
            return Ok(Some(Value::boolean(own)));
        }
        if !self
            .objects
            .is_array(object_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Ok(None);
        }
        Ok(Some(match key {
            "push" => {
                let frozen = self
                    .objects
                    .is_frozen(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let sealed = self
                    .objects
                    .is_sealed(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                if !frozen && !sealed {
                    for value in args {
                        self.objects
                            .array_push(object_id, *value)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                }
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                self.heap.number(length as f64)
            }
            "pop" => {
                if self
                    .objects
                    .is_frozen(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    || self
                        .objects
                        .is_sealed(object_id)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                {
                    Value::undefined()
                } else {
                    self.objects
                        .array_pop(object_id)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                }
            }
            "join" => {
                let separator = args
                    .first()
                    .map(|value| self.heap.to_js_string(*value))
                    .unwrap_or_else(|| ",".into());
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let mut parts = Vec::with_capacity(length as usize);
                for index in 0..length {
                    let value = self
                        .objects
                        .array_get(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    if value.is_undefined() || value.is_null() {
                        parts.push(String::new());
                    } else {
                        parts.push(self.heap.to_js_string(value));
                    }
                }
                self.heap.string(&parts.join(&separator))
            }
            "indexOf" | "includes" => {
                let needle = args.first().copied().unwrap_or_else(Value::undefined);
                let from = arg_index(args, 1) as u32;
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let mut found = None;
                for index in from..length {
                    let has = self
                        .objects
                        .array_has(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    if !has && key == "indexOf" {
                        continue;
                    }
                    let value = if has {
                        self.objects
                            .array_get(object_id, index)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    } else {
                        Value::undefined()
                    };
                    if self.heap.strict_eq(value, needle) {
                        found = Some(index);
                        break;
                    }
                }
                if key == "includes" {
                    Value::boolean(found.is_some())
                } else {
                    found
                        .map(|index| self.heap.number(index as f64))
                        .unwrap_or_else(|| self.heap.number(-1.0))
                }
            }
            "slice" => {
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    as i32;
                let start = args
                    .first()
                    .and_then(|value| value.as_smi())
                    .unwrap_or(0)
                    .clamp(0, length);
                let end = args
                    .get(1)
                    .and_then(|value| value.as_smi())
                    .unwrap_or(length)
                    .clamp(start, length);
                let array = self
                    .objects
                    .allocate_array(self.shapes.root())
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                for index in start..end {
                    let has = self
                        .objects
                        .array_has(object_id, index as u32)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let value = if has {
                        self.objects
                            .array_get(object_id, index as u32)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    } else {
                        Value::hole()
                    };
                    self.objects
                        .array_push(array, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                Value::object(array.index())
            }
            "reverse" => {
                if self
                    .objects
                    .is_frozen(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                {
                    return Ok(Some(Value::object(object_id.index())));
                }
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let mut low = 0u32;
                let mut high = length;
                while low + 1 < high {
                    high -= 1;
                    let has_low = self
                        .objects
                        .array_has(object_id, low)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let has_high = self
                        .objects
                        .array_has(object_id, high)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let low_value = if has_low {
                        self.objects
                            .array_get(object_id, low)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    } else {
                        Value::hole()
                    };
                    let high_value = if has_high {
                        self.objects
                            .array_get(object_id, high)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    } else {
                        Value::hole()
                    };
                    self.objects
                        .array_set(object_id, low, high_value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    self.objects
                        .array_set(object_id, high, low_value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    low += 1;
                }
                Value::object(object_id.index())
            }
            "concat" => {
                let array = self
                    .objects
                    .allocate_array(self.shapes.root())
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                for index in 0..length {
                    let value = if self
                        .objects
                        .array_has(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        self.objects
                            .array_get(object_id, index)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    } else {
                        Value::hole()
                    };
                    self.objects
                        .array_push(array, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                for argument in args {
                    let source = argument.object_offset().map(ObjectId::new);
                    if let Some(source) = source
                        && self
                            .objects
                            .is_array(source)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        let length = self
                            .objects
                            .array_length(source)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                        for index in 0..length {
                            let value =
                                if self.objects.array_has(source, index).map_err(|error| {
                                    RuntimeError::runtime(error.to_string(), span)
                                })? {
                                    self.objects.array_get(source, index).map_err(|error| {
                                        RuntimeError::runtime(error.to_string(), span)
                                    })?
                                } else {
                                    Value::hole()
                                };
                            self.objects
                                .array_push(array, value)
                                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                        }
                    } else {
                        self.objects
                            .array_push(array, *argument)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                }
                Value::object(array.index())
            }
            "sort" => {
                if self
                    .objects
                    .is_frozen(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                {
                    return Ok(Some(Value::object(object_id.index())));
                }
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let mut values = Vec::new();
                for index in 0..length {
                    if self
                        .objects
                        .array_has(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        values.push(
                            self.objects
                                .array_get(object_id, index)
                                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?,
                        );
                    }
                }
                let comparator = args.first().copied().unwrap_or_else(Value::undefined);
                if comparator.is_undefined() {
                    values.sort_by(|left, right| {
                        self.heap
                            .to_js_string(*left)
                            .cmp(&self.heap.to_js_string(*right))
                    });
                } else {
                    let Some(comparator_id) = comparator.object_offset().map(ObjectId::new) else {
                        return Err(RuntimeError::type_error(
                            "sort comparator must be a function",
                            span,
                        ));
                    };
                    if !self
                        .objects
                        .is_function(comparator_id)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        return Err(RuntimeError::type_error(
                            "sort comparator must be a function",
                            span,
                        ));
                    }
                    let mut roots = roots.to_vec();
                    roots.extend_from_slice(&values);
                    for index in 1..values.len() {
                        let mut current = index;
                        while current > 0 {
                            let order = self
                                .invoke_callable(
                                    comparator,
                                    &[values[current - 1], values[current]],
                                    &roots,
                                    span,
                                )
                                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                            let order = self.heap.as_number(order).unwrap_or(f64::NAN);
                            if order > 0.0 {
                                values.swap(current - 1, current);
                                current -= 1;
                            } else {
                                break;
                            }
                        }
                    }
                }
                for index in 0..length {
                    let value = if (index as usize) < values.len() {
                        values[index as usize]
                    } else {
                        Value::hole()
                    };
                    self.objects
                        .array_set(object_id, index, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                Value::object(object_id.index())
            }
            "map" | "filter" | "forEach" => {
                let callback = args.first().copied().unwrap_or_else(Value::undefined);
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let result = if key == "forEach" {
                    None
                } else {
                    Some(
                        self.objects
                            .allocate_array(self.shapes.root())
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?,
                    )
                };
                let mut roots = roots.to_vec();
                if let Some(result) = result {
                    roots.push(Value::object(result.index()));
                }
                for index in 0..length {
                    let has = self
                        .objects
                        .array_has(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    if !has {
                        if key == "map"
                            && let Some(result) = result
                        {
                            self.objects
                                .array_push(result, Value::hole())
                                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                        }
                        continue;
                    }
                    let value = self
                        .objects
                        .array_get(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let callback_args = [value, self.heap.number(index as f64), object];
                    let mapped = self.invoke_callable(callback, &callback_args, &roots, span)?;
                    match key {
                        "map" => {
                            if let Some(result) = result {
                                self.objects.array_push(result, mapped).map_err(|error| {
                                    RuntimeError::runtime(error.to_string(), span)
                                })?;
                                roots.push(mapped);
                            }
                        }
                        "filter" => {
                            if let Some(result) = result
                                && self.heap.truthy(mapped)
                            {
                                self.objects.array_push(result, value).map_err(|error| {
                                    RuntimeError::runtime(error.to_string(), span)
                                })?;
                            }
                        }
                        _ => {}
                    }
                }
                result.map_or_else(Value::undefined, |array| Value::object(array.index()))
            }
            "some" | "every" | "find" => {
                let callback = args.first().copied().unwrap_or_else(Value::undefined);
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let roots = roots.to_vec();
                for index in 0..length {
                    let has = self
                        .objects
                        .array_has(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    if !has {
                        continue;
                    }
                    let value = self
                        .objects
                        .array_get(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let callback_args = [value, self.heap.number(index as f64), object];
                    let result = self.invoke_callable(callback, &callback_args, &roots, span)?;
                    let truthy = self.heap.truthy(result);
                    match key {
                        "some" if truthy => return Ok(Some(Value::boolean(true))),
                        "every" if !truthy => return Ok(Some(Value::boolean(false))),
                        "find" if truthy => return Ok(Some(value)),
                        _ => {}
                    }
                }
                if key == "some" {
                    Value::boolean(false)
                } else if key == "every" {
                    Value::boolean(true)
                } else {
                    Value::undefined()
                }
            }
            "reduce" => {
                let callback = args.first().copied().unwrap_or_else(Value::undefined);
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let mut index = 0;
                let mut accumulator = if let Some(initial) = args.get(1) {
                    *initial
                } else {
                    let mut first = None;
                    while index < length {
                        if self
                            .objects
                            .array_has(object_id, index)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                        {
                            first = Some(index);
                            break;
                        }
                        index += 1;
                    }
                    let Some(first) = first else {
                        return Err(RuntimeError::type_error(
                            "reduce of empty array with no initial value",
                            span,
                        ));
                    };
                    index = first + 1;
                    self.objects
                        .array_get(object_id, first)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                };
                let mut roots = roots.to_vec();
                roots.push(accumulator);
                for current in index..length {
                    if !self
                        .objects
                        .array_has(object_id, current)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    {
                        continue;
                    }
                    let value = self
                        .objects
                        .array_get(object_id, current)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let callback_args =
                        [accumulator, value, self.heap.number(current as f64), object];
                    accumulator = self.invoke_callable(callback, &callback_args, &roots, span)?;
                    roots.push(accumulator);
                }
                accumulator
            }
            _ => return Ok(None),
        }))
    }
}
