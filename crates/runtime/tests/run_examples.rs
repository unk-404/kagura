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
const BUILTINS: &str = include_str!("../../../examples/builtins.ts");
const BUILTINS_CALLBACKS: &str = include_str!("../../../examples/builtins_callbacks.ts");
const BUILTINS_CHAIN: &str = include_str!("../../../examples/builtins_chain.ts");
const BUILTINS_CALLBACKS_MORE: &str = include_str!("../../../examples/builtins_callbacks_more.ts");
const OBJECT_BUILTINS: &str = include_str!("../../../examples/object_builtins.ts");
const PROTOTYPE_CHAIN: &str = include_str!("../../../examples/prototype_chain.ts");
const BUILTINS_HOLES: &str = include_str!("../../../examples/builtins_holes.ts");
const WHILE_CLOSURES: &str = include_str!("../../../examples/while_closures.ts");
const METHOD_THIS: &str = include_str!("../../../examples/method_this.ts");
const METHOD_SHADOWING: &str = include_str!("../../../examples/method_shadowing.ts");
const ARROW_THIS: &str = include_str!("../../../examples/arrow_this.ts");
const DEFAULT_PARAMS: &str = include_str!("../../../examples/default_params.ts");
const REST_PARAMS: &str = include_str!("../../../examples/rest_params.ts");
const ARGUMENTS: &str = include_str!("../../../examples/arguments.ts");
const OBJECT_CHECKS: &str = include_str!("../../../examples/object_checks.ts");
const FINALLY_RETURN: &str = include_str!("../../../examples/finally_return.ts");
const OBJECT_ASSIGN: &str = include_str!("../../../examples/object_assign.ts");
const FINALLY_LOOPS: &str = include_str!("../../../examples/finally_loops.ts");
const FINALLY_NESTED_LOOPS: &str = include_str!("../../../examples/finally_nested_loops.ts");
const BUILTIN_EXTRAS: &str = include_str!("../../../examples/builtin_extras.ts");
const ARRAY_SORT: &str = include_str!("../../../examples/array_sort.ts");
const FUNCTION_CALL_APPLY: &str = include_str!("../../../examples/function_call_apply.ts");
const FUNCTION_BIND: &str = include_str!("../../../examples/function_bind.ts");
const PROPERTY_DESCRIPTOR: &str = include_str!("../../../examples/property_descriptor.ts");
const OBJECT_FREEZE: &str = include_str!("../../../examples/object_freeze.ts");
const ARRAY_FREEZE: &str = include_str!("../../../examples/array_freeze.ts");
const OBJECT_SEAL: &str = include_str!("../../../examples/object_seal.ts");
const ARRAY_STATIC: &str = include_str!("../../../examples/array_static.ts");
const CONFIGURABLE_PROPERTY: &str = include_str!("../../../examples/configurable_property.ts");
const DELETE_OPERATOR: &str = include_str!("../../../examples/delete_operator.ts");
const OWN_PROPERTIES: &str = include_str!("../../../examples/own_properties.ts");
const EXTENSIBILITY: &str = include_str!("../../../examples/extensibility.ts");

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
fn catches_runtime_error_in_same_frame() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "try { undefined.missing; } catch (error) { console.log(\"caught\"); }",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "caught\n");
}

#[test]
fn catches_index_runtime_error() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "try { let value = undefined; value[0]; } catch (error) { console.log(\"index\"); }",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "index\n");
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
fn runs_array_and_string_builtin_methods() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(BUILTINS, &mut host).unwrap();
    assert_eq!(host.output(), "3\n1-2-3\n3\ntrue\n1\n1\ne\nHELLO\n2\nel\n");
}

#[test]
fn runs_array_callback_methods() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(BUILTINS_CALLBACKS, &mut host).unwrap();
    assert_eq!(host.output(), "2,4,6\n2\n6\n");
}

#[test]
fn runs_chained_array_callbacks() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(BUILTINS_CHAIN, &mut host).unwrap();
    assert_eq!(host.output(), "2,3,4\n");
}

#[test]
fn runs_search_and_reduce_array_callbacks() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(BUILTINS_CALLBACKS_MORE, &mut host).unwrap();
    assert_eq!(host.output(), "true\ntrue\n2\n10\n10\n");
}

#[test]
fn runs_object_builtins() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(OBJECT_BUILTINS, &mut host).unwrap();
    assert_eq!(host.output(), "a,b\n1,2\na:1\n0,2\n");
}

#[test]
fn follows_object_prototype_chain() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(PROTOTYPE_CHAIN, &mut host).unwrap();
    assert_eq!(host.output(), "1\n2\ntrue\ntrue\n3\n");
}

#[test]
fn runs_hole_aware_array_callbacks() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(BUILTINS_HOLES, &mut host).unwrap();
    assert_eq!(host.output(), "2,,6\n1,3\n2\nfalse\n-1\ntrue\n4\n");
}

#[test]
fn captures_while_body_bindings_per_iteration() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(WHILE_CLOSURES, &mut host).unwrap();
    assert_eq!(host.output(), "0\n1\n");
}

#[test]
fn binds_this_for_method_calls() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(METHOD_THIS, &mut host).unwrap();
    assert_eq!(host.output(), "5\nno receiver\n");
}

#[test]
fn user_methods_shadow_builtin_names() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(METHOD_SHADOWING, &mut host).unwrap();
    assert_eq!(host.output(), "11\n2,4\n");
}

#[test]
fn arrows_capture_lexical_this() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(ARROW_THIS, &mut host).unwrap();
    assert_eq!(host.output(), "7\n7\n");
}

#[test]
fn applies_default_parameters() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(DEFAULT_PARAMS, &mut host).unwrap();
    assert_eq!(host.output(), "6\n8\n8\n10\n11\n");
}

#[test]
fn collects_rest_parameters() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(REST_PARAMS, &mut host).unwrap();
    assert_eq!(host.output(), "1-2-3\n\n2\n");
}

#[test]
fn exposes_arguments_object() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(ARGUMENTS, &mut host).unwrap();
    assert_eq!(host.output(), "2:a\n3\ntrue\n0\n");
}

#[test]
fn runs_object_checks() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(OBJECT_CHECKS, &mut host).unwrap();
    assert_eq!(host.output(), "false\ntrue\ntrue\nfalse\ntrue\nfalse\n");
}

#[test]
fn runs_object_assign() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(OBJECT_ASSIGN, &mut host).unwrap();
    assert_eq!(host.output(), "true\n1\n2\n3\nx,y\n4\n");
}

#[test]
fn runs_finally_on_break_and_continue() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(FINALLY_LOOPS, &mut host).unwrap();
    assert_eq!(host.output(), "0,f0,f1\n0ff2f\n");
}

#[test]
fn runs_nested_finally_on_break_and_continue() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(FINALLY_NESTED_LOOPS, &mut host).unwrap();
    assert_eq!(host.output(), "inner,outer\nioio\n");
}

#[test]
fn runs_extra_array_and_string_builtins() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(BUILTIN_EXTRAS, &mut host).unwrap();
    assert_eq!(
        host.output(),
        "3,2,1\n1,2,3,4,5\na|b|c\ntrue\ntrue\nababab\ncde\ncde\n"
    );
}

#[test]
fn sorts_arrays_with_and_without_comparator() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(ARRAY_SORT, &mut host).unwrap();
    assert_eq!(host.output(), "a,b,c\n1,2,3,10\n1,10,2\n1,3,\n");
}

#[test]
fn calls_functions_with_call_and_apply() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(FUNCTION_CALL_APPLY, &mut host).unwrap();
    assert_eq!(host.output(), "13\n17\n10\ntrue\n");
}

#[test]
fn binds_this_and_partial_arguments() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(FUNCTION_BIND, &mut host).unwrap();
    assert_eq!(host.output(), "Hello Ada!\nHello Ada?\n3\nHi A!\n");
}

#[test]
fn reads_and_creates_property_descriptors() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(PROPERTY_DESCRIPTOR, &mut host).unwrap();
    assert_eq!(host.output(), "42\n42\nfalse\nfalse\n0\n");
}

#[test]
fn freezes_objects_and_enforces_writable() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(OBJECT_FREEZE, &mut host).unwrap();
    assert_eq!(host.output(), "1\nundefined\ntrue\nfalse\n");
}

#[test]
fn freezes_arrays_and_ignores_mutation() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(ARRAY_FREEZE, &mut host).unwrap();
    assert_eq!(host.output(), "1,2\n2\ntrue\n");
}

#[test]
fn seals_objects_and_arrays() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(OBJECT_SEAL, &mut host).unwrap();
    assert_eq!(host.output(), "2\nundefined\ntrue\n3,2\n2\ntrue\n");
}

#[test]
fn runs_array_static_and_has_own() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(ARRAY_STATIC, &mut host).unwrap();
    assert_eq!(host.output(), "1,2,3\na,b\n1,2\ntrue\nfalse\n");
}

#[test]
fn enforces_non_configurable_properties() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(CONFIGURABLE_PROPERTY, &mut host).unwrap();
    assert_eq!(host.output(), "locked\n1\nsame\n");
}

#[test]
fn runs_delete_operator() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(DELETE_OPERATOR, &mut host).unwrap();
    assert_eq!(
        host.output(),
        "true\nundefined\nfalse\nfalse\ntrue\n,2\n2\n"
    );
}

#[test]
fn reads_and_defines_own_properties() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(OWN_PROPERTIES, &mut host).unwrap();
    assert_eq!(host.output(), "hidden\n2\na\n1\n2\n");
}

#[test]
fn runs_extensibility_and_descriptor_map() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(EXTENSIBILITY, &mut host).unwrap();
    assert_eq!(host.output(), "undefined\nfalse\ntrue\n1\ntrue\n");
}

#[test]
fn runs_finally_on_return_and_throw() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(FINALLY_RETURN, &mut host).unwrap();
    assert_eq!(
        host.output(),
        "a\n1\n3\nc\n4\ninner\nboom\nbody\nfinally\nouter\ncatch finally\nreplaced\noverride\n"
    );
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
