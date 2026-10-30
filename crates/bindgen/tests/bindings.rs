use kagura_bindgen::{Bindings, CallExt, HostCall};
use kagura_value::Heap;

#[test]
fn binds_and_invokes_a_typed_host_function() {
    let mut bindings = Bindings::new();
    bindings.bind("math.add", |call: &mut HostCall<'_>| {
        let left = call.arg_number(0)?;
        let right = call.arg_number(1)?;
        Ok(call.heap_mut().number(left + right))
    });

    let mut heap = Heap::new(1024);
    let args = [heap.number(2.0), heap.number(3.0)];
    let mut call = HostCall::new(&mut heap, &args);
    let id = bindings.resolve("math.add").unwrap();
    let result = bindings.invoke(id, &mut call).unwrap();

    assert_eq!(heap.as_number(result), Some(5.0));
}
