use super::*;
use crate::sema::inference::{Atom, Arrow, CallableKind, EffectSet, EffectSummary, Generalization, InferenceContext, Parameter, RowField, TypeNode};
use crate::source::{SourceId, Span};
use crate::symbol::{Name, SymbolOwner};

fn span() -> Span { Span::new(SourceId::new(0), 0, 1) }

fn view(graph: &SolvedGraph, root: ScopedRoot, kind: LoweredType) -> CheckedStorageView {
    let before = graph.counters().clone();
    let view = checked_storage_view(graph, Some(root)).unwrap();
    assert_eq!(view.kind, kind);
    assert_eq!(view.root.ty, root.ty, "physical storage cannot replace the original semantic endpoint");
    assert_eq!(view.root.scope, root.scope);
    assert_eq!(graph.counters(), &before, "storage projection never solves, instantiates, or allocates graph nodes");
    view
}

#[test]
fn checked_ground_storage_keeps_exact_unsigned_and_nominal_roots() {
    let symbols = SymbolOwner::new();
    let _guard = symbols.enter();
    let mut graph = InferenceContext::default();
    let nominal = Name::intern("SpecificFailure");
    let variant = Name::intern("invalid");
    let tag = Name::intern("SpecificTag");
    let cases = [
        (Atom::Int, LoweredType::Int), (Atom::UInt, LoweredType::Int),
        (Atom::Bool, LoweredType::Bool), (Atom::Float, LoweredType::Float),
        (Atom::Str, LoweredType::Str), (Atom::Bytes, LoweredType::Bytes),
        (Atom::Duration, LoweredType::Duration), (Atom::Unit, LoweredType::Unit),
        (Atom::Path, LoweredType::Path), (Atom::Command, LoweredType::Command),
        (Atom::Digest, LoweredType::Digest), (Atom::Regex, LoweredType::Regex),
        (Atom::Status, LoweredType::Status), (Atom::FsRoot, LoweredType::FsRoot),
        (Atom::ProcessHandle, LoweredType::ProcessHandle), (Atom::NetJob, LoweredType::NetJob),
        (Atom::Any, LoweredType::Any), (Atom::Null, LoweredType::Any),
        (Atom::ErasedRecord, LoweredType::Record), (Atom::DynamicModule, LoweredType::Module),
        (Atom::Pure, LoweredType::Pure), (Atom::Proc, LoweredType::Proc),
        (Atom::Tag(tag), LoweredType::Tag), (Atom::Error, LoweredType::Error),
        (Atom::ProcessError, LoweredType::Error), (Atom::ErrorFamily(nominal), LoweredType::Error),
        (Atom::ErrorVariant { family: nominal, variant }, LoweredType::Error),
        (Atom::ErrorFacet(nominal), LoweredType::Error),
    ].into_iter().map(|(atom, kind)| (graph.atom(atom).unwrap(), atom, kind)).collect::<Vec<_>>();
    let alias = graph.fresh(0, span()).unwrap();
    let uint = cases.iter().find(|(_, atom, _)| *atom == Atom::UInt).unwrap().0;
    let reason = graph.reason(span(), None).unwrap();
    graph.unify(alias, uint, reason).unwrap();
    let mut roots = cases.iter().map(|(ty, _, _)| ScopedRoot { ty: *ty, scope: None }).collect::<Vec<_>>();
    roots.push(ScopedRoot { ty: alias, scope: None });
    let graph = graph.freeze_scoped(&roots).unwrap();
    for (ty, atom, kind) in cases {
        let projected = view(&graph, ScopedRoot { ty, scope: None }, kind);
        assert_eq!(graph.node(projected.root.ty).unwrap(), &TypeNode::Atom(atom));
        assert_eq!(projected.visited_nodes, 1);
    }
    let projected = view(&graph, ScopedRoot { ty: alias, scope: None }, LoweredType::Int);
    assert_ne!(projected.root.ty, graph.resolved(projected.root.ty).unwrap());
    assert!(matches!(graph.node(projected.root.ty).unwrap(), TypeNode::Meta(_)));
    assert_eq!(graph.resolved(projected.root.ty).unwrap(), uint);
}

#[test]
fn checked_nested_storage_preserves_container_shells_and_authorized_rigid_scopes() {
    let symbols = SymbolOwner::new();
    let _guard = symbols.enter();
    let mut graph = InferenceContext::default();
    let parameter = graph.fresh(1, span()).unwrap();
    let scheme = graph.generalize(parameter, 0, Generalization::Allowed, &[]).unwrap();
    let rigid = graph.scheme(scheme).unwrap().body;
    let int = graph.atom(Atom::UInt).unwrap();
    let string = graph.atom(Atom::Str).unwrap();
    let error = graph.atom(Atom::ErrorFamily(Name::intern("ItemFailure"))).unwrap();
    let list = graph.list(rigid).unwrap();
    let map = graph.map(string, list).unwrap();
    let result = graph.result(map, error).unwrap();
    let optional = graph.optional(result).unwrap();
    let stream = graph.stream(optional).unwrap();
    let row = graph.row(vec![RowField { label: Name::intern("items"), ty: stream }, RowField { label: Name::intern("count"), ty: int }], None).unwrap();
    let record = graph.record(row).unwrap();
    let other = graph.generalize(int, 0, Generalization::Allowed, &[]).unwrap();
    let cases = [(rigid, LoweredType::Generic), (list, LoweredType::List), (map, LoweredType::Map),
        (result, LoweredType::Result), (optional, LoweredType::Any), (stream, LoweredType::Stream), (record, LoweredType::Record)];
    let tail = graph.fresh_row(1, span()).unwrap();
    let open_row = graph.row(vec![RowField { label: Name::intern("count"), ty: int }], Some(tail)).unwrap();
    let open_record = graph.record(open_row).unwrap();
    let row_scheme = graph.generalize(open_record, 0, Generalization::Allowed, &[]).unwrap();
    let open_record = graph.scheme(row_scheme).unwrap().body;
    let TypeNode::Record(row) = graph.node(open_record).unwrap() else { panic!("generalized row remains a record") };
    let row_tail = graph.row_data(*row).unwrap().tail.unwrap();
    let mut roots = cases.iter().map(|(ty, _)| ScopedRoot { ty: *ty, scope: Some(scheme) }).collect::<Vec<_>>();
    roots.push(ScopedRoot { ty: open_record, scope: Some(row_scheme) });
    let graph = graph.freeze_scoped(&roots).unwrap();
    for (ty, kind) in cases { view(&graph, ScopedRoot { ty, scope: Some(scheme) }, kind); }
    view(&graph, ScopedRoot { ty: open_record, scope: Some(row_scheme) }, LoweredType::Record);
    let before = graph.counters().clone();
    assert_eq!(checked_storage_view(&graph, Some(ScopedRoot { ty: rigid, scope: None })).unwrap_err(), StorageViewError::Graph(InferenceError::ScopeEscape));
    assert_eq!(checked_storage_view(&graph, Some(ScopedRoot { ty: record, scope: Some(other) })).unwrap_err(), StorageViewError::Graph(InferenceError::ScopeEscape));
    assert_eq!(checked_storage_view(&graph, Some(ScopedRoot { ty: open_record, scope: None })).unwrap_err(), StorageViewError::Graph(InferenceError::ScopeEscape));
    assert_eq!(checked_storage_view(&graph, Some(ScopedRoot { ty: row_tail, scope: Some(row_scheme) })).unwrap_err(), StorageViewError::UnsupportedStorage(row_tail));
    assert_eq!(graph.counters(), &before);
}

#[test]
fn checked_storage_refuses_missing_foreign_unresolved_and_recovery_endpoints() {
    let mut graph = InferenceContext::default();
    let int = graph.atom(Atom::Int).unwrap();
    let unresolved = graph.fresh(0, span()).unwrap();
    let poison = graph.poison().unwrap();
    let noncompletion = graph.non_completion().unwrap();
    let env = graph.atom(Atom::EnvPathList).unwrap();
    let unresolved_list = graph.list(unresolved).unwrap();
    let poisoned_list = graph.list(poison).unwrap();
    let graph = graph.freeze(&[int]).unwrap();
    let mut foreign = InferenceContext::default();
    let foreign_int = foreign.atom(Atom::Int).unwrap();
    let before = graph.counters().clone();
    assert_eq!(checked_storage_view(&graph, None).unwrap_err(), StorageViewError::MissingSourceType);
    for (ty, expected) in [
        (foreign_int, StorageViewError::Graph(InferenceError::ForeignHandle)),
        (unresolved, StorageViewError::Graph(InferenceError::Unresolved(unresolved))),
        (poison, StorageViewError::Graph(InferenceError::Recovery(poison))),
        (noncompletion, StorageViewError::Graph(InferenceError::Recovery(noncompletion))),
        (env, StorageViewError::UnsupportedStorage(env)),
        (unresolved_list, StorageViewError::Graph(InferenceError::Unresolved(unresolved))),
        (poisoned_list, StorageViewError::Graph(InferenceError::Recovery(poison))),
    ] { assert_eq!(checked_storage_view(&graph, Some(ScopedRoot { ty, scope: None })).unwrap_err(), expected); }
    assert_eq!(graph.counters(), &before);
    let mut shallow = InferenceContext::new(crate::sema::inference::Limits { structural_depth: 4, ..Default::default() });
    let int = shallow.atom(Atom::Int).unwrap();
    let mut deep = int;
    for _ in 0..6 { deep = shallow.list(deep).unwrap(); }
    let shallow = shallow.freeze(&[int]).unwrap();
    let before = shallow.counters().clone();
    assert_eq!(checked_storage_view(&shallow, Some(ScopedRoot { ty: deep, scope: None })).unwrap_err(), StorageViewError::Graph(InferenceError::Limit("structural depth")));
    assert_eq!(shallow.counters(), &before);
}

#[test]
fn checked_callable_storage_covers_all_choice_arms_without_selecting_a_signature() {
    let symbols = SymbolOwner::new();
    let _guard = symbols.enter();
    let mut graph = InferenceContext::default();
    let reason = graph.reason(span(), None).unwrap();
    let int = graph.atom(Atom::Int).unwrap();
    let string = graph.atom(Atom::Str).unwrap();
    let mut arrows = Vec::new();
    for (kind, ty, label) in [(CallableKind::Pure, int, "first"), (CallableKind::Pure, string, "second"), (CallableKind::Proc, int, "third"), (CallableKind::Stream, int, "fourth")] {
        let result = if kind == CallableKind::Stream { graph.stream(ty).unwrap() } else { ty };
        arrows.push(graph.arrow(Arrow { kind, params: vec![Parameter { label: Name::intern(label), ty, defaulted: false, rest: false }], result, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap());
    }
    let compatible = graph.join_callable_values(arrows[0], arrows[1], 0, reason).unwrap();
    let mixed = graph.join_callable_values(arrows[0], arrows[2], 0, reason).unwrap();
    let reversed = graph.join_callable_values(arrows[2], arrows[0], 0, reason).unwrap();
    let roots = arrows.iter().chain([&compatible, &mixed, &reversed]).map(|ty| ScopedRoot { ty: *ty, scope: None }).collect::<Vec<_>>();
    let graph = graph.freeze_scoped(&roots).unwrap();
    for (ty, kind) in arrows.into_iter().zip([LoweredType::Pure, LoweredType::Pure, LoweredType::Proc, LoweredType::Pure]) { view(&graph, ScopedRoot { ty, scope: None }, kind); }
    let compatible_view = view(&graph, ScopedRoot { ty: compatible, scope: None }, LoweredType::Pure);
    assert_eq!(compatible_view.visited_nodes, 4, "one wrapper, one retained choice, and both distinct arrows");
    for ty in [mixed, reversed] { assert!(matches!(checked_storage_view(&graph, Some(ScopedRoot { ty, scope: None })), Err(StorageViewError::UnsupportedStorage(_)))); }
}

#[test]
fn original_checked_source_endpoints_project_after_arena_disposal_without_new_checking() {
    crate::runtime::eval::run_eval(|| {
        use crate::sema::check::Checker;
        use crate::syntax::parser::Parser;
        let source = "enum Label { Named(Int) }\nerror ItemFailure = invalid(message: Str)\npure identity(value) { value }\npure tagged(count: UInt) -> Result[List[Label], ItemFailure] { Ok([Named(count)]) }\nstream rows(value: Int) [] -> Stream[Int] { yield value }\nlet count: UInt = 7\nlet first = identity([7])\nlet second = identity(\"word\")\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let expressions = checked.solved.expressions.iter().filter_map(|(identity, ty)| {
            let text = &source[parsed.arena.arena.expr(identity.expression).span.range()];
            let kind = match text { "identity([7])" => LoweredType::List, "identity(\"word\")" => LoweredType::Str, "Ok([Named(count)])" => LoweredType::Result, _ => return None };
            Some((*identity, *ty, kind))
        }).collect::<Vec<_>>();
        assert_eq!(expressions.len(), 3);
        drop(parsed);
        let before = checked.solved.graph.counters().clone();
        let checking = Checker::module_reuse_counters();
        for (identity, ty, kind) in expressions {
            let scope = checked.solved.expression_scope(identity, checked.solved.expression_owners.get(&identity).copied()).unwrap();
            view(&checked.solved.graph, ScopedRoot { ty, scope }, kind);
        }
        for declaration in checked.solved.declarations.values() { view(&checked.solved.graph, ScopedRoot { ty: declaration.signature, scope: Some(declaration.scheme) }, LoweredType::Pure); }
        for binding in checked.solved.bindings.values() {
            let lexical = binding.owner.map(|owner| checked.solved.declarations[&owner].scheme);
            let root = ScopedRoot { ty: binding.ty, scope: binding.scheme.or(lexical) };
            checked_storage_view(&checked.solved.graph, Some(root)).unwrap();
        }
        assert_eq!(checked.solved.graph.counters(), &before);
        assert_eq!(Checker::module_reuse_counters(), checking);
    });
}
