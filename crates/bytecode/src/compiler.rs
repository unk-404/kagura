use kagura_ast::{
    AssignTarget, BinaryOp, Expr, ForInit, FuncDecl, Param, Program, Stmt, UnaryOp, VarDecl,
};
use kagura_support::{FastHashSet, Span};

use crate::error::CompileError;
use crate::module::{Const, FunctionProto, Module, VERSION};
use crate::op::Op;

pub fn compile(program: &Program) -> Result<Module, CompileError> {
    Compiler::default().compile(program)
}

#[derive(Default)]
struct Compiler {
    constants: Vec<Const>,
    functions: Vec<FunctionProto>,
    entry: usize,
}

impl Compiler {
    fn compile(&mut self, program: &Program) -> Result<Module, CompileError> {
        let mut names = FastHashSet::default();
        for stmt in &program.body {
            if let Stmt::Func(decl) = stmt {
                if !names.insert(decl.name.clone()) {
                    return Err(CompileError::new(
                        format!("duplicate function '{}'", decl.name),
                        decl.span,
                    ));
                }
                let proto = compile_function(decl, &mut self.constants)?;
                self.functions.push(proto);
            }
        }

        let script = FunctionCompiler::new("<script>", &[], &mut self.constants);
        let mut script = script;
        for stmt in &program.body {
            if matches!(stmt, Stmt::Func(_)) {
                continue;
            }
            script.compile_stmt(stmt)?;
        }
        script.emit_const(Const::Undefined, Span::empty(0));
        script.emit(Op::Return, Span::empty(0));
        self.entry = self.functions.len();
        self.functions.push(script.finish());

        Ok(Module {
            version: VERSION,
            constants: std::mem::take(&mut self.constants),
            functions: std::mem::take(&mut self.functions),
            entry: self.entry,
        })
    }
}

fn compile_function(
    decl: &FuncDecl,
    constants: &mut Vec<Const>,
) -> Result<FunctionProto, CompileError> {
    let mut params = FastHashSet::default();
    for param in &decl.params {
        if !params.insert(param.name.as_str()) {
            return Err(CompileError::new(
                format!("duplicate parameter '{}'", param.name),
                param.span,
            ));
        }
    }
    let mut compiler = FunctionCompiler::new(&decl.name, &decl.params, constants);
    for stmt in &decl.body {
        compiler.compile_stmt(stmt)?;
    }
    compiler.emit_const(Const::Undefined, decl.span);
    compiler.emit(Op::Return, decl.span);
    Ok(compiler.finish())
}

struct Local {
    name: String,
    mutable: bool,
}

struct LoopContext {
    break_patches: Vec<usize>,
    continue_patches: Vec<usize>,
    continue_target: Option<usize>,
}

struct FunctionCompiler<'a> {
    name: String,
    param_count: u16,
    code: Vec<u8>,
    spans: Vec<Span>,
    locals: Vec<Local>,
    scopes: Vec<usize>,
    loops: Vec<LoopContext>,
    constants: &'a mut Vec<Const>,
}

impl<'a> FunctionCompiler<'a> {
    fn new(name: &str, params: &[Param], constants: &'a mut Vec<Const>) -> Self {
        let mut compiler = Self {
            name: name.to_owned(),
            param_count: params.len() as u16,
            code: Vec::new(),
            spans: Vec::new(),
            locals: Vec::new(),
            scopes: vec![0],
            loops: Vec::new(),
            constants,
        };
        for param in params {
            compiler
                .declare(&param.name, true, param.span)
                .expect("parameters are unique");
        }
        compiler
    }

    fn finish(self) -> FunctionProto {
        FunctionProto {
            name: self.name,
            param_count: self.param_count,
            local_count: self.locals.len() as u16,
            code: self.code,
            spans: self.spans,
        }
    }

    fn compile_stmt(&mut self, stmt: &Stmt) -> Result<(), CompileError> {
        match stmt {
            Stmt::Var(decl) => self.compile_var(decl),
            Stmt::Func(decl) => Err(CompileError::new(
                "nested function declarations are not supported yet",
                decl.span,
            )),
            Stmt::Block(body) => {
                let base = self.locals.len();
                self.scopes.push(base);
                for stmt in body {
                    self.compile_stmt(stmt)?;
                }
                self.scopes.pop();
                Ok(())
            }
            Stmt::If {
                cond,
                then_branch,
                else_branch,
                span,
            } => {
                self.compile_expr(cond)?;
                let else_jump = self.emit_jump(Op::JumpIfFalse, *span);
                self.compile_stmt(then_branch)?;
                let end_jump = self.emit_jump(Op::Jump, *span);
                self.patch_jump(else_jump, self.code.len());
                if let Some(else_branch) = else_branch {
                    self.compile_stmt(else_branch)?;
                }
                self.patch_jump(end_jump, self.code.len());
                Ok(())
            }
            Stmt::While { cond, body, span } => {
                let loop_start = self.code.len();
                self.compile_expr(cond)?;
                let end_jump = self.emit_jump(Op::JumpIfFalse, *span);
                self.loops.push(LoopContext {
                    break_patches: Vec::new(),
                    continue_patches: Vec::new(),
                    continue_target: Some(loop_start),
                });
                self.compile_stmt(body)?;
                let context = self.loops.pop().expect("loop context");
                for patch in context.continue_patches {
                    self.patch_jump(patch, loop_start);
                }
                self.emit_jump_back(Op::Jump, loop_start, *span);
                self.patch_jump(end_jump, self.code.len());
                for patch in context.break_patches {
                    self.patch_jump(patch, self.code.len());
                }
                Ok(())
            }
            Stmt::For {
                init,
                cond,
                update,
                body,
                span,
            } => {
                let base = self.locals.len();
                self.scopes.push(base);
                if let Some(init) = init {
                    match init {
                        ForInit::Var(decl) => self.compile_var(decl)?,
                        ForInit::Expr(expr) => {
                            self.compile_expr(expr)?;
                            self.emit(Op::Pop, expr.span());
                        }
                    }
                }
                let loop_start = self.code.len();
                let end_jump = if let Some(cond) = cond {
                    self.compile_expr(cond)?;
                    Some(self.emit_jump(Op::JumpIfFalse, *span))
                } else {
                    None
                };
                self.loops.push(LoopContext {
                    break_patches: Vec::new(),
                    continue_patches: Vec::new(),
                    continue_target: None,
                });
                self.compile_stmt(body)?;
                let update_start = self.code.len();
                if let Some(update) = update {
                    self.compile_expr(update)?;
                    self.emit(Op::Pop, update.span());
                }
                let context = self.loops.pop().expect("loop context");
                let continue_target = context.continue_target.unwrap_or(update_start);
                for patch in context.continue_patches {
                    self.patch_jump(patch, continue_target);
                }
                self.emit_jump_back(Op::Jump, loop_start, *span);
                if let Some(end_jump) = end_jump {
                    self.patch_jump(end_jump, self.code.len());
                }
                for patch in context.break_patches {
                    self.patch_jump(patch, self.code.len());
                }
                self.scopes.pop();
                Ok(())
            }
            Stmt::Return { value, span } => {
                if let Some(value) = value {
                    self.compile_expr(value)?;
                } else {
                    self.emit_const(Const::Undefined, *span);
                }
                self.emit(Op::Return, *span);
                Ok(())
            }
            Stmt::Break { span } => {
                if self.loops.is_empty() {
                    return Err(CompileError::new("break outside loop", *span));
                }
                let patch = self.emit_jump(Op::Jump, *span);
                self.loops
                    .last_mut()
                    .expect("loop")
                    .break_patches
                    .push(patch);
                Ok(())
            }
            Stmt::Continue { span } => {
                if self.loops.is_empty() {
                    return Err(CompileError::new("continue outside loop", *span));
                }
                let patch = self.emit_jump(Op::Jump, *span);
                self.loops
                    .last_mut()
                    .expect("loop")
                    .continue_patches
                    .push(patch);
                Ok(())
            }
            Stmt::Expr { expr, .. } => {
                self.compile_expr(expr)?;
                self.emit(Op::Pop, expr.span());
                Ok(())
            }
            Stmt::TypeAlias { .. } | Stmt::Interface { .. } | Stmt::Empty { .. } => Ok(()),
        }
    }

    fn compile_var(&mut self, decl: &VarDecl) -> Result<(), CompileError> {
        let Some(slot) = self.declare(
            &decl.name,
            matches!(decl.kind, kagura_ast::VarKind::Let),
            decl.span,
        ) else {
            return Err(CompileError::new(
                format!("duplicate declaration '{}'", decl.name),
                decl.span,
            ));
        };
        if let Some(init) = &decl.init {
            self.compile_expr(init)?;
        } else {
            self.emit_const(Const::Undefined, decl.span);
        }
        self.emit_u16(Op::StoreLocal, slot, decl.span);
        Ok(())
    }

    fn compile_expr(&mut self, expr: &Expr) -> Result<(), CompileError> {
        match expr {
            Expr::Number { value, span } => {
                let index = self.add_const(Const::Number(*value));
                self.emit_u32(Op::LoadConst, index, *span);
            }
            Expr::String { value, span } => {
                let index = self.add_const(Const::String(value.clone()));
                self.emit_u32(Op::LoadConst, index, *span);
            }
            Expr::Boolean { value, span } => {
                let constant = if *value { Const::True } else { Const::False };
                let index = self.add_const(constant);
                self.emit_u32(Op::LoadConst, index, *span);
            }
            Expr::Null { span } => {
                let index = self.add_const(Const::Null);
                self.emit_u32(Op::LoadConst, index, *span);
            }
            Expr::Undefined { span } => {
                let index = self.add_const(Const::Undefined);
                self.emit_u32(Op::LoadConst, index, *span);
            }
            Expr::Ident { name, span } => {
                let (slot, _) = self.resolve(name).ok_or_else(|| {
                    CompileError::new(format!("unknown variable '{name}'"), *span)
                })?;
                self.emit_u16(Op::LoadLocal, slot, *span);
            }
            Expr::Unary { op, expr, span } => {
                self.compile_expr(expr)?;
                match op {
                    UnaryOp::Neg => self.emit(Op::Neg, *span),
                    UnaryOp::Not => self.emit(Op::Not, *span),
                    UnaryOp::Plus => {}
                }
            }
            Expr::Binary {
                op,
                left,
                right,
                span,
            } => {
                if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    self.compile_expr(left)?;
                    self.emit(Op::Dup, *span);
                    let jump = if matches!(op, BinaryOp::And) {
                        self.emit_jump(Op::JumpIfFalse, *span)
                    } else {
                        self.emit_jump(Op::JumpIfTrue, *span)
                    };
                    self.emit(Op::Pop, *span);
                    self.compile_expr(right)?;
                    self.patch_jump(jump, self.code.len());
                } else {
                    self.compile_expr(left)?;
                    self.compile_expr(right)?;
                    self.emit(binary_op(*op), *span);
                }
            }
            Expr::Assign {
                target,
                value,
                span,
            } => match target {
                AssignTarget::Ident(name) => {
                    let (slot, mutable) = self.resolve(name).ok_or_else(|| {
                        CompileError::new(format!("unknown variable '{name}'"), *span)
                    })?;
                    if !mutable {
                        return Err(CompileError::new(
                            format!("assignment to constant '{name}'"),
                            *span,
                        ));
                    }
                    self.compile_expr(value)?;
                    self.emit(Op::Dup, *span);
                    self.emit_u16(Op::StoreLocal, slot, *span);
                }
                AssignTarget::Member { object, property } => {
                    self.compile_expr(object)?;
                    self.compile_expr(value)?;
                    let index = self.add_const(Const::String(property.clone()));
                    self.emit_u32(Op::SetPropValue, index, *span);
                }
            },
            Expr::Call { callee, args, span } => {
                if let Expr::Ident { name, .. } = callee.as_ref() {
                    self.compile_args(args)?;
                    let index = self.add_const(Const::String(name.clone()));
                    self.emit_u32(Op::CallName, index, *span);
                    self.emit_u8_raw(args.len(), *span);
                } else if let Some(path) = member_path(callee) {
                    self.compile_args(args)?;
                    let index = self.add_const(Const::String(path));
                    self.emit_u32(Op::CallHost, index, *span);
                    self.emit_u8_raw(args.len(), *span);
                } else {
                    return Err(CompileError::new(
                        "unsupported call target in M0",
                        callee.span(),
                    ));
                }
            }
            Expr::Member {
                object,
                property,
                span,
            } => {
                self.compile_expr(object)?;
                let index = self.add_const(Const::String(property.clone()));
                self.emit_u32(Op::GetProp, index, *span);
            }
            Expr::Object { properties, span } => {
                self.emit(Op::NewObject, *span);
                for property in properties {
                    self.emit(Op::Dup, *span);
                    self.compile_expr(&property.value)?;
                    let index = self.add_const(Const::String(property.key.clone()));
                    self.emit_u32(Op::SetProp, index, property.span);
                }
            }
            Expr::As { expr, .. } => self.compile_expr(expr)?,
        }
        Ok(())
    }

    fn compile_args(&mut self, args: &[Expr]) -> Result<(), CompileError> {
        if args.len() > u8::MAX as usize {
            return Err(CompileError::new("too many arguments", Span::empty(0)));
        }
        for arg in args {
            self.compile_expr(arg)?;
        }
        Ok(())
    }

    fn declare(&mut self, name: &str, mutable: bool, _span: Span) -> Option<u16> {
        let base = *self.scopes.last().expect("scope");
        if self.locals[base..].iter().any(|local| local.name == name) {
            return None;
        }
        let slot = self.locals.len();
        if slot > u16::MAX as usize {
            return None;
        }
        self.locals.push(Local {
            name: name.to_owned(),
            mutable,
        });
        Some(slot as u16)
    }

    fn resolve(&self, name: &str) -> Option<(u16, bool)> {
        self.locals
            .iter()
            .enumerate()
            .rev()
            .find(|(_, local)| local.name == name)
            .map(|(slot, local)| (slot as u16, local.mutable))
    }

    fn add_const(&mut self, constant: Const) -> u32 {
        let index = self.constants.len() as u32;
        self.constants.push(constant);
        index
    }

    fn emit_const(&mut self, constant: Const, span: Span) {
        let index = self.add_const(constant);
        self.emit_u32(Op::LoadConst, index, span);
    }

    fn emit(&mut self, op: Op, span: Span) {
        self.emit_raw(op as u8, span);
    }

    fn emit_u8_raw(&mut self, value: usize, span: Span) {
        self.emit_raw(value as u8, span);
    }

    fn emit_u16(&mut self, op: Op, value: u16, span: Span) {
        self.emit(op, span);
        for byte in value.to_le_bytes() {
            self.emit_raw(byte, span);
        }
    }

    fn emit_u32(&mut self, op: Op, value: u32, span: Span) {
        self.emit(op, span);
        for byte in value.to_le_bytes() {
            self.emit_raw(byte, span);
        }
    }

    fn emit_jump(&mut self, op: Op, span: Span) -> usize {
        self.emit(op, span);
        let position = self.code.len();
        for _ in 0..4 {
            self.emit_raw(0, span);
        }
        position
    }

    fn emit_jump_back(&mut self, op: Op, target: usize, span: Span) {
        self.emit(op, span);
        for byte in (target as u32).to_le_bytes() {
            self.emit_raw(byte, span);
        }
    }

    fn patch_jump(&mut self, position: usize, target: usize) {
        let bytes = (target as u32).to_le_bytes();
        self.code[position..position + 4].copy_from_slice(&bytes);
    }

    fn emit_raw(&mut self, byte: u8, span: Span) {
        self.code.push(byte);
        self.spans.push(span);
    }
}

fn binary_op(op: BinaryOp) -> Op {
    match op {
        BinaryOp::Add => Op::Add,
        BinaryOp::Sub => Op::Sub,
        BinaryOp::Mul => Op::Mul,
        BinaryOp::Div => Op::Div,
        BinaryOp::Mod => Op::Mod,
        BinaryOp::Lt => Op::Lt,
        BinaryOp::Le => Op::Le,
        BinaryOp::Gt => Op::Gt,
        BinaryOp::Ge => Op::Ge,
        BinaryOp::Eq => Op::Eq,
        BinaryOp::StrictEq => Op::StrictEq,
        BinaryOp::NotEq => Op::Ne,
        BinaryOp::StrictNotEq => Op::StrictNotEq,
        BinaryOp::And | BinaryOp::Or => unreachable!(),
    }
}

fn member_path(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident { name, .. } => Some(name.clone()),
        Expr::Member {
            object, property, ..
        } => Some(format!("{}.{}", member_path(object)?, property)),
        _ => None,
    }
}
