use super::*;
use crate::modules::signature::{ApiArgCheck, ImplBinding, SemanticRule};
use crate::sema::check::SolvedOperationAuthority;
use crate::sema::registry_graph::RegistryOwner;

/// The algorithm is a checked boundary policy, separate from the two authored
/// values. Its implementation operand must not become a public default value.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildHashPolicyPacket {
    pub origin: ExpressionIdentity,
    pub caller: Option<crate::sema::check::DeclarationIdentity>,
    pub algorithm: Name,
    pub implementation: crate::sema::check::DeclarationIdentity,
    pub implementation_key: QualifiedName,
    pub public_operands: [BuildExprId; 2],
    pub algorithm_operand: BuildExprId,
}

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn append_checked_hash_policy_arguments(&mut self, id: ExprId, arguments: &mut Vec<LoweredCallArg>) -> Option<Option<BuildHashPolicyPacket>> {
        let origin = self.expression_identity(id);
        let solved = self.solved();
        let operation = solved.operations.get(&origin)?;
        let selected = solved.graph.candidate_evidence(operation.requirement).ok()??;
        let SolvedOperationAuthority::Registry(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate).ok()? else { return None; };
        if metadata.operation != RuntimeOp::HashVerifyFile { return Some(None); }
        let ImplBinding::Script(script) = metadata.binding else { return None; };
        if metadata.owner != RegistryOwner::Module("hash") || metadata.entry != "verify_file"
            || metadata.kind != crate::sema::inference::CallableKind::Proc
            || script.module != "hash" || script.function != "verify_file"
            || metadata.argument_check != ApiArgCheck::HashVerifyFile || metadata.semantic_rule != SemanticRule::Standard
            || operation.receiver.is_some() || !operation.argument_coercions.is_empty()
            || operation.binding.supplied_slots != [0, 1] || !operation.binding.default_slots.is_empty()
            || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some()
            || operation.actual_arguments.len() != 2 || arguments.len() != 2 { return None; }
        let boundary = solved.registry_boundaries.get(&origin)?;
        let algorithm = boundary.hash_algorithm()?;
        if boundary.requirement != Some(operation.requirement) || boundary.caller != operation.caller
            || solved.expression_owners.get(&origin).copied() != operation.caller
            || !matches!(algorithm.as_str().as_str(), "md5" | "sha1" | "sha256" | "sha512") { return None; }
        let recipes = solved.argument_sources.get(&origin)?;
        if recipes.len() != 2 || recipes[1].name != Some(algorithm)
            || recipes[0].name.is_some_and(|name| name != "path")
            || recipes.iter().any(|recipe| !matches!(recipe.value, crate::sema::arguments::ArgumentValueSource::Expression(_))) { return None; }
        let [LoweredCallArg::Single(path), LoweredCallArg::Single(checksum)] = arguments.as_slice() else { return None; };
        let public_operands = [*path, *checksum];
        let caller = operation.caller;
        let namespace = self.internal_namespace(script.module)?;
        let implementation_key = QualifiedName::new(namespace, Name::intern(script.function));
        let index = self.function_index();
        let definition = index.definition(LoweredFunctionKey::Qualified(implementation_key))?;
        let module = self.program.modules.iter().find(|module| module.name == namespace)?;
        let (catalog, source) = module.canonical_stdlib_source()?;
        if catalog.identity != "hash" || source != definition.definition_span.source_id { return None; }
        let implementation = crate::sema::check::DeclarationIdentity { source, namespace: Some(namespace), declaration: definition.id };
        self.solved().declarations.get(&implementation)?;
        let algorithm_operand = push_build_row!(self, expr, BuildExprRow::Str(Arc::from(algorithm.as_str().as_str())));
        arguments.push(LoweredCallArg::Single(algorithm_operand));
        Some(Some(BuildHashPolicyPacket { origin, caller, algorithm, implementation, implementation_key, public_operands, algorithm_operand }))
    }

    pub(super) fn record_hash_policy_packet(&mut self, expression: BuildExprId, packet: BuildHashPolicyPacket) {
        self.scratch.borrow_mut().hash_policy_packets.insert(expression, packet);
    }
}
