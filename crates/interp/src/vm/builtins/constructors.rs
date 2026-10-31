use kagura_object::ObjectId;
use kagura_support::Span;
use kagura_value::Value;

use crate::error::RuntimeError;

use super::Interpreter;
use super::method_id::MethodOwner;

impl<'a> Interpreter<'a> {
    pub(crate) fn build_builtin(&mut self, name: &str, span: Span) -> Result<Value, RuntimeError> {
        if name == "Math" {
            let object = self
                .objects
                .allocate(self.shapes.root())
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let object = Value::object(object.index());
            for (key, value) in [
                ("PI", std::f64::consts::PI),
                ("E", std::f64::consts::E),
                ("LN2", std::f64::consts::LN_2),
                ("LN10", std::f64::consts::LN_10),
                ("LOG2E", std::f64::consts::LOG2_E),
                ("LOG10E", std::f64::consts::LOG10_E),
                ("SQRT2", std::f64::consts::SQRT_2),
                ("SQRT1_2", std::f64::consts::FRAC_1_SQRT_2),
            ] {
                let value = self.heap.number(value);
                self.set_property(object, key, value, span)?;
            }
            for method in [
                "abs", "floor", "ceil", "trunc", "round", "sqrt", "pow", "acos", "asin", "atan",
                "atan2", "cos", "sin", "tan", "exp", "log", "log2", "log10", "cbrt", "fround",
                "clz32", "imul", "hypot", "min", "max", "random",
            ] {
                let function = self.allocate_builtin_method(method, MethodOwner::Math, span)?;
                self.set_non_enumerable_property(object, method, function, span)?;
            }
            return Ok(object);
        }
        if name == "NaN" {
            return Ok(self.heap.number(f64::NAN));
        }
        if name == "Infinity" {
            return Ok(self.heap.number(f64::INFINITY));
        }
        if name == "Function" {
            if self.object_prototype.is_none() {
                self.load_builtin("Object", span)?;
            }
            let prototype = self
                .objects
                .allocate_function(
                    self.shapes.function_shape(),
                    u32::MAX,
                    Value::undefined(),
                    Value::undefined(),
                )
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            if let Some(object_prototype) = self.object_prototype {
                let function_shape = self.shapes.function_shape();
                let shape = self
                    .shapes
                    .with_prototype(function_shape, Some(object_prototype))
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                self.objects
                    .set_shape(prototype, shape)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            }
            let prototype = Value::object(prototype.index());
            for method in ["call", "apply", "bind", "toString"] {
                let function = self.allocate_builtin_method(method, MethodOwner::Function, span)?;
                self.set_non_enumerable_property(prototype, method, function, span)?;
            }
            let object = self
                .objects
                .allocate_function(
                    self.shapes.function_shape(),
                    u32::MAX,
                    Value::undefined(),
                    Value::undefined(),
                )
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let object = Value::object(object.index());
            let name_value = self.heap.string("Function");
            self.set_property(object, "name", name_value, span)?;
            self.set_property(object, "prototype", prototype, span)?;
            self.set_non_enumerable_property(prototype, "constructor", object, span)?;
            return Ok(object);
        }
        if name == "String" || name == "Boolean" {
            let object = self
                .objects
                .allocate_function(
                    self.shapes.function_shape(),
                    u32::MAX,
                    Value::undefined(),
                    Value::undefined(),
                )
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let object = Value::object(object.index());
            let name_value = self.heap.string(name);
            self.set_property(object, "name", name_value, span)?;
            let prototype_shape = self.object_shape(span)?;
            let prototype = self
                .objects
                .allocate(prototype_shape)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let prototype = Value::object(prototype.index());
            let wrapper = self.heap.string(&name.to_lowercase());
            self.set_non_enumerable_property(prototype, "__wrapper", wrapper, span)?;
            let primitive = if name == "String" {
                self.heap.string("")
            } else {
                Value::boolean(false)
            };
            self.set_non_enumerable_property(prototype, "__primitive", primitive, span)?;
            let methods: &[&str] = if name == "String" {
                &[
                    "charAt",
                    "charCodeAt",
                    "codePointAt",
                    "includes",
                    "indexOf",
                    "slice",
                    "toUpperCase",
                    "toLowerCase",
                    "trim",
                    "trimStart",
                    "trimEnd",
                    "replace",
                    "replaceAll",
                    "startsWith",
                    "endsWith",
                    "repeat",
                    "substring",
                    "substr",
                    "at",
                    "concat",
                    "localeCompare",
                    "padStart",
                    "padEnd",
                    "split",
                    "toString",
                    "valueOf",
                ]
            } else {
                &["toString", "valueOf"]
            };
            for method in methods {
                let function = self.allocate_builtin_method(
                    method,
                    if name == "String" {
                        MethodOwner::String
                    } else {
                        MethodOwner::Boolean
                    },
                    span,
                )?;
                self.set_non_enumerable_property(prototype, method, function, span)?;
            }
            self.set_non_enumerable_property(prototype, "constructor", object, span)?;
            self.set_property(object, "prototype", prototype, span)?;
            if name == "String" {
                for (method, arity) in [("fromCharCode", 1), ("fromCodePoint", 1)] {
                    let path = format!("String.{method}");
                    let function = self.allocate_static_builtin(&path, arity, span)?;
                    self.set_non_enumerable_property(object, method, function, span)?;
                }
            }
            return Ok(object);
        }
        if name == "Number" {
            let object = self
                .objects
                .allocate_function(
                    self.shapes.function_shape(),
                    u32::MAX,
                    Value::undefined(),
                    Value::undefined(),
                )
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let object = Value::object(object.index());
            let name_value = self.heap.string("Number");
            self.set_property(object, "name", name_value, span)?;
            for (key, value) in [
                ("MAX_SAFE_INTEGER", 9_007_199_254_740_991.0),
                ("MIN_SAFE_INTEGER", -9_007_199_254_740_991.0),
                ("MAX_VALUE", f64::MAX),
                ("MIN_VALUE", f64::MIN_POSITIVE),
                ("EPSILON", f64::EPSILON),
                ("POSITIVE_INFINITY", f64::INFINITY),
                ("NEGATIVE_INFINITY", f64::NEG_INFINITY),
                ("NaN", f64::NAN),
            ] {
                let value = self.heap.number(value);
                self.set_non_enumerable_property(object, key, value, span)?;
            }
            for method in [
                "isNaN",
                "isFinite",
                "isInteger",
                "isSafeInteger",
                "parseInt",
                "parseFloat",
            ] {
                let function = self.allocate_builtin_method(method, MethodOwner::Number, span)?;
                self.set_non_enumerable_property(object, method, function, span)?;
            }
            let prototype_shape = self.object_shape(span)?;
            let prototype = self
                .objects
                .allocate(prototype_shape)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let prototype = Value::object(prototype.index());
            let wrapper = self.heap.string("number");
            self.set_non_enumerable_property(prototype, "__wrapper", wrapper, span)?;
            let zero = self.heap.number(0.0);
            self.set_non_enumerable_property(prototype, "__primitive", zero, span)?;
            for method in [
                "toString",
                "toLocaleString",
                "valueOf",
                "toFixed",
                "toExponential",
                "toPrecision",
            ] {
                let function = self.allocate_builtin_method(method, MethodOwner::Number, span)?;
                self.set_non_enumerable_property(prototype, method, function, span)?;
            }
            self.set_non_enumerable_property(prototype, "constructor", object, span)?;
            self.set_property(object, "prototype", prototype, span)?;
            return Ok(object);
        }
        if name == "Object" || name == "Array" {
            if name == "Array" && self.object_prototype.is_none() {
                self.load_builtin("Object", span)?;
            }
            let prototype = if name == "Object" {
                self.object_prototype
            } else {
                self.array_prototype
            };
            let prototype = if let Some(prototype) = prototype {
                prototype
            } else {
                let shape = if name == "Array" {
                    if let Some(object_prototype) = self.object_prototype {
                        self.shapes
                            .with_prototype(self.shapes.root(), Some(object_prototype))
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                    } else {
                        self.shapes.root()
                    }
                } else {
                    self.shapes.root()
                };
                let prototype = self
                    .objects
                    .allocate(shape)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                if name == "Object" {
                    self.object_prototype = Some(prototype);
                } else {
                    self.array_prototype = Some(prototype);
                }
                prototype
            };
            let prototype_value = Value::object(prototype.index());
            let methods: &[&str] = if name == "Array" {
                &[
                    "at",
                    "concat",
                    "every",
                    "fill",
                    "filter",
                    "find",
                    "findIndex",
                    "findLast",
                    "findLastIndex",
                    "flat",
                    "flatMap",
                    "forEach",
                    "includes",
                    "indexOf",
                    "join",
                    "lastIndexOf",
                    "map",
                    "pop",
                    "push",
                    "shift",
                    "unshift",
                    "splice",
                    "reduce",
                    "reduceRight",
                    "reverse",
                    "slice",
                    "some",
                    "sort",
                    "copyWithin",
                    "toString",
                    "toLocaleString",
                ]
            } else {
                &[
                    "hasOwnProperty",
                    "isPrototypeOf",
                    "propertyIsEnumerable",
                    "toString",
                    "toLocaleString",
                    "valueOf",
                ]
            };
            for method in methods {
                let function = self.allocate_builtin_method(
                    method,
                    if name == "Array" {
                        MethodOwner::Array
                    } else {
                        MethodOwner::Object
                    },
                    span,
                )?;
                self.set_non_enumerable_property(prototype_value, method, function, span)?;
            }
            let object = self
                .objects
                .allocate_function(
                    self.shapes.function_shape(),
                    u32::MAX,
                    Value::undefined(),
                    Value::undefined(),
                )
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let value = Value::object(object.index());
            let name_value = self.heap.string(name);
            self.set_property(value, "name", name_value, span)?;
            self.set_property(value, "prototype", prototype_value, span)?;
            self.set_non_enumerable_property(prototype_value, "constructor", value, span)?;
            if name == "Object" {
                for (method, arity) in [
                    ("assign", 2),
                    ("create", 2),
                    ("defineProperty", 3),
                    ("defineProperties", 2),
                    ("entries", 1),
                    ("freeze", 1),
                    ("getOwnPropertyDescriptor", 2),
                    ("getOwnPropertyDescriptors", 1),
                    ("getOwnPropertyNames", 1),
                    ("getPrototypeOf", 1),
                    ("hasOwn", 2),
                    ("is", 2),
                    ("isExtensible", 1),
                    ("isFrozen", 1),
                    ("isSealed", 1),
                    ("keys", 1),
                    ("preventExtensions", 1),
                    ("seal", 1),
                    ("setPrototypeOf", 2),
                    ("values", 1),
                ] {
                    let path = format!("Object.{method}");
                    let function = self.allocate_static_builtin(&path, arity, span)?;
                    self.set_non_enumerable_property(value, method, function, span)?;
                }
            } else if name == "Array" {
                for (method, arity) in [("from", 1), ("isArray", 1), ("of", 0)] {
                    let path = format!("Array.{method}");
                    let function = self.allocate_static_builtin(&path, arity, span)?;
                    self.set_non_enumerable_property(value, method, function, span)?;
                }
            }
            return Ok(value);
        }
        if name == "JSON" {
            let object = self
                .objects
                .allocate(self.shapes.root())
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let object = Value::object(object.index());
            for (method, arity) in [("parse", 2), ("stringify", 3)] {
                let path = format!("JSON.{method}");
                let function = self.allocate_static_builtin(&path, arity, span)?;
                self.set_non_enumerable_property(object, method, function, span)?;
            }
            return Ok(object);
        }
        if name == "Date" {
            let object = self
                .objects
                .allocate_function(
                    self.shapes.function_shape(),
                    u32::MAX,
                    Value::undefined(),
                    Value::undefined(),
                )
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let value = Value::object(object.index());
            let name_value = self.heap.string(name);
            self.set_property(value, "name", name_value, span)?;
            let prototype = self
                .objects
                .allocate(self.shapes.root())
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let prototype = Value::object(prototype.index());
            for method in [
                "getTime",
                "valueOf",
                "toISOString",
                "toString",
                "getYear",
                "getUTCFullYear",
                "getFullYear",
                "getUTCMonth",
                "getMonth",
                "getUTCDate",
                "getDate",
                "getUTCDay",
                "getDay",
                "getUTCHours",
                "getHours",
                "getUTCMinutes",
                "getMinutes",
                "getUTCSeconds",
                "getSeconds",
                "getUTCMilliseconds",
                "getMilliseconds",
                "getTimezoneOffset",
                "setTime",
                "toJSON",
                "toUTCString",
                "toGMTString",
                "toDateString",
                "toTimeString",
                "toLocaleDateString",
                "toLocaleTimeString",
                "toLocaleString",
            ] {
                let function = self.allocate_builtin_method(method, MethodOwner::Date, span)?;
                self.set_non_enumerable_property(prototype, method, function, span)?;
            }
            self.set_property(value, "prototype", prototype, span)?;
            for (method, arity) in [("now", 0), ("parse", 1), ("UTC", 7)] {
                let path = format!("Date.{method}");
                let function = self.allocate_static_builtin(&path, arity, span)?;
                self.set_non_enumerable_property(value, method, function, span)?;
            }
            return Ok(value);
        }
        if matches!(
            name,
            "Error"
                | "TypeError"
                | "RangeError"
                | "ReferenceError"
                | "SyntaxError"
                | "EvalError"
                | "URIError"
                | "Test262Error"
        ) {
            let prototype_shape = self.object_shape(span)?;
            let prototype = self
                .objects
                .allocate(prototype_shape)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let prototype_value = Value::object(prototype.index());
            let object = self
                .objects
                .allocate_function(
                    self.shapes.function_shape(),
                    u32::MAX,
                    Value::undefined(),
                    Value::undefined(),
                )
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            let value = Value::object(object.index());
            let name_value = self.heap.string(name);
            self.set_non_enumerable_property(value, "name", name_value, span)?;
            self.set_non_enumerable_frozen_property(value, "prototype", prototype_value, span)?;
            let length = self.heap.number(1.0);
            self.set_non_enumerable_readonly_property(value, "length", length, span)?;
            self.set_non_enumerable_property(prototype_value, "constructor", value, span)?;
            self.set_non_enumerable_property(prototype_value, "name", name_value, span)?;
            let empty = self.heap.string("");
            self.set_non_enumerable_property(prototype_value, "message", empty, span)?;
            let to_string =
                self.allocate_builtin_method("Error.prototype.toString", MethodOwner::Error, span)?;
            let to_string_name = self.heap.string("toString");
            self.set_non_enumerable_readonly_property(to_string, "name", to_string_name, span)?;
            self.set_non_enumerable_property(prototype_value, "toString", to_string, span)?;
            if name != "Error" {
                let parent = self.load_builtin("Error", span)?;
                let parent_prototype = self.get_property(parent, "prototype", span)?;
                if let Some(parent_prototype) = parent_prototype.object_offset().map(ObjectId::new)
                {
                    let current_shape = self
                        .objects
                        .shape(prototype)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let shape = self
                        .shapes
                        .with_prototype(current_shape, Some(parent_prototype))
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    self.objects
                        .set_shape(prototype, shape)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
            }
            return Ok(value);
        }
        let object = self
            .objects
            .allocate_function(
                self.shapes.function_shape(),
                u32::MAX,
                Value::undefined(),
                Value::undefined(),
            )
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let value = Value::object(object.index());
        let name_value = self.heap.string(name);
        self.set_property(value, "name", name_value, span)?;
        Ok(value)
    }

    pub(crate) fn new_name(
        &mut self,
        name: &str,
        args: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        match name {
            "Error" | "TypeError" | "RangeError" | "ReferenceError" | "SyntaxError"
            | "EvalError" | "URIError" | "Test262Error" => {
                let builtin = self.load_builtin(name, span)?;
                let prototype = self.get_property(builtin, "prototype", span)?;
                let shape = if let Some(prototype) = prototype.object_offset().map(ObjectId::new) {
                    self.shapes
                        .with_prototype(self.shapes.root(), Some(prototype))
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                } else {
                    self.shapes.root()
                };
                let object = self
                    .objects
                    .allocate(shape)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let object = Value::object(object.index());
                self.set_non_enumerable_property(object, "__error", Value::boolean(true), span)?;
                if let Some(message) = args.first().copied().filter(|value| !value.is_undefined()) {
                    let message = self.heap.to_js_string(message);
                    let message_value = self.heap.string(&message);
                    self.set_non_enumerable_property(object, "message", message_value, span)?;
                }
                Ok(object)
            }
            "String" | "Number" | "Boolean" => {
                let primitive = match name {
                    "String" => {
                        if args.is_empty() {
                            self.heap.string("")
                        } else {
                            let text = self.heap.to_js_string(args[0]);
                            self.heap.string(&text)
                        }
                    }
                    "Number" => {
                        let value = if args.is_empty() {
                            0.0
                        } else {
                            self.heap.to_number(args[0]).unwrap_or(f64::NAN)
                        };
                        self.heap.number(value)
                    }
                    _ => {
                        let value = args.first().copied().unwrap_or_else(Value::undefined);
                        Value::boolean(self.heap.truthy(value))
                    }
                };
                let builtin = self.load_builtin(name, span)?;
                let prototype = self.get_property(builtin, "prototype", span)?;
                let shape = if let Some(prototype) = prototype.object_offset().map(ObjectId::new) {
                    self.shapes
                        .with_prototype(self.shapes.root(), Some(prototype))
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                } else {
                    self.shapes.root()
                };
                let object = self
                    .objects
                    .allocate(shape)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                let object = Value::object(object.index());
                let wrapper = self.heap.string(&name.to_lowercase());
                self.set_non_enumerable_property(object, "__wrapper", wrapper, span)?;
                self.set_non_enumerable_property(object, "__primitive", primitive, span)?;
                Ok(object)
            }
            "Object" => {
                let shape = self.object_shape(span)?;
                let object = self
                    .objects
                    .allocate(shape)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                Ok(Value::object(object.index()))
            }
            "Array" => {
                let shape = self.array_shape(span)?;
                let array = self
                    .objects
                    .allocate_array(shape)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                if args.len() == 1
                    && let Some(length) = self.heap.as_number(args[0])
                    && length >= 0.0
                    && length.fract() == 0.0
                    && length <= 1024.0
                {
                    for _ in 0..length as u32 {
                        self.objects
                            .array_push(array, Value::hole())
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                } else {
                    for value in args {
                        self.objects
                            .array_push(array, *value)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                }
                Ok(Value::object(array.index()))
            }
            "Function" => self.anonymous_function(span),
            "Date" => self.date_construct(args, span),
            _ => Err(RuntimeError::type_error(
                format!("{name} is not a constructor"),
                span,
            )),
        }
    }

    pub(crate) fn anonymous_function(&mut self, span: Span) -> Result<Value, RuntimeError> {
        let object = self
            .objects
            .allocate_function(
                self.shapes.function_shape(),
                u32::MAX,
                Value::undefined(),
                Value::undefined(),
            )
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let object = Value::object(object.index());
        let name_value = self.heap.string("anonymous");
        self.set_property(object, "name", name_value, span)?;
        let length = self.heap.number(0.0);
        self.set_property(object, "length", length, span)?;
        Ok(object)
    }

    pub(crate) fn construct(
        &mut self,
        callee: Value,
        args: &[Value],
        span: Span,
        roots: &[Value],
    ) -> Result<Value, RuntimeError> {
        let callee_id = self.object_id(callee, span)?;
        if !self
            .objects
            .is_function(callee_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Err(RuntimeError::type_error("value is not a constructor", span));
        }
        let (function_index, closure_env) = self
            .objects
            .function_data(callee_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        if function_index == u32::MAX {
            let name = self.get_property(callee, "name", span)?;
            let name = self
                .heap
                .as_str(name)
                .ok_or_else(|| RuntimeError::type_error("invalid constructor", span))?
                .to_owned();
            return self.new_name(&name, args, span);
        }
        let prototype = self.get_property(callee, "prototype", span)?;
        let root = self.shapes.root();
        let shape = if let Some(prototype_id) = prototype.object_offset().map(ObjectId::new) {
            self.shapes
                .with_prototype(root, Some(prototype_id))
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        } else {
            root
        };
        let object = self
            .objects
            .allocate(shape)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let object = Value::object(object.index());
        let mut all_roots = roots.to_vec();
        all_roots.push(object);
        all_roots.push(prototype);
        let class_key = self.heap.string("__kaguraClass");
        self.set_non_enumerable_property(object, "__kaguraClass", callee, span)?;
        let _ = class_key;
        let default_super = self.get_property(callee, "__kaguraDefaultCtor", span)?;
        if !default_super.is_undefined() {
            let super_id = self.object_id(default_super, span)?;
            if self
                .objects
                .is_function(super_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            {
                let (super_index, _) = self
                    .objects
                    .function_data(super_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                if super_index != u32::MAX {
                    self.call(
                        super_index as usize,
                        args,
                        &all_roots,
                        None,
                        None,
                        Some(object),
                    )?;
                }
            }
        }
        match self.call(
            function_index as usize,
            args,
            &all_roots,
            closure_env.object_offset().map(ObjectId::new),
            None,
            Some(object),
        ) {
            Ok(value) if value.object_offset().is_some() => Ok(value),
            Ok(_) => Ok(object),
            Err(error) => Err(error),
        }
    }
}
