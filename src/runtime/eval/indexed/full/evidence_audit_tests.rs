use super::*;
use super::super::generic::{ForwardedRequirement, ForwardingId};
use crate::sema::check::Checker;

fn run_with_large_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(f)
        .expect("spawn prepared evidence test")
        .join()
        .unwrap_or_else(|payload| std::panic::resume_unwind(payload));
}

// These tests mutate executable proofs after ordinary source preparation. Native
// source tests cannot express foreign handles or inconsistent encoded operands.
fn fixture(name: &str, source: &str) -> FullProgram {
    fixture_with_solved(name, source, |_, _| {})
}

fn fixture_with_solved(name: &str, source: &str, inspect: impl FnOnce(&ArenaProgram, &crate::sema::check::SolvedTypes)) -> FullProgram {
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        name,
        crate::loader::entry_source_from_text(name, source.to_string()),
        Vec::new(),
    );
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
    parsed.arena.symbol_owner().with_current(|| inspect(&parsed.arena, &declarations.solved));
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let program = FullBuilder::build_compact(
        &parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id,
    ).unwrap();
    FullVerifier::verify(&program).unwrap();
    program
}

fn function(program: &FullProgram, name: &str) -> IrFunctionId {
    IrFunctionId::new(program.store.functions.iter().position(|function| {
        program.store.string(function.name).unwrap() == name
    }).unwrap()).unwrap()
}

fn assert_rejected_evidence(program: &FullProgram, reason: &str) {
    let _symbols = program.symbol_owner().enter();
    assert!(FullVerifier::verify_generic_evidence(&program.store).is_err(), "{reason}");
    assert!(FullVerifier::verify(program).is_err(), "{reason}");
}

fn unused_forwarding(program: &FullProgram) -> ForwardingId {
    let unused = function(program, "unused");
    program.store.generic.as_deref().unwrap().calls().iter().find_map(|call| {
        if call.caller != InstructionOwner::Function(unused) { return None; }
        match call.evidence { CallEvidence::Forwarded(plan) => Some(plan), _ => None }
    }).expect("unused declaration has a symbolic forwarding plan")
}

fn unused_forwarding_fixture() -> (FullProgram, ForwardingId) {
    let program = fixture_with_solved("unused-operation-forwarding.xsh", r#"
pure add(left, right) { left + right }
pure unused(left, right, extra, first, second, third) {
    let pending = add(first, second)
    let other = third + second
    let inner: Int = add(left, right)
    inner + extra
}
let sum: Int = add(2, 3)
"#, |program, solved| {
        let (owner, declaration) = solved.declarations.iter().find(|(identity, _)|
            program.arena.function_def(identity.declaration).name == "unused").unwrap();
        let scheme = solved.graph.scheme(declaration.scheme).unwrap();
        assert_eq!(scheme.requirement_origins.len(), 3, "forwarded and body operations retain independent pending Add guards");
        assert_eq!(scheme.requirement_origins.iter().copied().collect::<std::collections::BTreeSet<_>>().len(), 3);
        assert!(scheme.requirements.iter().all(|requirement| matches!(requirement, crate::sema::inference::RequirementTemplate::Add { .. })));
        assert!(scheme.requirement_origins.iter().all(|requirement| solved.graph.discharge(*requirement).unwrap().is_none()));
        let fixed = solved.additions.iter().filter(|(expression, _)| solved.expression_owners.get(expression) == Some(owner))
            .filter_map(|(_, requirement)| solved.graph.discharge(*requirement).unwrap()).collect::<Vec<_>>();
        assert_eq!(fixed.len(), 1, "the final Add keeps its source-owned fixed discharge");
        assert_eq!(fixed[0].operation, crate::sema::inference::SealedOperation::AddInt);
        assert!(!scheme.requirement_origins.contains(&fixed[0].requirement));
        for ty in [fixed[0].left, fixed[0].right, fixed[0].result] {
            assert_eq!(solved.graph.export_type(ty).unwrap(), Type::Int);
        }
    });
    let evidence = program.store.generic.as_deref().unwrap();
    let forwarding = unused_forwarding(&program);
    let plan = evidence.forwarding(forwarding).unwrap();
    assert_eq!(evidence.scope(plan.caller).unwrap().requirements.len(), 3);
    assert_ne!(evidence.scope(plan.caller).unwrap().requirements[0], evidence.scope(plan.caller).unwrap().requirements[1]);
    assert!(plan.instances.is_empty());
    assert_eq!(plan.requirements.len(), 1);
    assert!(matches!(plan.requirements[0], ForwardedRequirement::Caller(_)));
    (program, forwarding)
}

#[test]
fn unused_forwarding_rejects_an_in_bounds_but_different_caller_requirement() {
    run_with_large_stack(|| {
        let (mut program, forwarding) = unused_forwarding_fixture();
        let symbols = program.symbol_owner().clone();
        let _symbols = symbols.enter();
        let evidence = program.store.generic.as_deref_mut().unwrap();
        let plan = evidence.forwarding(forwarding).unwrap();
        let original = plan.requirements[0];
        let ForwardedRequirement::Caller(original_index) = original else { unreachable!() };
        let wrong = evidence.scope(plan.caller).unwrap().requirements.iter().enumerate()
            .find(|(index, requirement)| *index != original_index as usize &&
                matches!(requirement, Requirement::Add { .. })).unwrap().0 as u32;
        assert_ne!(original, ForwardedRequirement::Caller(wrong));
        evidence.test_forwarding_mut(forwarding).unwrap().requirements[0] =
            ForwardedRequirement::Caller(wrong);
        assert_rejected_evidence(&program,
            "one Add obligation cannot certify a different unused forwarded Add");
    });
}

#[test]
fn unused_forwarding_rejects_a_fixed_projection_with_a_foreign_layout() {
    run_with_large_stack(|| {
        let mut program = fixture("unused-fixed-projection.xsh", r#"
pure name(entry) { entry.name }
pure unused(ignored) { name({name: "fixed"}) }
"#);
        let forwarding = unused_forwarding(&program);
        let evidence = program.store.generic.as_deref_mut().unwrap();
        let plan = evidence.forwarding(forwarding).unwrap();
        assert!(plan.instances.is_empty());
        assert_eq!(plan.requirements.len(), 1);
        let ForwardedRequirement::Fixed(RequirementWitness::Projection {
            layout, field_slot, result,
        }) = plan.requirements[0] else {
            panic!("fixed record forwarding must already have a projection witness");
        };
        let mut foreign = GenericEvidenceBuilder::default();
        let foreign_layout = foreign.add_layout(evidence.layout(layout).unwrap().clone()).unwrap();
        assert_ne!(layout, foreign_layout);
        evidence.test_forwarding_mut(forwarding).unwrap().requirements[0] =
            ForwardedRequirement::Fixed(RequirementWitness::Projection {
                layout: foreign_layout, field_slot, result,
            });
        assert_rejected_evidence(&program,
            "an unused forwarding plan still requires a proof owned by its program");
    });
}

#[test]
fn identity_return_plan_cannot_wrap_or_discard_its_declared_value() {
    run_with_large_stack(|| {
        let program = fixture("identity-return-plan.xsh", r#"
pure identity(value) { value }
let value: Int = identity(7)
"#);
        let identity = function(&program, "identity");
        let evidence = program.store.generic.as_deref().unwrap();
        let scope = evidence.scope_for_function(identity).unwrap();
        assert!(matches!(evidence.scope(scope).unwrap().result, TypeRef::Rigid(_)));
        assert_eq!(evidence.scope(scope).unwrap().return_plan, GenericReturnPlan::Value);
        assert!(evidence.instances().any(|(_, instance)| instance.scope == scope &&
            program.store.semantic.to_type(instance.result_type).unwrap() == Type::Int));
        for wrong in [GenericReturnPlan::Result, GenericReturnPlan::Unit, GenericReturnPlan::ResultUnit] {
            let mut mutated = program.clone();
            mutated.store.generic.as_deref_mut().unwrap().test_scope_mut(scope).unwrap()
                .return_plan = wrong;
            assert_rejected_evidence(&mutated,
                &format!("plain identity result was certified with {wrong:?} return behavior"));
        }
    });
}

#[test]
fn written_result_return_keeps_its_declaration_return_plan() {
    run_with_large_stack(|| {
        let mut program = fixture("written-result-return-plan.xsh", r#"
pure fixed(marker) -> Result[Int] { let _ = marker; Ok(7) }
let _ = fixed(1)
"#);
        let fixed = function(&program, "fixed");
        let evidence = program.store.generic.as_deref().unwrap();
        let scope = evidence.scope_for_function(fixed).unwrap();
        let definition = evidence.scope(scope).unwrap();
        assert_eq!(definition.return_plan, GenericReturnPlan::Value);
        let TypeRef::Ground(result) = definition.result else {
            panic!("written closed Result must retain its ground type");
        };
        assert!(matches!(program.store.semantic.to_type(result).unwrap(),
            Type::Result(ok, _) if *ok == Type::Int));
        program.store.generic.as_deref_mut().unwrap().test_scope_mut(scope).unwrap()
            .return_plan = GenericReturnPlan::Result;
        assert_rejected_evidence(&program,
            "a written Result value cannot acquire implicit wrapping behavior");
    });
}

#[test]
fn add_witness_rejects_an_incompatible_operand_from_the_same_function() {
    run_with_large_stack(|| {
        let mut program = fixture("add-operand-proof.xsh", r#"
pure add(left, right) {
    let _ = false
    left + right
}
let sum: Int = add(2, 3)
"#);
        let add = function(&program, "add");
        let mut range = program.store.function_instruction_range(add.index()).unwrap();
        let boolean = range.clone().find(|&instruction|
            program.store.tags[instruction] == FullTag::ExprBool)
            .expect("same function retains the discarded Bool expression");
        let binary = range.find(|&instruction|
            program.store.tags[instruction] == FullTag::ExprBinary).unwrap();
        let evidence = program.store.generic.as_deref().unwrap();
        let use_ = evidence.requirement_use(binary as u32).unwrap();
        assert!(matches!(evidence.scope(use_.scope).unwrap().requirements[use_.requirement as usize],
            Requirement::Add { .. }));
        assert!(evidence.instances().any(|(_, instance)| instance.scope == use_.scope));
        let start = program.store.data[binary].lhs as usize;
        assert_ne!(program.store.extra[start + 1], boolean as u32);
        program.store.extra[start + 1] = boolean as u32;
        assert_rejected_evidence(&program,
            "an Int Add witness cannot authorize a same-owner Bool operand");
    });
}
