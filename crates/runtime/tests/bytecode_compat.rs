use kagura_runtime::Runtime;
use kagura_testkit::CaptureHost;

const FIB: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/fib.kbc");
const HELLO: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/hello.kbc");
const OBJECTS: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/objects.kbc");
const OBJECTS_WRITE: &[u8] =
    include_bytes!("../../../testdata/compat/bytecode/v1/objects_write.kbc");
const OBJECTS_MANY: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/objects_many.kbc");
const ARRAYS: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/arrays.kbc");

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
