use super::*;
use crate::sema::check::{ConstructorAuthority, ConstructorDefaultIdentity, ConstructorValueSource, ExpressionIdentity, SolvedArgumentSource, SolvedConstructorApplication};
use crate::sema::constants::LiteralConstant;
use crate::sema::inference::{ConstraintRelation, ScopedRoot};

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalRecordConstructorPlan {
    pub origin: ExpressionIdentity,
    pub checked: ScopedRoot,
    pub application: SolvedConstructorApplication,
    pub parameters: Box<[(Name, Type)]>,
    pub recipes: Box<[SolvedArgumentSource]>,
    pub defaults: Box<[(usize, ConstructorDefaultIdentity, ExpressionIdentity, LiteralConstant)]>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalRecordConstructorSource {
    pub plan: OriginalRecordConstructorPlan,
    pub record: BuildExprId,
    pub validation: BuildExprId,
    pub fields: Box<[BuildExprId]>,
    pub spreads: Box<[OriginalRecordConstructorSpread]>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalRecordConstructorSpread {
    pub ordinal: usize,
    pub origin: ExpressionIdentity,
    pub checked: ScopedRoot,
    pub record_initializer: BuildExprId,
    pub record_read: BuildExprId,
    pub record_slot: usize,
    pub field_initializer: BuildExprId,
    pub field_read: BuildExprId,
    pub field_slot: usize,
    pub field: Name,
}

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn original_record_constructor_plan(&self, id: ExprId) -> Option<OriginalRecordConstructorPlan> {
        use crate::sema::arguments::ArgumentValueSource;
        let origin = self.expression_identity(id);
        let solved = self.solved();
        let application = solved.constructor_applications.get(&origin)?.clone();
        let ConstructorAuthority::Record { application: schema, origin: declaration } = &application.authority else { return None; };
        if application.requirement.is_some() || application.expectation.applications.first() != Some(schema)
            || !application.expectation.applications.iter().any(|schema| schema.declaration == *declaration)
            || solved.expression_owners.get(&origin).copied() != application.caller { return None; }
        let checked = ScopedRoot { ty: application.result, scope: solved.expression_scope(origin, application.caller).ok()? };
        solved.graph.validate_scoped(checked).ok()?;
        if solved.graph.resolved(*solved.expressions.get(&origin)?).ok()? != solved.graph.resolved(application.result).ok()? { return None; }
        let Type::Record(fields) = super::super::indexed::generic::graph_ground_type(&solved.graph, application.result).ok()? else { return None; };
        for &argument in &schema.arguments { super::super::indexed::generic::graph_ground_type(&solved.graph, argument).ok()?; }
        if fields.len() != application.parameters.len() { return None; }
        let mut parameters = Vec::with_capacity(fields.len());
        for ((name, ty), parameter) in fields.iter().zip(&application.parameters) {
            if parameter.label != Some(*name) || super::super::indexed::generic::graph_ground_type(&solved.graph, parameter.ty).ok()? != *ty { return None; }
            parameters.push((*name, ty.clone()));
        }
        let recipes = solved.argument_sources.get(&origin)?.clone();
        if recipes.len() != application.supplied.len() { return None; }
        let mut occupied = std::collections::BTreeSet::new();
        for (recipe, argument) in recipes.iter().zip(&application.supplied) {
            let parameter = application.parameters.get(argument.slot)?;
            if !occupied.insert(argument.slot) || recipe.name != parameter.label { return None; }
            let source = match (recipe.value, argument.value) {
                (ArgumentValueSource::Expression(expression), ConstructorValueSource::Expression(source))
                    if source == (ExpressionIdentity { expression, ..origin }) => source,
                (ArgumentValueSource::RecordField { record, field }, ConstructorValueSource::RecordField { record: source, field: actual })
                    if field == actual && source == (ExpressionIdentity { expression: record, ..origin }) => source,
                _ => return None,
            };
            let original = *solved.expressions.get(&source)?;
            solved.graph.validate_scoped(ScopedRoot { ty: original, scope: solved.expression_scope(source, application.caller).ok()? }).ok()?;
            super::super::indexed::generic::graph_ground_type(&solved.graph, argument.actual).ok()?;
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
        }
        let mut defaults = Vec::with_capacity(application.default_slots.len());
        for &slot in &application.default_slots {
            if !occupied.insert(slot) { return None; }
            let parameter = application.parameters.get(slot)?;
            let identity = parameter.default?;
            let default = solved.constructor_defaults.get(&identity)?;
            if identity.owner != *declaration || identity.field != parameter.label? || default.owner != identity.owner || default.slot != slot { return None; }
            defaults.push((slot, identity, default.source, default.value.clone().in_type(&parameters.get(slot)?.1)));
        }
        if occupied.len() != parameters.len() { return None; }
        Some(OriginalRecordConstructorPlan { origin, checked, application, parameters: parameters.into_boxed_slice(), recipes: recipes.into_boxed_slice(), defaults: defaults.into_boxed_slice() })
    }

    pub(super) fn lower_original_record_constructor_call(&mut self, id: ExprId, slots: &mut SlotScope, current_function: Option<Name>, item_slot: Option<usize>) -> Option<BuildExprId> {
        let plan = self.original_record_constructor_plan(id)?;
        let span = self.program.arena.expr(id).span;
        let lowered = self.lower_source_argument_values(plan.recipes.iter().map(|recipe| (recipe.entry_index, recipe.value, recipe.span)), slots, current_function, item_slot)?;
        self.record_original_argument_bindings(id, &plan.recipes, &lowered)?;
        let mut spreads = Vec::new();
        for (ordinal, (recipe, &read)) in plan.recipes.iter().zip(&lowered.values).enumerate() {
            let crate::sema::arguments::ArgumentValueSource::RecordField { record, field } = recipe.value else { continue; };
            let scratch = self.scratch.borrow();
            let argument = scratch.argument_binding_origins.get(&read)?;
            let BuildExprRow::Field { base, name, .. } = scratch.expressions.get(argument.initializer.index())? else { return None; };
            if name.as_str() != field.as_str().as_str() { return None; }
            let binding = scratch.argument_record_binding_origins.get(base)?;
            let origin = ExpressionIdentity { expression: record, ..plan.origin };
            if binding.record != origin { return None; }
            spreads.push(OriginalRecordConstructorSpread { ordinal, origin, checked: binding.source_type,
                record_initializer: binding.initializer, record_read: *base, record_slot: binding.slot,
                field_initializer: argument.initializer, field_read: read, field_slot: argument.slot, field });
        }
        let mut fields = vec![None; plan.parameters.len()];
        for ((argument, recipe), value) in plan.application.supplied.iter().zip(plan.recipes.iter()).zip(lowered.values) {
            fields[argument.slot] = Some(self.checked_unsigned_value(value, &plan.parameters[argument.slot].1, recipe.span));
        }
        for (slot, _, _, value) in plan.defaults.iter() { fields[*slot] = Some(self.lower_record_default(value)?); }
        let fields = fields.into_iter().collect::<Option<Vec<_>>>()?;
        let entries = plan.parameters.iter().zip(&fields).map(|((name, _), value)| LoweredRecordEntry::Field(*name, *value)).collect();
        let record = push_build_row!(self, expr, BuildExprRow::Record(entries));
        let ty = Type::Record(plan.parameters.iter().cloned().collect());
        let schema = super::super::require::PreparedSchema::compile_record_layout(&ty)?;
        let check = LoweredTypeCheck { schema: Some(schema), ty, name: Arc::from("record constructor") };
        let validation = push_build_row!(self, expr, BuildExprRow::Require { value: record, check, span });
        let value = push_build_row!(self, expr, BuildExprRow::Try(validation));
        self.scratch.borrow_mut().record_constructor_sources.insert(value, OriginalRecordConstructorSource { plan, record, validation, fields: fields.into_boxed_slice(), spreads: spreads.into_boxed_slice() });
        Some(self.wrap_argument_bindings(value, lowered.bindings, span))
    }
}
