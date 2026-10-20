use super::*;

#[derive(Clone, Copy)]
pub(super) struct LexicalScope {
    frame: usize,
    loop_depth: usize,
}

struct LocalBinding {
    ty: SchemeType,
    mutable: bool,
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

    pub(super) fn slot_type<'a>(&self, scopes: &'a LexicalScopes, slot: usize, budget: &mut SchemeInstantiationBudget) -> Result<Option<&'a SchemeType>, IrVerifyError> {
        Ok(self.binding(scopes, slot, budget)?.map(|binding| &binding.ty))
    }
}

enum Obligation {
    Expression(usize, LexicalScope),
    Publish { statements: Arc<[usize]>, position: usize, scope: LexicalScope, value: Option<SchemeType>, slot: usize, ty: SchemeType, mutable: bool },
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

fn declare(scope: &LexicalScope, scopes: &mut LexicalScopes, slot: usize, ty: SchemeType, mutable: bool, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    if scope.binding(scopes, slot, budget)?.is_some() { return Err(IrVerifyError::new("generic declaration rewrites a visible slot")); }
    budget.charge(1)?;
    scopes.frames[scope.frame].bindings.insert(slot, LocalBinding { ty, mutable });
    Ok(())
}

fn condition(types: &BTreeMap<usize, SchemeType>, instruction: usize, role: crate::sema::types::ConditionRole, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    let ty = expression_type(types, instruction, budget)?.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic condition is not a ground truth value"))?;
    if !role.accepts(&ty) { return Err(IrVerifyError::new("generic condition disagrees with its control role")); }
    Ok(())
}

fn project(ty: &SchemeType, projection: super::super::CheckedValueProjection) -> Result<Option<SchemeType>, IrVerifyError> {
    use super::super::CheckedValueProjection as P;
    let symbolic = match (ty, projection) {
        (SchemeType::Record { fields, .. }, P::Field(name)) => fields.get(&name).cloned(),
        (SchemeType::List(item) | SchemeType::Stream(item) | SchemeType::Set(item), P::Element | P::IterationItem) => Some((**item).clone()),
        (SchemeType::Map(_, value), P::Element | P::IterationItem) => Some((**value).clone()),
        (SchemeType::Result(ok, _), P::ResultPayload(true)) => Some((**ok).clone()),
        (SchemeType::Result(_, error), P::ResultPayload(false)) => Some((**error).clone()),
        (SchemeType::Ground(ty), projection) => match (ty, projection) {
            (Type::Record(fields), P::Field(name)) => fields.get(&name).cloned().map(SchemeType::Ground),
            (Type::Result(ok, _), P::ResultPayload(true)) => Some(SchemeType::Ground((**ok).clone())),
            (Type::Result(_, error), P::ResultPayload(false)) => Some(SchemeType::Ground((**error).clone())),
            (ty, P::Element | P::IterationItem) => ty.iteration_item_type().map(SchemeType::Ground),
            _ => None,
        },
        _ => None,
    };
    Ok(symbolic)
}

fn pattern_scope(decoder: &FullDecoder<'_>, pattern: u32, subject: &SchemeType, scope: &LexicalScope, scopes: &mut LexicalScopes, slot_count: usize, writers: &mut BTreeMap<usize, SchemeType>, budget: &mut SchemeInstantiationBudget) -> Result<LexicalScope, IrVerifyError> {
    let mut bindings = BTreeMap::new();
    let ground = subject.closed_ground_type();
    let work = RefCell::new(budget);
    decoder.visit_pattern_evidence(pattern, ground.as_ref(), Some(subject.clone()),
        |ty, step| {
            let Some(ty) = ty else { return Ok(None) };
            work.borrow_mut().charge_scheme_clone(ty)?;
            project(ty, step)
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
        declare(&result, scopes, slot, ty, false, &mut work.borrow_mut())?;
    }
    Ok(result)
}

pub(super) fn verify_body(
    store: &FullStore, decoder: &FullDecoder<'_>, types: &BTreeMap<usize, SchemeType>,
    tails: &mut BTreeMap<usize, SchemeType>, declarations: &mut BTreeMap<usize, (SchemeType, bool)>,
    slots: BTreeMap<usize, SchemeType>, expected_return: &SchemeType, statements: Vec<usize>,
    analysis: &super::super::IndexedReturnAnalysis, registered: &mut BTreeSet<u32>, budget: &mut SchemeInstantiationBudget,
) -> Result<(BTreeSet<usize>, BTreeMap<usize, SchemeType>, BTreeSet<usize>), IrVerifyError> {
    let slot_count = decoder.slot_count as usize;
    for ty in slots.values() { budget.charge_scheme_clone(ty)?; }
    let mut writers = slots.clone();
    let mut scopes = LexicalScopes { frames: vec![LexicalFrame { parent: None, bindings: slots.into_iter().map(|(slot, ty)| (slot, LocalBinding { ty, mutable: false })).collect() }] };
    let scope = LexicalScope { frame: 0, loop_depth: 0 };
    let mut pending = vec![Obligation::Block { statements: statements.into(), position: 0, scope, value: None }];
    let mut visited = BTreeSet::new();
    let mut visited_statements = BTreeSet::new();
    while let Some(obligation) = pending.pop() {
        budget.charge(1)?;
        match obligation {
            Obligation::Publish { statements, position, scope, value, slot, ty, mutable } => {
                declare(&scope, &mut scopes, slot, ty, mutable, budget)?;
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
                        let subject = expression_type(types, value.raw() as usize, budget)?;
                        for arm in &mut arms {
                            let (pattern, guard, value) = arm?;
                            let inner = pattern_scope(decoder, pattern.raw(), &subject, &scope, &mut scopes, slot_count, &mut writers, budget)?;
                            if !body_type_matches(&expression_type(types, value.raw() as usize, budget)?, &claimed, budget)? { return Err(IrVerifyError::new("generic match arm disagrees with its result")); }
                            pending.push(Obligation::Expression(value.raw() as usize, inner.clone()));
                            if let Some(guard) = guard {
                                condition(types, guard.raw() as usize, crate::sema::types::ConditionRole::Boolean, budget)?;
                                pending.push(Obligation::Expression(guard.raw() as usize, inner));
                            }
                        }
                        pending.push(Obligation::Expression(value.raw() as usize, scope));
                    }
                    _ => {
                        let mut children = Vec::new();
                        verify_body_expression(store, decoder, instruction, types, &scope, &scopes, &mut children, &mut visited, registered, budget)?;
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
                        pending.push(Obligation::Publish { statements, position: position + 1, scope, value: value_contract, slot, ty: declared, mutable });
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
                    schema::StmtView::LetRecord { source, target: super::super::decode::CompTargetView::Discard, .. } => expressions.push(source.raw() as usize),
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
                        let subject_type = expression_type(types, subject.raw() as usize, budget)?;
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
    Ok((visited, writers, visited_statements))
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

}
