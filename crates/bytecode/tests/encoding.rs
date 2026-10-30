use kagura_bytecode::{DecodeError, Module, VERSION, compile};
use kagura_parser::parse;

fn sample() -> Module {
    let program = parse(
        "function add(a: number, b: number): number { return a + b; }\nlet value: number = add(1, 2);\nconsole.log(value);",
    )
    .unwrap();
    compile(&program).unwrap()
}

#[test]
fn round_trips_current_bytecode() {
    let module = sample();
    let bytes = module.encode();
    let decoded = Module::decode(&bytes).unwrap();
    assert_eq!(decoded.version, VERSION);
    assert_eq!(decoded.constants, module.constants);
    assert_eq!(decoded.feedback, module.feedback);
    assert_eq!(decoded.entry, module.entry);
    assert_eq!(decoded.functions.len(), module.functions.len());
    for (left, right) in decoded.functions.iter().zip(&module.functions) {
        assert_eq!(left.name, right.name);
        assert_eq!(left.param_count, right.param_count);
        assert_eq!(left.local_count, right.local_count);
        assert_eq!(left.code, right.code);
        assert_eq!(left.spans, right.spans);
        assert_eq!(left.safepoints, right.safepoints);
    }
}

#[test]
fn rejects_newer_bytecode_version() {
    let mut bytes = sample().encode();
    bytes[4..6].copy_from_slice(&(VERSION + 1).to_le_bytes());
    assert!(matches!(
        Module::decode(&bytes),
        Err(DecodeError::UnsupportedVersion(_))
    ));
}

#[test]
fn rejects_truncated_bytecode() {
    let bytes = sample().encode();
    assert!(matches!(
        Module::decode(&bytes[..5]),
        Err(DecodeError::Truncated)
    ));
}

#[test]
fn rejects_wrong_magic() {
    let mut bytes = sample().encode();
    bytes[0] = b'X';
    assert!(matches!(Module::decode(&bytes), Err(DecodeError::BadMagic)));
}
