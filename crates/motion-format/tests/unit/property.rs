//! Unit tests for `src/property.rs`.

use super::*;

#[test]
fn lookup_returns_index_and_spec() {
    let (i, spec) = property("opacity").unwrap();
    assert_eq!(PROPERTIES[i].name, "opacity");
    assert_eq!(spec.default, 1.0);
    assert_eq!(spec.range, Some((0.0, 1.0)));
    assert!(property("zoom").is_none());
}

#[test]
fn registry_names_are_unique_and_documented() {
    for (i, p) in PROPERTIES.iter().enumerate() {
        assert_eq!(property(p.name).unwrap().0, i);
        assert!(!p.doc.is_empty(), "{}", p.name);
    }
}

#[test]
fn names_list_follows_registry_order() {
    assert_eq!(property_names(), "x, y, opacity, scale, rotate");
}
