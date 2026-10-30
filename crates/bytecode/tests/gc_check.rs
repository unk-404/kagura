use kagura_bytecode::{Op, compile};
use kagura_parser::parse;

#[test]
fn emits_feedback_slots_for_property_access() {
    let program = parse("let object = { value: 1 }; object.value = object.value + 1;").unwrap();
    let module = compile(&program).unwrap();
    let entry = &module.functions[module.entry];
    assert!(entry.code.contains(&(Op::GetPropIC as u8)));
    assert!(entry.code.contains(&(Op::SetPropValueIC as u8)));
    assert_eq!(module.feedback.len(), 2);
}

#[test]
fn emits_gc_checks_in_entry_loop_body() {
    let program = parse("for (let i = 0; i < 3; i = i + 1) { let o = { x: i }; }").unwrap();
    let module = compile(&program).unwrap();
    let entry = &module.functions[module.entry];
    assert!(entry.code.contains(&(Op::GcCheck as u8)));
    assert!(!entry.safepoints.is_empty());
    for safepoint in &entry.safepoints {
        assert_eq!(entry.code[safepoint.pc as usize], Op::GcCheck as u8);
    }
}
