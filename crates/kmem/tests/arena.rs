use kagura_kmem::Arena;

#[test]
fn allocates_and_reads_values() {
    let mut arena = Arena::new(1024);
    let string = arena.alloc_str("kagura").unwrap();
    assert_eq!(arena.read_str(string), Some("kagura"));

    let number = arena.alloc_f64(1.5).unwrap();
    assert_eq!(arena.read_f64(number), Some(1.5));
}

#[test]
fn refuses_allocation_beyond_capacity() {
    let mut arena = Arena::new(16);
    assert!(arena.alloc(8, 8).is_some());
    assert!(arena.alloc(16, 8).is_none());
}
