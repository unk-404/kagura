mod analysis;

use kagura_ast::{AssignTarget, BinaryOp, Expr, ForInit, Param, Program, Stmt, UnaryOp, VarDecl};
use kagura_support::{FastHashSet, Span};

use crate::error::CompileError;
use crate::module::{Const, FunctionProto, Module, VERSION};
use crate::op::Op;
use analysis::{body_has_closure, collect_iteration_locals, loop_has_closure, member_path};

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
        let mut script = FunctionCompiler::new(
            "<script>",
            &[],
            &program.body,
            true,
            &mut self.constants,
            &mut self.functions,
        );
        for stmt in &program.body {
            match stmt {
                Stmt::Var(decl) => script.predeclare_or_error(
                    &decl.name,
                    matches!(decl.kind, kagura_ast::VarKind::Let),
                    decl.span,
                )?,
                Stmt::Func(decl) => {
                    script.predeclare_or_error(&decl.name, true, decl.span)?;
                }
                _ => {}
            }
        }
        for stmt in &program.body {
            if let Stmt::Func(decl) = stmt {
                let Some(Resolved::Local { slot, .. }) = script.resolve(&decl.name) else {
                    return Err(CompileError::new(
                        format!("unknown function '{}'", decl.name),
                        decl.span,
                    ));
                };
                script.compile_closure(&decl.name, &decl.params, &decl.body, None, decl.span)?;
                script.emit_u16(Op::StoreLocal, slot, decl.span);
            }
        }
        for stmt in &program.body {
            if matches!(stmt, Stmt::Func(_)) {
                continue;
            }
            script.compile_stmt(stmt)?;
        }
        script.emit_const(Const::Undefined, Span::empty(0));
        script.emit(Op::Return, Span::empty(0));
        let script = script.finish();
        self.entry = self.functions.len();
        self.functions.push(script);

        Ok(Module {
            version: VERSION,
            constants: std::mem::take(&mut self.constants),
            functions: std::mem::take(&mut self.functions),
            entry: self.entry,
        })
    }
}

#[derive(Clone)]
struct Local {
    name: String,
    mutable: bool,
    predeclared: bool,
    active: bool,
}

#[derive(Clone, Copy)]
enum Resolved {
    Local {
        slot: u16,
        mutable: bool,
    },
    Capture {
        level: u16,
        slot: u16,
        mutable: bool,
    },
    Iteration {
        depth: u16,
        slot: u16,
        mutable: bool,
    },
}

impl Resolved {
    fn mutable(self) -> bool {
        match self {
            Self::Local { mutable, .. }
            | Self::Capture { mutable, .. }
            | Self::Iteration { mutable, .. } => mutable,
        }
    }
}

struct IterationScope {
    bindings: Vec<Local>,
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
    gc_safe: bool,
    uses_env: bool,
    self_slot: Option<u16>,
    iterations: Vec<IterationScope>,
    outer: Vec<Vec<Local>>,
    constants: &'a mut Vec<Const>,
    functions: &'a mut Vec<FunctionProto>,
}

impl<'a> FunctionCompiler<'a> {
    fn new(
        name: &str,
        params: &[Param],
        body: &[Stmt],
        gc_safe: bool,
        constants: &'a mut Vec<Const>,
        functions: &'a mut Vec<FunctionProto>,
    ) -> Self {
        let mut compiler = Self {
            name: name.to_owned(),
            param_count: params.len() as u16,
            code: Vec::new(),
            spans: Vec::new(),
            locals: Vec::new(),
            scopes: vec![0],
            loops: Vec::new(),
            gc_safe,
            uses_env: body_has_closure(body),
            self_slot: None,
            iterations: Vec::new(),
            outer: Vec::new(),
            constants,
            functions,
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
            uses_env: self.uses_env,
            self_slot: self.self_slot,
            code: self.code,
            spans: self.spans,
        }
    }

    fn compile_stmt(&mut self, stmt: &Stmt) -> Result<(), CompileError> {
        if self.gc_safe {
            self.emit(Op::GcCheck, stmt.span());
        }
        match stmt {
            Stmt::Var(decl) => self.compile_var(decl),
            Stmt::Func(decl) => {
                let Some(slot) = self.declare(&decl.name, true, decl.span) else {
                    return Err(CompileError::new(
                        format!("duplicate declaration '{}'", decl.name),
                        decl.span,
                    ));
                };
                self.compile_closure(&decl.name, &decl.params, &decl.body, None, decl.span)?;
                self.emit_u16(Op::StoreLocal, slot, decl.span);
                Ok(())
            }
            Stmt::Block(body) => {
                let base = self.locals.len();
                self.scopes.push(base);
                for stmt in body {
                    self.compile_stmt(stmt)?;
                }
                self.close_scope();
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
                let mut iteration_source = None;
                if body_has_closure(std::slice::from_ref(body)) {
                    let mut bindings = Vec::new();
                    collect_iteration_locals(body, &mut bindings);
                    if !bindings.is_empty() {
                        let count = bindings.len();
                        self.emit(Op::IterationBegin, *span);
                        self.emit_u8_raw(count, *span);
                        for _ in 0..count {
                            self.emit_u16_raw(u16::MAX, *span);
                        }
                        self.iterations.push(IterationScope { bindings });
                        iteration_source = Some(count);
                    }
                }
                let loop_start = self.code.len();
                self.compile_expr(cond)?;
                let end_jump = self.emit_jump(Op::JumpIfFalse, *span);
                self.loops.push(LoopContext {
                    break_patches: Vec::new(),
                    continue_patches: Vec::new(),
                    continue_target: None,
                });
                self.compile_stmt(body)?;
                let fork_start = self.code.len();
                if let Some(count) = iteration_source {
                    self.emit(Op::IterationNext, *span);
                    self.emit_u8_raw(count, *span);
                }
                let context = self.loops.pop().expect("loop context");
                let continue_target = if iteration_source.is_some() {
                    fork_start
                } else {
                    loop_start
                };
                for patch in context.continue_patches {
                    self.patch_jump(patch, continue_target);
                }
                self.emit_jump_back(Op::Jump, loop_start, *span);
                let cleanup = self.code.len();
                if iteration_source.is_some() {
                    self.emit(Op::IterationEnd, *span);
                    self.iterations.pop();
                }
                self.patch_jump(end_jump, cleanup);
                for patch in context.break_patches {
                    self.patch_jump(patch, cleanup);
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
                let mut iteration_source = None;
                if let Some(init) = init {
                    match init {
                        ForInit::Var(decl) => {
                            self.compile_var(decl)?;
                            if matches!(
                                decl.kind,
                                kagura_ast::VarKind::Let | kagura_ast::VarKind::Const
                            ) && loop_has_closure(cond.as_ref(), update.as_ref(), body)
                            {
                                let Some(Resolved::Local { slot, .. }) = self.resolve(&decl.name)
                                else {
                                    return Err(CompileError::new(
                                        format!("unknown loop variable '{}'", decl.name),
                                        decl.span,
                                    ));
                                };
                                let mut bindings = vec![Local {
                                    name: decl.name.clone(),
                                    mutable: matches!(decl.kind, kagura_ast::VarKind::Let),
                                    predeclared: false,
                                    active: true,
                                }];
                                collect_iteration_locals(body, &mut bindings);
                                let count = bindings.len();
                                self.emit(Op::IterationBegin, *span);
                                self.emit_u8_raw(count, *span);
                                self.emit_u16_raw(slot, *span);
                                for _ in 1..count {
                                    self.emit_u16_raw(u16::MAX, *span);
                                }
                                self.iterations.push(IterationScope { bindings });
                                iteration_source = Some(count);
                            }
                        }
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
                let fork_start = self.code.len();
                if let Some(count) = iteration_source {
                    self.emit(Op::IterationNext, *span);
                    self.emit_u8_raw(count, *span);
                }
                let update_start = self.code.len();
                if let Some(update) = update {
                    self.compile_expr(update)?;
                    self.emit(Op::Pop, update.span());
                }
                let context = self.loops.pop().expect("loop context");
                let continue_target = if iteration_source.is_some() {
                    fork_start
                } else {
                    context.continue_target.unwrap_or(update_start)
                };
                for patch in context.continue_patches {
                    self.patch_jump(patch, continue_target);
                }
                self.emit_jump_back(Op::Jump, loop_start, *span);
                let cleanup = self.code.len();
                if iteration_source.is_some() {
                    self.emit(Op::IterationEnd, *span);
                    self.iterations.pop();
                }
                if let Some(end_jump) = end_jump {
                    self.patch_jump(end_jump, cleanup);
                }
                for patch in context.break_patches {
                    self.patch_jump(patch, cleanup);
                }
                self.close_scope();
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
            Stmt::Throw { value, span } => {
                self.compile_expr(value)?;
                self.emit(Op::Throw, *span);
                Ok(())
            }
            Stmt::Try {
                block,
                catch_param,
                catch_block,
                finally_block,
                span,
            } => {
                let catch_patch = self.emit_jump(Op::TryBegin, *span);
                self.compile_block_body(block)?;
                self.emit(Op::TryEnd, *span);
                let end_patch = self.emit_jump(Op::Jump, *span);
                let catch_pc = self.code.len();
                self.patch_jump(catch_patch, catch_pc);
                if let Some(catch_block) = catch_block {
                    let base = self.locals.len();
                    self.scopes.push(base);
                    let slot = catch_param
                        .as_ref()
                        .and_then(|param| self.declare(param, true, *span));
                    if let Some(slot) = slot {
                        self.emit_u16(Op::StoreLocal, slot, *span);
                    } else {
                        self.emit(Op::Pop, *span);
                    }
                    for stmt in catch_block {
                        self.compile_stmt(stmt)?;
                    }
                    self.close_scope();
                }
                let finally_pc = self.code.len();
                self.patch_jump(end_patch, finally_pc);
                if let Some(finally_block) = finally_block {
                    self.compile_block_body(finally_block)?;
                }
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

    fn compile_block_body(&mut self, body: &[Stmt]) -> Result<(), CompileError> {
        let base = self.locals.len();
        self.scopes.push(base);
        for stmt in body {
            self.compile_stmt(stmt)?;
        }
        self.close_scope();
        Ok(())
    }

    fn compile_var(&mut self, decl: &VarDecl) -> Result<(), CompileError> {
        if let Some(scope) = self.iterations.last()
            && let Some((slot, _)) = scope
                .bindings
                .iter()
                .enumerate()
                .find(|(_, binding)| binding.name == decl.name)
        {
            if let Some(init) = &decl.init {
                self.compile_expr(init)?;
            } else {
                self.emit_const(Const::Undefined, decl.span);
            }
            self.emit_u16(Op::StoreIter, 0, decl.span);
            self.emit_u16_raw(slot as u16, decl.span);
            return Ok(());
        }
        let base = *self.scopes.last().expect("scope");
        let predeclared = self.locals[base..]
            .iter()
            .position(|local| local.active && local.name == decl.name && local.predeclared)
            .map(|offset| base + offset);
        let slot = if let Some(slot) = predeclared {
            self.locals[slot].predeclared = false;
            slot as u16
        } else {
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
            slot
        };
        if let Some(init) = &decl.init {
            self.compile_expr(init)?;
        } else {
            self.emit_const(Const::Undefined, decl.span);
        }
        self.emit_u16(Op::StoreLocal, slot, decl.span);
        Ok(())
    }

    fn compile_closure(
        &mut self,
        name: &str,
        params: &[Param],
        body: &[Stmt],
        self_name: Option<&str>,
        span: Span,
    ) -> Result<(), CompileError> {
        let mut names = FastHashSet::default();
        for param in params {
            if !names.insert(param.name.as_str()) {
                return Err(CompileError::new(
                    format!("duplicate parameter '{}'", param.name),
                    param.span,
                ));
            }
        }
        let mut outer = Vec::with_capacity(self.outer.len() + self.iterations.len() + 1);
        for scope in self.iterations.iter().rev() {
            outer.push(scope.bindings.clone());
        }
        outer.push(
            self.locals
                .iter()
                .filter(|local| local.active)
                .cloned()
                .collect(),
        );
        outer.extend(self.outer.iter().cloned());
        let mut compiler =
            FunctionCompiler::new(name, params, body, true, self.constants, self.functions);
        compiler.outer = outer;
        if let Some(self_name) = self_name
            && let Some(slot) = compiler.declare(self_name, false, span)
        {
            compiler.self_slot = Some(slot);
        }
        for stmt in body {
            compiler.compile_stmt(stmt)?;
        }
        compiler.emit_const(Const::Undefined, span);
        compiler.emit(Op::Return, span);
        let function = compiler.finish();
        let index = self.functions.len() as u32;
        self.functions.push(function);
        self.emit_u32(Op::NewClosure, index, span);
        Ok(())
    }

    fn compile_arrow(
        &mut self,
        params: &[Param],
        body: &kagura_ast::ArrowBody,
        span: Span,
    ) -> Result<(), CompileError> {
        let body = match body {
            kagura_ast::ArrowBody::Block(body) => body.clone(),
            kagura_ast::ArrowBody::Expr(expr) => vec![Stmt::Return {
                value: Some((**expr).clone()),
                span: expr.span(),
            }],
        };
        self.compile_closure("<arrow>", params, &body, None, span)
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
            Expr::This { span } => {
                self.emit(Op::LoadThis, *span);
            }
            Expr::Ident { name, span } => match self.resolve(name) {
                Some(Resolved::Local { slot, .. }) => self.emit_u16(Op::LoadLocal, slot, *span),
                Some(Resolved::Capture { level, slot, .. }) => {
                    self.emit_u16(Op::LoadCapture, level, *span);
                    self.emit_u16_raw(slot, *span);
                }
                Some(Resolved::Iteration { depth, slot, .. }) => {
                    self.emit_u16(Op::LoadIter, depth, *span);
                    self.emit_u16_raw(slot, *span);
                }
                None => {
                    return Err(CompileError::new(
                        format!("unknown variable '{name}'"),
                        *span,
                    ));
                }
            },
            Expr::Unary { op, expr, span } => {
                self.compile_expr(expr)?;
                match op {
                    UnaryOp::Neg => self.emit(Op::Neg, *span),
                    UnaryOp::Not => self.emit(Op::Not, *span),
                    UnaryOp::Plus => {}
                    UnaryOp::TypeOf => self.emit(Op::TypeOf, *span),
                    UnaryOp::Void => {
                        self.emit(Op::Pop, *span);
                        self.emit_const(Const::Undefined, *span);
                    }
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
                    let resolved = self.resolve(name).ok_or_else(|| {
                        CompileError::new(format!("unknown variable '{name}'"), *span)
                    })?;
                    if !resolved.mutable() {
                        return Err(CompileError::new(
                            format!("assignment to constant '{name}'"),
                            *span,
                        ));
                    }
                    self.compile_expr(value)?;
                    self.emit(Op::Dup, *span);
                    match resolved {
                        Resolved::Local { slot, .. } => self.emit_u16(Op::StoreLocal, slot, *span),
                        Resolved::Capture { level, slot, .. } => {
                            self.emit_u16(Op::StoreCapture, level, *span);
                            self.emit_u16_raw(slot, *span);
                        }
                        Resolved::Iteration { depth, slot, .. } => {
                            self.emit_u16(Op::StoreIter, depth, *span);
                            self.emit_u16_raw(slot, *span);
                        }
                    }
                }
                AssignTarget::Member { object, property } => {
                    self.compile_expr(object)?;
                    self.compile_expr(value)?;
                    let index = self.add_const(Const::String(property.clone()));
                    self.emit_u32(Op::SetPropValue, index, *span);
                }
                AssignTarget::Index { object, index } => {
                    self.compile_expr(object)?;
                    self.compile_expr(index)?;
                    self.compile_expr(value)?;
                    self.emit(Op::SetIndexValue, *span);
                }
            },
            Expr::Call { callee, args, span } => {
                if let Expr::Ident { name, .. } = callee.as_ref() {
                    if self.resolve(name).is_some() {
                        self.compile_expr(callee)?;
                        self.compile_args(args)?;
                        self.emit(Op::CallValue, *span);
                        self.emit_u8_raw(args.len(), *span);
                    } else {
                        self.compile_args(args)?;
                        let index = self.add_const(Const::String(name.clone()));
                        self.emit_u32(Op::CallName, index, *span);
                        self.emit_u8_raw(args.len(), *span);
                    }
                } else if let Expr::Member {
                    object, property, ..
                } = callee.as_ref()
                {
                    let host_path = member_path(callee).filter(|path| {
                        let root = path.split('.').next().unwrap_or_default();
                        self.resolve(root).is_none()
                    });
                    if let Some(path) = host_path {
                        self.compile_args(args)?;
                        let index = self.add_const(Const::String(path));
                        self.emit_u32(Op::CallHost, index, *span);
                        self.emit_u8_raw(args.len(), *span);
                    } else {
                        self.compile_expr(object)?;
                        self.compile_args(args)?;
                        let index = self.add_const(Const::String(property.clone()));
                        self.emit_u32(Op::CallMethod, index, *span);
                        self.emit_u8_raw(args.len(), *span);
                    }
                } else {
                    self.compile_expr(callee)?;
                    self.compile_args(args)?;
                    self.emit(Op::CallValue, *span);
                    self.emit_u8_raw(args.len(), *span);
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
            Expr::Array { elements, span } => {
                self.emit(Op::NewArray, *span);
                for element in elements {
                    self.emit(Op::Dup, *span);
                    match element {
                        Some(element) => self.compile_expr(element)?,
                        None => self.emit(Op::LoadHole, *span),
                    }
                    self.emit(Op::ArrayPush, *span);
                }
            }
            Expr::Index {
                object,
                index,
                span,
            } => {
                self.compile_expr(object)?;
                self.compile_expr(index)?;
                self.emit(Op::GetIndex, *span);
            }
            Expr::Function {
                name,
                params,
                body,
                span,
            } => {
                let display = name.as_deref().unwrap_or("<function>");
                self.compile_closure(display, params, body, name.as_deref(), *span)?;
            }
            Expr::Arrow { params, body, span } => {
                self.compile_arrow(params, body, *span)?;
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

    fn predeclare_or_error(
        &mut self,
        name: &str,
        mutable: bool,
        span: Span,
    ) -> Result<(), CompileError> {
        if self.predeclare(name, mutable, span) {
            Ok(())
        } else {
            Err(CompileError::new(
                format!("duplicate declaration '{name}'"),
                span,
            ))
        }
    }

    fn close_scope(&mut self) {
        if let Some(base) = self.scopes.pop() {
            for local in &mut self.locals[base..] {
                local.active = false;
            }
        }
    }

    fn predeclare(&mut self, name: &str, mutable: bool, _span: Span) -> bool {
        if self
            .locals
            .iter()
            .any(|local| local.active && local.name == name)
        {
            return false;
        }
        self.locals.push(Local {
            name: name.to_owned(),
            mutable,
            predeclared: true,
            active: true,
        });
        true
    }

    fn declare(&mut self, name: &str, mutable: bool, _span: Span) -> Option<u16> {
        let base = *self.scopes.last().expect("scope");
        if self.locals[base..]
            .iter()
            .any(|local| local.active && local.name == name)
        {
            return None;
        }
        let slot = self.locals.len();
        if slot > u16::MAX as usize {
            return None;
        }
        self.locals.push(Local {
            name: name.to_owned(),
            mutable,
            predeclared: false,
            active: true,
        });
        Some(slot as u16)
    }

    fn resolve(&self, name: &str) -> Option<Resolved> {
        for (depth, scope) in self.iterations.iter().rev().enumerate() {
            if let Some((slot, binding)) = scope
                .bindings
                .iter()
                .enumerate()
                .find(|(_, binding)| binding.name == name)
            {
                return Some(Resolved::Iteration {
                    depth: depth as u16,
                    slot: slot as u16,
                    mutable: binding.mutable,
                });
            }
        }
        if let Some((slot, local)) = self
            .locals
            .iter()
            .enumerate()
            .rev()
            .find(|(_, local)| local.active && local.name == name)
            .map(|(slot, local)| (slot as u16, local))
        {
            return Some(Resolved::Local {
                slot,
                mutable: local.mutable,
            });
        }
        for (level, scope) in self.outer.iter().enumerate() {
            if let Some((slot, local)) = scope
                .iter()
                .enumerate()
                .rev()
                .find(|(_, local)| local.name == name)
            {
                return Some(Resolved::Capture {
                    level: (level + 1) as u16,
                    slot: slot as u16,
                    mutable: local.mutable,
                });
            }
        }
        None
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

    fn emit_u16_raw(&mut self, value: u16, span: Span) {
        for byte in value.to_le_bytes() {
            self.emit_raw(byte, span);
        }
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
        BinaryOp::In => Op::In,
        BinaryOp::And | BinaryOp::Or => unreachable!(),
    }
}
