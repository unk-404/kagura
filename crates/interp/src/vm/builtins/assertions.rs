use kagura_object::{ObjectId, PropertyKey, own_property_keys};
use kagura_support::Span;
use kagura_value::Value;

use crate::error::{RuntimeError, RuntimeErrorKind};

use super::Interpreter;

impl<'a> Interpreter<'a> {
    pub(crate) fn error_name(
        &mut self,
        error: &RuntimeError,
        span: Span,
    ) -> Result<String, RuntimeError> {
        if let Some(thrown) = error.thrown
            && let Some(id) = thrown.object_offset()
        {
            let id = ObjectId::new(id);
            if self
                .objects
                .is_function(id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            {
                return Ok("function".into());
            }
            let name = self.get_property(thrown, "name", span)?;
            if let Some(name) = self.heap.as_str(name) {
                return Ok(name.to_owned());
            }
        }
        Ok(match error.kind {
            RuntimeErrorKind::Syntax => "SyntaxError",
            RuntimeErrorKind::Type => "TypeError",
            RuntimeErrorKind::Reference => "ReferenceError",
            RuntimeErrorKind::Range => "RangeError",
            RuntimeErrorKind::Runtime => "Error",
        }
        .into())
    }

    pub(crate) fn function_name(
        &mut self,
        value: Value,
        span: Span,
    ) -> Result<String, RuntimeError> {
        let name = self.get_property(value, "name", span)?;
        self.heap
            .as_str(name)
            .map(str::to_owned)
            .ok_or_else(|| RuntimeError::type_error("expected a constructor function", span))
    }

    pub(crate) fn assert_throws(
        &mut self,
        args: &[Value],
        span: Span,
        roots: &[Value],
    ) -> Result<Value, RuntimeError> {
        let expected = args.first().copied().ok_or_else(|| {
            RuntimeError::type_error("assert.throws requires a constructor", span)
        })?;
        let function = args
            .get(1)
            .copied()
            .ok_or_else(|| RuntimeError::type_error("assert.throws requires a function", span))?;
        let expected_name = self.function_name(expected, span)?;
        let function_id = self.object_id(function, span)?;
        if !self
            .objects
            .is_function(function_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Err(RuntimeError::type_error(
                "assert.throws requires a function",
                span,
            ));
        }
        let (function_index, _) = self
            .objects
            .function_data(function_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if function_index == u32::MAX {
            return Err(RuntimeError::type_error(
                "assert.throws cannot invoke a builtin stub",
                span,
            ));
        }
        match self.call(function_index as usize, &[], roots, None, None, None) {
            Ok(_) => Err(RuntimeError::runtime(
                format!("assert.throws expected {expected_name} but nothing was thrown"),
                span,
            )),
            Err(error) => {
                let actual = self.error_name(&error, span)?;
                if actual == expected_name {
                    Ok(Value::undefined())
                } else {
                    Err(RuntimeError::type_error(
                        format!("assert.throws expected {expected_name} but caught {actual}"),
                        span,
                    ))
                }
            }
        }
    }

    pub(crate) fn deep_equal(
        &mut self,
        actual: Value,
        expected: Value,
        span: Span,
    ) -> Result<bool, RuntimeError> {
        if self.same_value(actual, expected) {
            return Ok(true);
        }
        let Some(actual_id) = actual.object_offset().map(ObjectId::new) else {
            return Ok(false);
        };
        let Some(expected_id) = expected.object_offset().map(ObjectId::new) else {
            return Ok(false);
        };
        let actual_array = self
            .objects
            .is_array(actual_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let expected_array = self
            .objects
            .is_array(expected_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if actual_array || expected_array {
            return self.compare_arrays(actual, expected, span);
        }
        let actual_shape = self
            .objects
            .shape(actual_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let expected_shape = self
            .objects
            .shape(expected_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let actual_keys = own_property_keys(self.shapes, actual_shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let expected_keys = own_property_keys(self.shapes, expected_shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if actual_keys.len() != expected_keys.len() {
            return Ok(false);
        }
        for (actual_key, expected_key) in actual_keys.iter().zip(expected_keys.iter()) {
            if actual_key != expected_key {
                return Ok(false);
            }
            let key = match actual_key {
                PropertyKey::String(key) => key.clone(),
                PropertyKey::Index(index) => index.to_string(),
                PropertyKey::Symbol(_) => return Ok(false),
            };
            let left = self.get_property(actual, &key, span)?;
            let right = self.get_property(expected, &key, span)?;
            if !self.deep_equal(left, right, span)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(crate) fn compare_arrays(
        &mut self,
        actual: Value,
        expected: Value,
        span: Span,
    ) -> Result<bool, RuntimeError> {
        let actual_id = self.object_id(actual, span)?;
        let expected_id = self.object_id(expected, span)?;
        if !self
            .objects
            .is_array(actual_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            || !self
                .objects
                .is_array(expected_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Ok(false);
        }
        let actual_length = self
            .objects
            .array_length(actual_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let expected_length = self
            .objects
            .array_length(expected_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if actual_length != expected_length {
            return Ok(false);
        }
        for index in 0..actual_length {
            let left = self
                .objects
                .array_get(actual_id, index)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let right = self
                .objects
                .array_get(expected_id, index)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            if !self.same_value(left, right) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(crate) fn assert_deep_equal(
        &mut self,
        args: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let actual = args.first().copied().ok_or_else(|| {
            RuntimeError::type_error("assert.deepEqual requires two values", span)
        })?;
        let expected = args.get(1).copied().ok_or_else(|| {
            RuntimeError::type_error("assert.deepEqual requires two values", span)
        })?;
        if self.deep_equal(actual, expected, span)? {
            Ok(Value::undefined())
        } else {
            Err(RuntimeError::runtime("assert.deepEqual failed", span))
        }
    }

    pub(crate) fn assert_compare_array(
        &mut self,
        args: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let actual = args.first().copied().ok_or_else(|| {
            RuntimeError::type_error("assert.compareArray requires two arrays", span)
        })?;
        let expected = args.get(1).copied().ok_or_else(|| {
            RuntimeError::type_error("assert.compareArray requires two arrays", span)
        })?;
        if self.compare_arrays(actual, expected, span)? {
            Ok(Value::undefined())
        } else {
            Err(RuntimeError::runtime("assert.compareArray failed", span))
        }
    }
}
