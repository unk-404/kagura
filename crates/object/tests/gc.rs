use kagura_object::{ObjectHeap, ShapeTable};
use kagura_value::Value;

fn root_object(heap: &mut ObjectHeap, shapes: &ShapeTable) -> Value {
    let object = heap.allocate(shapes.root()).unwrap();
    ObjectHeap::value(object)
}

#[test]
fn collects_unreachable_object_graph() {
    let shapes = ShapeTable::new();
    let mut heap = ObjectHeap::new(4096);
    let parent = heap.allocate(shapes.root()).unwrap();
    let child = heap.allocate(shapes.root()).unwrap();
    heap.set_slot(parent, 0, ObjectHeap::value(child)).unwrap();

    let stats = heap.collect(&[ObjectHeap::value(parent)]);
    assert_eq!(stats.freed_objects, 0);
    assert_eq!(stats.freed_arrays, 0);
    assert_eq!(stats.live_bytes_after, 96);

    let stats = heap.collect(&[]);
    assert_eq!(stats.freed_objects, 2);
    assert_eq!(stats.live_bytes_after, 0);
}

#[test]
fn traces_property_and_element_arrays() {
    let shapes = ShapeTable::new();
    let mut heap = ObjectHeap::new(4096);
    let parent = heap.allocate(shapes.root()).unwrap();
    let child = heap.allocate(shapes.root()).unwrap();
    let array = heap.allocate_array(shapes.root()).unwrap();

    heap.set_slot(parent, 0, ObjectHeap::value(array)).unwrap();
    heap.set_property_array(parent, 0, ObjectHeap::value(child))
        .unwrap();
    heap.array_push(array, ObjectHeap::value(child)).unwrap();

    let stats = heap.collect(&[ObjectHeap::value(parent)]);
    assert_eq!(stats.freed_objects, 0);
    assert_eq!(stats.freed_arrays, 0);

    let stats = heap.collect(&[]);
    assert_eq!(stats.freed_objects, 3);
    assert!(stats.freed_arrays >= 2);
    assert_eq!(stats.live_bytes_after, 0);
}

#[test]
fn reuses_freed_object_records() {
    let shapes = ShapeTable::new();
    let mut heap = ObjectHeap::new(4096);
    let first = heap.allocate(shapes.root()).unwrap();
    heap.collect(&[]);
    let second = heap.allocate(shapes.root()).unwrap();
    assert_eq!(first, second);
    assert_eq!(heap.live_bytes(), 48);
}

#[test]
fn keeps_returned_roots_alive() {
    let shapes = ShapeTable::new();
    let mut heap = ObjectHeap::new(4096);
    let root = root_object(&mut heap, &shapes);
    let stats = heap.collect(&[root]);
    assert_eq!(stats.freed_objects, 0);
    assert_eq!(stats.live_bytes_after, 48);
}

#[test]
fn stresses_one_million_allocations_with_reclamation() {
    let shapes = ShapeTable::new();
    let mut heap = ObjectHeap::new(1 << 20);
    for index in 0..1_000_000 {
        let _object = heap.allocate(shapes.root()).unwrap();
        if index % 1024 == 0 {
            heap.collect(&[]);
        }
    }
    let stats = heap.collect(&[]);
    assert_eq!(stats.live_bytes_after, 0);
    assert!(heap.used() <= heap.capacity());
}
