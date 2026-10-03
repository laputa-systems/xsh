use super::*;
use crate::sema::check::{ConstructorAuthority, ConstructorValueSource, ExpressionIdentity, NominalDeclaration, NominalMemberKind, QualifiedNominalIdentity, SolvedArgumentSource, SolvedConstructorApplication};
use crate::runtime::eval::indexed::generic::OperationSourceOrigin;
use crate::sema::arguments::ArgumentValueSource;
use crate::sema::inference::{ConstraintRelation, ScopedRequirementRoot, ScopedRoot};

/// The original application owns its slots; the declaration owns the canonical family, member and wire mapping.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalTagConstructorPlan {
    pub origin: OperationSourceOrigin,
    pub checked: ScopedRoot,
    pub checked_type: Type,
    pub authority: QualifiedNominalIdentity,
    pub application: SolvedConstructorApplication,
    pub family: Name,
    pub member: Name,
    pub parameters: Box<[Type]>,
    pub wire: Option<Arc<crate::sema::wire_enums::WireEnumMapping>>,
    pub recipes: Box<[SolvedArgumentSource]>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalTagConstructorSource {
    pub plan: OriginalTagConstructorPlan,
    pub fields: Box<[BuildExprId]>,
}

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn original_tag_constructor_application(&self, id: ExprId) -> bool {
        self.solved().constructor_applications.get(&self.expression_identity(id)).is_some_and(|application| matches!(application.authority,
            ConstructorAuthority::Nominal(QualifiedNominalIdentity::Source { declaration: NominalDeclaration::Type(_), member: Some(_), .. })))
    }

    pub(super) fn original_tag_constructor_plan(&self, id: ExprId) -> Option<OriginalTagConstructorPlan> {
        let expression = self.expression_identity(id);
        let application = self.solved().constructor_applications.get(&expression)?.clone();
        self.original_tag_constructor_plan_from_source(OperationSourceOrigin::Expression(expression), application)
    }

    pub(super) fn original_tag_tail_constructor_plan(&self, statement: StmtId) -> Option<OriginalTagConstructorPlan> {
        let statement = self.statement_identity(statement);
        let source = self.solved().checked_tag_tail_constructor(statement).ok()?;
        self.original_tag_constructor_plan_from_source(OperationSourceOrigin::Statement(statement), source.application.clone())
    }

    fn original_tag_constructor_plan_from_source(&self, origin: OperationSourceOrigin, application: SolvedConstructorApplication) -> Option<OriginalTagConstructorPlan> {
        let solved = self.solved();
        let (expression, scope) = match origin {
            OperationSourceOrigin::Expression(expression) => {
                if solved.expression_owners.get(&expression).copied() != application.caller
                    || solved.graph.resolved(*solved.expressions.get(&expression)?).ok()? != solved.graph.resolved(application.result).ok()? { return None; }
                (Some(expression), solved.expression_scope(expression, application.caller).ok()?)
            }
            OperationSourceOrigin::Statement(statement) => {
                let source = solved.checked_tag_tail_constructor(statement).ok()?;
                if solved.statement_owners.get(&statement).copied() != application.caller || !source.application.supplied.is_empty() { return None; }
                (None, solved.tag_tail_constructor_scope(application.caller).ok()?)
            }
            _ => return None,
        };
        let ConstructorAuthority::Nominal(authority @ QualifiedNominalIdentity::Source { declaration: NominalDeclaration::Type(_), member: Some(_), .. }) = &application.authority else { return None; };
        let authority = *authority;
        if !application.default_slots.is_empty()
            || application.parameters.iter().any(|parameter| parameter.default.is_some()) { return None; }
        let checked = ScopedRoot { ty: application.result, scope };
        solved.graph.validate_scoped(checked).ok()?;
        let requirement = application.requirement?;
        solved.graph.validate_requirement_scoped(ScopedRequirementRoot { requirement, scope: checked.scope }).ok()?;
        let selected = solved.graph.candidate_evidence(requirement).ok()??;
        let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate).ok()? else { return None; };
        let crate::sema::operation_graph::PreparedLanguageOperation::Declaration { authority: selected_authority, identity } = metadata.operation else { return None; };
        if selected_authority != "language.constructor.tag" || metadata.authority != selected_authority
            || &*identity.as_str() != format!("{authority:?}") { return None; }
        let member = solved.checked_nominal_member(authority).ok()?;
        if member.kind != NominalMemberKind::Tag || member.scope.is_some() || member.fields.len() != application.parameters.len() { return None; }
        let checked_type = super::super::indexed::generic::graph_ground_type(&solved.graph, application.result).ok()?;
        let family_authority = match authority { QualifiedNominalIdentity::Source { source, namespace, declaration, .. } => QualifiedNominalIdentity::Source { source, namespace, declaration, member: None }, _ => return None };
        if !matches!(checked_type, Type::Tag(family) if family == member.family)
            || solved.nominals.get(&solved.graph.resolved(application.result).ok()?) != Some(&family_authority) { return None; }
        let declared = solved.constructor_nominals.get(&authority)?;
        if declared.len() != application.parameters.len() { return None; }
        let mut parameters = Vec::with_capacity(application.parameters.len());
        for (((label, declared), parameter), &(member_label, member_type)) in declared.iter().zip(&application.parameters).zip(&member.fields) {
            if label.is_some() || parameter.label.is_some() || member_label.is_some()
                || solved.graph.resolved(*declared).ok()? != solved.graph.resolved(parameter.ty).ok()? { return None; }
            solved.graph.validate_scoped(ScopedRoot { ty: parameter.ty, scope: checked.scope }).ok()?;
            let ty = super::super::indexed::generic::graph_ground_type(&solved.graph, parameter.ty).ok()?;
            if super::super::indexed::generic::graph_ground_type(&solved.graph, member_type).ok()? != ty { return None; }
            parameters.push(ty);
        }
        let recipes = match expression.and_then(|expression| solved.argument_sources.get(&expression)) {
            Some(recipes) => recipes.clone(),
            None if application.supplied.is_empty() => Vec::new(),
            None => return None,
        };
        if recipes.len() != application.supplied.len() { return None; }
        let mut occupied = std::collections::BTreeSet::new();
        for (recipe, argument) in recipes.iter().zip(&application.supplied) {
            let self_source = expression?;
            let parameter = application.parameters.get(argument.slot)?;
            if !occupied.insert(argument.slot) || recipe.name.is_some_and(|name| parameter.label != Some(name)) { return None; }
            let source = match (recipe.value, argument.value) {
                (ArgumentValueSource::Expression(expression), ConstructorValueSource::Expression(source)) if source == (ExpressionIdentity { expression, ..self_source }) => source,
                _ => return None,
            };
            let original = *solved.expressions.get(&source)?;
            solved.graph.validate_scoped(ScopedRoot { ty: original, scope: solved.expression_scope(source, application.caller).ok()? }).ok()?;
            let resolve = |ty| solved.graph.resolved(ty).ok();
            if argument.projection.is_some() || resolve(original) != resolve(argument.actual) { return None; }
            let ConstraintRelation::Assignable { expected, actual } = solved.graph.constraint_origins().get(argument.assignability)?.relation else { return None; };
            if resolve(expected) != resolve(parameter.ty) || resolve(actual) != resolve(argument.actual) { return None; }
            if super::super::indexed::generic::graph_ground_type(&solved.graph, argument.actual).is_err()
                && (parameters.get(argument.slot) != Some(&Type::Any) || checked.scope.is_none()) { return None; }
        }
        if occupied.len() != parameters.len() { return None; }
        Some(OriginalTagConstructorPlan { origin, checked, checked_type, authority, application, family: member.family, member: member.member,
            parameters: parameters.into_boxed_slice(), wire: member.wire.clone(), recipes: recipes.into_boxed_slice() })
    }

    pub(super) fn lower_original_tag_tail_constructor(&self, statement: StmtId) -> Option<BuildExprId> {
        let plan = self.original_tag_tail_constructor_plan(statement)?;
        if !plan.parameters.is_empty() || !plan.recipes.is_empty() { return None; }
        let value = push_build_row!(self, expr, BuildExprRow::Tag { type_name: plan.family, name: Arc::from(plan.member.as_str().as_str()), fields: Vec::new(), wire: plan.wire.clone() });
        self.scratch.borrow_mut().tag_constructor_sources.insert(value, OriginalTagConstructorSource { plan, fields: Box::new([]) });
        Some(value)
    }

    pub(super) fn lower_original_tag_constructor_call(&mut self, id: ExprId, slots: &mut SlotScope, current_function: Option<Name>, item_slot: Option<usize>) -> Option<BuildExprId> {
        let plan = self.original_tag_constructor_plan(id)?;
        let span = self.program.arena.expr(id).span;
        let lowered = self.lower_source_argument_values(plan.recipes.iter().map(|recipe| (recipe.entry_index, recipe.value, recipe.span)), slots, current_function, item_slot)?;
        if !plan.recipes.is_empty() { self.record_original_argument_bindings(id, &plan.recipes, &lowered)?; }
        let mut fields = vec![None; plan.parameters.len()];
        for ((argument, recipe), value) in plan.application.supplied.iter().zip(plan.recipes.iter()).zip(lowered.values) {
            fields[argument.slot] = Some(self.checked_unsigned_value(value, &plan.parameters[argument.slot], recipe.span));
        }
        let fields = fields.into_iter().collect::<Option<Vec<_>>>()?;
        let value = push_build_row!(self, expr, BuildExprRow::Tag { type_name: plan.family, name: Arc::from(plan.member.as_str().as_str()), fields: fields.clone(), wire: plan.wire.clone() });
        self.scratch.borrow_mut().tag_constructor_sources.insert(value, OriginalTagConstructorSource { plan, fields: fields.into_boxed_slice() });
        Some(self.wrap_argument_bindings(value, lowered.bindings, span))
    }
}
