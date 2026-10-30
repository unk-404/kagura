use kagura_value::Heap;

#[test]
fn stores_smis_and_numbers() {
    let mut heap = Heap::new(1024);
    let smi = heap.number(42.0);
    assert_eq!(heap.as_number(smi), Some(42.0));
    let number = heap.number(1.5);
    assert_eq!(heap.as_number(number), Some(1.5));
}

#[test]
fn formats_strings_and_specials() {
    let mut heap = Heap::new(1024);
    let string = heap.string("kagura");
    assert_eq!(heap.display(string).to_string(), "\"kagura\"");
    assert_eq!(
        heap.to_js_string(kagura_value::Value::undefined()),
        "undefined"
    );
}
