use kagura_bytecode::{Op, compile};
use kagura_parser::parse;

#[test]
fn emits_gc_checks_in_entry_loop_body() {
    let program = parse("for (let i = 0; i < 3; i = i + 1) { let o = { x: i }; }").unwrap();
    let module = compile(&program).unwrap();
    let entry = &module.functions[module.entry];
    assert!(entry.code.contains(&(Op::GcCheck as u8)));
}
