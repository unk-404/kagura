use kagura_parser::parse;

#[test]
fn parses_ts_annotations() {
    let program = parse(
        "let value: number = 1;\nfunction add(a: number, b: number): number { return a + b; }",
    )
    .unwrap();
    assert_eq!(program.body.len(), 2);
}

#[test]
fn parses_interface_without_runtime_semantics() {
    let program = parse("interface Point { x: number; y: number; }\nlet p: Point = 1;").unwrap();
    assert_eq!(program.body.len(), 2);
}

#[test]
fn parses_object_literal() {
    let program = parse("let point = { x: 1, y: 2 };").unwrap();
    let kagura_ast::Stmt::Var(decl) = &program.body[0] else {
        panic!("expected var declaration");
    };
    assert!(matches!(decl.init, Some(kagura_ast::Expr::Object { .. })));
}

#[test]
fn parses_member_assignment_target() {
    let program = parse("point.x = 2;").unwrap();
    let kagura_ast::Stmt::Expr { expr, .. } = &program.body[0] else {
        panic!("expected expression statement");
    };
    assert!(matches!(
        expr,
        kagura_ast::Expr::Assign {
            target: kagura_ast::AssignTarget::Member { .. },
            ..
        }
    ));
}

#[test]
fn parses_array_literal_and_index_assignment() {
    let program = parse("let values = [1, 2]; values[0] = 3;").unwrap();
    let kagura_ast::Stmt::Var(decl) = &program.body[0] else {
        panic!("expected var declaration");
    };
    assert!(matches!(decl.init, Some(kagura_ast::Expr::Array { .. })));
    let kagura_ast::Stmt::Expr { expr, .. } = &program.body[1] else {
        panic!("expected expression statement");
    };
    assert!(matches!(
        expr,
        kagura_ast::Expr::Assign {
            target: kagura_ast::AssignTarget::Index { .. },
            ..
        }
    ));
}

#[test]
fn rejects_lexical_declaration_in_statement_position() {
    let error = parse("if (true) let x;").unwrap_err();
    assert!(error.message.contains("single-statement position"));
}

#[test]
fn parses_throw_and_try_catch() {
    let program = parse("try { throw 1; } catch (error) { 2; } finally { 3; }").unwrap();
    let kagura_ast::Stmt::Try {
        catch_param,
        catch_block,
        finally_block,
        ..
    } = &program.body[0]
    else {
        panic!("expected try statement");
    };
    assert_eq!(catch_param.as_deref(), Some("error"));
    assert!(catch_block.is_some());
    assert!(finally_block.is_some());
}

#[test]
fn parses_function_expressions_and_arrows() {
    let program =
        parse("let f = function (a) { return a; }; let g = (a, b) => a + b; let h = x => x;")
            .unwrap();
    assert_eq!(program.body.len(), 3);
    let kagura_ast::Stmt::Var(f) = &program.body[0] else {
        panic!("expected variable declaration");
    };
    assert!(matches!(f.init, Some(kagura_ast::Expr::Function { .. })));
    let kagura_ast::Stmt::Var(g) = &program.body[1] else {
        panic!("expected variable declaration");
    };
    assert!(matches!(g.init, Some(kagura_ast::Expr::Arrow { .. })));
    let kagura_ast::Stmt::Var(h) = &program.body[2] else {
        panic!("expected variable declaration");
    };
    assert!(matches!(h.init, Some(kagura_ast::Expr::Arrow { .. })));
}

#[test]
fn parenthesized_expression_is_not_an_arrow() {
    let program = parse("let value = (1 + 2) * 3;").unwrap();
    assert_eq!(program.body.len(), 1);
}

#[test]
fn parses_array_holes() {
    let program = parse("let values = [1, , 3];").unwrap();
    let kagura_ast::Stmt::Var(decl) = &program.body[0] else {
        panic!("expected var declaration");
    };
    let Some(kagura_ast::Expr::Array { elements, .. }) = &decl.init else {
        panic!("expected array literal");
    };
    assert_eq!(elements.len(), 3);
    assert!(elements[0].is_some());
    assert!(elements[1].is_none());
    assert!(elements[2].is_some());
}

#[test]
fn rejects_unterminated_string() {
    let error = parse("let value = \"oops;").unwrap_err();
    assert!(error.message.contains("unterminated string"));
}
