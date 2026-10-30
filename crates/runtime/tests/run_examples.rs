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
const FUNCTIONS: &str = include_str!("../../../examples/functions.ts");
const CLOSURES: &str = include_str!("../../../examples/closures.ts");
const CLOSURES_MUTABLE: &str = include_str!("../../../examples/closures_mutable.ts");
const CLOSURES_NESTED: &str = include_str!("../../../examples/closures_nested.ts");
const CLOSURES_DECL: &str = include_str!("../../../examples/closures_decl.ts");
const CLOSURES_METHOD: &str = include_str!("../../../examples/closures_method.ts");
const CLOSURES_GC: &str = include_str!("../../../examples/closures_gc.ts");
const CLOSURES_SHARED: &str = include_str!("../../../examples/closures_shared.ts");
const HOISTING: &str = include_str!("../../../examples/hoisting.ts");
const HIGHER_ORDER: &str = include_str!("../../../examples/higher_order.ts");
const TOP_CLOSURE: &str = include_str!("../../../examples/top_closure.ts");
const NAMED_FUNCTION: &str = include_str!("../../../examples/named_function.ts");
const LOOP_CLOSURES: &str = include_str!("../../../examples/loop_closures.ts");
const LOOP_CLOSURES_NESTED: &str = include_str!("../../../examples/loop_closures_nested.ts");
const LOOP_CLOSURES_CONTROL: &str = include_str!("../../../examples/loop_closures_control.ts");
const LOOP_CLOSURES_GC: &str = include_str!("../../../examples/loop_closures_gc.ts");
const LOOP_BODY_CLOSURES: &str = include_str!("../../../examples/loop_body_closures.ts");
const TRY_CATCH: &str = include_str!("../../../examples/try_catch.ts");
const TRY_FINALLY: &str = include_str!("../../../examples/try_finally.ts");
const TRY_NESTED: &str = include_str!("../../../examples/try_nested.ts");
const TRY_CROSS_FUNCTION: &str = include_str!("../../../examples/try_cross_function.ts");

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
fn runs_function_expressions_and_arrows() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(FUNCTIONS, &mut host).unwrap();
    assert_eq!(host.output(), "5\n12\n6\n");
}

#[test]
fn runs_closures() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(CLOSURES, &mut host).unwrap();
    assert_eq!(host.output(), "8\n");
}

#[test]
fn runs_mutable_closures() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(CLOSURES_MUTABLE, &mut host).unwrap();
    assert_eq!(host.output(), "1\n2\n");
}

#[test]
fn runs_nested_closures() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(CLOSURES_NESTED, &mut host).unwrap();
    assert_eq!(host.output(), "6\n");
}

#[test]
fn runs_nested_function_declarations() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(CLOSURES_DECL, &mut host).unwrap();
    assert_eq!(host.output(), "12\n");
}

#[test]
fn runs_function_valued_properties() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(CLOSURES_METHOD, &mut host).unwrap();
    assert_eq!(host.output(), "8\n");
}

#[test]
fn keeps_closure_environments_alive_during_gc() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::new(64 * 1024);
    runtime.run(CLOSURES_GC, &mut host).unwrap();
    assert_eq!(host.output(), "function\n7\n");
}

#[test]
fn closures_share_one_environment() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(CLOSURES_SHARED, &mut host).unwrap();
    assert_eq!(host.output(), "1\n11\n");
}

#[test]
fn hoists_functions_and_captures_later_bindings() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(HOISTING, &mut host).unwrap();
    assert_eq!(host.output(), "7\n9\n");
}

#[test]
fn passes_functions_and_returns_closures() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(HIGHER_ORDER, &mut host).unwrap();
    assert_eq!(host.output(), "12\n11\n");
}

#[test]
fn captures_top_level_script_bindings() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(TOP_CLOSURE, &mut host).unwrap();
    assert_eq!(host.output(), "9\n");
}

#[test]
fn closure_can_capture_block_binding() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "let read; { let value = 3; read = () => value; } console.log(read());",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "3\n");
}

#[test]
fn hides_block_bindings_after_scope() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    assert!(runtime.run("{ let value = 1; } value;", &mut host).is_err());
}

#[test]
fn named_function_expression_is_recursive() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(NAMED_FUNCTION, &mut host).unwrap();
    assert_eq!(host.output(), "120\n");
}

#[test]
fn captured_tdz_errors_before_initialization() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    let result = runtime.run(
        "function read() { return value; } read(); let value = 1;",
        &mut host,
    );
    assert!(result.is_err());
}

#[test]
fn captures_per_iteration_loop_bindings() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(LOOP_CLOSURES, &mut host).unwrap();
    assert_eq!(host.output(), "0\n1\n2\n");
}

#[test]
fn captures_nested_loop_bindings() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(LOOP_CLOSURES_NESTED, &mut host).unwrap();
    assert_eq!(host.output(), "0\n1\n10\n11\n");
}

#[test]
fn loop_closure_environments_survive_break_and_continue() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(LOOP_CLOSURES_CONTROL, &mut host).unwrap();
    assert_eq!(host.output(), "0\n1\n3\n");
}

#[test]
fn keeps_loop_closure_environments_alive_during_gc() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::new(64 * 1024);
    runtime.run(LOOP_CLOSURES_GC, &mut host).unwrap();
    assert_eq!(host.output(), "0\n1\n2\n");
}

#[test]
fn captures_body_let_bindings_per_iteration() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(LOOP_BODY_CLOSURES, &mut host).unwrap();
    assert_eq!(host.output(), "0\n1\n2\n");
}

#[test]
fn runs_throw_and_catch() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(TRY_CATCH, &mut host).unwrap();
    assert_eq!(host.output(), "42\nafter\n");
}

#[test]
fn runs_finally_after_catch() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(TRY_FINALLY, &mut host).unwrap();
    assert_eq!(host.output(), "boom\nfinally\nafter\n");
}

#[test]
fn runs_nested_try_catch() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(TRY_NESTED, &mut host).unwrap();
    assert_eq!(host.output(), "2\n");
}

#[test]
fn uncaught_throw_fails_execution() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    assert!(runtime.run("throw 1;", &mut host).is_err());
}

#[test]
fn catches_throw_from_called_function() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(TRY_CROSS_FUNCTION, &mut host).unwrap();
    assert_eq!(host.output(), "7\nafter\n");
}

#[test]
fn catches_missing_host_error() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "try { missingHost(); } catch (error) { console.log(\"caught\"); }",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "caught\n");
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
