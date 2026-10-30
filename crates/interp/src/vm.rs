mod builtins;
mod calls;
mod properties;

use kagura_bytecode::{Const, Module, Op};
use kagura_object::{ObjectHeap, ObjectId, PropertyLocation, ShapeId, ShapeTable};
use kagura_support::{FastHashMap, Span};
use kagura_value::{Heap, Value};
use tracing::instrument;

use crate::error::{RuntimeError, RuntimeErrorKind};
use crate::host::{Host, HostFunctionId};

const MAX_CALL_DEPTH: usize = 256;
const MAX_STACK: usize = 4096;
const GC_THRESHOLD: u32 = 32 * 1024;

#[derive(Clone, Copy, Debug)]
struct PropertyCacheEntry {
    shape: ShapeId,
    holder: ObjectId,
    holder_shape: ShapeId,
    location: PropertyLocation,
    writable: bool,
}

#[derive(Clone, Copy, Debug, Default)]
enum PropertyFeedback {
    #[default]
    Empty,
    Monomorphic(PropertyCacheEntry),
    Polymorphic {
        entries: [Option<PropertyCacheEntry>; 4],
        len: u8,
    },
    Megamorphic,
}

impl PropertyFeedback {
    fn lookup(self, shape: ShapeId) -> Option<PropertyCacheEntry> {
        match self {
            Self::Empty | Self::Megamorphic => None,
            Self::Monomorphic(entry) => (entry.shape == shape).then_some(entry),
            Self::Polymorphic { entries, len } => entries[..len as usize]
                .iter()
                .flatten()
                .find(|entry| entry.shape == shape)
                .copied(),
        }
    }

    fn record(&mut self, entry: PropertyCacheEntry) {
        match self {
            Self::Empty => *self = Self::Monomorphic(entry),
            Self::Monomorphic(current) => {
                if current.shape == entry.shape {
                    *current = entry;
                } else {
                    let mut entries = [None; 4];
                    entries[0] = Some(*current);
                    entries[1] = Some(entry);
                    *self = Self::Polymorphic { entries, len: 2 };
                }
            }
            Self::Polymorphic { entries, len } => {
                if let Some(slot) = entries[..*len as usize]
                    .iter_mut()
                    .find(|slot| slot.is_some_and(|cached| cached.shape == entry.shape))
                {
                    *slot = Some(entry);
                } else if *len < 4 {
                    entries[*len as usize] = Some(entry);
                    *len += 1;
                } else {
                    *self = Self::Megamorphic;
                }
            }
            Self::Megamorphic => {}
        }
    }
}

/// Snapshot of the interpreter frame at a GC safepoint.
#[derive(Clone, Debug, Default)]
pub struct FrameDescriptor {
    pub function_index: usize,
    pub pc: usize,
    pub param_count: u16,
    pub local_count: u16,
    pub defined_locals: u16,
    pub operand_depth: u32,
}

/// Observability for interpreter safepoints.
#[derive(Clone, Debug, Default)]
pub struct SafepointStats {
    pub count: u64,
    pub max_operand_depth: u32,
    pub last: Option<FrameDescriptor>,
}

struct FrameContext<'a> {
    caller_roots: &'a [Value],
    args: &'a [Value],
    env: Option<ObjectId>,
    closure_env: Option<ObjectId>,
    self_value: Option<Value>,
}

struct IterationFrame {
    parent: Option<ObjectId>,
    env: ObjectId,
}

struct TryHandler {
    catch_pc: usize,
    stack_depth: usize,
}

pub struct Interpreter<'a> {
    module: &'a Module,
    heap: &'a mut Heap,
    objects: &'a mut ObjectHeap,
    shapes: &'a mut ShapeTable,
    external_roots: &'a [Value],
    host: &'a mut dyn Host,
    host_cache: Vec<Option<HostFunctionId>>,
    builtin_cache: FastHashMap<String, Value>,
    feedback: Vec<PropertyFeedback>,
    property_feedback: Vec<Vec<PropertyFeedback>>,
    property_ic_hits: u64,
    property_ic_misses: u64,
    safepoint_stats: SafepointStats,
    rng_state: u64,
    global_object: Option<ObjectId>,
    array_prototype: Option<ObjectId>,
    object_prototype: Option<ObjectId>,
    depth: usize,
    step_limit: u64,
}

impl<'a> Interpreter<'a> {
    pub fn new(
        module: &'a Module,
        heap: &'a mut Heap,
        objects: &'a mut ObjectHeap,
        shapes: &'a mut ShapeTable,
        external_roots: &'a [Value],
        host: &'a mut dyn Host,
        step_limit: u64,
    ) -> Self {
        Self {
            module,
            heap,
            objects,
            shapes,
            external_roots,
            host,
            host_cache: vec![None; module.constants.len()],
            builtin_cache: FastHashMap::default(),
            feedback: vec![PropertyFeedback::Empty; module.feedback.len()],
            property_feedback: module
                .functions
                .iter()
                .map(|function| vec![PropertyFeedback::Empty; function.code.len()])
                .collect(),
            property_ic_hits: 0,
            property_ic_misses: 0,
            safepoint_stats: SafepointStats::default(),
            rng_state: 0x9E37_79B9_7F4A_7C15,
            global_object: None,
            array_prototype: None,
            object_prototype: None,
            depth: 0,
            step_limit,
        }
    }

    pub fn property_ic_stats(&self) -> (u64, u64) {
        (self.property_ic_hits, self.property_ic_misses)
    }

    pub fn safepoint_stats(&self) -> SafepointStats {
        self.safepoint_stats.clone()
    }

    #[instrument(skip(self), fields(entry = self.module.entry))]
    pub fn run(&mut self) -> Result<Value, RuntimeError> {
        let entry = self.module.entry;
        let global = self.global_object(Span::empty(0))?;
        self.call(entry, &[], &[], None, None, Some(global))
    }

    #[instrument(skip(self, locals, defined, frame), fields(function = function_index))]
    fn execute(
        &mut self,
        function_index: usize,
        function: &kagura_bytecode::FunctionProto,
        locals: &mut [Value],
        defined: &mut [bool],
        frame: &FrameContext<'_>,
    ) -> Result<Value, RuntimeError> {
        let caller_roots = frame.caller_roots;
        let env = frame.env;
        let closure_env = frame.closure_env;
        let self_value = frame.self_value;
        let mut capture_env = env;
        let mut iteration_frames: Vec<IterationFrame> = Vec::new();
        let mut handlers: Vec<TryHandler> = Vec::new();
        let mut stack: Vec<Value> = Vec::new();
        let mut pc = 0usize;
        let mut steps = 0u64;
        macro_rules! try_op {
            ($expr:expr) => {
                match $expr {
                    Ok(value) => value,
                    Err(error) => {
                        let catch_pc = self.catch_error(&mut handlers, &mut stack, error)?;
                        pc = catch_pc;
                        continue;
                    }
                }
            };
        }
        loop {
            let span = self.module.span_at(function_index, pc);
            steps += 1;
            if steps > self.step_limit {
                return Err(RuntimeError::range("execution step limit exceeded", span));
            }
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
                Op::LoadHole => {
                    push(&mut stack, Value::hole(), span)?;
                }
                Op::LoadThis => {
                    push(
                        &mut stack,
                        self_value.unwrap_or_else(Value::undefined),
                        span,
                    )?;
                }
                Op::Rest => {
                    let start = read_u16(&function.code, &mut pc, span)? as usize;
                    let array = self
                        .objects
                        .allocate_array(self.shapes.root())
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    for value in frame.args.iter().skip(start) {
                        self.objects
                            .array_push(array, *value)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                    push(&mut stack, Value::object(array.index()), span)?;
                }
                Op::Arguments => {
                    let array = self
                        .objects
                        .allocate_array(self.shapes.root())
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    for value in frame.args {
                        self.objects
                            .array_push(array, *value)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                    push(&mut stack, Value::object(array.index()), span)?;
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
                    let value = match env {
                        Some(env) => self
                            .objects
                            .array_get(env, slot as u32)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?,
                        None => locals[slot],
                    };
                    push(&mut stack, value, span)?;
                }
                Op::StoreLocal => {
                    let slot = read_u16(&function.code, &mut pc, span)? as usize;
                    if slot >= locals.len() {
                        return Err(RuntimeError::reference("invalid local slot", span));
                    }
                    let value = pop(&mut stack, span)?;
                    if let Some(env) = env {
                        self.objects
                            .array_set(env, slot as u32, value)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                    locals[slot] = value;
                    defined[slot] = true;
                }
                Op::Add => {
                    let right = pop(&mut stack, span)?;
                    let left = pop(&mut stack, span)?;
                    let value = try_op!(self.add(left, right, span));
                    push(&mut stack, value, span)?;
                }
                Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Pow => {
                    let right = pop(&mut stack, span)?;
                    let left = pop(&mut stack, span)?;
                    let left = self.heap.to_number(left).unwrap_or(f64::NAN);
                    let right = self.heap.to_number(right).unwrap_or(f64::NAN);
                    let value = match op {
                        Op::Sub => left - right,
                        Op::Mul => left * right,
                        Op::Div => left / right,
                        Op::Mod => left % right,
                        Op::Pow => left.powf(right),
                        _ => unreachable!(),
                    };
                    push(&mut stack, self.heap.number(value), span)?;
                }
                Op::SpreadObject => {
                    let source = pop(&mut stack, span)?;
                    let target = *stack
                        .last()
                        .ok_or_else(|| RuntimeError::runtime("operand stack underflow", span))?;
                    try_op!(self.copy_spread_properties(target, source, span));
                }
                Op::SpreadArray => {
                    let source = pop(&mut stack, span)?;
                    let target = *stack
                        .last()
                        .ok_or_else(|| RuntimeError::runtime("operand stack underflow", span))?;
                    try_op!(self.spread_array_into(target, source, span));
                }
                Op::ArrayRest => {
                    let start = pop(&mut stack, span)?;
                    let source = pop(&mut stack, span)?;
                    let start = try_op!(self.number(start, span)) as u32;
                    let value = try_op!(self.array_rest(source, start, span));
                    push(&mut stack, value, span)?;
                }
                Op::ObjectRest => {
                    let index = read_u32(&function.code, &mut pc, span)? as usize;
                    let excluded = match function
                        .code
                        .get(index)
                        .and_then(|_| self.module.constants.get(index))
                    {
                        Some(Const::String(keys)) => {
                            if keys.is_empty() {
                                Vec::new()
                            } else {
                                keys.split('\0').map(str::to_owned).collect()
                            }
                        }
                        _ => return Err(RuntimeError::runtime("invalid object rest keys", span)),
                    };
                    let source = pop(&mut stack, span)?;
                    let value = try_op!(self.object_rest(source, &excluded, span));
                    push(&mut stack, value, span)?;
                }
                Op::Await => {
                    let value = pop(&mut stack, span)?;
                    let resolved = if value.object_offset().is_some() {
                        let resolved = try_op!(self.get_property(value, "__resolved", span));
                        if self.heap.truthy(resolved) {
                            try_op!(self.get_property(value, "__value", span))
                        } else {
                            value
                        }
                    } else {
                        value
                    };
                    push(&mut stack, resolved, span)?;
                }
                Op::BitOr | Op::BitXor | Op::BitAnd | Op::Shl | Op::Shr | Op::UShr => {
                    let right = pop(&mut stack, span)?;
                    let left = pop(&mut stack, span)?;
                    let left = to_int32(self.heap.to_number(left).unwrap_or(f64::NAN));
                    let right = to_int32(self.heap.to_number(right).unwrap_or(f64::NAN));
                    let value = match op {
                        Op::BitOr => (left | right) as f64,
                        Op::BitXor => (left ^ right) as f64,
                        Op::BitAnd => (left & right) as f64,
                        Op::Shl => left.wrapping_shl((right as u32) & 31) as f64,
                        Op::Shr => left.wrapping_shr((right as u32) & 31) as f64,
                        Op::UShr => ((left as u32) >> ((right as u32) & 31)) as f64,
                        _ => unreachable!(),
                    };
                    push(&mut stack, self.heap.number(value), span)?;
                }
                Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                    let right = pop(&mut stack, span)?;
                    let left = pop(&mut stack, span)?;
                    let value = try_op!(self.compare(op, left, right, span));
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
                Op::In => {
                    let target = pop(&mut stack, span)?;
                    let key = pop(&mut stack, span)?;
                    let found = try_op!(self.has_property(key, target, span));
                    push(&mut stack, Value::boolean(found), span)?;
                }
                Op::ForInKeys => {
                    let value = pop(&mut stack, span)?;
                    let keys = try_op!(self.for_in_keys(value, span));
                    push(&mut stack, keys, span)?;
                }
                Op::InstanceOf => {
                    let constructor = pop(&mut stack, span)?;
                    let value = pop(&mut stack, span)?;
                    let result = try_op!(self.instance_of(value, constructor, span));
                    push(&mut stack, Value::boolean(result), span)?;
                }
                Op::TypeOf => {
                    let value = pop(&mut stack, span)?;
                    let type_name = if value.is_undefined() || value.is_hole() {
                        "undefined"
                    } else if value.is_null() {
                        "object"
                    } else if value.as_boolean().is_some() {
                        "boolean"
                    } else if value.is_number() {
                        "number"
                    } else if value.is_string() {
                        "string"
                    } else if value.is_object()
                        && value
                            .object_offset()
                            .map(ObjectId::new)
                            .and_then(|object| self.objects.is_function(object).ok())
                            .unwrap_or(false)
                    {
                        "function"
                    } else {
                        "object"
                    };
                    let value = self.heap.string(type_name);
                    push(&mut stack, value, span)?;
                }
                Op::Neg => {
                    let value = pop(&mut stack, span)?;
                    let number = self.heap.to_number(value).unwrap_or(f64::NAN);
                    push(&mut stack, self.heap.number(-number), span)?;
                }
                Op::BitNot => {
                    let value = pop(&mut stack, span)?;
                    let number = to_int32(self.heap.to_number(value).unwrap_or(f64::NAN));
                    push(&mut stack, self.heap.number((!number) as f64), span)?;
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
                Op::LoadCapture => {
                    let level = read_u16(&function.code, &mut pc, span)?;
                    let slot = read_u16(&function.code, &mut pc, span)?;
                    let env = closure_env.ok_or_else(|| {
                        RuntimeError::reference("capture used without a closure", span)
                    })?;
                    let env = self.capture_env(env, level, span)?;
                    let value = self
                        .objects
                        .array_get_raw(env, slot as u32)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    if value.is_hole() {
                        return Err(RuntimeError::reference(
                            "cannot access captured variable before initialization",
                            span,
                        ));
                    }
                    push(&mut stack, value, span)?;
                }
                Op::StoreCapture => {
                    let level = read_u16(&function.code, &mut pc, span)?;
                    let slot = read_u16(&function.code, &mut pc, span)?;
                    let env = closure_env.ok_or_else(|| {
                        RuntimeError::reference("capture used without a closure", span)
                    })?;
                    let env = self.capture_env(env, level, span)?;
                    let value = pop(&mut stack, span)?;
                    self.objects
                        .array_set(env, slot as u32, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                Op::ToPropertyKey => {
                    let value = pop(&mut stack, span)?;
                    let key = try_op!(self.property_key(value, span));
                    push(&mut stack, self.heap.string(&key), span)?;
                }
                Op::LoadGlobal => {
                    let index = read_u32(&function.code, &mut pc, span)?;
                    let name = match self.module.constants.get(index as usize) {
                        Some(Const::String(name)) => name.clone(),
                        _ => return Err(RuntimeError::runtime("invalid global name", span)),
                    };
                    let value = if let Some(global) = self.global_object {
                        let target = Value::object(global.index());
                        let key = self.heap.string(&name);
                        if !try_op!(self.has_property(key, target, span)) {
                            return Err(RuntimeError::reference(
                                format!("{name} is not defined"),
                                span,
                            ));
                        }
                        try_op!(self.get_property(target, &name, span))
                    } else {
                        Value::undefined()
                    };
                    push(&mut stack, value, span)?;
                }
                Op::StoreGlobal => {
                    let index = read_u32(&function.code, &mut pc, span)?;
                    let name = match self.module.constants.get(index as usize) {
                        Some(Const::String(name)) => name.clone(),
                        _ => return Err(RuntimeError::runtime("invalid global name", span)),
                    };
                    let value = pop(&mut stack, span)?;
                    let Some(global) = self.global_object else {
                        return Err(RuntimeError::runtime("global object is missing", span));
                    };
                    try_op!(self.set_property(Value::object(global.index()), &name, value, span));
                    push(&mut stack, value, span)?;
                }
                Op::LoadBuiltin => {
                    let index = read_u32(&function.code, &mut pc, span)?;
                    let name = match self.module.constants.get(index as usize) {
                        Some(Const::String(name)) => name.clone(),
                        _ => return Err(RuntimeError::runtime("invalid builtin name", span)),
                    };
                    let value = try_op!(self.load_builtin(&name, span));
                    push(&mut stack, value, span)?;
                }
                Op::ThrowReference => {
                    let index = read_u32(&function.code, &mut pc, span)?;
                    let name = match self.module.constants.get(index as usize) {
                        Some(Const::String(name)) => name.clone(),
                        _ => return Err(RuntimeError::runtime("invalid identifier", span)),
                    };
                    return Err(RuntimeError::reference(
                        format!("{name} is not defined"),
                        span,
                    ));
                }
                Op::NewName => {
                    let index = read_u32(&function.code, &mut pc, span)?;
                    let argc = read_u8(&function.code, &mut pc, span)? as usize;
                    let name = match self.module.constants.get(index as usize) {
                        Some(Const::String(name)) => name.clone(),
                        _ => return Err(RuntimeError::runtime("invalid constructor name", span)),
                    };
                    let mut args = Vec::with_capacity(argc);
                    for _ in 0..argc {
                        args.push(pop(&mut stack, span)?);
                    }
                    args.reverse();
                    let value = try_op!(self.new_name(&name, &args, span));
                    push(&mut stack, value, span)?;
                }
                Op::NewValue => {
                    let argc = read_u8(&function.code, &mut pc, span)? as usize;
                    let mut args = Vec::with_capacity(argc);
                    for _ in 0..argc {
                        args.push(pop(&mut stack, span)?);
                    }
                    args.reverse();
                    let callee = pop(&mut stack, span)?;
                    let mut roots =
                        Vec::with_capacity(caller_roots.len() + locals.len() + stack.len() + 3);
                    roots.extend_from_slice(caller_roots);
                    for (value, is_defined) in locals.iter().zip(defined.iter()) {
                        if *is_defined {
                            roots.push(*value);
                        }
                    }
                    roots.extend_from_slice(&stack);
                    roots.push(callee);
                    roots.extend_from_slice(&args);
                    if let Some(env) = env {
                        roots.push(Value::object(env.index()));
                    }
                    let value = try_op!(self.construct(callee, &args, span, &roots));
                    push(&mut stack, value, span)?;
                }
                Op::CallThis => {
                    let argc = read_u8(&function.code, &mut pc, span)? as usize;
                    let mut args = Vec::with_capacity(argc);
                    for _ in 0..argc {
                        args.push(pop(&mut stack, span)?);
                    }
                    args.reverse();
                    let receiver = pop(&mut stack, span)?;
                    let callee = pop(&mut stack, span)?;
                    let mut roots =
                        Vec::with_capacity(caller_roots.len() + locals.len() + stack.len() + 3);
                    roots.extend_from_slice(caller_roots);
                    for (value, is_defined) in locals.iter().zip(defined.iter()) {
                        if *is_defined {
                            roots.push(*value);
                        }
                    }
                    roots.extend_from_slice(&stack);
                    roots.push(callee);
                    roots.push(receiver);
                    roots.extend_from_slice(&args);
                    if let Some(env) = env {
                        roots.push(Value::object(env.index()));
                    }
                    if let Some(closure_env) = closure_env {
                        roots.push(Value::object(closure_env.index()));
                    }
                    if let Some(capture_env) = capture_env {
                        roots.push(Value::object(capture_env.index()));
                    }
                    let value = match self.call_with_this(callee, receiver, &args, &roots, span) {
                        Ok(value) => value,
                        Err(error) => {
                            let catch_pc = self.catch_error(&mut handlers, &mut stack, error)?;
                            pc = catch_pc;
                            continue;
                        }
                    };
                    push(&mut stack, value, span)?;
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
                    let mut roots =
                        Vec::with_capacity(caller_roots.len() + locals.len() + stack.len() + 2);
                    roots.extend_from_slice(caller_roots);
                    for (value, is_defined) in locals.iter().zip(defined.iter()) {
                        if *is_defined {
                            roots.push(*value);
                        }
                    }
                    roots.extend_from_slice(&stack);
                    if let Some(env) = env {
                        roots.push(Value::object(env.index()));
                    }
                    if let Some(closure_env) = closure_env {
                        roots.push(Value::object(closure_env.index()));
                    }
                    if let Some(capture_env) = capture_env {
                        roots.push(Value::object(capture_env.index()));
                    }
                    let value = match self.call_name(&name, index as usize, &args, span, &roots) {
                        Ok(value) => value,
                        Err(error) => {
                            let catch_pc = self.catch_error(&mut handlers, &mut stack, error)?;
                            pc = catch_pc;
                            continue;
                        }
                    };
                    push(&mut stack, value, span)?;
                }
                Op::TryBegin => {
                    let catch_pc = read_u32(&function.code, &mut pc, span)? as usize;
                    handlers.push(TryHandler {
                        catch_pc,
                        stack_depth: stack.len(),
                    });
                }
                Op::TryEnd => {
                    handlers.pop();
                }
                Op::Throw => {
                    let value = pop(&mut stack, span)?;
                    if let Some(handler) = handlers.pop() {
                        stack.truncate(handler.stack_depth);
                        push(&mut stack, value, span)?;
                        pc = handler.catch_pc;
                    } else {
                        return Err(RuntimeError::thrown(value, span));
                    }
                }
                Op::IterationBegin => {
                    let count = read_u8(&function.code, &mut pc, span)? as usize;
                    let mut sources = Vec::with_capacity(count);
                    for _ in 0..count {
                        sources.push(read_u16(&function.code, &mut pc, span)?);
                    }
                    let source_env = env.ok_or_else(|| {
                        RuntimeError::runtime("iteration without an environment", span)
                    })?;
                    let frame_env = self
                        .objects
                        .allocate_array(self.shapes.root())
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    for source in sources {
                        let value = if source == u16::MAX {
                            Value::hole()
                        } else {
                            self.objects
                                .array_get_raw(source_env, source as u32)
                                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                        };
                        self.objects
                            .array_push(frame_env, value)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                    let parent = capture_env
                        .map(|env| Value::object(env.index()))
                        .unwrap_or_else(Value::undefined);
                    self.objects
                        .array_push(frame_env, parent)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    iteration_frames.push(IterationFrame {
                        parent: capture_env,
                        env: frame_env,
                    });
                    capture_env = Some(frame_env);
                }
                Op::IterationNext => {
                    let count = read_u8(&function.code, &mut pc, span)? as usize;
                    let Some(frame) = iteration_frames.last() else {
                        return Err(RuntimeError::runtime("iteration is not active", span));
                    };
                    let old_env = frame.env;
                    let parent = frame.parent;
                    let next = self
                        .objects
                        .allocate_array(self.shapes.root())
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    for slot in 0..count {
                        let value = self
                            .objects
                            .array_get_raw(old_env, slot as u32)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                        self.objects
                            .array_push(next, value)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    }
                    let parent_link = parent
                        .map(|env| Value::object(env.index()))
                        .unwrap_or_else(Value::undefined);
                    self.objects
                        .array_push(next, parent_link)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    if let Some(frame) = iteration_frames.last_mut() {
                        frame.env = next;
                    }
                    capture_env = Some(next);
                }
                Op::IterationEnd => {
                    let frame = iteration_frames
                        .pop()
                        .ok_or_else(|| RuntimeError::runtime("iteration is not active", span))?;
                    capture_env = frame.parent;
                }
                Op::LoadIter => {
                    let depth = read_u16(&function.code, &mut pc, span)? as usize;
                    let slot = read_u16(&function.code, &mut pc, span)?;
                    let Some(index) = iteration_frames.len().checked_sub(depth + 1) else {
                        return Err(RuntimeError::runtime("iteration is not active", span));
                    };
                    let frame = &iteration_frames[index];
                    let value = self
                        .objects
                        .array_get_raw(frame.env, slot as u32)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    push(&mut stack, value, span)?;
                }
                Op::StoreIter => {
                    let depth = read_u16(&function.code, &mut pc, span)? as usize;
                    let slot = read_u16(&function.code, &mut pc, span)?;
                    let Some(index) = iteration_frames.len().checked_sub(depth + 1) else {
                        return Err(RuntimeError::runtime("iteration is not active", span));
                    };
                    let frame = &iteration_frames[index];
                    let value = pop(&mut stack, span)?;
                    self.objects
                        .array_set(frame.env, slot as u32, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                Op::NewClosure => {
                    let function = read_u32(&function.code, &mut pc, span)?;
                    let env = capture_env.ok_or_else(|| {
                        RuntimeError::runtime("closure created without an environment", span)
                    })?;
                    let lexical_this = self
                        .module
                        .functions
                        .get(function as usize)
                        .is_some_and(|proto| proto.name == "<arrow>");
                    let captured_this = if lexical_this {
                        self_value.unwrap_or_else(Value::undefined)
                    } else {
                        Value::undefined()
                    };
                    let object = self
                        .objects
                        .allocate_function(
                            self.shapes.function_shape(),
                            function,
                            Value::object(env.index()),
                            captured_this,
                        )
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let function_value = Value::object(object.index());
                    if !lexical_this {
                        let proto_shape = try_op!(self.object_shape(span));
                        let prototype = self
                            .objects
                            .allocate(proto_shape)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                        let prototype = Value::object(prototype.index());
                        try_op!(self.set_non_enumerable_property(
                            function_value,
                            "prototype",
                            prototype,
                            span
                        ));
                        try_op!(self.set_non_enumerable_property(
                            prototype,
                            "constructor",
                            function_value,
                            span
                        ));
                    }
                    push(&mut stack, function_value, span)?;
                }
                Op::CallValue => {
                    let argc = read_u8(&function.code, &mut pc, span)? as usize;
                    let mut args = Vec::with_capacity(argc);
                    for _ in 0..argc {
                        args.push(pop(&mut stack, span)?);
                    }
                    args.reverse();
                    let callee = pop(&mut stack, span)?;
                    let mut roots =
                        Vec::with_capacity(caller_roots.len() + locals.len() + stack.len() + 2);
                    roots.extend_from_slice(caller_roots);
                    for (value, is_defined) in locals.iter().zip(defined.iter()) {
                        if *is_defined {
                            roots.push(*value);
                        }
                    }
                    roots.extend_from_slice(&stack);
                    if let Some(env) = env {
                        roots.push(Value::object(env.index()));
                    }
                    if let Some(closure_env) = closure_env {
                        roots.push(Value::object(closure_env.index()));
                    }
                    if let Some(capture_env) = capture_env {
                        roots.push(Value::object(capture_env.index()));
                    }
                    let value = match self.invoke_callable(callee, &args, &roots, span) {
                        Ok(value) => value,
                        Err(error) => {
                            let catch_pc = self.catch_error(&mut handlers, &mut stack, error)?;
                            pc = catch_pc;
                            continue;
                        }
                    };
                    push(&mut stack, value, span)?;
                }
                Op::CallMethod => {
                    let index = read_u32(&function.code, &mut pc, span)?;
                    let argc = read_u8(&function.code, &mut pc, span)? as usize;
                    let key = match self.module.constants.get(index as usize) {
                        Some(Const::String(key)) => key.clone(),
                        _ => return Err(RuntimeError::runtime("invalid method name", span)),
                    };
                    let mut args = Vec::with_capacity(argc);
                    for _ in 0..argc {
                        args.push(pop(&mut stack, span)?);
                    }
                    args.reverse();
                    let object = pop(&mut stack, span)?;
                    let mut roots =
                        Vec::with_capacity(caller_roots.len() + locals.len() + stack.len() + 4);
                    roots.extend_from_slice(caller_roots);
                    for (value, is_defined) in locals.iter().zip(defined.iter()) {
                        if *is_defined {
                            roots.push(*value);
                        }
                    }
                    roots.extend_from_slice(&stack);
                    if let Some(env) = env {
                        roots.push(Value::object(env.index()));
                    }
                    if let Some(closure_env) = closure_env {
                        roots.push(Value::object(closure_env.index()));
                    }
                    if let Some(capture_env) = capture_env {
                        roots.push(Value::object(capture_env.index()));
                    }
                    roots.push(object);
                    roots.extend_from_slice(&args);
                    if object.is_object()
                        && (key == "call" || key == "apply")
                        && self
                            .objects
                            .is_function(ObjectId::new(object.object_offset().unwrap_or(0)))
                            .unwrap_or(false)
                    {
                        let this_value = args.first().copied().unwrap_or_else(Value::undefined);
                        let call_args = if key == "call" {
                            args.get(1..).unwrap_or(&[]).to_vec()
                        } else {
                            let array = args.get(1).copied().unwrap_or_else(Value::undefined);
                            if array.is_undefined() {
                                Vec::new()
                            } else {
                                let array_id = try_op!(self.object_id(array, span));
                                let length =
                                    try_op!(self.objects.array_length(array_id).map_err(|error| {
                                        RuntimeError::runtime(error.to_string(), span)
                                    }));
                                let mut values = Vec::with_capacity(length as usize);
                                for index in 0..length {
                                    values.push(try_op!(
                                        self.objects.array_get(array_id, index).map_err(|error| {
                                            RuntimeError::runtime(error.to_string(), span)
                                        })
                                    ));
                                }
                                values
                            }
                        };
                        let value = try_op!(
                            self.call_with_this(object, this_value, &call_args, &roots, span)
                        );
                        push(&mut stack, value, span)?;
                        continue;
                    }
                    let method = if object.is_object() {
                        try_op!(self.get_property(object, &key, span))
                    } else {
                        Value::undefined()
                    };
                    let value = if method.is_object()
                        && self
                            .objects
                            .is_function(ObjectId::new(method.object_offset().unwrap_or(0)))
                            .unwrap_or(false)
                    {
                        let method_id = ObjectId::new(method.object_offset().unwrap_or(0));
                        let (function, _) = try_op!(
                            self.objects
                                .function_data(method_id)
                                .map_err(|error| RuntimeError::runtime(error.to_string(), span))
                        );
                        if function == u32::MAX {
                            try_op!(
                                self.call_builtin(object, &key, &args, span, &roots)
                                    .and_then(|value| value.ok_or_else(|| {
                                        RuntimeError::type_error("method is not supported", span)
                                    }))
                            )
                        } else {
                            try_op!(self.invoke_method(method, &args, &roots, object, span))
                        }
                    } else {
                        try_op!(
                            self.call_builtin(object, &key, &args, span, &roots)
                                .and_then(|value| value.ok_or_else(|| {
                                    RuntimeError::type_error("method is not supported", span)
                                }))
                        )
                    };
                    push(&mut stack, value, span)?;
                }
                Op::CallBuiltin => {
                    let id = read_u16(&function.code, &mut pc, span)?;
                    let argc = read_u8(&function.code, &mut pc, span)? as usize;
                    let mut args = Vec::with_capacity(argc);
                    for _ in 0..argc {
                        args.push(pop(&mut stack, span)?);
                    }
                    args.reverse();
                    let id = kagura_bytecode::builtins::BuiltinId::from_u16(id)
                        .ok_or_else(|| RuntimeError::runtime("unknown builtin id", span))?;
                    let value = try_op!(self.call_builtin_id(id, &args, span));
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
                    if path == "assert.throws" {
                        let mut roots =
                            Vec::with_capacity(caller_roots.len() + locals.len() + stack.len() + 2);
                        roots.extend_from_slice(caller_roots);
                        for (value, is_defined) in locals.iter().zip(defined.iter()) {
                            if *is_defined {
                                roots.push(*value);
                            }
                        }
                        roots.extend_from_slice(&stack);
                        if let Some(env) = env {
                            roots.push(Value::object(env.index()));
                        }
                        let value = try_op!(self.assert_throws(&args, span, &roots));
                        push(&mut stack, value, span)?;
                        continue;
                    }
                    if path == "assert.compareArray" {
                        let value = try_op!(self.assert_compare_array(&args, span));
                        push(&mut stack, value, span)?;
                        continue;
                    }
                    if path == "assert.deepEqual" {
                        let value = try_op!(self.assert_deep_equal(&args, span));
                        push(&mut stack, value, span)?;
                        continue;
                    }
                    let dispatched = match self.resolve_host(&path, index as usize, span) {
                        Ok(id) => self.invoke_host(id, &args, span),
                        Err(host_error) => match self.call_global(&path, &args, span) {
                            Ok(Some(value)) => Ok(value),
                            Ok(None) => Err(host_error),
                            Err(error) => Err(error),
                        },
                    };
                    let value = match dispatched {
                        Ok(value) => value,
                        Err(error) => {
                            let catch_pc = self.catch_error(&mut handlers, &mut stack, error)?;
                            pc = catch_pc;
                            continue;
                        }
                    };
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
                    let value = match self.invoke_host(id, &args, span) {
                        Ok(value) => value,
                        Err(error) => {
                            let catch_pc = self.catch_error(&mut handlers, &mut stack, error)?;
                            pc = catch_pc;
                            continue;
                        }
                    };
                    push(&mut stack, value, span)?;
                }
                Op::NewObject => {
                    let shape = try_op!(self.object_shape(span));
                    let object = self
                        .objects
                        .allocate(shape)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    push(&mut stack, Value::object(object.index()), span)?;
                }
                Op::GetPropIC => {
                    let index = read_u32(&function.code, &mut pc, span)?;
                    let feedback = read_u32(&function.code, &mut pc, span)?;
                    let key = match self.module.constants.get(index as usize) {
                        Some(Const::String(key)) => key.clone(),
                        _ => {
                            return Err(RuntimeError::runtime("invalid property key", span));
                        }
                    };
                    let object = pop(&mut stack, span)?;
                    let value = try_op!(self.get_property_feedback(object, feedback, &key, span));
                    push(&mut stack, value, span)?;
                }
                Op::GetProp => {
                    let site = pc.saturating_sub(1) as u32;
                    let index = read_u32(&function.code, &mut pc, span)?;
                    let key = match self.module.constants.get(index as usize) {
                        Some(Const::String(key)) => key.clone(),
                        _ => {
                            return Err(RuntimeError::runtime("invalid property key", span));
                        }
                    };
                    let object = pop(&mut stack, span)?;
                    let value =
                        try_op!(self.get_property_ic(object, function_index, site, &key, span));
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
                    try_op!(self.set_property(object, &key, value, span));
                }
                Op::SetPropNonEnum => {
                    let index = read_u32(&function.code, &mut pc, span)?;
                    let key = match self.module.constants.get(index as usize) {
                        Some(Const::String(key)) => key.clone(),
                        _ => {
                            return Err(RuntimeError::runtime("invalid property key", span));
                        }
                    };
                    let value = pop(&mut stack, span)?;
                    let object = pop(&mut stack, span)?;
                    try_op!(self.set_non_enumerable_property(object, &key, value, span));
                }
                Op::SetPrototype => {
                    let prototype = pop(&mut stack, span)?;
                    let object = pop(&mut stack, span)?;
                    let object_id = self.object_id(object, span)?;
                    let shape = self
                        .objects
                        .shape(object_id)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    let prototype = prototype.object_offset().map(ObjectId::new);
                    let shape = self
                        .shapes
                        .with_prototype(shape, prototype)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    self.objects
                        .set_shape(object_id, shape)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                Op::SetPropValueIC => {
                    let index = read_u32(&function.code, &mut pc, span)?;
                    let feedback = read_u32(&function.code, &mut pc, span)?;
                    let key = match self.module.constants.get(index as usize) {
                        Some(Const::String(key)) => key.clone(),
                        _ => {
                            return Err(RuntimeError::runtime("invalid property key", span));
                        }
                    };
                    let value = pop(&mut stack, span)?;
                    let object = pop(&mut stack, span)?;
                    try_op!(self.set_property_feedback(object, feedback, &key, value, span));
                    push(&mut stack, value, span)?;
                }
                Op::SetPropValue => {
                    let site = pc.saturating_sub(1) as u32;
                    let index = read_u32(&function.code, &mut pc, span)?;
                    let key = match self.module.constants.get(index as usize) {
                        Some(Const::String(key)) => key.clone(),
                        _ => {
                            return Err(RuntimeError::runtime("invalid property key", span));
                        }
                    };
                    let value = pop(&mut stack, span)?;
                    let object = pop(&mut stack, span)?;
                    try_op!(self.set_property_ic(object, function_index, site, &key, value, span));
                    push(&mut stack, value, span)?;
                }
                Op::NewArray => {
                    let shape = try_op!(self.array_shape(span));
                    let object = self
                        .objects
                        .allocate_array(shape)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    push(&mut stack, Value::object(object.index()), span)?;
                }
                Op::ArrayPush => {
                    let value = pop(&mut stack, span)?;
                    let object = pop(&mut stack, span)?;
                    let id = try_op!(self.object_id(object, span));
                    try_op!(
                        self.objects
                            .array_push(id, value)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))
                    );
                    push(&mut stack, object, span)?;
                }
                Op::GetIndex => {
                    let index = pop(&mut stack, span)?;
                    let object = pop(&mut stack, span)?;
                    if let Some(text) = self.heap.as_str(object) {
                        let index = try_op!(self.index_value(index, span));
                        let value = text
                            .encode_utf16()
                            .nth(index as usize)
                            .and_then(|unit| String::from_utf16(&[unit]).ok())
                            .map(|unit| self.heap.string(&unit))
                            .unwrap_or_else(Value::undefined);
                        push(&mut stack, value, span)?;
                        continue;
                    }
                    let id = try_op!(self.object_id(object, span));
                    if try_op!(
                        self.objects
                            .is_array(id)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))
                    ) {
                        if let Some(index) = self.array_index(index) {
                            let value =
                                try_op!(self.objects.array_get(id, index).map_err(|error| {
                                    RuntimeError::runtime(error.to_string(), span)
                                }));
                            push(&mut stack, value, span)?;
                        } else {
                            let key = try_op!(self.property_key(index, span));
                            let value = try_op!(self.get_property(object, &key, span));
                            push(&mut stack, value, span)?;
                        }
                    } else {
                        let key = try_op!(self.property_key(index, span));
                        let value = try_op!(self.get_property(object, &key, span));
                        push(&mut stack, value, span)?;
                    }
                }
                Op::SetIndexValue => {
                    let value = pop(&mut stack, span)?;
                    let index = pop(&mut stack, span)?;
                    let object = pop(&mut stack, span)?;
                    let id = try_op!(self.object_id(object, span));
                    if try_op!(
                        self.objects
                            .is_frozen(id)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))
                    ) {
                        push(&mut stack, value, span)?;
                        continue;
                    }
                    if try_op!(
                        self.objects
                            .is_array(id)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))
                    ) {
                        let Some(index) = self.array_index(index) else {
                            let key = try_op!(self.property_key(index, span));
                            try_op!(self.set_property(object, &key, value, span));
                            push(&mut stack, value, span)?;
                            continue;
                        };
                        if try_op!(
                            self.objects
                                .is_sealed(id)
                                .map_err(|error| RuntimeError::runtime(error.to_string(), span))
                        ) {
                            let length = try_op!(self.objects.array_length(id).map_err(|error| {
                                RuntimeError::runtime(error.to_string(), span)
                            }));
                            if index >= length {
                                push(&mut stack, value, span)?;
                                continue;
                            }
                        }
                        try_op!(
                            self.objects
                                .array_set(id, index, value)
                                .map_err(|error| RuntimeError::runtime(error.to_string(), span))
                        );
                    } else {
                        let key = try_op!(self.property_key(index, span));
                        try_op!(self.set_property(object, &key, value, span));
                    }
                    push(&mut stack, value, span)?;
                }
                Op::DeleteProp => {
                    let index = read_u32(&function.code, &mut pc, span)?;
                    let key = match self.module.constants.get(index as usize) {
                        Some(Const::String(key)) => key.clone(),
                        _ => return Err(RuntimeError::runtime("invalid property key", span)),
                    };
                    let object = pop(&mut stack, span)?;
                    let deleted = try_op!(self.delete_property(object, &key, span));
                    push(&mut stack, Value::boolean(deleted), span)?;
                }
                Op::DeleteIndex => {
                    let index = pop(&mut stack, span)?;
                    let object = pop(&mut stack, span)?;
                    let id = try_op!(self.object_id(object, span));
                    let blocked = try_op!(
                        self.objects
                            .is_frozen(id)
                            .map_err(|error| { RuntimeError::runtime(error.to_string(), span) })
                    ) || try_op!(
                        self.objects
                            .is_sealed(id)
                            .map_err(|error| { RuntimeError::runtime(error.to_string(), span) })
                    );
                    if blocked {
                        push(&mut stack, Value::boolean(false), span)?;
                        continue;
                    }
                    if try_op!(
                        self.objects
                            .is_array(id)
                            .map_err(|error| RuntimeError::runtime(error.to_string(), span))
                    ) {
                        let index = try_op!(self.index_value(index, span));
                        let length = try_op!(
                            self.objects
                                .array_length(id)
                                .map_err(|error| RuntimeError::runtime(error.to_string(), span))
                        );
                        if index < length {
                            try_op!(
                                self.objects
                                    .array_set(id, index, Value::hole())
                                    .map_err(|error| RuntimeError::runtime(
                                        error.to_string(),
                                        span
                                    ))
                            );
                        }
                    } else {
                        let key = try_op!(self.property_key(index, span));
                        try_op!(self.delete_property(object, &key, span));
                    }
                    push(&mut stack, Value::boolean(true), span)?;
                }
                Op::GcCheck => {
                    let descriptor = FrameDescriptor {
                        function_index,
                        pc: pc.saturating_sub(1),
                        param_count: function.param_count,
                        local_count: function.local_count,
                        defined_locals: defined.iter().filter(|defined| **defined).count() as u16,
                        operand_depth: stack.len() as u32,
                    };
                    self.safepoint_stats.count = self.safepoint_stats.count.saturating_add(1);
                    self.safepoint_stats.max_operand_depth = self
                        .safepoint_stats
                        .max_operand_depth
                        .max(descriptor.operand_depth);
                    self.safepoint_stats.last = Some(descriptor.clone());
                    if self.objects.live_bytes() >= GC_THRESHOLD {
                        let mut roots = Vec::with_capacity(
                            self.external_roots.len()
                                + caller_roots.len()
                                + descriptor.local_count as usize
                                + descriptor.operand_depth as usize,
                        );
                        roots.extend_from_slice(self.external_roots);
                        roots.extend_from_slice(caller_roots);
                        for (value, is_defined) in locals
                            .iter()
                            .zip(defined.iter())
                            .take(descriptor.local_count as usize)
                        {
                            if *is_defined {
                                roots.push(*value);
                            }
                        }
                        roots.extend(
                            stack
                                .iter()
                                .take(descriptor.operand_depth as usize)
                                .copied(),
                        );
                        if let Some(env) = env {
                            roots.push(Value::object(env.index()));
                        }
                        if let Some(closure_env) = closure_env {
                            roots.push(Value::object(closure_env.index()));
                        }
                        if let Some(self_value) = self_value {
                            roots.push(self_value);
                        }
                        if let Some(capture_env) = capture_env {
                            roots.push(Value::object(capture_env.index()));
                        }
                        for frame in &iteration_frames {
                            roots.push(Value::object(frame.env.index()));
                        }
                        if let Some(prototype) = self.array_prototype {
                            roots.push(Value::object(prototype.index()));
                        }
                        if let Some(prototype) = self.object_prototype {
                            roots.push(Value::object(prototype.index()));
                        }
                        if let Some(global) = self.global_object {
                            roots.push(Value::object(global.index()));
                        }
                        roots.extend(self.builtin_cache.values().copied());
                        let stats = self.objects.collect(&roots);
                        if std::env::var_os("KAGURA_GC_TRACE").is_some() {
                            eprintln!(
                                "gc live_before={} live_after={} freed_objects={} freed_arrays={}",
                                stats.live_bytes_before,
                                stats.live_bytes_after,
                                stats.freed_objects,
                                stats.freed_arrays
                            );
                        }
                    }
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

    fn object_id(&self, value: Value, span: Span) -> Result<ObjectId, RuntimeError> {
        value
            .object_offset()
            .map(ObjectId::new)
            .ok_or_else(|| RuntimeError::type_error("value is not an object", span))
    }

    fn array_shape(&mut self, span: Span) -> Result<ShapeId, RuntimeError> {
        if self.array_prototype.is_none() {
            self.load_builtin("Array", span)?;
        }
        if let Some(prototype) = self.array_prototype {
            self.shapes
                .with_prototype(self.shapes.root(), Some(prototype))
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))
        } else {
            Ok(self.shapes.root())
        }
    }

    fn object_shape(&mut self, span: Span) -> Result<ShapeId, RuntimeError> {
        if self.object_prototype.is_none() {
            self.load_builtin("Object", span)?;
        }
        if let Some(prototype) = self.object_prototype {
            self.shapes
                .with_prototype(self.shapes.root(), Some(prototype))
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))
        } else {
            Ok(self.shapes.root())
        }
    }

    fn property_key(&mut self, value: Value, span: Span) -> Result<String, RuntimeError> {
        if !value.is_object() {
            return Ok(self.heap.to_js_string(value));
        }
        let roots = [value];
        for name in ["toString", "valueOf"] {
            let method = self.get_property(value, name, span)?;
            let Some(id) = method.object_offset().map(kagura_object::ObjectId::new) else {
                continue;
            };
            let callable = self
                .objects
                .is_function(id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            if !callable {
                continue;
            }
            let result = self.call_with_this(method, value, &[], &roots, span)?;
            if !result.is_object() {
                return Ok(self.heap.to_js_string(result));
            }
        }
        Ok("[object Object]".into())
    }

    fn array_index(&self, value: Value) -> Option<u32> {
        if let Some(index) = value.as_smi() {
            return index.ge(&0).then_some(index as u32);
        }
        if let Some(number) = self.heap.as_number(value)
            && number >= 0.0
            && number.fract() == 0.0
            && number <= 0xFFFF_FFFE_u32 as f64
        {
            return Some(number as u32);
        }
        if let Some(text) = self.heap.as_str(value) {
            if text == "0" {
                return Some(0);
            }
            if let Ok(index) = text.parse::<u32>()
                && index != u32::MAX
                && index.to_string() == text
            {
                return Some(index);
            }
        }
        None
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

    fn add(&mut self, left: Value, right: Value, _span: Span) -> Result<Value, RuntimeError> {
        if left.is_string() || right.is_string() {
            let text = format!(
                "{}{}",
                self.heap.to_js_string(left),
                self.heap.to_js_string(right)
            );
            return Ok(self.heap.string(&text));
        }
        let left = self.heap.to_number(left).unwrap_or(f64::NAN);
        let right = self.heap.to_number(right).unwrap_or(f64::NAN);
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
                left.partial_cmp(right)
            } else {
                let left = self.heap.to_number(left).unwrap_or(f64::NAN);
                let right = self.heap.to_number(right).unwrap_or(f64::NAN);
                left.partial_cmp(&right)
            };
        let _ = span;
        Ok(match op {
            Op::Lt => matches!(ordering, Some(std::cmp::Ordering::Less)),
            Op::Le => matches!(
                ordering,
                Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
            ),
            Op::Gt => matches!(ordering, Some(std::cmp::Ordering::Greater)),
            Op::Ge => matches!(
                ordering,
                Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
            ),
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

fn to_int32(number: f64) -> i32 {
    if !number.is_finite() || number == 0.0 {
        return 0;
    }
    let modulo = number.trunc() % 4_294_967_296.0;
    let unsigned = if modulo < 0.0 {
        modulo + 4_294_967_296.0
    } else {
        modulo
    };
    unsigned as u32 as i32
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
