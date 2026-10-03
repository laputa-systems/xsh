use super::*;
use crate::sema::arguments::ArgumentValueSource;
use crate::sema::check::SolvedOperationAuthority;
use crate::sema::inference::{OperationBinding, RequirementTemplate, ScopedRequirementRoot, ScopedRoot, TypeNode};
use crate::sema::registry_graph::RegistryOwner;

// Formal destinations belong to the original selected operation. Authored
// argument recipes remain in source order; absent defaults retain empty slots.
fn original_native_module_call_args(solved: &SolvedTypes, origin: ExpressionIdentity) -> Option<LoweredModuleCallArgs> {
    let operation = solved.operations.get(&origin)?;
    let graph = &solved.graph;
    let selected = graph.candidate_evidence(operation.requirement).ok()??;
    let SolvedOperationAuthority::Registry(metadata) = solved.operation_catalog.candidate(graph, selected.candidate).ok()? else { return None; };
    if metadata.binding != crate::modules::signature::ImplBinding::Native
        || !matches!(metadata.owner, RegistryOwner::Module(_)) || operation.receiver.is_some()
        || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some()
        || !operation.argument_coercions.is_empty() { return None; }
    if solved.expression_owners.get(&origin).copied() != operation.caller { return None; }
    let scope = solved.expression_scope(origin, operation.caller).ok()?;
    graph.validate_requirement_scoped(ScopedRequirementRoot { requirement: operation.requirement, scope }).ok()?;
    graph.validate_scoped(ScopedRoot { ty: *solved.expressions.get(&origin)?, scope }).ok()?;
    graph.validate_scoped(ScopedRoot { ty: operation.result, scope }).ok()?;
    graph.validate_scoped(ScopedRoot { ty: selected.result, scope }).ok()?;
    graph.validate_scoped(ScopedRoot { ty: selected.signature, scope }).ok()?;
    // The requirement certifies the result relationship. Independently
    // allocated container roots retain their own identities after unification.
    if let Some(boundary) = solved.registry_boundaries.get(&origin) {
        if boundary.requirement != Some(operation.requirement) || boundary.caller != operation.caller
            || boundary.input != operation.result
            || solved.expressions.get(&origin) != Some(&boundary.result) { return None; }
    }
    let RequirementTemplate::Operation { call, .. } = graph.requirement_template(operation.requirement).ok()? else { return None; };
    let call = graph.operation_call(call).ok()?;
    if call.binding != OperationBinding::Slots || call.receiver.is_some() || call.result != operation.result { return None; }
    let TypeNode::Arrow(arrow) = graph.node(graph.resolved(selected.signature).ok()?).ok()? else { return None; };
    let sources = solved.argument_sources.get(&origin)?;
    if arrow.kind != metadata.kind || arrow.params.len() != metadata.parameters.len() || call.arguments.len() != arrow.params.len()
        || selected.actual_arguments.len() != arrow.params.len()
        || sources.len() != operation.binding.supplied_slots.len() || sources.len() != operation.actual_arguments.len()
        || arrow.params.iter().zip(&metadata.parameters).any(|(parameter, original)|
            parameter.rest || parameter.label != original.label || parameter.defaulted != original.defaulted) { return None; }
    let mut arguments = vec![None; arrow.params.len()];
    for (ordinal, ((source, &slot), &actual)) in sources.iter().zip(&operation.binding.supplied_slots).zip(&operation.actual_arguments).enumerate() {
        let parameter = arrow.params.get(slot)?;
        let ArgumentValueSource::Expression(expression) = source.value else { return None; };
        if source.entry_index != ordinal || source.span.source_id != origin.source
            || source.name.is_some_and(|name| name != parameter.label) || arguments.get(slot)?.is_some() { return None; }
        let source_origin = ExpressionIdentity { expression, ..origin };
        let source_scope = solved.expression_scope(source_origin, operation.caller).ok()?;
        let checked = *solved.expressions.get(&source_origin)?;
        graph.validate_scoped(ScopedRoot { ty: checked, scope: source_scope }).ok()?;
        if solved.expression_owners.get(&source_origin).copied() != operation.caller
            || graph.resolved(call.arguments.get(slot).copied().flatten()?).ok()? != graph.resolved(actual).ok()? { return None; }
        arguments[slot] = Some(expression);
    }
    let mut defaults = std::collections::BTreeSet::new();
    for &slot in &operation.binding.default_slots {
        if !defaults.insert(slot) || !arrow.params.get(slot)?.defaulted || arguments.get(slot)?.is_some()
            || call.arguments.get(slot)?.is_some() { return None; }
    }
    if arguments.iter().enumerate().any(|(slot, argument)| argument.is_none() != defaults.contains(&slot)) { return None; }
    while arguments.last().is_some_and(Option::is_none) { arguments.pop(); }
    Some(LoweredModuleCallArgs { semantic_rule: metadata.semantic_rule, op: metadata.operation, args: arguments })
}

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn checked_native_module_call_args(&self, id: ExprId) -> Option<LoweredModuleCallArgs> {
        original_native_module_call_args(self.solved(), self.expression_identity(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::Checker;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    #[test]
    fn original_native_module_arguments_keep_named_order_interior_omissions_and_selected_overloads() {
        crate::runtime::eval::run_eval(|| {
            let source = "proc selected() [fs] { archive.compress(dest: p\"out.gz\", source: p\"in\", overwrite: true) }\npure bytes_digest() { hash.sha256(b\"bytes\") }\nproc path_digest() [fs] { hash.sha256(p\"file\") }\n";
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let mut count = 0;
            for (&origin, operation) in &checked.solved.operations {
                let Some(packet) = original_native_module_call_args(&checked.solved, origin) else { continue; };
                let selected = checked.solved.graph.candidate_evidence(operation.requirement).unwrap().unwrap();
                let SolvedOperationAuthority::Registry(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, selected.candidate).unwrap() else { panic!(); };
                assert_eq!(packet.op, metadata.operation);
                let recipes = &checked.solved.argument_sources[&origin];
                for (recipe, &slot) in recipes.iter().zip(&operation.binding.supplied_slots) {
                    let ArgumentValueSource::Expression(expression) = recipe.value else { panic!(); };
                    assert_eq!(packet.args[slot], Some(expression));
                }
                if packet.op == RuntimeOp::ArchiveCompress {
                    assert_ne!(checked.solved.graph.resolved(checked.solved.expressions[&origin]).unwrap(), checked.solved.graph.resolved(operation.result).unwrap(),
                        "the checked Result expression and original call retain independently allocated roots");
                    assert_eq!(operation.binding.supplied_slots, [1, 0, 4]);
                    assert_eq!(packet.args.len(), 5);
                    assert_eq!(packet.args[2..4], [None, None]);
                }
                count += 1;
            }
            assert_eq!(count, 3);
            drop(parsed);
            checked.solved.validate().unwrap();
        });
    }

    #[test]
    fn original_native_module_arguments_refuse_missing_or_rebound_original_evidence() {
        crate::runtime::eval::run_eval(|| {
            let source = "proc selected() [fs] { archive.compress(dest: p\"out.gz\", source: p\"in\") }\n";
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let mut checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let origin = *checked.solved.operations.keys().find(|&&origin| original_native_module_call_args(&checked.solved, origin).is_some()).unwrap();
            let solved = Arc::get_mut(&mut checked.solved).unwrap();
            let original = solved.operations.remove(&origin).unwrap();
            assert!(original_native_module_call_args(solved, origin).is_none());
            solved.operations.insert(origin, original);
            let slots = solved.operations[&origin].binding.supplied_slots.clone();
            solved.operations.get_mut(&origin).unwrap().binding.supplied_slots[1] = slots[0];
            assert!(original_native_module_call_args(solved, origin).is_none());
            solved.operations.get_mut(&origin).unwrap().binding.supplied_slots = slots;
            let source = solved.argument_sources[&origin][0].clone();
            solved.argument_sources.get_mut(&origin).unwrap()[0].name = Some(Name::intern("source"));
            assert!(original_native_module_call_args(solved, origin).is_none());
            solved.argument_sources.get_mut(&origin).unwrap()[0] = source;
            let ArgumentValueSource::Expression(expression) = solved.argument_sources[&origin][0].value else { panic!(); };
            let source_origin = ExpressionIdentity { expression, ..origin };
            let root = solved.expressions.remove(&source_origin).unwrap();
            assert!(original_native_module_call_args(solved, origin).is_none());
            solved.expressions.insert(source_origin, root);
            let caller = solved.expression_owners.remove(&source_origin).unwrap();
            assert!(original_native_module_call_args(solved, origin).is_none());
            solved.expression_owners.insert(source_origin, caller);
            let result = solved.operations[&origin].result;
            solved.operations.get_mut(&origin).unwrap().result = root;
            assert!(original_native_module_call_args(solved, origin).is_none());
            solved.operations.get_mut(&origin).unwrap().result = result;
            let defaults = solved.operations[&origin].binding.default_slots.clone();
            solved.operations.get_mut(&origin).unwrap().binding.default_slots.push(defaults[0]);
            assert!(original_native_module_call_args(solved, origin).is_none());
            solved.operations.get_mut(&origin).unwrap().binding.default_slots = defaults;
            assert!(original_native_module_call_args(solved, origin).is_some());
        });
    }
}
