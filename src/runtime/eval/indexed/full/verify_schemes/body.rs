use super::*;

#[derive(Clone, Copy)]
pub(super) struct LexicalScope {
    frame: usize,
    loop_depth: usize,
    error_boundary: Option<usize>,
}

pub(super) struct InitialBinding {
    pub(super) ty: SchemeType,
    pub(super) mutable: bool,
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
    error_boundaries: Vec<PropagationBoundary>,
    function_boundary: PropagationBoundary,
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
    ApplicationReturn { instruction: usize, value: usize, site: super::super::schemes::FullSchemeApplicationReturn },
    Publish { statements: Arc<[usize]>, position: usize, scope: LexicalScope, value: Option<SchemeType>, bindings: Vec<(usize, SchemeType, bool, Option<usize>)> },
    Block { statements: Arc<[usize]>, position: usize, scope: LexicalScope, value: Option<SchemeType> },
}

fn application_return_origin(
    store: &FullStore, decoder: &FullDecoder<'_>, instruction: usize,
    expression_scopes: &BTreeMap<usize, LexicalScope>, scopes: &LexicalScopes,
    types: &BTreeMap<usize, SchemeType>, scheme_scope: &SchemeScope, expected_return: &SchemeType,
    memo: &mut BTreeMap<usize, Option<u32>>, budget: &mut SchemeInstantiationBudget,
) -> Result<Option<u32>, IrVerifyError> {
    enum Visit { Enter(usize), Finish(usize, Vec<usize>) }
    let mut pending = vec![Visit::Enter(instruction)];
    while let Some(visit) = pending.pop() {
        budget.charge(1)?;
        let instruction = match visit {
            Visit::Finish(instruction, children) => {
                let mut minimum = None;
                let mut all_applications = !children.is_empty();
                let actual = types.get(&instruction).ok_or_else(|| IrVerifyError::new("application origin has no checked producer type"))?;
                for child in children {
                    budget.charge(1)?;
                    let child_type = types.get(&child).ok_or_else(|| IrVerifyError::new("application origin child has no checked producer type"))?;
                    if !body_types_equal(actual, child_type) { return Err(IrVerifyError::new("application return branches disagree on their raw result type")); }
                    match memo[&child] {
                        Some(ordinal) => minimum = Some(minimum.map_or(ordinal, |previous: u32| previous.min(ordinal))),
                        None => all_applications = false,
                    }
                }
                memo.insert(instruction, if all_applications { minimum } else { None });
                continue;
            }
            Visit::Enter(instruction) => instruction,
        };
        if memo.contains_key(&instruction) { continue; }
        let mut children = Vec::new();
        match decoder.expression_view(instruction)? {
            schema::ExprView::Param(slot) => {
                let scope = expression_scopes.get(&instruction).ok_or_else(|| IrVerifyError::new("application return has no authenticated lexical scope"))?;
                let binding = scope.binding(scopes, slot, budget)?.ok_or_else(|| IrVerifyError::new("application return reads an unbound slot"))?;
                if binding.mutable { return Err(IrVerifyError::new("application return reads mutable storage")); }
                if let Some((source, _)) = binding.initializer { budget.charge(1)?; children.push(source); }
            }
            schema::ExprView::DynamicCall { .. } => {
                let site = store.scheme_application_calls.binary_search_by_key(&(instruction as u32), |site| site.instruction)
                    .map_err(|_| IrVerifyError::new("application return producer has no checked application"))?;
                let ordinal = store.scheme_application_calls[site].application;
                let function = &store.functions[IrFunctionId::from_raw(decoder.owner).ok_or_else(|| IrVerifyError::new("application return owner is invalid"))?.index()];
                let FullCallableContract::Scheme(id) = function.contract else { unreachable!() };
                let application = bounded(store.scheme(id)?.applications, &store.scheme_applications, "scheme application range is invalid")?
                    .get(ordinal as usize).ok_or_else(|| IrVerifyError::new("application return ordinal is out of scope"))?;
                let actual = types.get(&instruction).ok_or_else(|| IrVerifyError::new("application return has no actual result type"))?;
                let result = decode_scheme_type(store, scheme_scope, application.result, budget, 0)?;
                if !body_types_equal(actual, &result) { return Err(IrVerifyError::new("application return disagrees with its actual producer or envelope")); }
                let mut matched = false;
                for envelope in bounded(application.proc_returns, &store.scheme_proc_returns, "application return envelope range is invalid")? {
                    budget.charge(1)?;
                    let result = decode_scheme_type(store, scheme_scope, envelope.result, budget, 0)?;
                    matched |= body_types_equal(&result, expected_return);
                }
                if !matched { return Err(IrVerifyError::new("application return producer has no matching proc envelope")); }
                memo.insert(instruction, Some(ordinal));
                continue;
            }
            schema::ExprView::IfExpr { branches, else_value, .. } => {
                for branch in branches { budget.charge(1)?; children.push(branch?.1.raw() as usize); }
                budget.charge(1)?;
                children.push(else_value.raw() as usize);
            }
            schema::ExprView::MatchExpr { arms, .. } => {
                for arm in arms { budget.charge(1)?; children.push(arm?.2.raw() as usize); }
            }
            schema::ExprView::ValueBlock { body, .. } => {
                let mut tail = None;
                for statement in body { budget.charge(1)?; tail = Some(statement?.raw() as usize); }
                if let Some(tail) = tail && let schema::StmtView::Value { value } = decoder.statement_view(tail)? {
                    budget.charge(1)?;
                    children.push(value.raw() as usize);
                }
            }
            _ => {}
        }
        if children.is_empty() { memo.insert(instruction, None); continue; }
        budget.charge(children.len())?;
        pending.push(Visit::Finish(instruction, children.clone()));
        for child in children { pending.push(Visit::Enter(child)); }
    }
    Ok(memo[&instruction])
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
    Ok(LexicalScope { frame, loop_depth: scope.loop_depth, error_boundary: scope.error_boundary })
}

fn propagation_boundary<'a>(scope: &LexicalScope, scopes: &'a LexicalScopes) -> &'a PropagationBoundary {
    scope.error_boundary.map_or(&scopes.function_boundary, |index| &scopes.error_boundaries[index])
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

    fn list_item(self) -> Option<Self> {
        match self.unvalidated() {
            Self::Scheme(SchemeType::List(item)) => Some(Self::Scheme(item)),
            Self::Ground(Type::List(item)) => Some(Self::Ground(item)),
            _ => None,
        }
    }

    fn result_parts(self) -> Option<(Self, Self)> {
        match self.unvalidated() {
            Self::Scheme(SchemeType::Result(ok, error)) => Some((Self::Scheme(ok), Self::Scheme(error))),
            Self::Ground(Type::Result(ok, error)) => Some((Self::Ground(ok), Self::Ground(error))),
            _ => None,
        }
    }

    fn map_parts(self) -> Option<(Self, Self)> {
        match self.unvalidated() {
            Self::Scheme(SchemeType::Map(key, value)) => Some((Self::Scheme(key), Self::Scheme(value))),
            Self::Ground(Type::Map(key, value)) => Some((Self::Ground(key), Self::Ground(value))),
            _ => None,
        }
    }

    fn validation(self) -> Option<Validation> {
        match self {
            Self::Scheme(SchemeType::Validated(validation, _)) => Some(*validation),
            Self::Scheme(SchemeType::Ground(ty)) => Self::Ground(ty).validation(),
            Self::Ground(Type::Validated(ty)) => Some(ty.validation()),
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

fn iteration_item(store: &FullStore, source: &SchemeType, instruction: usize, boundary: &PropagationBoundary, budget: &mut SchemeInstantiationBudget) -> Result<(SchemeType, Option<super::producers::StreamProducer>), IrVerifyError> {
    budget.charge(1)?;
    match source {
        SchemeType::Result(ok, error) => {
            body_propagated_error_matches_owner(store, error, boundary, budget)?;
            direct_iteration_item(ok, super::producers::StreamProducer::ResultSuccess(instruction), budget)
        }
        SchemeType::Ground(Type::Result(ok, error)) => {
            budget.charge_clone(error)?;
            body_propagated_error_matches_owner(store, &SchemeType::Ground((**error).clone()), boundary, budget)?;
            budget.charge_clone(ok)?;
            direct_iteration_item(&SchemeType::Ground((**ok).clone()), super::producers::StreamProducer::ResultSuccess(instruction), budget)
        }
        SchemeType::Validated(_, source) => iteration_item(store, source, instruction, boundary, budget),
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

fn comprehension_scope(
    store: &FullStore, types: &BTreeMap<usize, SchemeType>,
    mut qualifiers: super::super::decode::ListView<'_, crate::runtime::eval::LoweredCompQualifier>,
    scope: &LexicalScope, scopes: &mut LexicalScopes, slot_count: usize,
    writers: &mut BTreeMap<usize, SchemeType>,
    pending: &mut Vec<Obligation>, stream_iterations: &mut Vec<(super::producers::StreamProducer, PropagationBoundary)>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<(LexicalScope, Option<Validation>), IrVerifyError> {
    let count = qualifiers.len();
    if count == 0 { return Err(IrVerifyError::new("comprehension has no iteration source")); }
    let mut current = *scope;
    let mut mapped_validation = None;
    let mut position = 0;
    while qualifiers.len() != 0 {
        budget.charge(1)?;
        match qualifiers.next().ok_or_else(|| IrVerifyError::new("comprehension qualifier is absent"))?? {
            super::super::decode::CompQualifierView::For { target, iter, .. } => {
                let instruction = iter.raw() as usize;
                let source = expression_type(types, instruction, budget)?;
                if count == 1 { mapped_validation = BodyTypeRef::Scheme(&source).validation().filter(|validation| validation.survives_mapping()); }
                let (item, stream_source) = iteration_item(store, &source, instruction, propagation_boundary(&current, scopes), budget)?;
                if let Some(source) = stream_source { budget.charge(1)?; stream_iterations.push((source, propagation_boundary(&current, scopes).clone())); }
                pending.push(Obligation::Expression(instruction, current));
                let bindings = target_bindings(target, &item, budget)?;
                current = branch_scope(&current, scopes, budget)?;
                for (slot, ty) in bindings {
                    budget.charge_scheme_clone(&ty)?;
                    if slot >= slot_count || writers.insert(slot, ty.clone()).is_some() { return Err(IrVerifyError::new("generic comprehension binding slot is duplicated or out of bounds")); }
                    declare(&current, scopes, slot, ty, false, None, budget)?;
                }
            }
            super::super::decode::CompQualifierView::If { condition: value, .. } => {
                if position == 0 { return Err(IrVerifyError::new("comprehension does not start with an iteration source")); }
                let instruction = value.raw() as usize;
                condition(types, instruction, crate::sema::types::ConditionRole::Control, budget)?;
                pending.push(Obligation::Expression(instruction, current));
            }
        }
        position += 1;
    }
    Ok((current, mapped_validation))
}

fn assignment_replacement(
    actual: &SchemeType, expected: &SchemeType, op: crate::syntax::node::AssignOp,
    check: Option<super::super::decode::TypeCheckView<'_>>, budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    use crate::syntax::node::{AssignOp, BinaryOp};
    let result = if op == AssignOp::Set {
        budget.charge_scheme_clone(actual)?;
        actual.clone()
    } else if op == AssignOp::Add && BodyTypeRef::Scheme(expected).list_item().is_some() {
        let left = BodyTypeRef::Scheme(expected).unvalidated().materialize(budget)?;
        let right = BodyTypeRef::Scheme(actual).unvalidated().materialize(budget)?;
        if !body_types_equal(&left, &right)
            || BodyTypeRef::Scheme(expected).validation().is_some_and(|validation| !validation.survives_concatenation())
        { return Err(IrVerifyError::new("generic compound assignment changes its list domain")); }
        budget.charge_scheme_clone(expected)?;
        expected.clone()
    } else {
        budget.charge_scheme_clone(expected)?;
        budget.charge_scheme_clone(actual)?;
        let left = expected.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic compound assignment has no closed operator domain"))?;
        let right = actual.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic compound assignment has no closed operand domain"))?;
        let binary = match op { AssignOp::Set => unreachable!(), AssignOp::Add => BinaryOp::Add, AssignOp::Sub => BinaryOp::Sub, AssignOp::Mul => BinaryOp::Mul, AssignOp::Div => BinaryOp::Div, AssignOp::Rem => BinaryOp::Rem };
        SchemeType::Ground(FullDecoder::checked_binary_result_type(binary, left.unvalidated(), right.unvalidated()).ok_or_else(|| IrVerifyError::new("generic compound assignment has incompatible operands"))?)
    };
    if let Some(check) = check {
        let checked = check.ty.materialize()?;
        if check.schema.is_some() || expected.closed_ground_type().as_ref() != Some(&checked)
            || !body_type_matches_ground(&result, &checked) || !checked.has_unsigned_constraint()
        { return Err(IrVerifyError::new("generic assignment has an invalid storage check")); }
    } else if !body_type_matches(&result, expected, budget)?
        || expected.closed_ground_type().is_some_and(|expected| expected.has_unsigned_constraint() && result.closed_ground_type() != Some(expected))
    { return Err(IrVerifyError::new("generic assignment disagrees with its invariant slot contract")); }
    Ok(())
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

pub(super) struct BodyProof {
    pub(super) expressions: BTreeSet<usize>,
    pub(super) writers: BTreeMap<usize, SchemeType>,
    pub(super) statements: BTreeSet<usize>,
    pub(super) streams: Vec<(super::producers::StreamProducer, PropagationBoundary)>,
    pub(super) captures: Vec<(usize, SchemeType)>,
    pub(super) applications: BTreeSet<usize>,
}

pub(super) fn verify_body(
    store: &FullStore, decoder: &FullDecoder<'_>, types: &BTreeMap<usize, SchemeType>,
    constructors: &super::constructors::ConstructorFacts,
    tails: &mut BTreeMap<usize, SchemeType>, declarations: &mut BTreeMap<usize, (SchemeType, bool)>,
    slots: BTreeMap<usize, InitialBinding>, scheme_scope: &SchemeScope, effect_scopes: &VerifiedSchemeScopes, expected_return: &SchemeType, statements: Vec<usize>,
    analysis: &super::super::IndexedReturnAnalysis, registered: &mut BTreeSet<u32>, budget: &mut SchemeInstantiationBudget, constant_budget: &mut SchemeInstantiationBudget,
) -> Result<BodyProof, IrVerifyError> {
    let slot_count = decoder.slot_count as usize;
    for binding in slots.values() { budget.charge_scheme_clone(&binding.ty)?; }
    let mut writers = slots.iter().map(|(&slot, binding)| (slot, binding.ty.clone())).collect::<BTreeMap<_, _>>();
    let function_boundary = PropagationBoundary::for_function(store, decoder.owner, expected_return, budget)?;
    let mut scopes = LexicalScopes { frames: vec![LexicalFrame { parent: None, bindings: slots.into_iter().map(|(slot, binding)| (slot, LocalBinding { ty: binding.ty, mutable: binding.mutable, initializer: None })).collect() }], error_boundaries: Vec::new(), function_boundary };
    let scope = LexicalScope { frame: 0, loop_depth: 0, error_boundary: None };
    let owner = IrFunctionId::from_raw(decoder.owner).ok_or_else(|| IrVerifyError::new("generic body has no function owner"))?;
    let function = store.functions.get(owner.index()).ok_or_else(|| IrVerifyError::new("generic default has no function owner"))?;
    let stream_item = if store.function_metadata.get(owner.index()).is_some_and(|metadata| metadata.callable_kind == crate::sema::types::CallableKind::Stream) {
        Some(match expected_return {
            SchemeType::Stream(item) => { budget.charge_scheme_clone(item)?; (**item).clone() }
            SchemeType::Ground(Type::Stream(item)) => { budget.charge_clone(item)?; SchemeType::Ground((**item).clone()) }
            _ => return Err(IrVerifyError::new("generic stream owner has no stream item contract")),
        })
    } else { None };
    let parameter_count = function.params.bounds(store.params.len()).ok_or_else(|| IrVerifyError::new("generic default parameter range is invalid"))?.len();
    let mut default_bindings = BTreeMap::new();
    for (&slot, binding) in &scopes.frames[0].bindings {
        if slot >= parameter_count {
            budget.charge_scheme_clone(&binding.ty)?;
            default_bindings.insert(slot, LocalBinding { ty: binding.ty.clone(), mutable: binding.mutable, initializer: None });
        }
    }
    let default_scope = LexicalScope { frame: scopes.frames.len(), loop_depth: 0, error_boundary: None };
    scopes.frames.push(LexicalFrame { parent: None, bindings: default_bindings });
    let mut pending = vec![Obligation::Block { statements: statements.into(), position: 0, scope, value: None }];
    let mut visited = BTreeSet::new();
    let mut visited_statements = BTreeSet::new();
    let mut stream_iterations = Vec::new();
    let mut captures = Vec::new();
    let mut applications = BTreeSet::new();
    let mut application_returns = BTreeSet::new();
    let mut expression_scopes = BTreeMap::new();
    let mut return_origins = BTreeMap::new();
    while let Some(obligation) = pending.pop() {
        budget.charge(1)?;
        match obligation {
            Obligation::Publish { statements, position, scope, value, bindings } => {
                for (slot, ty, mutable, initializer) in bindings { declare(&scope, &mut scopes, slot, ty, mutable, initializer.map(|initializer| (initializer, scope)), budget)?; }
                pending.push(Obligation::Block { statements, position, scope, value });
            }
            Obligation::ApplicationReturn { instruction, value, site } => {
                if application_return_origin(store, decoder, value, &expression_scopes, &scopes, types, scheme_scope, expected_return, &mut return_origins, budget)? != Some(site.application) {
                    return Err(IrVerifyError::new("application return disagrees with its actual producer or envelope"));
                }
                if !application_returns.insert(instruction) { return Err(IrVerifyError::new("application return is duplicated")); }
            }
            Obligation::Expression(instruction, scope) => {
                budget.charge(1)?;
                expression_scopes.insert(instruction, scope);
                let claimed = expression_type(types, instruction, budget)?;
                match decoder.expression_view(instruction)? {
                    schema::ExprView::DynamicCall { .. } => {
                        if !visited.insert(instruction) { return Err(IrVerifyError::new("generic expression is shared or cyclic")); }
                        applications.insert(instruction);
                        for child in applications::verify_site(store, decoder, instruction, scheme_scope, effect_scopes, scope.error_boundary.is_some(), types, budget)? {
                            pending.push(Obligation::Expression(child, scope));
                        }
                    }
                    schema::ExprView::Capture { body, .. } => {
                        if !visited.insert(instruction) { return Err(IrVerifyError::new("generic expression is shared or cyclic")); }
                        let (ok, error) = BodyTypeRef::Scheme(&claimed).result_parts()
                            .ok_or_else(|| IrVerifyError::new("generic capture has no checked result contract"))?;
                        let ok = ok.materialize(budget)?;
                        let error = error.materialize(budget)?;
                        budget.charge(1)?;
                        captures.push((instruction, error));
                        let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                        budget.charge(statements.len())?;
                        let value = if statements.last().is_some_and(|statement| store.tags[*statement] == FullTag::StmtValue) { Some(ok) }
                            else {
                                if !body_type_matches(&SchemeType::Ground(Type::Unit), &ok, budget)? {
                                    return Err(IrVerifyError::new("generic capture fallthrough disagrees with its success contract"));
                                }
                                None
                            };
                        let mut inner = branch_scope(&scope, &mut scopes, budget)?;
                        inner.error_boundary = Some(scopes.error_boundaries.len());
                        scopes.error_boundaries.push(PropagationBoundary::for_result(&claimed, budget)?);
                        pending.push(Obligation::Block { statements: statements.into(), position: 0, scope: inner, value });
                    }
                    schema::ExprView::ValueBlock { body, .. } => {
                        if !visited.insert(instruction) { return Err(IrVerifyError::new("generic expression is shared or cyclic")); }
                        let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                        budget.charge(statements.len())?;
                        pending.push(Obligation::Block { statements: statements.into(), position: 0, scope: branch_scope(&scope, &mut scopes, budget)?, value: Some(claimed) });
                    }
                    schema::ExprView::ListComp { value, qualifiers, .. } => {
                        if !visited.insert(instruction) { return Err(IrVerifyError::new("generic expression is shared or cyclic")); }
                        let item = BodyTypeRef::Scheme(&claimed).list_item().ok_or_else(|| IrVerifyError::new("generic comprehension has no list item contract"))?.materialize(budget)?;
                        let (inner, mapped_validation) = comprehension_scope(store, types, qualifiers, &scope, &mut scopes, slot_count, &mut writers, &mut pending, &mut stream_iterations, budget)?;
                        if let Some(validation) = BodyTypeRef::Scheme(&claimed).validation()
                            && Some(validation) != mapped_validation
                        { return Err(IrVerifyError::new("generic comprehension cannot establish its claimed validation")); }
                        if !body_type_matches(&expression_type(types, value.raw() as usize, budget)?, &item, budget)? { return Err(IrVerifyError::new("generic comprehension value disagrees with its item domain")); }
                        pending.push(Obligation::Expression(value.raw() as usize, inner));
                    }
                    schema::ExprView::MapComp { key, value, qualifiers, .. } => {
                        if !visited.insert(instruction) { return Err(IrVerifyError::new("generic expression is shared or cyclic")); }
                        if BodyTypeRef::Scheme(&claimed).validation().is_some() { return Err(IrVerifyError::new("generic map comprehension cannot establish a validation")); }
                        let (expected_key, item) = BodyTypeRef::Scheme(&claimed).map_parts().ok_or_else(|| IrVerifyError::new("generic map comprehension has no map contract"))?;
                        let expected_key = expected_key.materialize(budget)?;
                        let domain = expected_key.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic map comprehension key domain is unresolved"))?;
                        if !domain.is_map_key() { return Err(IrVerifyError::new("generic map comprehension key domain is invalid")); }
                        let item = item.materialize(budget)?;
                        let (inner, _) = comprehension_scope(store, types, qualifiers, &scope, &mut scopes, slot_count, &mut writers, &mut pending, &mut stream_iterations, budget)?;
                        if !body_type_matches(&expression_type(types, key.raw() as usize, budget)?, &expected_key, budget)? { return Err(IrVerifyError::new("generic comprehension key disagrees with its map domain")); }
                        if !body_type_matches(&expression_type(types, value.raw() as usize, budget)?, &item, budget)? { return Err(IrVerifyError::new("generic comprehension value disagrees with its item domain")); }
                        pending.push(Obligation::Expression(value.raw() as usize, inner));
                        pending.push(Obligation::Expression(key.raw() as usize, inner));
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
                        verify_body_expression(store, decoder, instruction, types, constructors, &scope, &scopes, &mut children, &mut visited, registered, propagation_boundary(&scope, &scopes), budget, constant_budget)?;
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
                    view @ (schema::StmtView::Assign { .. } | schema::StmtView::AssignPath { .. }) => {
                        let (slot, op, value, check, path) = match view {
                            schema::StmtView::Assign { slot, op, value, check, .. } => (slot, op, value, check, None),
                            schema::StmtView::AssignPath { slot, op, value, check, path, .. } => (slot, op, value, check, Some(path)),
                            _ => unreachable!(),
                        };
                        let binding = scope.binding(&scopes, slot, budget)?.ok_or_else(|| IrVerifyError::new("generic assignment does not target a mutable declaration"))?;
                        if !binding.mutable { return Err(IrVerifyError::new("generic assignment does not target a mutable declaration")); }
                        let mut expected = BodyTypeRef::Scheme(&binding.ty);
                        if let Some(mut path) = path {
                            if path.len() == 0 { return Err(IrVerifyError::new("generic assignment path is empty")); }
                            while path.len() != 0 {
                                budget.charge(1)?;
                                expected = match path.next().ok_or_else(|| IrVerifyError::new("generic assignment selector is absent"))?? {
                                    super::super::decode::AssignStepView::Field(name) => expected.field(name).ok_or_else(|| IrVerifyError::new("generic assignment field disagrees with its storage contract"))?,
                                    super::super::decode::AssignStepView::Index(index) => {
                                        let instruction = index.raw() as usize;
                                        let actual = expression_type(types, instruction, budget)?;
                                        let (key, item) = if let Some((key, item)) = expected.map_parts() {
                                            (key.materialize(budget)?, item)
                                        } else {
                                            (SchemeType::Ground(Type::Int), expected.list_item().ok_or_else(|| IrVerifyError::new("generic assignment index has no list or map storage contract"))?)
                                        };
                                        if !body_type_matches(&actual, &key, budget)? { return Err(IrVerifyError::new("generic assignment selector disagrees with its key contract")); }
                                        expressions.push(instruction);
                                        item
                                    }
                                };
                            }
                        }
                        let expected = expected.materialize(budget)?;
                        let actual = expression_type(types, value.raw() as usize, budget)?;
                        assignment_replacement(&actual, &expected, op, check, budget)?;
                        expressions.push(value.raw() as usize);
                    }
                    schema::StmtView::LetRecord { source, target, .. } => {
                        if matches!(target, super::super::decode::CompTargetView::Discard) {
                            if constructors.contains_key(&(source.raw() as usize)) { super::constructors::discard(decoder, source.raw() as usize, constructors, &mut visited, budget, |instruction, budget| {
                                let ty = expression_type(types, instruction, budget)?;
                                expressions.push(instruction);
                                Ok(ty)
                            })?; }
                            else { expressions.push(source.raw() as usize); }
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
                    schema::StmtView::DefaultParameter { slot, value, kind, check, .. } => {
                        if slot >= parameter_count { return Err(IrVerifyError::new("generic default does not target a parameter")); }
                        let expected = scope.slot_type(&scopes, slot, budget)?.ok_or_else(|| IrVerifyError::new("generic default has no parameter contract"))?;
                        budget.charge_scheme_clone(expected)?;
                        let expected = expected.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic default requires a concrete parameter contract"))?;
                        if super::super::lowered_type_from_type(&expected)? != super::super::lowered_type_from_type(&store.semantic.to_type_with_budget(kind.id, &mut budget.semantic_types)?)? { return Err(IrVerifyError::new("generic default representation disagrees with its parameter")); }
                        let actual = expression_type(types, value.raw() as usize, budget)?;
                        if let Some(check) = check {
                            if check.ty.materialize()? != expected || !body_type_matches_ground(&actual, &expected) { return Err(IrVerifyError::new("generic default check disagrees with its parameter")); }
                        } else if !body_type_matches(&actual, &SchemeType::Ground(expected), budget)? { return Err(IrVerifyError::new("generic default value disagrees with its parameter")); }
                        nested.push(Obligation::Expression(value.raw() as usize, default_scope));
                    }
                    schema::StmtView::Print { args, .. } => {
                        for argument in args {
                            budget.charge(1)?;
                            let argument = argument?;
                            let actual = expression_type(types, argument.raw() as usize, budget)?.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic print argument has no checked display contract"))?;
                            if !actual.can_display() { return Err(IrVerifyError::new("generic print argument cannot be displayed")); }
                            expressions.push(argument.raw() as usize);
                        }
                    }
                    schema::StmtView::Cd { target, body, .. } => {
                        let actual = expression_type(types, target.raw() as usize, budget)?.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic cd target has no checked path contract"))?;
                        if !matches!(actual.unvalidated(), Type::Path | Type::Str) { return Err(IrVerifyError::new("generic cd target is not a path")); }
                        expressions.push(target.raw() as usize);
                        let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                        budget.charge(statements.len())?;
                        nested.push(Obligation::Block { statements: statements.into(), position: 0, scope: branch_scope(&scope, &mut scopes, budget)?, value: None });
                    }
                    schema::StmtView::Env { env, body } => {
                        for assignment in env {
                            budget.charge(1)?;
                            let assignment = assignment?;
                            super::commands::word(assignment.value, super::producers::ProcessWordRole::Environment, &mut |value, budget| {
                                expressions.push(value.raw() as usize);
                                expression_type(types, value.raw() as usize, budget)
                            }, budget)?;
                        }
                        let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                        budget.charge(statements.len())?;
                        nested.push(Obligation::Block { statements: statements.into(), position: 0, scope: branch_scope(&scope, &mut scopes, budget)?, value: None });
                    }
                    schema::StmtView::Run { value, .. } => {
                        let process = match decoder.expression_view(value.raw() as usize)? {
                            schema::ExprView::Try { value, .. } => decoder.expression_view(value.raw() as usize)?,
                            process => process,
                        };
                        if !matches!(process, schema::ExprView::RunCapture(_) | schema::ExprView::RunPipeline { .. }) { return Err(IrVerifyError::new("generic run statement has no process producer")); }
                        expressions.push(value.raw() as usize);
                    }
                    schema::StmtView::Expr { value, .. } => {
                        if constructors.contains_key(&(value.raw() as usize)) { super::constructors::discard(decoder, value.raw() as usize, constructors, &mut visited, budget, |instruction, budget| {
                            let ty = expression_type(types, instruction, budget)?;
                            expressions.push(instruction);
                            Ok(ty)
                        })?; }
                        else { expressions.push(value.raw() as usize); }
                    },
                    schema::StmtView::Yield { value } => {
                        let item = stream_item.as_ref().ok_or_else(|| IrVerifyError::new("generic yield has no stream owner"))?;
                        if !body_type_matches(&expression_type(types, value.raw() as usize, budget)?, item, budget)? {
                            return Err(IrVerifyError::new("generic yield disagrees with its stream item contract"));
                        }
                        expressions.push(value.raw() as usize);
                    }
                    schema::StmtView::YieldDelegate { value, .. } => {
                        let item = stream_item.as_ref().ok_or_else(|| IrVerifyError::new("generic delegation has no stream owner"))?;
                        let source = expression_type(types, value.raw() as usize, budget)?;
                        let (actual, stream) = match &source {
                            SchemeType::Stream(actual) => { budget.charge_scheme_clone(actual)?; ((**actual).clone(), true) }
                            SchemeType::List(actual) => { budget.charge_scheme_clone(actual)?; ((**actual).clone(), false) }
                            SchemeType::Ground(Type::Stream(actual)) => { budget.charge_clone(actual)?; (SchemeType::Ground((**actual).clone()), true) }
                            SchemeType::Ground(Type::List(actual)) => { budget.charge_clone(actual)?; (SchemeType::Ground((**actual).clone()), false) }
                            _ => return Err(IrVerifyError::new("generic delegation has no list or stream producer")),
                        };
                        if !body_type_matches(&actual, item, budget)? { return Err(IrVerifyError::new("generic delegation disagrees with its stream item contract")); }
                        if stream {
                            budget.charge(1)?;
                            stream_iterations.push((super::producers::StreamProducer::Value(value.raw() as usize), scopes.function_boundary.clone()));
                        }
                        expressions.push(value.raw() as usize);
                    }
                    schema::StmtView::Defer { value, .. } => {
                        if stream_item.is_none() { return Err(IrVerifyError::new("generic defer has no stream owner")); }
                        expressions.push(value.raw() as usize);
                    }
                    schema::StmtView::Return { value: returned } => {
                        let actual = expression_type(types, returned.raw() as usize, budget)?;
                        if stream_item.is_some() {
                            if actual != SchemeType::Ground(Type::Unit) { return Err(IrVerifyError::new("generic stream returns an eager value")); }
                            expressions.push(returned.raw() as usize);
                            pending.push(Obligation::Block { statements, position: position + 1, scope: scope.clone(), value: value_contract });
                            for instruction in expressions { pending.push(Obligation::Expression(instruction, scope)); }
                            continue;
                        }
                        let returned_site = store.scheme_application_returns.binary_search_by_key(&(instruction as u32), |site| site.instruction).ok()
                            .map(|index| store.scheme_application_returns[index]);
                        if let Some(site) = returned_site {
                            budget.charge(1)?;
                            if site.function.raw() != decoder.owner {
                                return Err(IrVerifyError::new("application return has a wrong or duplicate owner"));
                            }
                            let FullCallableContract::Scheme(id) = function.contract else { unreachable!() };
                            let application = bounded(store.scheme(id)?.applications, &store.scheme_applications, "scheme application range is invalid")?
                                .get(site.application as usize).ok_or_else(|| IrVerifyError::new("scheme return application ordinal is out of scope"))?;
                            let envelope = bounded(application.proc_returns, &store.scheme_proc_returns, "scheme return envelope range is invalid")?
                                .get(site.envelope as usize).ok_or_else(|| IrVerifyError::new("scheme return envelope ordinal is out of scope"))?;
                            let source = decode_scheme_type(store, scheme_scope, application.result, budget, 0)?;
                            let output = decode_scheme_type(store, scheme_scope, envelope.result, budget, 0)?;
                            if !body_types_equal(&source, &actual) {
                                return Err(IrVerifyError::new("application return disagrees with its actual producer or envelope"));
                            }
                            if !body_types_equal(&output, expected_return) {
                                return Err(IrVerifyError::new("application return producer has no matching proc envelope"));
                            }
                            if tails.remove(&instruction).is_some_and(|tail| !body_types_equal(&tail, &actual)) { return Err(IrVerifyError::new("application return tail disagrees with its actual producer")); }
                            nested.push(Obligation::ApplicationReturn { instruction, value: returned.raw() as usize, site });
                        } else {
                            if !actual.matches_body_return(expected_return) { return Err(IrVerifyError::new("generic body return does not match its scheme return")); }
                            if tails.remove(&instruction).is_some_and(|tail| !tail.matches_body_return(expected_return)) { return Err(IrVerifyError::new("generic tail disagrees with its scheme return")); }
                        }
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
                        let (item, stream_source) = iteration_item(store, &source, iter.raw() as usize, propagation_boundary(&scope, &scopes), budget)?;
                        if let Some(source) = stream_source { budget.charge(1)?; stream_iterations.push((source, propagation_boundary(&scope, &scopes).clone())); }
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
    let owner_range = store.function_instruction_range(IrFunctionId::from_raw(decoder.owner).ok_or_else(|| IrVerifyError::new("application return owner is invalid"))?.index())?;
    let first_return = store.scheme_application_returns.partition_point(|site| (site.instruction as usize) < owner_range.start);
    let end_return = store.scheme_application_returns.partition_point(|site| (site.instruction as usize) < owner_range.end);
    budget.charge(end_return - first_return)?;
    if store.scheme_application_returns[first_return..end_return].iter().any(|site| !application_returns.contains(&(site.instruction as usize))) {
        return Err(IrVerifyError::new("scheme application return has no actual body instruction"));
    }
    Ok(BodyProof { expressions: visited, writers, statements: visited_statements, streams: stream_iterations, captures, applications })
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
        program.store.scheme_body_facts.iter().position(|fact| fact.kind() == FullSchemeBodyFactKind::LocalMutable).unwrap()
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
        let index = declaration(&program);
        immutable.store.scheme_body_facts[index].value = super::super::super::schemes::FullSchemeBodyFactValue::LocalImmutable(immutable.store.scheme_body_facts[index].ty().unwrap());
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
        let changed = retarget_comprehension_operand(&program, owner, scheme, assigned, 0, 1);
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic assignment disagrees with its invariant slot contract");
    }

    #[test]
    fn ordinary_generic_body_assignments_cannot_escape_nested_scope() {
        let program = prepare("proc keep(value) [] { var result = value; { var hidden = value; hidden = value }; result = value; result }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let mutables = program.store.scheme_body_facts.iter().filter(|fact| fact.kind() == FullSchemeBodyFactKind::LocalMutable).map(|fact| {
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
                let node = &mut changed.store.scheme_types[fact.ty().unwrap().index()];
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
                let node = &mut changed.store.scheme_types[fact.ty().unwrap().index()];
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
                let node = &mut changed.store.scheme_types[fact.ty().unwrap().index()];
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
                let node = &mut changed.store.scheme_types[fact.ty().unwrap().index()];
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
                let node = &mut changed.store.scheme_types[fact.ty().unwrap().index()];
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
                    let node = &mut changed.store.scheme_types[fact.ty().unwrap().index()];
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
                let result = changed.store.scheme_types[fact.ty().unwrap().index()];
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
                let optional = changed.store.scheme_types[fact.ty().unwrap().index()];
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
                let node = &mut changed.store.scheme_types[fact.ty().unwrap().index()];
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
                let ty = &mut changed.store.scheme_types[fact.ty().unwrap().index()];
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
                let node = &mut changed.store.scheme_types[fact.ty().unwrap().index()];
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
                let node = changed.store.scheme_types[fact.ty().unwrap().index()];
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
                let node = changed.store.scheme_types[fact.ty().unwrap().index()];
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
                let node = &mut changed.store.scheme_types[fact.ty().unwrap().index()];
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
                let node = target.store.scheme_types[fact.ty().unwrap().index()];
                match node.tag {
                    FullSchemeTypeTag::Ground => target.store.scheme_types[fact.ty().unwrap().index()].first = expected.raw(),
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
                let leaf = record_field_type(&changed.store, record_field_type(&changed.store, fact.ty().unwrap(), "nested"), "count");
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
                assert_ne!(fact.ty().unwrap(), flag);
                let node = &mut result.store.scheme_types[fact.ty().unwrap().index()];
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
                let actual = evaluator.call_indexed_direct(key, crate::runtime::eval::CallableKind::Pure, &[crate::runtime::value::Value::Int(number), crate::runtime::value::Value::Str(Arc::from("wrong"))], crate::source::Span::at(program.store.source_id, 0)).unwrap().unwrap();
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

    fn retarget_comprehension_operand(program: &FullProgram, owner: usize, scheme: FullSchemeId, root: usize, from: usize, to: usize) -> FullProgram {
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        let actual = program.store.scheme_types[program.store.scheme_params[parameters.start + to].ty.index()];
        let mut actual = actual;
        while actual.tag == FullSchemeTypeTag::List {
            actual = program.store.scheme_types[SchemeTypeId::from_raw(actual.first).unwrap().index()];
        }
        let mut pending = vec![root];
        let mut instructions = BTreeSet::new();
        let mut leaves = Vec::new();
        while let Some(instruction) = pending.pop() {
            if !instructions.insert(instruction) { continue; }
            match decoder.expression_view(instruction).unwrap() {
                schema::ExprView::ValueBlock { mut body, .. } => {
                    for statement in &mut body {
                        let statement = statement.unwrap().raw() as usize;
                        instructions.insert(statement);
                        match decoder.statement_view(statement).unwrap() {
                            schema::StmtView::Let { value, .. } | schema::StmtView::Value { value } => pending.push(value.raw() as usize),
                            _ => panic!("the fixture operand contains only its ANF bindings and terminal value"),
                        }
                    }
                }
                schema::ExprView::List(mut items) => { for item in &mut items { pending.push(item.unwrap().raw() as usize); } }
                schema::ExprView::Param(slot) => { if slot == from { leaves.push(instruction); } }
                _ => panic!("the fixture operand derives its domain from one header parameter"),
            }
        }
        assert_eq!(leaves.len(), 1);
        let mut changed = program.clone();
        let words = changed.store.data[leaves[0]].range().bounds(changed.store.extra.len()).unwrap();
        changed.store.extra[words.start] = to as u32;
        let mut changed_types = BTreeSet::new();
        for fact in &program.store.scheme_body_facts {
            if !instructions.contains(&(fact.instruction as usize)) { continue; }
            let mut pending = vec![fact.ty().unwrap()];
            while let Some(ty) = pending.pop() {
                if !changed_types.insert(ty) { continue; }
                let node = &mut changed.store.scheme_types[ty.index()];
                match node.tag {
                    FullSchemeTypeTag::List => pending.push(SchemeTypeId::from_raw(node.first).unwrap()),
                    FullSchemeTypeTag::Parameter | FullSchemeTypeTag::Ground => {
                        assert_eq!(node.tag, actual.tag);
                        node.first = actual.first;
                    }
                    _ => panic!("the coordinated fixture has only scalar and list domains"),
                }
            }
        }
        for parameter in &program.store.scheme_params[parameters] {
            assert_eq!(program.store.scheme_types[parameter.ty.index()], changed.store.scheme_types[parameter.ty.index()]);
        }
        changed
    }

    #[test]
    fn generic_assignment_writers_require_mutable_lexical_owners() {
        let program = prepare("pure keep(value) { var row = {item: value}; let frozen = row; row.item = value; row.item }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let mut range = program.store.function_instruction_range(owner).unwrap();
        let (assignment, root) = range.clone().find_map(|instruction| match decoder.statement_view(instruction) { Ok(schema::StmtView::AssignPath { slot, .. }) => Some((instruction, slot)), _ => None }).unwrap();
        let frozen = range.find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Let { slot, value, .. }) if matches!(decoder.expression_view(value.raw() as usize), Ok(schema::ExprView::Param(source)) if source == root) => Some(slot),
            _ => None,
        }).unwrap();
        let mut changed = program.clone();
        let words = changed.store.data[assignment].range().bounds(changed.store.extra.len()).unwrap();
        changed.store.extra[words.start] = frozen as u32;
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic assignment does not target a mutable declaration");
        let program = prepare("pure keep(value) { {var inner = value; inner = value}; var outer = value; outer = value; outer }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let facts = program.store.schemes[scheme.index()].body_facts.bounds(program.store.scheme_body_facts.len()).unwrap();
        let locals = program.store.scheme_body_facts[facts].iter().filter(|fact| fact.kind() == FullSchemeBodyFactKind::LocalMutable).map(|fact| match decoder.statement_view(fact.instruction as usize).unwrap() { schema::StmtView::Let { slot, .. } => slot, _ => panic!("the mutable locals have simple declarations") }).collect::<Vec<_>>();
        assert_eq!(locals.len(), 2);
        let assignment = program.store.function_instruction_range(owner).unwrap().find(|instruction| matches!(decoder.statement_view(*instruction), Ok(schema::StmtView::Assign { slot, .. }) if slot == locals[1])).unwrap();
        let mut changed = program.clone();
        let words = changed.store.data[assignment].range().bounds(changed.store.extra.len()).unwrap();
        changed.store.extra[words.start] = locals[0] as u32;
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic assignment does not target a mutable declaration");
    }

    #[test]
    fn generic_assignment_selectors_authenticate_actual_key_domains() {
        let program = prepare("pure keep(value, index: Int, wrong: Bool) { var items = [value]; items[index] = value; items }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let selector = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::AssignPath { mut path, .. }) => match path.next().unwrap().unwrap() { super::super::super::decode::AssignStepView::Index(index) => Some(index.raw() as usize), _ => None },
            _ => None,
        }).unwrap();
        let changed = retarget_comprehension_operand(&program, owner, scheme, selector, 1, 2);
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic assignment selector disagrees with its key contract");
    }

    #[test]
    fn generic_assignment_values_authenticate_the_actual_selected_payload() {
        let program = prepare("pure replace(value, other, spare) { var items = [value]; items[0] = other; items }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let value = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.statement_view(instruction) { Ok(schema::StmtView::AssignPath { value, .. }) => Some(value.raw() as usize), _ => None }).unwrap();
        let changed = retarget_comprehension_operand(&program, owner, scheme, value, 1, 2);
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic assignment disagrees with its invariant slot contract");
    }

    #[test]
    fn generic_assignment_unsigned_compounds_require_the_actual_check() {
        let program = prepare("pure update(value, initial: UInt, signed: Int) { var row = {value, count: initial}; row.count += signed; row }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let assignment = program.store.function_instruction_range(owner).unwrap().find(|instruction| matches!(decoder.statement_view(*instruction), Ok(schema::StmtView::AssignPath { check: Some(_), .. }))).unwrap();
        let schema::StmtView::AssignPath { check: Some(check), .. } = decoder.statement_view(assignment).unwrap() else { unreachable!() };
        assert_eq!(check.ty.materialize().unwrap(), Type::UInt);
        assert!(check.schema.is_none());
        let mut changed = program.clone();
        let words = changed.store.data[assignment].range().bounds(changed.store.extra.len()).unwrap();
        let mut payload = changed.store.extra[words].to_vec();
        assert_eq!(payload.len(), 9);
        assert_eq!((payload[4], payload[7]), (1, 0));
        payload[4] = 0;
        payload.drain(5..8);
        let start = changed.store.extra.len() as u32;
        changed.store.extra.extend_from_slice(&payload);
        changed.store.data[assignment] = crate::runtime::eval::indexed::IrData::new(start, payload.len() as u32);
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic assignment disagrees with its invariant slot contract");
    }

    #[test]
    fn generic_assignment_list_compounds_preserve_the_unsigned_payload_domain() {
        let program = prepare("pure append(value, unsigned: List[UInt], signed: List[Int]) { var items = unsigned; items += unsigned; value }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let value = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Assign { value, .. }) => Some(value.raw() as usize),
            _ => None,
        }).unwrap();
        let changed = retarget_comprehension_operand(&program, owner, scheme, value, 1, 2);
        let changed_decoder = decoder_for_function(&changed.store, owner).unwrap();
        let source = program.store.function_instruction_range(owner).unwrap().find(|instruction| matches!(changed_decoder.expression_view(*instruction), Ok(schema::ExprView::Param(2)))).unwrap();
        assert!(matches!(decoder.expression_view(source).unwrap(), schema::ExprView::Param(1)));
        let scopes = verify_function_schemes(&program.store, &mut effects::EffectCoverage::new(&program.store)).unwrap();
        let scope = scopes[scheme.index()].as_ref().unwrap();
        let mut budget = SchemeInstantiationBudget::new(&changed.store);
        for instruction in [value, source] {
            let fact = changed.store.scheme_body_facts.iter().find(|fact| fact.instruction as usize == instruction).unwrap();
            let actual = decode_scheme_type(&changed.store, scope, fact.ty().unwrap(), &mut budget, 0).unwrap();
            assert_eq!(actual.closed_ground_type(), Some(Type::List(Box::new(Type::Int))));
        }
        let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        for parameter in &program.store.scheme_params[parameters] {
            let original = decode_scheme_type(&program.store, scope, parameter.ty, &mut budget, 0).unwrap();
            let retained = decode_scheme_type(&changed.store, scope, parameter.ty, &mut budget, 0).unwrap();
            assert_eq!(original, retained);
        }
        for fact in program.store.scheme_body_facts.iter().filter(|fact| fact.kind() == FullSchemeBodyFactKind::LocalMutable) {
            assert_eq!(decode_scheme_type(&changed.store, scope, fact.ty().unwrap(), &mut budget, 0).unwrap().closed_ground_type(), Some(Type::List(Box::new(Type::UInt))));
        }
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic compound assignment changes its list domain");
    }

    #[test]
    fn generic_assignment_fields_authenticate_the_actual_storage_domain() {
        let program = prepare("pure replace(value, other, spare) { var row = {item: value, wrong: spare}; row.item = other; let _ = row.wrong; row.item }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let mut range = program.store.function_instruction_range(owner).unwrap();
        let path = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::AssignPath { path, .. }) => Some(path.block_identity_for_test()), _ => None,
        }).unwrap();
        let wrong = range.find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Field { name: "wrong", .. }))).unwrap();
        assert!(matches!(decoder.expression_view(wrong).unwrap(), schema::ExprView::Field { name: "wrong", .. }));
        let wrong_name = program.symbol_owner().with_current(|| Name::intern("wrong").symbol().raw());
        let mut changed = program.clone();
        let words = changed.store.blocks[path.index()].instructions.bounds(changed.store.extra.len()).unwrap();
        assert_eq!(&changed.store.extra[words.start..words.start + 2], &[1, 0]);
        changed.store.extra[words.start + 2] = wrong_name;
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic assignment disagrees with its invariant slot contract");
    }

    #[test]
    fn generic_assignment_unsigned_selectors_require_the_actual_converted_domain() {
        let mut program = prepare("pure put(value, key: UInt, signed: Int) { var items = {[key]: value}; items[key] = value; items }");
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let selector = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::AssignPath { mut path, .. }) => match path.next().unwrap().unwrap() { super::super::super::decode::AssignStepView::Index(index) => Some(index.raw() as usize), _ => None },
            _ => None,
        }).unwrap();
        let schema::ExprView::Try { value, .. } = decoder.expression_view(selector).unwrap() else { panic!("unsigned keys retain their ordinary propagation") };
        let requirement = value.raw() as usize;
        let schema::ExprView::Require { value, check, .. } = decoder.expression_view(requirement).unwrap() else { panic!("unsigned keys retain their ordinary validation") };
        assert_eq!(check.ty.materialize().unwrap(), Type::UInt);
        let mut changed = retarget_comprehension_operand(&program, owner, scheme, value.raw() as usize, 1, 2);
        let params = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        let signed = program.store.scheme_types[program.store.scheme_params[params.start + 2].ty.index()];
        assert_eq!(signed.tag, FullSchemeTypeTag::Ground);
        let words = changed.store.data[requirement].range().bounds(changed.store.extra.len()).unwrap();
        changed.store.extra[words.start + 1] = signed.first;
        for fact in &changed.store.scheme_body_facts {
            if fact.instruction as usize == selector {
                let node = &mut changed.store.scheme_types[fact.ty().unwrap().index()];
                assert_eq!(node.tag, FullSchemeTypeTag::Ground);
                node.first = signed.first;
            } else if fact.instruction as usize == requirement {
                let result = changed.store.scheme_types[fact.ty().unwrap().index()];
                assert_eq!(result.tag, FullSchemeTypeTag::Result);
                let node = &mut changed.store.scheme_types[result.first as usize];
                assert_eq!(node.tag, FullSchemeTypeTag::Ground);
                node.first = signed.first;
            }
        }
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic assignment selector disagrees with its key contract");
    }

    #[test]
    fn generic_comprehension_actual_iterables_authenticate_item_binders() {
        let program = prepare("pure copy(value, other) { [item for item in [value]] }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let source = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::ListComp { mut qualifiers, .. }) => match qualifiers.next().unwrap().unwrap() { super::super::super::decode::CompQualifierView::For { iter, .. } => Some(iter.raw() as usize), _ => None },
            _ => None,
        }).unwrap();
        let changed = retarget_comprehension_operand(&program, owner, scheme, source, 0, 1);
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic body expression disagrees with its checked type");
    }

    #[test]
    fn generic_comprehension_filters_authenticate_actual_control_domains() {
        let program = prepare("pure copy(value, selected: Bool, wrong: Int) { [item for item in [value] if selected] }");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let condition = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::ListComp { mut qualifiers, .. }) => qualifiers.find_map(|qualifier| match qualifier.unwrap() { super::super::super::decode::CompQualifierView::If { condition, .. } => Some(condition.raw() as usize), _ => None }),
            _ => None,
        }).unwrap();
        let changed = retarget_comprehension_operand(&program, owner, scheme, condition, 1, 2);
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic condition disagrees with its control role");
    }

    #[test]
    fn generic_comprehension_keys_and_values_authenticate_actual_domains() {
        for key in [true, false] {
            let source = if key { "pure copy(value, key: Str, wrong: Int) { let _ = {key: value for item in [1]}; value }" } else { "pure copy(value, other) { let _ = [value for item in [1]]; value }" };
            let program = prepare(source);
            let (owner, scheme) = generic_owner(&program);
            let decoder = decoder_for_function(&program.store, owner).unwrap();
            let projection = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
                Ok(schema::ExprView::MapComp { key, .. }) => Some(key.raw() as usize),
                Ok(schema::ExprView::ListComp { value, .. }) => Some(value.raw() as usize),
                _ => None,
            }).unwrap();
            let changed = retarget_comprehension_operand(&program, owner, scheme, projection, if key { 1 } else { 0 }, if key { 2 } else { 1 });
            assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, if key { "generic comprehension key disagrees with its map domain" } else { "generic comprehension value disagrees with its item domain" });
        }
    }

    #[test]
    fn generic_comprehension_bindings_remain_immutable_and_lexically_owned() {
        let program = prepare("proc copy(value) [] { var result = value; let rows = [{ result = item; item } for item in [value]]; result }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let slot = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::ListComp { mut qualifiers, .. }) => match qualifiers.next().unwrap().unwrap() { super::super::super::decode::CompQualifierView::For { target: super::super::super::decode::CompTargetView::Slot(slot), .. } => Some(slot), _ => None },
            _ => None,
        }).unwrap();
        let assignment = range.clone().find(|instruction| matches!(decoder.statement_view(*instruction), Ok(schema::StmtView::Assign { .. }))).unwrap();
        let mut mutable = program.clone();
        let words = mutable.store.data[assignment].range().bounds(mutable.store.extra.len()).unwrap();
        mutable.store.extra[words.start] = slot as u32;
        assert_eq!(FullVerifier::verify(&mutable).unwrap_err().message, "generic assignment does not target a mutable declaration");
        let tail = range.filter_map(|instruction| match decoder.statement_view(instruction) { Ok(schema::StmtView::Return { value }) => Some(value.raw() as usize), _ => None }).last().unwrap();
        let schema::ExprView::Ok(tail) = decoder.expression_view(tail).unwrap() else { panic!("the proc wraps its actual tail") };
        let tail = tail.raw() as usize;
        assert!(matches!(decoder.expression_view(tail).unwrap(), schema::ExprView::Param(_)));
        let mut escaped = program.clone();
        let words = escaped.store.data[tail].range().bounds(escaped.store.extra.len()).unwrap();
        escaped.store.extra[words.start] = slot as u32;
        assert_eq!(FullVerifier::verify(&escaped).unwrap_err().message, "generic body reads an uninitialized slot");
    }

    #[test]
    fn generic_comprehension_early_qualifiers_cannot_read_later_bindings() {
        for filter in [false, true] {
            let source = if filter { "pure copy(value, selected: Bool) { [early for early in [value] if selected for late in [selected]] }" } else { "pure copy(value) { [late for early in [value] for late in [early]] }" };
            let program = prepare(source);
            let (owner, _) = generic_owner(&program);
            let decoder = decoder_for_function(&program.store, owner).unwrap();
            let mut range = program.store.function_instruction_range(owner).unwrap();
            let slot = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
                Ok(schema::ExprView::ListComp { qualifiers, .. }) => qualifiers.filter_map(|qualifier| match qualifier.unwrap() { super::super::super::decode::CompQualifierView::For { target: super::super::super::decode::CompTargetView::Slot(slot), .. } => Some(slot), _ => None }).last(),
                _ => None,
            }).unwrap();
            let from = if filter { 1 } else { 0 };
            let read = range.find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(actual)) if actual == from)).unwrap();
            let mut changed = program.clone();
            let words = changed.store.data[read].range().bounds(changed.store.extra.len()).unwrap();
            changed.store.extra[words.start] = slot as u32;
            assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic body reads an uninitialized slot");
        }
    }

    #[test]
    fn generic_comprehension_validation_requires_the_actual_single_source_mapping() {
        for qualifier in ["if true", "for other in empty"] {
            let mut program = prepare(&format!("pure copy(value, items: NonEmpty[Int], empty: List[Int]) {{ let _ = [item for item in items {qualifier}]; value }}"));
            program.store.checked_expr_types.retain(|fact| fact.is_contract());
            FullVerifier::verify(&program).unwrap();
            let (owner, scheme) = generic_owner(&program);
            let decoder = decoder_for_function(&program.store, owner).unwrap();
            let mut range = program.store.function_instruction_range(owner).unwrap();
            let comprehension = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::ListComp { .. }))).unwrap();
            let (declaration, slot) = range.clone().find_map(|instruction| match decoder.statement_view(instruction) { Ok(schema::StmtView::Let { slot, value, .. }) if value.raw() as usize == comprehension => Some((instruction, slot)), _ => None }).unwrap();
            let read = range.find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(actual)) if actual == slot)).unwrap();
            let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
            let source = program.store.scheme_params[parameters.start + 1].ty;
            let source_type = program.store.scheme_types[source.index()];
            assert_eq!(source_type.tag, FullSchemeTypeTag::Validated);
            let mut changed = program.clone();
            for fact in &mut changed.store.scheme_body_facts {
                if [comprehension, declaration, read].contains(&(fact.instruction as usize)) {
                    assert_eq!(changed.store.scheme_types[fact.ty().unwrap().index()].tag, FullSchemeTypeTag::List);
                    let mut validated = source_type;
                    validated.first = fact.ty().unwrap().raw();
                    let ty = SchemeTypeId::new(changed.store.scheme_types.len()).unwrap();
                    fact.value = match fact.value {
                        super::super::super::schemes::FullSchemeBodyFactValue::Expression(_) => super::super::super::schemes::FullSchemeBodyFactValue::Expression(ty),
                        super::super::super::schemes::FullSchemeBodyFactValue::LocalImmutable(_) => super::super::super::schemes::FullSchemeBodyFactValue::LocalImmutable(ty),
                        super::super::super::schemes::FullSchemeBodyFactValue::LocalMutable(_) => super::super::super::schemes::FullSchemeBodyFactValue::LocalMutable(ty),
                        _ => panic!("the selected fact has an expression or local type"),
                    };
                    changed.store.scheme_types.push(validated);
                }
            }
            assert_eq!(program.store.scheme_types[source.index()], changed.store.scheme_types[source.index()]);
            assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic comprehension cannot establish its claimed validation");
        }
    }

    #[test]
    fn generic_iteration_record_targets_authenticate_actual_field_domains() {
        let program = prepare("pure keep(value, other) { let {item: copy, ..} = {item: value, other}; let _ = copy; value }");
        let (owner, _) = generic_owner(&program);
        let declaration = program.store.function_instruction_range(owner).unwrap().find(|instruction| program.store.tags[*instruction] == super::super::super::FullTag::StmtLetRecord && program.store.scheme_body_facts.iter().any(|fact| fact.instruction as usize == *instruction && fact.kind() == FullSchemeBodyFactKind::LocalImmutable)).unwrap();
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
        let fact = program.store.scheme_body_facts.iter().position(|fact| fact.kind() == FullSchemeBodyFactKind::LocalImmutable && program.store.tags[fact.instruction as usize] == super::super::super::FullTag::StmtLetRecord).unwrap();
        let mut missing = program.clone();
        missing.store.scheme_body_facts[fact].value = super::super::super::schemes::FullSchemeBodyFactValue::Expression(missing.store.scheme_body_facts[fact].ty().unwrap());
        assert_eq!(FullVerifier::verify(&missing).unwrap_err().message, "generic body fact has an invalid instruction kind");
        let duplicate = program.store.scheme_body_facts.iter().position(|other| other.kind() == FullSchemeBodyFactKind::LocalImmutable && other.instruction != program.store.scheme_body_facts[fact].instruction).unwrap();
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
        let changed = retarget_comprehension_operand(&program, owner, scheme, writer, 0, 1);
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic assignment disagrees with its invariant slot contract");
        let mut immutable = program.clone();
        let declaration = immutable.store.scheme_body_facts.iter_mut().find(|fact| fact.kind() == FullSchemeBodyFactKind::LocalMutable).unwrap();
        declaration.value = super::super::super::schemes::FullSchemeBodyFactValue::LocalImmutable(declaration.ty().unwrap());
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
                let ty = program.store.scheme_types[fact.ty().unwrap().index()];
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
                let list = changed.store.scheme_types[fact.ty().unwrap().index()];
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
        let scopes = verify_function_schemes(&program.store, &mut effects::EffectCoverage::new(&program.store)).unwrap();
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
                let result = changed.store.scheme_types[fact.ty().unwrap().index()];
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

    fn argument_default_operand(program: &FullProgram, owner: usize) -> (usize, usize, BTreeSet<usize>) {
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let root = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::DefaultParameter { value, .. }) => Some(value.raw() as usize), _ => None,
        }).unwrap();
        let mut pending = vec![root];
        let mut owned = BTreeSet::new();
        let mut literal = None;
        while let Some(instruction) = pending.pop() {
            assert!(owned.insert(instruction));
            match decoder.expression_view(instruction).unwrap() {
                schema::ExprView::ValueBlock { mut body, .. } => for statement in &mut body {
                    let statement = statement.unwrap().raw() as usize;
                    assert!(owned.insert(statement));
                    let (schema::StmtView::Let { value, .. } | schema::StmtView::Value { value }) = decoder.statement_view(statement).unwrap() else { panic!("the default retains only its ANF declarations and value") };
                    pending.push(value.raw() as usize);
                },
                schema::ExprView::Bool(_) => assert!(literal.replace(instruction).is_none()),
                schema::ExprView::Param(_) => {},
                _ => panic!("the default fixture has one authentic Bool producer"),
            }
        }
        (root, literal.unwrap(), owned)
    }

    #[test]
    fn generic_arguments_defaults_authenticate_actual_values_and_scope() {
        let program = prepare("pure choose(value, flag: Bool = false) { let _ = \"wrong\"; value }");
        let (owner, scheme) = generic_owner(&program);
        let (root, literal, owned) = argument_default_operand(&program, owner);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let wrong = program.store.function_instruction_range(owner).unwrap().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Str(_)))).unwrap();
        let wrong_type = program.store.scheme_body_facts.iter().find(|fact| fact.instruction as usize == wrong).unwrap().ty().unwrap();
        let wrong_node = program.store.scheme_types[wrong_type.index()];
        assert_eq!(wrong_node.tag, FullSchemeTypeTag::Ground);
        let mut changed = program.clone();
        changed.store.tags[literal] = program.store.tags[wrong];
        changed.store.data[literal] = program.store.data[wrong];
        for fact in &program.store.scheme_body_facts {
            if owned.contains(&(fact.instruction as usize)) {
                let node = &mut changed.store.scheme_types[fact.ty().unwrap().index()];
                assert_eq!(node.tag, FullSchemeTypeTag::Ground);
                node.first = wrong_node.first;
            }
        }
        let scopes = verify_function_schemes(&program.store, &mut effects::EffectCoverage::new(&program.store)).unwrap();
        let scope = scopes[scheme.index()].as_ref().unwrap();
        let mut budget = SchemeInstantiationBudget::new(&changed.store);
        let root_type = changed.store.scheme_body_facts.iter().find(|fact| fact.instruction as usize == root).unwrap().ty().unwrap();
        assert_eq!(decode_scheme_type(&changed.store, scope, root_type, &mut budget, 0).unwrap().closed_ground_type(), Some(Type::Str));
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic default value disagrees with its parameter");
        let mut escaped = program.clone();
        escaped.store.tags[literal] = super::super::super::FullTag::ExprParam;
        let start = escaped.store.extra.len() as u32;
        escaped.store.extra.push(1);
        escaped.store.data[literal] = crate::runtime::eval::indexed::IrData::new(start, 1);
        assert_eq!(FullVerifier::verify(&escaped).unwrap_err().message, "generic body reads an uninitialized slot");
    }

    #[test]
    fn generic_arguments_default_entries_require_unique_matching_parameters() {
        let program = prepare("pure choose(value, first: Bool = false, second: Bool = true) { let _ = first; let _ = second; value }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let entries = program.store.function_instruction_range(owner).unwrap().filter(|instruction| matches!(decoder.statement_view(*instruction), Ok(schema::StmtView::DefaultParameter { .. }))).collect::<Vec<_>>();
        assert_eq!(entries.len(), 2);
        let mut changed = program.clone();
        let words = changed.store.data[entries[1]].range().bounds(changed.store.extra.len()).unwrap();
        assert_eq!(changed.store.extra[words.start], 2);
        changed.store.extra[words.start] = 1;
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "parameter default entry does not match its parameter");
        let mut missing = program.clone();
        let parameter = program.store.functions[owner].params.start as usize + 1;
        missing.store.params[parameter].flags &= !4;
        assert_eq!(FullVerifier::verify(&missing).unwrap_err().message, "generic default parameter requires an expression entry");
    }

    #[test]
    fn generic_arguments_actual_payloads_must_fit_the_selected_scheme() {
        let program = prepare("pure head(value, ...items: List[Int]) { let _ = items; value }\npure call(value, other) { let _ = other; head(value, 1, 2) }");
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "call").unwrap();
        let FullCallableContract::Scheme(scheme) = program.store.functions[owner].contract else { panic!("the caller retains its independent payload binders") };
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let argument = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::Call { mut args, .. } | schema::ExprView::DirectPureCall { mut args, .. }) => match args.next().unwrap().unwrap() { super::super::super::decode::CallArgView::Single(value) => Some(value.raw() as usize), _ => None },
            _ => None,
        }).unwrap();
        let changed = retarget_record_operand(&program, owner, scheme, argument, 0, 1);
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic call argument does not match its checked scheme fact");
    }

    #[test]
    fn generic_arguments_actual_splices_must_fit_the_rest_domain() {
        let program = prepare("pure head(value, ...items: List[Int]) { let _ = items; value }\npure call(value, items: List[Int], wrong: List[Str]) { let _ = wrong; head(value, @items) }");
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "call").unwrap();
        let FullCallableContract::Scheme(scheme) = program.store.functions[owner].contract else { panic!("the caller retains its actual rest domains") };
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let argument = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::Call { args, .. } | schema::ExprView::DirectPureCall { args, .. }) => args.filter_map(|arg| match arg.unwrap() { super::super::super::decode::CallArgView::Splice(value) => Some(value.raw() as usize), _ => None }).next(),
            _ => None,
        }).unwrap();
        let (declaration, initializer) = record_operand_binding(&program, owner, argument);
        assert!(matches!(decoder.expression_view(initializer).unwrap(), schema::ExprView::Param(1)));
        let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        let wrong = program.store.scheme_types[program.store.scheme_params[parameters.start + 2].ty.index()];
        assert_eq!(wrong.tag, FullSchemeTypeTag::List);
        let wrong_item = program.store.scheme_types[SchemeTypeId::from_raw(wrong.first).unwrap().index()];
        assert_eq!(wrong_item.tag, FullSchemeTypeTag::Ground);
        let mut changed = program.clone();
        let words = changed.store.data[initializer].range().bounds(changed.store.extra.len()).unwrap();
        changed.store.extra[words.start] = 2;
        for fact in &program.store.scheme_body_facts {
            if [argument, declaration, initializer].contains(&(fact.instruction as usize)) {
                let node = changed.store.scheme_types[fact.ty().unwrap().index()];
                assert_eq!(node.tag, FullSchemeTypeTag::List);
                let item = &mut changed.store.scheme_types[SchemeTypeId::from_raw(node.first).unwrap().index()];
                assert_eq!(item.tag, FullSchemeTypeTag::Ground);
                item.first = wrong_item.first;
            }
        }
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic call argument does not match its checked scheme fact");
    }

    #[test]
    fn generic_arguments_default_calls_keep_the_owner_effect_ceiling() {
        let program = prepare("proc ordinary() [error] -> Int { 7 }\nproc environmental() [env] -> Int { let _ = env.get(\"XSH_GENERIC_DEFAULT\"); 7 }\nproc choose(value, count: Int = ordinary()) [error] { let _ = count; value }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let call = program.store.function_instruction_range(owner).unwrap().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Call { .. }))).unwrap();
        let environmental = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "environmental").unwrap();
        let schema::ExprView::Call { function, .. } = decoder.expression_view(call).unwrap() else { unreachable!() };
        assert_eq!(program.store.string(program.store.functions[function.id.index()].name).unwrap(), "ordinary");
        let mut changed = program.clone();
        let words = changed.store.data[call].range().bounds(changed.store.extra.len()).unwrap();
        changed.store.extra[words.start] = IrFunctionId::new(environmental).unwrap().raw();
        changed.store.checked_call_targets.iter_mut().find(|target| target.instruction as usize == call).unwrap().function = IrFunctionId::new(environmental).unwrap();
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, format!("executable operation requires effect env outside its owner contract at callable {owner}, instruction {call} ExprCall"));
    }

}
