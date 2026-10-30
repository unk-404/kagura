use kagura_runtime::Runtime;
use kagura_testkit::CaptureHost;

const FIB_V1: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/fib.kbc");
const HELLO_V1: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/hello.kbc");

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
