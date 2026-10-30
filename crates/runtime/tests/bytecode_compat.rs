use kagura_runtime::Runtime;
use kagura_testkit::CaptureHost;

const FIB: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/fib.kbc");
const HELLO: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/hello.kbc");
const OBJECTS: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/objects.kbc");
const OBJECTS_WRITE: &[u8] =
    include_bytes!("../../../testdata/compat/bytecode/v1/objects_write.kbc");
const OBJECTS_MANY: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/objects_many.kbc");
const ARRAYS: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/arrays.kbc");
const ARRAYS_HOLES: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/arrays_holes.kbc");
const ARRAYS_IN: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/arrays_in.kbc");
const TYPEOF: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/typeof.kbc");
const VOID: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/void.kbc");
const FUNCTIONS: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/functions.kbc");
const CLOSURES_NESTED: &[u8] =
    include_bytes!("../../../testdata/compat/bytecode/v1/closures_nested.kbc");
const CLOSURES_SHARED: &[u8] =
    include_bytes!("../../../testdata/compat/bytecode/v1/closures_shared.kbc");
const ARRAYS_LENGTH: &[u8] =
    include_bytes!("../../../testdata/compat/bytecode/v1/arrays_length.kbc");

#[test]
fn executes_v1_fib_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(FIB, &mut host).unwrap();
    assert_eq!(host.output(), "sum=88\n");
}

#[test]
fn executes_v1_hello_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(HELLO, &mut host).unwrap();
    assert_eq!(host.output(), "hello, Kagura\n");
}

#[test]
fn executes_v1_object_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(OBJECTS, &mut host).unwrap();
    assert_eq!(host.output(), "3\n");
}

#[test]
fn executes_v1_property_write_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(OBJECTS_WRITE, &mut host).unwrap();
    assert_eq!(host.output(), "7\n");
}

#[test]
fn executes_v1_property_array_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(OBJECTS_MANY, &mut host).unwrap();
    assert_eq!(host.output(), "9\n");
}

#[test]
fn executes_v1_array_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(ARRAYS, &mut host).unwrap();
    assert_eq!(host.output(), "13\n");
}

#[test]
fn executes_v1_array_holes_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(ARRAYS_HOLES, &mut host).unwrap();
    assert_eq!(host.output(), "3\nundefined\n1\n");
}

#[test]
fn executes_v1_in_operator_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(ARRAYS_IN, &mut host).unwrap();
    assert_eq!(host.output(), "true\nfalse\ntrue\ntrue\ntrue\nfalse\n");
}

#[test]
fn executes_v1_typeof_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(TYPEOF, &mut host).unwrap();
    assert_eq!(
        host.output(),
        "undefined\nobject\nboolean\nnumber\nstring\nobject\nobject\n"
    );
}

#[test]
fn executes_v1_void_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(VOID, &mut host).unwrap();
    assert_eq!(host.output(), "undefined\nundefined\nundefined\n");
}

#[test]
fn executes_v1_function_value_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(FUNCTIONS, &mut host).unwrap();
    assert_eq!(host.output(), "5\n12\n6\n");
}

#[test]
fn executes_v1_nested_closure_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(CLOSURES_NESTED, &mut host).unwrap();
    assert_eq!(host.output(), "6\n");
}

#[test]
fn executes_v1_shared_closure_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(CLOSURES_SHARED, &mut host).unwrap();
    assert_eq!(host.output(), "1\n11\n");
}

#[test]
fn executes_v1_array_length_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(ARRAYS_LENGTH, &mut host).unwrap();
    assert_eq!(host.output(), "3\n2\nundefined\n4\nundefined\n");
}
