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
const STRING_REPLACE: &str = include_str!("../../../examples/string_replace.ts");
const NEW_ERRORS: &str = include_str!("../../../examples/new_errors.ts");
const NEW_CONSTRUCTOR: &str = include_str!("../../../examples/new_constructor.ts");
const GLOBALS_MATH: &str = include_str!("../../../examples/globals_math.ts");
const FOR_IN: &str = include_str!("../../../examples/for_in.ts");
const JSON_PARSE: &str = include_str!("../../../examples/json_parse.ts");
const DATE_BASIC: &str = include_str!("../../../examples/date_basic.ts");
const ARRAY_INDEX: &str = include_str!("../../../examples/array_index.ts");
const ARRAY_METHODS: &str = include_str!("../../../examples/array_methods.ts");
const DUPLICATE_VAR: &str = include_str!("../../../examples/duplicate_var.ts");
const FUNCTION_CALL: &str = include_str!("../../../examples/function_call.ts");
const PRIMITIVE_WRAPPERS: &str = include_str!("../../../examples/primitive_wrappers.ts");
const ARRAY_EXTRA: &str = include_str!("../../../examples/array_extra.ts");

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
fn runs_string_replace_and_trim() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(STRING_REPLACE, &mut host).unwrap();
    assert_eq!(host.output(), "x-b-a\nx-b-x\n[hi  ]\n[  hi]\n");
}

#[test]
fn constructs_error_objects() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(NEW_ERRORS, &mut host).unwrap();
    assert_eq!(
        host.output(),
        "Error boom\nTypeError bad\nTypeError thrown\n"
    );
}

#[test]
fn constructs_user_objects() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(NEW_CONSTRUCTOR, &mut host).unwrap();
    assert_eq!(host.output(), "5\n2\n");
}

#[test]
fn runs_global_conversions_and_math() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(GLOBALS_MATH, &mut host).unwrap();
    assert_eq!(
        host.output(),
        "1 3\n1 3 1\n42 255 1.5\ntrue true\n5 31 6 3\n"
    );
}

#[test]
fn runs_for_in_loops() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(FOR_IN, &mut host).unwrap();
    assert_eq!(host.output(), "a 1\nb 2\n");
}

#[test]
fn runs_json_parse_from_serde() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(JSON_PARSE, &mut host).unwrap();
    assert_eq!(
        host.output(),
        "1 null x 2\n{\"a\":1,\"b\":[true,null,\"x\"],\"c\":{\"d\":2}}\n3\nhello\n"
    );
}

#[test]
fn runs_date_utc_operations() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(DATE_BASIC, &mut host).unwrap();
    assert_eq!(
        host.output(),
        "0\n1970-01-01T00:00:00.000Z\n1970 0 1 4\n2020-01-02T03:04:05.006Z 3 4 5 6\n1577934245006\n1577934245006\n"
    );
}

#[test]
fn distinguishes_array_indices_from_named_properties() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(ARRAY_INDEX, &mut host).unwrap();
    assert_eq!(host.output(), "named fractional first 1\n");
}

#[test]
fn runs_extended_array_methods() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(ARRAY_METHODS, &mut host).unwrap();
    assert_eq!(host.output(), "1\n1,9,3\n3 undefined\n2 0\n");
}

#[test]
fn allows_duplicate_var_and_array_prototype_lookup() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(DUPLICATE_VAR, &mut host).unwrap();
    assert_eq!(host.output(), "2 7 7\n");
}

#[test]
fn runs_function_call_and_array_prototype_methods() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(FUNCTION_CALL, &mut host).unwrap();
    assert_eq!(host.output(), "1,2 2\ntrue\n[object Array]\n1,2\n");
}

#[test]
fn runs_primitive_wrappers() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(PRIMITIVE_WRAPPERS, &mut host).unwrap();
    assert_eq!(host.output(), "3 b c abc abc\n5 5\nfalse false true\n");
}

#[test]
fn rejects_strict_mode_eval_bindings() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    let result = runtime.run(
        "\"use strict\"; function strictFunction(eval) { return eval; }",
        &mut host,
    );
    assert!(result.is_err());
}

#[test]
fn allows_duplicate_parameters_in_sloppy_functions() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "function f(a, a) { return a; } console.log(f(1, 2));",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "2\n");
}

#[test]
fn runs_extra_array_methods() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.run(ARRAY_EXTRA, &mut host).unwrap();
    assert_eq!(host.output(), "2,3,3\n1,2,3\n1,1,2,2\n0\n");
}

#[test]
fn exposes_nan_and_infinity_globals() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "console.log(NaN !== NaN, Infinity > 1e308, typeof NaN, isFinite(Infinity));",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "true true number false\n");
}

#[test]
fn runs_number_static_and_prototype_methods() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "console.log((255).toString(16), (1.5).toFixed(2), Number.isNaN(NaN), Number.isInteger(5), Number.MAX_SAFE_INTEGER);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "ff 1.50 true true 9007199254740991\n");
}

#[test]
fn runs_string_statics_and_boolean_methods() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "console.log(String.fromCharCode(65, 66), String.fromCodePoint(128512), true.toString());",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "AB 😀 true\n");
}

#[test]
fn exposes_function_constructor_surface() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "console.log(typeof Function('return 1'), typeof Function.prototype, typeof Function.prototype.call);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "function function function\n");
}

#[test]
fn runs_exponentiation_separators_and_find_last() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "console.log(2 ** 3 ** 2, 1_000 + 2, [1,2,3].findLast((x) => x < 3), [1,2,3].findLastIndex((x) => x < 3));",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "512 1002 2 1\n");
}

#[test]
fn runs_logical_assignments_and_nullish() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "var a = 0; a ||= 5; var b = 1; b &&= 7; var c = null; c ??= 9; console.log(a, b, c, null ?? 4, true ? 1 : 2);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "5 7 9 4 1\n");
}

#[test]
fn keeps_builtin_identity_stable() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "console.log(Array === Array, Object === Object, Array.prototype === Array.prototype);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "true true true\n");
}

#[test]
fn exposes_global_this() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "console.log(this === globalThis, globalThis.Array === Array, globalThis.undefined, typeof globalThis);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "true true undefined object\n");
}

#[test]
fn spreads_object_literals() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "var x = { a: 1, b: 2 }; var y = { ...x, b: 3, c: 4 }; console.log(y.a, y.b, y.c, Object.keys(y).join(','));",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "1 3 4 a,b,c\n");
}

#[test]
fn spreads_array_literals() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "var a = [1, 2]; var b = [0, ...a, 3]; console.log(b.join(','), [...a, ...b].join(','));",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "0,1,2,3 1,2,0,1,2,3\n");
}

#[test]
fn binds_array_and_object_destructuring_patterns() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "var [a, b] = [1, 2]; var { x, y: z } = { x: 3, y: 4 }; let [p, , q] = [5, 6, 7]; var [[m], { n }] = [[8], { n: 9 }]; console.log(a, b, x, z, p, q, m, n);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "1 2 3 4 5 7 8 9\n");
}

#[test]
fn supports_destructuring_defaults_and_rest() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "var [a = 1, b = 2] = [5]; var { x = 3, y = 4 } = { x: undefined }; var [first, ...rest] = [1, 2, 3, 4]; console.log(a, b, x, y, first, rest.join(','));",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "5 2 3 4 1 2,3,4\n");
}

#[test]
fn supports_object_rest_bindings() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "var { a, b: c, ...rest } = { a: 1, b: 2, d: 3, e: 4 }; console.log(a, c, Object.keys(rest).join(','), rest.d, rest.e);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "1 2 d,e 3 4\n");
}

#[test]
fn supports_destructuring_parameters() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "function f([a, b], { x, y = 7 }, ...rest) { return a + b + x + y + rest.length; } var g = ({ a = 4 }) => a; console.log(f([1, 2], { x: 3 }, 9, 9), g({}));",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "15 4\n");
}

#[test]
fn supports_destructuring_assignment_and_multiple_declarators() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "var a, b, c, d; [a, b] = [1, 2]; ({ x: c, y: d } = { x: 3, y: 4 }); console.log(a, b, c, d); var p = 0, q = 0; [p, q = 9] = [5]; console.log(p, q);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "1 2 3 4\n5 9\n");
}

#[test]
fn runs_for_of_over_arrays_and_strings() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "var total = 0; for (var x of [1, 2, 3]) total += x; var out = ''; for (const ch of 'abc') out += ch; console.log(total, out);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "6 abc\n");
}

#[test]
fn wires_constructor_prototypes_and_instanceof() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "function F(x) { this.x = x; } F.prototype.get = function () { return this.x; }; var f = new F(5); console.log(f.get(), f instanceof F, F.prototype.constructor === F);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "5 true true\n");
}

#[test]
fn runs_basic_class_declarations() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "class Point { constructor(x, y) { this.x = x; this.y = y; } sum() { return this.x + this.y; } static origin() { return new Point(0, 0); } } var p = new Point(2, 3); var o = Point.origin(); console.log(p.sum(), p instanceof Point, o.x, o.y, typeof Point);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "5 true 0 0 function\n");
}

#[test]
fn runs_public_class_fields() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "class Counter { count = 0; step = 2; static total = 10; constructor(start) { this.count = start; } add() { this.count += this.step; return this.count; } } var c = new Counter(1); console.log(c.add(), c.count, Counter.total, Object.keys(Counter.prototype).length);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "3 3 10 0\n");
}

#[test]
fn supports_default_derived_class_constructors() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "class A { constructor(x) { this.x = x; } value() { return this.x; } } class B extends A { doubled() { return this.x * 2; } } var b = new B(4); console.log(b.doubled(), b.value(), b instanceof B, b instanceof A);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "8 4 true true\n");
}

#[test]
fn supports_explicit_super_calls() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "class A { constructor(x) { this.x = x; } value() { return this.x; } } class B extends A { constructor(x, y) { super(x); this.y = y; } sum() { return this.value() + this.y; } } var b = new B(4, 5); console.log(b.sum(), b.x, b.y, b instanceof A);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "9 4 5 true\n");
}

#[test]
fn supports_super_method_calls() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "class A { constructor(x) { this.x = x; } value() { return this.x; } } class B extends A { constructor(x, y) { super(x); this.y = y; } total() { return super.value() + this.y; } } var b = new B(4, 5); console.log(b.total());",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "9\n");
}

#[test]
fn runs_synchronous_async_await_shim() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "function P(executor) { this.__resolved = false; executor((v) => { this.__value = v; this.__resolved = true; }, (e) => { throw e; }); } async function f() { var x = await new P((resolve) => resolve(4)); return x + 1; } var arrow = async (v) => (await v) * 2; console.log(f(), arrow(3));",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "5 6\n");
}

#[test]
fn runs_bitwise_operators_and_asi() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "function f() { return 7 }\nvar x = 0xD008;\nconsole.log(x, (5 & 3) | 8, 1 << 4, 16 >> 2, ~0, f());",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "53256 9 16 4 -1 7\n");
}

#[test]
fn hoists_function_scope_vars_before_nested_functions() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "function outer() { function inner() { return size; } var size = 7; return inner(); } console.log(outer());",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "7\n");
}

#[test]
fn runs_do_while_and_math_random() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "var i = 0; do { i++; } while (i < 3); var r = Math.random(); console.log(i, r >= 0 && r < 1);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "3 true\n");
}

#[test]
fn runs_bitwise_compound_assignment() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "var x = 0; x |= 5; x ^= 1; x &= 7; x <<= 2; x >>>= 1; console.log(x, (-1) >>> 0);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "8 4294967295\n");
}

#[test]
fn parses_nested_conditional_expressions() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "console.log(1 < 2 ? 3 : 4 < 5 ? 6 : 7, 1 > 2 ? 3 : 4 < 5 ? 6 : 7);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "3 6\n");
}

#[test]
fn grows_value_heap_for_many_numbers() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.set_step_limit(5_000_000);
    runtime
        .run(
            "var total = 0; for (var i = 0; i < 200000; i++) { total += i + 0.5; } console.log(total > 0);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "true\n");
}

#[test]
fn creates_implicit_global_in_sloppy_mode() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "function set() { implicitGlobal = 3; implicitGlobal++; } set(); console.log(implicitGlobal);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "4\n");
}

#[test]
fn builtin_methods_use_registered_ids() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "var values = []; values.push(1); values.push(2); console.log(values.join(','));",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "1,2\n");
    assert!(runtime.builtin_method_hits() >= 2);
}

#[test]
fn math_methods_are_installed() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "console.log(typeof Math.abs, typeof Math.max, Math.abs(-2), Math.max(1, 3));",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "function function 2 3\n");
}

#[test]
fn wrapper_prototypes_are_installed() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "console.log(typeof String.prototype.slice, typeof Number.prototype.toFixed, typeof Boolean.prototype.toString);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "function function function\n");
}

#[test]
fn array_flat_reduce_right_and_find_last_are_available() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "console.log([1, [2, [3]]].flat(2).join(','), [1, 2, 3].reduceRight(function (a, b) { return a - b; }), [1, 2, 3].findLast(function (value) { return value < 3; }));",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "1,2,3 0 2\n");
}

#[test]
fn global_parse_builtins_are_installed() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "console.log(parseInt('42'), parseFloat('3.5'), isNaN(NaN), isFinite(Infinity));",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "42 3.5 true false\n");
}

#[test]
fn hoists_catch_var_to_function_scope() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "try { throw 1; } catch (e) { var foo = 2; } console.log(foo); function f() { try { throw 1; } catch (e) { var bar = 3; } return bar; } console.log(f());",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "2\n3\n");
}

#[test]
fn global_assignment_leaves_no_operand_residue() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "var count = 0; function update() { implicitGlobal = 3; implicitGlobal++; } update(); count = implicitGlobal; console.log(count);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "4\n");
    let stats = runtime.safepoint_stats();
    assert!(stats.count > 0);
    assert_eq!(stats.max_operand_depth, 0);
}

#[test]
fn coerces_non_numbers_in_arithmetic_and_comparison() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "console.log(-undefined, undefined < 1, +'3', 1 + undefined);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "NaN false 3 NaN\n");
}

#[test]
fn converts_object_keys_with_to_string() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "var key = { toString: function () { return 'x'; } }; var target = {}; target[key] = 3; console.log(target.x);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "3\n");
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
fn prefix_member_updates_leave_no_operand_residue() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "var o = { x: 1 }; ++o.x; console.log(o.x, ++o.x); o.x++; console.log(o.x);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "2 3\n4\n");
}

#[test]
fn gc_observability_records_pause_and_last_stats() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::new(64 * 1024);
    runtime
        .run(
            "for (let i = 0; i < 2000; i = i + 1) { let o = { x: i }; }",
            &mut host,
        )
        .unwrap();
    let gc = runtime.gc_observability();
    assert!(gc.collections > 0);
    assert!(gc.pause_nanos > 0);
    let last = gc.last.expect("last gc stats");
    assert!(last.freed_objects > 0 || last.freed_arrays > 0);
}

#[test]
fn records_interpreter_safepoints() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "for (let i = 0; i < 2000; i = i + 1) { let o = { x: i }; }",
            &mut host,
        )
        .unwrap();
    let stats = runtime.safepoint_stats();
    assert!(stats.count > 0);
    let last = stats.last.as_ref().expect("last safepoint");
    assert!(last.local_count > 0);
    assert!(last.defined_locals > 0);
}

#[test]
fn handle_table_roots_survive_gc() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::new(64 * 1024);
    let value = runtime.allocate_object().expect("plain object");
    assert!(value.is_object());
    let handle = runtime.create_handle(value);
    runtime
        .run(
            "for (let i = 0; i < 2000; i = i + 1) { let o = { x: i }; }",
            &mut host,
        )
        .unwrap();
    assert!(runtime.object_live_bytes() > 0);
    assert!(runtime.handle_value(handle).is_some());
    assert!(runtime.drop_handle(handle).is_some());
    let stats = runtime.collect_garbage(&[]);
    assert_eq!(stats.live_bytes_after, 0);
    assert_eq!(runtime.handle_count(), 0);
}

#[test]
fn weak_handles_do_not_root_their_target() {
    let mut runtime = Runtime::new(64 * 1024);
    let value = runtime.allocate_object().expect("plain object");
    let weak = runtime.create_weak_handle(value);
    assert!(runtime.weak_value(weak).is_some());
    let stats = runtime.collect_garbage(&[]);
    assert_eq!(stats.live_bytes_after, 0);
    assert!(runtime.weak_value(weak).is_none());
    assert_eq!(runtime.weak_handle_count(), 0);
}

#[test]
fn weak_handles_are_enqueued_for_finalization() {
    let mut runtime = Runtime::new(64 * 1024);
    let value = runtime.allocate_object().expect("plain object");
    let weak = runtime.create_weak_handle(value);
    runtime.collect_garbage(&[]);
    let cleared = runtime.take_cleared_weak_handles();
    assert_eq!(cleared, vec![weak]);
    assert!(runtime.take_cleared_weak_handles().is_empty());
}

#[test]
fn weak_handle_follows_strong_handle_lifetime() {
    let mut runtime = Runtime::new(64 * 1024);
    let value = runtime.allocate_object().expect("plain object");
    let strong = runtime.create_handle(value);
    let weak = runtime.create_weak_handle(value);
    runtime.collect_garbage(&[]);
    assert!(runtime.weak_value(weak).is_some());
    runtime.drop_handle(strong);
    runtime.collect_garbage(&[]);
    assert!(runtime.weak_value(weak).is_none());
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

#[test]
fn stops_infinite_loops_at_the_step_limit() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.set_step_limit(100);
    let error = runtime.run("while (true) {}", &mut host).unwrap_err();
    assert!(error.to_string().contains("step limit"));
}

#[test]
fn provides_performance_now() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run("console.log(performance.now() > 0);", &mut host)
        .unwrap();
    assert_eq!(host.output(), "true\n");
}

#[test]
fn property_inline_cache_is_per_call_site() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.set_step_limit(5_000_000);
    runtime
        .run(
            "var first = { value: 1 }; var second = { pad: 0, value: 2 }; var total = 0; for (var i = 0; i < 100; i++) { total += first.value; total += second.value; } console.log(total);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "300\n");
    let (hits, misses) = runtime.property_ic_stats();
    assert!(misses >= 2, "each shape/site pair needs a fill");
    assert!(
        hits > 100,
        "per-site caches should not thrash between shapes"
    );
}

#[test]
fn property_ic_tracks_prototype_shape_guard() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "var proto = { value: 1 }; var object = Object.create(proto); var total = 0; for (var i = 0; i < 100; i++) { total += object.value; } proto.value = 5; total += object.value; proto.extra = 0; total += object.value; console.log(total, object.value);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "110 5\n");
    let (hits, misses) = runtime.property_ic_stats();
    assert!(hits > 90);
    assert!(misses >= 2);
}

#[test]
fn property_inline_cache_is_polymorphic() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "var objects = [{ a: 1 }, { b: 0, a: 2 }, { b: 1, a: 3 }, { b: 2, c: 0, a: 4 }]; var total = 0; for (var i = 0; i < 100; i++) { for (var j = 0; j < 4; j++) { total += objects[j].a; } } console.log(total);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "1000\n");
    let (hits, misses) = runtime.property_ic_stats();
    assert!(misses >= 4);
    assert!(hits > 300);
}

#[test]
fn property_inline_cache_falls_back_when_megamorphic() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime
        .run(
            "var objects = [{ a: 1 }, { b: 0, a: 2 }, { b: 1, a: 3 }, { b: 2, c: 0, a: 4 }, { b: 3, d: 0, a: 5 }]; var total = 0; for (var i = 0; i < 100; i++) { for (var j = 0; j < 5; j++) { total += objects[j].a; } } console.log(total);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "1500\n");
}

#[test]
fn property_inline_cache_hits_and_stays_correct() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.set_step_limit(5_000_000);
    runtime
        .run(
            "var object = { value: 1 }; var total = 0; for (var i = 0; i < 1000; i++) { total += object.value; object.value = total % 7; } console.log(total > 0, object.value);",
            &mut host,
        )
        .unwrap();
    assert!(host.output().starts_with("true "));
    let (hits, misses) = runtime.property_ic_stats();
    assert!(misses > 0, "expected at least one IC fill");
    assert!(hits > 0, "expected repeated property access to hit the IC");
}

#[test]
fn grows_value_heap_for_long_string_building() {
    let mut host = CaptureHost::default();
    let mut runtime = Runtime::default();
    runtime.set_step_limit(5_000_000);
    runtime
        .run(
            "var text = ''; for (var i = 0; i < 20000; i++) { text += 'x'; } console.log(text.length);",
            &mut host,
        )
        .unwrap();
    assert_eq!(host.output(), "20000\n");
}
