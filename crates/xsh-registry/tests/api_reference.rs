use xsh_registry::reference::language_references;

#[test]
fn map_reference_preserves_scalar_key_domains() {
    let references = language_references();
    let literals = references.iter().find(|item| item.id == "core.map-literals").expect("map literals");
    assert!(literals.docs.contract.contains("Map[K, V]"));
    for domain in ["Str", "Int", "UInt", "Bool", "Bytes", "Path", "Duration"] {
        assert!(literals.docs.contract.contains(domain), "missing {domain}");
    }
    assert!(!literals.docs.contract.contains("Computed keys require Str"));
    let iteration = references.iter().find(|item| item.id == "core.comprehensions").expect("comprehensions");
    assert!(iteration.docs.contract.contains("{key: K, value: V}"));
    assert!(iteration.docs.contract.contains("Unicode scalar order"));
}

#[test]
fn causes_reference_documents_typed_translation_and_example() {
    let references = language_references();
    let causes = references.iter().find(|item| item.id == "core.causes").expect("typed causes API reference");
    for spelling in ["cause:", "nominal", "immutable", "once"] {
        assert!(causes.docs.contract.contains(spelling), "missing {spelling}");
    }
    let example = causes.docs.example.as_deref().expect("typed cause source example");
    assert!(example.contains("cause: failure"));
    assert!(example.contains("BuildCauseError"));
}
