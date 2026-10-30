//! 针对真实、手工拼装 wasm 二进制文件的行为测试。
//!
//! 下面的 helper 直接编码二进制格式（LEB128、section 框架、opcode），
//! 保证 decoder 和解释器面对的是真实 wasm bytes，而不是 mock。

use kagura_wasm::{
    AddressWidth, Instance, InstantiateError, Memory, MemoryCap, MemoryType, Module, Trap, ValType,
    Value, WASM_PAGE_SIZE, WasmFeatures, default_features,
};

const SECTION_TYPE: u8 = 1;
const SECTION_IMPORT: u8 = 2;
const SECTION_FUNCTION: u8 = 3;
const SECTION_MEMORY: u8 = 5;
const SECTION_EXPORT: u8 = 7;
const SECTION_CODE: u8 = 10;

const OP_END: u8 = 0x0b;
const OP_RETURN: u8 = 0x0f;
const OP_I32_CONST: u8 = 0x41;
const OP_I32_ADD: u8 = 0x6a;
const OP_I32_SUB: u8 = 0x6b;
const OP_I32_MUL: u8 = 0x6c;
const OP_I32_DIV_S: u8 = 0x6d;
const OP_I32_EXTEND8_S: u8 = 0xc0;
const OP_LOCAL_GET: u8 = 0x20;
const OP_LOCAL_TEE: u8 = 0x22;

fn push_uleb(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn push_sleb_i32(out: &mut Vec<u8>, mut value: i32) {
    loop {
        let byte = (value as u8) & 0x7f;
        value >>= 7;
        let sign_bit = byte & 0x40 != 0;
        let done = (value == 0 && !sign_bit) || (value == -1 && sign_bit);
        out.push(if done { byte } else { byte | 0x80 });
        if done {
            break;
        }
    }
}

fn section(id: u8, payload: Vec<u8>) -> Vec<u8> {
    let mut encoded = vec![id];
    push_uleb(&mut encoded, payload.len() as u32);
    encoded.extend_from_slice(&payload);
    encoded
}

fn module(sections: &[Vec<u8>]) -> Vec<u8> {
    let mut encoded = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    for section in sections {
        encoded.extend_from_slice(section);
    }
    encoded
}

fn type_section(params: &[u8], results: &[u8]) -> Vec<u8> {
    let mut payload = Vec::new();
    push_uleb(&mut payload, 1);
    payload.push(0x60);
    push_uleb(&mut payload, params.len() as u32);
    payload.extend_from_slice(params);
    push_uleb(&mut payload, results.len() as u32);
    payload.extend_from_slice(results);
    section(SECTION_TYPE, payload)
}

fn function_section(type_index: u32) -> Vec<u8> {
    let mut payload = Vec::new();
    push_uleb(&mut payload, 1);
    push_uleb(&mut payload, type_index);
    section(SECTION_FUNCTION, payload)
}

fn export_func_section(name: &str, function_index: u32) -> Vec<u8> {
    let mut payload = Vec::new();
    push_uleb(&mut payload, 1);
    push_uleb(&mut payload, name.len() as u32);
    payload.extend_from_slice(name.as_bytes());
    payload.push(0x00);
    push_uleb(&mut payload, function_index);
    section(SECTION_EXPORT, payload)
}

fn memory_section(min_pages: u32, max_pages: Option<u32>) -> Vec<u8> {
    let mut payload = Vec::new();
    push_uleb(&mut payload, 1);
    match max_pages {
        Some(max_pages) => {
            payload.push(0x01);
            push_uleb(&mut payload, min_pages);
            push_uleb(&mut payload, max_pages);
        }
        None => {
            payload.push(0x00);
            push_uleb(&mut payload, min_pages);
        }
    }
    section(SECTION_MEMORY, payload)
}

fn code_section(body: &[u8]) -> Vec<u8> {
    let mut payload = Vec::new();
    push_uleb(&mut payload, 1);
    push_uleb(&mut payload, body.len() as u32);
    payload.extend_from_slice(body);
    section(SECTION_CODE, payload)
}

// `(func (export "add") (param i32 i32) (result i32) local.get 0 local.get 1 i32.add)`
fn add_module() -> Vec<u8> {
    let body = [
        0x00, // no local declarations
        OP_LOCAL_GET,
        0x00,
        OP_LOCAL_GET,
        0x01,
        OP_I32_ADD,
        OP_END,
    ];
    module(&[
        type_section(&[0x7f, 0x7f], &[0x7f]),
        function_section(0),
        export_func_section("add", 0),
        code_section(&body),
    ])
}

// `(func (export "id") (param i32) (result i32) local.get 0 return)`
fn id_module() -> Vec<u8> {
    let body = [0x00, OP_LOCAL_GET, 0x00, OP_RETURN, OP_END];
    module(&[
        type_section(&[0x7f], &[0x7f]),
        function_section(0),
        export_func_section("id", 0),
        code_section(&body),
    ])
}

// `(func (export "id64") (param i64) (result i64) local.get 0 return)`
fn id64_module() -> Vec<u8> {
    let body = [0x00, OP_LOCAL_GET, 0x00, OP_RETURN, OP_END];
    module(&[
        type_section(&[0x7e], &[0x7e]),
        function_section(0),
        export_func_section("id64", 0),
        code_section(&body),
    ])
}

// `(func (export "answer") (result i32) i32.const <value>)`
fn const_module(value: i32) -> Vec<u8> {
    let mut body = vec![0x00, OP_I32_CONST];
    push_sleb_i32(&mut body, value);
    body.push(OP_END);
    module(&[
        type_section(&[], &[0x7f]),
        function_section(0),
        export_func_section("answer", 0),
        code_section(&body),
    ])
}

// `(func (export "ops") (param i32) (result i32)
//      local.get 0 i32.const 3 i32.mul local.get 0 i32.sub)`
fn ops_module() -> Vec<u8> {
    let body = [
        0x00,
        OP_LOCAL_GET,
        0x00,
        OP_I32_CONST,
        0x03,
        OP_I32_MUL,
        OP_LOCAL_GET,
        0x00,
        OP_I32_SUB,
        OP_END,
    ];
    module(&[
        type_section(&[0x7f], &[0x7f]),
        function_section(0),
        export_func_section("ops", 0),
        code_section(&body),
    ])
}

// `(func (export "tee") (param i32) (result i32)
//      local.get 0 i32.const 1 i32.add local.tee 0 local.get 0 i32.add)`
fn local_tee_module() -> Vec<u8> {
    let body = [
        0x00,
        OP_LOCAL_GET,
        0x00,
        OP_I32_CONST,
        0x01,
        OP_I32_ADD,
        OP_LOCAL_TEE,
        0x00,
        OP_LOCAL_GET,
        0x00,
        OP_I32_ADD,
        OP_END,
    ];
    module(&[
        type_section(&[0x7f], &[0x7f]),
        function_section(0),
        export_func_section("tee", 0),
        code_section(&body),
    ])
}

// 一个带单个 memory 和 trivial export 的模块。
fn memory_module(min_pages: u32, max_pages: Option<u32>) -> Vec<u8> {
    let body = [0x00, OP_I32_CONST, 0x00, OP_END];
    module(&[
        type_section(&[], &[0x7f]),
        function_section(0),
        memory_section(min_pages, max_pages),
        export_func_section("noop", 0),
        code_section(&body),
    ])
}

// `(module (memory i64 1))`：memory64 模块，初始 1 页。
fn memory64_module() -> Vec<u8> {
    let mut payload = Vec::new();
    push_uleb(&mut payload, 1);
    payload.push(0x04);
    push_uleb(&mut payload, 1);
    module(&[section(SECTION_MEMORY, payload)])
}

// `(module (type (func)) (import "env" "f" (func (type 0))))`
fn imported_module() -> Vec<u8> {
    let mut imports = Vec::new();
    push_uleb(&mut imports, 1);
    push_uleb(&mut imports, 3);
    imports.extend_from_slice(b"env");
    push_uleb(&mut imports, 1);
    imports.extend_from_slice(b"f");
    imports.push(0x00);
    push_uleb(&mut imports, 0);
    module(&[type_section(&[], &[]), section(SECTION_IMPORT, imports)])
}

// 合法 MVP 模块，但 body 使用解释器子集之外的 `i32.div_s`。
fn div_module() -> Vec<u8> {
    let body = [
        0x00,
        OP_LOCAL_GET,
        0x00,
        OP_LOCAL_GET,
        0x01,
        OP_I32_DIV_S,
        OP_END,
    ];
    module(&[
        type_section(&[0x7f, 0x7f], &[0x7f]),
        function_section(0),
        export_func_section("div", 0),
        code_section(&body),
    ])
}

// 语法合法但使用 post-MVP `i32.extend8_s` 的模块。
fn sign_extension_module() -> Vec<u8> {
    let body = [0x00, OP_LOCAL_GET, 0x00, OP_I32_EXTEND8_S, OP_END];
    module(&[
        type_section(&[0x7f], &[0x7f]),
        function_section(0),
        export_func_section("ext", 0),
        code_section(&body),
    ])
}

#[test]
fn decodes_hardcoded_id_module() {
    // Hand-assembled encoding of:
    //   (module
    //     (type (func (param i32) (result i32)))
    //     (func (type 0) (param i32) (result i32) local.get 0 return)
    //     (export "id" (func 0)))
    let bytes: [u8; 37] = [
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // magic + version
        0x01, 0x06, 0x01, 0x60, 0x01, 0x7f, 0x01, 0x7f, // type section
        0x03, 0x02, 0x01, 0x00, // function section
        0x07, 0x06, 0x01, 0x02, 0x69, 0x64, 0x00, 0x00, // export "id"
        0x0a, 0x07, 0x01, 0x05, 0x00, 0x20, 0x00, 0x0f, 0x0b, // code section
    ];
    assert_eq!(bytes.as_slice(), id_module().as_slice());

    let module = Module::decode(&bytes).expect("hand-assembled module decodes");
    let mut instance = Instance::new(&module).expect("instance");
    assert_eq!(
        instance.invoke("id", &[Value::I32(-7)]).expect("id call"),
        vec![Value::I32(-7)]
    );
}

#[test]
fn decodes_valid_module_and_reports_statistics() {
    let bytes = add_module();
    let module = Module::decode(&bytes).expect("add module decodes");
    assert_eq!(module.version(), 1);
    assert_eq!(module.bytes(), bytes.as_slice());
    assert_eq!(module.types().len(), 1);
    assert_eq!(module.types()[0].params(), &[ValType::I32, ValType::I32]);
    assert_eq!(module.types()[0].results(), &[ValType::I32]);
    assert_eq!(module.functions().len(), 1);
    assert_eq!(module.functions()[0].type_index(), 0);
    assert!(module.functions()[0].locals().is_empty());
    assert!(module.memories().is_empty());
    assert!(module.imports().is_empty());
    assert_eq!(module.exports().len(), 1);
    assert_eq!(module.exports()[0].name, "add");
    assert_eq!(module.exports()[0].index, 0);
    assert_eq!(module.start_function(), None);

    let stats = module.stats();
    assert_eq!(stats.byte_len, bytes.len());
    assert_eq!(stats.type_count, 1);
    assert_eq!(stats.function_count, 1);
    assert_eq!(stats.export_count, 1);
    assert_eq!(stats.memory_count, 0);
    assert_eq!(stats.import_count, 0);
    assert!(stats.code_bytes > 0);
}

#[test]
fn executes_i32_add_with_wrapping_semantics() {
    let module = Module::decode(&add_module()).expect("add module decodes");
    let mut instance = Instance::new(&module).expect("instance");
    assert_eq!(
        instance
            .invoke("add", &[Value::I32(2), Value::I32(40)])
            .expect("add"),
        vec![Value::I32(42)]
    );
    assert_eq!(
        instance
            .invoke("add", &[Value::I32(i32::MAX), Value::I32(1)])
            .expect("wrapping add"),
        vec![Value::I32(i32::MIN)]
    );
}

#[test]
fn executes_i32_const_and_local_get_return() {
    let module = Module::decode(&const_module(42)).expect("const module decodes");
    let mut instance = Instance::new(&module).expect("instance");
    assert_eq!(
        instance.invoke("answer", &[]).expect("const call"),
        vec![Value::I32(42)]
    );

    let module = Module::decode(&const_module(-7)).expect("signed const module decodes");
    let mut instance = Instance::new(&module).expect("instance");
    assert_eq!(
        instance.invoke("answer", &[]).expect("signed const call"),
        vec![Value::I32(-7)]
    );

    let module = Module::decode(&id_module()).expect("id module decodes");
    let mut instance = Instance::new(&module).expect("instance");
    assert_eq!(
        instance.invoke("id", &[Value::I32(1234)]).expect("id call"),
        vec![Value::I32(1234)]
    );
}

#[test]
fn passes_i64_through_locals_and_return() {
    // 即使 v1 只执行 i32 算术，值模型/local/return 也必须保留 i64。
    let module = Module::decode(&id64_module()).expect("id64 module decodes");
    let mut instance = Instance::new(&module).expect("instance");
    let value = -(1_i64 << 40);
    assert_eq!(
        instance
            .invoke("id64", &[Value::I64(value)])
            .expect("id64 call"),
        vec![Value::I64(value)]
    );
}

#[test]
fn executes_sub_mul_and_local_tee() {
    let module = Module::decode(&ops_module()).expect("ops module decodes");
    let mut instance = Instance::new(&module).expect("instance");
    assert_eq!(
        instance.invoke("ops", &[Value::I32(7)]).expect("ops call"),
        vec![Value::I32(14)]
    );

    let module = Module::decode(&local_tee_module()).expect("tee module decodes");
    let mut instance = Instance::new(&module).expect("instance");
    assert_eq!(
        instance.invoke("tee", &[Value::I32(5)]).expect("tee call"),
        vec![Value::I32(12)]
    );
}

#[test]
fn rejects_invalid_and_truncated_binaries() {
    assert!(Module::decode(b"not wasm").is_err());
    assert!(Module::decode(&[]).is_err());

    let bytes = add_module();
    let error = Module::decode(&bytes[..bytes.len() - 2]).unwrap_err();
    assert!(
        error.offset().is_some(),
        "truncation should carry an offset"
    );
    assert!(!error.message().is_empty());

    let mut bad_magic = bytes;
    bad_magic[0] = 0xff;
    let error = Module::decode(&bad_magic).unwrap_err();
    assert!(error.offset().is_some());
}

#[test]
fn rejects_post_mvp_features_at_decode_time() {
    let error = Module::decode(&sign_extension_module()).unwrap_err();
    assert!(error.offset().is_some());
    assert!(error.to_string().contains("offset"));
}

#[test]
fn reports_unsupported_operator_at_run_time() {
    let module = Module::decode(&div_module()).expect("div module is valid MVP");
    let mut instance = Instance::new(&module).expect("instance");
    match instance.invoke("div", &[Value::I32(6), Value::I32(2)]) {
        Err(Trap::UnsupportedOperator { operator, offset }) => {
            assert!(
                operator.contains("I32DivS"),
                "unexpected operator: {operator}"
            );
            assert!(offset > 0);
        }
        other => panic!("expected UnsupportedOperator, got {other:?}"),
    }
}

#[test]
fn validates_call_contract() {
    let module = Module::decode(&add_module()).expect("add module decodes");
    let mut instance = Instance::new(&module).expect("instance");
    assert!(matches!(
        instance.invoke("add", &[Value::I32(1)]),
        Err(Trap::ArgumentCount {
            expected: 2,
            actual: 1
        })
    ));
    assert!(matches!(
        instance.invoke("add", &[Value::I32(1), Value::I64(2)]),
        Err(Trap::ArgumentType {
            index: 1,
            expected: ValType::I32,
            actual: ValType::I64
        })
    ));
    assert!(matches!(
        instance.invoke("missing", &[]),
        Err(Trap::MissingExport { .. })
    ));
    assert!(matches!(
        instance.invoke_function(99, &[]),
        Err(Trap::InvalidFunction { index: 99 })
    ));
}

#[test]
fn rejects_host_imports_at_instantiation() {
    let module = Module::decode(&imported_module()).expect("import module decodes");
    assert_eq!(module.imports().len(), 1);
    assert_eq!(module.imports()[0].module, "env");
    assert_eq!(module.imports()[0].name, "f");
    assert!(matches!(
        Instance::new(&module),
        Err(InstantiateError::UnsupportedImport { .. })
    ));
}

#[test]
fn instantiates_linear_memory_and_enforces_cap() {
    let module = Module::decode(&memory_module(1, Some(4))).expect("memory module decodes");
    assert_eq!(module.memories(), &[MemoryType::new(1, Some(4))]);
    assert_eq!(module.stats().memory_count, 1);

    let mut instance = Instance::new(&module).expect("instance");
    assert_eq!(instance.memories().len(), 1);
    assert_eq!(instance.memory(0).expect("memory").pages(), 1);
    assert_eq!(
        instance.memory(0).expect("memory").data().len() as u64,
        WASM_PAGE_SIZE
    );
    assert_eq!(instance.memory_mut(0).expect("memory").grow(2), Some(1));
    assert_eq!(instance.memory(0).expect("memory").pages(), 3);
    assert_eq!(instance.memory_mut(0).expect("memory").grow(2), None);
    assert_eq!(instance.memory(0).expect("memory").max_pages(), 4);

    let mut capped =
        Instance::with_memory_cap(&module, MemoryCap::from_pages(2, AddressWidth::W32))
            .expect("capped instance");
    assert_eq!(capped.memory(0).expect("memory").max_pages(), 2);
    assert_eq!(capped.memory_mut(0).expect("memory").grow(1), Some(1));
    assert_eq!(capped.memory_mut(0).expect("memory").grow(1), None);

    assert!(matches!(
        Instance::with_memory_cap(&module, MemoryCap::from_pages(0, AddressWidth::W32)),
        Err(InstantiateError::Memory(_))
    ));
}

#[test]
fn memory64_is_gated_but_architecturally_supported() {
    let bytes = memory64_module();

    // v1 默认门禁关闭 memory64，并明确报告缺失 capability。
    let error = Module::decode(&bytes).unwrap_err();
    assert_eq!(error.missing_feature(), Some(WasmFeatures::MEMORY64));
    assert!(error.to_string().contains("memory64"));

    let module = Module::decode_with_features(&bytes, default_features() | WasmFeatures::MEMORY64)
        .expect("memory64 module decodes when the feature is enabled");
    assert_eq!(
        module.memories(),
        &[MemoryType::with_address_width(1, None, AddressWidth::W64)]
    );
    assert_eq!(module.memory_address_width(), AddressWidth::W64);

    // 实例化走 64 位默认预算，只 commit 1 页，不会真的分配 4 GiB。
    let instance = Instance::new(&module).expect("memory64 instance");
    assert_eq!(
        instance.memory(0).expect("memory").address_width(),
        AddressWidth::W64
    );
    assert_eq!(instance.memory(0).expect("memory").pages(), 1);
    assert_eq!(
        instance.memory(0).expect("memory").cap().bytes(),
        4 * 1024 * 1024 * 1024
    );

    // 窄于 memory 地址宽度的 cap 必须被拒绝。
    assert!(matches!(
        Instance::with_memory_cap(&module, MemoryCap::default()),
        Err(InstantiateError::Memory(_))
    ));
}

#[test]
fn memory_accessors_are_bounds_checked_and_share_the_buffer() {
    let mut memory = Memory::new(
        MemoryType::new(1, Some(2)),
        MemoryCap::from_pages(2, AddressWidth::W32),
    )
    .expect("memory");
    assert!(memory.write_u32_le(0, 0x1122_3344));
    assert_eq!(memory.read_u32_le(0), Some(0x1122_3344));
    assert_eq!(
        memory.read_bytes(0, 4),
        Some(0x1122_3344u32.to_le_bytes().as_slice())
    );
    assert!(!memory.write_bytes(WASM_PAGE_SIZE - 2, &[1, 2, 3]));

    assert_eq!(memory.grow(1), Some(1));
    assert!(memory.write_u32_le(WASM_PAGE_SIZE, 7));
    assert_eq!(memory.read_u32_le(WASM_PAGE_SIZE), Some(7));
}
