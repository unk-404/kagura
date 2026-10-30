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
