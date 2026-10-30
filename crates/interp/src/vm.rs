use kagura_bytecode::{Const, Module, Op};
use kagura_object::{
    ObjectHeap, ObjectId, PropertyAttributes, PropertyKey, PropertyLocation, ShapeTable,
};
use kagura_support::Span;
use kagura_value::{Heap, Value};
use tracing::instrument;

use crate::error::{RuntimeError, RuntimeErrorKind};
use crate::host::{Host, HostCall, HostFunctionId};

const MAX_CALL_DEPTH: usize = 256;
const MAX_STACK: usize = 4096;

pub struct Interpreter<'a> {
    module: &'a Module,
    heap: &'a mut Heap,
    objects: &'a mut ObjectHeap,
    shapes: &'a mut ShapeTable,
    host: &'a mut dyn Host,
    host_cache: Vec<Option<HostFunctionId>>,
    depth: usize,
}

impl<'a> Interpreter<'a> {
    pub fn new(
        module: &'a Module,
        heap: &'a mut Heap,
        objects: &'a mut ObjectHeap,
        shapes: &'a mut ShapeTable,
        host: &'a mut dyn Host,
    ) -> Self {
        Self {
            module,
            heap,
            objects,
            shapes,
            host,
            host_cache: vec![None; module.constants.len()],
            depth: 0,
        }
    }

    #[instrument(skip(self), fields(entry = self.module.entry))]
    pub fn run(&mut self) -> Result<Value, RuntimeError> {
        let entry = self.module.entry;
        self.call(entry, &[])
    }

    #[instrument(skip(self, args), fields(function = index, argc = args.len()))]
    fn call(&mut self, index: usize, args: &[Value]) -> Result<Value, RuntimeError> {
        if self.depth >= MAX_CALL_DEPTH {
            return Err(RuntimeError::range(
                "maximum call depth exceeded",
                Span::empty(0),
            ));
        }
        let function = self.module.functions.get(index).cloned().ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorKind::Runtime,
                "missing function",
                Span::empty(0),
            )
        })?;
        let mut locals = vec![Value::undefined(); function.local_count as usize];
        let mut defined = vec![false; function.local_count as usize];
        for (slot, arg) in args.iter().take(function.param_count as usize).enumerate() {
            locals[slot] = *arg;
            defined[slot] = true;
        }
        defined[..function.param_count as usize].fill(true);

        self.depth += 1;
        let result = self.execute(index, &function, &mut locals, &mut defined);
        self.depth -= 1;
        result
    }

    fn execute(
        &mut self,
        function_index: usize,
        function: &kagura_bytecode::FunctionProto,
        locals: &mut [Value],
        defined: &mut [bool],
    ) -> Result<Value, RuntimeError> {
        let mut stack: Vec<Value> = Vec::new();
        let mut pc = 0usize;
        loop {
            let span = self.module.span_at(function_index, pc);
            let byte = *function.code.get(pc).ok_or_else(|| {
                RuntimeError::new(
                    RuntimeErrorKind::Runtime,
                    "bytecode ended unexpectedly",
                    span,
                )
            })?;
            pc += 1;
            let op = Op::from_byte(byte).ok_or_else(|| {
                RuntimeError::new(
                    RuntimeErrorKind::Runtime,
                    format!("unknown opcode {byte}"),
                    span,
                )
            })?;
            match op {
                Op::Halt => return Ok(stack.pop().unwrap_or_else(Value::undefined)),
                Op::LoadConst => {
                    let index = read_u32(&function.code, &mut pc, span)?;
                    let constant = self.module.constants.get(index as usize).ok_or_else(|| {
                        RuntimeError::new(RuntimeErrorKind::Runtime, "invalid constant index", span)
                    })?;
                    let value = match constant {
                        Const::Number(value) => self.heap.number(*value),
                        Const::String(value) => self.heap.string(value),
                        Const::Undefined => Value::undefined(),
                        Const::Null => Value::null(),
                        Const::True => Value::boolean(true),
                        Const::False => Value::boolean(false),
                    };
                    push(&mut stack, value, span)?;
                }
                Op::LoadLocal => {
                    let slot = read_u16(&function.code, &mut pc, span)? as usize;
                    if slot >= locals.len() {
                        return Err(RuntimeError::reference("invalid local slot", span));
                    }
                    if !defined[slot] {
                        return Err(RuntimeError::reference(
                            "cannot access local before initialization",
                            span,
                        ));
                    }
                    push(&mut stack, locals[slot], span)?;
                }
                Op::StoreLocal => {
                    let slot = read_u16(&function.code, &mut pc, span)? as usize;
                    if slot >= locals.len() {
                        return Err(RuntimeError::reference("invalid local slot", span));
                    }
                    let value = pop(&mut stack, span)?;
                    locals[slot] = value;
                    defined[slot] = true;
                }
                Op::Add => {
                    let right = pop(&mut stack, span)?;
                    let left = pop(&mut stack, span)?;
                    let value = self.add(left, right, span)?;
                    push(&mut stack, value, span)?;
                }
                Op::Sub | Op::Mul | Op::Div | Op::Mod => {
                    let right = pop(&mut stack, span)?;
                    let left = pop(&mut stack, span)?;
                    let left = self.number(left, span)?;
                    let right = self.number(right, span)?;
                    let value = match op {
                        Op::Sub => left - right,
                        Op::Mul => left * right,
                        Op::Div => left / right,
                        Op::Mod => left % right,
                        _ => unreachable!(),
                    };
                    push(&mut stack, self.heap.number(value), span)?;
                }
                Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                    let right = pop(&mut stack, span)?;
                    let left = pop(&mut stack, span)?;
                    let value = self.compare(op, left, right, span)?;
                    push(&mut stack, Value::boolean(value), span)?;
                }
                Op::Eq | Op::Ne => {
                    let right = pop(&mut stack, span)?;
                    let left = pop(&mut stack, span)?;
                    let equal = self.loose_eq(left, right);
                    push(
                        &mut stack,
                        Value::boolean(if matches!(op, Op::Eq) { equal } else { !equal }),
                        span,
                    )?;
                }
                Op::StrictEq | Op::StrictNotEq => {
                    let right = pop(&mut stack, span)?;
                    let left = pop(&mut stack, span)?;
                    let equal = self.heap.strict_eq(left, right);
                    push(
                        &mut stack,
                        Value::boolean(if matches!(op, Op::StrictEq) {
                            equal
                        } else {
                            !equal
                        }),
                        span,
                    )?;
                }
                Op::Neg => {
                    let value = pop(&mut stack, span)?;
                    let number = self.number(value, span)?;
                    push(&mut stack, self.heap.number(-number), span)?;
                }
                Op::Not => {
                    let value = pop(&mut stack, span)?;
                    push(&mut stack, Value::boolean(!self.heap.truthy(value)), span)?;
                }
                Op::Jump => {
                    let target = read_u32(&function.code, &mut pc, span)? as usize;
                    if target > function.code.len() {
                        return Err(RuntimeError::new(
                            RuntimeErrorKind::Runtime,
                            "invalid jump target",
                            span,
                        ));
                    }
                    pc = target;
                }
                Op::JumpIfFalse => {
                    let target = read_u32(&function.code, &mut pc, span)? as usize;
                    let value = pop(&mut stack, span)?;
                    if !self.heap.truthy(value) {
                        pc = target;
                    }
                }
                Op::JumpIfTrue => {
                    let target = read_u32(&function.code, &mut pc, span)? as usize;
                    let value = pop(&mut stack, span)?;
                    if self.heap.truthy(value) {
                        pc = target;
                    }
                }
                Op::CallName => {
                    let index = read_u32(&function.code, &mut pc, span)?;
                    let argc = read_u8(&function.code, &mut pc, span)? as usize;
                    let name = match self.module.constants.get(index as usize) {
                        Some(Const::String(name)) => name.clone(),
                        _ => {
                            return Err(RuntimeError::new(
                                RuntimeErrorKind::Runtime,
                                "invalid call name",
                                span,
                            ));
                        }
                    };
                    let mut args = Vec::with_capacity(argc);
                    for _ in 0..argc {
                        args.push(pop(&mut stack, span)?);
                    }
                    args.reverse();
                    let value = self.call_name(&name, index as usize, &args, span)?;
                    push(&mut stack, value, span)?;
                }
                Op::CallHost => {
                    let index = read_u32(&function.code, &mut pc, span)?;
                    let argc = read_u8(&function.code, &mut pc, span)? as usize;
                    let path = match self.module.constants.get(index as usize) {
                        Some(Const::String(path)) => path.clone(),
                        _ => {
                            return Err(RuntimeError::new(
                                RuntimeErrorKind::Runtime,
                                "invalid host path",
                                span,
                            ));
                        }
                    };
                    let mut args = Vec::with_capacity(argc);
                    for _ in 0..argc {
                        args.push(pop(&mut stack, span)?);
                    }
                    args.reverse();
                    let id = self.resolve_host(&path, index as usize, span)?;
                    let value = self.invoke_host(id, &args, span)?;
                    push(&mut stack, value, span)?;
                }
                Op::Print => {
                    // Bytecode v1 compatibility: Print is permanently reserved and
                    // is routed through the host's console.log binding, not an
                    // engine-internal output path.
                    let argc = read_u8(&function.code, &mut pc, span)? as usize;
                    let mut args = Vec::with_capacity(argc);
                    for _ in 0..argc {
                        args.push(pop(&mut stack, span)?);
                    }
                    args.reverse();
                    let id = self.host.resolve("console.log").ok_or_else(|| {
                        RuntimeError::runtime("console.log is not registered", span)
                    })?;
                    let value = self.invoke_host(id, &args, span)?;
                    push(&mut stack, value, span)?;
                }
                Op::NewObject => {
                    let shape = self.shapes.root();
                    let object = self
                        .objects
                        .allocate(shape)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    push(&mut stack, Value::object(object.index()), span)?;
                }
                Op::GetProp => {
                    let index = read_u32(&function.code, &mut pc, span)?;
                    let key = match self.module.constants.get(index as usize) {
                        Some(Const::String(key)) => key.clone(),
                        _ => {
                            return Err(RuntimeError::runtime("invalid property key", span));
                        }
                    };
                    let object = pop(&mut stack, span)?;
                    let value = self.get_property(object, &key, span)?;
                    push(&mut stack, value, span)?;
                }
                Op::SetProp => {
                    let index = read_u32(&function.code, &mut pc, span)?;
                    let key = match self.module.constants.get(index as usize) {
                        Some(Const::String(key)) => key.clone(),
                        _ => {
                            return Err(RuntimeError::runtime("invalid property key", span));
                        }
                    };
                    let value = pop(&mut stack, span)?;
                    let object = pop(&mut stack, span)?;
                    self.set_property(object, &key, value, span)?;
                    push(&mut stack, object, span)?;
                }
                Op::SetPropValue => {
                    let index = read_u32(&function.code, &mut pc, span)?;
                    let key = match self.module.constants.get(index as usize) {
                        Some(Const::String(key)) => key.clone(),
                        _ => {
                            return Err(RuntimeError::runtime("invalid property key", span));
                        }
                    };
                    let value = pop(&mut stack, span)?;
                    let object = pop(&mut stack, span)?;
                    self.set_property(object, &key, value, span)?;
                    push(&mut stack, value, span)?;
                }
                Op::NewArray => {
                    let shape = self.shapes.root();
                    let object = self
                        .objects
                        .allocate_array(shape)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    push(&mut stack, Value::object(object.index()), span)?;
                }
                Op::ArrayPush => {
                    let value = pop(&mut stack, span)?;
                    let object = pop(&mut stack, span)?;
                    let id = self.object_id(object, span)?;
                    self.objects
                        .array_push(id, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    push(&mut stack, object, span)?;
                }
                Op::GetIndex => {
                    let index = pop(&mut stack, span)?;
                    let object = pop(&mut stack, span)?;
                    let id = self.object_id(object, span)?;
                    let index = self.index_value(index, span)?;
                    let value = self
                        .objects
                        .array_get(id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    push(&mut stack, value, span)?;
                }
                Op::SetIndexValue => {
                    let value = pop(&mut stack, span)?;
                    let index = pop(&mut stack, span)?;
                    let object = pop(&mut stack, span)?;
                    let id = self.object_id(object, span)?;
                    let index = self.index_value(index, span)?;
                    self.objects
                        .array_set(id, index, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    push(&mut stack, value, span)?;
                }
                Op::Pop => {
                    pop(&mut stack, span)?;
                }
                Op::Return => {
                    return Ok(pop(&mut stack, span).unwrap_or_else(|_| Value::undefined()));
                }
                Op::Dup => {
                    let value = *stack.last().ok_or_else(|| {
                        RuntimeError::new(RuntimeErrorKind::Runtime, "stack underflow", span)
                    })?;
                    push(&mut stack, value, span)?;
                }
            }
        }
    }

    fn call_name(
        &mut self,
        name: &str,
        index: usize,
        args: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        if let Some(function) = self.module.find_function(name) {
            return self.call(function, args);
        }
        let id = self.resolve_host(name, index, span)?;
        self.invoke_host(id, args, span)
    }

    fn resolve_host(
        &mut self,
        path: &str,
        index: usize,
        span: Span,
    ) -> Result<HostFunctionId, RuntimeError> {
        if let Some(Some(id)) = self.host_cache.get(index) {
            return Ok(*id);
        }
        let id = self
            .host
            .resolve(path)
            .ok_or_else(|| RuntimeError::reference(format!("{path} is not defined"), span))?;
        if let Some(slot) = self.host_cache.get_mut(index) {
            *slot = Some(id);
        }
        Ok(id)
    }

    fn invoke_host(
        &mut self,
        id: HostFunctionId,
        args: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let mut call = HostCall::new(&mut *self.heap, args);
        self.host
            .invoke(id, &mut call)
            .map_err(|error| RuntimeError::runtime(error.message, span))
    }

    fn object_id(&self, value: Value, span: Span) -> Result<ObjectId, RuntimeError> {
        value
            .object_offset()
            .map(ObjectId::new)
            .ok_or_else(|| RuntimeError::type_error("value is not an object", span))
    }

    fn index_value(&self, value: Value, span: Span) -> Result<u32, RuntimeError> {
        if let Some(index) = value.as_smi()
            && index >= 0
        {
            return Ok(index as u32);
        }
        if let Some(number) = self.heap.as_number(value)
            && number >= 0.0
            && number.fract() == 0.0
            && number <= u32::MAX as f64
        {
            return Ok(number as u32);
        }
        if let Some(text) = self.heap.as_str(value)
            && let Ok(index) = text.parse::<u32>()
        {
            return Ok(index);
        }
        Err(RuntimeError::type_error("invalid array index", span))
    }

    fn get_property(
        &mut self,
        object: Value,
        key: &str,
        span: Span,
    ) -> Result<Value, RuntimeError> {
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
        let shape = self
            .objects
            .shape(id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let key = PropertyKey::from_string(key);
        let location = self
            .shapes
            .lookup(shape, &key)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            .map(|descriptor| descriptor.location);
        match location {
            Some(PropertyLocation::InObject(slot)) => self
                .objects
                .get_slot(id, slot)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span)),
            Some(PropertyLocation::PropertyArray(index)) => self
                .objects
                .get_property_array(id, index)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span)),
            None => Ok(Value::undefined()),
        }
    }

    fn set_property(
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

    fn add(&mut self, left: Value, right: Value, span: Span) -> Result<Value, RuntimeError> {
        if left.is_string() || right.is_string() {
            let text = format!(
                "{}{}",
                self.heap.to_js_string(left),
                self.heap.to_js_string(right)
            );
            return Ok(self.heap.string(&text));
        }
        let left = self.number(left, span)?;
        let right = self.number(right, span)?;
        Ok(self.heap.number(left + right))
    }

    fn number(&self, value: Value, span: Span) -> Result<f64, RuntimeError> {
        self.heap
            .to_number(value)
            .filter(|number| !number.is_nan())
            .ok_or_else(|| RuntimeError::type_error("value is not a number", span))
    }

    fn compare(&self, op: Op, left: Value, right: Value, span: Span) -> Result<bool, RuntimeError> {
        let ordering =
            if let (Some(left), Some(right)) = (self.heap.as_str(left), self.heap.as_str(right)) {
                left.partial_cmp(right).map(std::cmp::Ordering::is_lt)
            } else {
                let left = self.number(left, span)?;
                let right = self.number(right, span)?;
                Some(
                    left.partial_cmp(&right)
                        .map(std::cmp::Ordering::is_lt)
                        .unwrap_or(false),
                )
            };
        let less = ordering.unwrap_or(false);
        let equal =
            if let (Some(left), Some(right)) = (self.heap.as_str(left), self.heap.as_str(right)) {
                left == right
            } else {
                self.heap
                    .to_number(left)
                    .zip(self.heap.to_number(right))
                    .map(|(left, right)| left == right)
                    .unwrap_or(false)
            };
        Ok(match op {
            Op::Lt => less,
            Op::Le => less || equal,
            Op::Gt => !less && !equal,
            Op::Ge => !less,
            _ => unreachable!(),
        })
    }

    fn loose_eq(&self, left: Value, right: Value) -> bool {
        if left.is_null() && right.is_undefined() || left.is_undefined() && right.is_null() {
            return true;
        }
        if left.is_number() && right.is_string() {
            return self
                .heap
                .to_number(left)
                .zip(self.heap.to_number(right))
                .map(|(left, right)| left == right)
                .unwrap_or(false);
        }
        if left.is_string() && right.is_number() {
            return self.loose_eq(right, left);
        }
        if left.as_boolean().is_some() || right.as_boolean().is_some() {
            return self
                .heap
                .to_number(left)
                .zip(self.heap.to_number(right))
                .map(|(left, right)| left == right)
                .unwrap_or(false);
        }
        self.heap.strict_eq(left, right)
    }
}

fn push(stack: &mut Vec<Value>, value: Value, span: Span) -> Result<(), RuntimeError> {
    if stack.len() >= MAX_STACK {
        return Err(RuntimeError::range("operand stack overflow", span));
    }
    stack.push(value);
    Ok(())
}

fn pop(stack: &mut Vec<Value>, span: Span) -> Result<Value, RuntimeError> {
    stack.pop().ok_or_else(|| {
        RuntimeError::new(RuntimeErrorKind::Runtime, "operand stack underflow", span)
    })
}

fn read_u8(code: &[u8], pc: &mut usize, span: Span) -> Result<u8, RuntimeError> {
    let byte = *code
        .get(*pc)
        .ok_or_else(|| RuntimeError::new(RuntimeErrorKind::Runtime, "truncated bytecode", span))?;
    *pc += 1;
    Ok(byte)
}

fn read_u16(code: &[u8], pc: &mut usize, span: Span) -> Result<u16, RuntimeError> {
    let bytes = read_array::<2>(code, pc, span)?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_u32(code: &[u8], pc: &mut usize, span: Span) -> Result<u32, RuntimeError> {
    let bytes = read_array::<4>(code, pc, span)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_array<const N: usize>(
    code: &[u8],
    pc: &mut usize,
    span: Span,
) -> Result<[u8; N], RuntimeError> {
    let end = pc
        .checked_add(N)
        .ok_or_else(|| RuntimeError::new(RuntimeErrorKind::Runtime, "truncated bytecode", span))?;
    let bytes = code
        .get(*pc..end)
        .ok_or_else(|| RuntimeError::new(RuntimeErrorKind::Runtime, "truncated bytecode", span))?;
    *pc = end;
    Ok(bytes.try_into().expect("slice length"))
}
