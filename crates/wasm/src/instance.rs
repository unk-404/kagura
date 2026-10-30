//! Instance creation and the self-written MVP bytecode interpreter.

use wasmparser::{BinaryReader, Operator, OperatorsReader};

use crate::error::{InstantiateError, Trap};
use crate::memory::{Memory, MemoryCap};
use crate::module::{ExportKind, Module};
use crate::value::{ValType, Value};

/// A WebAssembly instance: a validated module plus its instantiated memories.
///
/// Instances borrow the module they were created from. Imports and start
/// functions are rejected in this foundation; data and element segments are
/// validated but not applied yet.
#[derive(Debug)]
pub struct Instance<'a> {
    module: &'a Module,
    memories: Vec<Memory>,
}

impl<'a> Instance<'a> {
    /// Creates an instance with the default memory budget for the module's
    /// address width.
    ///
    /// wasm32 modules get the 1 GiB 32-bit-first default; memory64 modules get
    /// the 4 GiB 64-bit default. Budgets are admission limits, not committed
    /// memory, so creating a memory64 instance only allocates its declared
    /// minimum pages.
    pub fn new(module: &'a Module) -> Result<Self, InstantiateError> {
        Self::with_memory_cap(
            module,
            MemoryCap::for_address_width(module.memory_address_width()),
        )
    }

    /// Creates an instance with an explicit per-memory cap.
    ///
    /// Every defined memory is instantiated with the same cap; a cap narrower
    /// than a declared memory's address width is rejected. Host imports and
    /// start functions are rejected rather than silently ignored.
    pub fn with_memory_cap(module: &'a Module, cap: MemoryCap) -> Result<Self, InstantiateError> {
        if let Some(import) = module.imports().first() {
            return Err(InstantiateError::UnsupportedImport {
                module: import.module.clone(),
                name: import.name.clone(),
                kind: import.kind.name(),
            });
        }
        if let Some(index) = module.start_function() {
            return Err(InstantiateError::UnsupportedStartFunction { index });
        }
        let mut memories = Vec::with_capacity(module.memories().len());
        for memory_type in module.memories() {
            memories.push(Memory::new(*memory_type, cap)?);
        }
        Ok(Self { module, memories })
    }

    /// Returns the module this instance was created from.
    pub fn module(&self) -> &Module {
        self.module
    }

    /// Returns all instantiated linear memories.
    pub fn memories(&self) -> &[Memory] {
        &self.memories
    }

    /// Returns a linear memory by index.
    pub fn memory(&self, index: u32) -> Option<&Memory> {
        self.memories.get(index as usize)
    }

    /// Returns a mutable linear memory by index.
    pub fn memory_mut(&mut self, index: u32) -> Option<&mut Memory> {
        self.memories.get_mut(index as usize)
    }

    /// Calls an exported function by name.
    pub fn invoke(&mut self, export_name: &str, args: &[Value]) -> Result<Vec<Value>, Trap> {
        let index = match self.module.export(export_name) {
            Some(export) => {
                if export.kind != ExportKind::Func {
                    return Err(Trap::ExportNotFunction {
                        name: export_name.to_string(),
                    });
                }
                export.index
            }
            None => {
                return Err(Trap::MissingExport {
                    name: export_name.to_string(),
                });
            }
        };
        self.invoke_function(index, args)
    }

    /// Calls a defined function by index.
    pub fn invoke_function(
        &mut self,
        function_index: u32,
        args: &[Value],
    ) -> Result<Vec<Value>, Trap> {
        let function = self
            .module
            .function(function_index)
            .ok_or(Trap::InvalidFunction {
                index: function_index,
            })?;
        let function_type =
            self.module
                .function_type(function)
                .ok_or_else(|| Trap::MalformedBody {
                    offset: function.operators_range().start as u64,
                    message: format!(
                        "function {function_index} references missing type {}",
                        function.type_index()
                    ),
                })?;

        if args.len() != function_type.params().len() {
            return Err(Trap::ArgumentCount {
                expected: function_type.params().len(),
                actual: args.len(),
            });
        }
        for (index, (argument, expected)) in args.iter().zip(function_type.params()).enumerate() {
            if argument.ty() != *expected {
                return Err(Trap::ArgumentType {
                    index,
                    expected: *expected,
                    actual: argument.ty(),
                });
            }
        }

        // 局部变量索引空间 = 参数 + 函数体声明的 locals，初值按类型置零。
        let mut locals = Vec::with_capacity(function_type.params().len() + function.locals().len());
        locals.extend_from_slice(args);
        locals.extend(function.locals().iter().copied().map(Value::zero));
        let mut stack: Vec<Value> = Vec::new();

        let operators = self.module.operators_bytes(function);
        let mut reader = OperatorsReader::new(BinaryReader::new(
            operators,
            function.operators_range().start as u64,
        ));

        loop {
            let (operator, offset) =
                reader
                    .read_with_offset()
                    .map_err(|error| Trap::MalformedBody {
                        offset: error.offset(),
                        message: error.message().to_string(),
                    })?;
            match operator {
                Operator::Nop => {}
                Operator::Unreachable => return Err(Trap::Unreachable { offset }),
                Operator::I32Const { value } => stack.push(Value::I32(value)),
                Operator::I32Add => binary_i32(&mut stack, offset, i32::wrapping_add)?,
                Operator::I32Sub => binary_i32(&mut stack, offset, i32::wrapping_sub)?,
                Operator::I32Mul => binary_i32(&mut stack, offset, i32::wrapping_mul)?,
                Operator::LocalGet { local_index } => {
                    let value =
                        locals
                            .get(local_index as usize)
                            .copied()
                            .ok_or(Trap::InvalidLocal {
                                index: local_index,
                                offset,
                            })?;
                    stack.push(value);
                }
                Operator::LocalSet { local_index } => {
                    let value = stack.pop().ok_or(Trap::StackUnderflow { offset })?;
                    store_local(&mut locals, local_index, value, offset)?;
                }
                Operator::LocalTee { local_index } => {
                    let value = stack
                        .last()
                        .copied()
                        .ok_or(Trap::StackUnderflow { offset })?;
                    store_local(&mut locals, local_index, value, offset)?;
                }
                Operator::Return | Operator::End => {
                    // 结构化控制流尚未实现，因此当前只会遇到函数最外层的
                    // `end`（或显式 `return`）。
                    return finish(stack, function_type.results(), offset);
                }
                other => {
                    return Err(Trap::UnsupportedOperator {
                        operator: format!("{other:?}"),
                        offset,
                    });
                }
            }
        }
    }
}

// 弹出两个 i32 操作数，执行 operation 后压回结果。
fn binary_i32(
    stack: &mut Vec<Value>,
    offset: u64,
    operation: fn(i32, i32) -> i32,
) -> Result<(), Trap> {
    let right = pop_i32(stack, offset)?;
    let left = pop_i32(stack, offset)?;
    stack.push(Value::I32(operation(left, right)));
    Ok(())
}

// 弹出一个 i32 操作数；栈空或类型不符分别报错。
fn pop_i32(stack: &mut Vec<Value>, offset: u64) -> Result<i32, Trap> {
    match stack.pop() {
        Some(Value::I32(value)) => Ok(value),
        Some(other) => Err(Trap::TypeMismatch {
            offset,
            expected: ValType::I32,
            actual: other.ty(),
        }),
        None => Err(Trap::StackUnderflow { offset }),
    }
}

// 写入 local 槽位，并检查索引越界与槽位类型。
fn store_local(
    locals: &mut [Value],
    local_index: u32,
    value: Value,
    offset: u64,
) -> Result<(), Trap> {
    let Some(slot) = locals.get_mut(local_index as usize) else {
        return Err(Trap::InvalidLocal {
            index: local_index,
            offset,
        });
    };
    if slot.ty() != value.ty() {
        return Err(Trap::TypeMismatch {
            offset,
            expected: slot.ty(),
            actual: value.ty(),
        });
    }
    *slot = value;
    Ok(())
}

// 按函数签名从操作数栈顶取返回值；多返回值暂不支持。
fn finish(stack: Vec<Value>, results: &[ValType], offset: u64) -> Result<Vec<Value>, Trap> {
    match results {
        [] => Ok(Vec::new()),
        [expected] => {
            let value = stack
                .last()
                .copied()
                .ok_or(Trap::StackUnderflow { offset })?;
            if value.ty() != *expected {
                return Err(Trap::TypeMismatch {
                    offset,
                    expected: *expected,
                    actual: value.ty(),
                });
            }
            Ok(vec![value])
        }
        _ => Err(Trap::UnsupportedFeature {
            feature: "multi-value function results",
        }),
    }
}
