use kagura_runtime::Runtime;
use kagura_testkit::CaptureHost;

const HELLO: &str = include_str!("../../../examples/hello.ts");
const FIB: &str = include_str!("../../../examples/fib.ts");
const OBJECTS: &str = include_str!("../../../examples/objects.ts");
const OBJECTS_WRITE: &str = include_str!("../../../examples/objects_write.ts");
const OBJECTS_MANY: &str = include_str!("../../../examples/objects_many.ts");
const ARRAYS: &str = include_str!("../../../examples/arrays.ts");
const ARRAYS_LENGTH: &str = include_str!("../../../examples/arrays_length.ts");

#[test]
fn runs_hello_world() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(HELLO, &mut host).unwrap();
    assert_eq!(host.output(), "hello, Kagura\n");
}

#[test]
fn runs_fib() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(FIB, &mut host).unwrap();
    assert_eq!(host.output(), "sum=88\n");
}

#[test]
fn runs_object_literals() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(OBJECTS, &mut host).unwrap();
    assert_eq!(host.output(), "3\n");
}

#[test]
fn writes_object_properties() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(OBJECTS_WRITE, &mut host).unwrap();
    assert_eq!(host.output(), "7\n");
}

#[test]
fn grows_property_arrays() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(OBJECTS_MANY, &mut host).unwrap();
    assert_eq!(host.output(), "9\n");
}

#[test]
fn runs_arrays_and_index_writes() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(ARRAYS, &mut host).unwrap();
    assert_eq!(host.output(), "13\n");
}

#[test]
fn reads_and_writes_array_length() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(ARRAYS_LENGTH, &mut host).unwrap();
    assert_eq!(host.output(), "3\n2\nundefined\n4\nundefined\n");
}

#[test]
fn reports_syntax_errors() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    let error = runtime.run("let value: number = ;", &mut host).unwrap_err();
    assert!(error.to_string().contains("expected expression"));
}
