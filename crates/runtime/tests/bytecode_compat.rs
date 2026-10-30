use kagura_runtime::Runtime;
use kagura_testkit::CaptureHost;

const FIB_V1: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/fib.kbc");
const HELLO_V1: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/hello.kbc");
const HELLO_V2: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v2/hello.kbc");
const OBJECTS_V3: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v3/objects.kbc");
const OBJECTS_WRITE_V4: &[u8] =
    include_bytes!("../../../testdata/compat/bytecode/v4/objects_write.kbc");

#[test]
fn executes_frozen_v1_fib_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(FIB_V1, &mut host).unwrap();
    assert_eq!(host.output(), "sum=88\n");
}

#[test]
fn executes_frozen_v1_hello_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(HELLO_V1, &mut host).unwrap();
    assert_eq!(host.output(), "hello, Kagura\n");
}

#[test]
fn executes_frozen_v2_call_host_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(HELLO_V2, &mut host).unwrap();
    assert_eq!(host.output(), "hello, Kagura\n");
}

#[test]
fn executes_frozen_v3_object_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(OBJECTS_V3, &mut host).unwrap();
    assert_eq!(host.output(), "3\n");
}

#[test]
fn executes_frozen_v4_property_write_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(OBJECTS_WRITE_V4, &mut host).unwrap();
    assert_eq!(host.output(), "7\n");
}
