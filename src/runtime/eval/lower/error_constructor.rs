use super::*;
use crate::sema::check::{ConstructorAuthority, ConstructorValueSource, ExpressionIdentity, NominalDeclaration, NominalMemberKind, QualifiedNominalIdentity, SolvedArgumentSource, SolvedConstructorApplication};
use crate::sema::inference::{ConstraintRelation, ScopedRequirementRoot, ScopedRoot};

/// The original application owns its slots; the declaration owns the canonical family and facets.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalErrorConstructorPlan {
    pub origin: ExpressionIdentity,
    pub checked: ScopedRoot,
    pub checked_type: Type,
    pub authority: QualifiedNominalIdentity,
    pub application: SolvedConstructorApplication,
    pub family: Name,
    pub member: Name,
    pub parameters: Box<[(Name, Type)]>,
    pub facets: Box<[Name]>,
    pub recipes: Box<[SolvedArgumentSource]>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalErrorConstructorSource {
    pub plan: OriginalErrorConstructorPlan,
    pub fields: Box<[BuildExprId]>,
}

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn original_error_constructor_plan(&self, id: ExprId) -> Option<OriginalErrorConstructorPlan> {
        use crate::sema::arguments::ArgumentValueSource;
        let origin = self.expression_identity(id);
        let solved = self.solved();
        let application = solved.constructor_applications.get(&origin)?.clone();
        let ConstructorAuthority::Nominal(authority @ QualifiedNominalIdentity::Source { declaration: NominalDeclaration::Error(_), member: Some(_), .. }) = &application.authority else { return None; };
        let authority = *authority;
        if solved.expression_owners.get(&origin).copied() != application.caller || !application.default_slots.is_empty()
            || application.parameters.iter().any(|parameter| parameter.default.is_some()) { return None; }
        let checked = ScopedRoot { ty: application.result, scope: solved.expression_scope(origin, application.caller).ok()? };
        solved.graph.validate_scoped(checked).ok()?;
        if solved.graph.resolved(*solved.expressions.get(&origin)?).ok()? != solved.graph.resolved(application.result).ok()? { return None; }
        let requirement = application.requirement?;
        solved.graph.validate_requirement_scoped(ScopedRequirementRoot { requirement, scope: checked.scope }).ok()?;
        let selected = solved.graph.candidate_evidence(requirement).ok()??;
        let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate).ok()? else { return None; };
        let crate::sema::operation_graph::PreparedLanguageOperation::Declaration { authority: selected_authority, identity } = metadata.operation else { return None; };
        if selected_authority != "language.constructor.error_variant" || metadata.authority != selected_authority
            || &*identity.as_str() != format!("{authority:?}") { return None; }
        let member = solved.checked_nominal_member(authority).ok()?;
        if member.kind != NominalMemberKind::Error || member.scope.is_some() || member.fields.len() != application.parameters.len() { return None; }
        let checked_type = super::super::indexed::generic::graph_ground_type(&solved.graph, application.result).ok()?;
        if !matches!(checked_type, Type::ErrorVariant { variant, .. } if variant == member.member)
            || solved.nominals.get(&solved.graph.resolved(application.result).ok()?) != Some(&authority) { return None; }
        let declared = solved.constructor_nominals.get(&authority)?;
        if declared.len() != application.parameters.len() { return None; }
        let mut parameters = Vec::with_capacity(application.parameters.len());
        for ((label, declared), parameter) in declared.iter().zip(&application.parameters) {
            let name = parameter.label?;
            if *label != Some(name) || solved.graph.resolved(*declared).ok()? != solved.graph.resolved(parameter.ty).ok()? { return None; }
            solved.graph.validate_scoped(ScopedRoot { ty: parameter.ty, scope: checked.scope }).ok()?;
            let ty = super::super::indexed::generic::graph_ground_type(&solved.graph, parameter.ty).ok()?;
            let member_type = member.fields.iter().find_map(|&(label, ty)| (label == Some(name)).then_some(ty))?;
            if super::super::indexed::generic::graph_ground_type(&solved.graph, member_type).ok()? != ty { return None; }
            parameters.push((name, ty));
        }
        let recipes = solved.argument_sources.get(&origin)?.clone();
        if recipes.len() != application.supplied.len() { return None; }
        let mut occupied = std::collections::BTreeSet::new();
        for (recipe, argument) in recipes.iter().zip(&application.supplied) {
            let parameter = application.parameters.get(argument.slot)?;
            if !occupied.insert(argument.slot) || recipe.name.is_some_and(|name| parameter.label != Some(name)) { return None; }
            let source = match (recipe.value, argument.value) {
                (ArgumentValueSource::Expression(expression), ConstructorValueSource::Expression(source)) if source == (ExpressionIdentity { expression, ..origin }) => source,
                _ => return None,
            };
            let original = *solved.expressions.get(&source)?;
            solved.graph.validate_scoped(ScopedRoot { ty: original, scope: solved.expression_scope(source, application.caller).ok()? }).ok()?;
            let resolve = |ty| solved.graph.resolved(ty).ok();
            match (argument.value, argument.projection) {
                (ConstructorValueSource::Expression(_), None) if resolve(original) == resolve(argument.actual) => {},
                (ConstructorValueSource::RecordField { field, .. }, Some(index)) => {
                    let ConstraintRelation::Projection { record, label, result } = solved.graph.constraint_origins().get(index)?.relation else { return None; };
                    if label != field || resolve(record) != resolve(original) || resolve(result) != resolve(argument.actual) { return None; }
                }
                _ => return None,
            }
            let ConstraintRelation::Assignable { expected, actual } = solved.graph.constraint_origins().get(argument.assignability)?.relation else { return None; };
            if resolve(expected) != resolve(parameter.ty) || resolve(actual) != resolve(argument.actual) { return None; }
            super::super::indexed::generic::graph_ground_type(&solved.graph, argument.actual).ok()?;
        }
        if occupied.len() != parameters.len() { return None; }
        Some(OriginalErrorConstructorPlan { origin, checked, checked_type, authority, application, family: member.family, member: member.member,
            parameters: parameters.into_boxed_slice(), facets: member.facets.clone().into_boxed_slice(), recipes: recipes.into_boxed_slice() })
    }

    pub(super) fn lower_original_error_constructor_call(&mut self, id: ExprId, slots: &mut SlotScope, current_function: Option<Name>, item_slot: Option<usize>) -> Option<BuildExprId> {
        let plan = self.original_error_constructor_plan(id)?;
        let span = self.program.arena.expr(id).span;
        let lowered = self.lower_source_argument_values(plan.recipes.iter().map(|recipe| (recipe.entry_index, recipe.value, recipe.span)), slots, current_function, item_slot)?;
        self.record_original_argument_bindings(id, &plan.recipes, &lowered)?;
        let mut fields = vec![None; plan.parameters.len()];
        for ((argument, recipe), value) in plan.application.supplied.iter().zip(plan.recipes.iter()).zip(lowered.values) {
            fields[argument.slot] = Some(self.checked_unsigned_value(value, &plan.parameters[argument.slot].1, recipe.span));
        }
        let fields = fields.into_iter().collect::<Option<Vec<_>>>()?;
        let error = LoweredErrorExpr::Structured { family: plan.family.to_string(), variant: plan.member.to_string(),
            fields: plan.parameters.iter().zip(&fields).map(|((name, _), &value)| (Arc::from(name.as_str().as_str()), value)).collect(), facets: plan.facets.to_vec() };
        let value = push_build_row!(self, expr, BuildExprRow::Error(Box::new(error)));
        self.scratch.borrow_mut().error_constructor_sources.insert(value, OriginalErrorConstructorSource { plan, fields: fields.into_boxed_slice() });
        Some(self.wrap_argument_bindings(value, lowered.bindings, span))
    }
}
