use super::*;
use crate::runtime::eval::indexed::semantic::{RawSemanticPools, SemanticPools, TypeTag};
use crate::runtime::eval::indexed::{IrData, IrRange};

fn apply_semantic_candidate(program: &FullProgram, raw: RawSemanticPools) -> FullProgram {
    let mut candidate = program.clone();
    (candidate.store.semantic, candidate.symbols) = SemanticPools::from_raw(raw);
    candidate
}

#[test]
fn candidate_semantic_tables_preserve_rows_and_reach_full_verifier() {
    let source = "pure project(value) { value.amount }\npure use_project() -> Int { project({amount: 7, enabled: true}) }\nassert use_project() == 7\n";
    let (_, _, program) = build_fixture(source).unwrap();
    let raw = program.store.semantic.export_raw(&program.symbols);
    let candidate = apply_semantic_candidate(&program, raw.clone());
    assert_eq!(candidate.store.semantic.export_raw(&candidate.symbols).tables, raw.tables);
    FullVerifier::verify(&candidate).unwrap();

    let mut missing_type_data = raw.clone();
    missing_type_data.tables.type_data.pop().unwrap();
    assert!(FullVerifier::verify(&apply_semantic_candidate(&program, missing_type_data)).is_err());

    let mut invalid_shape = raw.clone();
    invalid_shape.tables.shapes.push(IrRange { start: u32::MAX, len: 1 });
    assert!(FullVerifier::verify(&apply_semantic_candidate(&program, invalid_shape)).is_err());

    let mut invalid_signature = raw.clone();
    invalid_signature.tables.signature_data.push(IrData { lhs: u32::MAX, rhs: 3 });
    assert!(FullVerifier::verify(&apply_semantic_candidate(&program, invalid_signature)).is_err());

    let mut invalid_type = raw.clone();
    invalid_type.tables.type_tags.push(TypeTag::List);
    invalid_type.tables.type_data.push(IrData { lhs: IR_NONE, rhs: 0 });
    assert!(FullVerifier::verify(&apply_semantic_candidate(&program, invalid_type)).is_err());

    let mut duplicate_shape = raw.clone();
    duplicate_shape.tables.shapes.extend([IrRange { start: 0, len: 0 }; 2]);
    let candidate = apply_semantic_candidate(&program, duplicate_shape.clone());
    assert_eq!(candidate.store.semantic.export_raw(&candidate.symbols).tables, duplicate_shape.tables);

    let mut foreign_owner = raw;
    foreign_owner.tables.scheme_callable_owner = crate::sema::scheme_callable::SchemeCallableOwner::fresh();
    let candidate = apply_semantic_candidate(&program, foreign_owner.clone());
    assert_eq!(candidate.store.semantic.export_raw(&candidate.symbols).tables, foreign_owner.tables);
}
