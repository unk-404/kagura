use kagura_runtime::Runtime;
use kagura_testkit::CaptureHost;

const HELLO: &str = include_str!("../../../examples/hello.ts");
const FIB: &str = include_str!("../../../examples/fib.ts");
const OBJECTS: &str = include_str!("../../../examples/objects.ts");
const OBJECTS_WRITE: &str = include_str!("../../../examples/objects_write.ts");
const OBJECTS_MANY: &str = include_str!("../../../examples/objects_many.ts");
const ARRAYS: &str = include_str!("../../../examples/arrays.ts");
const ARRAYS_LENGTH: &str = include_str!("../../../examples/arrays_length.ts");
const STRINGS: &str = include_str!("../../../examples/strings.ts");
const ARRAYS_HOLES: &str = include_str!("../../../examples/arrays_holes.ts");
const ARRAYS_IN: &str = include_str!("../../../examples/arrays_in.ts");
const TYPEOF: &str = include_str!("../../../examples/typeof.ts");
const VOID: &str = include_str!("../../../examples/void.ts");

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
fn rewrites_string_length_and_index() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(STRINGS, &mut host).unwrap();
    assert_eq!(host.output(), "3\nb\nundefined\n");
}

#[test]
fn preserves_array_holes() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(ARRAYS_HOLES, &mut host).unwrap();
    assert_eq!(host.output(), "3\nundefined\n1\n");
}

#[test]
fn runs_in_operator() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(ARRAYS_IN, &mut host).unwrap();
    assert_eq!(host.output(), "true\nfalse\ntrue\ntrue\ntrue\nfalse\n");
}

#[test]
fn runs_typeof_operator() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(TYPEOF, &mut host).unwrap();
    assert_eq!(
        host.output(),
        "undefined\nobject\nboolean\nnumber\nstring\nobject\nobject\n"
    );
}

#[test]
fn runs_void_operator() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(VOID, &mut host).unwrap();
    assert_eq!(host.output(), "undefined\nundefined\nundefined\n");
}

#[test]
fn reads_and_writes_array_length() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(ARRAYS_LENGTH, &mut host).unwrap();
    assert_eq!(host.output(), "3\n2\nundefined\n4\nundefined\n");
}

#[test]
fn collects_script_objects_after_run() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run("let point = { x: 1 }; let values = [point];", &mut host)
        .unwrap();
    assert_eq!(runtime.object_live_bytes(), 0);
}

#[test]
fn gc_runs_during_script_allocations() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::new(64 * 1024);
    runtime
        .run(
            "for (let i = 0; i < 2000; i = i + 1) { let o = { x: i }; }",
            &mut host,
        )
        .unwrap();
    assert_eq!(runtime.object_live_bytes(), 0);
}

#[test]
fn gc_keeps_caller_roots_alive() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::new(64 * 1024);
    runtime
        .run(
            "function make() { for (let i = 0; i < 2000; i = i + 1) { let o = { x: i }; } return 1; } let keep = { x: 7 }; let result = make(); console.log(keep.x + result);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "8\n");
}

#[test]
fn reports_syntax_errors() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    let error = runtime.run("let value: number = ;", &mut host).unwrap_err();
    assert!(error.to_string().contains("expected expression"));
}
