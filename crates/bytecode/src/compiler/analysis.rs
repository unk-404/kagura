use kagura_ast::{AssignTarget, Expr, ForInit, Param, Stmt};

use super::Local;

pub(super) fn params_have_closure(params: &[Param]) -> bool {
    params.iter().any(|param| {
        param
            .default
            .as_ref()
            .is_some_and(|expr| expr_has_closure(expr))
    })
}

pub(super) fn loop_has_closure(cond: Option<&Expr>, update: Option<&Expr>, body: &Stmt) -> bool {
    cond.is_some_and(expr_has_closure)
        || update.is_some_and(expr_has_closure)
        || stmt_has_closure(body)
}

pub(super) fn collect_iteration_locals(stmt: &Stmt, bindings: &mut Vec<Local>) {
    match stmt {
        Stmt::Var(decl) => {
            if matches!(
                decl.kind,
                kagura_ast::VarKind::Let | kagura_ast::VarKind::Const
            ) && !bindings.iter().any(|binding| binding.name == decl.name)
            {
                bindings.push(Local {
                    name: decl.name.clone(),
                    mutable: matches!(decl.kind, kagura_ast::VarKind::Let),
                    predeclared: false,
                    active: true,
                });
            }
        }
        Stmt::Block(body) => {
            for stmt in body {
                collect_iteration_locals(stmt, bindings);
            }
        }
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_iteration_locals(then_branch, bindings);
            if let Some(else_branch) = else_branch {
                collect_iteration_locals(else_branch, bindings);
            }
        }
        _ => {}
    }
}

pub(super) fn body_has_closure(body: &[Stmt]) -> bool {
    body.iter().any(stmt_has_closure)
}

fn stmt_has_closure(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Var(decl) => decl.init.as_ref().is_some_and(expr_has_closure),
        Stmt::Func(_) => true,
        Stmt::Block(body) => body_has_closure(body),
        Stmt::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => {
            expr_has_closure(cond)
                || stmt_has_closure(then_branch)
                || else_branch.as_deref().is_some_and(stmt_has_closure)
        }
        Stmt::While { cond, body, .. } => expr_has_closure(cond) || stmt_has_closure(body),
        Stmt::For {
            init,
            cond,
            update,
            body,
            ..
        } => {
            init.as_ref().is_some_and(|init| match init {
                ForInit::Var(decl) => decl.init.as_ref().is_some_and(expr_has_closure),
                ForInit::Expr(expr) => expr_has_closure(expr),
            }) || cond.as_ref().is_some_and(expr_has_closure)
                || update.as_ref().is_some_and(expr_has_closure)
                || stmt_has_closure(body)
        }
        Stmt::Return { value, .. } => value.as_ref().is_some_and(expr_has_closure),
        Stmt::Throw { value, .. } => expr_has_closure(value),
        Stmt::Try {
            block,
            catch_block,
            finally_block,
            ..
        } => {
            body_has_closure(block)
                || catch_block
                    .as_ref()
                    .is_some_and(|body| body_has_closure(body))
                || finally_block
                    .as_ref()
                    .is_some_and(|body| body_has_closure(body))
        }
        Stmt::Expr { expr, .. } => expr_has_closure(expr),
        Stmt::Break { .. }
        | Stmt::Continue { .. }
        | Stmt::TypeAlias { .. }
        | Stmt::Interface { .. }
        | Stmt::Empty { .. } => false,
    }
}

fn expr_has_closure(expr: &Expr) -> bool {
    match expr {
        Expr::Function { .. } | Expr::Arrow { .. } => true,
        Expr::Unary { expr, .. } | Expr::As { expr, .. } => expr_has_closure(expr),
        Expr::Binary { left, right, .. } => expr_has_closure(left) || expr_has_closure(right),
        Expr::Assign { target, value, .. } => target_has_closure(target) || expr_has_closure(value),
        Expr::Call { callee, args, .. } => {
            expr_has_closure(callee) || args.iter().any(expr_has_closure)
        }
        Expr::Member { object, .. } => expr_has_closure(object),
        Expr::Index { object, index, .. } => expr_has_closure(object) || expr_has_closure(index),
        Expr::Object { properties, .. } => properties
            .iter()
            .any(|property| expr_has_closure(&property.value)),
        Expr::Array { elements, .. } => elements.iter().flatten().any(expr_has_closure),
        Expr::Number { .. }
        | Expr::String { .. }
        | Expr::Boolean { .. }
        | Expr::Null { .. }
        | Expr::Undefined { .. }
        | Expr::This { .. }
        | Expr::Ident { .. } => false,
    }
}

fn target_has_closure(target: &AssignTarget) -> bool {
    match target {
        AssignTarget::Ident(_) => false,
        AssignTarget::Member { object, .. } => expr_has_closure(object),
        AssignTarget::Index { object, index } => {
            expr_has_closure(object) || expr_has_closure(index)
        }
    }
}

pub(super) fn member_path(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident { name, .. } => Some(name.clone()),
        Expr::Member {
            object, property, ..
        } => Some(format!("{}.{}", member_path(object)?, property)),
        _ => None,
    }
}
