use kagura_bytecode::{Const, Module, Op};
use kagura_object::{
    ObjectHeap, ObjectId, PropertyAttributes, PropertyKey, PropertyLocation, ShapeTable,
    own_property_keys,
};
use kagura_support::Span;
use kagura_value::{Heap, Value};
use tracing::instrument;

use crate::error::{RuntimeError, RuntimeErrorKind};
use crate::host::{Host, HostCall, HostFunctionId};

const MAX_CALL_DEPTH: usize = 256;
const MAX_STACK: usize = 4096;
const GC_THRESHOLD: u32 = 32 * 1024;

struct FrameContext<'a> {
    caller_roots: &'a [Value],
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
        self.call(entry, &[], &[], None, None)
    }

    #[instrument(skip(self, args, caller_roots), fields(function = index, argc = args.len()))]
    fn call(
        &mut self,
        index: usize,
        args: &[Value],
        caller_roots: &[Value],
        closure_env: Option<ObjectId>,
        self_value: Option<Value>,
    ) -> Result<Value, RuntimeError> {
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

        let env = if function.uses_env {
            let env = self
                .objects
                .allocate_array(self.shapes.root())
                .map_err(|error| RuntimeError::runtime(error.to_string(), Span::empty(0)))?;
            for (slot, value) in locals.iter().enumerate() {
                let value = if defined[slot] { *value } else { Value::hole() };
                self.objects
                    .array_push(env, value)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), Span::empty(0)))?;
            }
            let parent = closure_env
                .map(|parent| Value::object(parent.index()))
                .unwrap_or_else(Value::undefined);
            self.objects
                .array_push(env, parent)
                .map_err(|error| RuntimeError::runtime(error.to_string(), Span::empty(0)))?;
            Some(env)
        } else {
            None
        };
        if let (Some(slot), Some(value)) = (function.self_slot, self_value) {
            let slot = slot as usize;
            if slot < locals.len() {
                locals[slot] = value;
                defined[slot] = true;
                if let Some(env) = env {
                    self.objects
                        .array_set(env, slot as u32, value)
                        .map_err(|error| {
                            RuntimeError::runtime(error.to_string(), Span::empty(0))
                        })?;
                }
            }
        }

        self.depth += 1;
        let frame = FrameContext {
            caller_roots,
            env,
            closure_env,
            self_value,
        };
        let result = self.execute(index, &function, &mut locals, &mut defined, &frame);
        self.depth -= 1;
        result
    }

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
                Op::LoadHole => {
                    push(&mut stack, Value::hole(), span)?;
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
                Op::In => {
                    let target = pop(&mut stack, span)?;
                    let key = pop(&mut stack, span)?;
                    let found = self.has_property(key, target, span)?;
                    push(&mut stack, Value::boolean(found), span)?;
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
                    let object = self
                        .objects
                        .allocate_function(self.shapes.root(), function, Value::object(env.index()))
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    push(&mut stack, Value::object(object.index()), span)?;
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
                    let value = self
                        .call_builtin(object, &key, &args, span, &roots)?
                        .ok_or_else(|| RuntimeError::type_error("method is not supported", span))?;
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
                    let index = self.index_value(index, span)?;
                    if let Some(text) = self.heap.as_str(object) {
                        let value = text
                            .encode_utf16()
                            .nth(index as usize)
                            .and_then(|unit| String::from_utf16(&[unit]).ok())
                            .map(|unit| self.heap.string(&unit))
                            .unwrap_or_else(Value::undefined);
                        push(&mut stack, value, span)?;
                        continue;
                    }
                    let id = self.object_id(object, span)?;
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
                Op::GcCheck => {
                    if self.objects.live_bytes() >= GC_THRESHOLD {
                        let mut roots =
                            Vec::with_capacity(caller_roots.len() + locals.len() + stack.len());
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
                        if let Some(self_value) = self_value {
                            roots.push(self_value);
                        }
                        if let Some(capture_env) = capture_env {
                            roots.push(Value::object(capture_env.index()));
                        }
                        for frame in &iteration_frames {
                            roots.push(Value::object(frame.env.index()));
                        }
                        self.objects.collect(&roots);
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

    fn call_name(
        &mut self,
        name: &str,
        index: usize,
        args: &[Value],
        span: Span,
        caller_roots: &[Value],
    ) -> Result<Value, RuntimeError> {
        if let Some(function) = self.module.find_function(name) {
            return self.call(function, args, caller_roots, None, None);
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

    fn call_global(
        &mut self,
        path: &str,
        args: &[Value],
        span: Span,
    ) -> Result<Option<Value>, RuntimeError> {
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

    fn invoke_callable(
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
        let (function, env_value) = self
            .objects
            .function_data(object)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let closure_env = env_value.object_offset().map(ObjectId::new);
        self.call(
            function as usize,
            args,
            caller_roots,
            closure_env,
            Some(callee),
        )
    }

    fn call_builtin(
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
                _ => return Ok(None),
            }));
        }
        let Ok(object_id) = self.object_id(object, span) else {
            return Ok(None);
        };
        if !self
            .objects
            .is_array(object_id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
        {
            return Ok(None);
        }
        Ok(Some(match key {
            "push" => {
                for value in args {
                    self.objects
                        .array_push(object_id, *value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                let length = self
                    .objects
                    .array_length(object_id)
                    .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                self.heap.number(length as f64)
            }
            "pop" => self
                .objects
                .array_pop(object_id)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?,
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
                    let value = self
                        .objects
                        .array_get(object_id, index)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
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
                    let value = self
                        .objects
                        .array_get(object_id, index as u32)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                    self.objects
                        .array_push(array, value)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
                }
                Value::object(array.index())
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
                } else if length > 0 {
                    index = 1;
                    self.objects
                        .array_get(object_id, 0)
                        .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
                } else {
                    return Err(RuntimeError::type_error(
                        "reduce of empty array with no initial value",
                        span,
                    ));
                };
                let mut roots = roots.to_vec();
                roots.push(accumulator);
                for current in index..length {
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

    fn catch_error(
        &mut self,
        handlers: &mut Vec<TryHandler>,
        stack: &mut Vec<Value>,
        error: RuntimeError,
    ) -> Result<usize, RuntimeError> {
        let Some(handler) = handlers.pop() else {
            return Err(error);
        };
        stack.truncate(handler.stack_depth);
        let value = error
            .thrown
            .unwrap_or_else(|| self.heap.string(&error.message));
        stack.push(value);
        Ok(handler.catch_pc)
    }

    fn capture_env(
        &mut self,
        mut env: ObjectId,
        level: u16,
        span: Span,
    ) -> Result<ObjectId, RuntimeError> {
        if level == 0 {
            return Err(RuntimeError::reference("invalid capture level", span));
        }
        for _ in 1..level {
            let length = self
                .objects
                .array_length(env)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            if length == 0 {
                return Err(RuntimeError::reference("broken closure environment", span));
            }
            let parent = self
                .objects
                .array_get(env, length - 1)
                .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
            env = self.object_id(parent, span)?;
        }
        Ok(env)
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

    fn has_property(
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
        let shape = self
            .objects
            .shape(id)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let key = PropertyKey::from_string(key);
        Ok(self
            .shapes
            .lookup(shape, &key)
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?
            .is_some())
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
