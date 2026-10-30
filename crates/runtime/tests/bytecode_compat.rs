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
const LOOP_CLOSURES: &[u8] =
    include_bytes!("../../../testdata/compat/bytecode/v1/loop_closures.kbc");
const LOOP_CLOSURES_NESTED: &[u8] =
    include_bytes!("../../../testdata/compat/bytecode/v1/loop_closures_nested.kbc");
const TRY_CATCH: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/try_catch.kbc");
const TRY_FINALLY: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/try_finally.kbc");
const TRY_CROSS_FUNCTION: &[u8] =
    include_bytes!("../../../testdata/compat/bytecode/v1/try_cross_function.kbc");
const BUILTINS: &[u8] = include_bytes!("../../../testdata/compat/bytecode/v1/builtins.kbc");
const BUILTINS_CALLBACKS: &[u8] =
    include_bytes!("../../../testdata/compat/bytecode/v1/builtins_callbacks.kbc");
const BUILTINS_CHAIN: &[u8] =
    include_bytes!("../../../testdata/compat/bytecode/v1/builtins_chain.kbc");
const BUILTINS_CALLBACKS_MORE: &[u8] =
    include_bytes!("../../../testdata/compat/bytecode/v1/builtins_callbacks_more.kbc");
const OBJECT_BUILTINS: &[u8] =
    include_bytes!("../../../testdata/compat/bytecode/v1/object_builtins.kbc");
const PROTOTYPE_CHAIN: &[u8] =
    include_bytes!("../../../testdata/compat/bytecode/v1/prototype_chain.kbc");
const BUILTINS_HOLES: &[u8] =
    include_bytes!("../../../testdata/compat/bytecode/v1/builtins_holes.kbc");
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
fn executes_v1_loop_closure_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(LOOP_CLOSURES, &mut host).unwrap();
    assert_eq!(host.output(), "0\n1\n2\n");
}

#[test]
fn executes_v1_nested_loop_closure_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run_bytecode(LOOP_CLOSURES_NESTED, &mut host)
        .unwrap();
    assert_eq!(host.output(), "0\n1\n10\n11\n");
}

#[test]
fn executes_v1_try_catch_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(TRY_CATCH, &mut host).unwrap();
    assert_eq!(host.output(), "42\nafter\n");
}

#[test]
fn executes_v1_try_finally_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(TRY_FINALLY, &mut host).unwrap();
    assert_eq!(host.output(), "boom\nfinally\nafter\n");
}

#[test]
fn executes_v1_cross_function_throw_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(TRY_CROSS_FUNCTION, &mut host).unwrap();
    assert_eq!(host.output(), "7\nafter\n");
}

#[test]
fn executes_v1_array_length_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(ARRAYS_LENGTH, &mut host).unwrap();
    assert_eq!(host.output(), "3\n2\nundefined\n4\nundefined\n");
}

#[test]
fn executes_v1_builtin_method_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(BUILTINS, &mut host).unwrap();
    assert_eq!(host.output(), "3\n1-2-3\n3\ntrue\n1\n1\ne\nHELLO\n2\nel\n");
}

#[test]
fn executes_v1_callback_builtin_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(BUILTINS_CALLBACKS, &mut host).unwrap();
    assert_eq!(host.output(), "2,4,6\n2\n6\n");
}

#[test]
fn executes_v1_chained_builtin_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(BUILTINS_CHAIN, &mut host).unwrap();
    assert_eq!(host.output(), "2,3,4\n");
}

#[test]
fn executes_v1_search_builtin_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run_bytecode(BUILTINS_CALLBACKS_MORE, &mut host)
        .unwrap();
    assert_eq!(host.output(), "true\ntrue\n2\n10\n10\n");
}

#[test]
fn executes_v1_object_builtin_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(OBJECT_BUILTINS, &mut host).unwrap();
    assert_eq!(host.output(), "a,b\n1,2\na:1\n0,2\n");
}

#[test]
fn executes_v1_prototype_chain_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(PROTOTYPE_CHAIN, &mut host).unwrap();
    assert_eq!(host.output(), "1\n2\ntrue\ntrue\n3\n");
}

#[test]
fn executes_v1_hole_builtin_bytecode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run_bytecode(BUILTINS_HOLES, &mut host).unwrap();
    assert_eq!(host.output(), "2,,6\n1,3\n2\nfalse\n-1\ntrue\n4\n");
}
