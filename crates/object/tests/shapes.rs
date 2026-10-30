use kagura_object::{
    ElementsKind, PropertyAttributes, PropertyKey, PropertyLocation, ShapeError, ShapeTable,
    own_property_keys,
};

#[test]
fn shapes_transition_and_reuse() {
    let mut table = ShapeTable::new();
    let root = table.root();
    let with_a = table
        .add_property(
            root,
            PropertyKey::from_string("a"),
            PropertyAttributes::data(),
        )
        .unwrap();
    let again = table
        .add_property(
            root,
            PropertyKey::from_string("a"),
            PropertyAttributes::data(),
        )
        .unwrap();
    assert_eq!(with_a, again);
    assert_eq!(table.len(), 2);
}

#[test]
fn descriptors_are_in_insertion_order_until_enumerated() {
    let mut table = ShapeTable::new();
    let mut shape = table.root();
    for key in ["b", "2", "a", "1"] {
        shape = table
            .add_property(
                shape,
                PropertyKey::from_string(key),
                PropertyAttributes::data(),
            )
            .unwrap();
    }

    let keys = own_property_keys(&table, shape).unwrap();
    let values: Vec<String> = keys
        .into_iter()
        .map(|key| match key {
            PropertyKey::Index(index) => index.to_string(),
            PropertyKey::String(value) => value,
            PropertyKey::Symbol(id) => format!("symbol:{id}"),
        })
        .collect();
    assert_eq!(values, ["1", "2", "b", "a"]);
}

#[test]
fn integer_keys_are_normalized() {
    assert_eq!(PropertyKey::from_string("0"), PropertyKey::Index(0));
    assert_eq!(
        PropertyKey::from_string("01"),
        PropertyKey::String("01".into())
    );
}

#[test]
fn in_object_slots_overflow_to_property_array() {
    let mut table = ShapeTable::new();
    let mut shape = table.root();
    for index in 0..=kagura_object::MAX_IN_OBJECT_SLOTS {
        shape = table
            .add_property(
                shape,
                PropertyKey::from_string(format!("p{index}")),
                PropertyAttributes::data(),
            )
            .unwrap();
    }
    let descriptor = table
        .lookup(shape, &PropertyKey::from_string("p8"))
        .unwrap()
        .unwrap();
    assert!(matches!(
        descriptor.location,
        PropertyLocation::PropertyArray(0)
    ));
    assert_eq!(table.get(shape).unwrap().elements_kind, ElementsKind::Smi);
}

#[test]
fn unknown_shape_is_rejected() {
    let table = ShapeTable::new();
    assert!(matches!(
        table.get(kagura_object::ShapeId::new(99)),
        Err(ShapeError::UnknownShape)
    ));
}
