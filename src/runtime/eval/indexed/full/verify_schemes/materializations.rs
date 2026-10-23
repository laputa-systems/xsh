use super::*;
use super::body::BodyTypeRef;
use super::super::value_materialization::FullMaterializationKind as K;

pub(super) fn verify(
    store: &FullStore,
    scopes: &[Option<SchemeScope>],
    signatures: &Arc<super::super::FunctionSignatureIndex>,
    type_coverage: &mut [bool],
    row_coverage: &mut [bool],
    field_coverage: &mut [bool],
) -> Result<(), IrVerifyError> {
    let mut budget = SchemeInstantiationBudget::new(store);
    budget.charge(store.value_materializations.len().saturating_add(store.materialization_nodes.len()).saturating_add(store.materialization_edges.len()).saturating_add(store.materialization_leaves.len()))?;
    let mut headers = vec![false; store.value_materializations.len()];
    let mut nodes = vec![false; store.materialization_nodes.len()];
    let mut edges = vec![false; store.materialization_edges.len()];
    let mut leaves = vec![false; store.materialization_leaves.len()];
    for instruction in 0..store.tags.len() {
        budget.charge(1)?;
        if store.tags[instruction] != FullTag::ExprCheckedValue { continue; }
        let decoder = decoder_for_instruction(store, instruction, signatures)?;
        let schema::ExprView::CheckedValue { check: ValueCheckView::Materialization(id), .. } = decoder.expression_view(instruction)? else { continue; };
        let header = store.value_materializations.get(id as usize).ok_or_else(invalid)?;
        let covered = headers.get_mut(id as usize).ok_or_else(invalid)?;
        if std::mem::replace(covered, true) || header.instruction as usize != instruction { return Err(invalid()); }
        let caller = caller_scheme_for_instruction(store, instruction, scopes)?;
        let source = endpoint(store, header.source, caller, &mut budget)?;
        let destination = endpoint(store, header.destination, caller, &mut budget)?;
        let schema::ExprView::CheckedValue { value, .. } = decoder.expression_view(instruction)? else { return Err(invalid()); };
        let actual = |instruction: u32, budget: &mut SchemeInstantiationBudget| -> Result<SchemeType, IrVerifyError> {
            if let Some((scheme, scope)) = caller {
                let fact = scheme_body_fact_type(store, scheme, instruction, FullSchemeBodyFactKind::Expression)?.ok_or_else(invalid)?;
                decode_scheme_type(store, scope, fact, budget, 0)
            } else {
                let id = checked_expression_type_id(store, instruction).ok_or_else(invalid)?;
                store.semantic.to_type_with_budget(id, &mut budget.semantic_types).map(SchemeType::Ground)
            }
        };
        if !body_types_equal(&actual(value.raw(), &mut budget)?, &source)
            || !body_types_equal(&actual(instruction as u32, &mut budget)?, &destination)
        { return Err(invalid()); }
        let mut proof = PlanProof { store, scopes, caller, nodes: &mut nodes, edges: &mut edges, leaves: &mut leaves, type_coverage, row_coverage, field_coverage, budget: &mut budget };
        if proof.node(header.root, BodyTypeRef::Scheme(&source), BodyTypeRef::Scheme(&destination), 0)? == 0 { return Err(invalid()); }
    }
    if headers.iter().chain(&nodes).chain(&edges).chain(&leaves).any(|covered| !covered) { return Err(invalid()); }
    Ok(())
}

pub(super) fn endpoints(
    store: &FullStore,
    id: u32,
    instruction: usize,
    scope: Option<&SchemeScope>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<(SchemeType, SchemeType), IrVerifyError> {
    let header = store.value_materializations.get(id as usize).ok_or_else(invalid)?;
    if header.instruction as usize != instruction { return Err(invalid()); }
    let ty = |id, budget: &mut SchemeInstantiationBudget| match scope {
        Some(scope) => decode_scheme_type(store, scope, id, budget, 0),
        None => instantiate_bound_type(store, id, &CallSubstitution::default(), budget, 0).map(SchemeType::Ground),
    };
    Ok((ty(header.source, budget)?, ty(header.destination, budget)?))
}

fn endpoint(
    store: &FullStore,
    id: SchemeTypeId,
    caller: Option<(FullSchemeId, &SchemeScope)>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<SchemeType, IrVerifyError> {
    match caller {
        Some((_, scope)) => decode_scheme_type(store, scope, id, budget, 0),
        None => instantiate_bound_type(store, id, &CallSubstitution::default(), budget, 0).map(SchemeType::Ground),
    }
}

struct PlanProof<'a, 'b> {
    store: &'a FullStore,
    scopes: &'a [Option<SchemeScope>],
    caller: Option<(FullSchemeId, &'a SchemeScope)>,
    nodes: &'b mut [bool],
    edges: &'b mut [bool],
    leaves: &'b mut [bool],
    type_coverage: &'b mut [bool],
    row_coverage: &'b mut [bool],
    field_coverage: &'b mut [bool],
    budget: &'b mut SchemeInstantiationBudget,
}

impl PlanProof<'_, '_> {
    fn node(&mut self, id: u32, source: BodyTypeRef<'_>, destination: BodyTypeRef<'_>, depth: usize) -> Result<usize, IrVerifyError> {
        if depth > 128 { return Err(invalid()); }
        self.budget.charge(1)?;
        if std::mem::replace(self.nodes.get_mut(id as usize).ok_or_else(invalid)?, true) { return Err(invalid()); }
        let node = *self.store.materialization_nodes.get(id as usize).ok_or_else(invalid)?;
        if node.reserved != [0; 3] { return Err(invalid()); }
        let (mut source, mut destination) = (source, destination);
        loop {
            self.budget.charge(1)?;
            match (source.validated_base(), destination.validated_base()) {
                (Some((left, source_base)), Some((right, destination_base))) if left == right => {
                    source = source_base;
                    destination = destination_base;
                }
                (None, None) => break,
                _ => return Err(invalid()),
            }
        }
        let children = mark_range(node.children, &self.store.materialization_edges, self.edges, "materialization child ranges overlap or are invalid")?;
        self.budget.charge(children.len())?;
        if node.kind != K::Callable && node.leaf != IR_NONE { return Err(invalid()); }
        match node.kind {
            K::Keep => {
                if !children.is_empty() { return Err(invalid()); }
                let source = source.materialize(self.budget)?;
                let destination = destination.materialize(self.budget)?;
                self.budget.charge_scheme_clone(&source)?;
                self.budget.charge_scheme_clone(&destination)?;
                if !body_types_equal(&source, &destination) { return Err(invalid()); }
                Ok(0)
            }
            K::Callable => {
                if !children.is_empty() { return Err(invalid()); }
                if std::mem::replace(self.leaves.get_mut(node.leaf as usize).ok_or_else(invalid)?, true) { return Err(invalid()); }
                let leaf = *self.store.materialization_leaves.get(node.leaf as usize).ok_or_else(invalid)?;
                let relationship = leaf.relationship;
                let source = source.relationship().ok_or_else(invalid)?;
                if self.store.semantic.scheme_callable_index(source)? != relationship { return Err(invalid()); }
                let relationship = self.store.scheme_callable_values.get(relationship as usize).ok_or_else(invalid)?;
                let target = self.store.functions.get(relationship.canonical.index()).ok_or_else(invalid)?;
                let FullCallableContract::Scheme(id) = target.contract else { return Err(invalid()); };
                let scope = self.scopes.get(id.index()).and_then(Option::as_ref).ok_or_else(invalid)?;
                let scheme = self.store.scheme(id)?;
                let claimed = destination.materialize(self.budget)?;
                self.leaf_instance(target, scheme, scope, &leaf.instance, &claimed)?;
                Ok(1)
            }
            K::Record => {
                let source_shape = source.record_shape().ok_or_else(invalid)?;
                if Some(source_shape) != destination.record_shape() || source_shape.0 != children.len() { return Err(invalid()); }
                let mut seen = BTreeSet::new();
                let mut count = 0;
                for child in children {
                    self.budget.charge(1 + 3 * children.len().max(1).ilog2() as usize)?;
                    let name = Name::intern(self.store.string(child.name)?);
                    if !seen.insert(name) { return Err(invalid()); }
                    count += self.node(child.node, source.field(name).ok_or_else(invalid)?, destination.field(name).ok_or_else(invalid)?, depth + 1)?;
                }
                Ok(count)
            }
            K::List | K::MapValues | K::Optional => {
                let [child] = children else { return Err(invalid()); };
                if child.name != IR_NONE { return Err(invalid()); }
                let (source, destination) = match node.kind {
                    K::List => (source.list_item().ok_or_else(invalid)?, destination.list_item().ok_or_else(invalid)?),
                    K::MapValues => {
                        let (source_key, source) = source.map_parts().ok_or_else(invalid)?;
                        let (destination_key, destination) = destination.map_parts().ok_or_else(invalid)?;
                        let source_key = source_key.materialize(self.budget)?;
                        let destination_key = destination_key.materialize(self.budget)?;
                        if source_key != destination_key { return Err(invalid()); }
                        (source, destination)
                    }
                    _ => (source.optional_payload().ok_or_else(invalid)?, destination.optional_payload().ok_or_else(invalid)?),
                };
                self.node(child.node, source, destination, depth + 1)
            }
            K::Result => {
                let [ok, error] = children else { return Err(invalid()); };
                if ok.name != IR_NONE || error.name != IR_NONE { return Err(invalid()); }
                let (source_ok, source_error) = source.result_parts().ok_or_else(invalid)?;
                let (destination_ok, destination_error) = destination.result_parts().ok_or_else(invalid)?;
                Ok(self.node(ok.node, source_ok, destination_ok, depth + 1)? + self.node(error.node, source_error, destination_error, depth + 1)?)
            }
        }
    }

    fn leaf_instance(
        &mut self,
        target: &FullFunction,
        scheme: &FullScheme,
        scope: &SchemeScope,
        instance: &super::super::schemes::FullInstantiation,
        claimed: &SchemeType,
    ) -> Result<(), IrVerifyError> {
        let store = self.store;
        let types = mark_range(instance.type_arguments, &store.call_type_arguments, self.type_coverage, "materialization type arguments overlap or are invalid")?;
        let rows = mark_range(instance.row_arguments, &store.call_row_arguments, self.row_coverage, "materialization row arguments overlap or are invalid")?;
        let effects_rows = bounded(instance.effect_arguments, &store.call_effect_arguments, "materialization effect arguments are invalid")?;
        if instance.application_bindings.len != scheme.applications.len || instance.operation_bindings.len != scheme.operations.len
            || instance.method_bindings.len != scheme.methods.len || instance.wrapper_bindings.len != scheme.wrappers.len
        { return Err(invalid()); }
        if let Some((caller_id, caller_scope)) = self.caller {
            let substitution = build_symbolic_substitution(store, scope, caller_scope, types, rows, effects_rows, self.field_coverage)?;
            let caller = Some((store.scheme(caller_id)?, caller_scope));
            effects::verify_predicates(store, scheme, scope, &substitution.effects, caller, self.budget)?;
            self.leaf_bindings(scheme, scope, applications::ApplicationSubstitution::Symbolic(&substitution), caller, instance)?;
            references::verify(store, target, scheme, scope, applications::ApplicationSubstitution::Symbolic(&substitution), claimed, self.budget)
        } else {
            let substitution = build_substitution(store, scope, types, rows, effects_rows, self.field_coverage)?;
            effects::verify_predicates(store, scheme, scope, &substitution.effects, None, self.budget)?;
            self.leaf_bindings(scheme, scope, applications::ApplicationSubstitution::Ground(&substitution), None, instance)?;
            references::verify(store, target, scheme, scope, applications::ApplicationSubstitution::Ground(&substitution), claimed, self.budget)
        }
    }

    fn leaf_bindings(
        &mut self,
        scheme: &FullScheme,
        scope: &SchemeScope,
        substitution: applications::ApplicationSubstitution<'_>,
        caller: Option<(&FullScheme, &SchemeScope)>,
        instance: &super::super::schemes::FullInstantiation,
    ) -> Result<(), IrVerifyError> {
        let store = self.store;
        applications::verify_bindings(store, scheme, scope, substitution, caller, bounded(instance.application_bindings, &store.call_application_bindings, "materialization application bindings are invalid")?, self.budget)?;
        operations::verify_bindings(store, scheme, scope, substitution, caller, bounded(instance.operation_bindings, &store.call_operation_bindings, "materialization operation bindings are invalid")?, self.budget)?;
        methods::verify_bindings(store, scheme, scope, substitution, caller, bounded(instance.method_bindings, &store.call_method_bindings, "materialization method bindings are invalid")?, self.budget)?;
        wrappers::verify_bindings(store, scheme, scope, substitution, caller, bounded(instance.wrapper_bindings, &store.call_wrapper_bindings, "materialization wrapper bindings are invalid")?, self.budget)
    }
}

fn invalid() -> IrVerifyError { IrVerifyError::new("value materialization has incomplete or inconsistent owned evidence") }

pub(super) fn owned_instances(
    store: &FullStore,
    budget: &mut SchemeInstantiationBudget,
) -> Result<Vec<(u32, super::super::schemes::FullInstantiation)>, IrVerifyError> {
    budget.charge(store.materialization_nodes.len().saturating_add(store.materialization_edges.len()).saturating_add(store.materialization_leaves.len()))?;
    let mut nodes = vec![false; store.materialization_nodes.len()];
    let mut edges = vec![false; store.materialization_edges.len()];
    let mut leaves = vec![false; store.materialization_leaves.len()];
    let mut instances = Vec::with_capacity(store.materialization_leaves.len());
    for plan in &store.value_materializations {
        budget.charge(1)?;
        let mut pending = vec![(plan.root, 0)];
        while let Some((id, depth)) = pending.pop() {
            if depth > 128 { return Err(invalid()); }
            budget.charge(1)?;
            if std::mem::replace(nodes.get_mut(id as usize).ok_or_else(invalid)?, true) { return Err(invalid()); }
            let node = store.materialization_nodes.get(id as usize).ok_or_else(invalid)?;
            let children = mark_range(node.children, &store.materialization_edges, &mut edges, "materialization owned children overlap or are invalid")?;
            budget.charge(children.len())?;
            pending.extend(children.iter().map(|child| (child.node, depth + 1)));
            if node.kind == K::Callable {
                if std::mem::replace(leaves.get_mut(node.leaf as usize).ok_or_else(invalid)?, true) { return Err(invalid()); }
                instances.push((plan.instruction, store.materialization_leaves.get(node.leaf as usize).ok_or_else(invalid)?.instance));
            }
        }
    }
    if nodes.iter().chain(&edges).chain(&leaves).any(|covered| !covered) { return Err(invalid()); }
    Ok(instances)
}
