mod analysis;

use kagura_ast::{
    AssignTarget, BinaryOp, DestructureDecl, Expr, ForInit, Param, Pattern, Program, Stmt, UnaryOp,
    VarDecl,
};
use kagura_support::{FastHashSet, Span};

use crate::error::CompileError;
use crate::module::{Const, FeedbackKind, FunctionProto, Module, Safepoint, VERSION};
use crate::op::Op;
use analysis::{
    body_has_closure, collect_iteration_locals, loop_has_closure, member_path, params_have_closure,
};

pub fn compile(program: &Program) -> Result<Module, CompileError> {
    Compiler::default().compile(program)
}

#[derive(Default)]
struct Compiler {
    constants: Vec<Const>,
    functions: Vec<FunctionProto>,
    feedback: Vec<FeedbackKind>,
    entry: usize,
}

impl Compiler {
    fn compile(&mut self, program: &Program) -> Result<Module, CompileError> {
        let mut script = FunctionCompiler::new(
            "<script>",
            &[],
            &program.body,
            true,
            has_use_strict(&program.body),
            &mut self.constants,
            &mut self.functions,
            &mut self.feedback,
        );
        for stmt in &program.body {
            match stmt {
                Stmt::Var(decl) => script.predeclare_or_error(
                    &decl.name,
                    matches!(
                        decl.kind,
                        kagura_ast::VarKind::Var | kagura_ast::VarKind::Let
                    ),
                    decl.span,
                    matches!(decl.kind, kagura_ast::VarKind::Var),
                )?,
                Stmt::Func(decl) => {
                    script.predeclare_or_error(&decl.name, true, decl.span, false)?;
                }
                Stmt::Class { name, span, .. } => {
                    script.predeclare_or_error(name, false, *span, false)?;
                }
                Stmt::VarList(decls) => {
                    for decl in decls {
                        script.predeclare_or_error(
                            &decl.name,
                            matches!(
                                decl.kind,
                                kagura_ast::VarKind::Var | kagura_ast::VarKind::Let
                            ),
                            decl.span,
                            matches!(decl.kind, kagura_ast::VarKind::Var),
                        )?;
                    }
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
            feedback: std::mem::take(&mut self.feedback),
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
    is_switch: bool,
}

struct FinallyContext {
    return_slot: u16,
    flag_slot: u16,
    completion_slot: u16,
    completions: Vec<Completion>,
    return_patches: Vec<usize>,
    completion_jumps: Vec<usize>,
}

struct Completion {
    code: u32,
    loop_index: usize,
    is_break: bool,
}

struct FunctionCompiler<'a> {
    name: String,
    param_count: u16,
    code: Vec<u8>,
    spans: Vec<Span>,
    safepoints: Vec<Safepoint>,
    locals: Vec<Local>,
    scopes: Vec<usize>,
    loops: Vec<LoopContext>,
    gc_safe: bool,
    strict: bool,
    uses_env: bool,
    self_slot: Option<u16>,
    arguments_slot: Option<u16>,
    parameter_patterns: Vec<(u16, Pattern)>,
    finallys: Vec<FinallyContext>,
    completion_counter: u32,
    iterations: Vec<IterationScope>,
    outer: Vec<Vec<Local>>,
    constants: &'a mut Vec<Const>,
    functions: &'a mut Vec<FunctionProto>,
    feedback: &'a mut Vec<FeedbackKind>,
}

impl<'a> FunctionCompiler<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        name: &str,
        params: &[Param],
        body: &[Stmt],
        gc_safe: bool,
        strict: bool,
        constants: &'a mut Vec<Const>,
        functions: &'a mut Vec<FunctionProto>,
        feedback: &'a mut Vec<FeedbackKind>,
    ) -> Self {
        let mut compiler = Self {
            name: name.to_owned(),
            param_count: params.len() as u16,
            code: Vec::new(),
            spans: Vec::new(),
            safepoints: Vec::new(),
            locals: Vec::new(),
            scopes: vec![0],
            loops: Vec::new(),
            gc_safe,
            strict,
            uses_env: body_has_closure(body) || params_have_closure(params),
            self_slot: None,
            arguments_slot: None,
            parameter_patterns: Vec::new(),
            finallys: Vec::new(),
            completion_counter: 1,
            iterations: Vec::new(),
            outer: Vec::new(),
            constants,
            functions,
            feedback,
        };
        for param in params {
            let slot = compiler.declare_parameter(&param.name);
            if let Some(pattern) = &param.pattern {
                compiler.parameter_patterns.push((slot, pattern.clone()));
            }
        }
        if !matches!(name, "<arrow>" | "<script>") {
            compiler.arguments_slot = compiler.declare("arguments", true, Span::empty(0));
        }
        compiler
    }

    fn hoist_function_scope(&mut self, body: &[Stmt]) {
        for stmt in body {
            self.hoist_stmt(stmt);
        }
    }

    fn hoist_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Var(decl) if matches!(decl.kind, kagura_ast::VarKind::Var) => {
                self.predeclare(&decl.name, true, decl.span);
            }
            Stmt::VarList(decls) => {
                for decl in decls {
                    if matches!(decl.kind, kagura_ast::VarKind::Var) {
                        self.predeclare(&decl.name, true, decl.span);
                    }
                }
            }
            Stmt::Func(decl) => {
                self.predeclare(&decl.name, true, decl.span);
            }
            Stmt::Block(body) => {
                for stmt in body {
                    self.hoist_stmt(stmt);
                }
            }
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.hoist_stmt(then_branch);
                if let Some(else_branch) = else_branch {
                    self.hoist_stmt(else_branch);
                }
            }
            Stmt::While { body, .. } => self.hoist_stmt(body),
            Stmt::DoWhile { body, .. } => self.hoist_stmt(body),
            Stmt::For { init, body, .. } => {
                if let Some(ForInit::Var(decl)) = init
                    && matches!(decl.kind, kagura_ast::VarKind::Var)
                {
                    self.predeclare(&decl.name, true, decl.span);
                }
                self.hoist_stmt(body);
            }
            Stmt::ForIn {
                declaration, body, ..
            } => {
                if let Some(decl) = declaration
                    && matches!(decl.kind, kagura_ast::VarKind::Var)
                {
                    self.predeclare(&decl.name, true, decl.span);
                }
                self.hoist_stmt(body);
            }
            Stmt::ForOf {
                declaration, body, ..
            } => {
                if let Some(decl) = declaration
                    && matches!(decl.kind, kagura_ast::VarKind::Var)
                {
                    self.predeclare(&decl.name, true, decl.span);
                }
                self.hoist_stmt(body);
            }
            Stmt::Switch { cases, .. } => {
                for case in cases {
                    for stmt in &case.body {
                        self.hoist_stmt(stmt);
                    }
                }
            }
            Stmt::Try {
                block,
                catch_block,
                finally_block,
                ..
            } => {
                for stmt in block {
                    self.hoist_stmt(stmt);
                }
                if let Some(block) = catch_block {
                    for stmt in block {
                        self.hoist_stmt(stmt);
                    }
                }
                if let Some(block) = finally_block {
                    for stmt in block {
                        self.hoist_stmt(stmt);
                    }
                }
            }
            _ => {}
        }
    }

    fn compile_arguments(&mut self, span: Span) -> Result<(), CompileError> {
        if let Some(slot) = self.arguments_slot {
            self.emit(Op::Arguments, span);
            self.emit_u16(Op::StoreLocal, slot, span);
        }
        Ok(())
    }

    fn hidden_local(&mut self, _span: Span) -> u16 {
        let slot = self.locals.len() as u16;
        self.locals.push(Local {
            name: format!("$hidden_{slot}"),
            mutable: true,
            predeclared: false,
            active: true,
        });
        slot
    }

    fn emit_return(&mut self, span: Span) {
        if let Some(context) = self.finallys.last() {
            let (return_slot, flag_slot) = (context.return_slot, context.flag_slot);
            self.emit_u16(Op::StoreLocal, return_slot, span);
            self.emit_const(Const::True, span);
            self.emit_u16(Op::StoreLocal, flag_slot, span);
            let patch = self.emit_jump(Op::Jump, span);
            self.finallys
                .last_mut()
                .expect("finally context")
                .return_patches
                .push(patch);
        } else {
            self.emit(Op::Return, span);
        }
    }

    fn compile_param_defaults(&mut self, params: &[Param]) -> Result<(), CompileError> {
        for (slot, param) in params.iter().enumerate() {
            let Some(default) = &param.default else {
                continue;
            };
            self.emit_u16(Op::LoadLocal, slot as u16, param.span);
            self.emit_const(Const::Undefined, param.span);
            self.emit(Op::StrictEq, param.span);
            let skip = self.emit_jump(Op::JumpIfFalse, param.span);
            self.compile_expr(default)?;
            self.emit_u16(Op::StoreLocal, slot as u16, param.span);
            self.patch_jump(skip, self.code.len());
        }
        Ok(())
    }

    fn compile_rest_param(&mut self, params: &[Param]) -> Result<(), CompileError> {
        let Some((slot, param)) = params.iter().enumerate().find(|(_, param)| param.rest) else {
            return Ok(());
        };
        if slot + 1 != params.len() {
            return Err(CompileError::new("rest parameter must be last", param.span));
        }
        self.emit_u16(Op::Rest, slot as u16, param.span);
        self.emit_u16(Op::StoreLocal, slot as u16, param.span);
        Ok(())
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
            safepoints: self.safepoints,
        }
    }

    fn compile_stmt(&mut self, stmt: &Stmt) -> Result<(), CompileError> {
        if self.gc_safe {
            self.safepoints.push(Safepoint {
                pc: self.code.len() as u32,
                local_count: self.locals.len() as u16,
                operand_depth: 0,
            });
            self.emit(Op::GcCheck, stmt.span());
        }
        match stmt {
            Stmt::Var(decl) => self.compile_var(decl),
            Stmt::VarList(decls) => {
                for decl in decls {
                    self.compile_var(decl)?;
                }
                Ok(())
            }
            Stmt::Destructure(decl) => self.compile_destructure(decl),
            Stmt::Class {
                name,
                super_class,
                fields,
                methods,
                span,
            } => self.compile_class(name, super_class.as_ref(), fields, methods, *span),
            Stmt::Func(decl) => {
                if self.strict && matches!(decl.name.as_str(), "eval" | "arguments") {
                    return Err(CompileError::new(
                        format!("invalid strict-mode function name '{}'", decl.name),
                        decl.span,
                    ));
                }
                let slot = if let Some(Resolved::Local { slot, .. }) = self.resolve(&decl.name) {
                    slot
                } else {
                    let Some(slot) = self.declare(&decl.name, true, decl.span) else {
                        return Err(CompileError::new(
                            format!("duplicate declaration '{}'", decl.name),
                            decl.span,
                        ));
                    };
                    slot
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
            Stmt::DoWhile { body, cond, span } => {
                let loop_start = self.code.len();
                self.loops.push(LoopContext {
                    break_patches: Vec::new(),
                    continue_patches: Vec::new(),
                    continue_target: None,
                    is_switch: false,
                });
                self.compile_stmt(body)?;
                let cond_start = self.code.len();
                self.compile_expr(cond)?;
                self.emit_jump_back(Op::JumpIfTrue, loop_start, *span);
                let end = self.code.len();
                let context = self.loops.pop().expect("loop context");
                for patch in context.continue_patches {
                    self.patch_jump(patch, cond_start);
                }
                for patch in context.break_patches {
                    self.patch_jump(patch, end);
                }
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
                    is_switch: false,
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
                                    mutable: matches!(
                                        decl.kind,
                                        kagura_ast::VarKind::Var | kagura_ast::VarKind::Let
                                    ),
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
                    is_switch: false,
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
            Stmt::ForIn {
                declaration,
                target,
                object,
                body,
                span,
            } => {
                let keys_slot = self.hidden_local(*span);
                let index_slot = self.hidden_local(*span);
                let key_slot = self.hidden_local(*span);
                self.compile_expr(object)?;
                self.emit(Op::ForInKeys, *span);
                self.emit_u16(Op::StoreLocal, keys_slot, *span);
                self.emit_const(Const::Number(0.0), *span);
                self.emit_u16(Op::StoreLocal, index_slot, *span);
                let loop_start = self.code.len();
                self.emit_u16(Op::LoadLocal, index_slot, *span);
                self.emit_u16(Op::LoadLocal, keys_slot, *span);
                let length_key = self.add_const(Const::String("length".into()));
                self.emit_get_prop(length_key, *span);
                self.emit(Op::Lt, *span);
                let end_jump = self.emit_jump(Op::JumpIfFalse, *span);
                self.emit_u16(Op::LoadLocal, keys_slot, *span);
                self.emit_u16(Op::LoadLocal, index_slot, *span);
                self.emit(Op::GetIndex, *span);
                self.emit_u16(Op::StoreLocal, key_slot, *span);
                if let Some(declaration) = declaration {
                    let slot = match self.resolve(&declaration.name) {
                        Some(Resolved::Local { slot, .. }) => slot,
                        Some(_) => {
                            return Err(CompileError::new(
                                format!("unsupported for-in binding '{}'", declaration.name),
                                declaration.span,
                            ));
                        }
                        None => self
                            .declare(&declaration.name, true, declaration.span)
                            .ok_or_else(|| {
                                CompileError::new(
                                    format!("duplicate for-in binding '{}'", declaration.name),
                                    declaration.span,
                                )
                            })?,
                    };
                    self.emit_u16(Op::LoadLocal, key_slot, *span);
                    self.emit_u16(Op::StoreLocal, slot, *span);
                } else if let Some(target) = target {
                    match target {
                        AssignTarget::Ident(name) => {
                            let Some(Resolved::Local { slot, .. }) = self.resolve(name) else {
                                return Err(CompileError::new(
                                    format!("unsupported for-in target '{name}'"),
                                    *span,
                                ));
                            };
                            self.emit_u16(Op::LoadLocal, key_slot, *span);
                            self.emit_u16(Op::StoreLocal, slot, *span);
                        }
                        _ => {
                            return Err(CompileError::new(
                                "only identifier for-in targets are supported",
                                *span,
                            ));
                        }
                    }
                }
                self.loops.push(LoopContext {
                    break_patches: Vec::new(),
                    continue_patches: Vec::new(),
                    continue_target: None,
                    is_switch: false,
                });
                self.compile_stmt(body)?;
                let update_start = self.code.len();
                self.emit_u16(Op::LoadLocal, index_slot, *span);
                self.emit_const(Const::Number(1.0), *span);
                self.emit(Op::Add, *span);
                self.emit_u16(Op::StoreLocal, index_slot, *span);
                let context = self.loops.pop().expect("loop context");
                for patch in context.continue_patches {
                    self.patch_jump(patch, update_start);
                }
                self.emit_jump_back(Op::Jump, loop_start, *span);
                let end = self.code.len();
                self.patch_jump(end_jump, end);
                for patch in context.break_patches {
                    self.patch_jump(patch, end);
                }
                Ok(())
            }
            Stmt::ForOf {
                declaration,
                target,
                iterable,
                body,
                span,
            } => {
                let iterable_slot = self.hidden_local(*span);
                let index_slot = self.hidden_local(*span);
                let value_slot = self.hidden_local(*span);
                self.compile_expr(iterable)?;
                self.emit_u16(Op::StoreLocal, iterable_slot, *span);
                self.emit_const(Const::Number(0.0), *span);
                self.emit_u16(Op::StoreLocal, index_slot, *span);
                let loop_start = self.code.len();
                self.emit_u16(Op::LoadLocal, index_slot, *span);
                self.emit_u16(Op::LoadLocal, iterable_slot, *span);
                let length_key = self.add_const(Const::String("length".into()));
                self.emit_get_prop(length_key, *span);
                self.emit(Op::Lt, *span);
                let end_jump = self.emit_jump(Op::JumpIfFalse, *span);
                self.emit_u16(Op::LoadLocal, iterable_slot, *span);
                self.emit_u16(Op::LoadLocal, index_slot, *span);
                self.emit(Op::GetIndex, *span);
                self.emit_u16(Op::StoreLocal, value_slot, *span);
                if let Some(declaration) = declaration {
                    let slot = match self.resolve(&declaration.name) {
                        Some(Resolved::Local { slot, .. }) => slot,
                        Some(_) => {
                            return Err(CompileError::new(
                                format!("unsupported for-of binding '{}'", declaration.name),
                                declaration.span,
                            ));
                        }
                        None => self
                            .declare(&declaration.name, true, declaration.span)
                            .ok_or_else(|| {
                                CompileError::new(
                                    format!("duplicate for-of binding '{}'", declaration.name),
                                    declaration.span,
                                )
                            })?,
                    };
                    self.emit_u16(Op::LoadLocal, value_slot, *span);
                    self.emit_u16(Op::StoreLocal, slot, *span);
                } else if let Some(target) = target {
                    match target {
                        AssignTarget::Ident(name) => {
                            let Some(Resolved::Local { slot, .. }) = self.resolve(name) else {
                                return Err(CompileError::new(
                                    format!("unsupported for-of target '{name}'"),
                                    *span,
                                ));
                            };
                            self.emit_u16(Op::LoadLocal, value_slot, *span);
                            self.emit_u16(Op::StoreLocal, slot, *span);
                        }
                        _ => {
                            return Err(CompileError::new(
                                "only identifier for-of targets are supported",
                                *span,
                            ));
                        }
                    }
                }
                self.loops.push(LoopContext {
                    break_patches: Vec::new(),
                    continue_patches: Vec::new(),
                    continue_target: None,
                    is_switch: false,
                });
                self.compile_stmt(body)?;
                let update_start = self.code.len();
                self.emit_u16(Op::LoadLocal, index_slot, *span);
                self.emit_const(Const::Number(1.0), *span);
                self.emit(Op::Add, *span);
                self.emit_u16(Op::StoreLocal, index_slot, *span);
                let context = self.loops.pop().expect("loop context");
                for patch in context.continue_patches {
                    self.patch_jump(patch, update_start);
                }
                self.emit_jump_back(Op::Jump, loop_start, *span);
                let end = self.code.len();
                self.patch_jump(end_jump, end);
                for patch in context.break_patches {
                    self.patch_jump(patch, end);
                }
                Ok(())
            }
            Stmt::Switch {
                discriminant,
                cases,
                span,
            } => {
                let value_slot = self.hidden_local(*span);
                self.compile_expr(discriminant)?;
                self.emit_u16(Op::StoreLocal, value_slot, *span);
                let mut test_jumps = Vec::new();
                let mut default_index = None;
                for (index, case) in cases.iter().enumerate() {
                    if let Some(test) = &case.test {
                        self.emit_u16(Op::LoadLocal, value_slot, *span);
                        self.compile_expr(test)?;
                        self.emit(Op::StrictEq, *span);
                        let jump = self.emit_jump(Op::JumpIfTrue, *span);
                        test_jumps.push((index, jump));
                    } else {
                        default_index = Some(index);
                    }
                }
                let dispatch_jump = self.emit_jump(Op::Jump, *span);
                self.loops.push(LoopContext {
                    break_patches: Vec::new(),
                    continue_patches: Vec::new(),
                    continue_target: None,
                    is_switch: true,
                });
                let mut starts = Vec::with_capacity(cases.len());
                for case in cases {
                    starts.push(self.code.len());
                    for stmt in &case.body {
                        self.compile_stmt(stmt)?;
                    }
                }
                let end = self.code.len();
                let context = self.loops.pop().expect("switch context");
                self.patch_jump(
                    dispatch_jump,
                    default_index.map_or(end, |index| starts[index]),
                );
                for (index, jump) in test_jumps {
                    self.patch_jump(jump, starts[index]);
                }
                for patch in context.break_patches {
                    self.patch_jump(patch, end);
                }
                Ok(())
            }
            Stmt::Return { value, span } => {
                if let Some(value) = value {
                    self.compile_expr(value)?;
                } else {
                    self.emit_const(Const::Undefined, *span);
                }
                self.emit_return(*span);
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
                let mut finally_state = None;
                if finally_block.is_some() {
                    let return_slot = self.hidden_local(*span);
                    let flag_slot = self.hidden_local(*span);
                    let completion_slot = self.hidden_local(*span);
                    self.emit_const(Const::False, *span);
                    self.emit_u16(Op::StoreLocal, flag_slot, *span);
                    self.emit_const(Const::Number(0.0), *span);
                    self.emit_u16(Op::StoreLocal, completion_slot, *span);
                    self.finallys.push(FinallyContext {
                        return_slot,
                        flag_slot,
                        completion_slot,
                        completions: Vec::new(),
                        return_patches: Vec::new(),
                        completion_jumps: Vec::new(),
                    });
                    finally_state = Some((return_slot, flag_slot, completion_slot));
                }
                let rethrow = if finally_block.is_some() {
                    let error_slot = self.hidden_local(*span);
                    let threw_slot = self.hidden_local(*span);
                    self.emit_const(Const::False, *span);
                    self.emit_u16(Op::StoreLocal, threw_slot, *span);
                    Some((error_slot, threw_slot))
                } else {
                    None
                };
                let catch_patch = self.emit_jump(Op::TryBegin, *span);
                self.compile_block_body(block)?;
                self.emit(Op::TryEnd, *span);
                let end_patch = self.emit_jump(Op::Jump, *span);
                let catch_pc = self.code.len();
                self.patch_jump(catch_patch, catch_pc);
                let mut to_finally = vec![end_patch];
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
                    if let Some((error_slot, threw_slot)) = rethrow {
                        let catch_try = self.emit_jump(Op::TryBegin, *span);
                        for stmt in catch_block {
                            self.compile_stmt(stmt)?;
                        }
                        self.emit(Op::TryEnd, *span);
                        to_finally.push(self.emit_jump(Op::Jump, *span));
                        let handler_pc = self.code.len();
                        self.patch_jump(catch_try, handler_pc);
                        self.emit_u16(Op::StoreLocal, error_slot, *span);
                        self.emit_const(Const::True, *span);
                        self.emit_u16(Op::StoreLocal, threw_slot, *span);
                        to_finally.push(self.emit_jump(Op::Jump, *span));
                    } else {
                        for stmt in catch_block {
                            self.compile_stmt(stmt)?;
                        }
                    }
                    self.close_scope();
                } else if let Some((error_slot, threw_slot)) = rethrow {
                    self.emit_u16(Op::StoreLocal, error_slot, *span);
                    self.emit_const(Const::True, *span);
                    self.emit_u16(Op::StoreLocal, threw_slot, *span);
                    to_finally.push(self.emit_jump(Op::Jump, *span));
                }
                let finally_pc = self.code.len();
                for patch in to_finally {
                    self.patch_jump(patch, finally_pc);
                }
                if let Some((return_slot, flag_slot, completion_slot)) = finally_state {
                    let context = self.finallys.pop().expect("finally context");
                    for patch in &context.return_patches {
                        self.patch_jump(*patch, finally_pc);
                    }
                    for patch in &context.completion_jumps {
                        self.patch_jump(*patch, finally_pc);
                    }
                    if let Some(finally_block) = finally_block {
                        self.compile_block_body(finally_block)?;
                    }
                    if let Some((error_slot, threw_slot)) = rethrow {
                        self.emit_u16(Op::LoadLocal, threw_slot, *span);
                        self.emit_const(Const::True, *span);
                        self.emit(Op::StrictEq, *span);
                        let no_throw = self.emit_jump(Op::JumpIfFalse, *span);
                        self.emit_u16(Op::LoadLocal, error_slot, *span);
                        self.emit(Op::Throw, *span);
                        self.patch_jump(no_throw, self.code.len());
                    }
                    self.emit_u16(Op::LoadLocal, flag_slot, *span);
                    self.emit_const(Const::True, *span);
                    self.emit(Op::StrictEq, *span);
                    let no_return = self.emit_jump(Op::JumpIfFalse, *span);
                    self.emit_u16(Op::LoadLocal, return_slot, *span);
                    self.emit_return(*span);
                    self.patch_jump(no_return, self.code.len());
                    for completion in context.completions {
                        self.emit_u16(Op::LoadLocal, completion_slot, *span);
                        self.emit_const(Const::Number(completion.code as f64), *span);
                        self.emit(Op::StrictEq, *span);
                        let next = self.emit_jump(Op::JumpIfFalse, *span);
                        let jump = self.emit_jump(Op::Jump, *span);
                        let Some(loop_context) = self.loops.get_mut(completion.loop_index) else {
                            return Err(CompileError::new(
                                "break/continue through finally crosses a loop boundary",
                                *span,
                            ));
                        };
                        if completion.is_break {
                            loop_context.break_patches.push(jump);
                        } else {
                            loop_context.continue_patches.push(jump);
                        }
                        self.patch_jump(next, self.code.len());
                    }
                    if !self.finallys.is_empty() {
                        self.emit_u16(Op::LoadLocal, completion_slot, *span);
                        self.emit_const(Const::Number(0.0), *span);
                        self.emit(Op::StrictEq, *span);
                        let skip = self.emit_jump(Op::JumpIfTrue, *span);
                        let jump = self.emit_jump(Op::Jump, *span);
                        self.finallys
                            .last_mut()
                            .expect("outer finally context")
                            .completion_jumps
                            .push(jump);
                        self.patch_jump(skip, self.code.len());
                    }
                } else if let Some(finally_block) = finally_block {
                    self.compile_block_body(finally_block)?;
                }
                Ok(())
            }
            Stmt::Break { span } => {
                if self.loops.is_empty() {
                    return Err(CompileError::new("break outside loop", *span));
                }
                let slots: Vec<u16> = self
                    .finallys
                    .iter()
                    .map(|context| context.completion_slot)
                    .collect();
                if !slots.is_empty() {
                    let code = self.completion_counter;
                    self.completion_counter += 1;
                    for slot in slots {
                        self.emit_const(Const::Number(code as f64), *span);
                        self.emit_u16(Op::StoreLocal, slot, *span);
                    }
                    let jump = self.emit_jump(Op::Jump, *span);
                    let loop_index = self.loops.len() - 1;
                    self.finallys
                        .last_mut()
                        .expect("finally context")
                        .completion_jumps
                        .push(jump);
                    self.finallys
                        .first_mut()
                        .expect("finally context")
                        .completions
                        .push(Completion {
                            code,
                            loop_index,
                            is_break: true,
                        });
                    return Ok(());
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
                let Some(continue_index) =
                    self.loops.iter().rposition(|context| !context.is_switch)
                else {
                    return Err(CompileError::new("continue outside loop", *span));
                };
                let slots: Vec<u16> = self
                    .finallys
                    .iter()
                    .map(|context| context.completion_slot)
                    .collect();
                if !slots.is_empty() {
                    let code = self.completion_counter;
                    self.completion_counter += 1;
                    for slot in slots {
                        self.emit_const(Const::Number(code as f64), *span);
                        self.emit_u16(Op::StoreLocal, slot, *span);
                    }
                    let jump = self.emit_jump(Op::Jump, *span);
                    let loop_index = continue_index;
                    self.finallys
                        .last_mut()
                        .expect("finally context")
                        .completion_jumps
                        .push(jump);
                    self.finallys
                        .first_mut()
                        .expect("finally context")
                        .completions
                        .push(Completion {
                            code,
                            loop_index,
                            is_break: false,
                        });
                    return Ok(());
                }
                let patch = self.emit_jump(Op::Jump, *span);
                self.loops[continue_index].continue_patches.push(patch);
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

    fn compile_class(
        &mut self,
        name: &str,
        super_class: Option<&Expr>,
        fields: &[kagura_ast::ClassField],
        methods: &[kagura_ast::ClassMethod],
        span: Span,
    ) -> Result<(), CompileError> {
        let slot = match self.resolve(name) {
            Some(Resolved::Local { slot, .. }) => slot,
            Some(_) => {
                return Err(CompileError::new(
                    format!("unsupported class binding '{name}'"),
                    span,
                ));
            }
            None => self
                .declare(name, false, span)
                .ok_or_else(|| CompileError::new(format!("duplicate class '{name}'"), span))?,
        };
        let instance_fields = fields
            .iter()
            .filter(|field| !field.is_static)
            .map(|field| Stmt::Expr {
                expr: Expr::Assign {
                    target: AssignTarget::Member {
                        object: Box::new(Expr::This { span: field.span }),
                        property: field.name.clone(),
                    },
                    value: Box::new(
                        field
                            .value
                            .clone()
                            .unwrap_or(Expr::Undefined { span: field.span }),
                    ),
                    span: field.span,
                },
                span: field.span,
            })
            .collect::<Vec<_>>();
        let constructor_body =
            if let Some(constructor) = methods.iter().find(|method| method.name == "constructor") {
                let mut body = instance_fields.clone();
                body.extend(constructor.body.iter().cloned());
                body
            } else {
                instance_fields
            };
        if let Some(constructor) = methods.iter().find(|method| method.name == "constructor") {
            self.compile_closure(
                name,
                &constructor.params,
                &constructor_body,
                None,
                constructor.span,
            )?;
        } else {
            self.compile_closure(name, &[], &constructor_body, None, span)?;
        }
        self.emit_u16(Op::StoreLocal, slot, span);
        let has_constructor = methods.iter().any(|method| method.name == "constructor");
        let super_slot = if let Some(super_class) = super_class {
            let super_slot = self.hidden_local(span);
            self.compile_expr(super_class)?;
            self.emit_u16(Op::StoreLocal, super_slot, span);
            self.emit_u16(Op::LoadLocal, slot, span);
            let prototype_key = self.add_const(Const::String("prototype".into()));
            self.emit_get_prop(prototype_key, span);
            self.emit_u16(Op::LoadLocal, super_slot, span);
            self.emit_get_prop(prototype_key, span);
            self.emit(Op::SetPrototype, span);
            self.emit_u16(Op::LoadLocal, slot, span);
            self.emit_u16(Op::LoadLocal, super_slot, span);
            self.emit(Op::SetPrototype, span);
            self.emit_u16(Op::LoadLocal, slot, span);
            self.emit_u16(Op::LoadLocal, super_slot, span);
            let super_key = self.add_const(Const::String("__kaguraSuper".into()));
            self.emit_u32(Op::SetProp, super_key, span);
            Some(super_slot)
        } else {
            None
        };
        if let Some(super_slot) = super_slot
            && !has_constructor
        {
            self.emit_u16(Op::LoadLocal, slot, span);
            self.emit_u16(Op::LoadLocal, super_slot, span);
            let key = self.add_const(Const::String("__kaguraDefaultCtor".into()));
            self.emit_u32(Op::SetProp, key, span);
        }
        for field in fields.iter().filter(|field| field.is_static) {
            self.emit_u16(Op::LoadLocal, slot, span);
            if let Some(value) = &field.value {
                self.compile_expr(value)?;
            } else {
                self.emit_const(Const::Undefined, field.span);
            }
            let key = self.add_const(Const::String(field.name.clone()));
            self.emit_u32(Op::SetProp, key, field.span);
        }
        for method in methods {
            if method.name == "constructor" {
                continue;
            }
            self.emit_u16(Op::LoadLocal, slot, span);
            if !method.is_static {
                let prototype_key = self.add_const(Const::String("prototype".into()));
                self.emit_get_prop(prototype_key, span);
            }
            self.compile_closure(
                &method.name,
                &method.params,
                &method.body,
                None,
                method.span,
            )?;
            let key = self.add_const(Const::String(method.name.clone()));
            self.emit_u32(Op::SetPropNonEnum, key, method.span);
        }
        Ok(())
    }

    fn compile_destructure(&mut self, decl: &DestructureDecl) -> Result<(), CompileError> {
        let source = self.hidden_local(decl.span);
        self.compile_expr(&decl.init)?;
        self.emit_u16(Op::StoreLocal, source, decl.span);
        self.bind_pattern(&decl.pattern, source, decl.kind, decl.span, false)
    }

    fn bind_pattern(
        &mut self,
        pattern: &Pattern,
        source: u16,
        kind: kagura_ast::VarKind,
        span: Span,
        assign: bool,
    ) -> Result<(), CompileError> {
        match pattern {
            Pattern::Ident(name) => {
                let mutable = !matches!(kind, kagura_ast::VarKind::Const);
                let slot = if assign {
                    match self.resolve(name) {
                        Some(Resolved::Local { slot, .. }) => slot,
                        _ => {
                            return Err(CompileError::new(
                                format!("unknown variable '{name}'"),
                                span,
                            ));
                        }
                    }
                } else if matches!(kind, kagura_ast::VarKind::Var) {
                    if let Some(Resolved::Local { slot, .. }) = self.resolve(name) {
                        slot
                    } else {
                        self.declare(name, mutable, span)
                            .ok_or_else(|| CompileError::new("duplicate binding", span))?
                    }
                } else {
                    self.declare(name, mutable, span)
                        .ok_or_else(|| CompileError::new("duplicate binding", span))?
                };
                self.emit_u16(Op::LoadLocal, source, span);
                self.emit_u16(Op::StoreLocal, slot, span);
            }
            Pattern::Array(elements) => {
                for (index, element) in elements.iter().enumerate() {
                    let Some(element) = element else {
                        continue;
                    };
                    if let Pattern::Rest(inner) = element {
                        self.emit_u16(Op::LoadLocal, source, span);
                        self.emit_const(Const::Number(index as f64), span);
                        self.emit(Op::ArrayRest, span);
                        let value = self.hidden_local(span);
                        self.emit_u16(Op::StoreLocal, value, span);
                        self.bind_pattern(inner, value, kind, span, assign)?;
                        break;
                    }
                    self.emit_u16(Op::LoadLocal, source, span);
                    self.emit_const(Const::Number(index as f64), span);
                    self.emit(Op::GetIndex, span);
                    let value = self.hidden_local(span);
                    self.emit_u16(Op::StoreLocal, value, span);
                    self.bind_pattern(element, value, kind, span, assign)?;
                }
            }
            Pattern::Object { properties, rest } => {
                for (key, element) in properties {
                    self.emit_u16(Op::LoadLocal, source, span);
                    let index = self.add_const(Const::String(key.clone()));
                    self.emit_get_prop(index, span);
                    let value = self.hidden_local(span);
                    self.emit_u16(Op::StoreLocal, value, span);
                    self.bind_pattern(element, value, kind, span, assign)?;
                }
                if let Some(rest) = rest {
                    self.emit_u16(Op::LoadLocal, source, span);
                    let keys = properties
                        .iter()
                        .map(|(key, _)| key.as_str())
                        .collect::<Vec<_>>()
                        .join("\u{0}");
                    let index = self.add_const(Const::String(keys));
                    self.emit_u32(Op::ObjectRest, index, span);
                    let value = self.hidden_local(span);
                    self.emit_u16(Op::StoreLocal, value, span);
                    self.bind_pattern(rest, value, kind, span, assign)?;
                }
            }
            Pattern::Default { pattern, default } => {
                self.emit_u16(Op::LoadLocal, source, span);
                self.emit(Op::Dup, span);
                self.emit_const(Const::Undefined, span);
                self.emit(Op::StrictEq, span);
                let not_undefined = self.emit_jump(Op::JumpIfFalse, span);
                self.emit(Op::Pop, span);
                self.compile_expr(default)?;
                let end = self.emit_jump(Op::Jump, span);
                self.patch_jump(not_undefined, self.code.len());
                self.patch_jump(end, self.code.len());
                let value = self.hidden_local(span);
                self.emit_u16(Op::StoreLocal, value, span);
                self.bind_pattern(pattern, value, kind, span, assign)?;
            }
            Pattern::Rest(_) => {
                return Err(CompileError::new(
                    "rest pattern is only valid inside an array or object pattern",
                    span,
                ));
            }
        }
        Ok(())
    }

    fn compile_var(&mut self, decl: &VarDecl) -> Result<(), CompileError> {
        if self.strict && matches!(decl.name.as_str(), "eval" | "arguments") {
            return Err(CompileError::new(
                format!("invalid strict-mode binding '{}'", decl.name),
                decl.span,
            ));
        }
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
        } else if matches!(decl.kind, kagura_ast::VarKind::Var)
            && let Some(Resolved::Local { slot, .. }) = self.resolve(&decl.name)
        {
            slot
        } else {
            let Some(slot) = self.declare(
                &decl.name,
                matches!(
                    decl.kind,
                    kagura_ast::VarKind::Var | kagura_ast::VarKind::Let
                ),
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
        let strict = self.strict || has_use_strict(body);
        let arrow = name == "<arrow>";
        for param in params {
            if !names.insert(param.name.as_str()) && (strict || arrow) {
                return Err(CompileError::new(
                    format!("duplicate parameter '{}'", param.name),
                    param.span,
                ));
            }
            if strict && matches!(param.name.as_str(), "eval" | "arguments") {
                return Err(CompileError::new(
                    format!("invalid strict-mode parameter '{}'", param.name),
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
        let mut compiler = FunctionCompiler::new(
            name,
            params,
            body,
            true,
            strict,
            self.constants,
            self.functions,
            self.feedback,
        );
        compiler.outer = outer;
        compiler.hoist_function_scope(body);
        if let Some(self_name) = self_name
            && let Some(slot) = compiler.declare(self_name, false, span)
        {
            compiler.self_slot = Some(slot);
        }
        compiler.compile_arguments(span)?;
        compiler.compile_param_defaults(params)?;
        compiler.compile_rest_param(params)?;
        let patterns = compiler.parameter_patterns.clone();
        for (slot, pattern) in patterns {
            compiler.bind_pattern(&pattern, slot, kagura_ast::VarKind::Let, span, false)?;
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
                    self.uses_env = true;
                    self.emit_u16(Op::LoadCapture, level, *span);
                    self.emit_u16_raw(slot, *span);
                }
                Some(Resolved::Iteration { depth, slot, .. }) => {
                    self.emit_u16(Op::LoadIter, depth, *span);
                    self.emit_u16_raw(slot, *span);
                }
                None => {
                    if is_builtin_global(name) {
                        let index = self.add_const(Const::String(name.clone()));
                        self.emit_u32(Op::LoadBuiltin, index, *span);
                    } else {
                        let index = self.add_const(Const::String(name.clone()));
                        self.emit_u32(Op::LoadGlobal, index, *span);
                    }
                }
            },
            Expr::Unary { op, expr, span } => {
                if matches!(op, UnaryOp::Delete) {
                    match expr.as_ref() {
                        Expr::Ident { span: target, .. } if self.strict => {
                            return Err(CompileError::new(
                                "delete of an unqualified identifier in strict mode",
                                *target,
                            ));
                        }
                        Expr::Member {
                            object, property, ..
                        } => {
                            self.compile_expr(object)?;
                            let index = self.add_const(Const::String(property.clone()));
                            self.emit_u32(Op::DeleteProp, index, *span);
                        }
                        Expr::Index { object, index, .. } => {
                            self.compile_expr(object)?;
                            self.compile_expr(index)?;
                            self.emit(Op::DeleteIndex, *span);
                        }
                        other => {
                            self.compile_expr(other)?;
                            self.emit(Op::Pop, *span);
                            self.emit_const(Const::True, *span);
                        }
                    }
                    return Ok(());
                }
                if matches!(op, UnaryOp::TypeOf)
                    && let Expr::Ident { name, .. } = expr.as_ref()
                    && self.resolve(name).is_none()
                    && !is_builtin_global(name)
                {
                    self.emit_const(Const::String("undefined".into()), *span);
                    return Ok(());
                }
                self.compile_expr(expr)?;
                match op {
                    UnaryOp::Neg => self.emit(Op::Neg, *span),
                    UnaryOp::Not => self.emit(Op::Not, *span),
                    UnaryOp::BitNot => self.emit(Op::BitNot, *span),
                    UnaryOp::Plus => {
                        self.emit(Op::Neg, *span);
                        self.emit(Op::Neg, *span);
                    }
                    UnaryOp::TypeOf => self.emit(Op::TypeOf, *span),
                    UnaryOp::Void => {
                        self.emit(Op::Pop, *span);
                        self.emit_const(Const::Undefined, *span);
                    }
                    UnaryOp::Delete => unreachable!(),
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
            Expr::CompoundAssign {
                target,
                op,
                value,
                span,
            } => {
                let op = binary_op(*op);
                match target {
                    AssignTarget::Ident(name) => {
                        if self.strict && matches!(name.as_str(), "eval" | "arguments") {
                            return Err(CompileError::new(
                                format!("assignment to '{name}' in strict mode"),
                                *span,
                            ));
                        }
                        if let Some(resolved) = self.resolve(name) {
                            if !resolved.mutable() {
                                return Err(CompileError::new(
                                    format!("assignment to constant '{name}'"),
                                    *span,
                                ));
                            }
                            self.emit_load_resolved(resolved, *span);
                            self.compile_expr(value)?;
                            self.emit(op, *span);
                            self.emit(Op::Dup, *span);
                            self.emit_store_resolved(resolved, *span);
                        } else {
                            if self.strict {
                                return Err(CompileError::new(
                                    format!("assignment to undeclared '{name}' in strict mode"),
                                    *span,
                                ));
                            }
                            let index = self.add_const(Const::String(name.clone()));
                            self.emit_u32(Op::LoadGlobal, index, *span);
                            self.compile_expr(value)?;
                            self.emit(op, *span);
                            self.emit(Op::Dup, *span);
                            self.emit_u32(Op::StoreGlobal, index, *span);
                        }
                    }
                    AssignTarget::Member { object, property } => {
                        let object_slot = self.hidden_local(*span);
                        let value_slot = self.hidden_local(*span);
                        let result_slot = self.hidden_local(*span);
                        self.compile_expr(object)?;
                        self.emit_u16(Op::StoreLocal, object_slot, *span);
                        self.compile_expr(value)?;
                        self.emit_u16(Op::StoreLocal, value_slot, *span);
                        let key = self.add_const(Const::String(property.clone()));
                        self.emit_u16(Op::LoadLocal, object_slot, *span);
                        self.emit_get_prop(key, *span);
                        self.emit_u16(Op::LoadLocal, value_slot, *span);
                        self.emit(op, *span);
                        self.emit_u16(Op::StoreLocal, result_slot, *span);
                        self.emit_u16(Op::LoadLocal, object_slot, *span);
                        self.emit_u16(Op::LoadLocal, result_slot, *span);
                        self.emit_set_prop_value(key, *span);
                        self.emit(Op::Pop, *span);
                        self.emit_u16(Op::LoadLocal, result_slot, *span);
                    }
                    AssignTarget::Index { object, index } => {
                        let object_slot = self.hidden_local(*span);
                        let key_slot = self.hidden_local(*span);
                        let value_slot = self.hidden_local(*span);
                        let result_slot = self.hidden_local(*span);
                        self.compile_expr(object)?;
                        self.emit_u16(Op::StoreLocal, object_slot, *span);
                        self.compile_expr(index)?;
                        self.emit(Op::ToPropertyKey, *span);
                        self.emit_u16(Op::StoreLocal, key_slot, *span);
                        self.compile_expr(value)?;
                        self.emit_u16(Op::StoreLocal, value_slot, *span);
                        self.emit_u16(Op::LoadLocal, object_slot, *span);
                        self.emit_u16(Op::LoadLocal, key_slot, *span);
                        self.emit(Op::GetIndex, *span);
                        self.emit_u16(Op::LoadLocal, value_slot, *span);
                        self.emit(op, *span);
                        self.emit_u16(Op::StoreLocal, result_slot, *span);
                        self.emit_u16(Op::LoadLocal, object_slot, *span);
                        self.emit_u16(Op::LoadLocal, key_slot, *span);
                        self.emit_u16(Op::LoadLocal, result_slot, *span);
                        self.emit(Op::SetIndexValue, *span);
                        self.emit(Op::Pop, *span);
                        self.emit_u16(Op::LoadLocal, result_slot, *span);
                    }
                }
            }
            Expr::Assign {
                target,
                value,
                span,
            } => match target {
                AssignTarget::Ident(name) => {
                    if self.strict && matches!(name.as_str(), "eval" | "arguments") {
                        return Err(CompileError::new(
                            format!("assignment to '{name}' in strict mode"),
                            *span,
                        ));
                    }
                    let resolved = self.resolve(name);
                    if resolved.is_none() {
                        if self.strict {
                            return Err(CompileError::new(
                                format!("assignment to undeclared '{name}' in strict mode"),
                                *span,
                            ));
                        }
                        self.compile_expr(value)?;
                        self.emit(Op::Dup, *span);
                        let index = self.add_const(Const::String(name.clone()));
                        self.emit_u32(Op::StoreGlobal, index, *span);
                        return Ok(());
                    }
                    let resolved = resolved.expect("resolved assignment");
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
                            self.uses_env = true;
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
                    self.emit_set_prop_value(index, *span);
                }
                AssignTarget::Index { object, index } => {
                    self.compile_expr(object)?;
                    self.compile_expr(index)?;
                    self.compile_expr(value)?;
                    self.emit(Op::SetIndexValue, *span);
                }
            },
            Expr::DestructureAssign {
                pattern,
                value,
                span,
            } => {
                let source = self.hidden_local(*span);
                self.compile_expr(value)?;
                self.emit_u16(Op::StoreLocal, source, *span);
                self.bind_pattern(pattern, source, kagura_ast::VarKind::Var, *span, true)?;
                self.emit_u16(Op::LoadLocal, source, *span);
            }
            Expr::SuperCall { args, span } => {
                self.compile_expr(&Expr::This { span: *span })?;
                let class_key = self.add_const(Const::String("__kaguraClass".into()));
                self.emit_get_prop(class_key, *span);
                let super_key = self.add_const(Const::String("__kaguraSuper".into()));
                self.emit_get_prop(super_key, *span);
                self.compile_expr(&Expr::This { span: *span })?;
                self.compile_args(args)?;
                self.emit(Op::CallThis, *span);
                self.emit_u8_raw(args.len(), *span);
            }
            Expr::Await { expr, span } => {
                self.compile_expr(expr)?;
                self.emit(Op::Await, *span);
            }
            Expr::SuperMember { property, span } => {
                self.compile_expr(&Expr::This { span: *span })?;
                let class_key = self.add_const(Const::String("__kaguraClass".into()));
                self.emit_get_prop(class_key, *span);
                let super_key = self.add_const(Const::String("__kaguraSuper".into()));
                self.emit_get_prop(super_key, *span);
                let prototype_key = self.add_const(Const::String("prototype".into()));
                self.emit_get_prop(prototype_key, *span);
                let key = self.add_const(Const::String(property.clone()));
                self.emit_get_prop(key, *span);
            }
            Expr::Update {
                target,
                prefix,
                decrement,
                span,
            } => {
                let op = if *decrement { Op::Sub } else { Op::Add };
                match target {
                    AssignTarget::Ident(name) => {
                        if self.strict && matches!(name.as_str(), "eval" | "arguments") {
                            return Err(CompileError::new(
                                format!("update of '{name}' in strict mode"),
                                *span,
                            ));
                        }
                        let Some(resolved) = self.resolve(name) else {
                            if self.strict {
                                return Err(CompileError::new(
                                    format!("update of undeclared '{name}' in strict mode"),
                                    *span,
                                ));
                            }
                            let index = self.add_const(Const::String(name.clone()));
                            self.emit_u32(Op::LoadGlobal, index, *span);
                            if *prefix {
                                self.emit_const(Const::Number(1.0), *span);
                                self.emit(op, *span);
                                self.emit(Op::Dup, *span);
                                self.emit_u32(Op::StoreGlobal, index, *span);
                            } else {
                                self.emit(Op::Dup, *span);
                                self.emit_const(Const::Number(1.0), *span);
                                self.emit(op, *span);
                                self.emit_u32(Op::StoreGlobal, index, *span);
                                self.emit(Op::Pop, *span);
                                self.emit_u32(Op::LoadGlobal, index, *span);
                            }
                            return Ok(());
                        };
                        if !resolved.mutable() {
                            return Err(CompileError::new(
                                format!("update of constant '{name}'"),
                                *span,
                            ));
                        }
                        self.emit_load_resolved(resolved, *span);
                        if *prefix {
                            self.emit_const(Const::Number(1.0), *span);
                            self.emit(op, *span);
                            self.emit(Op::Dup, *span);
                            self.emit_store_resolved(resolved, *span);
                        } else {
                            self.emit(Op::Dup, *span);
                            self.emit_const(Const::Number(1.0), *span);
                            self.emit(op, *span);
                            self.emit_store_resolved(resolved, *span);
                        }
                    }
                    AssignTarget::Member { object, property } => {
                        let object_slot = self.hidden_local(*span);
                        let old_slot = self.hidden_local(*span);
                        let new_slot = self.hidden_local(*span);
                        self.compile_expr(object)?;
                        self.emit_u16(Op::StoreLocal, object_slot, *span);
                        let key = self.add_const(Const::String(property.clone()));
                        self.emit_u16(Op::LoadLocal, object_slot, *span);
                        self.emit_get_prop(key, *span);
                        self.emit_u16(Op::StoreLocal, old_slot, *span);
                        self.emit_u16(Op::LoadLocal, old_slot, *span);
                        self.emit_const(Const::Number(1.0), *span);
                        self.emit(op, *span);
                        self.emit_u16(Op::StoreLocal, new_slot, *span);
                        self.emit_u16(Op::LoadLocal, object_slot, *span);
                        self.emit_u16(Op::LoadLocal, new_slot, *span);
                        self.emit_set_prop_value(key, *span);
                        if *prefix {
                            self.emit_u16(Op::LoadLocal, new_slot, *span);
                        } else {
                            self.emit(Op::Pop, *span);
                            self.emit_u16(Op::LoadLocal, old_slot, *span);
                        }
                    }
                    AssignTarget::Index { object, index } => {
                        let object_slot = self.hidden_local(*span);
                        let index_slot = self.hidden_local(*span);
                        let value_slot = self.hidden_local(*span);
                        self.compile_expr(object)?;
                        self.emit_u16(Op::StoreLocal, object_slot, *span);
                        self.compile_expr(index)?;
                        self.emit_u16(Op::StoreLocal, index_slot, *span);
                        self.emit_u16(Op::LoadLocal, object_slot, *span);
                        self.emit_u16(Op::LoadLocal, index_slot, *span);
                        self.emit(Op::GetIndex, *span);
                        if !*prefix {
                            self.emit(Op::Dup, *span);
                        }
                        self.emit_const(Const::Number(1.0), *span);
                        self.emit(op, *span);
                        self.emit_u16(Op::StoreLocal, value_slot, *span);
                        self.emit_u16(Op::LoadLocal, object_slot, *span);
                        self.emit_u16(Op::LoadLocal, index_slot, *span);
                        self.emit_u16(Op::LoadLocal, value_slot, *span);
                        self.emit(Op::SetIndexValue, *span);
                        if !*prefix {
                            self.emit(Op::Pop, *span);
                        }
                    }
                }
            }
            Expr::Conditional {
                condition,
                then_branch,
                else_branch,
                span,
            } => {
                self.compile_expr(condition)?;
                let else_jump = self.emit_jump(Op::JumpIfFalse, *span);
                self.compile_expr(then_branch)?;
                let end_jump = self.emit_jump(Op::Jump, *span);
                self.patch_jump(else_jump, self.code.len());
                self.compile_expr(else_branch)?;
                self.patch_jump(end_jump, self.code.len());
            }
            Expr::Call { callee, args, span } => {
                if let Expr::SuperMember { .. } = callee.as_ref() {
                    self.compile_expr(callee)?;
                    self.compile_expr(&Expr::This { span: *span })?;
                    self.compile_args(args)?;
                    self.emit(Op::CallThis, *span);
                    self.emit_u8_raw(args.len(), *span);
                    return Ok(());
                }
                if let Expr::Ident { name, .. } = callee.as_ref() {
                    if self.resolve(name).is_some() {
                        self.compile_expr(callee)?;
                        self.compile_args(args)?;
                        self.emit(Op::CallValue, *span);
                        self.emit_u8_raw(args.len(), *span);
                    } else if let Some(id) = crate::builtins::BuiltinId::from_path(name) {
                        self.compile_args(args)?;
                        self.emit_u16(Op::CallBuiltin, id as u16, *span);
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
                            && !path.contains(".prototype")
                            && !path.ends_with(".call")
                            && !path.ends_with(".apply")
                            && !path.ends_with(".bind")
                    });
                    if let Some(path) = host_path {
                        if let Some(id) = crate::builtins::BuiltinId::from_path(&path) {
                            self.compile_args(args)?;
                            self.emit_u16(Op::CallBuiltin, id as u16, *span);
                            self.emit_u8_raw(args.len(), *span);
                        } else {
                            self.compile_args(args)?;
                            let index = self.add_const(Const::String(path));
                            self.emit_u32(Op::CallHost, index, *span);
                            self.emit_u8_raw(args.len(), *span);
                        }
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
            Expr::New { callee, args, span } => {
                if let Expr::Ident { name, .. } = callee.as_ref()
                    && self.resolve(name).is_none()
                {
                    self.compile_args(args)?;
                    let index = self.add_const(Const::String(name.clone()));
                    self.emit_u32(Op::NewName, index, *span);
                    self.emit_u8_raw(args.len(), *span);
                } else {
                    self.compile_expr(callee)?;
                    self.compile_args(args)?;
                    self.emit(Op::NewValue, *span);
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
                self.emit_get_prop(index, *span);
            }
            Expr::Object { properties, span } => {
                self.emit(Op::NewObject, *span);
                for property in properties {
                    if property.spread {
                        self.compile_expr(&property.value)?;
                        self.emit(Op::SpreadObject, property.span);
                        continue;
                    }
                    self.emit(Op::Dup, *span);
                    self.compile_expr(&property.value)?;
                    let index = self.add_const(Const::String(property.key.clone()));
                    self.emit_u32(Op::SetProp, index, property.span);
                }
            }
            Expr::Array {
                elements,
                spreads,
                span,
            } => {
                self.emit(Op::NewArray, *span);
                for (index, element) in elements.iter().enumerate() {
                    match element {
                        Some(element) if spreads.get(index).copied().unwrap_or(false) => {
                            self.compile_expr(element)?;
                            self.emit(Op::SpreadArray, element.span());
                        }
                        Some(element) => self.compile_expr(element)?,
                        None => self.emit(Op::LoadHole, *span),
                    }
                    if !spreads.get(index).copied().unwrap_or(false) {
                        self.emit(Op::ArrayPush, *span);
                    }
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
        allow_duplicate: bool,
    ) -> Result<(), CompileError> {
        if self.predeclare(name, mutable, span) || allow_duplicate {
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

    fn emit_load_resolved(&mut self, resolved: Resolved, span: Span) {
        match resolved {
            Resolved::Local { slot, .. } => self.emit_u16(Op::LoadLocal, slot, span),
            Resolved::Capture { level, slot, .. } => {
                self.uses_env = true;
                self.emit_u16(Op::LoadCapture, level, span);
                self.emit_u16_raw(slot, span);
            }
            Resolved::Iteration { depth, slot, .. } => {
                self.emit_u16(Op::LoadIter, depth, span);
                self.emit_u16_raw(slot, span);
            }
        }
    }

    fn emit_store_resolved(&mut self, resolved: Resolved, span: Span) {
        match resolved {
            Resolved::Local { slot, .. } => self.emit_u16(Op::StoreLocal, slot, span),
            Resolved::Capture { level, slot, .. } => {
                self.uses_env = true;
                self.emit_u16(Op::StoreCapture, level, span);
                self.emit_u16_raw(slot, span);
            }
            Resolved::Iteration { depth, slot, .. } => {
                self.emit_u16(Op::StoreIter, depth, span);
                self.emit_u16_raw(slot, span);
            }
        }
    }

    fn declare_parameter(&mut self, name: &str) -> u16 {
        let slot = self.locals.len() as u16;
        self.locals.push(Local {
            name: name.to_owned(),
            mutable: true,
            predeclared: false,
            active: true,
        });
        slot
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

    fn add_feedback(&mut self, kind: FeedbackKind) -> u32 {
        let index = self.feedback.len() as u32;
        self.feedback.push(kind);
        index
    }

    fn emit_get_prop(&mut self, key: u32, span: Span) {
        let feedback = self.add_feedback(FeedbackKind::PropertyLoad);
        self.emit_u32(Op::GetPropIC, key, span);
        self.emit_u32_raw(feedback, span);
    }

    fn emit_set_prop_value(&mut self, key: u32, span: Span) {
        let feedback = self.add_feedback(FeedbackKind::PropertyStore);
        self.emit_u32(Op::SetPropValueIC, key, span);
        self.emit_u32_raw(feedback, span);
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

    fn emit_u32_raw(&mut self, value: u32, span: Span) {
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

fn is_builtin_global(name: &str) -> bool {
    matches!(
        name,
        "Error"
            | "TypeError"
            | "RangeError"
            | "ReferenceError"
            | "SyntaxError"
            | "Test262Error"
            | "EvalError"
            | "URIError"
            | "Object"
            | "Array"
            | "Math"
            | "JSON"
            | "Date"
            | "Number"
            | "String"
            | "Boolean"
            | "Function"
            | "NaN"
            | "Infinity"
            | "globalThis"
            | "performance"
    )
}

fn has_use_strict(body: &[Stmt]) -> bool {
    for stmt in body {
        match stmt {
            Stmt::Expr {
                expr: Expr::String { value, .. },
                ..
            } => {
                if value == "use strict" {
                    return true;
                }
            }
            _ => break,
        }
    }
    false
}

fn binary_op(op: BinaryOp) -> Op {
    match op {
        BinaryOp::Add => Op::Add,
        BinaryOp::Sub => Op::Sub,
        BinaryOp::Mul => Op::Mul,
        BinaryOp::Pow => Op::Pow,
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
        BinaryOp::InstanceOf => Op::InstanceOf,
        BinaryOp::BitOr => Op::BitOr,
        BinaryOp::BitXor => Op::BitXor,
        BinaryOp::BitAnd => Op::BitAnd,
        BinaryOp::Shl => Op::Shl,
        BinaryOp::Shr => Op::Shr,
        BinaryOp::UShr => Op::UShr,
        BinaryOp::And | BinaryOp::Or => unreachable!(),
    }
}
