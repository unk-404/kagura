use kagura_bytecode::{Const, Module, Op};
use kagura_support::Span;
use kagura_value::{Heap, Value};
use tracing::instrument;

use crate::error::{RuntimeError, RuntimeErrorKind};
use crate::host::Host;

const MAX_CALL_DEPTH: usize = 256;
const MAX_STACK: usize = 4096;

pub struct Interpreter<'a> {
    module: &'a Module,
    heap: &'a mut Heap,
    host: &'a mut dyn Host,
    depth: usize,
}

impl<'a> Interpreter<'a> {
    pub fn new(module: &'a Module, heap: &'a mut Heap, host: &'a mut dyn Host) -> Self {
        Self {
            module,
            heap,
            host,
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
                    let value = self.call_name(&name, &args, span)?;
                    push(&mut stack, value, span)?;
                }
                Op::Print => {
                    let argc = read_u8(&function.code, &mut pc, span)? as usize;
                    let mut args = Vec::with_capacity(argc);
                    for _ in 0..argc {
                        args.push(pop(&mut stack, span)?);
                    }
                    args.reverse();
                    let text = args
                        .iter()
                        .map(|value| self.heap.to_js_string(*value))
                        .collect::<Vec<_>>()
                        .join(" ");
                    self.host.print(&text);
                    push(&mut stack, Value::undefined(), span)?;
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

    fn call_name(&mut self, name: &str, args: &[Value], span: Span) -> Result<Value, RuntimeError> {
        if let Some(index) = self.module.find_function(name) {
            return self.call(index, args);
        }
        if name == "print" {
            let text = args
                .iter()
                .map(|value| self.heap.to_js_string(*value))
                .collect::<Vec<_>>()
                .join(" ");
            self.host.print(&text);
            return Ok(Value::undefined());
        }
        Err(RuntimeError::reference(
            format!("{name} is not defined"),
            span,
        ))
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
