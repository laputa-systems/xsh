use super::*;

#[derive(Clone, Copy)]
pub(super) struct LexicalScope {
    frame: usize,
    loop_depth: usize,
}

struct LocalBinding {
    ty: SchemeType,
    mutable: bool,
    initializer: Option<(usize, LexicalScope)>,
}

struct LexicalFrame {
    parent: Option<usize>,
    bindings: BTreeMap<usize, LocalBinding>,
}

pub(super) struct LexicalScopes {
    frames: Vec<LexicalFrame>,
}

impl LexicalScope {
    fn binding<'a>(&self, scopes: &'a LexicalScopes, slot: usize, budget: &mut SchemeInstantiationBudget) -> Result<Option<&'a LocalBinding>, IrVerifyError> {
        let mut frame = Some(self.frame);
        while let Some(index) = frame {
            budget.charge(1)?;
            let current = &scopes.frames[index];
            if let Some(binding) = current.bindings.get(&slot) { return Ok(Some(binding)); }
            frame = current.parent;
        }
        Ok(None)
    }

    pub(super) fn literal_string(&self, scopes: &LexicalScopes, decoder: &FullDecoder<'_>, mut instruction: usize, budget: &mut SchemeInstantiationBudget) -> Result<Option<Arc<str>>, IrVerifyError> {
        let mut scope = *self;
        loop {
            budget.charge(1)?;
            match decoder.expression_view(instruction)? {
                schema::ExprView::Param(slot) => {
                    let Some(binding) = scope.binding(scopes, slot, budget)? else { return Ok(None) };
                    if binding.mutable { return Ok(None); }
                    let Some((source, source_scope)) = binding.initializer else { return Ok(None) };
                    instruction = source;
                    scope = source_scope;
                }
                _ => return decoder.checked_literal_string(instruction),
            }
        }
    }

    pub(super) fn slot_type<'a>(&self, scopes: &'a LexicalScopes, slot: usize, budget: &mut SchemeInstantiationBudget) -> Result<Option<&'a SchemeType>, IrVerifyError> {
        Ok(self.binding(scopes, slot, budget)?.map(|binding| &binding.ty))
    }
}

enum Obligation {
    Expression(usize, LexicalScope),
    Publish { statements: Arc<[usize]>, position: usize, scope: LexicalScope, value: Option<SchemeType>, bindings: Vec<(usize, SchemeType, bool, Option<usize>)> },
    Block { statements: Arc<[usize]>, position: usize, scope: LexicalScope, value: Option<SchemeType> },
}

fn expression_type(types: &BTreeMap<usize, SchemeType>, instruction: usize, budget: &mut SchemeInstantiationBudget) -> Result<SchemeType, IrVerifyError> {
    let ty = types.get(&instruction).ok_or_else(|| IrVerifyError::new("generic expression has no checked body fact"))?;
    budget.charge_scheme_clone(ty)?;
    Ok(ty.clone())
}

fn branch_scope(scope: &LexicalScope, scopes: &mut LexicalScopes, budget: &mut SchemeInstantiationBudget) -> Result<LexicalScope, IrVerifyError> {
    budget.charge(1)?;
    let frame = scopes.frames.len();
    scopes.frames.push(LexicalFrame { parent: Some(scope.frame), bindings: BTreeMap::new() });
    Ok(LexicalScope { frame, loop_depth: scope.loop_depth })
}

fn declare(scope: &LexicalScope, scopes: &mut LexicalScopes, slot: usize, ty: SchemeType, mutable: bool, initializer: Option<(usize, LexicalScope)>, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    if scope.binding(scopes, slot, budget)?.is_some() { return Err(IrVerifyError::new("generic declaration rewrites a visible slot")); }
    budget.charge(1)?;
    scopes.frames[scope.frame].bindings.insert(slot, LocalBinding { ty, mutable, initializer });
    Ok(())
}

fn condition(types: &BTreeMap<usize, SchemeType>, instruction: usize, role: crate::sema::types::ConditionRole, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    let ty = expression_type(types, instruction, budget)?.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic condition is not a ground truth value"))?;
    if !role.accepts(&ty) { return Err(IrVerifyError::new("generic condition disagrees with its control role")); }
    Ok(())
}

#[derive(Clone, Copy)]
enum BodyTypeRef<'a> {
    Scheme(&'a SchemeType),
    Ground(&'a Type),
}

impl<'a> BodyTypeRef<'a> {
    fn unvalidated(mut self) -> Self {
        loop {
            self = match self {
                Self::Scheme(SchemeType::Validated(_, base)) => Self::Scheme(base),
                Self::Scheme(SchemeType::Ground(ty)) => Self::Ground(ty),
                Self::Ground(Type::Validated(base)) => Self::Ground(base.base()),
                _ => return self,
            };
        }
    }

    fn is_record(self) -> bool {
        matches!(self.unvalidated(), Self::Scheme(SchemeType::Record { .. }) | Self::Scheme(SchemeType::Ground(Type::Record(_))) | Self::Ground(Type::Record(_)))
    }

    fn field(self, name: Name) -> Option<Self> {
        match self.unvalidated() {
            Self::Scheme(SchemeType::Record { fields, .. }) => fields.get(&name).map(Self::Scheme),
            Self::Scheme(SchemeType::Ground(ty)) => Self::Ground(ty).field(name),
            Self::Ground(Type::Record(fields)) => fields.get(&name).map(Self::Ground),
            _ => None,
        }
    }

    fn materialize(self, budget: &mut SchemeInstantiationBudget) -> Result<SchemeType, IrVerifyError> {
        match self {
            Self::Scheme(ty) => { budget.charge_scheme_clone(ty)?; Ok(ty.clone()) }
            Self::Ground(ty) => { budget.charge_clone(ty)?; Ok(SchemeType::Ground(ty.clone())) }
        }
    }
}

pub(super) fn record_path(ty: &SchemeType, path: &[Name], budget: &mut SchemeInstantiationBudget) -> Result<Option<SchemeType>, IrVerifyError> {
    let mut selected = BodyTypeRef::Scheme(ty);
    for &name in path {
        budget.charge(1)?;
        let Some(field) = selected.field(name) else { return Ok(None) };
        selected = field;
    }
    selected.materialize(budget).map(Some)
}

fn project(ty: &SchemeType, projection: super::super::CheckedValueProjection, budget: &mut SchemeInstantiationBudget) -> Result<Option<SchemeType>, IrVerifyError> {
    use super::super::CheckedValueProjection as P;
    if let P::Field(name) = projection { return record_path(ty, &[name], budget); }
    budget.charge(1)?;
    let selected = match (ty, projection) {
        (SchemeType::List(item) | SchemeType::Stream(item) | SchemeType::Set(item), P::Element | P::IterationItem) => Some(item.as_ref()),
        (SchemeType::Map(_, value), P::Element | P::IterationItem) => Some(value.as_ref()),
        (SchemeType::Result(ok, _), P::ResultPayload(true)) => Some(ok.as_ref()),
        (SchemeType::Result(_, error), P::ResultPayload(false)) => Some(error.as_ref()),
        _ => None,
    };
    if let Some(selected) = selected { budget.charge_scheme_clone(selected)?; return Ok(Some(selected.clone())); }
    let SchemeType::Ground(ty) = ty else { return Ok(None) };
    let selected = match (ty, projection) {
        (Type::Result(ok, _), P::ResultPayload(true)) => Some(ok.as_ref()),
        (Type::Result(_, error), P::ResultPayload(false)) => Some(error.as_ref()),
        _ => None,
    };
    if let Some(selected) = selected { budget.charge_clone(selected)?; return Ok(Some(SchemeType::Ground(selected.clone()))); }
    match projection {
        P::Element | P::IterationItem => {
            budget.charge_clone(ty)?;
            Ok(ty.iteration_item_type().map(SchemeType::Ground))
        }
        _ => Ok(None),
    }
}

fn iteration_item(source: &SchemeType, instruction: usize, expected_return: &SchemeType, budget: &mut SchemeInstantiationBudget) -> Result<(SchemeType, Option<super::producers::StreamProducer>), IrVerifyError> {
    budget.charge(1)?;
    match source {
        SchemeType::Result(ok, error) => {
            body_propagated_error_matches_owner(error, expected_return, budget)?;
            direct_iteration_item(ok, super::producers::StreamProducer::ResultSuccess(instruction), budget)
        }
        SchemeType::Ground(Type::Result(ok, error)) => {
            budget.charge_clone(error)?;
            body_propagated_error_matches_owner(&SchemeType::Ground((**error).clone()), expected_return, budget)?;
            budget.charge_clone(ok)?;
            direct_iteration_item(&SchemeType::Ground((**ok).clone()), super::producers::StreamProducer::ResultSuccess(instruction), budget)
        }
        SchemeType::Validated(_, source) => iteration_item(source, instruction, expected_return, budget),
        _ => direct_iteration_item(source, super::producers::StreamProducer::Value(instruction), budget),
    }
}

fn direct_iteration_item(source: &SchemeType, stream_source: super::producers::StreamProducer, budget: &mut SchemeInstantiationBudget) -> Result<(SchemeType, Option<super::producers::StreamProducer>), IrVerifyError> {
    budget.charge(1)?;
    match source {
        SchemeType::Stream(item) => { budget.charge_scheme_clone(item)?; Ok(((**item).clone(), Some(stream_source))) }
        SchemeType::List(item) | SchemeType::Set(item) => { budget.charge_scheme_clone(item)?; Ok(((**item).clone(), None)) }
        SchemeType::Map(key, value) => {
            budget.charge_scheme_clone(key)?;
            budget.charge_scheme_clone(value)?;
            budget.charge(2)?;
            Ok((SchemeType::Record { fields: BTreeMap::from([(Name::intern("key"), (**key).clone()), (Name::intern("value"), (**value).clone())]), tail: crate::sema::schemes::SchemeRowTail::Closed }, None))
        }
        SchemeType::Validated(_, source) => direct_iteration_item(source, stream_source, budget),
        SchemeType::Ground(source) => {
            if matches!(source.unvalidated(), Type::Result(_, _)) { return Err(IrVerifyError::new("generic iteration source has no iterable contract")); }
            budget.charge_clone(source)?;
            source.iteration_item_type().map(|item| (SchemeType::Ground(item), matches!(source.unvalidated(), Type::Stream(_)).then_some(stream_source))).ok_or_else(|| IrVerifyError::new("generic iteration source has no iterable contract"))
        }
        _ => Err(IrVerifyError::new("generic iteration source has no iterable contract")),
    }
}

fn target_bindings(target: super::super::decode::CompTargetView<'_>, source: &SchemeType, budget: &mut SchemeInstantiationBudget) -> Result<BTreeMap<usize, SchemeType>, IrVerifyError> {
    let mut bindings = BTreeMap::new();
    let work = RefCell::new(budget);
    FullDecoder::visit_comp_target_evidence(target, BodyTypeRef::Scheme(source),
        |source| if source.is_record() { Ok(()) } else { Err(IrVerifyError::new("generic record target has no record source")) },
        |source, name| Ok(source.field(name)),
        |slot, source| { bindings.insert(slot, source.materialize(&mut work.borrow_mut())?); Ok(()) },
        |amount| work.borrow_mut().charge(amount))?;
    Ok(bindings)
}

fn subject_after_pattern(decoder: &FullDecoder<'_>, pattern: u32, guarded: bool, subject: SchemeType, budget: &mut SchemeInstantiationBudget) -> Result<SchemeType, IrVerifyError> {
    budget.charge(1)?;
    if !decoder.pattern_excludes_null(pattern, guarded)? { return Ok(subject); }
    Ok(match subject {
        SchemeType::Optional(inner) => *inner,
        SchemeType::Ground(Type::Optional(inner)) => SchemeType::Ground(*inner),
        other => other,
    })
}

fn pattern_scope(decoder: &FullDecoder<'_>, pattern: u32, subject: &SchemeType, scope: &LexicalScope, scopes: &mut LexicalScopes, slot_count: usize, writers: &mut BTreeMap<usize, SchemeType>, budget: &mut SchemeInstantiationBudget) -> Result<LexicalScope, IrVerifyError> {
    let mut bindings = BTreeMap::new();
    let ground = subject.closed_ground_type();
    let work = RefCell::new(budget);
    decoder.visit_pattern_evidence(pattern, ground.as_ref(), Some(subject.clone()),
        |ty, step| {
            let Some(ty) = ty else { return Ok(None) };
            project(ty, step, &mut work.borrow_mut())
        },
        |slot, actual, projected| {
            if slot >= slot_count { return Err(IrVerifyError::new("generic pattern slot is out of bounds")); }
            let ty = match actual { Some(actual) => SchemeType::Ground(actual.clone()), None => projected.clone().ok_or_else(|| IrVerifyError::new("generic pattern binding has no authenticated projection"))? };
            work.borrow_mut().charge_scheme_clone(&ty)?;
            if let Some(previous) = bindings.insert(slot, ty.clone()) && previous != ty { return Err(IrVerifyError::new("generic pattern alternatives disagree on a binding")); }
            Ok(())
        },
        |view, actual, projected| {
            let ty = actual.cloned().or_else(|| projected.as_ref().and_then(SchemeType::closed_ground_type));
            match view {
                schema::PatternView::Literal(value) => {
                    if ty.is_none() && matches!(projected, Some(SchemeType::Optional(_))) && value.is_null()? { return Ok(()); }
                    let ty = ty.ok_or_else(|| IrVerifyError::new("generic literal pattern has no ground subject"))?;
                    value.with_scalar_view(|amount| work.borrow_mut().charge(amount), |value| {
                        if crate::runtime::eval::type_view::value_matches_static_type(value, &ty) { Ok(()) } else { Err(IrVerifyError::new("generic literal pattern disagrees with its subject")) }
                    })?;
                }
                schema::PatternView::List { .. } if !matches!(projected, Some(SchemeType::List(_))) && !matches!(ty, Some(Type::List(_))) => return Err(IrVerifyError::new("generic list pattern has no list subject")),
                schema::PatternView::RecordTest { .. } if !matches!(projected, Some(SchemeType::Record { .. })) && !matches!(ty, Some(Type::Record(_))) => return Err(IrVerifyError::new("generic record pattern has no record subject")),
                schema::PatternView::ResultTest { .. } | schema::PatternView::ResultOk { .. } | schema::PatternView::ResultErr { .. } if !matches!(projected, Some(SchemeType::Result(_, _))) && !matches!(ty, Some(Type::Result(_, _))) => return Err(IrVerifyError::new("generic result pattern has no result subject")),
                _ => {}
            }
            Ok(())
        },
        |amount| work.borrow_mut().charge(amount))?;
    let result = branch_scope(scope, scopes, &mut work.borrow_mut())?;
    for (slot, ty) in bindings {
        if writers.insert(slot, ty.clone()).is_some() { return Err(IrVerifyError::new("generic pattern slot has multiple declarations")); }
        declare(&result, scopes, slot, ty, false, None, &mut work.borrow_mut())?;
    }
    Ok(result)
}

pub(super) fn verify_body(
    store: &FullStore, decoder: &FullDecoder<'_>, types: &BTreeMap<usize, SchemeType>,
    tails: &mut BTreeMap<usize, SchemeType>, declarations: &mut BTreeMap<usize, (SchemeType, bool)>,
    slots: BTreeMap<usize, SchemeType>, expected_return: &SchemeType, statements: Vec<usize>,
    analysis: &super::super::IndexedReturnAnalysis, registered: &mut BTreeSet<u32>, budget: &mut SchemeInstantiationBudget,
) -> Result<(BTreeSet<usize>, BTreeMap<usize, SchemeType>, BTreeSet<usize>, Vec<super::producers::StreamProducer>), IrVerifyError> {
    let slot_count = decoder.slot_count as usize;
    for ty in slots.values() { budget.charge_scheme_clone(ty)?; }
    let mut writers = slots.clone();
    let mut scopes = LexicalScopes { frames: vec![LexicalFrame { parent: None, bindings: slots.into_iter().map(|(slot, ty)| (slot, LocalBinding { ty, mutable: false, initializer: None })).collect() }] };
    let scope = LexicalScope { frame: 0, loop_depth: 0 };
    let mut pending = vec![Obligation::Block { statements: statements.into(), position: 0, scope, value: None }];
    let mut visited = BTreeSet::new();
    let mut visited_statements = BTreeSet::new();
    let mut stream_iterations = Vec::new();
    while let Some(obligation) = pending.pop() {
        budget.charge(1)?;
        match obligation {
            Obligation::Publish { statements, position, scope, value, bindings } => {
                for (slot, ty, mutable, initializer) in bindings { declare(&scope, &mut scopes, slot, ty, mutable, initializer.map(|initializer| (initializer, scope)), budget)?; }
                pending.push(Obligation::Block { statements, position, scope, value });
            }
            Obligation::Expression(instruction, scope) => {
                let claimed = expression_type(types, instruction, budget)?;
                match decoder.expression_view(instruction)? {
                    schema::ExprView::ValueBlock { body, .. } => {
                        if !visited.insert(instruction) { return Err(IrVerifyError::new("generic expression is shared or cyclic")); }
                        let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                        budget.charge(statements.len())?;
                        pending.push(Obligation::Block { statements: statements.into(), position: 0, scope: branch_scope(&scope, &mut scopes, budget)?, value: Some(claimed) });
                    }
                    schema::ExprView::Loop { body, .. } => {
                        if !visited.insert(instruction) { return Err(IrVerifyError::new("generic expression is shared or cyclic")); }
                        if analysis.loop_unit_exits.as_ref().is_some_and(|exits| exits.contains(&instruction)) && !body_type_matches(&SchemeType::Ground(Type::Unit), &claimed, budget)? { return Err(IrVerifyError::new("generic value loop has a bare exit incompatible with its result")); }
                        for value in analysis.loop_values.as_ref().and_then(|values| values.get(&instruction)).into_iter().flatten() {
                            if !body_type_matches(&expression_type(types, *value, budget)?, &claimed, budget)? { return Err(IrVerifyError::new("generic value loop break disagrees with its result")); }
                        }
                        let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                        budget.charge(statements.len())?;
                        let mut inner = branch_scope(&scope, &mut scopes, budget)?;
                        inner.loop_depth += 1;
                        pending.push(Obligation::Block { statements: statements.into(), position: 0, scope: inner, value: None });
                    }
                    schema::ExprView::MatchExpr { value, mut arms, .. } => {
                        if !visited.insert(instruction) { return Err(IrVerifyError::new("generic expression is shared or cyclic")); }
                        let mut subject = expression_type(types, value.raw() as usize, budget)?;
                        for arm in &mut arms {
                            let (pattern, guard, value) = arm?;
                            let inner = pattern_scope(decoder, pattern.raw(), &subject, &scope, &mut scopes, slot_count, &mut writers, budget)?;
                            if !body_type_matches(&expression_type(types, value.raw() as usize, budget)?, &claimed, budget)? { return Err(IrVerifyError::new("generic match arm disagrees with its result")); }
                            pending.push(Obligation::Expression(value.raw() as usize, inner.clone()));
                            if let Some(guard) = guard {
                                condition(types, guard.raw() as usize, crate::sema::types::ConditionRole::Boolean, budget)?;
                                pending.push(Obligation::Expression(guard.raw() as usize, inner));
                            }
                            subject = subject_after_pattern(decoder, pattern.raw(), guard.is_some(), subject, budget)?;
                        }
                        pending.push(Obligation::Expression(value.raw() as usize, scope));
                    }
                    _ => {
                        let mut children = Vec::new();
                        verify_body_expression(store, decoder, instruction, types, &scope, &scopes, &mut children, &mut visited, registered, expected_return, budget)?;
                        for instruction in children { pending.push(Obligation::Expression(instruction, scope)); }
                    }
                }
            }
            Obligation::Block { statements, position, scope, value: value_contract } => {
                let Some(&instruction) = statements.get(position) else {
                    if value_contract.is_some() { return Err(IrVerifyError::new("generic value block has no terminal value")); }
                    continue;
                };
                if !visited_statements.insert(instruction) { return Err(IrVerifyError::new("generic statement is shared or cyclic")); }
                let mut expressions = Vec::new();
                let mut nested = Vec::new();
                match decoder.statement_view(instruction)? {
                    schema::StmtView::Let { slot, value, annotation } => {
                        let actual = expression_type(types, value.raw() as usize, budget)?;
                        let stored = scheme_stored_type(actual, annotation.map(|annotation| annotation.materialize()).transpose()?)?;
                        let (declared, mutable) = declarations.remove(&instruction).ok_or_else(|| IrVerifyError::new("generic local declaration has no permission fact"))?;
                        if !body_types_equal(&stored, &declared) { return Err(IrVerifyError::new("generic declaration disagrees with its initializer")); }
                        if slot >= slot_count || writers.insert(slot, declared.clone()).is_some() { return Err(IrVerifyError::new("generic declaration slot is duplicated or out of bounds")); }
                        let initializer = value.raw() as usize;
                        let initializer_scope = scope;
                        pending.push(Obligation::Publish { statements, position: position + 1, scope, value: value_contract, bindings: vec![(slot, declared, mutable, Some(initializer))] });
                        pending.push(Obligation::Expression(initializer, initializer_scope));
                        continue;
                    }
                    schema::StmtView::Assign { slot, op, value, check, .. } => {
                        let binding = scope.binding(&scopes, slot, budget)?.ok_or_else(|| IrVerifyError::new("generic assignment does not target a mutable declaration"))?;
                        if op != crate::syntax::node::AssignOp::Set || !binding.mutable { return Err(IrVerifyError::new("generic assignment does not target a mutable declaration")); }
                        let expected = &binding.ty;
                        let actual = expression_type(types, value.raw() as usize, budget)?;
                        if let Some(check) = check {
                            let checked = check.ty.materialize()?;
                            if check.schema.is_some() || !body_type_matches_ground(expected, &checked) || !body_type_matches_ground(&actual, &checked) || !checked.has_unsigned_constraint() { return Err(IrVerifyError::new("generic assignment has an invalid storage check")); }
                        } else if !body_type_matches(&actual, expected, budget)? || expected.closed_ground_type().is_some_and(|expected| expected.has_unsigned_constraint() && actual.closed_ground_type() != Some(expected)) { return Err(IrVerifyError::new("generic assignment disagrees with its invariant slot contract")); }
                        expressions.push(value.raw() as usize);
                    }
                    schema::StmtView::LetRecord { source, target, .. } => {
                        if matches!(target, super::super::decode::CompTargetView::Discard) {
                            expressions.push(source.raw() as usize);
                        } else {
                            let (declared, mutable) = declarations.remove(&instruction).ok_or_else(|| IrVerifyError::new("generic record declaration has no checked declaration fact"))?;
                            let actual = expression_type(types, source.raw() as usize, budget)?;
                            if !body_types_equal(&actual, &declared) { return Err(IrVerifyError::new("generic record declaration disagrees with its initializer")); }
                            let projected = target_bindings(target, &declared, budget)?;
                            let mut bindings = Vec::new();
                            for (slot, ty) in projected {
                                budget.charge_scheme_clone(&ty)?;
                                if slot >= slot_count || writers.insert(slot, ty.clone()).is_some() { return Err(IrVerifyError::new("generic record declaration slot is duplicated or out of bounds")); }
                                bindings.push((slot, ty, mutable, None));
                            }
                            pending.push(Obligation::Publish { statements, position: position + 1, scope, value: value_contract, bindings });
                            pending.push(Obligation::Expression(source.raw() as usize, scope));
                            continue;
                        }
                    }
                    schema::StmtView::Expr { value, .. } => expressions.push(value.raw() as usize),
                    schema::StmtView::Return { value: returned } => {
                        let actual = expression_type(types, returned.raw() as usize, budget)?;
                        if !actual.matches_body_return(expected_return) { return Err(IrVerifyError::new("generic body return does not match its scheme return")); }
                        if tails.remove(&instruction).is_some_and(|tail| !tail.matches_body_return(expected_return)) { return Err(IrVerifyError::new("generic tail disagrees with its scheme return")); }
                        expressions.push(returned.raw() as usize);
                    }
                    schema::StmtView::Value { value: returned } => {
                        let expected = value_contract.as_ref().ok_or_else(|| IrVerifyError::new("generic statement block contains an unowned value"))?;
                        if position + 1 != statements.len() || !body_type_matches(&expression_type(types, returned.raw() as usize, budget)?, expected, budget)? { return Err(IrVerifyError::new("generic block value disagrees with its contract")); }
                        pending.push(Obligation::Expression(returned.raw() as usize, scope));
                        continue;
                    }
                    schema::StmtView::BreakValue { value } => {
                        if scope.loop_depth == 0 { return Err(IrVerifyError::new("generic break has no lexical loop")); }
                        expressions.push(value.raw() as usize);
                    }
                    schema::StmtView::Break | schema::StmtView::Continue => {
                        if scope.loop_depth == 0 { return Err(IrVerifyError::new("generic loop transfer has no lexical loop")); }
                    }
                    schema::StmtView::If { mut branches, else_body } => {
                        for branch in &mut branches {
                            let (condition_value, body) = branch?;
                            condition(types, condition_value.raw() as usize, crate::sema::types::ConditionRole::Control, budget)?;
                            expressions.push(condition_value.raw() as usize);
                            let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                            budget.charge(statements.len())?;
                            nested.push(Obligation::Block { statements: statements.into(), position: 0, scope: branch_scope(&scope, &mut scopes, budget)?, value: None });
                        }
                        if let Some(body) = else_body {
                            let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                            budget.charge(statements.len())?;
                            nested.push(Obligation::Block { statements: statements.into(), position: 0, scope: branch_scope(&scope, &mut scopes, budget)?, value: None });
                        }
                    }
                    schema::StmtView::ForRecord { target, iter, body, .. } => {
                        let source = expression_type(types, iter.raw() as usize, budget)?;
                        let (item, stream_source) = iteration_item(&source, iter.raw() as usize, expected_return, budget)?;
                        if let Some(source) = stream_source { budget.charge(1)?; stream_iterations.push(source); }
                        let bindings = target_bindings(target, &item, budget)?;
                        let mut inner = branch_scope(&scope, &mut scopes, budget)?;
                        inner.loop_depth += 1;
                        for (slot, ty) in bindings {
                            budget.charge_scheme_clone(&ty)?;
                            if slot >= slot_count || writers.insert(slot, ty.clone()).is_some() { return Err(IrVerifyError::new("generic loop binding slot is duplicated or out of bounds")); }
                            declare(&inner, &mut scopes, slot, ty, false, None, budget)?;
                        }
                        expressions.push(iter.raw() as usize);
                        let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                        budget.charge(statements.len())?;
                        nested.push(Obligation::Block { statements: statements.into(), position: 0, scope: inner, value: None });
                    }
                    schema::StmtView::While { condition: condition_value, body } => {
                        condition(types, condition_value.raw() as usize, crate::sema::types::ConditionRole::Control, budget)?;
                        expressions.push(condition_value.raw() as usize);
                        let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                        budget.charge(statements.len())?;
                        let mut inner = branch_scope(&scope, &mut scopes, budget)?;
                        inner.loop_depth += 1;
                        nested.push(Obligation::Block { statements: statements.into(), position: 0, scope: inner, value: None });
                    }
                    schema::StmtView::Loop { body } => {
                        let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                        budget.charge(statements.len())?;
                        let mut inner = branch_scope(&scope, &mut scopes, budget)?;
                        inner.loop_depth += 1;
                        nested.push(Obligation::Block { statements: statements.into(), position: 0, scope: inner, value: None });
                    }
                    schema::StmtView::Match { value: subject, mut arms, .. } => {
                        let mut subject_type = expression_type(types, subject.raw() as usize, budget)?;
                        expressions.push(subject.raw() as usize);
                        for arm in &mut arms {
                            let (pattern, guard, body) = arm?;
                            let inner = pattern_scope(decoder, pattern.raw(), &subject_type, &scope, &mut scopes, slot_count, &mut writers, budget)?;
                            let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                            budget.charge(statements.len())?;
                            nested.push(Obligation::Block { statements: statements.into(), position: 0, scope: inner.clone(), value: None });
                            if let Some(guard) = guard {
                                condition(types, guard.raw() as usize, crate::sema::types::ConditionRole::Boolean, budget)?;
                                nested.push(Obligation::Expression(guard.raw() as usize, inner));
                            }
                            subject_type = subject_after_pattern(decoder, pattern.raw(), guard.is_some(), subject_type, budget)?;
                        }
                    }
                    _ => return Err(IrVerifyError::new("generic body has an unsupported statement contract")),
                }
                pending.push(Obligation::Block { statements, position: position + 1, scope: scope.clone(), value: value_contract });
                pending.extend(nested);
                for instruction in expressions { pending.push(Obligation::Expression(instruction, scope)); }
            }
        }
    }
    Ok((visited, writers, visited_statements, stream_iterations))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::super::{FullBuilder, FullProgram, FullVerifier};
    use crate::sema::check::Checker;
    use crate::source::SourceMap;
    use crate::syntax::parser::Parser;

    fn prepare(definition: &str) -> FullProgram {
        let source = format!("{definition}\n");
        let mut sources = SourceMap::default();
        let source_id = sources.add_file("generic-body-proof.xsh", source.clone());
        let parsed = Parser::parse_source_arena_only(source_id, &source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
        let program = FullBuilder::build_compact(&parsed.arena, &checked, &source, Arc::new(sources), source_id).unwrap();
        FullVerifier::verify(&program).unwrap();
        program
    }

    fn generic_owner(program: &FullProgram) -> (usize, FullSchemeId) {
        program.store.functions.iter().enumerate().find_map(|(owner, function)| match function.contract {
            FullCallableContract::Scheme(scheme) => Some((owner, scheme)),
            _ => None,
        }).unwrap()
    }

    fn declaration(program: &FullProgram) -> usize {
        program.store.scheme_body_facts.iter().position(|fact| fact.kind == FullSchemeBodyFactKind::LocalMutable).unwrap()
    }

    #[test]
    fn ordinary_generic_body_declarations_require_complete_unique_permissions() {
        let program = prepare("proc keep(value) [] { var result = value; result = value; result }");
        let (_, scheme) = generic_owner(&program);
        let declaration = declaration(&program);
        let mut missing = program.clone();
        missing.store.scheme_body_facts.remove(declaration);
        missing.store.schemes[scheme.index()].body_facts.len -= 1;
        assert_eq!(FullVerifier::verify(&missing).unwrap_err().message, "scheme type tables contain unowned rows");
        let mut duplicated = program.clone();
        duplicated.store.scheme_body_facts.push(program.store.scheme_body_facts[declaration]);
        duplicated.store.schemes[scheme.index()].body_facts.len += 1;
        assert_eq!(FullVerifier::verify(&duplicated).unwrap_err().message, "generic local declaration fact is duplicated");
        let mut escaped = program.clone();
        let other_owner = IrFunctionId::new(program.store.functions.len()).unwrap();
        escaped.store.scheme_body_facts[declaration].function = other_owner;
        assert_eq!(FullVerifier::verify(&escaped).unwrap_err().message, "generic body fact has invalid ownership");
    }

    #[test]
    fn ordinary_generic_body_assignment_requires_mutable_declaration() {
        let program = prepare("proc keep(value) [] { var result = value; result = value; result }");
        let mut immutable = program.clone();
        immutable.store.scheme_body_facts[declaration(&program)].kind = FullSchemeBodyFactKind::LocalImmutable;
        assert_eq!(FullVerifier::verify(&immutable).unwrap_err().message, "generic assignment does not target a mutable declaration");
    }

    #[test]
    fn ordinary_generic_body_all_writers_preserve_one_scheme_binder() {
        let program = prepare("proc keep(value, other) [] { let _ = other; var result = value; result = value; result }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let assigned = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Assign { value, .. }) => Some(value.raw() as usize),
            _ => None,
        }).unwrap();
        let schema::ExprView::Param(temporary) = decoder.expression_view(assigned).unwrap() else { panic!("assignment retains its evaluated value") };
        let (initializer, declaration) = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Let { slot, value, .. }) if slot == temporary => Some((value.raw() as usize, instruction)),
            _ => None,
        }).unwrap();
        assert!(matches!(decoder.expression_view(initializer).unwrap(), schema::ExprView::Param(0)));
        let alternate = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(1)))).unwrap();
        let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        let alternate_type = program.store.scheme_params[parameters.start + 1].ty;
        let mut changed = program.clone();
        changed.store.data[initializer] = program.store.data[alternate];
        let alternate_node = program.store.scheme_types[alternate_type.index()];
        for fact in &changed.store.scheme_body_facts {
            if [initializer, assigned, declaration].contains(&(fact.instruction as usize)) {
                assert_ne!(fact.ty, program.store.scheme_params[parameters.start].ty);
                let node = &mut changed.store.scheme_types[fact.ty.index()];
                assert_eq!(node.tag, alternate_node.tag);
                node.first = alternate_node.first;
            }
        }
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic assignment disagrees with its invariant slot contract");
    }

    #[test]
    fn ordinary_generic_body_assignments_cannot_escape_nested_scope() {
        let program = prepare("proc keep(value) [] { var result = value; { var hidden = value; hidden = value }; result = value; result }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let mutables = program.store.scheme_body_facts.iter().filter(|fact| fact.kind == FullSchemeBodyFactKind::LocalMutable).map(|fact| {
            let schema::StmtView::Let { slot, .. } = decoder.statement_view(fact.instruction as usize).unwrap() else { unreachable!() };
            slot
        }).collect::<Vec<_>>();
        assert_eq!(mutables.len(), 2);
        let assignment = range.filter(|instruction| matches!(decoder.statement_view(*instruction), Ok(schema::StmtView::Assign { slot, .. }) if slot == mutables[0])).last().unwrap();
        let mut escaped = program.clone();
        let payload = escaped.store.data[assignment].range().bounds(escaped.store.extra.len()).unwrap();
        escaped.store.extra[payload.start] = mutables[1] as u32;
        assert_eq!(FullVerifier::verify(&escaped).unwrap_err().message, "generic assignment does not target a mutable declaration");
    }

    #[test]
    fn ordinary_generic_body_declarations_cannot_invent_unsigned_validation() {
        let program = prepare("pure keep(value, unsigned: UInt) { let _ = 7; value }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let (declaration, slot, literal) = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Let { slot, value, annotation: None }) if matches!(decoder.expression_view(value.raw() as usize), Ok(schema::ExprView::Int(7))) => Some((instruction, slot, value.raw() as usize)),
            _ => None,
        }).unwrap();
        let read = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(actual)) if actual == slot)).unwrap();
        let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        let unsigned = program.store.scheme_types[program.store.scheme_params[parameters.start + 1].ty.index()];
        let mut changed = program.clone();
        let payload = changed.store.data[literal].range().bounds(changed.store.extra.len()).unwrap();
        assert_eq!(payload.len(), 2);
        changed.store.extra[payload].fill(u32::MAX);
        for fact in &changed.store.scheme_body_facts {
            if [declaration, read].contains(&(fact.instruction as usize)) {
                let node = &mut changed.store.scheme_types[fact.ty.index()];
                assert_eq!(node.tag, unsigned.tag);
                node.first = unsigned.first;
            }
        }
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic declaration disagrees with its initializer");
    }

    #[test]
    fn ordinary_generic_body_loops_preserve_owned_reachable_exits_without_producer_demand() {
        for definition in [
            "pure keep(value) { let result = loop { break value }; result }",
            "pure keep(value) { let result = loop { let inner = loop { break 1 }; let _ = inner; break value }; result }",
            "pure keep(value) { let result = loop { break value; break }; result }",
        ] {
            let program = prepare(definition);
            assert_eq!(GroundProducerIndex::new(&program.store).unwrap().demand(), crate::runtime::eval::ProducerDemand::Absent);
        }
    }
    #[test]
    fn ordinary_generic_body_patterns_authenticate_the_actual_subject() {
        let program = prepare("pure choose(flag: Bool, value) { let _ = match 1 { 1 => value; _ => value }; match flag { true => value; false => value } }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let mut boolean = None;
        let mut integer = None;
        for pattern in 0..program.store.patterns.len() {
            if let schema::PatternView::Literal(value) = decoder.checked_pattern_view(pattern as u32).unwrap() {
                match value.materialize().unwrap() {
                    crate::runtime::eval::LoweredValue::Bool(true) => boolean = Some(pattern),
                    crate::runtime::eval::LoweredValue::Int(1) => integer = Some(pattern),
                    _ => {}
                }
            }
        }
        let mut changed = program.clone();
        changed.store.pattern_data[boolean.unwrap()] = program.store.pattern_data[integer.unwrap()];
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic literal pattern disagrees with its subject");
    }

    #[test]
    fn ordinary_generic_body_match_arms_preserve_the_authentic_result_binder() {
        let program = prepare("pure choose(flag: Bool, value, other) { match flag { true => value; false => value } }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let instruction = program.store.function_instruction_range(owner).unwrap().find(|instruction| program.store.tags[*instruction] == FullTag::ExprMatch).unwrap();
        let schema::ExprView::MatchExpr { mut arms, .. } = decoder.expression_view(instruction).unwrap() else { unreachable!() };
        let (_, _, arm) = arms.next().unwrap().unwrap();
        let schema::ExprView::ValueBlock { mut body, .. } = decoder.expression_view(arm.raw() as usize).unwrap() else { panic!("a match arm retains its lexical value block") };
        let statement = body.next().unwrap().unwrap();
        assert!(body.next().is_none());
        let schema::StmtView::Value { value } = decoder.statement_view(statement.raw() as usize).unwrap() else { panic!("the selected arm returns its parameter") };
        assert!(matches!(decoder.expression_view(value.raw() as usize).unwrap(), schema::ExprView::Param(1)));
        let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        let alternate = program.store.scheme_types[program.store.scheme_params[parameters.start + 2].ty.index()];
        let mut changed = program.clone();
        let payload = changed.store.data[value.raw() as usize].range().bounds(changed.store.extra.len()).unwrap();
        assert_eq!(payload.len(), 1);
        changed.store.extra[payload.start] = 2;
        for fact in &changed.store.scheme_body_facts {
            if [arm.raw(), value.raw()].contains(&fact.instruction) {
                let node = &mut changed.store.scheme_types[fact.ty.index()];
                assert_eq!(node.tag, alternate.tag);
                node.first = alternate.first;
            }
        }
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic match arm disagrees with its result");
    }

    #[test]
    fn ordinary_generic_body_bare_loop_exits_cannot_claim_an_unrelated_binder() {
        let program = prepare("pure keep(value) { let ignored = loop { break }; value }");
        assert_eq!(GroundProducerIndex::new(&program.store).unwrap().demand(), crate::runtime::eval::ProducerDemand::Absent);
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let instruction = range.clone().find(|instruction| program.store.tags[*instruction] == FullTag::ExprLoop).unwrap();
        let (declaration, slot) = range.clone().find_map(|statement| match decoder.statement_view(statement) {
            Ok(schema::StmtView::Let { slot, value, annotation: None }) if value.raw() as usize == instruction => Some((statement, slot)),
            _ => None,
        }).unwrap();
        let read = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(actual)) if actual == slot)).unwrap();
        let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        let claimed = program.store.scheme_types[program.store.scheme_params[parameters.start].ty.index()];
        let mut changed = program.clone();
        for fact in &changed.store.scheme_body_facts {
            if [instruction, declaration, read].contains(&(fact.instruction as usize)) {
                let node = &mut changed.store.scheme_types[fact.ty.index()];
                assert_eq!(node.children.len, 0);
                *node = claimed;
            }
        }
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic value loop has a bare exit incompatible with its result");
    }

    #[test]
    fn ordinary_generic_body_proof_work_is_bounded_for_wide_sibling_scopes() {
        for width in [256, 512] {
            let mut source = String::from("pure keep(value) {\n");
            for index in 0..width { source.push_str(&format!("let outer{index} = value\n")); }
            for _ in 0..width { source.push_str("{ let copy = value; let _ = copy }\n"); }
            source.push_str("value\n}\n");
            prepare(&source);
        }
    }

    #[test]
    fn ordinary_generic_body_nested_reads_cannot_see_future_parent_declarations() {
        let program = prepare("pure keep(value) { { let copy = value; let _ = copy }; let later = value; value }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let function = program.store.functions[owner];
        let block = &program.store.blocks[IrBlockId::from_raw(function.body).unwrap().index()];
        let statements = program.store.payload(block.instructions).unwrap();
        let later = statements.iter().skip(1).rev().find_map(|statement| match decoder.statement_view(*statement as usize) {
            Ok(schema::StmtView::Let { slot, .. }) => Some(slot),
            _ => None,
        }).unwrap();
        let schema::StmtView::Expr { value, .. } = decoder.statement_view(statements[1] as usize).unwrap() else { panic!("the first statement owns the nested scope") };
        let schema::ExprView::ValueBlock { mut body, .. } = decoder.expression_view(value.raw() as usize).unwrap() else { unreachable!() };
        let declaration = body.next().unwrap().unwrap();
        let schema::StmtView::Let { value, .. } = decoder.statement_view(declaration.raw() as usize).unwrap() else { unreachable!() };
        assert!(matches!(decoder.expression_view(value.raw() as usize).unwrap(), schema::ExprView::Param(0)));
        let mut changed = program.clone();
        let payload = changed.store.data[value.raw() as usize].range().bounds(changed.store.extra.len()).unwrap();
        assert_eq!(payload.len(), 1);
        changed.store.extra[payload.start] = later as u32;
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic body reads an uninitialized slot");
    }

    #[test]
    fn ordinary_generic_body_compound_producers_cannot_invent_unsigned_validation() {
        let program = prepare("pure keep(value, unsigned: UInt, signed: Int) { let _ = loop { break unsigned }; value }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let instruction = program.store.function_instruction_range(owner).unwrap().find(|instruction| program.store.tags[*instruction] == FullTag::ExprLoop).unwrap();
        let schema::ExprView::Loop { body, .. } = decoder.expression_view(instruction).unwrap() else { unreachable!() };
        let statements = body.map(|statement| statement.unwrap().raw() as usize).collect::<Vec<_>>();
        let value = statements.iter().find_map(|statement| match decoder.statement_view(*statement) {
            Ok(schema::StmtView::BreakValue { value }) => Some(value.raw() as usize),
            _ => None,
        }).unwrap();
        let schema::ExprView::Param(slot) = decoder.expression_view(value).unwrap() else { unreachable!() };
        let (declaration, initializer) = statements.iter().find_map(|statement| match decoder.statement_view(*statement) {
            Ok(schema::StmtView::Let { slot: actual, value, annotation: None }) if actual == slot => Some((*statement, value.raw() as usize)),
            _ => None,
        }).unwrap();
        assert!(matches!(decoder.expression_view(initializer).unwrap(), schema::ExprView::Param(1)));
        let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        let signed = program.store.scheme_types[program.store.scheme_params[parameters.start + 2].ty.index()];
        let mut changed = program.clone();
        let payload = changed.store.data[initializer].range().bounds(changed.store.extra.len()).unwrap();
        assert_eq!(payload.len(), 1);
        changed.store.extra[payload.start] = 2;
        for fact in &changed.store.scheme_body_facts {
            if [declaration, initializer, value].contains(&(fact.instruction as usize)) {
                let node = &mut changed.store.scheme_types[fact.ty.index()];
                assert_eq!(node.tag, signed.tag);
                node.first = signed.first;
            }
        }
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic value loop break disagrees with its result");
    }

    #[test]
    fn ordinary_generic_aggregate_binary_cannot_invent_unsigned_items() {
        let program = prepare("pure keep(value, unsigned: List[UInt], signed: List[Int]) { let _ = unsigned + unsigned; value }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let right = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::Binary { right, .. }) => Some(right.raw() as usize),
            _ => None,
        }).unwrap();
        let schema::ExprView::Param(slot) = decoder.expression_view(right).unwrap() else { unreachable!() };
        let (declaration, initializer) = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Let { slot: actual, value, .. }) if actual == slot => Some((instruction, value.raw() as usize)),
            _ => None,
        }).unwrap();
        assert!(matches!(decoder.expression_view(initializer).unwrap(), schema::ExprView::Param(1)));
        let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        let signed = program.store.scheme_types[program.store.scheme_params[parameters.start + 2].ty.index()];
        let signed_item = program.store.scheme_types[SchemeTypeId::from_raw(signed.first).unwrap().index()];
        let mut changed = program.clone();
        let payload = changed.store.data[initializer].range().bounds(changed.store.extra.len()).unwrap();
        assert_eq!(payload.len(), 1);
        changed.store.extra[payload.start] = 2;
        for fact in &changed.store.scheme_body_facts {
            if [declaration, initializer, right].contains(&(fact.instruction as usize)) {
                let node = &mut changed.store.scheme_types[fact.ty.index()];
                assert_eq!(node.tag, signed.tag);
                let item = SchemeTypeId::from_raw(node.first).unwrap();
                let item = &mut changed.store.scheme_types[item.index()];
                assert_eq!(item.tag, signed_item.tag);
                item.first = signed_item.first;
            }
        }
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic binary operands disagree with the operation");
    }

    #[test]
    fn ordinary_generic_aggregate_validated_projections_preserve_base_contracts() {
        for operation in ["items[0]", "items[0..1]"] {
            prepare(&format!("pure keep(value, items: NonEmpty[Int]) {{ let _ = {operation}; value }}"));
        }
    }

    #[test]
    fn ordinary_generic_aggregate_bounds_authenticate_the_actual_operand() {
        for operation in ["[value][index]", "[value][index..]"] {
            let program = prepare(&format!("pure keep(value, index: Int, wrong: Str) {{ {operation} }}"));
            let (owner, scheme) = generic_owner(&program);
            let decoder = decoder_for_function(&program.store, owner).unwrap();
            let range = program.store.function_instruction_range(owner).unwrap();
            let operand = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
                Ok(schema::ExprView::Index { index, .. }) => Some(index.raw() as usize),
                Ok(schema::ExprView::Slice { start: Some(start), .. }) => Some(start.raw() as usize),
                _ => None,
            }).unwrap();
            let schema::ExprView::Param(slot) = decoder.expression_view(operand).unwrap() else { unreachable!() };
            let (declaration, initializer) = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
                Ok(schema::StmtView::Let { slot: actual, value, .. }) if actual == slot => Some((instruction, value.raw() as usize)),
                _ => None,
            }).unwrap();
            assert!(matches!(decoder.expression_view(initializer).unwrap(), schema::ExprView::Param(1)));
            let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
            let wrong = program.store.scheme_types[program.store.scheme_params[parameters.start + 2].ty.index()];
            let mut changed = program.clone();
            let payload = changed.store.data[initializer].range().bounds(changed.store.extra.len()).unwrap();
            assert_eq!(payload.len(), 1);
            changed.store.extra[payload.start] = 2;
            for fact in &changed.store.scheme_body_facts {
                if [declaration, initializer, operand].contains(&(fact.instruction as usize)) {
                    let node = &mut changed.store.scheme_types[fact.ty.index()];
                    assert_eq!(node.tag, wrong.tag);
                    node.first = wrong.first;
                }
            }
            let message = FullVerifier::verify(&changed).unwrap_err().message;
            assert!(matches!(message.as_str(), "generic index operands have no collection contract" | "generic slice bound is not an integer"), "{message}");
        }
    }

    #[test]
    fn ordinary_generic_aggregate_record_keys_follow_the_actual_initializer() {
        let program = prepare("pure keep(value) { let _ = \"other\"; {item: value, other: 1}[\"item\"] }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let key = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Str("item")))).unwrap();
        let other = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Str("other")))).unwrap();
        let mut changed = program.clone();
        changed.store.data[key] = program.store.data[other];
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic body expression disagrees with its checked type");
    }



    #[test]
    fn ordinary_generic_guarded_optional_projection_preserves_the_receiver() {
        prepare("pure keep(value, items: List[Int]?) { let _ = items?[0]; value }");
    }


    fn guarded_receiver_source(program: &FullProgram, owner: usize, source: usize) -> (usize, usize, usize) {
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let schema::ExprView::Param(slot) = decoder.expression_view(source).unwrap() else { panic!("guard receiver ANF read") };
        let (declaration, initializer) = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Let { slot: actual, value, .. }) if actual == slot => Some((instruction, value.raw() as usize)),
            _ => None,
        }).unwrap();
        assert!(matches!(decoder.expression_view(initializer).unwrap(), schema::ExprView::Param(1)));
        (declaration, initializer, source)
    }

    #[test]
    fn ordinary_generic_guarded_results_authenticate_the_owner_error_family() {
        let program = prepare("error First = Missing\nerror Second = Missing\npure keep(value, items: Result[List[Int], First], wrong: Result[List[Int], Second]) -> Result[Int, First] { let _ = value; let _ = wrong; Ok(items?[0]) }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let source = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::Try { value, .. }) => Some(value.raw() as usize),
            _ => None,
        }).unwrap();
        let (declaration, initializer, read) = guarded_receiver_source(&program, owner, source);
        let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        let wrong = program.store.scheme_types[program.store.scheme_params[parameters.start + 2].ty.index()];
        assert_eq!(wrong.tag, FullSchemeTypeTag::Result);
        let error = program.store.scheme_types[SchemeTypeId::from_raw(wrong.second).unwrap().index()];
        let mut changed = program.clone();
        let payload = changed.store.data[initializer].range().bounds(changed.store.extra.len()).unwrap();
        assert_eq!(payload.len(), 1);
        changed.store.extra[payload.start] = 2;
        for fact in &changed.store.scheme_body_facts {
            if [declaration, initializer, read].contains(&(fact.instruction as usize)) {
                let result = changed.store.scheme_types[fact.ty.index()];
                assert_eq!(result.tag, FullSchemeTypeTag::Result);
                let actual_error = &mut changed.store.scheme_types[SchemeTypeId::from_raw(result.second).unwrap().index()];
                assert_eq!(actual_error.tag, error.tag);
                actual_error.first = error.first;
                actual_error.second = error.second;
            }
        }
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic propagated error disagrees with its owner boundary");
    }

    #[test]
    fn ordinary_generic_guarded_receivers_follow_the_actual_nullable_producer() {
        let program = prepare("pure keep(value, items: List[Int]?, wrong: List[Str]?) { let _ = wrong; let _ = items?[0]; value }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let source = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::MatchExpr { value, .. }) => Some(value.raw() as usize),
            _ => None,
        }).unwrap();
        let (declaration, initializer, read) = guarded_receiver_source(&program, owner, source);
        let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        let wrong = program.store.scheme_types[program.store.scheme_params[parameters.start + 2].ty.index()];
        assert_eq!(wrong.tag, FullSchemeTypeTag::Optional);
        let list = program.store.scheme_types[SchemeTypeId::from_raw(wrong.first).unwrap().index()];
        assert_eq!(list.tag, FullSchemeTypeTag::List);
        let item = program.store.scheme_types[SchemeTypeId::from_raw(list.first).unwrap().index()];
        let mut changed = program.clone();
        let payload = changed.store.data[initializer].range().bounds(changed.store.extra.len()).unwrap();
        changed.store.extra[payload.start] = 2;
        for fact in &changed.store.scheme_body_facts {
            if [declaration, initializer, read].contains(&(fact.instruction as usize)) {
                let optional = changed.store.scheme_types[fact.ty.index()];
                assert_eq!(optional.tag, FullSchemeTypeTag::Optional);
                let list = changed.store.scheme_types[SchemeTypeId::from_raw(optional.first).unwrap().index()];
                assert_eq!(list.tag, FullSchemeTypeTag::List);
                let actual = &mut changed.store.scheme_types[SchemeTypeId::from_raw(list.first).unwrap().index()];
                assert_eq!(actual.tag, item.tag);
                actual.first = item.first;
            }
        }
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic body expression disagrees with its checked type");
    }

    #[test]
    fn ordinary_generic_guarded_present_bindings_require_prior_null_exclusion() {
        let program = prepare("pure keep(value, items: List[Int]?) { let _ = items?[0]; value }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let matched = program.store.function_instruction_range(owner).unwrap().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::MatchExpr { .. }))).unwrap();
        let payload = program.store.payload(program.store.data[matched].range()).unwrap();
        let block = IrBlockId::from_raw(payload[1]).unwrap();
        let range = program.store.blocks[block.index()].instructions.bounds(program.store.extra.len()).unwrap();
        assert_eq!(program.store.extra[range.start], 2);
        assert_eq!(range.len(), 7);
        let mut changed = program.clone();
        for index in 0..3 { changed.store.extra.swap(range.start + 1 + index, range.start + 4 + index); }
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic body expression disagrees with its checked type");
    }


    #[test]
    fn ordinary_generic_guarded_explicit_matches_preserve_optional_binding_types() {
        prepare("pure keep(value, items: List[Int]?) { let _ = match items { null => items; present => present }; value }");
    }


    #[test]
    fn ordinary_generic_guarded_uint_map_methods_retain_key_validation() {
        prepare("pure keep(marker, items: Map[UInt, Int]?, key: UInt) { let _ = items?.get(key); marker }");
    }


    #[test]
    fn ordinary_generic_guarded_key_requirements_preserve_the_owner_error_family() {
        let program = prepare("error First = Missing\npure keep(marker, items: Map[UInt, Int]?, key: UInt, wrong: Int) -> Result[Int, First] { let _ = marker; let _ = wrong; let _ = items?.get(key); Ok(7) }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let source = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::Require { value, check, .. }) if check.ty.materialize().unwrap() == Type::UInt => Some(value.raw() as usize),
            _ => None,
        }).unwrap();
        let schema::ExprView::Param(slot) = decoder.expression_view(source).unwrap() else { panic!("unsigned argument ANF read") };
        let (declaration, initializer) = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Let { slot: actual, value, .. }) if actual == slot => Some((instruction, value.raw() as usize)),
            _ => None,
        }).unwrap();
        assert!(matches!(decoder.expression_view(initializer).unwrap(), schema::ExprView::Param(2)));
        let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        let wrong = program.store.scheme_types[program.store.scheme_params[parameters.start + 3].ty.index()];
        assert_eq!(wrong.tag, FullSchemeTypeTag::Ground);
        let mut changed = program.clone();
        let payload = changed.store.data[initializer].range().bounds(changed.store.extra.len()).unwrap();
        assert_eq!(payload.len(), 1);
        changed.store.extra[payload.start] = 3;
        for fact in &changed.store.scheme_body_facts {
            if [declaration, initializer, source].contains(&(fact.instruction as usize)) {
                let node = &mut changed.store.scheme_types[fact.ty.index()];
                assert_eq!(node.tag, wrong.tag);
                node.first = wrong.first;
            }
        }
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic propagated error disagrees with its owner boundary");
    }


    #[test]
    fn nullable_branch_inference_authenticates_the_actual_nonnull_payload() {
        let program = prepare("pure keep(marker, flag: Bool, number: Int, wrong: Str) { let _ = marker; let _ = wrong; if flag { number } else { null } }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let branch = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::IfExpr { mut branches, .. }) => Some(branches.next().unwrap().unwrap().1.raw() as usize),
            _ => None,
        }).unwrap();
        let schema::ExprView::ValueBlock { body, .. } = decoder.expression_view(branch).unwrap() else { panic!("nonnull branch value block") };
        let statements = body.map(|statement| statement.unwrap().raw() as usize).collect::<Vec<_>>();
        let read = statements.iter().find_map(|instruction| match decoder.statement_view(*instruction).unwrap() {
            schema::StmtView::Value { value } => Some(value.raw() as usize),
            _ => None,
        }).unwrap();
        assert!(matches!(decoder.expression_view(read).unwrap(), schema::ExprView::Param(2)));
        let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        let wrong = program.store.scheme_types[program.store.scheme_params[parameters.start + 3].ty.index()];
        let mut changed = program.clone();
        let payload = changed.store.data[read].range().bounds(changed.store.extra.len()).unwrap();
        changed.store.extra[payload.start] = 3;
        for fact in &changed.store.scheme_body_facts {
            if [branch, read].contains(&(fact.instruction as usize)) {
                let ty = &mut changed.store.scheme_types[fact.ty.index()];
                assert_eq!(ty.tag, wrong.tag);
                ty.first = wrong.first;
            }
        }
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic branch value disagrees with its checked contract");
    }


    #[test]
    fn nullable_branch_inference_preserves_required_optional_tail_contracts() {
        for expression in [
            "if flag { 7 } else { null }",
            "if flag { null } else { 7 }",
            "match flag { true => 7; false => null }",
            "match flag { true => null; false => 7 }",
        ] {
            prepare(&format!("pure keep(marker, flag: Bool) -> Int? {{ let _ = marker; {expression} }}"));
        }
    }

    fn retarget_construction_operand(program: &FullProgram, owner: usize, scheme: FullSchemeId, operand: usize, wrapped: bool) -> FullProgram {
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let mut instructions = vec![operand];
        let read = if let schema::ExprView::ValueBlock { body, .. } = decoder.expression_view(operand).unwrap() {
            let statements = body.map(|statement| statement.unwrap().raw() as usize).collect::<Vec<_>>();
            let schema::StmtView::Value { value } = decoder.statement_view(*statements.last().unwrap()).unwrap() else { panic!("the operand retains its value tail") };
            instructions.push(value.raw() as usize);
            value.raw() as usize
        } else { operand };
        let schema::ExprView::Param(slot) = decoder.expression_view(read).unwrap() else { panic!("the constructor reads its bound operand") };
        let (declaration, initializer) = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Let { slot: actual, value, .. }) if actual == slot => Some((instruction, value.raw() as usize)),
            _ => None,
        }).unwrap();
        assert!(matches!(decoder.expression_view(initializer).unwrap(), schema::ExprView::Param(1)));
        instructions.extend([declaration, initializer]);
        let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        let wrong = program.store.scheme_types[program.store.scheme_params[parameters.start + 2].ty.index()];
        let wrong_child = wrapped.then(|| program.store.scheme_types[SchemeTypeId::from_raw(wrong.first).unwrap().index()]);
        let mut changed = program.clone();
        let payload = changed.store.data[initializer].range().bounds(changed.store.extra.len()).unwrap();
        assert_eq!(payload.len(), 1);
        changed.store.extra[payload.start] = 2;
        for fact in &changed.store.scheme_body_facts {
            if instructions.contains(&(fact.instruction as usize)) {
                let node = &mut changed.store.scheme_types[fact.ty.index()];
                assert_eq!(node.tag, wrong.tag);
                if let Some(wrong_child) = wrong_child {
                    let child = SchemeTypeId::from_raw(node.first).unwrap();
                    let child = &mut changed.store.scheme_types[child.index()];
                    assert_eq!(child.tag, wrong_child.tag);
                    child.first = wrong_child.first;
                } else { node.first = wrong.first; }
            }
        }
        changed
    }

    #[test]
    fn generic_construction_interpolation_requires_the_actual_display_domain() {
        for prefix in ["f", "fp"] {
            let program = prepare(&format!("pure keep(value, text: Str, wrong: Regex) {{ let _ = {prefix}\"{{text}}\"; value }}"));
            let (owner, scheme) = generic_owner(&program);
            let decoder = decoder_for_function(&program.store, owner).unwrap();
            let operand = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
                Ok(schema::ExprView::FmtString(mut parts)) | Ok(schema::ExprView::PathFmtString { mut parts, .. }) => match parts.next().unwrap().unwrap() {
                    super::super::super::decode::FmtPartView::Expr(value, _, _) => Some(value.raw() as usize),
                    _ => None,
                },
                _ => None,
            }).unwrap();
            let changed = retarget_construction_operand(&program, owner, scheme, operand, false);
            assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic interpolation producer cannot be displayed");
        }
    }

    #[test]
    fn generic_construction_splices_preserve_the_actual_payload_domain() {
        prepare("pure keep(value, items: NonEmpty[Int]) { let _ = [@items]; value }");
        let program = prepare("pure keep(value, items: List[Int], wrong: List[Str]) { let _ = [@items, 1]; value }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let operand = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::ListBuild(mut entries)) => { let (splice, value, _) = entries.next().unwrap().unwrap(); assert!(splice); Some(value.raw() as usize) }
            _ => None,
        }).unwrap();
        let changed = retarget_construction_operand(&program, owner, scheme, operand, true);
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic list splice disagrees with its item contract");

        let program = prepare("pure keep(value, unsigned: UInt) { let _ = {-1, 2}; value }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let method = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Method { name: "to_set", .. }))).unwrap();
        let (declaration, slot) = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Let { slot, value, .. }) if value.raw() as usize == method => Some((instruction, slot)),
            _ => None,
        }).unwrap();
        let read = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(actual)) if actual == slot)).unwrap();
        let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        let unsigned = program.store.scheme_types[program.store.scheme_params[parameters.start + 1].ty.index()];
        let mut changed = program.clone();
        for fact in &changed.store.scheme_body_facts {
            if [method, declaration, read].contains(&(fact.instruction as usize)) {
                let node = changed.store.scheme_types[fact.ty.index()];
                let child = SchemeTypeId::from_raw(node.first).unwrap();
                let child = &mut changed.store.scheme_types[child.index()];
                assert_eq!(child.tag, unsigned.tag);
                child.first = unsigned.first;
            }
        }
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic host method disagrees with its selected registry signature");
    }

    #[test]
    fn generic_construction_maps_require_the_actual_key_domain() {
        let program = prepare("pure keep(value, key: Int, wrong: Str) { let _ = {[key]: value}; value }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let operand = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::MapLiteral(mut entries)) => Some(entries.next().unwrap().unwrap().0.unwrap().raw() as usize),
            _ => None,
        }).unwrap();
        let changed = retarget_construction_operand(&program, owner, scheme, operand, false);
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic map entry disagrees with its checked domain");
    }

    #[test]
    fn generic_construction_record_spreads_prove_actual_fields() {
        let program = prepare("pure keep(value) { let _ = {other: 1}; {...{item: value}} }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let (block, record) = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::Record(mut entries)) => match entries.next().unwrap().unwrap() {
                super::super::super::decode::RecordEntryView::Field(name, _) if name.as_str().as_str() == "item" => Some((entries.block_identity_for_test(), instruction)),
                _ => None,
            },
            _ => None,
        }).unwrap();
        let payload = program.store.blocks[block.index()].instructions.bounds(program.store.extra.len()).unwrap();
        let mut changed = program.clone();
        assert_eq!(changed.store.extra[payload.start], 1);
        assert_eq!(changed.store.extra[payload.start + 1], 0);
        changed.store.extra[payload.start + 2] = program.symbol_owner().with_current(|| Name::intern("other").symbol().raw());
        let (declaration, slot) = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Let { slot, value, .. }) if value.raw() as usize == record => Some((instruction, slot)),
            _ => None,
        }).unwrap();
        let read = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(actual)) if actual == slot)).unwrap();
        let other = program.store.scheme_fields.iter().find(|field| program.store.string(field.name).unwrap() == "other").unwrap().name;
        for fact in &changed.store.scheme_body_facts {
            if [record, declaration, read].contains(&(fact.instruction as usize)) {
                let node = changed.store.scheme_types[fact.ty.index()];
                let fields = node.children.bounds(changed.store.scheme_fields.len()).unwrap();
                assert_eq!(fields.len(), 1);
                assert_eq!(changed.store.string(changed.store.scheme_fields[fields.start].name).unwrap(), "item");
                changed.store.scheme_fields[fields.start].name = other;
            }
        }
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic body expression disagrees with its checked type");
    }

    fn record_operand_binding(program: &FullProgram, owner: usize, read: usize) -> (usize, usize) {
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let schema::ExprView::Param(slot) = decoder.expression_view(read).unwrap() else { panic!("the operation reads an ANF binding") };
        program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Let { slot: actual, value, .. }) if actual == slot => Some((instruction, value.raw() as usize)),
            _ => None,
        }).unwrap()
    }

    fn retarget_record_operand(program: &FullProgram, owner: usize, scheme: FullSchemeId, read: usize, from: usize, to: usize) -> FullProgram {
        let (declaration, initializer) = record_operand_binding(program, owner, read);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        assert!(matches!(decoder.expression_view(initializer).unwrap(), schema::ExprView::Param(slot) if slot == from));
        let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        let actual = program.store.scheme_types[program.store.scheme_params[parameters.start + to].ty.index()];
        assert_eq!(actual.children.len, 0);
        let mut changed = program.clone();
        let payload = changed.store.data[initializer].range().bounds(changed.store.extra.len()).unwrap();
        assert_eq!(payload.len(), 1);
        changed.store.extra[payload.start] = to as u32;
        for fact in &changed.store.scheme_body_facts {
            if [read, declaration, initializer].contains(&(fact.instruction as usize)) {
                let node = &mut changed.store.scheme_types[fact.ty.index()];
                assert_eq!(node.tag, actual.tag);
                assert_eq!(node.children.len, 0);
                node.first = actual.first;
            }
        }
        changed
    }

    #[test]
    fn generic_record_forms_shorthand_authenticates_binding_and_scope() {
        let program = prepare("pure copy(value, other) { {value} }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let read = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::Record(mut entries)) => match entries.next().unwrap().unwrap() { super::super::super::decode::RecordEntryView::Field(_, value) => Some(value.raw() as usize), _ => None },
            _ => None,
        }).unwrap();
        let changed = retarget_record_operand(&program, owner, scheme, read, 0, 1);
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic body expression disagrees with its checked type");

        let program = prepare("pure copy(value) { let item = value; { let item = 1; let _ = {item} }; {item} }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let inner = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Let { slot, value, .. }) if matches!(decoder.expression_view(value.raw() as usize), Ok(schema::ExprView::Int(1))) => Some(slot),
            _ => None,
        }).unwrap();
        let read = range.filter_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::Record(mut entries)) => match entries.next().unwrap().unwrap() { super::super::super::decode::RecordEntryView::Field(_, value) => Some(value.raw() as usize), _ => None },
            _ => None,
        }).last().unwrap();
        let (_, initializer) = record_operand_binding(&program, owner, read);
        let mut changed = program.clone();
        let payload = changed.store.data[initializer].range().bounds(changed.store.extra.len()).unwrap();
        assert_eq!(payload.len(), 1);
        changed.store.extra[payload.start] = inner as u32;
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic body reads an uninitialized slot");

        let program = prepare("pure keep(value, items: Set[Int], wrong: Set[Str]) { let stored: Set[Int] = items; let other: Set[Str] = wrong; let _ = stored; let _ = other; value }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let input = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::CheckedValue { value, .. }) => Some(value.raw() as usize),
            _ => None,
        }).unwrap();
        let changed = retarget_construction_operand(&program, owner, scheme, input, true);
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic checked value is not a complete storage contract");
        let range = program.store.function_instruction_range(owner).unwrap();
        let (check_instruction, other_check, expected) = {
            let mut checks = range.clone().filter_map(|instruction| match decoder.expression_view(instruction) {
                Ok(schema::ExprView::CheckedValue { check, .. }) => Some((instruction, check.ty.id)),
                _ => None,
            });
            let (first, _) = checks.next().unwrap();
            let (second, expected) = checks.next().unwrap();
            assert!(checks.next().is_none());
            (first, second, expected)
        };
        let mut output = BTreeSet::from([check_instruction]);
        let mut slots = BTreeSet::new();
        loop {
            let previous = output.len();
            for instruction in range.clone() {
                match decoder.statement_view(instruction) {
                    Ok(schema::StmtView::Let { slot, value, .. }) if output.contains(&(value.raw() as usize)) => { output.insert(instruction); slots.insert(slot); }
                    _ => {}
                }
                if matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(slot)) if slots.contains(&slot)) { output.insert(instruction); }
            }
            if previous == output.len() { break; }
        }
        let mut target = program.clone();
        let payload = target.store.data[check_instruction].range().bounds(target.store.extra.len()).unwrap();
        let other_payload = target.store.data[other_check].range().bounds(target.store.extra.len()).unwrap();
        target.store.extra[payload.start + 1] = expected.raw();
        target.store.extra[payload.start + 2] = program.store.extra[other_payload.start + 2];
        for instruction in range.clone().filter(|instruction| output.contains(instruction)) {
            if let Ok(schema::StmtView::Let { slot, annotation: Some(_), .. }) = decoder.statement_view(instruction) {
                let payload = target.store.data[instruction].range().bounds(target.store.extra.len()).unwrap();
                assert_eq!(target.store.extra[payload.start + 2], 1);
                target.store.extra[payload.start + 3] = expected.raw();
                let facts = target.store.functions[owner].slot_type_facts.bounds(target.store.slot_type_facts.len()).unwrap();
                if target.store.slot_type_facts[facts.start + slot] != IR_NONE { target.store.slot_type_facts[facts.start + slot] = expected.raw(); }
            }
        }
        let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        let expected_item = program.store.scheme_types[program.store.scheme_params[parameters.start + 2].ty.index()].first;
        let expected_item = program.store.scheme_types[SchemeTypeId::from_raw(expected_item).unwrap().index()];
        for fact in &target.store.scheme_body_facts {
            if output.contains(&(fact.instruction as usize)) {
                let node = target.store.scheme_types[fact.ty.index()];
                match node.tag {
                    FullSchemeTypeTag::Ground => target.store.scheme_types[fact.ty.index()].first = expected.raw(),
                    FullSchemeTypeTag::Set => {
                        let child = SchemeTypeId::from_raw(node.first).unwrap();
                        let item = &mut target.store.scheme_types[child.index()];
                        assert_eq!(item.tag, expected_item.tag);
                        item.first = expected_item.first;
                    }
                    _ => panic!("unexpected checked Set output type"),
                }
            }
        }
        assert_eq!(FullVerifier::verify(&target).unwrap_err().message, "generic checked value is not a complete storage contract");

    }

    #[test]
    fn generic_record_forms_updates_authenticate_paths_and_replacements() {
        let program = prepare("type Inner = {left: Int, right: Int}\ntype Row = {nested: Inner}\npure keep(value, row: Row, replacement: Int, wrong: Str) { let _ = {...row, nested.left: replacement, nested.right: replacement}; value }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let (first, second, read) = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::RecordUpdate { mut updates, .. }) => {
                let (first, value, _) = updates.next().unwrap().unwrap();
                let (second, _, _) = updates.next().unwrap().unwrap();
                Some((first.block_identity_for_test(), second.block_identity_for_test(), value.raw() as usize))
            }
            _ => None,
        }).unwrap();
        let first = program.store.blocks[first.index()].instructions.bounds(program.store.extra.len()).unwrap();
        let second = program.store.blocks[second.index()].instructions.bounds(program.store.extra.len()).unwrap();
        assert_eq!(program.store.extra[first.start], 2);
        assert_eq!(program.store.extra[second.start], 2);
        let mut overlap = program.clone();
        overlap.store.extra[second.start + 2] = overlap.store.extra[first.start + 2];
        assert_eq!(FullVerifier::verify(&overlap).unwrap_err().message, "record update paths must be nonempty and disjoint");
        let mut missing = program.clone();
        missing.store.extra[first.start + 2] = program.symbol_owner().with_current(|| Name::intern("wrong").symbol().raw());
        assert_eq!(FullVerifier::verify(&missing).unwrap_err().message, "generic record update target has no checked field");
        let changed = retarget_record_operand(&program, owner, scheme, read, 2, 3);
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic record update replacement disagrees with its field");
    }

    fn record_field_type(store: &FullStore, ty: SchemeTypeId, name: &str) -> SchemeTypeId {
        let node = store.scheme_types[ty.index()];
        let range = node.children.bounds(store.scheme_fields.len()).unwrap();
        store.scheme_fields[range].iter().find(|field| store.string(field.name).unwrap() == name).unwrap().ty
    }

    #[test]
    fn generic_record_forms_updates_preserve_unsigned_guarantees_and_work_bounds() {
        let program = prepare("type SignedInner = {count: Int}\ntype UnsignedInner = {count: UInt}\ntype Signed = {nested: SignedInner}\ntype Unsigned = {nested: UnsignedInner}\npure keep(value, row: Signed, unsigned: Unsigned, replacement: Int) { let _ = {...row, nested.count: replacement}; value }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let read = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::RecordUpdate { base, .. }) => Some(base.raw() as usize),
            _ => None,
        }).unwrap();
        let (declaration, initializer) = record_operand_binding(&program, owner, read);
        assert!(matches!(decoder.expression_view(initializer).unwrap(), schema::ExprView::Param(1)));
        let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        let actual = program.store.scheme_params[parameters.start + 2].ty;
        let unsigned = record_field_type(&program.store, record_field_type(&program.store, actual, "nested"), "count");
        let unsigned = program.store.scheme_types[unsigned.index()];
        let mut changed = program.clone();
        let payload = changed.store.data[initializer].range().bounds(changed.store.extra.len()).unwrap();
        assert_eq!(payload.len(), 1);
        changed.store.extra[payload.start] = 2;
        for fact in &changed.store.scheme_body_facts {
            if [read, declaration, initializer].contains(&(fact.instruction as usize)) {
                let leaf = record_field_type(&changed.store, record_field_type(&changed.store, fact.ty, "nested"), "count");
                let node = &mut changed.store.scheme_types[leaf.index()];
                assert_eq!(node.tag, unsigned.tag);
                node.first = unsigned.first;
            }
        }
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic record update replacement disagrees with its field");
        for width in [256, 512] {
            let fields = (0..width).map(|index| format!("f{index}: Int")).collect::<Vec<_>>().join(",");
            let updates = (0..width).map(|index| format!("nested.f{index}: 0")).collect::<Vec<_>>().join(",");
            prepare(&format!("type Inner = {{{fields}}}\ntype Row = {{nested: Inner}}\npure keep(value, row: Row) {{ let _ = {{...row, {updates}}}; value }}"));
        }
    }

    #[test]
    fn generic_operators_logical_contracts_reject_conflicting_operands_and_results() {
        let program = prepare("pure keep(value, flag: Bool, wrong: Int) { let _ = flag and flag; value }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let (binary, right) = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::Binary { op: crate::syntax::node::BinaryOp::And, right, .. }) => Some((instruction, right.raw() as usize)),
            _ => None,
        }).unwrap();
        let changed = retarget_construction_operand(&program, owner, scheme, right, false);
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic logical operand has no Boolean contract");
        let mut operation = program.clone();
        let payload = operation.store.data[binary].range().bounds(operation.store.extra.len()).unwrap();
        let selected = operation.store.extra[payload.start] as usize;
        operation.store.binary_ops[selected] = crate::syntax::node::BinaryOp::Add;
        assert_eq!(FullVerifier::verify(&operation).unwrap_err().message, "generic binary operands disagree with the operation");
        let mut output = BTreeSet::from([binary]);
        let mut slots = BTreeSet::new();
        loop {
            let previous = output.len();
            for instruction in range.clone() {
                if let Ok(schema::StmtView::Let { slot, value, .. }) = decoder.statement_view(instruction)
                    && output.contains(&(value.raw() as usize))
                { output.insert(instruction); slots.insert(slot); }
                if matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(slot)) if slots.contains(&slot)) { output.insert(instruction); }
            }
            if output.len() == previous { break; }
        }
        let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        let flag = program.store.scheme_params[parameters.start + 1].ty;
        let integer = program.store.scheme_types[program.store.scheme_params[parameters.start + 2].ty.index()].first;
        let mut result = program.clone();
        for fact in &result.store.scheme_body_facts {
            if output.contains(&(fact.instruction as usize)) {
                assert_ne!(fact.ty, flag);
                let node = &mut result.store.scheme_types[fact.ty.index()];
                assert_eq!(node.tag, FullSchemeTypeTag::Ground);
                node.first = integer;
            }
        }
        let facts = result.store.functions[owner].slot_type_facts.bounds(result.store.slot_type_facts.len()).unwrap();
        for slot in slots {
            if result.store.slot_type_facts[facts.start + slot] != IR_NONE { result.store.slot_type_facts[facts.start + slot] = integer; }
        }
        assert_eq!(result.store.scheme_types[flag.index()], program.store.scheme_types[flag.index()]);
        assert_eq!(FullVerifier::verify(&result).unwrap_err().message, "generic body expression disagrees with its checked type");
    }

    #[test]
    fn generic_operators_membership_and_sets_preserve_actual_domains() {
        let program = prepare("pure keep(value, other) { let _ = value in [value]; value }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let left = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::Binary { op: crate::syntax::node::BinaryOp::In, left, .. }) => Some(left.raw() as usize),
            _ => None,
        }).unwrap();
        let changed = retarget_record_operand(&program, owner, scheme, left, 0, 1);
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic membership operands have incompatible domains");
        for operator in ["|", "&", "-"] {
            let program = prepare(&format!("pure keep(value, items: Set[UInt], wrong: Set[Int]) {{ let _ = items {operator} items; value }}"));
            let (owner, scheme) = generic_owner(&program);
            let decoder = decoder_for_function(&program.store, owner).unwrap();
            let right = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
                Ok(schema::ExprView::Binary { right, .. }) => Some(right.raw() as usize),
                _ => None,
            }).unwrap();
            let changed = retarget_construction_operand(&program, owner, scheme, right, true);
            assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic set operands have incompatible domains");
        }
    }

    #[test]
    fn generic_operators_chain_links_authenticate_unreached_operand_domains() {
        let program = prepare("pure keep(value, number: Int, wrong: Str) { let _ = 0 <= number < number; value }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let ignored = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::ComparisonChain { mut pairs, .. }) => {
                pairs.next().unwrap().unwrap();
                let (_, right, _) = pairs.next().unwrap().unwrap();
                Some(right.raw() as usize)
            }
            _ => None,
        }).unwrap();
        let changed = retarget_construction_operand(&program, owner, scheme, ignored, false);
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic ordering operands have incompatible domains");
    }

    #[test]
    fn generic_operators_chain_completion_preserves_a_skipped_return() {
        let program = Arc::new(prepare("pure choose(number: Int, wrong: Str) -> Int { let _ = 0 < number < { return 7 }; 0 }"));
        let owner = 0;
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let chain = program.store.function_instruction_range(owner).unwrap().find(|instruction| program.store.tags[*instruction] == super::super::super::FullTag::ExprComparisonChain).unwrap();
        assert!(super::super::super::indexed_expr_can_complete(&decoder, chain, 0).unwrap());
        let tail = program.store.function_instruction_range(owner).unwrap().filter(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Int(0)))).last().unwrap();
        let mut wrong_tail = (*program).clone();
        wrong_tail.store.tags[tail] = super::super::super::FullTag::ExprParam;
        let words = wrong_tail.store.data[tail].range().bounds(wrong_tail.store.extra.len()).unwrap();
        wrong_tail.store.extra[words.start] = 1;
        assert!(FullVerifier::verify(&wrong_tail).is_err());
        for force_frames in [false, true] {
            let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| crate::runtime::eval::LoweredFunctionKey::Name(Name::intern("choose")));
            for (number, expected) in [(-1, 0), (1, 7)] {
                let actual = evaluator.call_indexed_direct(key, crate::runtime::eval::LoweredFunctionKind::Pure, &[crate::runtime::value::Value::Int(number), crate::runtime::value::Value::Str(Arc::from("wrong"))], crate::source::Span::at(program.store.source_id, 0)).unwrap().unwrap();
                assert_eq!(actual, crate::runtime::value::Value::Int(expected), "frames={force_frames}");
            }
        }
    }

    #[test]
    fn generic_operators_chain_completion_keeps_required_operands() {
        for definition in [
            "pure choose() -> Int { let _ = 0 < 1 < { return 7 }; 0 }",
            "pure choose() -> Int { let _ = { return 7 } < 1 < 2; 0 }",
            "pure choose() -> Int { let _ = 0 < { return 7 } < 2; 0 }",
        ] {
            let program = prepare(definition);
            let decoder = decoder_for_function(&program.store, 0).unwrap();
            let chain = program.store.function_instruction_range(0).unwrap().find(|instruction| program.store.tags[*instruction] == super::super::super::FullTag::ExprComparisonChain).unwrap();
            assert!(!super::super::super::indexed_expr_can_complete(&decoder, chain, 0).unwrap(), "{definition}");
        }
    }

    fn nested_chain_expression(depth: usize) -> String {
        let mut expression = "1".to_string();
        for _ in 0..depth { expression = format!("0 < {{ let _ = {expression}; 1 }} < 2"); }
        expression
    }

    #[test]
    fn generic_operators_nested_chains_keep_physical_growth_bounded() {
        let source = |depth| format!("pure keep(value) {{ let _ = {}; value }}", nested_chain_expression(depth));
        let shallow = prepare(&source(8));
        let deep = prepare(&source(12));
        eprintln!("generic nested chain depth 8/12 rows: {}/{}", shallow.instruction_count(), deep.instruction_count());
        assert!(deep.instruction_count() <= shallow.instruction_count() * 2, "nested chain rows grew from {} to {}", shallow.instruction_count(), deep.instruction_count());
    }

    #[test]
    fn generic_operators_ordinary_nested_chains_keep_physical_growth_bounded() {
        let source = |depth| format!("pure keep(value: Int) -> Int {{ let _ = {}; value }}", nested_chain_expression(depth));
        let shallow = prepare(&source(8));
        let deep = prepare(&source(12));
        eprintln!("ordinary nested chain depth 8/12 rows: {}/{}", shallow.instruction_count(), deep.instruction_count());
        assert!(deep.instruction_count() <= shallow.instruction_count() * 2, "ordinary nested chain rows grew from {} to {}", shallow.instruction_count(), deep.instruction_count());
    }

    #[test]
    fn generic_iteration_record_targets_authenticate_actual_field_domains() {
        let program = prepare("pure keep(value, other) { let {item: copy, ..} = {item: value, other}; let _ = copy; value }");
        let (owner, _) = generic_owner(&program);
        let declaration = program.store.function_instruction_range(owner).unwrap().find(|instruction| program.store.tags[*instruction] == super::super::super::FullTag::StmtLetRecord && program.store.scheme_body_facts.iter().any(|fact| fact.instruction as usize == *instruction && fact.kind == FullSchemeBodyFactKind::LocalImmutable)).unwrap();
        let mut changed = program.clone();
        let words = changed.store.data[declaration].range().bounds(changed.store.extra.len()).unwrap();
        assert_eq!(changed.store.extra[words.start + 1], 1);
        assert_eq!(changed.store.extra[words.start + 2], 1);
        changed.store.extra[words.start + 3] = program.symbol_owner().with_current(|| Name::intern("other").symbol().raw());
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic body expression disagrees with its checked type");
    }

    #[test]
    fn generic_iteration_record_declarations_require_unique_complete_evidence() {
        let program = prepare("pure keep(value) { let {item: copy} = {item: value}; copy }");
        let fact = program.store.scheme_body_facts.iter().position(|fact| fact.kind == FullSchemeBodyFactKind::LocalImmutable && program.store.tags[fact.instruction as usize] == super::super::super::FullTag::StmtLetRecord).unwrap();
        let mut missing = program.clone();
        missing.store.scheme_body_facts[fact].kind = FullSchemeBodyFactKind::Expression;
        assert_eq!(FullVerifier::verify(&missing).unwrap_err().message, "generic body fact has an invalid instruction kind");
        let duplicate = program.store.scheme_body_facts.iter().position(|other| other.kind == FullSchemeBodyFactKind::LocalImmutable && other.instruction != program.store.scheme_body_facts[fact].instruction).unwrap();
        let mut changed = program.clone();
        changed.store.scheme_body_facts[duplicate].instruction = changed.store.scheme_body_facts[fact].instruction;
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic local declaration fact is duplicated");
    }

    #[test]
    fn generic_iteration_record_mutability_and_all_writers_are_authenticated() {
        let program = prepare("pure keep(value, other) { var {item: copy} = {item: value}; copy = value; value }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let writer = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Assign { value, .. }) => Some(value.raw() as usize), _ => None,
        }).unwrap();
        let changed = retarget_record_operand(&program, owner, scheme, writer, 0, 1);
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic assignment disagrees with its invariant slot contract");
        let mut immutable = program.clone();
        let declaration = immutable.store.scheme_body_facts.iter_mut().find(|fact| fact.kind == FullSchemeBodyFactKind::LocalMutable).unwrap();
        declaration.kind = FullSchemeBodyFactKind::LocalImmutable;
        assert_eq!(FullVerifier::verify(&immutable).unwrap_err().message, "generic assignment does not target a mutable declaration");
    }

    #[test]
    fn generic_iteration_loop_bindings_are_immutable_and_lexically_owned() {
        let program = prepare("pure keep(value) { var result = value; for item in [value] { result = item }; value }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let slot = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::ForRecord { target: super::super::super::decode::CompTargetView::Slot(slot), .. }) => Some(slot), _ => None,
        }).unwrap();
        let writer = range.clone().find(|instruction| matches!(decoder.statement_view(*instruction), Ok(schema::StmtView::Assign { .. }))).unwrap();
        let mut assigned = program.clone();
        let words = assigned.store.data[writer].range().bounds(assigned.store.extra.len()).unwrap();
        assigned.store.extra[words.start] = slot as u32;
        assert_eq!(FullVerifier::verify(&assigned).unwrap_err().message, "generic assignment does not target a mutable declaration");
        let tail = range.filter_map(|instruction| match decoder.statement_view(instruction) { Ok(schema::StmtView::Return { value }) => Some(value.raw() as usize), _ => None }).last().unwrap();
        assert!(matches!(decoder.expression_view(tail).unwrap(), schema::ExprView::Param(0)));
        let mut escaped = program.clone();
        let words = escaped.store.data[tail].range().bounds(escaped.store.extra.len()).unwrap();
        escaped.store.extra[words.start] = slot as u32;
        assert_eq!(FullVerifier::verify(&escaped).unwrap_err().message, "generic body reads an uninitialized slot");
    }

    #[test]
    fn generic_iteration_actual_item_domains_authenticate_the_loop_binder() {
        let program = prepare("pure keep(value, other) { let rows = [value]; let spare = [other]; var result = value; for item in rows { result = item }; result }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let iter = range.clone().find_map(|instruction| match decoder.statement_view(instruction) { Ok(schema::StmtView::ForRecord { iter, .. }) => Some(iter.raw() as usize), _ => None }).unwrap();
        let (declaration, initializer) = record_operand_binding(&program, owner, iter);
        let schema::ExprView::Param(rows_slot) = decoder.expression_view(initializer).unwrap() else { panic!("the loop source reads its declared list") };
        let (spare_slot, spare_type) = range.clone().filter_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Let { slot, value, .. }) if slot != rows_slot => {
                let fact = program.store.scheme_body_facts.iter().find(|fact| fact.instruction as usize == value.raw() as usize)?;
                let ty = program.store.scheme_types[fact.ty.index()];
                (ty.tag == FullSchemeTypeTag::List).then_some((slot, ty))
            }
            _ => None,
        }).find(|(_, ty)| {
            let child = program.store.scheme_types[SchemeTypeId::from_raw(ty.first).unwrap().index()];
            child.tag == FullSchemeTypeTag::Parameter && child.first == 1
        }).unwrap();
        let actual_item = program.store.scheme_types[SchemeTypeId::from_raw(spare_type.first).unwrap().index()];
        let mut changed = program.clone();
        let words = changed.store.data[initializer].range().bounds(changed.store.extra.len()).unwrap();
        changed.store.extra[words.start] = spare_slot as u32;
        for fact in &changed.store.scheme_body_facts {
            if [declaration, initializer, iter].contains(&(fact.instruction as usize)) {
                let list = changed.store.scheme_types[fact.ty.index()];
                assert_eq!(list.tag, FullSchemeTypeTag::List);
                let item = &mut changed.store.scheme_types[SchemeTypeId::from_raw(list.first).unwrap().index()];
                assert_eq!(item.tag, FullSchemeTypeTag::Parameter);
                item.first = actual_item.first;
            }
        }
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic body expression disagrees with its checked type");
    }

    #[test]
    fn generic_iteration_stream_error_sources_must_fit_the_owner_boundary() {
        let mut program = prepare("error First = Missing\nstream safe() [] -> Stream[Int] { yield 1 }\nstream failing() [error] -> Stream[Int] { yield [1][9] }\nproc keep(marker, result: Result[Int, First]) [error] -> Result[Int, First] { for item in safe() { let _ = item }; result }");
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        let (owner, scheme_id) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let call = program.store.function_instruction_range(owner).unwrap().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Call { .. } | schema::ExprView::DirectPureCall { .. }))).unwrap();
        let failing = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "failing").unwrap();
        let mut changed = program.clone();
        let words = changed.store.data[call].range().bounds(changed.store.extra.len()).unwrap();
        changed.store.extra[words.start] = IrFunctionId::new(failing).unwrap().raw();
        let selected = changed.store.checked_call_targets.iter_mut().find(|target| target.instruction as usize == call).unwrap();
        selected.function = IrFunctionId::new(failing).unwrap();
        let changed_decoder = decoder_for_function(&changed.store, owner).unwrap();
        let (schema::ExprView::Call { function, .. } | schema::ExprView::DirectPureCall { function, .. }) = changed_decoder.expression_view(call).unwrap() else { panic!("the source retains its selected call") };
        assert_eq!(function.id.index(), failing);
        assert_eq!(changed.store.checked_call_targets.iter().find(|target| target.instruction as usize == call).unwrap().function.index(), failing);
        let effects = super::super::super::verify_effects::function_effects(&changed.store, failing).unwrap().unwrap();
        assert!(effects.contains(&crate::syntax::node::Effect::Error));
        let scopes = verify_function_schemes(&program.store).unwrap();
        let scope = scopes[scheme_id.index()].as_ref().unwrap();
        let scheme = program.store.schemes[scheme_id.index()];
        let mut budget = SchemeInstantiationBudget::new(&program.store);
        let owner_return = decode_scheme_type(&program.store, scope, scheme.return_type, &mut budget, 0).unwrap();
        let actual_error = match &owner_return {
            SchemeType::Result(_, error) => error.closed_ground_type().unwrap(),
            SchemeType::Ground(Type::Result(_, error)) => (**error).clone(),
            _ => panic!("the actual owner has a result boundary: {owner_return:?}"),
        };
        let params = scheme.params.bounds(program.store.scheme_params.len()).unwrap();
        let declared_result = decode_scheme_type(&program.store, scope, program.store.scheme_params[params.start + 1].ty, &mut budget, 0).unwrap().closed_ground_type().unwrap();
        let Type::Result(_, first) = declared_result else { panic!("the source formal retains its declared First boundary") };
        assert_eq!(actual_error, *first, "actual owner: {owner_return:?}, changed target effects: {effects:?}");
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic propagated error disagrees with its owner boundary");
    }

    #[test]
    fn generic_iteration_result_source_errors_must_fit_the_owner_boundary() {
        let program = prepare("error First = Missing\nerror Second = Missing\npure keep(marker, rows: Result[List[Int], First], other: Result[List[Int], Second]) -> Result[Int, First] { for item in rows { let _ = item }; Ok(7) }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let iter = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::ForRecord { iter, .. }) => Some(iter.raw() as usize), _ => None,
        }).unwrap();
        let (declaration, initializer, read) = guarded_receiver_source(&program, owner, iter);
        let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        let wrong = program.store.scheme_types[program.store.scheme_params[parameters.start + 2].ty.index()];
        assert_eq!(wrong.tag, FullSchemeTypeTag::Result);
        let error = program.store.scheme_types[SchemeTypeId::from_raw(wrong.second).unwrap().index()];
        let mut changed = program.clone();
        let payload = changed.store.data[initializer].range().bounds(changed.store.extra.len()).unwrap();
        assert_eq!(payload.len(), 1);
        changed.store.extra[payload.start] = 2;
        for fact in &changed.store.scheme_body_facts {
            if [declaration, initializer, read].contains(&(fact.instruction as usize)) {
                let result = changed.store.scheme_types[fact.ty.index()];
                assert_eq!(result.tag, FullSchemeTypeTag::Result);
                let actual_error = &mut changed.store.scheme_types[SchemeTypeId::from_raw(result.second).unwrap().index()];
                assert_eq!(actual_error.tag, error.tag);
                actual_error.first = error.first;
                actual_error.second = error.second;
            }
        }
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic propagated error disagrees with its owner boundary");
    }

    #[test]
    fn generic_iteration_map_arguments_preserve_actual_key_and_value_domains() {
        for (parameters, literal, from, to) in [
            ("key: Str, wrong: Int", "{[key]: 1}", 0, 1),
            ("number: Int, wrong: Str", "{[\"key\"]: number}", 0, 1),
        ] {
            let program = prepare(&format!("pure identity(value) {{ value }}\npure selected({parameters}) -> Map[Str, Int] {{ identity({literal}) }}"));
            let owner = program.store.functions.iter().enumerate().find(|(_, function)| program.store.string(function.name).unwrap() == "selected").map(|(owner, _)| owner).unwrap();
            let decoder = decoder_for_function(&program.store, owner).unwrap();
            let operand = program.store.function_instruction_range(owner).unwrap().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(slot)) if slot == from)).unwrap();
            let mut changed = program.clone();
            let words = changed.store.data[operand].range().bounds(changed.store.extra.len()).unwrap();
            changed.store.extra[words.start] = to as u32;
            changed.store.checked_expr_types.retain(|fact| fact.instruction as usize != operand);
            assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic call argument does not match its checked parameter slot");
        }
        prepare("pure identity(value) { value }\npure selected(rows: Map[Str, Int]) -> Map[Str, Int] { identity({[\"first\"]: 1, ...rows, [\"last\"]: 2}) }");
    }

}
