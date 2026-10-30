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
fn rejects_unterminated_string() {
    let error = parse("let value = \"oops;").unwrap_err();
    assert!(error.message.contains("unterminated string"));
}
