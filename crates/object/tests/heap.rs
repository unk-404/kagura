use kagura_object::{ObjectHeap, ObjectHeapError, PropertyAttributes, PropertyKey, ShapeTable};
use kagura_value::Value;

#[test]
fn allocates_objects_in_kmem() {
    let mut shapes = ShapeTable::new();
    let root = shapes.root();
    let shape = shapes
        .add_property(
            root,
            PropertyKey::from_string("value"),
            PropertyAttributes::data(),
        )
        .unwrap();
    let mut heap = ObjectHeap::new(4096);
    let object = heap.allocate(shape).unwrap();

    assert_eq!(heap.shape(object).unwrap(), shape);
    assert_eq!(heap.get_slot(object, 0).unwrap(), Value::undefined());
    heap.set_slot(object, 0, Value::smi(42)).unwrap();
    assert_eq!(heap.get_slot(object, 0).unwrap(), Value::smi(42));
    assert_eq!(ObjectHeap::value(object), Value::object(object.index()));
}

#[test]
fn grows_property_array_without_realloc() {
    let shapes = ShapeTable::new();
    let mut heap = ObjectHeap::new(4096);
    let object = heap.allocate(shapes.root()).unwrap();
    for index in 0..12 {
        heap.set_property_array(object, index, Value::smi(index as i32))
            .unwrap();
    }
    for index in 0..12 {
        assert_eq!(
            heap.get_property_array(object, index).unwrap(),
            Value::smi(index as i32)
        );
    }
    assert_eq!(
        heap.get_property_array(object, 12).unwrap(),
        Value::undefined()
    );
}

#[test]
fn grows_elements_and_preserves_holes() {
    let shapes = ShapeTable::new();
    let mut heap = ObjectHeap::new(4096);
    let object = heap.allocate_array(shapes.root()).unwrap();
    heap.array_push(object, Value::smi(1)).unwrap();
    heap.array_push(object, Value::smi(2)).unwrap();
    assert_eq!(heap.array_length(object).unwrap(), 2);
    heap.array_set(object, 4, Value::smi(5)).unwrap();
    assert_eq!(heap.array_length(object).unwrap(), 5);
    assert_eq!(heap.array_get(object, 0).unwrap(), Value::smi(1));
    assert_eq!(heap.array_get(object, 2).unwrap(), Value::undefined());
    assert_eq!(heap.array_get(object, 4).unwrap(), Value::smi(5));
}

#[test]
fn rejects_slot_outside_record() {
    let shapes = ShapeTable::new();
    let mut heap = ObjectHeap::new(4096);
    let object = heap.allocate(shapes.root()).unwrap();
    assert!(matches!(
        heap.get_slot(object, kagura_object::MAX_IN_OBJECT_SLOTS),
        Err(ObjectHeapError::InvalidSlot)
    ));
}

#[test]
fn reports_exhaustion() {
    let shapes = ShapeTable::new();
    let mut heap = ObjectHeap::new(16);
    assert!(matches!(
        heap.allocate(shapes.root()),
        Err(ObjectHeapError::OutOfMemory)
    ));
}
