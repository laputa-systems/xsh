use std::path::Path;

#[test]
fn build_script_uses_registry_instead_of_source_scraping() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let build = std::fs::read_to_string(root.join("build.rs")).expect("read build.rs");

    for forbidden in [
        "SEMANTIC_SOURCES",
        "SymbolSeedVisitor",
        "syn::parse_file",
        "parse_file(",
        "src/modules/signature/modules.rs",
        "src/modules/signature/methods.rs",
        "src/modules/signature/runtime_op.rs",
        "src/modules/signature/builders.rs",
        "src/modules/signature/streams.rs",
    ] {
        assert!(
            !build.contains(forbidden),
            "build.rs must not source-scrape `{forbidden}`"
        );
    }
}

#[test]
fn legacy_signature_registry_files_are_removed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for path in [
        "src/modules/signature/modules.rs",
        "src/modules/signature/methods.rs",
        "src/modules/signature/runtime_op.rs",
        "src/modules/signature/builders.rs",
        "src/modules/signature/streams.rs",
    ] {
        assert!(
            !root.join(path).exists(),
            "`{path}` should live in crates/xsh-registry, not src/modules"
        );
    }
}
