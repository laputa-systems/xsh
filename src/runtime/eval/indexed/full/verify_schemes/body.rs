use super::{SchemeType, BTreeMap, PropagationBoundary, SchemeInstantiationBudget, IrVerifyError, FullDecoder, Arc, schema, FullStore, SchemeScope, body_types_equal, IrFunctionId, FullCallableContract, bounded, decode_scheme_type, BTreeSet, VerifiedSchemeScopes, producers, wrappers, Type, Name, Validation, iteration_contract, body_propagated_error_matches_owner, RefCell, body_type_matches_ground, body_type_matches, FullTag, applications, scheme_conversion, ValueConversion, SchemeRowTail, verify_body_expression, scheme_stored_type};

#[derive(Clone, Copy)]
pub(super) struct LexicalScope {
    frame: usize,
    loop_depth: usize,
    error_boundary: Option<usize>,
}

pub(super) struct InitialBinding {
    pub(super) ty: SchemeType,
    pub(super) mutable: bool,
    pub(super) access: BindingAccess,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum BindingAccess { Value, StaticCallableNamespace }

struct LocalBinding {
    ty: SchemeType,
    mutable: bool,
    initializer: Option<(usize, LexicalScope)>,
    access: BindingAccess,
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
    pub(super) fn captures_error(&self) -> bool { self.error_boundary.is_some() }

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
        let binding = self.binding(scopes, slot, budget)?;
        if binding.is_some_and(|binding| binding.access == BindingAccess::StaticCallableNamespace) { return Err(IrVerifyError::new("generic namespace capture escapes its callable projection")); }
        Ok(binding.map(|binding| &binding.ty))
    }

    pub(super) fn immutable_initializer(
        &self,
        scopes: &LexicalScopes,
        slot: usize,
        budget: &mut SchemeInstantiationBudget,
    ) -> Result<Option<usize>, IrVerifyError> {
        Ok(self.immutable_initializer_context(scopes, slot, budget)?.map(|(instruction, _)| instruction))
    }

    pub(super) fn immutable_slot_type<'a>(
        &self, scopes: &'a LexicalScopes, slot: usize, budget: &mut SchemeInstantiationBudget,
    ) -> Result<Option<&'a SchemeType>, IrVerifyError> {
        Ok(self.binding(scopes, slot, budget)?.filter(|binding| !binding.mutable).map(|binding| &binding.ty))
    }

    pub(super) fn immutable_initializer_context(
        &self, scopes: &LexicalScopes, slot: usize, budget: &mut SchemeInstantiationBudget,
    ) -> Result<Option<(usize, LexicalScope)>, IrVerifyError> {
        let Some(binding) = self.binding(scopes, slot, budget)? else { return Ok(None) };
        if binding.mutable { return Ok(None); }
        Ok(binding.initializer)
    }
}

enum Obligation {
    Expression(usize, LexicalScope),
    ApplicationReturn { instruction: usize, value: usize, site: super::super::schemes::FullSchemeApplicationReturn },
    Publish { statements: Arc<[usize]>, position: usize, scope: LexicalScope, value: Option<SchemeType>, bindings: Vec<(usize, SchemeType, bool, Option<usize>)> },
    GuardPublish { statements: Arc<[usize]>, position: usize, scope: LexicalScope, value: Option<SchemeType>, bindings: Vec<(usize, SchemeType, bool, Option<usize>)>, source: usize, actual: SchemeType, payload: SchemeType },
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
                    .get(ordinal as usize).and_then(|application| application.invocation()).ok_or_else(|| IrVerifyError::new("application return ordinal is out of scope"))?;
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
            schema::ExprView::PatternIf { branches, else_value, .. } => {
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

fn push_completion_contribution(
    contributions: &mut Vec<crate::sema::wrapper_operation::WrapperCompletionContribution<SchemeType>>,
    role: fn(SchemeType) -> crate::sema::wrapper_operation::WrapperCompletionContribution<SchemeType>,
    ty: &SchemeType,
    budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    budget.charge(1)?;
    budget.charge_scheme_clone(ty)?;
    contributions.push(role(ty.clone()));
    Ok(())
}

pub(super) fn completion_result_error(ty: &SchemeType, budget: &mut SchemeInstantiationBudget) -> Result<Option<SchemeType>, IrVerifyError> {
    BodyTypeRef::Scheme(ty).result_parts().map(|(_, error)| error.materialize(budget)).transpose()
}

pub(super) fn completion_result_success(ty: &SchemeType, budget: &mut SchemeInstantiationBudget) -> Result<Option<SchemeType>, IrVerifyError> {
    BodyTypeRef::Scheme(ty).result_parts().map(|(success, _)| success.materialize(budget)).transpose()
}

fn authored_result_constructor(
    decoder: &FullDecoder<'_>,
    types: &BTreeMap<usize, SchemeType>,
    scope: &LexicalScope,
    scopes: &LexicalScopes,
    expression_scopes: &BTreeMap<usize, LexicalScope>,
    visited: &BTreeSet<usize>,
    producer: usize,
    source: &SchemeType,
    form: crate::runtime::eval::SchemeCompletionSourceForm,
    budget: &mut SchemeInstantiationBudget,
) -> Result<usize, IrVerifyError> {
    let is_authored = |instruction| -> Result<bool, IrVerifyError> {
        Ok(matches!((form, decoder.expression_view(instruction)?),
            (crate::runtime::eval::SchemeCompletionSourceForm::AuthoredOk, schema::ExprView::Ok(_))
            | (crate::runtime::eval::SchemeCompletionSourceForm::AuthoredErr, schema::ExprView::Err { .. })))
    };
    let constructor = match decoder.expression_view(producer)? {
        _ if is_authored(producer)? => producer,
        schema::ExprView::Param(slot) => {
            let binding = scope.binding(scopes, slot, budget)?
                .ok_or_else(|| IrVerifyError::new("authored Result completion producer reads an unbound slot"))?;
            if binding.mutable { return Err(IrVerifyError::new("authored Result completion producer reads mutable storage")); }
            let (initializer, initializer_scope) = binding.initializer
                .ok_or_else(|| IrVerifyError::new("authored Result completion producer has no immutable initializer"))?;
            let proved_scope = expression_scopes.get(&initializer)
                .ok_or_else(|| IrVerifyError::new("authored Result completion initializer has no lexical proof"))?;
            if !visited.contains(&initializer)
                || proved_scope.frame != initializer_scope.frame
                || proved_scope.loop_depth != initializer_scope.loop_depth
                || proved_scope.error_boundary != initializer_scope.error_boundary
            {
                return Err(IrVerifyError::new("authored Result completion initializer is outside its checked lexical scope"));
            }
            let initialized = expression_type(types, initializer, budget)?;
            if !body_types_equal(source, &binding.ty) || !body_types_equal(source, &initialized) {
                return Err(IrVerifyError::new("authored Result completion initializer disagrees with its checked binding"));
            }
            if !is_authored(initializer)? {
                return Err(IrVerifyError::new("authored Result completion initializer has the wrong constructor"));
            }
            initializer
        }
        _ => return Err(IrVerifyError::new("authored Result completion source has the wrong constructor")),
    };
    let constructor_type = if constructor == producer { source.clone() } else { expression_type(types, constructor, budget)? };
    if !body_types_equal(source, &constructor_type) {
        return Err(IrVerifyError::new("authored Result completion constructor disagrees with its source type"));
    }
    if BodyTypeRef::Scheme(source).result_parts().is_none() {
        return Err(IrVerifyError::new("authored Result completion producer has no Result type"));
    }
    Ok(constructor)
}

fn completion_source_contribution(
    decoder: &FullDecoder<'_>,
    types: &BTreeMap<usize, SchemeType>,
    scope: &LexicalScope,
    scopes: &LexicalScopes,
    expression_scopes: &BTreeMap<usize, LexicalScope>,
    visited: &BTreeSet<usize>,
    returned_value: usize,
    value: super::super::schemes::FullSchemeBodyCompletionSource,
    budget: &mut SchemeInstantiationBudget,
) -> Result<Option<SchemeType>, IrVerifyError> {
    budget.charge(1)?;
    let producer = value.producer as usize;
    let source = expression_type(types, producer, budget)?;
    match value.form {
        crate::runtime::eval::SchemeCompletionSourceForm::LiftedValue => {
            if BodyTypeRef::Scheme(&source).result_parts().is_some() {
                return Err(IrVerifyError::new("lifted Proc completion source is already a Result"));
            }
            let schema::ExprView::Ok(inner) = decoder.expression_view(returned_value)? else {
                return Err(IrVerifyError::new("lifted Proc completion does not return an Ok value"));
            };
            if inner.raw() as usize != producer {
                return Err(IrVerifyError::new("lifted Proc completion does not wrap its recorded source"));
            }
            let actual = expression_type(types, returned_value, budget)?;
            let (ok, _) = BodyTypeRef::Scheme(&actual).result_parts()
                .ok_or_else(|| IrVerifyError::new("lifted Proc completion has no Result envelope"))?;
            let ok = ok.materialize(budget)?;
            if !body_types_equal(&source, &ok) {
                return Err(IrVerifyError::new("lifted Proc completion changes its source type"));
            }
            Ok(None)
        }
        crate::runtime::eval::SchemeCompletionSourceForm::ReturnedResult => {
            if returned_value != producer
                || matches!(decoder.expression_view(producer)?, schema::ExprView::Ok(_) | schema::ExprView::Err { .. })
                || BodyTypeRef::Scheme(&source).result_parts().is_none()
            {
                return Err(IrVerifyError::new("returned Proc completion source is not the recorded Result producer"));
            }
            Ok(Some(source))
        }
        crate::runtime::eval::SchemeCompletionSourceForm::AuthoredOk => {
            if returned_value != producer {
                return Err(IrVerifyError::new("authored Ok completion source has an invalid producer"));
            }
            authored_result_constructor(decoder, types, scope, scopes, expression_scopes, visited, producer, &source, value.form, budget)?;
            Ok(Some(source))
        }
        crate::runtime::eval::SchemeCompletionSourceForm::AuthoredErr => {
            if returned_value != producer { return Err(IrVerifyError::new("authored Err completion return does not identify its producer")); }
            authored_result_constructor(decoder, types, scope, scopes, expression_scopes, visited, producer, &source, value.form, budget)?;
            Ok(Some(source))
        }
    }
}

fn branch_scope(scope: &LexicalScope, scopes: &mut LexicalScopes, budget: &mut SchemeInstantiationBudget) -> Result<LexicalScope, IrVerifyError> {
    budget.charge(1)?;
    let frame = scopes.frames.len();
    scopes.frames.push(LexicalFrame { parent: Some(scope.frame), bindings: BTreeMap::new() });
    Ok(LexicalScope { frame, loop_depth: scope.loop_depth, error_boundary: scope.error_boundary })
}

fn optional_guard_continuation(
    scope: &LexicalScope, scopes: &mut LexicalScopes, decoder: &FullDecoder<'_>, instruction: usize,
    actual: &SchemeType, payload: &SchemeType, expression_scopes: &BTreeMap<usize, LexicalScope>,
    visited: &BTreeSet<usize>, producer_index: &super::producers::GroundProducerIndex, budget: &mut SchemeInstantiationBudget,
) -> Result<LexicalScope, IrVerifyError> {
    let mut instruction = instruction;
    let (slot, initializer) = loop {
        budget.charge(1)?;
        if !visited.contains(&instruction) { return Err(IrVerifyError::new("optional guard source has no executable proof")); }
        match decoder.expression_view(instruction)? {
            schema::ExprView::Param(slot) => {
                let source_scope = expression_scopes.get(&instruction).ok_or_else(|| IrVerifyError::new("optional guard source has no lexical scope"))?;
                let binding = source_scope.binding(scopes, slot, budget)?.ok_or_else(|| IrVerifyError::new("optional guard source has no lexical binding"))?;
                if binding.access != BindingAccess::Value || !body_types_equal(&binding.ty, actual) { return Ok(*scope); }
                if binding.mutable && !producer_index.mutable_guard_source_is_snapshot(decoder, instruction, slot, budget)? { return Ok(*scope); }
                if scope.binding(scopes, slot, budget)?.is_some_and(|visible| std::ptr::eq(visible, binding)) {
                    break (slot, binding.initializer);
                }
                let Some((initializer, initializer_scope)) = binding.initializer else { return Ok(*scope) };
                let proved_scope = expression_scopes.get(&initializer).ok_or_else(|| IrVerifyError::new("optional guard initializer has no lexical scope"))?;
                if proved_scope.frame != initializer_scope.frame || proved_scope.loop_depth != initializer_scope.loop_depth || proved_scope.error_boundary != initializer_scope.error_boundary {
                    return Err(IrVerifyError::new("optional guard initializer is outside its checked lexical scope"));
                }
                instruction = initializer;
            }
            schema::ExprView::ValueBlock { body, .. } => {
                let mut tail = None;
                for statement in body { budget.charge(1)?; tail = Some(statement?.raw() as usize); }
                let Some(tail) = tail else { return Ok(*scope) };
                let schema::StmtView::Value { value } = decoder.statement_view(tail)? else { return Ok(*scope) };
                instruction = value.raw() as usize;
            }
            _ => return Ok(*scope),
        }
    };
    budget.charge_scheme_clone(payload)?;
    let continuation = branch_scope(scope, scopes, budget)?;
    budget.charge(1)?;
    scopes.frames[continuation.frame].bindings.insert(slot, LocalBinding {
        ty: payload.clone(), mutable: false, initializer, access: BindingAccess::Value,
    });
    Ok(continuation)
}

fn propagation_boundary<'a>(scope: &LexicalScope, scopes: &'a LexicalScopes) -> &'a PropagationBoundary {
    scope.error_boundary.map_or(&scopes.function_boundary, |index| &scopes.error_boundaries[index])
}

fn record_capture_transfers(
    store: &FullStore,
    decoder: &FullDecoder<'_>,
    scheme_scope: &SchemeScope,
    effect_scopes: &VerifiedSchemeScopes,
    wrapper_sites: &BTreeMap<usize, u32>,
    operation_sites: &BTreeMap<usize, u32>,
    instruction: usize,
    scope: &LexicalScope,
    scopes: &LexicalScopes,
    types: &BTreeMap<usize, SchemeType>,
    captures: &mut [producers::CaptureErrorRequest],
    budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    let PropagationBoundary::CaptureRelationship { instruction: capture, request, .. } = propagation_boundary(scope, scopes) else { return Ok(()) };
    let transfers = wrappers::actual_transfer(
        store,
        decoder,
        scheme_scope,
        effect_scopes,
        wrapper_sites.get(&instruction).copied(),
        operation_sites.get(&instruction).copied(),
        instruction,
        decoder.expression_view(instruction)?,
        types,
        budget,
    )?;
    if transfers.is_empty() { return Ok(()) }
    budget.charge(transfers.len())?;
    let request = captures.get_mut(*request).filter(|request| request.instruction == *capture && request.owner == decoder.owner)
        .ok_or_else(|| IrVerifyError::new("captured transfer has no actual capture owner"))?;
    request.transfers.extend(transfers.into_iter().map(|transfer| (instruction, transfer)));
    Ok(())
}

fn declare(scope: &LexicalScope, scopes: &mut LexicalScopes, slot: usize, ty: SchemeType, mutable: bool, initializer: Option<(usize, LexicalScope)>, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    if scope.binding(scopes, slot, budget)?.is_some() { return Err(IrVerifyError::new("generic declaration rewrites a visible slot")); }
    budget.charge(1)?;
    scopes.frames[scope.frame].bindings.insert(slot, LocalBinding { ty, mutable, initializer, access: BindingAccess::Value });
    Ok(())
}

fn condition(types: &BTreeMap<usize, SchemeType>, instruction: usize, role: crate::sema::types::ConditionRole, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    let ty = expression_type(types, instruction, budget)?.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic condition is not a ground truth value"))?;
    if !role.accepts(&ty) { return Err(IrVerifyError::new("generic condition disagrees with its control role")); }
    Ok(())
}

#[derive(Clone, Copy)]
pub(super) enum BodyTypeRef<'a> {
    Scheme(&'a SchemeType),
    Ground(&'a Type),
}

impl<'a> BodyTypeRef<'a> {
    pub(super) fn unvalidated(mut self) -> Self {
        loop {
            self = match self {
                Self::Scheme(SchemeType::Validated(_, base)) => Self::Scheme(base),
                Self::Scheme(SchemeType::Ground(ty)) => Self::Ground(ty),
                Self::Ground(Type::Validated(base)) => Self::Ground(base.base()),
                _ => return self,
            };
        }
    }

    pub(super) fn is_record(self) -> bool {
        matches!(self.unvalidated(), Self::Scheme(SchemeType::Record { .. }) | Self::Scheme(SchemeType::Ground(Type::Record(_))) | Self::Ground(Type::Record(_)))
    }

    pub(super) fn field(self, name: Name) -> Option<Self> {
        match self.unvalidated() {
            Self::Scheme(SchemeType::Record { fields, .. }) => fields.get(&name).map(Self::Scheme),
            Self::Scheme(SchemeType::Ground(ty)) => Self::Ground(ty).field(name),
            Self::Ground(Type::Record(fields)) => fields.get(&name).map(Self::Ground),
            _ => None,
        }
    }

    pub(super) fn list_item(self) -> Option<Self> {
        match self.unvalidated() {
            Self::Scheme(SchemeType::List(item)) => Some(Self::Scheme(item)),
            Self::Ground(Type::List(item)) => Some(Self::Ground(item)),
            _ => None,
        }
    }

    pub(super) fn result_parts(self) -> Option<(Self, Self)> {
        match self.unvalidated() {
            Self::Scheme(SchemeType::Result(ok, error)) => Some((Self::Scheme(ok), Self::Scheme(error))),
            Self::Ground(Type::Result(ok, error)) => Some((Self::Ground(ok), Self::Ground(error))),
            _ => None,
        }
    }

    pub(super) fn map_parts(self) -> Option<(Self, Self)> {
        match self.unvalidated() {
            Self::Scheme(SchemeType::Map(key, value)) => Some((Self::Scheme(key), Self::Scheme(value))),
            Self::Ground(Type::Map(key, value)) => Some((Self::Ground(key), Self::Ground(value))),
            _ => None,
        }
    }

    pub(super) fn validation(self) -> Option<Validation> {
        match self {
            Self::Scheme(SchemeType::Validated(validation, _)) => Some(*validation),
            Self::Scheme(SchemeType::Ground(ty)) => Self::Ground(ty).validation(),
            Self::Ground(Type::Validated(ty)) => Some(ty.validation()),
            _ => None,
        }
    }

    pub(super) fn materialize(self, budget: &mut SchemeInstantiationBudget) -> Result<SchemeType, IrVerifyError> {
        match self {
            Self::Scheme(ty) => { budget.charge_scheme_clone(ty)?; Ok(ty.clone()) }
            Self::Ground(ty) => { budget.charge_clone(ty)?; Ok(SchemeType::Ground(ty.clone())) }
        }
    }
    pub(super) fn optional_payload(self) -> Option<Self> {
        match self.unvalidated() {
            Self::Scheme(SchemeType::Optional(value)) => Some(Self::Scheme(value)),
            Self::Ground(Type::Optional(value)) => Some(Self::Ground(value)),
            _ => None,
        }
    }

    pub(super) fn record_shape(self) -> Option<(usize, crate::sema::schemes::SchemeRowTail)> {
        match self.unvalidated() {
            Self::Scheme(SchemeType::Record { fields, tail }) => Some((fields.len(), *tail)),
            Self::Ground(Type::Record(fields)) => Some((fields.len(), crate::sema::schemes::SchemeRowTail::Closed)),
            _ => None,
        }
    }

    pub(super) fn relationship(self) -> Option<crate::sema::types::SchemeCallableId> {
        match self {
            Self::Scheme(SchemeType::SchemeCallable(id)) | Self::Ground(Type::SchemeCallable(id)) => Some(*id),
            Self::Scheme(SchemeType::Ground(ty)) => Self::Ground(ty).relationship(),
            _ => None,
        }
    }

    pub(super) fn validated_base(self) -> Option<(Validation, Self)> {
        match self {
            Self::Scheme(SchemeType::Validated(validation, base)) => Some((*validation, Self::Scheme(base))),
            Self::Scheme(SchemeType::Ground(ty)) => Self::Ground(ty).validated_base(),
            Self::Ground(Type::Validated(ty)) => Some((ty.validation(), Self::Ground(ty.base()))),
            _ => None,
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

fn iteration_item(store: &FullStore, scheme_scope: &SchemeScope, source: &SchemeType, instruction: usize, boundary: &PropagationBoundary, budget: &mut SchemeInstantiationBudget) -> Result<(SchemeType, Option<super::producers::StreamProducer>, Option<SchemeType>), IrVerifyError> {
    budget.charge(1)?;
    match source {
        SchemeType::Parameter(_) => {
            let iteration = iteration_contract(store, source, scheme_scope, budget)?;
            let propagated = match &iteration.policy {
                crate::sema::schemes::IterationPolicy::IterablePropagates(error) | crate::sema::schemes::IterationPolicy::PipelinePropagates(error) => {
                    body_propagated_error_matches_owner(store, Some(scheme_scope), error, boundary, budget)?;
                    budget.charge_scheme_clone(error)?;
                    Some(error.clone())
                }
                crate::sema::schemes::IterationPolicy::IterableInfallible | crate::sema::schemes::IterationPolicy::PipelineInfallible => None,
                crate::sema::schemes::IterationPolicy::PipelineTransport => return Err(IrVerifyError::new("generic iteration has only a pipeline transport contract")),
            };
            Ok((iteration.item, None, propagated))
        }
        SchemeType::Result(ok, error) => {
            body_propagated_error_matches_owner(store, Some(scheme_scope), error, boundary, budget)?;
            budget.charge_scheme_clone(error)?;
            let (item, stream) = direct_iteration_item(ok, super::producers::StreamProducer::ResultSuccess(instruction), budget)?;
            Ok((item, stream, Some((**error).clone())))
        }
        SchemeType::Ground(Type::Result(ok, error)) => {
            budget.charge_clone(error)?;
            let propagated = SchemeType::Ground((**error).clone());
            body_propagated_error_matches_owner(store, Some(scheme_scope), &propagated, boundary, budget)?;
            budget.charge_clone(error)?;
            budget.charge_clone(ok)?;
            let (item, stream) = direct_iteration_item(&SchemeType::Ground((**ok).clone()), super::producers::StreamProducer::ResultSuccess(instruction), budget)?;
            Ok((item, stream, Some(propagated)))
        }
        SchemeType::Validated(_, source) => iteration_item(store, scheme_scope, source, instruction, boundary, budget),
        _ => {
            let (item, stream) = direct_iteration_item(source, super::producers::StreamProducer::Value(instruction), budget)?;
            Ok((item, stream, None))
        }
    }
}

fn pipeline_iteration_item(store: &FullStore, scheme_scope: &SchemeScope, source: &SchemeType, instruction: usize, budget: &mut SchemeInstantiationBudget) -> Result<(SchemeType, Option<super::producers::StreamProducer>), IrVerifyError> {
    let contract = iteration_contract(store, source, scheme_scope, budget)?;
    if !contract.policy.is_pipeline() { return Err(IrVerifyError::new("generic pipeline source has no pipeline contract")); }
    let stream = match BodyTypeRef::Scheme(source).unvalidated() {
        BodyTypeRef::Scheme(SchemeType::Stream(_)) | BodyTypeRef::Ground(Type::Stream(_)) => Some(super::producers::StreamProducer::Value(instruction)),
        BodyTypeRef::Scheme(SchemeType::Result(ok, _)) if matches!(BodyTypeRef::Scheme(ok).unvalidated(), BodyTypeRef::Scheme(SchemeType::Stream(_)) | BodyTypeRef::Ground(Type::Stream(_))) => Some(super::producers::StreamProducer::ResultSuccess(instruction)),
        BodyTypeRef::Ground(Type::Result(ok, _)) if matches!(ok.unvalidated(), Type::Stream(_)) => Some(super::producers::StreamProducer::ResultSuccess(instruction)),
        _ => None,
    };
    Ok((contract.item, stream))
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
    store: &FullStore, types: &BTreeMap<usize, SchemeType>, scheme_scope: &SchemeScope,
    mut qualifiers: super::super::decode::ListView<'_, crate::runtime::eval::LoweredCompQualifier>,
    scope: &LexicalScope, scopes: &mut LexicalScopes, slot_count: usize,
    writers: &mut BTreeMap<usize, SchemeType>,
    pending: &mut Vec<Obligation>, stream_iterations: &mut Vec<(super::producers::StreamProducer, PropagationBoundary)>,
    completion_group: Option<u32>,
    completion_contributions: &mut Vec<crate::sema::wrapper_operation::WrapperCompletionContribution<SchemeType>>,
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
                let (item, stream_source, propagated_error) = iteration_item(store, scheme_scope, &source, instruction, propagation_boundary(&current, scopes), budget)?;
                if completion_group.is_some() && current.error_boundary.is_none()
                    && let Some(error) = propagated_error
                {
                    push_completion_contribution(completion_contributions, crate::sema::wrapper_operation::WrapperCompletionContribution::PropagatedError, &error, budget)?;
                }
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

fn pipeline_callback_scope(
    scope: &LexicalScope, scopes: &mut LexicalScopes, slot: usize, item: &SchemeType,
    slot_count: usize, writers: &mut BTreeMap<usize, SchemeType>, budget: &mut SchemeInstantiationBudget,
) -> Result<LexicalScope, IrVerifyError> {
    budget.charge_scheme_clone(item)?;
    if slot >= slot_count || writers.insert(slot, item.clone()).is_some() { return Err(IrVerifyError::new("generic pipeline item slot is duplicated or out of bounds")); }
    let inner = branch_scope(scope, scopes, budget)?;
    budget.charge_scheme_clone(item)?;
    declare(&inner, scopes, slot, item.clone(), false, None, budget)?;
    Ok(inner)
}

fn pipeline_configuration_bool(
    decoder: &FullDecoder<'_>, mut instruction: usize, name: &str,
    expression_scopes: &BTreeMap<usize, LexicalScope>, scopes: &LexicalScopes,
    budget: &mut SchemeInstantiationBudget,
) -> Result<Option<bool>, IrVerifyError> {
    let mut field = Some(Name::intern(name));
    let mut seen = BTreeSet::new();
    loop {
        budget.charge(1)?;
        if !seen.insert(instruction) { return Err(IrVerifyError::new("pipeline configuration origin is cyclic")); }
        match decoder.expression_view(instruction)? {
            schema::ExprView::Param(slot) => {
                let scope = expression_scopes.get(&instruction).ok_or_else(|| IrVerifyError::new("pipeline configuration has no proved lexical scope"))?;
                let binding = scope.binding(scopes, slot, budget)?.ok_or_else(|| IrVerifyError::new("pipeline configuration reads an unbound slot"))?;
                if binding.mutable { return Ok(None); }
                let Some((value, _)) = binding.initializer else { return Ok(None); };
                instruction = value;
            }
            schema::ExprView::ValueBlock { body, .. } => {
                let mut tail = None;
                for statement in body { budget.charge(1)?; tail = Some(statement?.raw() as usize); }
                let Some(tail) = tail else { return Ok(None); };
                let schema::StmtView::Value { value } = decoder.statement_view(tail)? else { return Ok(None); };
                instruction = value.raw() as usize;
            }
            schema::ExprView::Record(entries) if field.is_some() => {
                let mut selected = None;
                let mut names = BTreeSet::new();
                for entry in entries {
                    budget.charge(1)?;
                    let super::super::decode::RecordEntryView::Field(name, value) = entry? else { return Ok(None); };
                    if !names.insert(name) { return Err(IrVerifyError::new("pipeline configuration repeats a field")); }
                    if Some(name) == field { selected = Some(value.raw() as usize); }
                }
                let Some(value) = selected else { return Ok(None); };
                instruction = value;
                field = None;
            }
            schema::ExprView::Bool(value) if field.is_none() => return Ok(Some(value)),
            _ => return Ok(None),
        }
    }
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

fn pattern_condition_scope(
    decoder: &FullDecoder<'_>, instruction: usize, captures: super::super::decode::ListView<'_, usize>,
    types: &BTreeMap<usize, SchemeType>, scope: &LexicalScope, scopes: &mut LexicalScopes,
    slot_count: usize, writers: &mut BTreeMap<usize, SchemeType>, pending: &mut Vec<Obligation>,
    visited: &mut BTreeSet<usize>, expression_scopes: &mut BTreeMap<usize, LexicalScope>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<LexicalScope, IrVerifyError> {
    condition(types, instruction, crate::sema::types::ConditionRole::Control, budget)?;
    budget.charge(captures.len())?;
    let mut declared = BTreeSet::new();
    for slot in captures {
        if !declared.insert(slot?) { return Err(IrVerifyError::new("generic pattern condition repeats a capture")); }
    }
    if declared.is_empty() {
        pending.push(Obligation::Expression(instruction, *scope));
        return branch_scope(scope, scopes, budget);
    }
    let schema::ExprView::MatchExpr { value, arms, .. } = decoder.expression_view(instruction)? else { return Err(IrVerifyError::new("generic pattern condition has no match producer")) };
    if expression_type(types, instruction, budget)?.closed_ground_type() != Some(Type::Bool) { return Err(IrVerifyError::new("generic pattern condition has no boolean result")); }
    if !visited.insert(instruction) { return Err(IrVerifyError::new("generic pattern condition is shared or cyclic")); }
    expression_scopes.insert(instruction, *scope);
    let mut subject = expression_type(types, value.raw() as usize, budget)?;
    let mut selected = None;
    for arm in arms {
        budget.charge(1)?;
        let (pattern, guard, result) = arm?;
        if guard.is_some() { return Err(IrVerifyError::new("generic pattern condition has an unowned guard")); }
        let schema::ExprView::Bool(success) = decoder.expression_view(result.raw() as usize)? else { return Err(IrVerifyError::new("generic pattern condition has a nonliteral result")) };
        let inner = pattern_scope(decoder, pattern.raw(), &subject, scope, scopes, slot_count, writers, budget)?;
        let bindings = &scopes.frames[inner.frame].bindings;
        budget.charge(bindings.len())?;
        if success {
            if bindings.keys().copied().collect::<BTreeSet<_>>() != declared || selected.replace(inner).is_some() { return Err(IrVerifyError::new("generic pattern condition captures disagree with its success producer")); }
        } else if !bindings.is_empty() {
            return Err(IrVerifyError::new("generic pattern condition binds on failure"));
        }
        if expression_type(types, result.raw() as usize, budget)?.closed_ground_type() != Some(Type::Bool) { return Err(IrVerifyError::new("generic pattern condition arm has no boolean contract")); }
        pending.push(Obligation::Expression(result.raw() as usize, inner));
        subject = subject_after_pattern(decoder, pattern.raw(), false, subject, budget)?;
    }
    pending.push(Obligation::Expression(value.raw() as usize, *scope));
    selected.ok_or_else(|| IrVerifyError::new("generic pattern condition has no successful capture producer"))
}

pub(super) struct BodyProof {
    pub(super) expressions: BTreeSet<usize>,
    pub(super) writers: BTreeMap<usize, SchemeType>,
    pub(super) statements: BTreeSet<usize>,
    pub(super) streams: Vec<(super::producers::StreamProducer, PropagationBoundary)>,
    pub(super) captures: Vec<producers::CaptureErrorRequest>,
    pub(super) applications: BTreeSet<usize>,
    pub(super) checked_accumulators: Vec<usize>,
    pub(super) completion_contributions: Vec<crate::sema::wrapper_operation::WrapperCompletionContribution<SchemeType>>,
    pub(super) callable_projections: Vec<(usize, Type)>,
}

fn with_error_types(decoder: &FullDecoder<'_>, types: &BTreeMap<usize, SchemeType>, budget: &mut SchemeInstantiationBudget) -> Result<BTreeMap<usize, SchemeType>, IrVerifyError> {
    let work = RefCell::new(&mut *budget);
    decoder.with_initializer_error_types_with(|source| {
        let instruction = match source { super::super::WithErrorSource::Initializer(instruction) | super::super::WithErrorSource::Propagation(instruction) => instruction };
        let ty = types.get(&instruction).ok_or_else(|| IrVerifyError::new("generic With source has no checked expression contract"))?;
        BodyTypeRef::Scheme(ty).result_parts().map(|(_, error)| error.materialize(&mut work.borrow_mut())).transpose()
    }, SchemeType::Ground(Type::Error), |current, next| {
        *current = Some(match current.take() { None => next, Some(previous) if previous == next => previous, Some(_) => SchemeType::Ground(Type::Error) });
    }, |amount| work.borrow_mut().charge(amount))
}

fn context_scope_has_known_escape(ty: &SchemeType, budget: &mut SchemeInstantiationBudget) -> Result<bool, IrVerifyError> {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        budget.charge(1)?;
        match ty {
            SchemeType::Ground(ty) => {
                budget.charge_clone(ty)?;
                if !ty.can_escape_context_scope() { return Ok(true); }
            }
            SchemeType::Stream(_) => return Ok(true),
            SchemeType::List(inner) | SchemeType::Optional(inner) | SchemeType::Validated(_, inner) => pending.push(inner),
            SchemeType::Map(_, value) => pending.push(value),
            SchemeType::Result(ok, error) => { pending.push(ok); pending.push(error); }
            SchemeType::Record { fields, .. } => { budget.charge(fields.len())?; pending.extend(fields.values()); }
            SchemeType::Union(members) => { budget.charge(members.len())?; pending.extend(members); }
            SchemeType::SchemeCallable(_) | SchemeType::Parameter(_) | SchemeType::Callable { .. } | SchemeType::Set(_) => {},
        }
    }
    Ok(false)
}

fn scoped_block_value(store: &FullStore, statements: &[usize], payload: SchemeType, budget: &mut SchemeInstantiationBudget) -> Result<Option<SchemeType>, IrVerifyError> {
    if statements.last().is_some_and(|statement| store.tags[*statement] == FullTag::StmtValue) { return Ok(Some(payload)); }
    if !body_type_matches(&SchemeType::Ground(Type::Unit), &payload, budget)? {
        return Err(IrVerifyError::new("generic scoped block fallthrough disagrees with its value contract"));
    }
    Ok(None)
}

pub(super) fn verify_body(
    store: &FullStore, decoder: &FullDecoder<'_>, types: &BTreeMap<usize, SchemeType>,
    constructors: &super::constructors::ConstructorFacts,
    operation_sites: &BTreeMap<usize, u32>,
    method_sites: &BTreeMap<usize, u32>,
    wrapper_sites: &BTreeMap<usize, u32>,
    completion_sources: &mut BTreeMap<usize, super::super::schemes::FullSchemeBodyCompletionSource>,
    tails: &mut BTreeMap<usize, SchemeType>, declarations: &mut BTreeMap<usize, (SchemeType, bool)>,
    slots: BTreeMap<usize, InitialBinding>, scheme_scope: &SchemeScope, effect_scopes: &VerifiedSchemeScopes, expected_return: &SchemeType, completion_group: Option<u32>, statements: Vec<usize>,
    analysis: &super::super::IndexedReturnAnalysis, producer_index: &super::producers::GroundProducerIndex, registered: &mut BTreeSet<u32>, budget: &mut SchemeInstantiationBudget, constant_budget: &mut SchemeInstantiationBudget,
) -> Result<BodyProof, IrVerifyError> {
    let mut with_errors = None;
    let mut checked_accumulators = Vec::new();
    let slot_count = decoder.slot_count as usize;
    for binding in slots.values() { budget.charge_scheme_clone(&binding.ty)?; }
    let mut writers = slots.iter().map(|(&slot, binding)| (slot, binding.ty.clone())).collect::<BTreeMap<_, _>>();
    let function_boundary = PropagationBoundary::for_scheme_function(store, decoder.owner, expected_return, scheme_scope, completion_group, budget)?;
    let namespace_slots = slots.iter().filter_map(|(&slot, binding)| (binding.access == BindingAccess::StaticCallableNamespace).then_some(slot)).collect::<BTreeSet<_>>();
    let mut scopes = LexicalScopes { frames: vec![LexicalFrame { parent: None, bindings: slots.into_iter().map(|(slot, binding)| (slot, LocalBinding { ty: binding.ty, mutable: binding.mutable, initializer: None, access: binding.access })).collect() }], error_boundaries: Vec::new(), function_boundary };
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
            default_bindings.insert(slot, LocalBinding { ty: binding.ty.clone(), mutable: binding.mutable, initializer: None, access: binding.access });
        }
    }
    let default_scope = LexicalScope { frame: scopes.frames.len(), loop_depth: 0, error_boundary: None };
    scopes.frames.push(LexicalFrame { parent: None, bindings: default_bindings });
    let mut pending = vec![Obligation::Block { statements: statements.into(), position: 0, scope, value: None }];
    let mut visited = BTreeSet::new();
    let mut visited_statements = BTreeSet::new();
    let mut stream_iterations = Vec::new();
    let mut batch_argv_checks = Vec::new();
    let mut captures = Vec::new();
    let mut callable_projections = Vec::new();
    let mut projected_namespaces = BTreeSet::new();
    let mut applications = BTreeSet::new();
    let mut application_returns = BTreeSet::new();
    let mut completion_contributions = Vec::new();
    let mut expression_scopes = BTreeMap::new();
    let mut return_origins = BTreeMap::new();
    while let Some(obligation) = pending.pop() {
        budget.charge(1)?;
        match obligation {
            Obligation::GuardPublish { statements, position, scope, value, bindings, source, actual, payload } => {
                let scope = optional_guard_continuation(&scope, &mut scopes, decoder, source, &actual, &payload, &expression_scopes, &visited, producer_index, budget)?;
                pending.push(Obligation::Publish { statements, position, scope, value, bindings });
            }
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
                    schema::ExprView::Field { base, name, .. }
                        if types.get(&(base.raw() as usize)).and_then(SchemeType::closed_ground_type).is_some_and(|ty| matches!(ty, Type::Module(_))) =>
                    {
                        let base = base.raw() as usize;
                        let schema::ExprView::Param(slot) = decoder.expression_view(base)? else { return Err(IrVerifyError::new("generic callable projection has no captured namespace read")); };
                        let binding = scope.binding(&scopes, slot, budget)?.ok_or_else(|| IrVerifyError::new("generic callable projection has no namespace binding"))?;
                        if binding.mutable || binding.access != BindingAccess::StaticCallableNamespace { return Err(IrVerifyError::new("generic callable projection has no static namespace authority")); }
                        if !body_types_equal(&expression_type(types, base, budget)?, &binding.ty) { return Err(IrVerifyError::new("generic callable projection changes its actual namespace type")); }
                        let Some(Type::Module(module)) = binding.ty.closed_ground_type() else { return Err(IrVerifyError::new("generic callable projection has no exact module contract")); };
                        if !module.exact { return Err(IrVerifyError::new("generic namespace projection has no exact module contract")); }
                        let actual = claimed.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic callable projection has no closed callable contract"))?;
                        match (module.get(&Name::intern(name)), &actual) {
                            (Some(crate::sema::types::ModuleExportType::GenericCallable { kind, optional: false }), Type::SchemeCallable(id)) => {
                                let relationship = store.semantic.scheme_callable_index(*id)?;
                                let relationship = store.scheme_callable_values.get(relationship as usize).ok_or_else(|| IrVerifyError::new("generic namespace relationship is out of bounds"))?;
                                if store.function_metadata.get(relationship.canonical.index()).is_none_or(|target| target.callable_kind != *kind) {
                                    return Err(IrVerifyError::new("generic namespace projection changes its export kind"));
                                }
                            }
                            (Some(export), actual) if export.static_callable_value_type().as_ref() == Some(actual) => {}
                            _ => return Err(IrVerifyError::new("generic callable projection disagrees with its actual export signature")),
                        }
                        if !visited.insert(instruction) || !visited.insert(base) { return Err(IrVerifyError::new("generic callable projection is shared or cyclic")); }
                        expression_scopes.insert(base, scope);
                        projected_namespaces.insert(slot);
                        callable_projections.push((instruction, actual));
                    }
                    schema::ExprView::DynamicCall { .. } => {
                        if !visited.insert(instruction) { return Err(IrVerifyError::new("generic expression is shared or cyclic")); }
                        let children = if store.scheme_callable_calls.binary_search_by_key(&(instruction as u32), |site| site.instruction).is_ok() {
                            super::scheme_values::verify_call_site(store, decoder, instruction, effect_scopes, scope.error_boundary.is_some(), types, budget)?
                        } else {
                            applications.insert(instruction);
                            applications::verify_site(store, decoder, instruction, scheme_scope, effect_scopes, scope.error_boundary.is_some(), types, budget)?
                        };
                        record_capture_transfers(store, decoder, scheme_scope, effect_scopes, wrapper_sites, operation_sites, instruction, &scope, &scopes, types, &mut captures, budget)?;
                        for child in children {
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
                        captures.push(producers::CaptureErrorRequest { owner: decoder.owner, instruction, expected_error: error.clone(), wrapper: wrapper_sites.get(&instruction).copied(), transfers: Vec::new() });
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
                        scopes.error_boundaries.push(wrappers::capture_boundary(store, scheme_scope, wrapper_sites.get(&instruction).copied(), instruction, captures.len().checked_sub(1), &error, budget)?);
                        pending.push(Obligation::Block { statements: statements.into(), position: 0, scope: inner, value });
                    }
                    schema::ExprView::ErrorContext { message, body, .. } => {
                        if !visited.insert(instruction) { return Err(IrVerifyError::new("generic expression is shared or cyclic")); }
                        if !body_type_matches(&expression_type(types, message.raw() as usize, budget)?, &SchemeType::Ground(Type::Str), budget)? {
                            return Err(IrVerifyError::new("generic error context message is not Str"));
                        }
                        pending.push(Obligation::Expression(message.raw() as usize, scope));
                        let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                        budget.charge(statements.len())?;
                        let value = scoped_block_value(store, &statements, claimed, budget)?;
                        pending.push(Obligation::Block { statements: statements.into(), position: 0, scope: branch_scope(&scope, &mut scopes, budget)?, value });
                    }
                    view @ (schema::ExprView::ContextScope { .. } | schema::ExprView::Retry { .. }) => {
                        if !visited.insert(instruction) { return Err(IrVerifyError::new("generic expression is shared or cyclic")); }
                        if BodyTypeRef::Scheme(&claimed).validation().is_some() { return Err(IrVerifyError::new("generic scoped expression cannot establish a validation")); }
                        let (ok, error) = BodyTypeRef::Scheme(&claimed).result_parts().ok_or_else(|| IrVerifyError::new("generic scoped expression has no Result contract"))?;
                        let ok = ok.materialize(budget)?;
                        let error = error.materialize(budget)?;
                        let (body, retry) = match view {
                            schema::ExprView::ContextScope { kind, input, body, .. } => {
                                if store.function_metadata[owner.index()].callable_kind == crate::sema::types::CallableKind::Pure {
                                    return Err(IrVerifyError::new("generic context scope has a Pure owner"));
                                }
                                let input_type = expression_type(types, input.raw() as usize, budget)?;
                                budget.charge_scheme_clone(&input_type)?;
                                if kind == crate::syntax::arena::ContextScopeKind::Env {
                                    super::commands::environment_overlay(&input_type, scheme_scope, budget)?;
                                } else {
                                    let input_type = input_type.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic context scope input is unresolved"))?;
                                    if !input_type.accepts_context_scope_input(kind) { return Err(IrVerifyError::new("generic context scope input disagrees with its kind")); }
                                }
                                if !body_type_matches(&SchemeType::Ground(Type::Error), &error, budget)? { return Err(IrVerifyError::new("generic context scope has an incompatible error contract")); }
                                if context_scope_has_known_escape(&ok, budget)? {
                                    return Err(IrVerifyError::new("generic context scope result escapes its restored context"));
                                }
                                pending.push(Obligation::Expression(input.raw() as usize, scope));
                                (body, false)
                            }
                            schema::ExprView::Retry { delays, body, .. } => {
                                for delay in delays {
                                    budget.charge(1)?;
                                    let delay = delay?.raw() as usize;
                                    if !body_type_matches(&expression_type(types, delay, budget)?, &SchemeType::Ground(Type::Duration), budget)? {
                                        return Err(IrVerifyError::new("generic retry delay is not Duration"));
                                    }
                                    pending.push(Obligation::Expression(delay, scope));
                                }
                                captures.push(producers::CaptureErrorRequest { owner: decoder.owner, instruction, expected_error: error.clone(), wrapper: wrapper_sites.get(&instruction).copied(), transfers: Vec::new() });
                                (body, true)
                            }
                            _ => unreachable!(),
                        };
                        let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                        budget.charge(statements.len())?;
                        let preserves_result = if retry {
                            match statements.last().map(|statement| decoder.statement_view(*statement)).transpose()? {
                                Some(schema::StmtView::Value { value }) => matches!(expression_type(types, value.raw() as usize, budget)?, SchemeType::Result(..) | SchemeType::Ground(Type::Result(..))),
                                _ => false,
                            }
                        } else { false };
                        let payload = if preserves_result {
                            budget.charge_scheme_clone(&claimed)?;
                            claimed.clone()
                        } else { ok };
                        let value = scoped_block_value(store, &statements, payload, budget)?;
                        let mut inner = branch_scope(&scope, &mut scopes, budget)?;
                        inner.error_boundary = Some(scopes.error_boundaries.len());
                        scopes.error_boundaries.push(wrappers::capture_boundary(store, scheme_scope, wrapper_sites.get(&instruction).copied(), instruction, if retry { captures.len().checked_sub(1) } else { None }, &error, budget)?);
                        pending.push(Obligation::Block { statements: statements.into(), position: 0, scope: inner, value });
                    }
                    schema::ExprView::ValueBlock { body, .. } => {
                        if !visited.insert(instruction) { return Err(IrVerifyError::new("generic expression is shared or cyclic")); }
                        let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                        budget.charge(statements.len())?;
                        pending.push(Obligation::Block { statements: statements.into(), position: 0, scope: branch_scope(&scope, &mut scopes, budget)?, value: Some(claimed) });
                    }
                    schema::ExprView::ListPipeline { input, mut stages, .. } => {
                        if !visited.insert(instruction) { return Err(IrVerifyError::new("generic expression is shared or cyclic")); }
                        let source = expression_type(types, input.raw() as usize, budget)?;
                        let position = super::super::decode::ListView::position(&stages);
                        let first = stages.next_with_id().transpose()?;
                        stages.resume(position)?;
                        let adapter = first.is_some_and(|stage| matches!(stage.view, super::super::StageView::TextLines | super::super::StageView::BytesChunks { .. } | super::super::StageView::JsonLines));
                        let (mut item, stream) = if adapter {
                            budget.charge_scheme_clone(&source)?;
                            (source.clone(), None)
                        } else { pipeline_iteration_item(store, scheme_scope, &source, input.raw() as usize, budget)? };
                        let mut terminal = None;
                        let mut stage_ids = Vec::new();
                        while let Some(stage) = stages.next_with_id() {
                            budget.charge(1)?;
                            if terminal.is_some() { return Err(IrVerifyError::new("generic pipeline continues after a terminal stage")); }
                            let stage = stage?;
                            stage_ids.push(stage.id.raw());
                            let each = matches!(&stage.view, super::super::StageView::Each { .. });
                            match stage.view {
                                super::super::StageView::TextLines | super::super::StageView::BytesChunks { .. } | super::super::StageView::JsonLines => {
                                    if stage_ids.len() != 1 { return Err(IrVerifyError::new("generic pipeline adapter is not the first stage")); }
                                    let input_type = if matches!(stage.view, super::super::StageView::BytesChunks { .. }) { Type::Bytes } else { Type::Str };
                                    if !body_type_matches(&source, &SchemeType::Ground(input_type), budget)? { return Err(IrVerifyError::new("generic pipeline adapter has the wrong source type")); }
                                    if let super::super::StageView::BytesChunks { size } = stage.view {
                                        if !body_type_matches(&expression_type(types, size.raw() as usize, budget)?, &SchemeType::Ground(Type::Int), budget)? { return Err(IrVerifyError::new("generic byte chunk size is not Int")); }
                                        pending.push(Obligation::Expression(size.raw() as usize, scope));
                                    }
                                    item = SchemeType::Ground(match stage.view { super::super::StageView::TextLines => Type::Str, super::super::StageView::BytesChunks { .. } => Type::Bytes, _ => Type::Any });
                                }
                                super::super::StageView::Map { slot, value }
                                | super::super::StageView::ParMap { slot, value, .. } => {
                                    let inner = pipeline_callback_scope(&scope, &mut scopes, slot, &item, slot_count, &mut writers, budget)?;
                                    if let super::super::StageView::ParMap { jobs: Some(jobs), .. } = stage.view {
                                        if !body_type_matches(&expression_type(types, jobs.raw() as usize, budget)?, &SchemeType::Ground(Type::Int), budget)? { return Err(IrVerifyError::new("generic parallel pipeline jobs is not Int")); }
                                        pending.push(Obligation::Expression(jobs.raw() as usize, scope));
                                    }
                                    item = expression_type(types, value.raw() as usize, budget)?;
                                    if matches!(BodyTypeRef::Scheme(&item).unvalidated(), BodyTypeRef::Ground(Type::Unit)) { return Err(IrVerifyError::new("generic map callback has no value result")); }
                                    pending.push(Obligation::Expression(value.raw() as usize, inner));
                                }
                                super::super::StageView::GroupBy { slot, key }
                                | super::super::StageView::UniqueBy { slot, key }
                                | super::super::StageView::SortBy { slot, key, .. }
                                | super::super::StageView::CountBy { slot, key } => {
                                    let inner = pipeline_callback_scope(&scope, &mut scopes, slot, &item, slot_count, &mut writers, budget)?;
                                    let key_type = expression_type(types, key.raw() as usize, budget)?;
                                    match stage.view {
                                        super::super::StageView::SortBy { descending, .. } => {
                                            super::operations::verify_key_domain(store, scheme_scope, crate::sema::sealed_operation::TypeDomain::SortKey, &key_type, budget)?;
                                            if let Some(descending) = descending {
                                                if !body_type_matches(&expression_type(types, descending.raw() as usize, budget)?, &SchemeType::Ground(Type::Bool), budget)? { return Err(IrVerifyError::new("generic sort direction is not Bool")); }
                                                pending.push(Obligation::Expression(descending.raw() as usize, scope));
                                            }
                                        }
                                        super::super::StageView::GroupBy { .. } => {
                                            budget.charge_scheme_clone(&item)?;
                                            budget.charge(4)?;
                                            item = SchemeType::Record { fields: BTreeMap::from([(Name::intern("key"), key_type), (Name::intern("items"), SchemeType::List(Box::new(item.clone())))]), tail: crate::sema::schemes::SchemeRowTail::Closed };
                                        }
                                        super::super::StageView::CountBy { .. } => {
                                            super::operations::verify_key_domain(store, scheme_scope, crate::sema::sealed_operation::TypeDomain::CountKey, &key_type, budget)?;
                                            budget.charge(3)?;
                                            terminal = Some(SchemeType::Map(Box::new(SchemeType::Ground(Type::Str)), Box::new(SchemeType::Ground(Type::Int))));
                                        }
                                        _ => {}
                                    }
                                    pending.push(Obligation::Expression(key.raw() as usize, inner));
                                }
                                super::super::StageView::Each { slot, body }
                                | super::super::StageView::Tee { slot, body } => {
                                    let inner = pipeline_callback_scope(&scope, &mut scopes, slot, &item, slot_count, &mut writers, budget)?;
                                    let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                                    budget.charge(statements.len())?;
                                    pending.push(Obligation::Block { statements: statements.into(), position: 0, scope: inner, value: None });
                                    if each { terminal = Some(SchemeType::Ground(Type::Unit)); }
                                }
                                super::super::StageView::Fold { acc_slot, item_slot, initial, body, value } => {
                                    let accumulator = expression_type(types, initial.raw() as usize, budget)?;
                                    let inner = pipeline_callback_scope(&scope, &mut scopes, acc_slot, &accumulator, slot_count, &mut writers, budget)?;
                                    let inner = pipeline_callback_scope(&inner, &mut scopes, item_slot, &item, slot_count, &mut writers, budget)?;
                                    if !body_type_matches(&expression_type(types, value.raw() as usize, budget)?, &accumulator, budget)? { return Err(IrVerifyError::new("generic fold callback disagrees with its accumulator")); }
                                    pending.push(Obligation::Expression(initial.raw() as usize, scope));
                                    if body.len() != 0 { return Err(IrVerifyError::new("generic fold callback has detached statements")); }
                                    pending.push(Obligation::Expression(value.raw() as usize, inner));
                                    terminal = Some(accumulator);
                                }
                                super::super::StageView::ReduceBy { item_slot, ref body, value, .. }
                                | super::super::StageView::ReduceByConfigured { item_slot, ref body, value, .. } => {
                                    let inner = pipeline_callback_scope(&scope, &mut scopes, item_slot, &item, slot_count, &mut writers, budget)?;
                                    let row = expression_type(types, value.raw() as usize, budget)?;
                                    let row = BodyTypeRef::Scheme(&row).unvalidated();
                                    if row.record_shape().is_none() || row.field(Name::intern("key")).is_none() { return Err(IrVerifyError::new("generic reduce-by callback has no key record")); }
                                    let value_type = row.field(Name::intern("value")).ok_or_else(|| IrVerifyError::new("generic reduce-by callback has no value field"))?.materialize(budget)?;
                                    match stage.view {
                                        super::super::StageView::ReduceBy { jobs: Some(jobs), .. } => {
                                            if !body_type_matches(&expression_type(types, jobs.raw() as usize, budget)?, &SchemeType::Ground(Type::Int), budget)? { return Err(IrVerifyError::new("generic reduce-by jobs is not Int")); }
                                            pending.push(Obligation::Expression(jobs.raw() as usize, scope));
                                        }
                                        super::super::StageView::ReduceByConfigured { configuration, .. } => {
                                            let ty = expression_type(types, configuration.raw() as usize, budget)?;
                                            let configuration_type = BodyTypeRef::Scheme(&ty).unvalidated();
                                            let Some((size, SchemeRowTail::Closed)) = configuration_type.record_shape() else { return Err(IrVerifyError::new("generic reduce-by configuration has no closed record")); };
                                            let mut fields = 0;
                                            for (name, expected) in [("sum", Type::Bool), ("min", Type::Bool), ("max", Type::Bool), ("jobs", Type::Int)] {
                                                if let Some(field) = configuration_type.field(Name::intern(name)) {
                                                    fields += 1;
                                                    if !body_type_matches(&field.materialize(budget)?, &SchemeType::Ground(expected), budget)? { return Err(IrVerifyError::new("generic reduce-by configuration field has the wrong type")); }
                                                }
                                            }
                                            if fields != size { return Err(IrVerifyError::new("generic reduce-by configuration has an unknown field")); }
                                            pending.push(Obligation::Expression(configuration.raw() as usize, scope));
                                        }
                                        _ => {}
                                    }
                                    if body.len() != 0 { return Err(IrVerifyError::new("generic reduce-by callback has detached statements")); }
                                    pending.push(Obligation::Expression(value.raw() as usize, inner));
                                    terminal = Some(SchemeType::Map(Box::new(SchemeType::Ground(Type::Str)), Box::new(value_type)));
                                }
                                super::super::StageView::FlatMap { slot, value } => {
                                    let inner = pipeline_callback_scope(&scope, &mut scopes, slot, &item, slot_count, &mut writers, budget)?;
                                    let source = expression_type(types, value.raw() as usize, budget)?;
                                    if !matches!(BodyTypeRef::Scheme(&source).unvalidated(), BodyTypeRef::Scheme(SchemeType::List(_) | SchemeType::Stream(_)) | BodyTypeRef::Ground(Type::List(_) | Type::Stream(_))) {
                                        return Err(IrVerifyError::new("generic flat-map callback has no list or stream contract"));
                                    }
                                    let (mapped, stream) = direct_iteration_item(&source, super::producers::StreamProducer::Value(value.raw() as usize), budget)?;
                                    item = mapped;
                                    if let Some(stream) = stream { stream_iterations.push((stream, propagation_boundary(&inner, &scopes).clone())); }
                                    pending.push(Obligation::Expression(value.raw() as usize, inner));
                                }
                                super::super::StageView::Where { slot, predicate }
                                | super::super::StageView::Any { slot, predicate }
                                | super::super::StageView::All { slot, predicate } => {
                                    let inner = pipeline_callback_scope(&scope, &mut scopes, slot, &item, slot_count, &mut writers, budget)?;
                                    condition(types, predicate.raw() as usize, crate::sema::types::ConditionRole::Boolean, budget)?;
                                    if !matches!(stage.view, super::super::StageView::Where { .. }) { terminal = Some(SchemeType::Ground(Type::Bool)); }
                                    pending.push(Obligation::Expression(predicate.raw() as usize, inner));
                                }
                                super::super::StageView::Enumerate => {
                                    budget.charge(3)?;
                                    item = SchemeType::Record { fields: BTreeMap::from([(Name::intern("index"), SchemeType::Ground(Type::Int)), (Name::intern("value"), item)]), tail: crate::sema::schemes::SchemeRowTail::Closed };
                                }
                                super::super::StageView::Sort { descending } => {
                                    super::operations::verify_key_domain(store, scheme_scope, crate::sema::sealed_operation::TypeDomain::SortKey, &item, budget)?;
                                    if let Some(descending) = descending {
                                        if !body_type_matches(&expression_type(types, descending.raw() as usize, budget)?, &SchemeType::Ground(Type::Bool), budget)? { return Err(IrVerifyError::new("generic sort direction is not Bool")); }
                                        pending.push(Obligation::Expression(descending.raw() as usize, scope));
                                    }
                                }
                                super::super::StageView::BatchCount { count }
                                | super::super::StageView::BatchMaxBytes { max_bytes: count } => {
                                    if !body_type_matches(&expression_type(types, count.raw() as usize, budget)?, &SchemeType::Ground(Type::Int), budget)? { return Err(IrVerifyError::new("generic batch limit is not Int")); }
                                    if matches!(stage.view, super::super::StageView::BatchMaxBytes { .. }) && !scheme_conversion(&item, ValueConversion::Argv, scheme_scope, budget)? { return Err(IrVerifyError::new("generic byte-bounded batch item has no argv conversion")); }
                                    pending.push(Obligation::Expression(count.raw() as usize, scope));
                                    budget.charge(1)?;
                                    item = SchemeType::List(Box::new(item));
                                }
                                super::super::StageView::BatchMaxArgv { max_argv } => {
                                    if !scheme_conversion(&item, ValueConversion::Argv, scheme_scope, budget)? { return Err(IrVerifyError::new("generic argv-bounded batch item has no argv conversion")); }
                                    if let Some(limit) = max_argv {
                                        if !body_type_matches(&expression_type(types, limit.raw() as usize, budget)?, &SchemeType::Ground(Type::Int), budget)? { return Err(IrVerifyError::new("generic batch argv limit is not Int")); }
                                        pending.push(Obligation::Expression(limit.raw() as usize, scope));
                                    }
                                    budget.charge(1)?;
                                    item = SchemeType::List(Box::new(item));
                                }
                                super::super::StageView::BatchLimits { configuration } => {
                                    let ty = expression_type(types, configuration.raw() as usize, budget)?;
                                    let actual = BodyTypeRef::Scheme(&ty).unvalidated();
                                    let Some((size, SchemeRowTail::Closed)) = actual.record_shape() else { return Err(IrVerifyError::new("generic batch configuration has no closed record")); };
                                    let mut fields = 0;
                                    let mut bytes = false;
                                    let mut argv = false;
                                    for (name, expected) in [("count", Type::Int), ("max_bytes", Type::Int), ("max_argv", Type::Bool)] {
                                        if let Some(field) = actual.field(Name::intern(name)) {
                                            fields += 1;
                                            if !body_type_matches(&field.materialize(budget)?, &SchemeType::Ground(expected), budget)? { return Err(IrVerifyError::new("generic batch configuration field has the wrong type")); }
                                            bytes |= name == "max_bytes";
                                            argv |= name == "max_argv";
                                        }
                                    }
                                    if fields != size { return Err(IrVerifyError::new("generic batch configuration contains an unknown field")); }
                                    if bytes && !scheme_conversion(&item, ValueConversion::Argv, scheme_scope, budget)? { return Err(IrVerifyError::new("generic byte-bounded batch item has no argv conversion")); }
                                    if argv && !bytes { budget.charge_scheme_clone(&item)?; batch_argv_checks.push((configuration.raw() as usize, item.clone())); }
                                    pending.push(Obligation::Expression(configuration.raw() as usize, scope));
                                    budget.charge(1)?;
                                    item = SchemeType::List(Box::new(item));
                                }
                                super::super::StageView::Zip { other } => {
                                    let source = expression_type(types, other.raw() as usize, budget)?;
                                    let (right, stream) = pipeline_iteration_item(store, scheme_scope, &source, other.raw() as usize, budget)?;
                                    if let Some(stream) = stream { stream_iterations.push((stream, PropagationBoundary::PipelineTransport { owner: decoder.owner, instruction })); }
                                    budget.charge(3)?;
                                    item = SchemeType::Record { fields: BTreeMap::from([(Name::intern("left"), item), (Name::intern("right"), right)]), tail: SchemeRowTail::Closed };
                                    pending.push(Obligation::Expression(other.raw() as usize, scope));
                                }
                                super::super::StageView::Range { start, end } => {
                                    let start_type = expression_type(types, start.raw() as usize, budget)?;
                                    for ty in [&start_type, &expression_type(types, end.raw() as usize, budget)?] {
                                        if !body_type_matches(ty, &SchemeType::Ground(Type::Int), budget)? { return Err(IrVerifyError::new("generic range bound is not Int")); }
                                    }
                                    if let schema::ExprView::ValueBlock { body, .. } = decoder.expression_view(start.raw() as usize)? {
                                        if !visited.insert(start.raw() as usize) { return Err(IrVerifyError::new("generic range configuration is shared or cyclic")); }
                                        expression_scopes.insert(start.raw() as usize, scope);
                                        let inner = branch_scope(&scope, &mut scopes, budget)?;
                                        let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                                        budget.charge(statements.len())?;
                                        pending.push(Obligation::Expression(end.raw() as usize, inner));
                                        pending.push(Obligation::Block { statements: statements.into(), position: 0, scope: inner, value: Some(start_type) });
                                    } else {
                                        pending.push(Obligation::Expression(start.raw() as usize, scope));
                                        pending.push(Obligation::Expression(end.raw() as usize, scope));
                                    }
                                    item = SchemeType::Ground(Type::Int);
                                }
                                super::super::StageView::Shuffle { seed } => {
                                    if let Some(seed) = seed {
                                        if !body_type_matches(&expression_type(types, seed.raw() as usize, budget)?, &SchemeType::Ground(Type::Int), budget)? { return Err(IrVerifyError::new("generic shuffle seed is not Int")); }
                                        pending.push(Obligation::Expression(seed.raw() as usize, scope));
                                    }
                                }
                                super::super::StageView::Drop(value) | super::super::StageView::Take(value)
                                | super::super::StageView::Repeat { count: value } => {
                                    if !body_type_matches(&expression_type(types, value.raw() as usize, budget)?, &SchemeType::Ground(Type::Int), budget)? {
                                        return Err(IrVerifyError::new("generic pipeline count is not Int"));
                                    }
                                    pending.push(Obligation::Expression(value.raw() as usize, scope));
                                }
                                super::super::StageView::TablePrint { .. } | super::super::StageView::TablePrintConfigured { .. } => {
                                    if !BodyTypeRef::Scheme(&item).is_record() { return Err(IrVerifyError::new("generic table print item is not a record")); }
                                    if let super::super::StageView::TablePrintConfigured { columns } = stage.view {
                                        if !body_type_matches(&expression_type(types, columns.raw() as usize, budget)?, &SchemeType::Ground(Type::List(Box::new(Type::Str))), budget)? { return Err(IrVerifyError::new("generic table columns are not List[Str]")); }
                                        pending.push(Obligation::Expression(columns.raw() as usize, scope));
                                    }
                                    terminal = Some(SchemeType::Ground(Type::Unit));
                                }
                                super::super::StageView::Collect => {
                                    budget.charge_scheme_clone(&item)?;
                                    terminal = Some(SchemeType::List(Box::new(item.clone())));
                                }
                                super::super::StageView::Count => terminal = Some(SchemeType::Ground(Type::Int)),
                                super::super::StageView::Sum => {
                                    if !body_type_matches(&item, &SchemeType::Ground(Type::Int), budget)? { return Err(IrVerifyError::new("generic pipeline sum item is not Int")); }
                                    terminal = Some(SchemeType::Ground(Type::Int));
                                }
                                super::super::StageView::First | super::super::StageView::Last | super::super::StageView::Min | super::super::StageView::Max => {
                                    if matches!(stage.view, super::super::StageView::Min | super::super::StageView::Max) {
                                        super::operations::verify_key_domain(store, scheme_scope, crate::sema::sealed_operation::TypeDomain::MinMaxItem, &item, budget)?;
                                    }
                                    budget.charge_scheme_clone(&item)?;
                                    terminal = Some(SchemeType::Result(Box::new(item.clone()), Box::new(SchemeType::Ground(Type::Error))));
                                }
                                _ => return Err(IrVerifyError::new("generic pipeline contains an unsupported stage contract")),
                            }
                        }
                        let expected = terminal.unwrap_or_else(|| SchemeType::List(Box::new(item)));
                        if !body_type_matches(&expected, &claimed, budget)? { return Err(IrVerifyError::new("generic pipeline result disagrees with its stage contract")); }
                        if let Some(stream) = stream {
                            let mut cardinality = super::super::IndexedCardinalityQuery::new(decoder);
                            let pulls = super::super::indexed_pipeline_downstream_pulls(decoder, &stage_ids, &mut cardinality)?;
                            budget.charge(cardinality.memo.len() + stage_ids.len())?;
                            if !pulls[0].never { stream_iterations.push((stream, PropagationBoundary::PipelineTransport { owner: decoder.owner, instruction })); }
                        }
                        pending.push(Obligation::Expression(input.raw() as usize, scope));
                    }
                    schema::ExprView::ListComp { value, qualifiers, .. } => {
                        if !visited.insert(instruction) { return Err(IrVerifyError::new("generic expression is shared or cyclic")); }
                        let item = BodyTypeRef::Scheme(&claimed).list_item().ok_or_else(|| IrVerifyError::new("generic comprehension has no list item contract"))?.materialize(budget)?;
                        let (inner, mapped_validation) = comprehension_scope(store, types, scheme_scope, qualifiers, &scope, &mut scopes, slot_count, &mut writers, &mut pending, &mut stream_iterations, completion_group, &mut completion_contributions, budget)?;
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
                        let (inner, _) = comprehension_scope(store, types, scheme_scope, qualifiers, &scope, &mut scopes, slot_count, &mut writers, &mut pending, &mut stream_iterations, completion_group, &mut completion_contributions, budget)?;
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
                    schema::ExprView::PatternIf { branches, else_value, .. } => {
                        if !visited.insert(instruction) { return Err(IrVerifyError::new("generic expression is shared or cyclic")); }
                        for branch in branches {
                            budget.charge(1)?;
                            let (condition, value, captures) = branch?;
                            let inner = pattern_condition_scope(decoder, condition.raw() as usize, captures, types, &scope, &mut scopes, slot_count, &mut writers, &mut pending, &mut visited, &mut expression_scopes, budget)?;
                            if !body_type_matches(&expression_type(types, value.raw() as usize, budget)?, &claimed, budget)? { return Err(IrVerifyError::new("generic pattern branch value disagrees with its result")); }
                            pending.push(Obligation::Expression(value.raw() as usize, inner));
                        }
                        if !body_type_matches(&expression_type(types, else_value.raw() as usize, budget)?, &claimed, budget)? { return Err(IrVerifyError::new("generic pattern fallback value disagrees with its result")); }
                        pending.push(Obligation::Expression(else_value.raw() as usize, scope));
                    }
                    schema::ExprView::MatchExpr { value, arms, .. } => {
                        if !visited.insert(instruction) { return Err(IrVerifyError::new("generic expression is shared or cyclic")); }
                        let mut subject = expression_type(types, value.raw() as usize, budget)?;
                        let arms = arms.map(|arm| arm.map(|(pattern, guard, value)| (pattern.raw(), guard.map(|guard| guard.raw()), value.raw()))).collect::<Result<Vec<_>, _>>()?;
                        budget.charge(arms.len())?;
                        if let Some(ordinal) = wrapper_sites.get(&instruction) { wrappers::verify_handler_site(store, decoder, scheme_scope, *ordinal, &subject, &claimed, &arms, types, budget)?; }
                        for (arm, (pattern, guard, value)) in arms.into_iter().enumerate() {
                            let inner = pattern_scope(decoder, pattern, &subject, &scope, &mut scopes, slot_count, &mut writers, budget)?;
                            if !(arm == 1 && wrapper_sites.contains_key(&instruction)) && !body_type_matches(&expression_type(types, value as usize, budget)?, &claimed, budget)? { return Err(IrVerifyError::new("generic match arm disagrees with its result")); }
                            pending.push(Obligation::Expression(value as usize, inner));
                            if let Some(guard) = guard {
                                condition(types, guard as usize, crate::sema::types::ConditionRole::Boolean, budget)?;
                                pending.push(Obligation::Expression(guard as usize, inner));
                            }
                            subject = subject_after_pattern(decoder, pattern, guard.is_some(), subject, budget)?;
                        }
                        pending.push(Obligation::Expression(value.raw() as usize, scope));
                    }
                    _ => {
                        let mut children = Vec::new();
                        verify_body_expression(store, decoder, producer_index, instruction, types, constructors, operation_sites, method_sites, wrapper_sites, scheme_scope, effect_scopes, &scope, &scopes, &mut children, &mut visited, registered, propagation_boundary(&scope, &scopes), expected_return, completion_group, &mut completion_contributions, &mut checked_accumulators, budget, constant_budget)?;
                        record_capture_transfers(store, decoder, scheme_scope, effect_scopes, wrapper_sites, operation_sites, instruction, &scope, &scopes, types, &mut captures, budget)?;
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
                        if super::super::lowered_type_from_type(&expected)? != kind.materialize_lowered()? { return Err(IrVerifyError::new("generic default representation disagrees with its parameter")); }
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
                            let actual = expression_type(types, argument.raw() as usize, budget)?;
                            if !super::scheme_conversion(&actual, crate::sema::schemes::ValueConversion::Display, scheme_scope, budget)? { return Err(IrVerifyError::new("generic print argument cannot be displayed")); }
                            expressions.push(argument.raw() as usize);
                        }
                    }
                    schema::StmtView::Assert { value, message, .. } => {
                        condition(types, value.raw() as usize, crate::sema::types::ConditionRole::Boolean, budget)?;
                        expressions.push(value.raw() as usize);
                        if let Some(message) = message {
                            if !body_type_matches(&expression_type(types, message.raw() as usize, budget)?, &SchemeType::Ground(Type::Str), budget)? {
                                return Err(IrVerifyError::new("generic assertion message is not Str"));
                            }
                            expressions.push(message.raw() as usize);
                        }
                        if completion_group.is_some() && scope.error_boundary.is_none() {
                            let error = SchemeType::Ground(Type::ErrorFamily(Type::assertion_error_family()));
                            push_completion_contribution(&mut completion_contributions, crate::sema::wrapper_operation::WrapperCompletionContribution::PropagatedError, &error, budget)?;
                        }
                    }
                    schema::StmtView::Cd { target, body, .. } => {
                        let actual = expression_type(types, target.raw() as usize, budget)?.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic cd target has no checked path contract"))?;
                        if !matches!(actual.unvalidated(), Type::Path | Type::Str) { return Err(IrVerifyError::new("generic cd target is not a path")); }
                        expressions.push(target.raw() as usize);
                        if completion_group.is_some() && scope.error_boundary.is_none() {
                            let result = SchemeType::Ground(Type::core_command_result(crate::syntax::node::CoreCommand::Cd));
                            if let Some(error) = completion_result_error(&result, budget)? {
                                push_completion_contribution(&mut completion_contributions, crate::sema::wrapper_operation::WrapperCompletionContribution::PropagatedError, &error, budget)?;
                            }
                        }
                        let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                        budget.charge(statements.len())?;
                        nested.push(Obligation::Block { statements: statements.into(), position: 0, scope: branch_scope(&scope, &mut scopes, budget)?, value: None });
                    }
                    schema::StmtView::Env { env, body } => {
                        for assignment in env {
                            budget.charge(1)?;
                            let assignment = assignment?;
                            super::commands::word(assignment.value, super::producers::ProcessWordRole::Environment, scheme_scope, &mut |value, budget| {
                                expressions.push(value.raw() as usize);
                                expression_type(types, value.raw() as usize, budget)
                            }, budget)?;
                        }
                        let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                        budget.charge(statements.len())?;
                        nested.push(Obligation::Block { statements: statements.into(), position: 0, scope: branch_scope(&scope, &mut scopes, budget)?, value: None });
                    }
                    schema::StmtView::Run { value, propagate_result } => {
                        let process = match decoder.expression_view(value.raw() as usize)? {
                            schema::ExprView::Try { value, .. } => decoder.expression_view(value.raw() as usize)?,
                            process => process,
                        };
                        let process_failure_is_recorded = match process {
                            schema::ExprView::RunCapture(run) => run.assert_success || run.propagate,
                            schema::ExprView::RunPipeline { propagate, .. } => propagate,
                            _ => return Err(IrVerifyError::new("generic run statement has no process producer")),
                        };
                        if propagate_result && !process_failure_is_recorded && completion_group.is_some() && scope.error_boundary.is_none() {
                            let actual = expression_type(types, value.raw() as usize, budget)?;
                            let error = completion_result_error(&actual, budget)?.unwrap_or(SchemeType::Ground(Type::ProcessError));
                            push_completion_contribution(&mut completion_contributions, crate::sema::wrapper_operation::WrapperCompletionContribution::PropagatedError, &error, budget)?;
                        }
                        expressions.push(value.raw() as usize);
                    }
                    schema::StmtView::Expr { value, .. } => {
                        if completion_group.is_some() && scope.error_boundary.is_none() {
                            let actual = expression_type(types, value.raw() as usize, budget)?;
                            if let Some(error) = completion_result_error(&actual, budget)? {
                                push_completion_contribution(&mut completion_contributions, crate::sema::wrapper_operation::WrapperCompletionContribution::PropagatedError, &error, budget)?;
                            }
                        }
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
                    schema::StmtView::With { mut bindings, body, else_body, else_param_slot, captures: captured, .. } => {
                        if with_errors.is_none() { with_errors = Some(with_error_types(decoder, types, budget)?); }
                        let error = with_errors.as_ref().unwrap().get(&instruction).ok_or_else(|| IrVerifyError::new("generic With has no actual failure-source proof"))?;
                        budget.charge_scheme_clone(error)?;
                        let error = error.clone();
                        let mut current = branch_scope(&scope, &mut scopes, budget)?;
                        current.error_boundary = Some(scopes.error_boundaries.len());
                        budget.charge_scheme_clone(&error)?;
                        scopes.error_boundaries.push(PropagationBoundary::ResultError(Arc::new(error.clone())));
                        let mut slots = BTreeSet::new();
                        for binding in &mut bindings {
                            let (slot, value) = binding?;
                            let actual = expression_type(types, value.raw() as usize, budget)?;
                            let success = match BodyTypeRef::Scheme(&actual).result_parts() { Some((ok, _)) => ok.materialize(budget)?, None => actual };
                            budget.charge_scheme_clone(&success)?;
                            if slot >= slot_count || writers.insert(slot, success.clone()).is_some() || !slots.insert(slot) { return Err(IrVerifyError::new("generic With binding is duplicated or out of bounds")); }
                            nested.push(Obligation::Expression(value.raw() as usize, current));
                            let next = branch_scope(&current, &mut scopes, budget)?;
                            declare(&next, &mut scopes, slot, success, false, None, budget)?;
                            current = next;
                        }
                        current.error_boundary = scope.error_boundary;
                        let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                        budget.charge(statements.len())?;
                        nested.push(Obligation::Block { statements: statements.into(), position: 0, scope: current, value: None });
                        let handler = branch_scope(&scope, &mut scopes, budget)?;
                        if let Some(slot) = else_param_slot {
                            budget.charge_scheme_clone(&error)?;
                            if slot >= slot_count || writers.insert(slot, error.clone()).is_some() || !slots.insert(slot) { return Err(IrVerifyError::new("generic With failure binding is duplicated or out of bounds")); }
                            declare(&handler, &mut scopes, slot, error, false, None, budget)?;
                        }
                        let physical = captured.collect::<Result<Vec<_>, _>>()?;
                        budget.charge(physical.len())?;
                        if physical.len() != slots.len() || physical.into_iter().collect::<BTreeSet<_>>() != slots { return Err(IrVerifyError::new("generic With capture slots disagree with its actual declarations")); }
                        let statements = else_body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                        budget.charge(statements.len())?;
                        nested.push(Obligation::Block { statements: statements.into(), position: 0, scope: handler, value: None });
                    }
                    schema::StmtView::Guard { target, value, else_body, else_param_slot, optional, .. } => {
                        let actual = expression_type(types, value.raw() as usize, budget)?;
                        let (success, error) = if optional {
                            if else_param_slot.is_some() { return Err(IrVerifyError::new("generic optional guard has an error binding")); }
                            let success = match &actual {
                                SchemeType::Optional(item) => { budget.charge_scheme_clone(item)?; (**item).clone() }
                                SchemeType::Ground(Type::Optional(item)) => { budget.charge_clone(item)?; SchemeType::Ground((**item).clone()) }
                                _ => return Err(IrVerifyError::new("generic optional guard has no optional source")),
                            };
                            (success, None)
                        } else {
                            let (success, error) = BodyTypeRef::Scheme(&actual).result_parts().ok_or_else(|| IrVerifyError::new("generic Result guard has no Result source"))?;
                            (success.materialize(budget)?, Some(error.materialize(budget)?))
                        };
                        let handler = branch_scope(&scope, &mut scopes, budget)?;
                        if let Some(slot) = else_param_slot {
                            let error = error.ok_or_else(|| IrVerifyError::new("generic guard handler has no error source"))?;
                            budget.charge_scheme_clone(&error)?;
                            if slot >= slot_count || writers.insert(slot, error.clone()).is_some() { return Err(IrVerifyError::new("generic guard failure binding is duplicated or out of bounds")); }
                            declare(&handler, &mut scopes, slot, error, false, None, budget)?;
                        }
                        let handler_block = else_body.block_identity();
                        let handler_statements = else_body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                        budget.charge(handler_statements.len())?;
                        if !super::super::indexed_block_can_return_with_decoder(store, handler_block, Some(decoder))? { return Err(IrVerifyError::new("generic guard failure branch can complete")); }
                        let mut bindings = Vec::new();
                        for (slot, ty) in target_bindings(target, &success, budget)? {
                            budget.charge_scheme_clone(&ty)?;
                            if slot >= slot_count || writers.insert(slot, ty.clone()).is_some() { return Err(IrVerifyError::new("generic guard success binding is duplicated or out of bounds")); }
                            bindings.push((slot, ty, false, None));
                        }
                        if optional {
                            pending.push(Obligation::GuardPublish { statements, position: position + 1, scope, value: value_contract, bindings, source: value.raw() as usize, actual, payload: success });
                        } else {
                            pending.push(Obligation::Publish { statements, position: position + 1, scope, value: value_contract, bindings });
                        }
                        pending.push(Obligation::Block { statements: handler_statements.into(), position: 0, scope: handler, value: None });
                        pending.push(Obligation::Expression(value.raw() as usize, scope));
                        continue;
                    }
                    schema::StmtView::Defer { value, .. } => {
                        if !store.function_metadata.get(owner.index()).is_some_and(|metadata| matches!(metadata.callable_kind, crate::sema::types::CallableKind::Proc | crate::sema::types::CallableKind::Stream)) {
                            return Err(IrVerifyError::new("generic defer has no cleanup-capable owner"));
                        }
                        if completion_group.is_some() && scope.error_boundary.is_none() {
                            let actual = expression_type(types, value.raw() as usize, budget)?;
                            if let Some(error) = completion_result_error(&actual, budget)? {
                                push_completion_contribution(&mut completion_contributions, crate::sema::wrapper_operation::WrapperCompletionContribution::PropagatedError, &error, budget)?;
                            }
                        }
                        expressions.push(value.raw() as usize);
                    }
                    schema::StmtView::Return { value: returned } => {
                        let actual = expression_type(types, returned.raw() as usize, budget)?;
                        if stream_item.is_some() {
                            if actual != SchemeType::Ground(Type::Unit) { return Err(IrVerifyError::new("generic stream returns an eager value")); }
                            expressions.push(returned.raw() as usize);
                            pending.push(Obligation::Block { statements, position: position + 1, scope, value: value_contract });
                            for instruction in expressions { pending.push(Obligation::Expression(instruction, scope)); }
                            continue;
                        }
                        let returned_site = store.scheme_application_returns.binary_search_by_key(&(instruction as u32), |site| site.instruction).ok()
                            .map(|index| store.scheme_application_returns[index]);
                        let mut original_result_source = None;
                        if completion_group.is_some() {
                            if returned_site.is_some() {
                                if completion_sources.remove(&instruction).is_some() {
                                    return Err(IrVerifyError::new("application return conflicts with Proc completion source evidence"));
                                }
                            } else {
                                let source = completion_sources.remove(&instruction)
                                    .ok_or_else(|| IrVerifyError::new("generic Proc return has no authenticated completion source"))?;
                                let original_result = !matches!(source.form, crate::runtime::eval::SchemeCompletionSourceForm::LiftedValue);
                                if let Some(source_type) = completion_source_contribution(decoder, types, &scope, &scopes, &expression_scopes, &visited, returned.raw() as usize, source, budget)? {
                                    push_completion_contribution(&mut completion_contributions, crate::sema::wrapper_operation::WrapperCompletionContribution::ReturnedResult, &source_type, budget)?;
                                    if original_result { original_result_source = Some(source_type); }
                                }
                            }
                        }
                        if let Some(site) = returned_site {
                            budget.charge(1)?;
                            if site.function.raw() != decoder.owner {
                                return Err(IrVerifyError::new("application return has a wrong or duplicate owner"));
                            }
                            let FullCallableContract::Scheme(id) = function.contract else { unreachable!() };
                            let application = bounded(store.scheme(id)?.applications, &store.scheme_applications, "scheme application range is invalid")?
                                .get(site.application as usize).and_then(|application| application.invocation()).ok_or_else(|| IrVerifyError::new("scheme return application ordinal is out of scope"))?;
                            let envelope = bounded(application.proc_returns, &store.scheme_proc_returns, "scheme return envelope range is invalid")?
                                .get(site.envelope as usize).ok_or_else(|| IrVerifyError::new("scheme return envelope ordinal is out of scope"))?;
                            let source = decode_scheme_type(store, scheme_scope, application.result, budget, 0)?;
                            let output = decode_scheme_type(store, scheme_scope, envelope.result, budget, 0)?;
                            if !body_types_equal(&source, &actual) {
                                return Err(IrVerifyError::new("application return disagrees with its actual producer or envelope"));
                            }
                            if completion_group.is_some() {
                                push_completion_contribution(&mut completion_contributions, crate::sema::wrapper_operation::WrapperCompletionContribution::ApplicationSource, &source, budget)?;
                            }
                            if !body_types_equal(&output, expected_return) {
                                return Err(IrVerifyError::new("application return producer has no matching proc envelope"));
                            }
                            if tails.remove(&instruction).is_some_and(|tail| !body_types_equal(&tail, &actual)) { return Err(IrVerifyError::new("application return tail disagrees with its actual producer")); }
                            nested.push(Obligation::ApplicationReturn { instruction, value: returned.raw() as usize, site });
                        } else {
                            let joined_result_source = if let (Some(group), Some(source)) = (completion_group, original_result_source.as_ref()) {
                                if !body_types_equal(source, &actual) {
                                    return Err(IrVerifyError::new("authenticated Proc completion source disagrees with its returned operand"));
                                }
                                super::wrappers::completion_accepts_returned_result(store, scheme_scope, group, source, expected_return, budget)?
                            } else { false };
                            let method_path_return = !actual.matches_body_return(expected_return)
                                && super::methods::written_path_return_matches(store, decoder.owner, scheme_scope, &actual, expected_return, budget)?;
                            if !actual.matches_body_return(expected_return) && !joined_result_source && !method_path_return {
                                return Err(IrVerifyError::new("generic body return does not match its scheme return"));
                            }
                            if let Some(tail) = tails.remove(&instruction) {
                                let tail_matches = if joined_result_source || method_path_return {
                                    body_types_equal(&tail, &actual)
                                } else {
                                    tail.matches_body_return(expected_return)
                                };
                                if !tail_matches { return Err(IrVerifyError::new("generic tail disagrees with its scheme return")); }
                            }
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
                    schema::StmtView::PatternIf { branches, else_body, .. } => {
                        for branch in branches {
                            budget.charge(1)?;
                            let (condition, body, captures) = branch?;
                            let inner = pattern_condition_scope(decoder, condition.raw() as usize, captures, types, &scope, &mut scopes, slot_count, &mut writers, &mut pending, &mut visited, &mut expression_scopes, budget)?;
                            let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                            budget.charge(statements.len())?;
                            nested.push(Obligation::Block { statements: statements.into(), position: 0, scope: inner, value: None });
                        }
                        if let Some(body) = else_body {
                            let statements = body.map(|statement| statement.map(|statement| statement.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
                            budget.charge(statements.len())?;
                            nested.push(Obligation::Block { statements: statements.into(), position: 0, scope: branch_scope(&scope, &mut scopes, budget)?, value: None });
                        }
                    }
                    schema::StmtView::ForRecord { target, iter, body, .. } => {
                        let source = expression_type(types, iter.raw() as usize, budget)?;
                        let (item, stream_source, propagated_error) = iteration_item(store, scheme_scope, &source, iter.raw() as usize, propagation_boundary(&scope, &scopes), budget)?;
                        if completion_group.is_some() && scope.error_boundary.is_none()
                            && let Some(error) = propagated_error
                        {
                            push_completion_contribution(&mut completion_contributions, crate::sema::wrapper_operation::WrapperCompletionContribution::PropagatedError, &error, budget)?;
                        }
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
                            nested.push(Obligation::Block { statements: statements.into(), position: 0, scope: inner, value: None });
                            if let Some(guard) = guard {
                                condition(types, guard.raw() as usize, crate::sema::types::ConditionRole::Boolean, budget)?;
                                nested.push(Obligation::Expression(guard.raw() as usize, inner));
                            }
                            subject_type = subject_after_pattern(decoder, pattern.raw(), guard.is_some(), subject_type, budget)?;
                        }
                    }
                    _ => return Err(IrVerifyError::new("generic body has an unsupported statement contract")),
                }
                pending.push(Obligation::Block { statements, position: position + 1, scope, value: value_contract });
                pending.extend(nested);
                for instruction in expressions { pending.push(Obligation::Expression(instruction, scope)); }
            }
        }
    }
    for (configuration, item) in batch_argv_checks {
        if pipeline_configuration_bool(decoder, configuration, "max_argv", &expression_scopes, &scopes, budget)? != Some(false)
            && !scheme_conversion(&item, ValueConversion::Argv, scheme_scope, budget)?
        { return Err(IrVerifyError::new("generic enabled argv batch item has no argv conversion")); }
    }
    let owner_range = store.function_instruction_range(IrFunctionId::from_raw(decoder.owner).ok_or_else(|| IrVerifyError::new("application return owner is invalid"))?.index())?;
    let first_return = store.scheme_application_returns.partition_point(|site| (site.instruction as usize) < owner_range.start);
    let end_return = store.scheme_application_returns.partition_point(|site| (site.instruction as usize) < owner_range.end);
    budget.charge(end_return - first_return)?;
    if store.scheme_application_returns[first_return..end_return].iter().any(|site| !application_returns.contains(&(site.instruction as usize))) {
        return Err(IrVerifyError::new("scheme application return has no actual body instruction"));
    }
    if !completion_sources.is_empty() {
        return Err(IrVerifyError::new("generic Proc completion source is not consumed by its owner return"));
    }
    if namespace_slots != projected_namespaces { return Err(IrVerifyError::new("generic namespace capture has no actual callable projection")); }
    Ok(BodyProof { expressions: visited, writers, statements: visited_statements, streams: stream_iterations, captures, applications, checked_accumulators, completion_contributions, callable_projections })
}

#[cfg(test)]
mod tests {
    use super::super::{FullSchemeBodyFactKind, FullSchemeId, FullSchemeTypeTag, GroundProducerIndex, IR_NONE, IrBlockId, SchemeTypeId, decoder_for_function, effects, verify_function_schemes};
    use crate::runtime::eval::indexed::full::decode::ValueCheckView;
    use super::*;
    use super::super::super::{FullBuilder, FullProgram, FullVerifier};
    use crate::sema::check::Checker;
    use crate::source::SourceMap;
    use crate::syntax::parser::Parser;

    fn build_with(definition: &str, before_build: impl FnOnce(&crate::sema::check::CompactDeclOutput)) -> FullProgram {
        let source = format!("{definition}\n");
        let mut sources = SourceMap::default();
        let source_id = sources.add_file("generic-body-proof.xsh", source.clone());
        let parsed = Parser::parse_source_arena_only(source_id, &source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
        before_build(&checked);
        FullBuilder::build_compact(&parsed.arena, &checked, &source, Arc::new(sources), source_id).unwrap_or_else(|error| panic!("{source}\n{error:?}"))
    }

    fn build(definition: &str) -> FullProgram { build_with(definition, |_| {}) }

    fn prepare(definition: &str) -> FullProgram {
        let program = build(definition);
        FullVerifier::verify(&program).unwrap();
        program
    }

    #[test]
    fn retry_no_completion_accepts_the_original_imperative_source() {
        prepare("proc spin() { loop { let attempt = retry [] { assert false }; break } }\n");
    }

    #[test]
    fn retry_no_completion_requires_cold_termination_and_delay_contracts() {
        use super::super::super::{ExecutableOwner, FullCallableContract, FullCheckedExprPurpose, IrFunctionId};
        let mut program = prepare("proc spin() { let spare: Any = true; loop { let attempt = retry [0ms] { assert false }; break } }\nproc empty() [] {}\npure broad_error() -> Error { error.failure(\"spare\") }\n");
        program.store.in_place.take();
        assert_eq!(GroundProducerIndex::new(&program.store).unwrap().demand(), crate::runtime::eval::ProducerDemand::Needed);
        FullVerifier::verify(&program).unwrap();
        program.store.in_place.take();
        assert!(program.store.in_place.get().is_none());
        let fact = program.store.checked_expr_types.iter().position(|fact| matches!(fact.purpose, FullCheckedExprPurpose::CapturedErrorContract(_))).unwrap();
        let retry = program.store.checked_expr_types[fact].instruction;
        assert!(super::super::checked_expression_type_id(&program.store, retry).is_none());
        let mut missing = program.clone();
        missing.store.checked_expr_types.remove(fact);
        assert_eq!(FullVerifier::verify(&missing).unwrap_err().message, "capture or retry has no checked Result contract");
        let mut wrong_role = program.clone();
        wrong_role.store.checked_expr_types[fact].purpose = FullCheckedExprPurpose::CheckedContract { function_ref: None, with_error_source: false };
        let error = FullVerifier::verify(&wrong_role).unwrap_err();
        assert_eq!(error.message, "checked expression type row is neither callable, a structured error, nor a With error source");
        let broad = program.store.functions.iter().find(|function| program.store.string(function.name).unwrap() == "broad_error").unwrap();
        let FullCallableContract::Ground(signature) = broad.contract else { panic!("broad error is ground") };
        let mut wrong_error = program.clone();
        wrong_error.store.checked_expr_types[fact].type_id = program.store.semantic.signature_return_type(signature).unwrap();
        assert_eq!(FullVerifier::verify(&wrong_error).unwrap_err().message, "captured error contract disagrees with its actual retry errors");
        let condition = program.store.tags.iter().enumerate().find_map(|(instruction, tag)| (matches!(tag, super::super::super::FullTag::BoolBool | super::super::super::FullTag::ExprBool) && program.store.payload(program.store.data[instruction].range()).unwrap() == [0]).then_some(instruction)).unwrap();
        let mut completing = program.clone();
        let words = completing.store.data[condition].range().bounds(completing.store.extra.len()).unwrap();
        assert_eq!(&completing.store.extra[words.clone()], &[0]);
        completing.store.extra[words.start] = 1;
        let error = FullVerifier::verify(&completing).unwrap_err();
        assert_eq!(error.message, "capture or retry has no checked Result contract");
        let delay = program.store.tags.iter().position(|tag| *tag == super::super::super::FullTag::ExprDuration).unwrap();
        let boolean = program.store.tags.iter().position(|tag| *tag == super::super::super::FullTag::ExprBool).unwrap();
        assert!(!program.store.checked_expr_types.iter().any(|fact| [delay, boolean].contains(&(fact.instruction as usize))));
        let mut wrong_delay = program.clone();
        wrong_delay.store.tags.swap(delay, boolean);
        wrong_delay.store.data.swap(delay, boolean);
        let error = FullVerifier::verify(&wrong_delay).unwrap_err();
        assert!(error.message.contains("producer") && error.message.contains("contract"), "{error:?}");
        let function = (0..program.store.functions.len()).find(|index| program.store.function_instruction_range(*index).unwrap().contains(&delay)).unwrap();
        let empty = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "empty").unwrap();
        let mut foreign = program.clone();
        foreign.store.checked_expr_types[fact].purpose = FullCheckedExprPurpose::CapturedErrorContract(ExecutableOwner::Function(IrFunctionId::new(empty).unwrap()));
        assert_eq!(FullVerifier::verify(&foreign).unwrap_err().message, "captured error contract belongs to another owner");
        let mut wrong_source = program.clone();
        wrong_source.store.checked_expr_types[fact].instruction = boolean as u32;
        wrong_source.store.checked_expr_types.sort_by_key(|fact| fact.instruction);
        assert_eq!(FullVerifier::verify(&wrong_source).unwrap_err().message, "captured error contract has no physical retry");
        let mut duplicate = program.clone();
        duplicate.store.checked_expr_types.push(program.store.checked_expr_types[fact]);
        duplicate.store.checked_expr_types.sort_by_key(|fact| fact.instruction);
        assert_eq!(FullVerifier::verify(&duplicate).unwrap_err().message, "checked expression type rows are not strictly ordered expression facts");
        let mut wrong_owner = program.clone();
        wrong_owner.store.functions[function].contract = program.store.functions[empty].contract;
        let error = FullVerifier::verify(&wrong_owner).unwrap_err();
        assert!(error.message.contains("effect") && error.message.contains("owner"), "{error:?}");
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn retry_no_completion_preserves_error_cleanup_and_frames_after_disposal() {
        let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/retry-no-completion.xsh")).unwrap();
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("retry-no-completion.xsh", crate::loader::entry_source_from_text("retry-no-completion.xsh", source), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test("retry_no_completion", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "{:?}", result.output.diagnostics);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
            assert!(matches!(&result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)), "{force_frames}: {:?}", result.result);
        }
    }

    #[test]
    fn fresh_result_context_accepts_written_return_consumers() {
        prepare("error Failure = Rejected(message: Str)\npure expected_failure() -> Result[Int, Error] { Err(Failure.Rejected(message: \"closed callback failed\"), cause: error.failure(\"closed cause\")).context(\"closed-observer\", \"closed callback\") }\n");
    }

    #[test]
    fn fresh_result_context_written_returns_reject_wrong_method_when_cold() {
        let mut program = prepare("error Failure = Rejected(message: Str)\npure expected_failure() -> Result[Int, Error] { let spare = \"trim\"; Err(Failure.Rejected(\"failure\")).context(\"stage\") }\n");
        program.store.in_place.take();
        assert_eq!(GroundProducerIndex::new(&program.store).unwrap().demand(), crate::runtime::eval::ProducerDemand::Needed);
        FullVerifier::verify(&program).unwrap();
        program.store.in_place.take();
        assert!(program.store.in_place.get().is_none());
        let method = program.store.tags.iter().position(|tag| *tag == super::super::super::FullTag::ExprMethod).unwrap();
        let function = (0..program.store.functions.len()).find(|index| program.store.function_instruction_range(*index).unwrap().contains(&method)).unwrap();
        let decoder = decoder_for_function(&program.store, function).unwrap();
        let spelling = decoder.instruction_range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Str("trim")))).unwrap();
        let spelling = program.store.extra[program.store.data[spelling].range().start as usize];
        let mut changed = program.clone();
        assert!(changed.store.in_place.get().is_none());
        let words = changed.store.data[method].range().bounds(changed.store.extra.len()).unwrap();
        changed.store.extra[words.start + 1] = spelling;
        let error = FullVerifier::verify(&changed).unwrap_err();
        assert!(error.message.starts_with("ground method has no receiver contract") || error.message.starts_with("ground method has no registered parameter contract"), "{error:?}");
    }

    #[test]
    fn exit_effects_infer_and_reach_compact_owner_contracts() {
        prepare(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/snippets/spec/49-exit.xsh")));
        for source in [
            "proc leave(code: Int) { exit code }\nproc relay() [process] { leave(77) }\n",
            "proc leave(code: Int) [process] { exit code }\n",
            "exit 77\n",
        ] { prepare(source); }
    }

    #[test]
    fn exit_effects_reject_empty_pure_and_excluded_contracts() {
        use crate::diagnostic::{DiagnosticCode, Severity};
        for (source, code) in [
            ("proc leave() [] { exit 77 }\n", DiagnosticCode::CheckEffectViolation),
            ("pure leave() { exit 77 }\n", DiagnosticCode::CheckPureRun),
            ("proc leave() [process] { without process { exit 77 } }\n", DiagnosticCode::CheckEffectViolation),
            ("proc leave() { exit 77 }\nproc relay() [] { leave() }\n", DiagnosticCode::CheckEffectViolation),
        ] {
            let mut sources = SourceMap::default();
            let source_id = sources.add_file("exit-effects.xsh", source.to_owned());
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error && diagnostic.code == Some(code)), "{source}\n{:?}", checked.diagnostics);
        }
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn record_list_conditionals_execute_after_disposal_on_both_routes() {
        let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/record-list-conditionals.xsh")).unwrap();
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("record-list-conditionals.xsh", crate::loader::entry_source_from_text("record-list-conditionals.xsh", source), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test("record_list_conditionals", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
            assert!(matches!(&result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
        }
    }

    #[test]
    fn record_list_conditionals_authenticate_unknown_arms() {
        let mut program = prepare(r#"type Row = {value: Str}
pure values(rows: List[Row]) -> List[Str] { [row.value for row in rows] }
proc choose(stored: Record, erased: Any, numbers: List[Int]) -> Result[List[Str]] {
    let rows = if "rows" in stored { stored.get("rows")?.require(List[Row])? } else {
        let spare_record: Any = stored
        let spare_erased: Any = erased
        let spare_numbers: Any = numbers
        []
    }
    Ok(values(rows))
}
"#);
        assert_eq!(GroundProducerIndex::new(&program.store).unwrap().demand(), crate::runtime::eval::ProducerDemand::Needed);
        program.store.in_place.take();
        FullVerifier::verify(&program).unwrap();
        program.store.in_place.take();
        assert!(program.store.in_place.get().is_none());
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "choose").unwrap();
        let empty = program.store.function_instruction_range(owner).unwrap().find(|instruction| program.store.tags[*instruction] == super::super::super::FullTag::ExprList).unwrap();
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        for slot in [0, 1, 2] {
            let spare = program.store.function_instruction_range(owner).unwrap().filter(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(actual)) if actual == slot)).last().unwrap();
            let mut changed = program.clone();
            changed.store.tags.swap(empty, spare);
            changed.store.data.swap(empty, spare);
            let error = FullVerifier::verify(&changed).unwrap_err();
            assert!(error.message.contains("producer") && error.message.contains("contract"), "header slot {slot}: {error:?}");
        }
    }

    #[test]
    fn fresh_result_context_consumers_reject_wrong_physical_inputs() {
        fn swap_rows(program: &FullProgram, left: usize, right: usize) -> FullProgram {
            let mut changed = program.clone();
            changed.store.tags.swap(left, right);
            changed.store.data.swap(left, right);
            for fact in &mut changed.store.checked_expr_types {
                if fact.instruction as usize == left { fact.instruction = right as u32; }
                else if fact.instruction as usize == right { fact.instruction = left as u32; }
            }
            changed.store.checked_expr_types.sort_by_key(|fact| fact.instruction);
            changed
        }
        fn rejects(program: &FullProgram, input: &str) {
            let error = FullVerifier::verify(program).unwrap_err();
            assert!(error.message.starts_with("ground producer value disagrees with its checked contract")
                || error.message.starts_with("checked value input disagrees with its contract")
                || error.message.starts_with("checked value input has no producer contract")
                || error.message.starts_with("ground method has no receiver contract")
                || error.message.starts_with("ground method has no registered parameter contract"), "{input}: {error:?}");
        }
        let mut program = prepare(r#"
error Failure = Bad(message: Str)
error Cause = Bad(message: Str)
error Other = Bad(message: Str)
pure choose(erased: Any) -> Result[Unit, Error] {
    let spare: Result[Str, Error] = Ok("wrong")
    let compatible: Result[Unit, Error] = Ok()
    let alias: Result[Str, Error] = spare
    let spare_bool: Any = true
    let spare_name: Any = "trim"
    let alternate: Result[Unit, Other] = Err(Other.Bad("other"))
    let original: Result[Unit, Error] = Err(Failure.Bad("outer"), cause: Cause.Bad("cause")).context("stage", "detail")
    original
}
let observed = choose("data")
assert observed != Ok()
"#);
        assert_eq!(GroundProducerIndex::new(&program.store).unwrap().demand(), crate::runtime::eval::ProducerDemand::Needed);
        program.store.in_place.take();
        FullVerifier::verify(&program).unwrap();
        program.store.in_place.take();
        assert!(program.store.in_place.get().is_none());
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "choose").unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let (guard, method, checked) = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::CheckedValue { value, check: ValueCheckView::Ground(check), .. }) if program.store.tags[value.raw() as usize] == super::super::super::FullTag::ExprMethod => Some((instruction, value.raw() as usize, check.ty.id)),
            _ => None,
        }).unwrap();
        let schema::ExprView::Method { receiver, args, name: "context", .. } = decoder.expression_view(method).unwrap() else { unreachable!() };
        let receiver = receiver.raw() as usize;
        let arguments = args.collect::<Result<Vec<_>, _>>().unwrap().into_iter().map(|argument| argument.raw() as usize).collect::<Vec<_>>();
        assert_eq!(arguments.len(), 2);
        let mut constructor = receiver;
        let mut subjects = Vec::new();
        while let schema::ExprView::MatchExpr { value, mut arms, .. } = decoder.expression_view(constructor).unwrap() {
            let (pattern, guard, body) = arms.next().unwrap().unwrap();
            assert!(guard.is_none() && arms.next().is_none());
            assert!(matches!(decoder.checked_pattern_view(pattern.raw()).unwrap(), schema::PatternView::Bind { .. }));
            subjects.push(value.raw() as usize);
            constructor = body.raw() as usize;
        }
        assert!(matches!(decoder.expression_view(constructor).unwrap(), schema::ExprView::Err { cause: Some(_), .. }));
        assert_eq!(subjects.len(), 2);
        let primary = subjects[0];
        let cause = subjects[1];
        let compatible = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Ok(value)) if matches!(decoder.expression_view(value.raw() as usize), Ok(schema::ExprView::Unit)))).unwrap();
        FullVerifier::verify(&swap_rows(&program, receiver, compatible)).unwrap();
        let spare = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Ok(value)) if matches!(decoder.expression_view(value.raw() as usize), Ok(schema::ExprView::Str("wrong"))))).unwrap();
        rejects(&swap_rows(&program, receiver, spare), "success payload");
        let boolean = range.clone().find(|instruction| program.store.tags[*instruction] == super::super::super::FullTag::ExprBool).unwrap();
        for (name, input) in [("error", primary), ("cause", cause), ("kind", arguments[0]), ("message", arguments[1])] { rejects(&swap_rows(&program, input, boolean), name); }
        let alias = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(slot)) if slot != 0)).unwrap();
        let mut erased = swap_rows(&program, receiver, alias);
        let words = erased.store.data[receiver].range().bounds(erased.store.extra.len()).unwrap();
        assert_eq!(words.len(), 1);
        erased.store.extra[words.start] = 0;
        assert!(!erased.store.checked_expr_types.iter().any(|fact| fact.instruction as usize == receiver));
        rejects(&erased, "untyped receiver");
        let alternate = program.store.checked_expr_types.iter().find(|fact| range.contains(&(fact.instruction as usize)) && fact.instruction as usize != receiver && program.store.tags[fact.instruction as usize] == super::super::super::FullTag::ExprErr).unwrap().type_id;
        let mut wrong_family = program.clone();
        let words = wrong_family.store.data[guard].range().bounds(wrong_family.store.extra.len()).unwrap();
        assert_eq!(wrong_family.store.extra[words.start + 2], checked.raw());
        wrong_family.store.extra[words.start + 2] = alternate.raw();
        for fact in &mut wrong_family.store.checked_expr_types {
            if fact.instruction as usize == guard {
                assert!(fact.is_contract());
                fact.type_id = alternate;
            }
        }
        rejects(&wrong_family, "error family");
        let spelling = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Str("trim")))).unwrap();
        let spelling = program.store.extra[program.store.data[spelling].range().start as usize];
        assert_eq!(program.store.string(spelling).unwrap(), "trim");
        let mut wrong_method = program.clone();
        let words = wrong_method.store.data[method].range().bounds(wrong_method.store.extra.len()).unwrap();
        assert_eq!(program.store.string(wrong_method.store.extra[words.start + 1]).unwrap(), "context");
        wrong_method.store.extra[words.start + 1] = spelling;
        rejects(&wrong_method, "method selection");
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn fresh_result_context_preserves_cause_and_frames_after_disposal() {
        let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/fresh-result-context.xsh")).unwrap();
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("fresh-result-context.xsh", crate::loader::entry_source_from_text("fresh-result-context.xsh", source), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test("fresh_result_context", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
            assert!(matches!(&result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
        }
    }

    #[test]
    fn generic_pattern_conditions_authenticate_captures_and_branch_scopes() {
        let program = prepare(r#"
pure keep(payload, source: Result[Int]) {
    let _ = "wrong"
    let previous = if let Ok(earlier) = source { earlier } else { 0 }
    let selected = if let Ok(later) = source { later + previous } else { 0 }
    {payload, selected}
}
pure other(payload) { payload }
"#);
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let branches = range.clone().filter_map(|instruction| {
            let Ok(schema::ExprView::PatternIf { mut branches, .. }) = decoder.expression_view(instruction) else { return None };
            let (condition, _, captures) = branches.next().unwrap().unwrap();
            let block = captures.block_identity_for_test();
            let captures = captures.collect::<Result<Vec<_>, _>>().unwrap();
            assert_eq!(captures.len(), 1);
            Some((condition.raw() as usize, block, captures[0]))
        }).collect::<Vec<_>>();
        assert_eq!(branches.len(), 2);
        let (condition, capture_block, captured) = branches[1];
        let capture_words = program.store.blocks[capture_block.index()].instructions.bounds(program.store.extra.len()).unwrap();
        let mut wrong_capture = program.clone();
        wrong_capture.store.in_place.take();
        wrong_capture.store.extra[capture_words.start + 1] = 0;
        assert!(FullVerifier::verify(&wrong_capture).unwrap_err().message.contains("capture"));
        let schema::ExprView::MatchExpr { mut arms, .. } = decoder.expression_view(condition).unwrap() else { unreachable!() };
        let (pattern, _, yes) = arms.next().unwrap().unwrap();
        let (_, _, no) = arms.next().unwrap().unwrap();
        let mut false_success = program.clone();
        false_success.store.in_place.take();
        false_success.store.data[yes.raw() as usize] = program.store.data[no.raw() as usize];
        assert_eq!(FullVerifier::verify(&false_success).unwrap_err().message, "generic pattern condition binds on failure");
        let read = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(slot)) if slot == captured)).unwrap();
        let words = program.store.data[read].range().bounds(program.store.extra.len()).unwrap();
        assert_eq!(words.len(), 1);
        let mut sibling_capture = program.clone();
        sibling_capture.store.in_place.take();
        sibling_capture.store.extra[words.start] = u32::try_from(branches[0].2).unwrap();
        assert_eq!(FullVerifier::verify(&sibling_capture).unwrap_err().message, "generic body reads an uninitialized slot");
        let mut wrong_projection = program.clone();
        wrong_projection.store.in_place.take();
        assert_eq!(wrong_projection.store.patterns[pattern.raw() as usize], super::super::super::FullPatternTag::ResultOk);
        wrong_projection.store.patterns[pattern.raw() as usize] = super::super::super::FullPatternTag::ResultErr;
        assert!(FullVerifier::verify(&wrong_projection).is_err());
        let fact = program.store.scheme_body_facts.iter().position(|fact| fact.function.index() == owner && fact.instruction as usize == read && fact.kind() == FullSchemeBodyFactKind::Expression).unwrap();
        let text = program.store.scheme_body_facts.iter().find(|fact| fact.function.index() == owner && program.store.tags[fact.instruction as usize] == FullTag::ExprStr && fact.kind() == FullSchemeBodyFactKind::Expression).unwrap();
        let mut wrong_type = program.clone();
        wrong_type.store.in_place.take();
        wrong_type.store.scheme_body_facts[fact].value = text.value;
        assert!(FullVerifier::verify(&wrong_type).is_err());
        let other = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "other").unwrap();
        let mut wrong_owner = program.clone();
        wrong_owner.store.in_place.take();
        wrong_owner.store.scheme_body_facts[fact].function = IrFunctionId::new(other).unwrap();
        assert!(FullVerifier::verify(&wrong_owner).unwrap_err().message.contains("ownership"));
    }

    #[test]
    fn generic_pattern_conditions_authenticate_symbolic_result_subjects() {
        let program = prepare("pure keep(payload, value) { if let Ok(found) = value { return {payload, found} }; {payload, found: 0} }\n");
        let _symbols = program.symbol_owner().enter();
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let condition = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
            let Ok(schema::StmtView::PatternIf { mut branches, .. }) = decoder.statement_view(instruction) else { return None };
            Some(branches.next().unwrap().unwrap().0.raw() as usize)
        }).unwrap();
        let schema::ExprView::MatchExpr { value, .. } = decoder.expression_view(condition).unwrap() else { unreachable!() };
        let scopes = super::super::prepare_tables(&program.store).unwrap();
        let mut budget = SchemeInstantiationBudget::new(&program.store);
        assert!(matches!(super::super::scoped_expression_type(&program.store, &decoder, condition as u32, &scopes.scopes, &mut budget).unwrap(), Some(SchemeType::Ground(Type::Bool))));
        let subject = super::super::scoped_expression_type(&program.store, &decoder, value.raw(), &scopes.scopes, &mut budget).unwrap().unwrap();
        assert!(matches!(subject, SchemeType::Result(..)));
        assert!(subject.closed_ground_type().is_none());
        let read = program.store.function_instruction_range(owner).unwrap().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(1)))).unwrap();
        let words = program.store.data[read].range().bounds(program.store.extra.len()).unwrap();
        let mut wrong_source = program.clone();
        wrong_source.store.in_place.take();
        wrong_source.store.extra[words.start] = 0;
        assert!(FullVerifier::verify(&wrong_source).is_err());
        let fact = program.store.scheme_body_facts.iter().position(|fact| fact.function.index() == owner && fact.instruction as usize == condition && fact.kind() == FullSchemeBodyFactKind::Expression).unwrap();
        let integer = program.store.scheme_body_facts.iter().find(|fact| fact.function.index() == owner && program.store.tags[fact.instruction as usize] == FullTag::ExprInt && fact.kind() == FullSchemeBodyFactKind::Expression).unwrap();
        let mut wrong_condition = program.clone();
        wrong_condition.store.in_place.take();
        wrong_condition.store.scheme_body_facts[fact].value = integer.value;
        assert!(FullVerifier::verify(&wrong_condition).is_err());
    }

    #[test]
    fn materialized_named_callable_values_require_owned_instances_and_targets() {
        let program = prepare("pure identity(payload) { payload }\npure other(payload) { [payload] }\nproc keep(action, value) { let _ = action(value); value }\nproc relay(value) { let wrapped = Ok(value); keep(identity, wrapped) }\n");
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "relay").unwrap();
        let other = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "other").unwrap();
        let mut range = program.store.function_instruction_range(owner).unwrap();
        let instruction = range.find(|&instruction| program.store.tags[instruction] == super::super::super::FullTag::ExprFunctionRef).unwrap();
        let instance = program.store.call_instantiations.iter().position(|instance| instance.instruction as usize == instruction).unwrap();
        let target = program.store.checked_call_targets.iter().position(|target| target.instruction as usize == instruction).unwrap();
        let fact = program.store.scheme_body_facts.iter().position(|fact| fact.instruction as usize == instruction && fact.kind() == FullSchemeBodyFactKind::Expression).unwrap();
        let mut missing_instance = program.clone();
        missing_instance.store.call_instantiations.remove(instance);
        assert!(FullVerifier::verify(&missing_instance).is_err());
        let mut missing_target = program.clone();
        missing_target.store.checked_call_targets.remove(target);
        assert!(FullVerifier::verify(&missing_target).is_err());
        let mut foreign_owner = program.clone();
        foreign_owner.store.scheme_body_facts[fact].function = IrFunctionId::new(other).unwrap();
        assert!(FullVerifier::verify(&foreign_owner).is_err());
        let mut missing_fact = program.clone();
        remove_scheme_body_fact(&mut missing_fact, fact);
        assert!(FullVerifier::verify(&missing_fact).is_err());
        let mut wrong_target = program.clone();
        wrong_target.store.extra[program.store.data[instruction].range().start as usize] = IrFunctionId::new(other).unwrap().raw();
        assert!(FullVerifier::verify(&wrong_target).is_err());
        let mut coordinated_target = wrong_target;
        coordinated_target.store.checked_call_targets[target].function = IrFunctionId::new(other).unwrap();
        assert!(FullVerifier::verify(&coordinated_target).is_err());
        let argument = program.store.call_instantiations[instance].arguments.type_arguments.start as usize;
        let ty = program.store.call_type_arguments[argument].ty;
        let mut foreign_substitution = program.clone();
        foreign_substitution.store.scheme_types[ty.index()].first = u32::MAX;
        assert!(FullVerifier::verify(&foreign_substitution).is_err());
    }

    #[test]
    fn generic_scheme_value_calls_authenticate_owned_sources_and_bindings() {
        let program = prepare("pure items(maybe) { maybe?.items ?? [] }\npure other(maybe) { maybe?.other ?? [] }\npure relay(maybe) { let lookup = items; lookup(maybe) }\npure foreign(maybe) { let lookup = other; lookup(maybe) }\npure direct(maybe, wrong) { items(maybe) }\n");
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "relay").unwrap();
        let foreign = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "foreign").unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let foreign_range = program.store.function_instruction_range(foreign).unwrap();
        let site = program.store.scheme_callable_calls.iter().position(|site| range.contains(&(site.instruction as usize))).unwrap();
        let foreign_site = program.store.scheme_callable_calls.iter().find(|site| foreign_range.contains(&(site.instruction as usize))).unwrap();
        let call = program.store.scheme_callable_calls[site].instruction as usize;
        let decoder = super::super::decoder_for_function(&program.store, owner).unwrap();
        let schema::ExprView::DynamicCall { callee, mut args, .. } = decoder.expression_view(call).unwrap() else { unreachable!() };
        let super::super::super::decode::CallArgView::Single(argument) = args.next().unwrap().unwrap() else { unreachable!() };
        let callee = callee.raw() as usize;
        let argument = argument.raw() as usize;
        let fact = program.store.scheme_body_facts.iter().position(|fact| fact.instruction as usize == callee && fact.kind() == FullSchemeBodyFactKind::Expression).unwrap();
        let mut wrong_owner = program.clone();
        wrong_owner.store.scheme_body_facts[fact].function = IrFunctionId::new(foreign).unwrap();
        assert!(FullVerifier::verify(&wrong_owner).is_err());
        let mut missing_source = program.clone();
        remove_scheme_body_fact(&mut missing_source, fact);
        assert!(FullVerifier::verify(&missing_source).is_err());
        let mut wrong_relationship = program.clone();
        wrong_relationship.store.scheme_callable_calls[site].relationship = foreign_site.relationship;
        assert!(FullVerifier::verify(&wrong_relationship).is_err());
        let mut missing_invocation = program.clone();
        missing_invocation.store.call_instantiations.retain(|instance| instance.instruction as usize != call);
        assert!(FullVerifier::verify(&missing_invocation).is_err());
        assert_eq!(program.store.tags[callee], super::super::super::FullTag::ExprParam);
        assert_eq!(program.store.tags[argument], super::super::super::FullTag::ExprParam);
        let lookup = program.store.extra[program.store.data[callee].range().start as usize];
        let input = program.store.extra[program.store.data[argument].range().start as usize];
        assert_ne!(lookup, input);
        let mut wrong_callee = program.clone();
        wrong_callee.store.extra[program.store.data[callee].range().start as usize] = input;
        assert!(FullVerifier::verify(&wrong_callee).is_err());
        let mut wrong_argument = program.clone();
        wrong_argument.store.extra[program.store.data[argument].range().start as usize] = lookup;
        assert!(FullVerifier::verify(&wrong_argument).is_err());
        let direct = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "direct").unwrap();
        let direct_range = program.store.function_instruction_range(direct).unwrap();
        let direct_call = program.store.checked_call_targets.iter().find(|call| direct_range.contains(&(call.instruction as usize))).unwrap().instruction as usize;
        let mut wrong_target = program.clone();
        wrong_target.store.extra[program.store.data[direct_call].range().start as usize] = program.store.scheme_callable_values[foreign_site.relationship as usize].canonical.raw();
        wrong_target.store.checked_call_targets.iter_mut().find(|target| target.instruction as usize == direct_call).unwrap().function = program.store.scheme_callable_values[foreign_site.relationship as usize].canonical;
        assert!(FullVerifier::verify(&wrong_target).is_err());
        let direct_decoder = super::super::decoder_for_function(&program.store, direct).unwrap();
        let (schema::ExprView::Call { mut args, .. } | schema::ExprView::DirectPureCall { mut args, .. }) = direct_decoder.expression_view(direct_call).unwrap() else { unreachable!() };
        let super::super::super::decode::CallArgView::Single(argument) = args.next().unwrap().unwrap() else { unreachable!() };
        let argument = argument.raw() as usize;
        let mut wrong_binding = program.clone();
        wrong_binding.store.extra[program.store.data[argument].range().start as usize] = 1;
        assert!(FullVerifier::verify(&wrong_binding).is_err());
    }

    #[test]
    fn generic_pipeline_alias_calls_authenticate_values_and_invocations() {
        let program = prepare("pure identity(value) { value }\npure other(value) { [value] }\npure aliased(value) { let action = identity; let again = action; [value] |> map(again) }\npure foreign(value) { let action = other; [value] |> map(action) }\n");
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "aliased").unwrap();
        let foreign = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "foreign").unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let foreign_range = program.store.function_instruction_range(foreign).unwrap();
        let site = program.store.scheme_callable_calls.iter().position(|site| range.contains(&(site.instruction as usize))).unwrap();
        let foreign_site = program.store.scheme_callable_calls.iter().find(|site| foreign_range.contains(&(site.instruction as usize))).unwrap();
        let instruction = program.store.scheme_callable_calls[site].instruction as usize;
        let decoder = super::super::decoder_for_function(&program.store, owner).unwrap();
        let schema::ExprView::DynamicCall { callee, mut args, .. } = decoder.expression_view(instruction).unwrap() else { panic!("alias retains its actual callable value") };
        let super::super::super::decode::CallArgView::Single(argument) = args.next().unwrap().unwrap() else { unreachable!() };
        let callee = callee.raw() as usize;
        let argument = argument.raw() as usize;
        assert!(matches!(decoder.expression_view(callee), Ok(schema::ExprView::Param(_))));
        assert!(matches!(decoder.expression_view(argument), Ok(schema::ExprView::Param(_))));
        let mut missing_invocation = program.clone();
        missing_invocation.store.call_instantiations.retain(|instance| instance.instruction as usize != instruction);
        assert!(FullVerifier::verify(&missing_invocation).is_err());
        let mut wrong_relationship = program.clone();
        wrong_relationship.store.scheme_callable_calls[site].relationship = foreign_site.relationship;
        assert!(FullVerifier::verify(&wrong_relationship).is_err());
        let mut wrong_value = program.clone();
        wrong_value.store.extra[program.store.data[callee].range().start as usize] = program.store.extra[program.store.data[argument].range().start as usize];
        assert!(FullVerifier::verify(&wrong_value).is_err());
        let fact = program.store.scheme_body_facts.iter().position(|fact| fact.instruction as usize == callee && fact.kind() == FullSchemeBodyFactKind::Expression).unwrap();
        let mut foreign_owner = program.clone();
        foreign_owner.store.scheme_body_facts[fact].function = IrFunctionId::new(foreign).unwrap();
        assert!(FullVerifier::verify(&foreign_owner).is_err());
    }

    #[test]
    fn generic_pipeline_aliases_preserve_effect_and_erasure_rejections() {
        for source in [
            "proc effect(value) [env] { let _ = env.get(\"HOME\"); value }\npure mapped(value) { let action = effect; [value] |> map(action) }\n",
            "pure identity(value) { value }\npure mapped(value) { let action: Pure = identity; [value] |> map(action) }\n",
            "pure mapped(action: Any, value) { [value] |> map(action) }\n",
        ] {
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error), "{source}\n{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn ground_pipeline_aliases_reject_generic_targets_and_foreign_metadata() {
        let program = prepare("let prefix = \"captured:\"\npure decorated(value: Str, suffix: Str = \"!\") -> Str { prefix + value + suffix }\npure identity(value) { value }\npure observed(payload) { let action = decorated; {payload, rows: [\"kept\"] |> map(action)} }\npure generic_owner(value) { let action = identity; [value] |> map(action) }\n");
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "observed").unwrap();
        let generic_owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "generic_owner").unwrap();
        let target = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "decorated").unwrap();
        let generic_target = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "identity").unwrap();
        let reference = |owner| program.store.function_instruction_range(owner).unwrap().find(|instruction| program.store.tags[*instruction] == super::super::super::FullTag::ExprFunctionRef).unwrap();
        let instruction = reference(owner);
        let generic = reference(generic_owner);
        let target_fact = program.store.checked_call_targets.iter().position(|fact| fact.instruction as usize == instruction).unwrap();
        let mut missing_target = program.clone();
        missing_target.store.checked_call_targets.remove(target_fact);
        assert!(FullVerifier::verify(&missing_target).is_err());
        let mut wrong_target = program.clone();
        let destination = program.store.data[instruction].range().bounds(program.store.extra.len()).unwrap();
        let source = program.store.data[generic].range().bounds(program.store.extra.len()).unwrap();
        assert_eq!(destination.len(), source.len());
        wrong_target.store.extra[destination].copy_from_slice(&program.store.extra[source]);
        wrong_target.store.checked_call_targets[target_fact].function = IrFunctionId::new(generic_target).unwrap();
        assert!(FullVerifier::verify(&wrong_target).is_err());
        let mut wrong_kind = program.clone();
        wrong_kind.store.function_metadata[target].callable_kind = crate::sema::types::CallableKind::Proc;
        assert!(FullVerifier::verify(&wrong_kind).is_err());
        let mut wrong_capture = program.clone();
        let capture = program.store.functions[target].captures.bounds(program.store.captures.len()).unwrap().next().unwrap();
        wrong_capture.store.captures[capture].binding_identity = u32::MAX;
        assert!(FullVerifier::verify(&wrong_capture).is_err());
    }



    #[cfg(feature = "native-tests")]
    #[test]
    fn generic_pipeline_imported_aliases_retain_captures_and_defaults_after_disposal() {
        let work = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".work/tmp");
        std::fs::create_dir_all(&work).unwrap();
        let modules = tempfile::tempdir_in(work).unwrap();
        std::fs::write(modules.path().join("helpers.xsh"), "##! Callable stage fixture.\nlet prefix = \"module:\"\n## Render an item with its captured prefix and default suffix.\nexport pure render(value: Str, suffix: Str = \"!\") -> Str { prefix + value + suffix }\n## Preserve the callable value.\nexport let format = render\n").unwrap();
        std::fs::write(modules.path().join("foreign.xsh"), "##! Foreign callable namespace.\n## Preserve an integer.\nexport pure keep(value: Int) -> Int { value }\n## Retain the foreign callable.\nexport let format = keep\n").unwrap();
        let source = "use helpers\nuse foreign\npure local(payload) { let action = helpers.format; let again = action; {payload, rows: [\"local\"] |> map(again)} }\npure exported(payload) { {payload, rows: [\"exported\"] |> map(helpers.format)} }\ntest imported_aliases { assert local(7) == {payload: 7, rows: [\"module:local!\"]}; assert local(\"kept\").payload == \"kept\"; assert exported(7).rows == [\"module:exported!\"] }\n";
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("imported-pipeline-alias.xsh", crate::loader::entry_source_from_text("imported-pipeline-alias.xsh", source.to_string()), vec![modules.path().to_path_buf()]);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = crate::sema::check::Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let program = FullBuilder::build_compact(&parsed.arena, &checked, source, Arc::new(sources.clone()), source_id).unwrap();
        FullVerifier::verify(&program).unwrap();
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "local").unwrap();
        let target = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "render").unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let field = range.clone().find(|instruction| program.store.tags[*instruction] == super::super::super::FullTag::ExprField).unwrap();
        let decoder = super::super::decoder_for_function(&program.store, owner).unwrap();
        let schema::ExprView::Field { base, name: "format", .. } = decoder.expression_view(field).unwrap() else { panic!("actual exported alias projection") };
        let field_words = program.store.data[field].range().bounds(program.store.extra.len()).unwrap();
        let mut wrong_export = program.clone();
        let wrong_name = program.store.strings.iter().enumerate().find(|(index, _)| program.store.string(*index as u32 + 1).unwrap() == "prefix").unwrap().0 as u32 + 1;
        wrong_export.store.extra[field_words.start + 1] = wrong_name;
        assert!(FullVerifier::verify(&wrong_export).is_err());
        let base_words = program.store.data[base.raw() as usize].range().bounds(program.store.extra.len()).unwrap();
        let mut wrong_binding = program.clone();
        wrong_binding.store.extra[base_words.start] = 0;
        assert!(FullVerifier::verify(&wrong_binding).is_err());
        let capture = program.store.functions[owner].captures.bounds(program.store.captures.len()).unwrap().next().unwrap();
        let foreign_binding = program.store.binding_identities.iter().position(|binding| program.store.string(binding.name).unwrap() == "foreign").unwrap() as u32;
        let mut foreign_namespace = program.clone();
        foreign_namespace.store.captures[capture].binding_identity = foreign_binding;
        assert!(FullVerifier::verify(&foreign_namespace).is_err());
        let mut missing_capture = program.clone();
        missing_capture.store.captures[capture].binding_identity = u32::MAX;
        assert!(FullVerifier::verify(&missing_capture).is_err());
        let mut wrong_kind = program.clone();
        wrong_kind.store.function_metadata[target].callable_kind = crate::sema::types::CallableKind::Proc;
        assert!(FullVerifier::verify(&wrong_kind).is_err());
        let target_capture = program.store.functions[target].captures.bounds(program.store.captures.len()).unwrap().next().unwrap();
        let mut mutable_capture = program.clone();
        mutable_capture.store.captures[target_capture].slot_and_flags |= 1 << 31;
        assert!(FullVerifier::verify(&mutable_capture).is_err());
        let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test("imported_aliases", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
            assert!(matches!(&result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
        }
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn imported_pipeline_declarations_preserve_ground_and_generic_environments() {
        super::super::super::tests::run_with_large_stack(|| {
            let work = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".work/tmp");
            std::fs::create_dir_all(&work).unwrap();
            let modules = tempfile::tempdir_in(work).unwrap();
            std::fs::write(modules.path().join("operations.xsh"), r#"##! Imported declaration fixture.
let prefix = "module:"
## Render through the original environment and default.
export pure render(value: Str, suffix: Str = "!") -> Str { prefix + value + suffix }
## Preserve each caller's item domain.
export pure identity(value) { value }
## Require the actual two-argument signature.
export pure pair(value, other) { [value, other] }
"#).unwrap();
            std::fs::write(modules.path().join("foreign.xsh"), r#"##! Foreign declaration fixture.
let prefix = "foreign:"
## Render through a different environment.
export pure render(value: Str, suffix: Str = "!") -> Str { prefix + value + suffix }
## Require a different generic arity.
export pure identity(value, other) { let _ = other; value }
## Preserve the two-argument signature.
export pure pair(value, other) { [value, other] }
"#).unwrap();
            let source = r#"use operations
use foreign
pure declared(payload) { {payload, rows: ["kept"] |> map(operations.render)} }
pure imported(value) { [value] |> map(operations.identity) }
pure projected(value) { let action = operations.identity; let again = action; [value] |> map(again) }
pure dormant(value) { var counter = 0; counter = 1; let action = operations.identity; [value] |> map(action) }
test imported_declarations {
  assert declared(7) == {payload: 7, rows: ["module:kept!"]}
  assert declared("retained").payload == "retained"
  assert imported(7) == [7]
  assert imported("kept") == ["kept"]
  assert projected(7) == [7]
  assert projected("kept") == ["kept"]
  let data: Result[Int, Str] = Err("data")
  assert imported(data) == [data]
  assert projected(data) == [data]
}
"#;
            let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("imported-pipeline-declarations.xsh", crate::loader::entry_source_from_text("imported-pipeline-declarations.xsh", source.to_string()), vec![modules.path().to_path_buf()]);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let source_id = parsed.arena.source_text_source_id().unwrap();
            let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let program = FullBuilder::build_compact(&parsed.arena, &checked, source, Arc::new(sources.clone()), source_id).unwrap();
            FullVerifier::verify(&program).unwrap();
            let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "dormant").unwrap();
            let range = program.store.function_instruction_range(owner).unwrap();
            let decoder = super::super::decoder_for_function(&program.store, owner).unwrap();
            let field = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Field { name: "identity", .. }))).unwrap();
            let schema::ExprView::Field { base, .. } = decoder.expression_view(field).unwrap() else { unreachable!() };
            let field_words = program.store.data[field].range().bounds(program.store.extra.len()).unwrap();
            let mut wrong_export = program.clone();
            let pair = (1..=program.store.strings.len() as u32).find(|id| program.store.string(*id).unwrap() == "pair").unwrap();
            wrong_export.store.extra[field_words.start + 1] = pair;
            assert!(FullVerifier::verify(&wrong_export).is_err());
            let base_words = program.store.data[base.raw() as usize].range().bounds(program.store.extra.len()).unwrap();
            let mut wrong_receiver = program.clone();
            wrong_receiver.store.extra[base_words.start] = 0;
            assert!(FullVerifier::verify(&wrong_receiver).is_err());
            let fact = program.store.scheme_body_facts.iter().position(|fact| fact.function.index() == owner && fact.instruction as usize == field && fact.kind() == FullSchemeBodyFactKind::Expression).unwrap();
            let mut missing_fact = program.clone();
            remove_scheme_body_fact(&mut missing_fact, fact);
            assert!(FullVerifier::verify(&missing_fact).is_err());
            let call = program.store.scheme_callable_calls.iter().find(|call| range.contains(&(call.instruction as usize))).unwrap().instruction;
            let mut missing_relationship = program.clone();
            missing_relationship.store.scheme_callable_calls.retain(|site| site.instruction != call);
            assert!(FullVerifier::verify(&missing_relationship).is_err());
            let mut missing_instance = program.clone();
            missing_instance.store.call_instantiations.retain(|instance| instance.instruction != call);
            assert!(FullVerifier::verify(&missing_instance).is_err());
            let alias_slot = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
                Ok(schema::StmtView::Let { slot, value, .. }) if value.raw() as usize == field => Some(slot),
                _ => None,
            }).unwrap();
            let assignment = range.clone().find(|instruction| matches!(decoder.statement_view(*instruction), Ok(schema::StmtView::Assign { .. }) | Ok(schema::StmtView::AssignInt { .. }))).unwrap();
            let mut forged_writer = program.clone();
            forged_writer.store.extra[program.store.data[assignment].range().start as usize] = alias_slot as u32;
            assert!(FullVerifier::verify(&forged_writer).is_err());
            let captures = program.store.functions[owner].captures.bounds(program.store.captures.len()).unwrap();
            let capture = captures.into_iter().find(|index| program.store.string(program.store.captures[*index].name).unwrap() == "operations").unwrap();
            let foreign = program.store.binding_identities.iter().position(|binding| program.store.string(binding.name).unwrap() == "foreign").unwrap();
            let mut foreign_namespace = program.clone();
            foreign_namespace.store.captures[capture].binding_identity = foreign as u32;
            foreign_namespace.store.captures[capture].name = program.store.binding_identities[foreign].name;
            assert!(FullVerifier::verify(&foreign_namespace).is_err());
            let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
            for force_frames in [false, true] {
                let result = prepared.eval_test("imported_declarations", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
                assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
                assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
                assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
                assert!(matches!(&result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
            }
        });
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn imported_callable_namespace_borrow_preserves_shared_root_state() {
        super::super::super::tests::run_with_large_stack(|| {
            let work = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".work/tmp");
            std::fs::create_dir_all(&work).unwrap();
            let modules = tempfile::tempdir_in(work).unwrap();
            let marker = modules.path().join("initialized");
            let state = modules.path().join("state");
            let module = r#"##! Root namespace state fixture.
let prefix = "module:"
## Render through an immutable environment.
export pure render(value: Str, suffix: Str = "!") -> Str { prefix + value + suffix }
## Preserve the caller's item type.
export pure identity(value) { value }
## Retain the callable value.
export let format = render
let state = {
  let marker = p"INIT_MARKER"
  marker.write(marker.read_text()? + "I")?
  let state_file = p"STATE_PATH"
  state_file.write("init;")?
  state_file
}
## Observe the original shared state.
export proc probe(label: Str) [fs, io, error] -> Result[Str, Error] {
  print f"probe:{label}"
  let previous = state.read_text()?
  state.write(previous + label + ";")?
  previous
}
"#.replace("INIT_MARKER", marker.to_str().unwrap()).replace("STATE_PATH", state.to_str().unwrap());
            let source = r#"use borrowed
pure typed(payload: Int) { let action = borrowed.format; {payload, rows: ["kept"] |> map(action)} }
pure generic(payload) { let action = borrowed.format; {payload, rows: ["kept"] |> map(action)} }
pure declared(payload) { {payload, rows: ["kept"] |> map(borrowed.render)} }
pure forwarded(value) { [value] |> map(borrowed.identity) }
pure projected(value) { let action = borrowed.identity; [value] |> map(action) }
test namespace_borrow {
  assert borrowed.probe("before")? == "init;"
  assert typed(7) == {payload: 7, rows: ["module:kept!"]}
  assert borrowed.probe("typed")? == "init;before;"
  assert generic(7) == typed(7)
  assert declared(7) == typed(7)
  assert forwarded(7) == [7]
  assert projected(7) == [7]
  assert borrowed.probe("integer")? == "init;before;typed;"
  assert generic("kept") == {payload: "kept", rows: ["module:kept!"]}
  assert declared("kept").payload == "kept"
  assert forwarded("kept") == ["kept"]
  assert projected("kept") == ["kept"]
  assert borrowed.probe("string")? == "init;before;typed;integer;"
}
"#;
            for force_frames in [false, true] {
                std::fs::write(&marker, "").unwrap();
                std::fs::write(modules.path().join("borrowed.xsh"), &module).unwrap();
                let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("borrowed-pipeline-namespace.xsh", crate::loader::entry_source_from_text("borrowed-pipeline-namespace.xsh", source.to_string()), vec![modules.path().to_path_buf()]);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let source_id = parsed.arena.source_text_source_id().unwrap();
                let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
                assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
                let result = prepared.eval_test("namespace_borrow", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
                assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
                assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
                assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
                assert!(matches!(&result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
                assert_eq!(result.output.stdout, b"probe:before\nprobe:typed\nprobe:integer\nprobe:string\n", "force_frames={force_frames}");
                assert_eq!(std::fs::read_to_string(&marker).unwrap(), "I", "force_frames={force_frames}");
                assert_eq!(std::fs::read_to_string(&state).unwrap(), "init;before;typed;integer;string;", "force_frames={force_frames}");
            }
        });
    }

    #[test]
    fn imported_callable_projections_reject_resource_and_value_capture() {
        let work = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".work/tmp");
        std::fs::create_dir_all(&work).unwrap();
        let modules = tempfile::tempdir_in(work).unwrap();
        let module = "##! Scoped callable fixture.\nlet prefix = \"module:\"\n## Render one value.\nexport pure render(value: Str) -> Str { prefix + value }\n## Retain the callable value.\nexport let format = render\n";
        for (module, source) in [
            (module.to_owned(), "pure observed(payload) { {payload, leaked: helpers} }"),
            (format!("{module}export let number = 7\n"), "pure observed(payload) { {payload, leaked: helpers.number} }"),
            (format!("{module}export let resource: FsRoot? = null\n"), "pure observed(payload) { let action = helpers.format; {payload, rows: [\"kept\"] |> map(action)} }"),
            (module.replace("let prefix", "var prefix"), "pure observed(payload) { let action = helpers.format; {payload, rows: [\"kept\"] |> map(action)} }"),
            (module.replace("let prefix", "let root = fs.tempdir()?\nlet prefix"), "pure observed(payload) { let action = helpers.format; {payload, rows: [\"kept\"] |> map(action)} }"),
            (module.replace("pure render", "proc render").replace("(value: Str) -> Str { prefix + value }", "(value: Str) [env] -> Str { let _ = env.get(\"HOME\"); prefix + value }"), "pure observed(payload) { let action = helpers.format; {payload, rows: [\"kept\"] |> map(action)} }"),
            (module.to_owned(), "let copied = helpers\npure observed(payload) { let action = copied.format; {payload, rows: [\"kept\"] |> map(action)} }"),
            (module.to_owned(), "pure observed(action: Any, payload) { [payload] |> map(action) }"),
            (module.to_owned(), "pure identity(value) { value }\nvar action = identity\nlet number: Int = action(7)\nlet word: Str = action(\"kept\")"),
        ] {
            std::fs::write(modules.path().join("helpers.xsh"), module).unwrap();
            let source = format!("use helpers\n{source}\n");
            let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("rejected-imported-pipeline-alias.xsh", crate::loader::entry_source_from_text("rejected-imported-pipeline-alias.xsh", source.clone()), vec![modules.path().to_path_buf()]);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let source_id = parsed.arena.source_text_source_id().unwrap();
            let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
            if checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error) { continue; }
            assert!(FullBuilder::build_compact(&parsed.arena, &checked, &source, Arc::new(sources), source_id).is_err(), "{source}");
        }
    }

    #[test]
    fn generic_display_operations_authenticate_owner_conversion_requirements() {
        for definition in [
            "pure show(value) { f\"item:{value}\" }",
            "proc show(value) [io] { print (value); value }",
        ] {
            let program = prepare(definition);
            for mode in [0, 2, 128] {
                let mut changed = program.clone();
                let parameter = changed.store.scheme_type_parameters.iter_mut().find(|parameter| parameter.conversions != 0).unwrap();
                assert_eq!(parameter.conversions, 1);
                parameter.conversions = mode;
                assert!(FullVerifier::verify(&changed).is_err(), "{definition}: {mode}");
            }
        }
    }

    #[test]
    fn generic_argv_operations_authenticate_exact_physical_conversion_modes() {
        for (argument, expected) in [("(value)", 2), ("$value", 4), ("@value", 8)] {
            let program = prepare(&format!("proc send(value) [io, process, error] {{ run echo {argument}; value }}"));
            for mode in [0, 1, 128] {
                let mut changed = program.clone();
                let parameter = changed.store.scheme_type_parameters.iter_mut().find(|parameter| parameter.conversions != 0).unwrap();
                assert_eq!(parameter.conversions, expected);
                parameter.conversions = mode;
                assert!(FullVerifier::verify(&changed).is_err(), "{argument}: {mode}");
            }
        }
    }

    #[test]
    fn generic_conversion_proofs_follow_natural_body_work_bounds() {
        for width in [64, 128, 256, 512] {
            let names = (0..width).map(|index| format!("value{index}")).collect::<Vec<_>>();
            let parameters = names.join(", ");
            let parts = names.iter().map(|name| format!("{{{name}}}")).collect::<String>();
            prepare(&format!("pure render({parameters}) {{ f\"{parts}\" }}"));
        }
    }

    #[test]
    fn generic_conversion_returns_authenticate_the_owner_return_contract() {
        let program = prepare("proc choose(value, wrong: Result[Str]) [io] { print f\"{value}\"; let _ = wrong; 7 }");
        let (_, scheme) = generic_owner(&program);
        let parameters = program.store.schemes[scheme.index()].params.bounds(program.store.scheme_params.len()).unwrap();
        let wrong = program.store.scheme_params[parameters.start + 1].ty;
        let mut changed = program.clone();
        changed.store.schemes[scheme.index()].return_type = wrong;
        assert!(FullVerifier::verify(&changed).is_err());
    }

    #[test]
    fn generic_run_returns_authenticate_output_kind_and_permissions() {
        let program = prepare("proc send(marker) { let _ = marker; return run.text /bin/echo fixed ? }");
        let (owner, scheme) = generic_owner(&program);
        let capture = program.store.function_instruction_range(owner).unwrap()
            .find(|&instruction| program.store.tags[instruction] == super::super::super::FullTag::ExprRunCapture).unwrap();
        let words = program.store.data[capture].range().bounds(program.store.extra.len()).unwrap();
        let mut changed = program.clone();
        let kind = changed.store.run_kinds.len() as u32;
        changed.store.run_kinds.push(crate::syntax::node::RunKind::CaptureBytes);
        changed.store.extra[words.start] = kind;
        assert!(FullVerifier::verify(&changed).is_err());
        let effects = program.store.schemes[scheme.index()].effects.direct.bounds(program.store.scheme_effects.len()).unwrap();
        assert_eq!(effects.len(), 2);
        for forbidden in [crate::syntax::node::Effect::Process, crate::syntax::node::Effect::Error] {
            assert!(effects.clone().any(|index| program.store.scheme_effects[index] == forbidden));
            let remaining = effects.clone().find(|&index| program.store.scheme_effects[index] != forbidden).unwrap();
            let mut changed = program.clone();
            changed.store.schemes[scheme.index()].effects.direct = super::super::super::IrRange { start: remaining as u32, len: 1 };
            let error = FullVerifier::verify(&changed).unwrap_err();
            assert!(error.message.contains("effect"), "{forbidden:?}: {}", error.message);
        }
    }

    #[test]
    fn inferred_proc_completion_counts_plain_run_failure_once() {
        prepare("proc keep(value) [process, error] { run sh -c \"exit 1\"; value }");
    }

    #[test]
    fn inferred_proc_completion_counts_guarded_field_run_failure_once() {
        let program = build_with("proc keep(value) [process, error] { run sh -c \"exit 1\"; value?.name }", |checked| {
            let groups = checked.scheme_bodies.values().fold(0, |count, body| count + body.scheme().callable().requirements().wrappers.iter()
                .filter(|requirement| matches!(&requirement.kind, crate::sema::wrapper_operation::WrapperRequirementKind::ProcCompletion { .. }))
                .count());
            assert_eq!(groups, 1);
        });
        let (_, scheme) = generic_owner(&program);
        let wrappers = bounded(program.store.scheme(scheme).unwrap().wrappers, &program.store.scheme_wrappers, "wrappers").unwrap();
        assert_eq!(wrappers.iter().filter(|wrapper| matches!(wrapper, super::super::super::schemes::FullWrapper::ProcCompletion { .. })).count(), 1);
        FullVerifier::verify(&program).unwrap();
    }

    #[test]
    fn generic_index_capture_requires_its_conditional_operation_relation() {
        let program = prepare("pure safe(values, key, payload: Result[Int, Str]) -> Result[Int, Str] { let ignored = try { values[key] }; payload }");
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "safe").unwrap();
        let FullCallableContract::Scheme(scheme_id) = program.store.functions[owner].contract else { unreachable!() };
        let scheme = program.store.scheme(scheme_id).unwrap();
        let wrappers = bounded(scheme.wrappers, &program.store.scheme_wrappers, "wrapper range is invalid").unwrap();
        let (result, detail) = wrappers.iter().find_map(|wrapper| match wrapper {
            super::super::super::schemes::FullWrapper::Capture { result, detail } => Some((*result, *detail)),
            _ => None,
        }).unwrap();
        let capture = program.store.wrapper_captures[detail as usize];
        let contributions = capture.contributions.bounds(program.store.wrapper_contributions.len()).unwrap();
        assert_eq!(contributions.len(), 1);
        assert!(matches!(program.store.wrapper_contributions[contributions.start], super::super::super::schemes::FullWrapperContribution::KeyValidation(_)));

        let mut missing_relation = program.clone();
        missing_relation.store.wrapper_contributions[capture.contributions.start as usize] =
            super::super::super::schemes::FullWrapperContribution::Explicit(result);
        let error = FullVerifier::verify(&missing_relation).unwrap_err();
        assert_eq!(error.message, "capture contributions disagree with its actual body");
        assert!(!error.message.contains("unowned"), "the forged relation retained its owned contribution row");

        let concrete = prepare("pure safe(marker, values: Map[UInt, Int], key: Int, payload: Result[Int, Str]) -> Result[Int, Str] { let _ = marker; let ignored = try { values[key] }; payload }");
        let owner = concrete.store.functions.iter().position(|function| concrete.store.string(function.name).unwrap() == "safe").unwrap();
        let FullCallableContract::Scheme(scheme_id) = concrete.store.functions[owner].contract else { unreachable!() };
        let scheme = concrete.store.scheme(scheme_id).unwrap();
        let wrappers = bounded(scheme.wrappers, &concrete.store.scheme_wrappers, "wrapper range is invalid").unwrap();
        let (operation, detail) = wrappers.iter().find_map(|wrapper| match wrapper {
            super::super::super::schemes::FullWrapper::KeyValidation { operation, detail } => Some((*operation, *detail)),
            _ => None,
        }).expect("the concrete UInt-key operation retains its lexical capture boundary");
        assert_eq!(
            concrete.store.wrapper_key_validations[detail as usize].boundary,
            super::super::super::schemes::FullWrapperBoundary::Captured,
        );
        assert!(matches!(bounded(scheme.operations, &concrete.store.scheme_operations, "operation range is invalid").unwrap().get(operation as usize), Some(super::super::super::schemes::FullSealedOperation::Index { .. })));
        FullVerifier::verify(&concrete).unwrap();
    }

    fn remove_scheme_body_fact(program: &mut FullProgram, instruction: usize) {
        for scheme in &mut program.store.schemes {
            let start = scheme.body_facts.start as usize;
            let end = start + scheme.body_facts.len as usize;
            if start <= instruction && instruction < end {
                scheme.body_facts.len -= 1;
            } else if start > instruction {
                scheme.body_facts.start -= 1;
            }
        }
        program.store.scheme_body_facts.remove(instruction);
    }

    fn append_scheme_body_fact(program: &mut FullProgram, scheme_id: FullSchemeId, fact: super::super::super::schemes::FullSchemeBodyFact) {
        let range = program.store.schemes[scheme_id.index()].body_facts;
        let start = range.start as usize;
        let facts = &program.store.scheme_body_facts[start..start + range.len as usize];
        let key = (fact.instruction, fact.kind() as u8);
        let insertion = start + facts.partition_point(|existing| (existing.instruction, existing.kind() as u8) <= key);
        program.store.scheme_body_facts.insert(insertion, fact);
        for (index, scheme) in program.store.schemes.iter_mut().enumerate() {
            if index == scheme_id.index() {
                scheme.body_facts.len += 1;
            } else if scheme.body_facts.start as usize >= insertion {
                scheme.body_facts.start += 1;
            }
        }
    }

    #[test]
    fn record_constructors_preserve_union_list_field_producers() {
        prepare("type Word = Union[Str, Path]\n\ntype Task = {tool: Str, args: List[Word]}\n\n\nproc build(root: Path) [process, error] {\n  let task = Task(tool: \"make\", args: [\"-C\", root, \"all\"])\n\n  for word in task.args {\n    if word is Path {\n      print f\"in {word}\"\n      continue\n    }\n\n    print f\"word {word}\"\n  }\n}\n\n\nbuild(p\".\")\n");
    }

    #[test]
    fn union_record_literal_fields_require_actual_children_sources_and_owners() {
        let program = prepare("type Word = Union[Str, Path]\ntype Task = {tool: Str, args: List[Word]}\nproc foreign(root: Path) [] { let _ = root }\nproc build(root: Path, raw: List[Any], bad: Int, words: List[Word]) [process, error] {\n  let _ = raw\n  let _ = bad\n  let task = Task(tool: \"make\", args: [\"-C\", root, @words])\n  for word in task.args {\n    if word is Path { print f\"in {word}\"; continue }\n    print f\"word {word}\"\n  }\n}\nbuild(p\".\", [\"spare\"], 7, [\"all\"])\n");
        let _symbols = program.symbol_owner().enter();
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "build").unwrap();
        let foreign = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "foreign").unwrap();
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let (list, root, splice) = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::ListBuild(entries)) => {
                let entries = entries.collect::<Result<Vec<_>, _>>().unwrap();
                let root = entries.iter().find_map(|(splice, value, _)| (!splice && matches!(decoder.expression_view(value.raw() as usize).unwrap(), schema::ExprView::Param(0))).then_some(value.raw() as usize)).unwrap();
                let splice = entries.iter().find_map(|(splice, value, _)| splice.then_some(value.raw() as usize)).unwrap();
                Some((instruction, root, splice))
            }
            _ => None,
        }).unwrap();
        let field = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::Record(mut fields)) => decoder.checked_record_entries(&mut fields).unwrap().into_iter().find_map(|entry| match entry {
                super::super::super::CheckedRecordEntry::Field(name, value) if name == "args" => Some(value),
                _ => None,
            }),
            _ => None,
        }).unwrap();
        assert!(matches!(decoder.expression_view(field).unwrap(), schema::ExprView::Param(_)));
        assert!(matches!(decoder.expression_view(splice).unwrap(), schema::ExprView::Param(3)));
        let cold = || { let mut changed = program.clone(); changed.store.in_place.take(); assert!(changed.store.in_place.get().is_none()); changed };
        FullVerifier::verify(&cold()).unwrap();
        for (label, instruction, slot) in [("literal member", root, 1), ("nonlist splice", splice, 2), ("written Any list", field, 1)] {
            let mut changed = cold();
            let words = changed.store.data[instruction].range().bounds(changed.store.extra.len()).unwrap();
            assert_eq!(words.len(), 1);
            changed.store.extra[words.start] = slot;
            let error = FullVerifier::verify(&changed).expect_err(label);
            assert!(!error.message.contains("unowned rows"), "{label}: {error:?}");
            assert!(error.message.starts_with("ground producer value disagrees with its checked contract")
                || error.message == "checked record field disagrees with its contract"
                || error.message.contains("list splice"), "{label}: {error:?}");
        }
        let foreign_decoder = decoder_for_function(&program.store, foreign).unwrap();
        let foreign_value = program.store.function_instruction_range(foreign).unwrap().find(|instruction| matches!(foreign_decoder.expression_view(*instruction), Ok(schema::ExprView::Param(0)))).unwrap();
        let mut wrong_owner = cold();
        let schema::ExprView::ListBuild(entries) = decoder.expression_view(list).unwrap() else { unreachable!() };
        let block = entries.block_identity_for_test();
        let words = wrong_owner.store.blocks[block.index()].instructions.bounds(wrong_owner.store.extra.len()).unwrap();
        let references = words.filter(|word| wrong_owner.store.extra[*word] == root as u32).collect::<Vec<_>>();
        assert_eq!(references.len(), 1);
        wrong_owner.store.extra[references[0]] = foreign_value as u32;
        assert_eq!(FullVerifier::verify(&wrong_owner).unwrap_err().message, "full IR instruction belongs to another function");
    }

    #[test]
    fn path_method_results_authenticate_receivers_members_and_consumers() {
        let program = prepare(r#"
pure parent_transform(location, other: Int) { let _ = other; let _ = "unknown"; location.parent() }
pure normalize_transform(location) { location.normalize() }
pure path_text(location: Path) -> Str { location.display() }
pure contextual(location) { let first = parent_transform(location, 7); path_text(first) }
let relative: RelPath = p"a/b"
let parent_relative: RelPath = parent_transform(relative, 7)
let parent_absolute: Path = parent_transform(p"/a/b", 7)
"#);
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "parent_transform").unwrap();
        let other = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "normalize_transform").unwrap();
        let FullCallableContract::Scheme(scheme_id) = program.store.functions[owner].contract else { unreachable!() };
        let scheme = program.store.scheme(scheme_id).unwrap();
        let method_index = scheme.methods.start as usize;
        assert_eq!(scheme.methods.len, 1);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let method = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Method { .. }))).unwrap();
        let integer = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(1)))).unwrap();
        let reject = |mut changed: FullProgram, label: &str| {
            changed.store.in_place.take();
            assert!(changed.store.in_place.get().is_none());
            let error = FullVerifier::verify(&changed).expect_err(label);
            assert!(!error.message.contains("unowned rows"), "{label}: {error:?}");
        };
        let receivers = range.clone().filter(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(0)))).collect::<Vec<_>>();
        assert_eq!(receivers.len(), 1);
        let receiver = receivers[0];
        assert!(matches!(decoder.expression_view(integer).unwrap(), schema::ExprView::Param(1)));
        let mut wrong_receiver = program.clone();
        let receiver_words = wrong_receiver.store.data[receiver].range().bounds(wrong_receiver.store.extra.len()).unwrap();
        assert_eq!(receiver_words.len(), 1);
        wrong_receiver.store.extra[receiver_words.start] = 1;
        reject(wrong_receiver, "actual method receiver");
        let words = program.store.data[method].range().bounds(program.store.extra.len()).unwrap();
        let normal_name = program.store.strings.iter().enumerate().find(|(index, _)| program.store.string(*index as u32 + 1).unwrap() == "normalize").unwrap().0 as u32 + 1;
        let mut wrong_name = program.clone();
        wrong_name.store.extra[words.start + 1] = normal_name;
        reject(wrong_name, "actual method name");
        let mut wrong_family = program.clone();
        wrong_family.store.scheme_methods[method_index].family = xsh_registry::signature::MethodFamilyId::PathNormalize;
        reject(wrong_family, "retained method family");
        let mut wrong_result = program.clone();
        let result_type = program.store.scheme_methods[method_index].result;
        let receiver_type = program.store.scheme_methods[method_index].receiver;
        assert_eq!(program.store.scheme_types[result_type.index()].tag, FullSchemeTypeTag::Parameter);
        wrong_result.store.scheme_types[result_type.index()].first = program.store.scheme_types[receiver_type.index()].first;
        reject(wrong_result, "independent result binder");
        let method_fact = program.store.scheme_body_facts.iter().position(|fact| fact.function.index() == owner && matches!(fact.value, super::super::super::schemes::FullSchemeBodyFactValue::Method(_))).unwrap();
        let mut wrong_owner = program.clone();
        wrong_owner.store.scheme_body_facts[method_fact].function = IrFunctionId::new(other).unwrap();
        reject(wrong_owner, "method source owner");
        let contextual = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "contextual").unwrap();
        let FullCallableContract::Scheme(contextual_scheme) = program.store.functions[contextual].contract else { unreachable!() };
        let contextual_scheme = program.store.scheme(contextual_scheme).unwrap();
        let known = contextual_scheme.applications.bounds(program.store.scheme_applications.len()).unwrap().find_map(|index| match program.store.scheme_applications[index] {
            super::super::super::schemes::FullSchemeApplication::KnownArguments { parameters, arguments } => Some((parameters, arguments)),
            _ => None,
        }).unwrap();
        let context_input = program.store.scheme_params[contextual_scheme.params.start as usize].ty;
        let mut wrong_edge = program.clone();
        let argument = known.1.start as usize;
        let argument_type = program.store.scheme_application_arguments[argument].ty;
        assert_eq!(program.store.scheme_types[argument_type.index()].tag, FullSchemeTypeTag::Parameter);
        assert_ne!(program.store.scheme_types[argument_type.index()].first, program.store.scheme_types[context_input.index()].first);
        wrong_edge.store.scheme_types[argument_type.index()].first = program.store.scheme_types[context_input.index()].first;
        reject(wrong_edge, "required actual result edge");
        let mut wrong_formal = program.clone();
        let integer_type = program.store.scheme_params[scheme.params.start as usize + 1].ty;
        let formal_type = program.store.scheme_callable_parameters[known.0.start as usize].ty;
        assert_eq!(program.store.scheme_types[formal_type.index()].tag, FullSchemeTypeTag::Ground);
        wrong_formal.store.scheme_types[formal_type.index()].first = program.store.scheme_types[integer_type.index()].first;
        reject(wrong_formal, "required consumer formal");
    }

    #[test]
    fn authored_path_method_returns_authenticate_their_header_and_actual_tail() {
        use super::super::super::schemes::FullSchemeBodyFactValue;
        let program = prepare("pure written(location, other) -> Path { let _ = other.parent(); location.parent() }\npure foreign(location: Path) -> Path { location }\nlet relative: RelPath = p\"a/b\"\nlet _ = written(p\"/a/b\", relative)\n");
        let (owner, scheme_id) = generic_owner(&program);
        let scheme = program.store.scheme(scheme_id).unwrap();
        assert!(scheme.explicit_return);
        assert_eq!(scheme.methods.len, 2);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let value = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Return { value }) => Some(value.raw()),
            _ => None,
        }).unwrap();
        let (fact, tail) = scheme.body_facts.bounds(program.store.scheme_body_facts.len()).unwrap().find_map(|index| match program.store.scheme_body_facts[index] {
            super::super::super::schemes::FullSchemeBodyFact { instruction, value: FullSchemeBodyFactValue::Expression(ty), .. } if instruction == value => Some((index, ty)),
            _ => None,
        }).unwrap();
        assert_eq!(program.store.scheme_types[tail.index()].tag, FullSchemeTypeTag::Parameter);
        let other_result = bounded(scheme.methods, &program.store.scheme_methods, "methods").unwrap().iter().map(|method| method.result).find(|result| program.store.scheme_types[result.index()].first != program.store.scheme_types[tail.index()].first).unwrap();
        let reject = |mut changed: FullProgram, label: &str| {
            changed.store.in_place.take();
            assert!(changed.store.in_place.get().is_none());
            let error = FullVerifier::verify(&changed).expect_err(label);
            assert!(!error.message.contains("unowned rows"), "{label}: {error:?}");
        };
        let mut unwritten = program.clone();
        unwritten.store.schemes[scheme_id.index()].explicit_return = false;
        reject(unwritten, "actual authored return flag");
        let mut wrong_tail = program.clone();
        wrong_tail.store.scheme_types[tail.index()].first = program.store.scheme_types[other_result.index()].first;
        reject(wrong_tail, "actual returned result binder");
        let mut wrong_owner = program.clone();
        let foreign = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "foreign").unwrap();
        assert_ne!(owner, foreign);
        wrong_owner.store.scheme_body_facts[fact].function = IrFunctionId::new(foreign).unwrap();
        reject(wrong_owner, "actual return source owner");
    }

    #[test]
    fn unused_path_method_results_still_require_canonical_call_evidence() {
        let program = prepare("pure unused_parent(location) { let _ = location.parent(); true }\nlet relative: RelPath = p\"a/b\"\nassert unused_parent(relative)\nassert unused_parent(p\"/a/b\")\n");
        let (owner, scheme_id) = generic_owner(&program);
        let scheme = program.store.scheme(scheme_id).unwrap();
        let method = program.store.scheme_methods[scheme.methods.start as usize];
        let result = program.store.scheme_types[method.result.index()];
        assert_eq!(result.tag, FullSchemeTypeTag::Parameter);
        let scopes = verify_function_schemes(&program.store, &mut effects::EffectCoverage::new(&program.store)).unwrap();
        let scope = scopes[scheme_id.index()].as_ref().unwrap();
        let mut budget = SchemeInstantiationBudget::new(&program.store);
        let path_type = program.store.call_type_arguments.iter().find_map(|argument| {
            (program.store.scheme_types[argument.ty.index()].tag == FullSchemeTypeTag::Ground
                && decode_scheme_type(&program.store, scope, argument.ty, &mut budget, 0).unwrap().closed_ground_type() == Some(Type::Path)).then_some(argument.ty)
        }).unwrap();
        let call = program.store.call_instantiations.iter().find(|call| {
            program.store.checked_call_targets.iter().any(|target| target.instruction == call.instruction && target.function.index() == owner)
                && bounded(call.arguments.type_arguments, &program.store.call_type_arguments, "arguments").unwrap().iter().any(|argument| argument.binder == result.first
                    && decode_scheme_type(&program.store, scope, argument.ty, &mut SchemeInstantiationBudget::new(&program.store), 0).unwrap().closed_ground_type() == Some(Type::rel_path()))
        }).unwrap();
        let arguments = call.arguments.type_arguments.bounds(program.store.call_type_arguments.len()).unwrap();
        let output = arguments.clone().find(|&index| program.store.call_type_arguments[index].binder == result.first).unwrap();
        let mut wrong_result = program.clone();
        wrong_result.store.in_place.take();
        assert!(wrong_result.store.in_place.get().is_none());
        let result_type = program.store.call_type_arguments[output].ty;
        let receiver_binder = program.store.scheme_types[method.receiver.index()].first;
        let receiver_type = arguments.clone().find_map(|index| (program.store.call_type_arguments[index].binder == receiver_binder).then_some(program.store.call_type_arguments[index].ty)).unwrap();
        assert_ne!(result_type, receiver_type);
        assert_eq!(program.store.scheme_types[result_type.index()].tag, FullSchemeTypeTag::Ground);
        wrong_result.store.scheme_types[result_type.index()].first = program.store.scheme_types[path_type.index()].first;
        let error = FullVerifier::verify(&wrong_result).unwrap_err();
        assert!(error.message.contains("canonical result"), "{error:?}");
        let member = crate::modules::api_spec().method_family_members(xsh_registry::signature::MethodFamilyId::PathParent).find(|(_, domain, _, _)| *domain == crate::modules::MethodReceiver::Path).unwrap().0;
        let mut wrong_member = program.clone();
        wrong_member.store.in_place.take();
        assert!(wrong_member.store.in_place.get().is_none());
        assert_eq!(call.arguments.method_bindings.len, 1);
        wrong_member.store.call_method_bindings[call.arguments.method_bindings.start as usize] = super::super::super::schemes::FullMethodBinding::Selected(member);
        assert!(FullVerifier::verify(&wrong_member).unwrap_err().message.contains("canonical input signature"));
    }

    #[test]
    fn generic_fixed_fields_require_the_actual_closed_receiver_and_member() {
        let program = prepare("pure projected(marker, status: Status, number: Int) { let _ = marker; let _ = number; let _ = status.kind; status.ok }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let (field, base) = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::Field { base, name: "ok", .. }) => Some((instruction, base.raw() as usize)),
            _ => None,
        }).unwrap();
        let number = program.store.function_instruction_range(owner).unwrap().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(2)))).unwrap();
        assert_ne!(base, number);
        for name in ["kind", "projected"] {
            let name = program.store.strings.iter().enumerate().find(|(index, _)| program.store.string(*index as u32 + 1).unwrap() == name).unwrap().0 as u32 + 1;
            let mut changed = program.clone();
            changed.store.in_place.take();
            let words = changed.store.data[field].range().bounds(changed.store.extra.len()).unwrap();
            changed.store.extra[words.start + 1] = name;
            assert!(FullVerifier::verify(&changed).is_err());
        }
        let mut changed = program.clone();
        changed.store.in_place.take();
        let words = changed.store.data[field].range().bounds(changed.store.extra.len()).unwrap();
        changed.store.extra[words.start] = number as u32;
        assert!(FullVerifier::verify(&changed).is_err());
    }

    #[test]
    fn generic_proc_bare_returns_authenticate_the_actual_unit_completion() {
        let program = prepare("proc early(marker, done: Bool) { return when done; let _ = marker }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let mut range = program.store.function_instruction_range(owner).unwrap();
        let done = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(1)))).unwrap();
        let returned = range.find(|&instruction| matches!(decoder.statement_view(instruction), Ok(schema::StmtView::Return { .. }))).unwrap();
        let schema::StmtView::Return { value } = decoder.statement_view(returned).unwrap() else { unreachable!() };
        let schema::ExprView::Ok(unit) = decoder.expression_view(value.raw() as usize).unwrap() else { panic!("bare procedure return wraps its Unit success") };
        assert!(matches!(decoder.expression_view(unit.raw() as usize).unwrap(), schema::ExprView::Unit));
        let mut changed = program.clone();
        changed.store.in_place.take();
        let words = changed.store.data[value.raw() as usize].range().bounds(changed.store.extra.len()).unwrap();
        assert_eq!(words.len(), 1);
        changed.store.extra[words.start] = done as u32;
        assert!(FullVerifier::verify(&changed).is_err());
    }

    #[test]
    fn proc_completion_source_facts_authenticate_original_return_values() {
        let program = prepare("proc projected(value, marker: Str) [error] { let _ = marker; value?.name }");
        assert_eq!(std::mem::size_of::<super::super::super::schemes::FullSchemeBodyFact>(), 16);
        let (owner, scheme_id) = generic_owner(&program);
        let scheme = program.store.scheme(scheme_id).unwrap();
        let facts = scheme.body_facts.bounds(program.store.scheme_body_facts.len()).unwrap();
        let (source_index, source_fact, producer) = facts.clone().find_map(|index| {
            let fact = program.store.scheme_body_facts[index];
            fact.value.completion_source().map(|source| (index, fact, source.producer as usize))
        }).unwrap();
        assert_eq!(source_fact.function.index(), owner);
        assert!(matches!(source_fact.value, super::super::super::schemes::FullSchemeBodyFactValue::CompletionLiftedValue(_)));
        FullVerifier::verify(&program).unwrap();

        let other_producer = facts.clone().find_map(|index| {
            let fact = program.store.scheme_body_facts[index];
            matches!(fact.value, super::super::super::schemes::FullSchemeBodyFactValue::Expression(_))
                .then_some(fact.instruction as usize)
                .filter(|instruction| *instruction != producer)
        }).unwrap();
        let mut retargeted = program.clone();
        retargeted.store.scheme_body_facts[source_index].value = super::super::super::schemes::FullSchemeBodyFactValue::CompletionLiftedValue(other_producer as u32);
        assert!(FullVerifier::verify(&retargeted).unwrap_err().message.contains("wrap its recorded source"));

        let wrong_type = facts.clone().find_map(|index| {
            let fact = program.store.scheme_body_facts[index];
            match fact.value {
                super::super::super::schemes::FullSchemeBodyFactValue::Expression(ty) if fact.instruction as usize != producer => Some(ty),
                _ => None,
            }
        }).unwrap();
        let producer_fact = facts.clone().find(|&index| {
            let fact = program.store.scheme_body_facts[index];
            fact.instruction as usize == producer && matches!(fact.value, super::super::super::schemes::FullSchemeBodyFactValue::Expression(_))
        }).unwrap();
        let super::super::super::schemes::FullSchemeBodyFactValue::Expression(original_type) = program.store.scheme_body_facts[producer_fact].value else { unreachable!() };
        assert_ne!(wrong_type, original_type);
        let mut retagged = program.clone();
        retagged.store.scheme_body_facts[producer_fact].value = super::super::super::schemes::FullSchemeBodyFactValue::Expression(wrong_type);
        assert!(FullVerifier::verify(&retagged).is_err());

        let mut wrong_form = program.clone();
        wrong_form.store.scheme_body_facts[source_index].value = super::super::super::schemes::FullSchemeBodyFactValue::CompletionReturnedResult(producer as u32);
        assert!(FullVerifier::verify(&wrong_form).is_err());

        let mut wrong_owner = program.clone();
        wrong_owner.store.scheme_body_facts[source_index].function = super::super::super::IrFunctionId::from_raw(u32::MAX - 1).unwrap();
        assert!(FullVerifier::verify(&wrong_owner).unwrap_err().message.contains("body fact has invalid ownership"));

        let mut missing = program.clone();
        remove_scheme_body_fact(&mut missing, source_index);
        assert!(FullVerifier::verify(&missing).unwrap_err().message.contains("no authenticated completion source"));

        let mut duplicate = program.clone();
        append_scheme_body_fact(&mut duplicate, scheme_id, source_fact);
        assert!(FullVerifier::verify(&duplicate).unwrap_err().message.contains("generic body fact is duplicated"));
    }

    #[test]
    fn proc_completion_sources_cannot_claim_an_application_return() {
        let program = prepare("type TextRow = {name: Str}\nproc projected(value) [error] { value?.name }\nproc relay(value) { projected(value) }\nproc relay_explicit(first, second, value, flag: Bool) [error] { let _ = value?.name; if flag { return first(value) }; second(value) }\nproc authored(row, selected: Bool) [error] { let _ = row?.name; if selected { return Ok(\"done\") }; return Err(\"failed\") }\nproc construct(value) { Ok({name: value}) }\nproc selected_forward(marker, value) [error] { let _ = marker?.name; projected(construct(value)) }\nproc mixed(value) [error] { let _ = value?.name; projected(value) }\nproc mixed_sources(left, right) [error] { let _ = left?.name; projected(right) }\nproc mixed_reverse(left, right) [error] { let _ = right?.name; projected(left) }\nproc mixed_explicit_result(left, right) [error] { let _ = left?.name; return right }");
        assert_eq!(std::mem::size_of::<super::super::super::schemes::FullWrapperContribution>(), 8);
        let owner = program.store.functions.iter().enumerate().find(|(_, function)| program.store.string(function.name).unwrap() == "relay_explicit").map(|(owner, _)| owner).unwrap();
        let sites = program.store.scheme_application_returns.iter().filter(|site| site.function.index() == owner).copied().collect::<Vec<_>>();
        assert_eq!(sites.len(), 2);
        let site = sites[0];
        let decoder = super::super::decoder_for_function(&program.store, owner).unwrap();
        let schema::StmtView::Return { value } = decoder.statement_view(site.instruction as usize).unwrap() else { panic!("application site owns a return") };
        let FullCallableContract::Scheme(scheme_id) = program.store.functions[owner].contract else { unreachable!() };
        let fact = super::super::super::schemes::FullSchemeBodyFact {
            instruction: site.instruction,
            function: super::super::super::IrFunctionId::new(owner).unwrap(),
            value: super::super::super::schemes::FullSchemeBodyFactValue::CompletionReturnedResult(value.raw()),
        };
        let mut conflict = program.clone();
        append_scheme_body_fact(&mut conflict, scheme_id, fact);
        assert!(FullVerifier::verify(&conflict).unwrap_err().message.contains("Proc completion source"));

        let mixed_owner = program.store.functions.iter().enumerate().find(|(_, function)| program.store.string(function.name).unwrap() == "mixed").map(|(owner, _)| owner).unwrap();
        let FullCallableContract::Scheme(mixed_scheme_id) = program.store.functions[mixed_owner].contract else { unreachable!() };
        let mixed_scheme = program.store.scheme(mixed_scheme_id).unwrap();
        let mixed_wrappers = bounded(mixed_scheme.wrappers, &program.store.scheme_wrappers, "mixed wrapper range is invalid").unwrap();
        let completion_ordinals = mixed_wrappers.iter().enumerate().filter_map(|(ordinal, wrapper)| matches!(wrapper, super::super::super::schemes::FullWrapper::ProcCompletion { .. }).then_some(ordinal as u32)).collect::<Vec<_>>();
        assert_eq!(completion_ordinals.len(), 2);
        let local_group = program.store.scheme_body_facts.iter().filter(|fact| fact.function.index() == mixed_owner).find_map(|fact| {
            let super::super::super::schemes::FullSchemeBodyFactValue::Wrapper(ordinal) = fact.value else { return None; };
            match mixed_wrappers.get(ordinal as usize)? {
                super::super::super::schemes::FullWrapper::Field { detail, .. } => match program.store.wrapper_fields.get(*detail as usize)?.boundary {
                    super::super::super::schemes::FullWrapperBoundary::ProcCompletion { group, .. } => Some(group),
                    _ => None,
                },
                _ => None,
            }
        }).unwrap();
        let projected_owner = program.store.functions.iter().enumerate().find(|(_, function)| program.store.string(function.name).unwrap() == "projected").map(|(owner, _)| owner).unwrap();
        let FullCallableContract::Scheme(projected_scheme_id) = program.store.functions[projected_owner].contract else { unreachable!() };
        let projected_wrappers = bounded(program.store.scheme(projected_scheme_id).unwrap().wrappers, &program.store.scheme_wrappers, "projected wrapper range is invalid").unwrap();
        let projected_group = projected_wrappers.iter().position(|wrapper| matches!(wrapper, super::super::super::schemes::FullWrapper::ProcCompletion { .. })).unwrap();
        let mixed_range = program.store.function_instruction_range(mixed_owner).unwrap();
        let instantiation = program.store.call_instantiations.iter().find(|instantiation| mixed_range.contains(&(instantiation.instruction as usize))).unwrap();
        let forwarded = bounded(instantiation.arguments.wrapper_bindings, &program.store.call_wrapper_bindings, "mixed call wrapper binding range is invalid").unwrap();
        let super::super::super::schemes::FullWrapperBinding::Forwarded(imported_group) = forwarded[projected_group] else { panic!("projected completion group is forwarded") };
        assert_ne!(local_group, imported_group);
        assert!(completion_ordinals.contains(&local_group));
        assert!(completion_ordinals.contains(&imported_group));

        let selected_owner = program.store.functions.iter().enumerate().find(|(_, function)| program.store.string(function.name).unwrap() == "selected_forward").map(|(owner, _)| owner).unwrap();
        let projected_owner = program.store.functions.iter().enumerate().find(|(_, function)| program.store.string(function.name).unwrap() == "projected").map(|(owner, _)| owner).unwrap();
        let FullCallableContract::Scheme(selected_scheme_id) = program.store.functions[selected_owner].contract else { unreachable!() };
        let FullCallableContract::Scheme(projected_scheme_id) = program.store.functions[projected_owner].contract else { unreachable!() };
        let projected_wrappers = bounded(program.store.scheme(projected_scheme_id).unwrap().wrappers, &program.store.scheme_wrappers, "projected wrapper range is invalid").unwrap();
        let projected_group = projected_wrappers.iter().position(|wrapper| matches!(wrapper, super::super::super::schemes::FullWrapper::ProcCompletion { .. })).unwrap();
        let projected_field = projected_wrappers.iter().position(|wrapper| match wrapper {
            super::super::super::schemes::FullWrapper::Field { detail, .. } => matches!(program.store.wrapper_fields.get(*detail as usize).map(|field| field.boundary), Some(super::super::super::schemes::FullWrapperBoundary::ProcCompletion { group, .. }) if group as usize == projected_group),
            _ => false,
        }).unwrap();
        let selected_range = program.store.function_instruction_range(selected_owner).unwrap();
        let selected_call = program.store.call_instantiations.iter().find(|call| {
            selected_range.contains(&(call.instruction as usize))
                && call.arguments.wrapper_bindings.len != 0
                && program.store.checked_call_targets.iter().any(|target| target.instruction == call.instruction && target.function.index() == projected_owner)
        }).unwrap();
        let selected_bindings = bounded(selected_call.arguments.wrapper_bindings, &program.store.call_wrapper_bindings, "selected_forward call wrapper range is invalid").unwrap();
        let super::super::super::schemes::FullWrapperBinding::Forwarded(selected_group) = selected_bindings[projected_group] else { panic!("selected retained completion group is forwarded") };
        let super::super::super::schemes::FullWrapperBinding::Forwarded(selected_field_ordinal) = selected_bindings[projected_field] else { panic!("selected field relationship is forwarded") };
        let selected_wrappers = bounded(program.store.scheme(selected_scheme_id).unwrap().wrappers, &program.store.scheme_wrappers, "selected_forward wrapper range is invalid").unwrap();
        let Some(super::super::super::schemes::FullWrapper::Field { detail, .. }) = selected_wrappers.get(selected_field_ordinal as usize) else { panic!("forwarded field ordinal resolves to the selected field wrapper") };
        let super::super::super::schemes::FullWrapperBoundary::ProcCompletion { group: selected_field_group, .. } = program.store.wrapper_fields[*detail as usize].boundary else { panic!("selected field belongs to a proc completion group") };
        assert_eq!(selected_field_group, selected_group);
        assert_ne!(selected_group as usize, projected_group);

        let mut missing_selected_group = program.clone();
        let group_binding = selected_call.arguments.wrapper_bindings.start as usize + projected_group;
        missing_selected_group.store.call_wrapper_bindings[group_binding] = super::super::super::schemes::FullWrapperBinding::Selected(crate::sema::wrapper_operation::WrapperSelection::ProcCompletion);
        assert_eq!(FullVerifier::verify(&missing_selected_group).unwrap_err().message, "forwarded wrapper refers to an unforwarded proc completion group");

        let mut wrong_group = program.clone();
        let binding = instantiation.arguments.wrapper_bindings.start as usize + projected_group;
        wrong_group.store.call_wrapper_bindings[binding] = super::super::super::schemes::FullWrapperBinding::Forwarded(local_group);
        assert!(FullVerifier::verify(&wrong_group).unwrap_err().message.contains("forwarded wrapper"));

        let authored_owner = program.store.functions.iter().enumerate().find(|(_, function)| program.store.string(function.name).unwrap() == "authored").map(|(owner, _)| owner).unwrap();
        let FullCallableContract::Scheme(_) = program.store.functions[authored_owner].contract else { unreachable!() };
        let authored_decoder = super::super::decoder_for_function(&program.store, authored_owner).unwrap();
        let authored_producer = program.store.scheme_body_facts.iter().find_map(|fact| {
            if fact.function.index() != authored_owner { return None; }
            match fact.value {
                super::super::super::schemes::FullSchemeBodyFactValue::CompletionAuthoredErr(producer) => Some(producer as usize),
                _ => None,
            }
        }).unwrap();
        let schema::ExprView::Param(authored_slot) = authored_decoder.expression_view(authored_producer).unwrap() else { panic!("authored Err source is retained through its ANF binding") };
        let authored_range = program.store.function_instruction_range(authored_owner).unwrap();
        let (authored_declaration, authored_initializer) = authored_range.clone().find_map(|instruction| match authored_decoder.statement_view(instruction) {
            Ok(schema::StmtView::Let { slot, value, .. }) if slot == authored_slot && matches!(authored_decoder.expression_view(value.raw() as usize), Ok(schema::ExprView::Err { .. })) => Some((instruction, value.raw() as usize)),
            _ => None,
        }).unwrap();
        let authored_local_fact = program.store.scheme_body_facts.iter().position(|fact| {
            fact.function.index() == authored_owner && fact.instruction as usize == authored_declaration && fact.kind() == FullSchemeBodyFactKind::LocalImmutable
        }).unwrap();
        let authored_initializer_fact = program.store.scheme_body_facts.iter().position(|fact| {
            fact.function.index() == authored_owner && fact.instruction as usize == authored_initializer && matches!(fact.value, super::super::super::schemes::FullSchemeBodyFactValue::Expression(_))
        }).unwrap();

        let mut foreign_slot = program.clone();
        let payload = foreign_slot.store.data[authored_producer].range().bounds(foreign_slot.store.extra.len()).unwrap();
        assert_eq!(payload.len(), 1);
        foreign_slot.store.extra[payload.start] = u32::MAX;
        assert!(FullVerifier::verify(&foreign_slot).is_err());

        let mut mutable_binding = program.clone();
        let super::super::super::schemes::FullSchemeBodyFactValue::LocalImmutable(immutable_type) = mutable_binding.store.scheme_body_facts[authored_local_fact].value else { unreachable!() };
        mutable_binding.store.scheme_body_facts[authored_local_fact].value = super::super::super::schemes::FullSchemeBodyFactValue::LocalMutable(immutable_type);
        assert!(FullVerifier::verify(&mutable_binding).unwrap_err().message.contains("mutable storage"));

        let mut non_err_initializer = program.clone();
        non_err_initializer.store.tags[authored_initializer] = super::super::super::FullTag::ExprOk;
        assert!(FullVerifier::verify(&non_err_initializer).is_err());

        let producer_type = program.store.scheme_body_facts.iter().find_map(|fact| {
            (fact.function.index() == authored_owner && fact.instruction as usize == authored_producer).then_some(match fact.value {
                super::super::super::schemes::FullSchemeBodyFactValue::Expression(ty) => Some(ty),
                _ => None,
            }).flatten()
        }).unwrap();
        let other_type = program.store.scheme_body_facts.iter().find_map(|fact| {
            if fact.function.index() != authored_owner || fact.instruction as usize == authored_initializer { return None; }
            let super::super::super::schemes::FullSchemeBodyFactValue::Expression(ty) = fact.value else { return None; };
            matches!(authored_decoder.expression_view(fact.instruction as usize), Ok(schema::ExprView::Param(slot)) if slot == 1).then_some(ty)
        }).unwrap();
        assert_ne!(other_type, producer_type);
        let mut wrong_initializer_type = program.clone();
        wrong_initializer_type.store.scheme_body_facts[authored_initializer_fact].value = super::super::super::schemes::FullSchemeBodyFactValue::Expression(other_type);
        assert!(FullVerifier::verify(&wrong_initializer_type).is_err());

        let relay_wrappers = bounded(program.store.scheme(scheme_id).unwrap().wrappers, &program.store.scheme_wrappers, "relay_explicit wrapper range is invalid").unwrap();
        let relay_group = program.store.scheme_body_facts.iter().filter(|fact| fact.function.index() == owner).find_map(|fact| {
            let super::super::super::schemes::FullSchemeBodyFactValue::Wrapper(ordinal) = fact.value else { return None; };
            match relay_wrappers.get(ordinal as usize)? {
                super::super::super::schemes::FullWrapper::Field { detail, .. } => match program.store.wrapper_fields.get(*detail as usize)?.boundary {
                    super::super::super::schemes::FullWrapperBoundary::ProcCompletion { group, .. } => Some(group),
                    _ => None,
                },
                _ => None,
            }
        }).unwrap();
        let super::super::super::schemes::FullWrapper::ProcCompletion { detail: relay_detail, .. } = relay_wrappers[relay_group as usize] else { panic!("relay field owns its proc completion group") };
        let relay_completion = program.store.wrapper_completions[relay_detail as usize];
        let relay_contributions = relay_completion.contributions.bounds(program.store.wrapper_contributions.len()).unwrap();
        let application = bounded(program.store.scheme(scheme_id).unwrap().applications, &program.store.scheme_applications, "relay_explicit application range is invalid").unwrap().get(site.application as usize).unwrap().invocation().unwrap();
        let scopes = verify_function_schemes(&program.store, &mut effects::EffectCoverage::new(&program.store)).unwrap();
        let relay_scope = scopes[scheme_id.index()].as_ref().unwrap();
        let mut budget = SchemeInstantiationBudget::new(&program.store);
        let application_result = decode_scheme_type(&program.store, relay_scope, application.result, &mut budget, 0).unwrap();
        let application_contribution = relay_contributions.clone().find_map(|index| match program.store.wrapper_contributions[index] {
            super::super::super::schemes::FullWrapperContribution::ApplicationSource(ty) => {
                let contribution = decode_scheme_type(&program.store, relay_scope, ty, &mut budget, 0).unwrap();
                body_types_equal(&contribution, &application_result).then_some((index, ty))
            }
            _ => None,
        }).unwrap();
        let relay_field_source_type = relay_contributions.clone().find_map(|index| match program.store.wrapper_contributions[index] {
            super::super::super::schemes::FullWrapperContribution::FieldSource(ty) => Some(ty),
            _ => None,
        }).unwrap();
        let relay_field_type = decode_scheme_type(&program.store, relay_scope, relay_field_source_type, &mut budget, 0).unwrap();
        assert!(body_types_equal(&decode_scheme_type(&program.store, relay_scope, application_contribution.1, &mut budget, 0).unwrap(), &application_result));
        assert!(!body_types_equal(&application_result, &relay_field_type));

        let mut changed_contribution = program.clone();
        changed_contribution.store.wrapper_contributions[application_contribution.0] = super::super::super::schemes::FullWrapperContribution::FieldSource(application_contribution.1);
        assert!(FullVerifier::verify(&changed_contribution).is_err(), "application source role mutation was accepted");

        let mut missing_application_contribution = program.clone();
        missing_application_contribution.store.wrapper_contributions[application_contribution.0] = super::super::super::schemes::FullWrapperContribution::PropagatedError(application_contribution.1);
        assert!(FullVerifier::verify(&missing_application_contribution).is_err());

        let mut extra_contribution = program.clone();
        let insert_at = relay_completion.contributions.start as usize + relay_completion.contributions.len as usize;
        let duplicate = extra_contribution.store.wrapper_contributions[application_contribution.0];
        extra_contribution.store.wrapper_contributions.insert(insert_at, duplicate);
        for (index, completion) in extra_contribution.store.wrapper_completions.iter_mut().enumerate() {
            let start = completion.contributions.start as usize;
            if index == relay_detail as usize {
                completion.contributions.len += 1;
            } else if start >= insert_at {
                completion.contributions.start += 1;
            }
        }
        assert!(FullVerifier::verify(&extra_contribution).unwrap_err().message.contains("completion contributions differ"));

        let relay_source_type_fact = program.store.scheme_body_facts.iter().position(|fact| {
            fact.function.index() == owner && fact.instruction as usize == value.raw() as usize && matches!(fact.value, super::super::super::schemes::FullSchemeBodyFactValue::Expression(_))
        }).unwrap();
        let relay_field_source_fact = program.store.scheme_body_facts.iter().position(|fact| {
            if fact.function.index() != owner || !matches!(decoder.expression_view(fact.instruction as usize), Ok(schema::ExprView::Param(_))) { return false; }
            let super::super::super::schemes::FullSchemeBodyFactValue::Expression(ty) = fact.value else { return false; };
            body_types_equal(&decode_scheme_type(&program.store, relay_scope, ty, &mut budget, 0).unwrap(), &relay_field_type)
        }).unwrap();
        let super::super::super::schemes::FullSchemeBodyFactValue::Expression(application_source_type) = program.store.scheme_body_facts[relay_source_type_fact].value else { unreachable!() };
        let super::super::super::schemes::FullSchemeBodyFactValue::Expression(field_source_operand_type) = program.store.scheme_body_facts[relay_field_source_fact].value else { unreachable!() };
        assert!(body_types_equal(&decode_scheme_type(&program.store, relay_scope, application_source_type, &mut budget, 0).unwrap(), &application_result));
        assert!(body_types_equal(&decode_scheme_type(&program.store, relay_scope, field_source_operand_type, &mut budget, 0).unwrap(), &relay_field_type));
        assert_ne!(application_source_type, field_source_operand_type);
        let mut wrong_application_result = program.clone();
        wrong_application_result.store.scheme_body_facts[relay_source_type_fact].value = super::super::super::schemes::FullSchemeBodyFactValue::Expression(field_source_operand_type);
        wrong_application_result.store.scheme_body_facts[relay_field_source_fact].value = super::super::super::schemes::FullSchemeBodyFactValue::Expression(application_source_type);
        let wrong_application_error = FullVerifier::verify(&wrong_application_result).unwrap_err();
        assert!(!wrong_application_error.message.contains("unowned"), "coordinated application source type mutation orphaned a scheme row: {}", wrong_application_error.message);
        assert!(wrong_application_error.message.contains("declaration disagrees with its initializer"), "coordinated application source type mutation failed for an unrelated reason: {}", wrong_application_error.message);

        let mixed_decoder = super::super::decoder_for_function(&program.store, mixed_owner).unwrap();
        let local_field_fact = program.store.scheme_body_facts.iter().position(|fact| {
            fact.function.index() == mixed_owner
                && matches!(fact.value, super::super::super::schemes::FullSchemeBodyFactValue::Wrapper(_))
                && matches!(mixed_decoder.expression_view(fact.instruction as usize), Ok(schema::ExprView::Field { .. }))
        }).unwrap();
        let mut missing_local_site = program.clone();
        remove_scheme_body_fact(&mut missing_local_site, local_field_fact);
        assert_eq!(FullVerifier::verify(&missing_local_site).unwrap_err().message, "generic field projection has no required field contract");
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
    fn generic_with_bindings_authenticate_order_and_handler_isolation() {
        let program = prepare("proc keep(value, selected: Result[Int]) [] { with first = selected, second = if first > 0 { selected } else { selected } { let _ = second } else { |failure| let _ = failure }; value }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let (slots, failure) = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::With { bindings, else_param_slot: Some(failure), .. }) => Some((bindings.map(|binding| binding.unwrap().0).collect::<Vec<_>>(), failure)),
            _ => None,
        }).unwrap();
        assert_eq!(slots.len(), 2);
        for (source, target) in [(slots[0], slots[1]), (slots[1], failure), (failure, slots[0])] {
            let reads = program.store.function_instruction_range(owner).unwrap().filter(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(slot)) if slot == source)).collect::<Vec<_>>();
            assert_eq!(reads.len(), 1);
            let mut forged = program.clone();
            let payload = forged.store.data[reads[0]].range().bounds(forged.store.extra.len()).unwrap();
            assert_eq!(payload.len(), 1);
            forged.store.extra[payload.start] = target as u32;
            assert_eq!(FullVerifier::verify(&forged).unwrap_err().message, "generic body reads an uninitialized slot");
        }
    }

    #[test]
    fn line_loop_generic_arguments_require_the_actual_text_source() {
        let program = prepare(include_str!("../../../../../../tests/fixtures/runtime/generic-line-loop-arguments.xsh"));
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "collected_lines").unwrap();
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let text = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::ForStrLines { text, .. }) => Some(text.raw() as usize),
            _ => None,
        }).unwrap();
        assert!(matches!(decoder.expression_view(text).unwrap(), schema::ExprView::Param(0)));
        let mut wrong_source = program.clone();
        wrong_source.store.in_place.take();
        let words = wrong_source.store.data[text].range().bounds(wrong_source.store.extra.len()).unwrap();
        assert_eq!(words.len(), 1);
        wrong_source.store.extra[words.start] = 1;
        assert!(FullVerifier::verify(&wrong_source).is_err());
    }

    #[test]
    fn optional_guard_refinement_requires_the_actual_immutable_source() {
        let program = prepare("pure keep(marker, source: Int?, other: Int?) -> Int? { let _ = marker; let _ = other; guard let number = source else { return null }; source }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let (guard, source) = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Guard { value, .. }) => Some((instruction, value.raw() as usize)),
            _ => None,
        }).unwrap();
        let schema::ExprView::ValueBlock { mut body, .. } = decoder.expression_view(source).unwrap() else { unreachable!() };
        let source = body.find_map(|statement| match decoder.statement_view(statement.unwrap().raw() as usize).unwrap() {
            schema::StmtView::Let { value, .. } => Some(value.raw() as usize),
            _ => None,
        }).unwrap();
        assert!(matches!(decoder.expression_view(source).unwrap(), schema::ExprView::Param(1)));
        let mut wrong_source = program.clone();
        wrong_source.store.in_place.take();
        let words = wrong_source.store.data[source].range().bounds(wrong_source.store.extra.len()).unwrap();
        assert_eq!(words.len(), 1);
        wrong_source.store.extra[words.start] = 2;
        assert_eq!(FullVerifier::verify(&wrong_source).unwrap_err().message, "generic body expression disagrees with its checked type");
        let discarded = range.clone().find(|&instruction| matches!(decoder.statement_view(instruction), Ok(schema::StmtView::Value { .. }))).unwrap();
        let mut missing_guard = program.clone();
        missing_guard.store.in_place.take();
        missing_guard.store.tags[guard] = program.store.tags[discarded];
        missing_guard.store.data[guard] = program.store.data[discarded];
        assert!(FullVerifier::verify(&missing_guard).is_err());
    }

    fn verify_mutable_optional_guard(annotation: &str) {
        let source = format!("pure keep(marker{annotation}, value: Int?, minimum: Int) -> Int? {{ let _ = marker; var parsed = value; guard let number = parsed else {{ return null }}; return null when number < minimum; parsed }}\ntest verify_mutable_guard {{ assert keep(7, 4, 0) == 4; assert keep(7, 4, 9) == null; assert keep(7, null, 0) == null }}\n");
        verify_optional_guard_test_source(&source, "verify_mutable_guard");
    }

    fn verify_optional_guard_test_source(source: &str, selector: &str) {
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("mutable-optional-guard.xsh", crate::loader::entry_source_from_text("mutable-optional-guard.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = crate::sema::check::Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
        let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test(selector, crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty());
            assert!(result.output.traceback.is_none());
            assert!(matches!(result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(*value, crate::runtime::value::Value::Unit)));
        }
    }

    #[test]
    fn typed_mutable_optional_guard_retains_its_original_slot() {
        verify_mutable_optional_guard(": Int");
    }

    #[test]
    fn generic_mutable_optional_guard_retains_its_original_slot() {
        verify_mutable_optional_guard("");
    }

    fn verify_closed_wrapper_fallbacks(annotation: &str) {
        let source = format!(r#"type FallbackRow = {{value: Int}}
pure fallback_row() -> FallbackRow {{ {{value: 7}} }}
pure optional_row(marker{annotation}, present: Bool) -> FallbackRow {{
  let _ = marker
  var counted = null
  if present {{ counted = {{value: 9}} }}
  counted ?? fallback_row()
}}
pure result_row(marker{annotation}, value: Result[FallbackRow, Str]) -> FallbackRow {{ let _ = marker; value ?? fallback_row() }}
pure successful_row() -> Result[FallbackRow, Str] {{ Ok({{value: 9}}) }}
pure failed_row() -> Result[FallbackRow, Str] {{ Err("failure") }}
pure optional_result(marker{annotation}, value: Result[Int, Str]?) -> Result[Int, Str] {{ let _ = marker; value ?? Ok(7) }}
pure nested_result(marker{annotation}, value: Result[Result[Int, Str], Str]) -> Result[Int, Str] {{ let _ = marker; value ?? Ok(7) }}
pure failed_number() -> Result[Int, Str] {{ Err("inner") }}
pure successful_number() -> Result[Int, Str] {{ Ok(9) }}
pure backup_number() -> Result[Int, Str] {{ Ok(7) }}
pure nested_number() -> Result[Result[Int, Str], Str] {{ Ok(failed_number()) }}
pure lazy_optional(marker{annotation}, value: Int?) -> Int {{ let _ = marker; value ?? 1 / 0 }}
pure lazy_result(marker{annotation}, value: Result[Int, Str]) -> Int {{ let _ = marker; value ?? 1 / 0 }}
test verify_closed_fallbacks {{
  assert optional_row(7, true).value == 9
  assert optional_row(7, false).value == 7
  assert result_row(7, successful_row()).value == 9
  assert result_row(7, failed_row()).value == 7
  assert optional_result(7, failed_number()) == failed_number()
  assert optional_result(7, null) == backup_number()
  assert nested_result(7, nested_number()) == failed_number()
  assert lazy_optional(7, 9) == 9
  assert lazy_result(7, successful_number()) == 9
}}
"#);
        verify_optional_guard_test_source(&source, "verify_closed_fallbacks");
    }

    #[test]
    fn typed_closed_wrapper_fallbacks_preserve_actual_payloads() {
        verify_closed_wrapper_fallbacks(": Int");
    }

    #[test]
    fn generic_closed_wrapper_fallbacks_preserve_actual_payloads() {
        verify_closed_wrapper_fallbacks("");
    }

    fn verify_proc_match_completion(annotation: &str, result_annotation: &str) {
        let source = format!(r#"error MatchFailure = Bad
pure source_result(flag: Bool) -> Result[Int] {{ if flag {{ Ok(9) }} else {{ Err(MatchFailure.Bad()) }} }}
pure expected_bool(value: Bool) -> Result[Bool] {{ Ok(value) }}
proc tail_match(marker{annotation}, flag: Bool) [] {result_annotation} {{
  let _ = marker
  match source_result(flag) {{
    Ok(_) => Ok(true)
    Err(failure) => if !flag {{ Ok(false) }} else {{ Err(failure) }}
  }}
}}
test verify_match_completion {{
  assert tail_match(7, true) == expected_bool(true)
  assert tail_match(7, false) == expected_bool(false)
}}
"#);
        verify_optional_guard_test_source(&source, "verify_match_completion");
    }

    #[test]
    fn typed_proc_tail_match_preserves_value_completion() {
        verify_proc_match_completion(": Int", "-> Result[Bool]");
    }

    #[test]
    fn inferred_proc_tail_match_preserves_value_completion() {
        verify_proc_match_completion(": Int", "");
    }

    #[test]
    fn generic_proc_tail_match_preserves_value_completion() {
        verify_proc_match_completion("", "");
    }

    #[test]
    fn inferred_proc_control_completions_preserve_early_returns_and_unit() {
        verify_optional_guard_test_source(r#"error CompletionFailure = Bad
pure failed_number() -> Result[Int] { Err(CompletionFailure.Bad()) }
proc branches(mode: Int) {
  if mode == 0 { return Ok(0) }
  for item in [1] { if mode == 1 { return Ok(item) } }
  match failed_number() {
    Ok(value) => Ok(value)
    Err(failure) => if mode == 2 { Ok(2) } else { Err(failure) }
  }
}
proc statement_only(flag: Bool) { if flag { let _ = 7 } else { let _ = 8 } }
proc partial_statement(flag: Bool) { if flag { let _ = 7 } }
test verify_completion_control {
  assert branches(0) == Ok(0)
  assert branches(1) == Ok(1)
  assert branches(2) == Ok(2)
  assert branches(3) == failed_number()
  assert statement_only(true) == Ok()
  assert statement_only(false) == Ok()
  assert partial_statement(true) == Ok()
  assert partial_statement(false) == Ok()
}
"#, "verify_completion_control");
    }

    #[test]
    fn authored_nonunit_proc_still_requires_a_return_value() {
        for source in [
            "proc missing(flag: Bool) [] -> Result[Int] { if flag { return Ok(7) } }",
            "proc missing(flag: Bool) [] -> Result[Int] { if flag { let _ = 7 } else { let _ = 8 } }",
        ] {
            let parsed = Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(!checked.diagnostics.is_empty(), "{source}");
        }
    }

    #[test]
    fn inferred_proc_branch_completions_run_cleanup_once() {
        verify_optional_guard_test_source(r#"var cleaned = 0
proc complete(flag: Bool) {
  defer { cleaned += 1 }
  if flag { Ok(7) } else { Ok(9) }
}
test verify_completion_cleanup {
  assert complete(true) == Ok(7)
  assert cleaned == 1
  assert complete(false) == Ok(9)
  assert cleaned == 2
}
"#, "verify_completion_cleanup");
    }

    #[test]
    fn inferred_proc_completions_require_physical_arm_returns() {
        let program = prepare("proc complete(flag: Bool) { if flag { Ok(7) } else { Ok(9) } }");
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "complete").unwrap();
        let returns = program.store.function_instruction_range(owner).unwrap().filter(|&instruction| program.store.tags[instruction] == FullTag::StmtReturn).collect::<Vec<_>>();
        assert_eq!(returns.len(), 2);
        for returned in returns {
            let mut changed = program.clone();
            changed.store.in_place.take();
            assert_eq!(changed.store.data[returned].range().bounds(changed.store.extra.len()).unwrap().len(), 1);
            changed.store.tags[returned] = FullTag::StmtValue;
            assert!(FullVerifier::verify(&changed).is_err());
        }
    }

    #[test]
    fn closed_result_fallbacks_require_actual_source_payload_and_owner() {
        let program = prepare("pure keep(marker, value: Result[Int, Str], backup: Int, wrong: Str) -> Int { let _ = marker; value ?? backup }\npure other(marker) { marker }");
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "keep").unwrap();
        let other = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "other").unwrap();
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        for slot in [1, 2] {
            let instruction = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(actual)) if actual == slot)).unwrap();
            let words = program.store.data[instruction].range().bounds(program.store.extra.len()).unwrap();
            let mut changed = program.clone();
            changed.store.in_place.take();
            changed.store.extra[words.start] = 3;
            assert!(FullVerifier::verify(&changed).is_err(), "slot={slot}");
        }
        let instruction = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::ResultFallback { .. }))).unwrap();
        let fact = program.store.scheme_body_facts.iter().position(|fact| fact.instruction as usize == instruction && fact.function.index() == owner && fact.kind() == FullSchemeBodyFactKind::Expression).unwrap();
        let mut changed = program.clone();
        changed.store.scheme_body_facts[fact].function = IrFunctionId::new(other).unwrap();
        assert!(FullVerifier::verify(&changed).is_err());
    }

    #[test]
    fn closed_optional_fallbacks_require_the_actual_present_binding() {
        let program = prepare("pure keep(marker, value: Int?, wrong: Str) -> Int { let _ = marker; value ?? 7 }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let instruction = program.store.function_instruction_range(owner).unwrap().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::MatchExpr { .. }))).unwrap();
        let schema::ExprView::MatchExpr { mut arms, .. } = decoder.expression_view(instruction).unwrap() else { unreachable!() };
        let _ = arms.next().unwrap().unwrap();
        let (_, _, present) = arms.next().unwrap().unwrap();
        assert!(matches!(decoder.expression_view(present.raw() as usize).unwrap(), schema::ExprView::Param(_)));
        let words = program.store.data[present.raw() as usize].range().bounds(program.store.extra.len()).unwrap();
        let mut changed = program.clone();
        changed.store.in_place.take();
        changed.store.extra[words.start] = 2;
        assert!(FullVerifier::verify(&changed).is_err());
    }

    #[test]
    fn mutable_optional_guard_native_fixture_retains_its_original_slots() {
        let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/generic-guard-original-slot.xsh")).unwrap();
        verify_optional_guard_test_source(&source, "verify_guard_original_slot");
    }

    #[test]
    fn mutable_optional_guard_rejects_later_direct_and_callback_writes() {
        for written in ["source = null;", "[1] |> each { source = null }; "] {
            let program = prepare(&format!("proc keep(marker, value: Int?) [] -> Int? {{ let _ = marker; var source = value; guard let number = source else {{ return null }}; {written} number }}"));
            let (owner, _) = generic_owner(&program);
            let decoder = decoder_for_function(&program.store, owner).unwrap();
            let range = program.store.function_instruction_range(owner).unwrap();
            let source = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
                Ok(schema::StmtView::Assign { slot, .. }) => Some(slot),
                _ => None,
            }).unwrap();
            let tail = range.filter_map(|instruction| match decoder.statement_view(instruction) {
                Ok(schema::StmtView::Return { value }) => Some(value.raw() as usize),
                _ => None,
            }).last().unwrap();
            assert!(matches!(decoder.expression_view(tail).unwrap(), schema::ExprView::Param(slot) if slot != source));
            let mut stale = program.clone();
            stale.store.in_place.take();
            let words = stale.store.data[tail].range().bounds(stale.store.extra.len()).unwrap();
            stale.store.extra[words.start] = source as u32;
            assert!(FullVerifier::verify(&stale).is_err(), "{written}");
        }
    }

    #[test]
    fn mutable_optional_guard_cannot_refine_a_foreign_lexical_source() {
        let program = prepare("pure keep(marker, value: Int?, other: Int?, flag: Bool) -> Int? { let _ = marker; var source = value; if flag { var foreign = other; guard let selected = foreign else { return null }; let _ = selected }; guard let selected = source else { return null }; source }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let mut guards = Vec::new();
        for instruction in program.store.function_instruction_range(owner).unwrap() {
            let Ok(schema::StmtView::Guard { value, .. }) = decoder.statement_view(instruction) else { continue };
            let schema::ExprView::ValueBlock { mut body, .. } = decoder.expression_view(value.raw() as usize).unwrap() else { panic!("guard source is evaluated in its lexical block") };
            let read = body.find_map(|statement| match decoder.statement_view(statement.unwrap().raw() as usize).unwrap() {
                schema::StmtView::Let { value, .. } => Some(value.raw() as usize),
                _ => None,
            }).unwrap();
            let schema::ExprView::Param(slot) = decoder.expression_view(read).unwrap() else { panic!("guard retains its actual source read") };
            guards.push((read, slot));
        }
        assert_eq!(guards.len(), 2);
        let mut escaped = program.clone();
        escaped.store.in_place.take();
        let words = escaped.store.data[guards[1].0].range().bounds(escaped.store.extra.len()).unwrap();
        escaped.store.extra[words.start] = guards[0].1 as u32;
        assert!(FullVerifier::verify(&escaped).is_err());
    }

    #[test]
    fn optional_guard_captures_do_not_refine_a_written_source() {
        let program = prepare("pure keep(marker, value: Int?) -> Int? { let _ = marker; var source = value; guard let number = source else { return null }; source = null; number }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let source = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Assign { slot, .. }) => Some(slot),
            _ => None,
        }).unwrap();
        let tail = range.filter_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Return { value }) => Some(value.raw() as usize),
            _ => None,
        }).last().unwrap();
        assert!(matches!(decoder.expression_view(tail).unwrap(), schema::ExprView::Param(slot) if slot != source));
        let mut written_source = program.clone();
        written_source.store.in_place.take();
        let words = written_source.store.data[tail].range().bounds(written_source.store.extra.len()).unwrap();
        assert_eq!(words.len(), 1);
        written_source.store.extra[words.start] = source as u32;
        assert_eq!(FullVerifier::verify(&written_source).unwrap_err().message, "generic body expression disagrees with its checked type");
    }

    #[test]
    fn generic_guards_authenticate_handler_isolation_and_divergence() {
        let program = prepare("proc keep(value, selected: Result[Int]) [] { guard let item = selected else { |failure| let _ = failure; return value }; let _ = item; value }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let (item, failure, handler) = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Guard { target, else_body, else_param_slot: Some(failure), .. }) => {
                let mut budget = SchemeInstantiationBudget::new(&program.store);
                let bindings = target_bindings(target, &SchemeType::Ground(Type::Int), &mut budget).unwrap();
                assert_eq!(bindings.len(), 1);
                Some((*bindings.keys().next().unwrap(), failure, else_body.map(|statement| statement.unwrap().raw() as usize).collect::<Vec<_>>()))
            }
            _ => None,
        }).unwrap();
        for (source, target) in [(item, failure), (failure, item)] {
            let reads = program.store.function_instruction_range(owner).unwrap().filter(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(slot)) if slot == source)).collect::<Vec<_>>();
            assert_eq!(reads.len(), 1);
            let mut forged = program.clone();
            let payload = forged.store.data[reads[0]].range().bounds(forged.store.extra.len()).unwrap();
            assert_eq!(payload.len(), 1);
            forged.store.extra[payload.start] = target as u32;
            assert_eq!(FullVerifier::verify(&forged).unwrap_err().message, "generic body reads an uninitialized slot");
        }
        let returned = handler.into_iter().find(|&instruction| program.store.tags[instruction] == FullTag::StmtReturn).unwrap();
        let mut forged = program.clone();
        assert_eq!(forged.store.data[returned].range().bounds(forged.store.extra.len()).unwrap().len(), 1);
        forged.store.tags[returned] = FullTag::StmtValue;
        assert_eq!(FullVerifier::verify(&forged).unwrap_err().message, "generic guard failure branch can complete");
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
        let mut conflicting = program.store.scheme_body_facts[declaration];
        conflicting.value = super::super::super::schemes::FullSchemeBodyFactValue::LocalImmutable(conflicting.ty().unwrap());
        append_scheme_body_fact(&mut duplicated, scheme, conflicting);
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
                Ok(schema::ExprView::Index { index, .. }) => index.ordinary().map(|index| index.raw() as usize),
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
            assert!(
                matches!(
                    (operation, message.as_str()),
                    ("[value][index]", "generic index operands have no collection contract" | "generic index site disagrees with its sealed operation")
                        | ("[value][index..]", "generic slice bound is not an integer" | "generic slice bound has no Int producer")
                ),
                "{operation}: {message}"
            );
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
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic index site disagrees with its sealed operation");
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
    fn formatted_native_consumers_require_owned_body_and_display_proofs() {
        use super::super::super::schemes::FullSchemeBodyFactValue;
        let program = prepare(r#"pure foreign(value: Bytes) -> Bytes { value }
proc selected(value, wrong: Bytes) [fs, error] -> Result[Str] {
    let spare: Any = wrong
    let text = f"{value}"
    let file_path = fp"{value}"
    file_path.read_text()
}
"#);
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let (formatted, operand) = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::PathFmtString { mut parts, .. }) => match parts.next().unwrap().unwrap() {
                super::super::super::decode::FmtPartView::Expr(value, _, _) => Some((instruction, value.raw() as usize)),
                _ => None,
            },
            _ => None,
        }).unwrap();
        let expression_fact = |instruction| program.store.scheme_body_facts.iter().position(|fact| fact.instruction as usize == instruction && matches!(fact.value, FullSchemeBodyFactValue::Expression(_))).unwrap();
        let output = expression_fact(formatted);
        let child = expression_fact(operand);
        let text = range.clone().find(|instruction| program.store.tags[*instruction] == FullTag::ExprFmtString).unwrap();
        let text_fact = expression_fact(text);
        let bytes_read = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(1)))).unwrap();
        let bytes_fact = expression_fact(bytes_read);
        let mut parameters = program.store.schemes[scheme.index()].type_parameters.bounds(program.store.scheme_type_parameters.len()).unwrap();
        let conversion = parameters.find(|parameter| program.store.scheme_type_parameters[*parameter].conversions != 0).unwrap();
        let foreign = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "foreign").unwrap();
        let foreign_decoder = decoder_for_function(&program.store, foreign).unwrap();
        let foreign_source = program.store.function_instruction_range(foreign).unwrap().find(|instruction| matches!(foreign_decoder.expression_view(*instruction), Ok(schema::ExprView::Param(0)))).unwrap();
        let cold = || {
            let mut changed = program.clone();
            changed.store.in_place.take();
            assert!(changed.store.in_place.get().is_none());
            changed
        };
        FullVerifier::verify(&cold()).unwrap();
        for (label, first, second) in [("output", output, text_fact), ("child type", child, bytes_fact)] {
            let mut changed = cold();
            let first_value = changed.store.scheme_body_facts[first].value;
            changed.store.scheme_body_facts[first].value = changed.store.scheme_body_facts[second].value;
            changed.store.scheme_body_facts[second].value = first_value;
            super::super::prepare_tables(&changed.store).unwrap();
            let error = FullVerifier::verify(&changed).unwrap_err();
            assert!(error.message.contains("disagrees with") || error.message.contains("cannot be displayed"), "{label}: {}", error.message);
        }
        let mut missing_child_role = cold();
        missing_child_role.store.scheme_body_facts[child].value = FullSchemeBodyFactValue::Tail(program.store.scheme_body_facts[child].ty().unwrap());
        assert_eq!(FullVerifier::verify(&missing_child_role).unwrap_err().message, "generic body fact has an invalid instruction kind");
        let mut unbounded = cold();
        assert_eq!(unbounded.store.scheme_type_parameters[conversion].conversions, 1);
        unbounded.store.scheme_type_parameters[conversion].conversions = 0;
        assert_eq!(FullVerifier::verify(&unbounded).unwrap_err().message, "generic interpolation producer cannot be displayed");
        let mut wrong_owner = cold();
        wrong_owner.store.scheme_body_facts[child].function = IrFunctionId::new(foreign).unwrap();
        let error = FullVerifier::verify(&wrong_owner).unwrap_err();
        assert_eq!(error.message, "generic body fact has invalid ownership");
        let mut foreign_read = cold();
        let words = foreign_read.store.data[formatted].range().bounds(foreign_read.store.extra.len()).unwrap();
        let parts = IrBlockId::from_raw(foreign_read.store.extra[words.start]).unwrap();
        let words = foreign_read.store.blocks[parts.index()].instructions.bounds(foreign_read.store.extra.len()).unwrap();
        let references = words.filter(|word| foreign_read.store.extra[*word] == operand as u32).collect::<Vec<_>>();
        assert_eq!(references.len(), 1);
        foreign_read.store.extra[references[0]] = foreign_source as u32;
        assert_eq!(FullVerifier::verify(&foreign_read).unwrap_err().message, "full IR instruction belongs to another function");
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
                Ok(schema::ExprView::CheckedValue { check: ValueCheckView::Ground(check), .. }) => Some((instruction, check.ty.id)),
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
        target.store.extra[payload.start + 2] = expected.raw();
        target.store.extra[payload.start + 3] = program.store.extra[other_payload.start + 3];
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
    fn generic_membership_key_checks_authenticate_physical_sites_and_owners() {
        let program = prepare("pure has(item, container, other, other_container) { let first = item in container; let second = other not in other_container; [first, second] }\npure unrelated(value) { value }\nlet unsigned: Map[UInt, Int] = {[1]: 7}\nlet selected = has(1, unsigned, \"word\", [\"word\"])");
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "has").unwrap();
        let other_owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "unrelated").unwrap();
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let sites = program.store.function_instruction_range(owner).unwrap().filter_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::Binary { left, right, .. }) => {
                let schema::ExprView::CheckedValue { value, check: ValueCheckView::MembershipKey(ordinal), .. } = decoder.expression_view(left.raw() as usize).unwrap() else { panic!("membership owns its physical key check") };
                Some((instruction, left.raw() as usize, value.raw() as usize, right.raw() as usize, ordinal))
            }
            _ => None,
        }).collect::<Vec<_>>();
        assert_eq!(sites.len(), 2);
        let (binary, check, value, _, ordinal) = sites[0];
        assert_ne!(ordinal, sites[1].4);
        let binary_payload = program.store.data[binary].range().bounds(program.store.extra.len()).unwrap();
        let check_payload = program.store.data[check].range().bounds(program.store.extra.len()).unwrap();
        assert_eq!(program.store.extra[binary_payload.start + 1], check as u32);
        assert_eq!(program.store.extra[check_payload.start], value as u32);
        assert_eq!(program.store.extra[check_payload.start + 1], 3);
        assert_eq!(program.store.extra[check_payload.start + 2], ordinal);
        let cold = || { let mut changed = program.clone(); changed.store.in_place.take(); changed };
        let mut missing = cold();
        let int = (0..program.store.semantic.type_count()).map(|index| super::super::super::TypeId::new(index).unwrap())
            .find(|id| program.store.semantic.to_type(*id).unwrap() == Type::Int).unwrap();
        let mut payload = vec![value as u32, 0, int.raw(), program.store.functions[owner].name, 0];
        payload.extend_from_slice(&program.store.extra[check_payload.start + 3..check_payload.end]);
        let start = missing.store.extra.len() as u32;
        missing.store.extra.extend_from_slice(&payload);
        missing.store.data[check] = crate::runtime::eval::indexed::IrData::new(start, payload.len() as u32);
        assert!(matches!(decoder_for_function(&missing.store, owner).unwrap().expression_view(check).unwrap(), schema::ExprView::CheckedValue { check: ValueCheckView::Ground(_), .. }));
        let error = FullVerifier::verify(&missing).expect_err("a membership cannot omit its operation check");
        assert!(error.message.contains("selected key check"), "{error:?}");
        let mut wrong_guard = cold();
        let binding = wrong_guard.store.call_operation_bindings.iter_mut().find(|binding| matches!(binding, crate::sema::schemes::SealedOperationBinding::Selected(crate::sema::sealed_operation::SealedOperationSignature::Membership(crate::sema::sealed_operation::MembershipDomain::UnsignedMap)))).expect("the actual UInt map call selects its unsigned domain");
        *binding = crate::sema::schemes::SealedOperationBinding::Selected(crate::sema::sealed_operation::SealedOperationSignature::Membership(crate::sema::sealed_operation::MembershipDomain::Map));
        assert!(FullVerifier::verify(&wrong_guard).is_err());
        let mut wrong_ordinal = cold();
        wrong_ordinal.store.extra[check_payload.start + 2] = sites[1].4;
        assert!(FullVerifier::verify(&wrong_ordinal).unwrap_err().message.contains("differs from its operation"));
        let mut wrong_op = cold();
        wrong_op.store.extra[binary_payload.start] = crate::syntax::node::BinaryOp::Eq as u32;
        assert!(FullVerifier::verify(&wrong_op).is_err());
        let mut wrong_owner = cold();
        let fact = wrong_owner.store.scheme_body_facts.iter_mut().find(|fact| fact.instruction as usize == binary && matches!(fact.value, super::super::super::schemes::FullSchemeBodyFactValue::Operation(_))).unwrap();
        fact.function = super::super::super::IrFunctionId::new(other_owner).unwrap();
        assert!(FullVerifier::verify(&wrong_owner).unwrap_err().message.contains("ownership"));
        let mut wrong_needle = cold();
        let schema::ExprView::Param(original) = decoder.expression_view(value).unwrap() else { panic!("the key retains its actual ANF binding") };
        assert_ne!(original, 2);
        let payload = wrong_needle.store.data[value].range().bounds(wrong_needle.store.extra.len()).unwrap();
        assert_eq!(payload.len(), 1);
        wrong_needle.store.extra[payload.start] = 2;
        assert!(FullVerifier::verify(&wrong_needle).is_err());
        let mut moved = cold();
        let second = moved.store.data[sites[1].0].range().bounds(moved.store.extra.len()).unwrap();
        moved.store.extra[binary_payload.start + 1] = sites[1].1 as u32;
        moved.store.extra[second.start + 1] = check as u32;
        assert!(FullVerifier::verify(&moved).is_err());
        let mut duplicate = cold();
        let second = duplicate.store.data[sites[1].0].range().bounds(duplicate.store.extra.len()).unwrap();
        duplicate.store.extra[second.start + 1] = check as u32;
        let fact = duplicate.store.scheme_body_facts.iter_mut().find(|fact| fact.instruction as usize == sites[1].0 && matches!(fact.value, super::super::super::schemes::FullSchemeBodyFactValue::Operation(_))).unwrap();
        fact.value = super::super::super::schemes::FullSchemeBodyFactValue::Operation(ordinal);
        assert!(FullVerifier::verify(&duplicate).is_err());
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
    fn wrapper_handlers_authenticate_the_actual_success_projection() {
        let program = prepare("pure handled(value, backup) { value ?? { |error| backup } }");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let instruction = program.store.function_instruction_range(owner).unwrap().find(|instruction| program.store.tags[*instruction] == FullTag::ExprMatch).unwrap();
        let schema::ExprView::MatchExpr { mut arms, .. } = decoder.expression_view(instruction).unwrap() else { unreachable!() };
        let (_, _, success) = arms.next().unwrap().unwrap();
        let (_, _, handler) = arms.next().unwrap().unwrap();
        let schema::ExprView::ValueBlock { mut body, .. } = decoder.expression_view(handler.raw() as usize).unwrap() else { unreachable!() };
        let statement = body.next().unwrap().unwrap();
        let schema::StmtView::Value { value } = decoder.statement_view(statement.raw() as usize).unwrap() else { unreachable!() };
        let schema::ExprView::Param(backup) = decoder.expression_view(value.raw() as usize).unwrap() else { unreachable!() };
        let mut changed = program.clone();
        let payload = changed.store.data[success.raw() as usize].range().bounds(changed.store.extra.len()).unwrap();
        assert_eq!(payload.len(), 1);
        changed.store.extra[payload.start] = backup as u32;
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "wrapper handler changes its success payload");
    }

    #[test]
    fn wrapper_capture_preserves_nested_argument_and_call_error_order() {
        let program = prepare("type TextRow = {name: Str}\nerror InnerFailure = Bad\nproc outer(value: Int) [error] -> Int { value }\nproc grounded(value: TextRow?, inner: Result[Int, InnerFailure]) [] -> Result[Str?] { try { let _ = outer(inner?); value?.name } }\nproc captured(value, inner: Result[Int, InnerFailure]) [] -> Result[Str?] { try { let _ = outer(inner?); value?.name } }");
        let capture = program.store.wrapper_captures.iter().find(|capture| capture.contributions.len == 3).unwrap();
        let contributions = capture.contributions.bounds(program.store.wrapper_contributions.len()).unwrap();
        let mut changed = program.clone();
        changed.store.wrapper_contributions.swap(contributions.start, contributions.start + 1);
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "capture contributions disagree with its actual body");
    }

    #[test]
    fn wrapper_capture_authenticates_explicit_and_conditional_contribution_order() {
        let program = prepare("type TextRow = {name: Str}\npure captured(value) { try { let _ = Ok(0)?; value?.name } }\npure from_optional(value: TextRow?) -> Result[Str?] { captured(value) }\npure from_result(value: Result[TextRow, Str]) -> Result[Str] { captured(value) }");
        let capture = program.store.wrapper_captures.iter().find(|capture| capture.contributions.len == 2).unwrap();
        let contributions = capture.contributions.bounds(program.store.wrapper_contributions.len()).unwrap();
        let mut changed = program.clone();
        changed.store.wrapper_contributions.swap(contributions.start, contributions.start + 1);
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "capture contributions disagree with its actual body");
    }

    #[test]
    fn wrapper_capture_authenticates_actual_conditional_error_sources() {
        for calls in ["\npure result(value: Result[TextRow, Str]) -> Result[Str, Str] { captured(value, null) }\npure optional(value: TextRow?) -> Result[Str?] { captured(value, null) }", ""] {
        let program = prepare(&format!("type TextRow = {{name: Str}}\npure captured(value, spare: TextRow?) {{ try {{ value?.name }} }}{calls}"));
        let owner = program.store.functions.iter().position(|function| match function.contract {
            FullCallableContract::Scheme(id) => bounded(program.store.scheme(id).unwrap().wrappers, &program.store.scheme_wrappers, "wrappers").unwrap().iter().any(|wrapper| matches!(wrapper, super::super::super::schemes::FullWrapper::Capture { .. })),
            _ => false,
        }).unwrap();
        let FullCallableContract::Scheme(id) = program.store.functions[owner].contract else { unreachable!() };
        let scheme = program.store.scheme(id).unwrap();
        let spare = bounded(scheme.params, &program.store.scheme_params, "parameters").unwrap()[1].ty;
        let mut changed = program.clone();
        for wrapper in bounded(scheme.wrappers, &program.store.scheme_wrappers, "wrappers").unwrap() {
            if let super::super::super::schemes::FullWrapper::Capture { detail, .. } = wrapper {
                let capture = &mut changed.store.wrapper_captures[*detail as usize];
                capture.expected = true;
                let contributions = capture.contributions.bounds(changed.store.wrapper_contributions.len()).unwrap();
                let super::super::super::schemes::FullWrapperContribution::Source(source) = changed.store.wrapper_contributions[contributions.start] else { unreachable!() };
                assert_eq!(changed.store.scheme_types[source.index()].tag, FullSchemeTypeTag::Parameter);
                assert_eq!(changed.store.scheme_types[spare.index()].tag, FullSchemeTypeTag::Optional);
                changed.store.scheme_types[source.index()] = changed.store.scheme_types[spare.index()];
            }
        }
        assert!(FullVerifier::verify(&changed).is_err(), "capture error sources must come from its actual body, including uncalled definitions");
        }
    }

    #[test]
    fn wrapper_fields_authenticate_independent_optional_payloads() {
        let program = prepare("type TextRow = {name: Str}\ntype IntRow = {name: Int}\npure name(value) { value?.name }\npure text(value: TextRow?) -> Str? { name(value) }\npure number(value: IntRow?) -> Int? { name(value) }");
        let ordinal = program.store.scheme_wrappers.iter().position(|wrapper| matches!(wrapper, super::super::super::schemes::FullWrapper::Field { .. })).unwrap();
        let mut changed = program.clone();
        let super::super::super::schemes::FullWrapper::Field { source, result, .. } = &mut changed.store.scheme_wrappers[ordinal] else { unreachable!() };
        std::mem::swap(source, result);
        assert!(FullVerifier::verify(&changed).is_err());
    }

    #[test]
    fn wrapper_fallback_sites_authenticate_optional_and_result_modes_through_relays() {
        let program = prepare("pure fallback(value, backup) { value ?? backup }\npure relay(value, backup) { fallback(value, backup) }\npure keep(value: Result[Int, Str]?) -> Result[Int, Str] { relay(value, Ok(0)) }\npure recover(value: Result[Int, Str]) -> Int { relay(value, 0) }");
        let selected = program.store.call_wrapper_bindings.iter().position(|binding| matches!(binding, crate::sema::wrapper_operation::WrapperBinding::Selected(crate::sema::wrapper_operation::WrapperSelection::Optional))).unwrap();
        let mut changed = program.clone();
        changed.store.call_wrapper_bindings[selected] = crate::sema::wrapper_operation::WrapperBinding::Selected(crate::sema::wrapper_operation::WrapperSelection::Result);
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "selected wrapper mode disagrees with its source envelope");
    }

    #[test]
    fn sealed_binary_operation_sites_authenticate_opcode_inputs_and_result() {
        let program = prepare("pure combine(a, b, wrong) { a + b }\npure numbers() -> Int { combine(1, 2, false) }");
        let (owner, scheme_id) = generic_owner(&program);
        let scheme = program.store.scheme(scheme_id).unwrap();
        let operations = scheme.operations.bounds(program.store.scheme_operations.len()).unwrap();
        assert_eq!(operations.len(), 1);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let binary = program.store.function_instruction_range(owner).unwrap().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Binary { op: crate::syntax::node::BinaryOp::Add, .. }))).unwrap();
        let mut opcode = program.clone();
        let words = opcode.store.data[binary].range().bounds(opcode.store.extra.len()).unwrap();
        let selected = opcode.store.extra[words.start] as usize;
        opcode.store.binary_ops[selected] = crate::syntax::node::BinaryOp::Sub;
        assert_eq!(FullVerifier::verify(&opcode).unwrap_err().message, "generic binary site disagrees with its sealed operation");
        let mut input = program.clone();
        input.store.extra.swap(words.start + 1, words.start + 2);
        assert_eq!(FullVerifier::verify(&input).unwrap_err().message, "generic binary site disagrees with its sealed operation");
        let params = bounded(scheme.params, &program.store.scheme_params, "parameters").unwrap();
        let mut result = program.clone();
        let super::super::super::schemes::FullSealedOperation::Binary { result: required, .. } = result.store.scheme_operations[operations.start] else { unreachable!() };
        assert_eq!(result.store.scheme_types[required.index()].tag, FullSchemeTypeTag::Parameter);
        result.store.scheme_types[required.index()] = result.store.scheme_types[params[2].ty.index()];
        assert_eq!(FullVerifier::verify(&result).unwrap_err().message, "generic binary site disagrees with its sealed operation");
    }

    #[test]
    fn sealed_binary_operation_bindings_authenticate_selected_and_forwarded_signatures() {
        use crate::sema::schemes::SealedOperationBinding;
        let program = prepare("pure combine(a, b) { a + b }\npure subtract(a, b) { a - b }\npure relay(a, b) { let sum = combine(a, b); let difference = subtract(a, b); sum }\npure numbers() -> Int { relay(4, 2) }");
        let mut selected = program.clone();
        let selection = selected.store.call_operation_bindings.iter_mut().find(|binding| matches!(binding, SealedOperationBinding::Selected(_))).unwrap();
        *selection = SealedOperationBinding::Selected(crate::sema::sealed_operation::SealedOperationSignature::Binary(crate::sema::sealed_operation::ArithmeticSignature::Float));
        assert_eq!(FullVerifier::verify(&selected).unwrap_err().message, "selected operation differs from its canonical signature");
        let mut forwarded = program.clone();
        let forwarding = forwarded.store.call_operation_bindings.iter_mut().find_map(|binding| match binding { SealedOperationBinding::Forwarded(ordinal) => Some(ordinal), _ => None }).unwrap();
        assert!(*forwarding < 2);
        *forwarding = 1 - *forwarding;
        assert_eq!(FullVerifier::verify(&forwarded).unwrap_err().message, "forwarded operation differs from its actual caller requirement");
    }

    #[test]
    fn sealed_binary_operation_owners_reject_forged_nonempty_effects() {
        let program = prepare("pure combine(a, b) { a + b }\npure numbers() -> Int { combine(1, 2) }");
        let (_, scheme_id) = generic_owner(&program);
        let mut changed = program.clone();
        let start = changed.store.scheme_effects.len();
        changed.store.scheme_effects.push(crate::syntax::node::Effect::Io);
        let direct = super::super::super::table_range(start, changed.store.scheme_effects.len()).unwrap();
        changed.store.schemes[scheme_id.index()].effects.direct = direct;
        changed.store.schemes[scheme_id.index()].effects.known = true;
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic function contract is invalid");
        let mut latent = prepare("pure combine(a, b) { a + b }");
        let (_, scheme_id) = generic_owner(&latent);
        let start = latent.store.scheme_effect_parameters.len();
        latent.store.scheme_effect_parameters.push(super::super::super::schemes::FullEffectParameter { id: 0, binder: 0 });
        latent.store.schemes[scheme_id.index()].effect_parameters = super::super::super::table_range(start, latent.store.scheme_effect_parameters.len()).unwrap();
        let start = latent.store.scheme_effect_terms.len();
        latent.store.scheme_effect_terms.push(super::super::super::schemes::FullEffectTerm { binder: 0, mask: crate::sema::effects::EffectSet::new([crate::syntax::node::Effect::Io]).bits(), reserved: [0; 3] });
        latent.store.schemes[scheme_id.index()].effects.terms = super::super::super::table_range(start, latent.store.scheme_effect_terms.len()).unwrap();
        latent.store.schemes[scheme_id.index()].effects.known = true;
        assert_eq!(FullVerifier::verify(&latent).unwrap_err().message, "generic function contract is invalid");
    }

    #[test]
    fn sealed_binary_operation_schemes_preserve_domains_after_frontend_disposal() {
        let program = Arc::new(prepare("pure combine(a, b) { a + b }\npure divide(a, b) { a / b }\npure subtract(a, b) { a - b }\npure relay(a, b) { combine(a, b) }\npure check_domains() -> Bool {\nlet number: Int = relay(1, 2)\nlet text: Str = relay(\"one\", \" two\")\nlet joined: List[Int] = combine([1], [2])\nlet remaining: Set[Int] = subtract({1, 2, 3, 4}, {3, 4})\nlet ratio: Int = divide(4s, 2s)\nlet scaled: Duration = divide(4s, 2)\nnumber == 3 and text == \"one two\" and joined == [1, 2] and remaining == {1, 2} and ratio == 2 and scaled == 2s\n}"));
        for force_frames in [false, true] {
            let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.force_frames = force_frames;
            evaluator.indexed_program = Some(Arc::clone(&program));
            let key = program.symbol_owner().with_current(|| crate::runtime::eval::LoweredFunctionKey::Name(Name::intern("check_domains")));
            let actual = evaluator.call_indexed_direct(key, crate::runtime::eval::CallableKind::Pure, &[], crate::source::Span::at(program.store.source_id, 0)).unwrap().unwrap();
            assert_eq!(actual, crate::runtime::value::Value::Bool(true), "frames={force_frames}");
        }
    }

    #[test]
    fn sealed_binary_operations_preserve_directional_list_item_refinements() {
        prepare("pure join(a: Path, b: RelPath) { [a] + [b] }\npure check_paths(absolute: Path, relative: RelPath) -> List[Path] { join(absolute, relative) }");
        let program = prepare("pure join(a, b) { [a] + [b] }\npure check_paths(absolute: Path, relative: RelPath) -> List[Path] { join(absolute, relative) }");
        let (owner, scheme) = generic_owner(&program);
        let scopes = verify_function_schemes(&program.store, &mut effects::EffectCoverage::new(&program.store)).unwrap();
        let scope = scopes[scheme.index()].as_ref().unwrap();
        let calls = program.store.call_instantiations.iter().filter(|call| program.store.checked_call_targets.iter().any(|target| target.instruction == call.instruction && target.function.index() == owner)).collect::<Vec<_>>();
        assert_eq!(calls.len(), 1);
        let arguments = bounded(calls[0].arguments.type_arguments, &program.store.call_type_arguments, "type arguments").unwrap();
        let mut budget = SchemeInstantiationBudget::new(&program.store);
        let actual = arguments.iter().map(|argument| decode_scheme_type(&program.store, scope, argument.ty, &mut budget, 0).unwrap().closed_ground_type().unwrap()).collect::<Vec<_>>();
        assert!(actual.contains(&Type::Path));
        assert!(actual.contains(&Type::rel_path()));
    }

    #[test]
    fn sealed_binary_operations_retain_set_item_key_requirements() {
        prepare("pure remove(values: Set[Int]) { values - {1, 2} }\npure remaining() -> Set[Int] { remove({1, 2, 3}) }");
        prepare("pure remove(values) { values - {1, 2} }\npure remaining() -> Set[Int] { remove({1, 2, 3}) }");
    }

    #[test]
    fn generic_fsroot_calls_authenticate_native_target_receiver_effects_and_owner() {
        use crate::modules::RuntimeOp;
        let program = prepare("proc listed(marker, root: FsRoot, wrong: Str) [fs, error] { let _ = marker; let _ = wrong; root.children(p\".\")? }\npure foreign(value: Str) -> Str { value }\n");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let (call, receiver) = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::ModuleCall { op: RuntimeOp::FsRootChildren, mut args, .. }) => Some((instruction, args.next().unwrap().unwrap().unwrap().raw() as usize)),
            _ => None,
        }).unwrap();
        assert!(matches!(decoder.expression_view(receiver).unwrap(), schema::ExprView::Param(_)));
        let mut wrong_receiver = program.clone();
        let words = wrong_receiver.store.data[receiver].range().bounds(wrong_receiver.store.extra.len()).unwrap();
        wrong_receiver.store.extra[words.start] = 2;
        assert!(FullVerifier::verify(&wrong_receiver).is_err());
        let mut wrong_target = program.clone();
        let words = wrong_target.store.data[call].range().bounds(wrong_target.store.extra.len()).unwrap();
        let operation = wrong_target.store.extra[words.start] as usize;
        wrong_target.store.runtime_ops[operation] = RuntimeOp::FsRootReadText;
        assert!(FullVerifier::verify(&wrong_target).is_err());
        let direct = program.store.schemes[scheme.index()].effects.direct.bounds(program.store.scheme_effects.len()).unwrap();
        let error = direct.clone().find(|index| program.store.scheme_effects[*index] == crate::syntax::node::Effect::Error).unwrap();
        let mut missing_effect = program.clone();
        missing_effect.store.schemes[scheme.index()].effects.direct = super::super::super::IrRange { start: error as u32, len: 1 };
        assert!(FullVerifier::verify(&missing_effect).is_err());
        let foreign = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "foreign").unwrap();
        let mut wrong_owner = program.clone();
        let fact = wrong_owner.store.scheme_body_facts.iter_mut().find(|fact| fact.function.index() == owner && fact.instruction as usize == receiver).unwrap();
        fact.function = IrFunctionId::new(foreign).unwrap();
        assert!(FullVerifier::verify(&wrong_owner).is_err());
    }

    #[test]
    fn generic_collect_accumulators_authenticate_items_writers_and_owners() {
        let program = prepare("pure gather(value, other) { collect { yield value } }\npure foreign(value) { value }\n");
        let (owner, _) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let read = program.store.function_instruction_range(owner).unwrap().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(0)))).unwrap();
        let mut wrong_item = program.clone();
        let words = wrong_item.store.data[read].range().bounds(wrong_item.store.extra.len()).unwrap();
        wrong_item.store.extra[words.start] = 1;
        assert!(FullVerifier::verify(&wrong_item).is_err());
        let declaration = declaration(&program);
        let ty = program.store.scheme_body_facts[declaration].ty().unwrap();
        let mut immutable = program.clone();
        immutable.store.scheme_body_facts[declaration].value = super::super::super::schemes::FullSchemeBodyFactValue::LocalImmutable(ty);
        assert!(FullVerifier::verify(&immutable).is_err());
        let mut missing = program.clone();
        remove_scheme_body_fact(&mut missing, declaration);
        assert!(FullVerifier::verify(&missing).is_err());
        let foreign = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "foreign").unwrap();
        let mut wrong_owner = program.clone();
        wrong_owner.store.scheme_body_facts[declaration].function = IrFunctionId::new(foreign).unwrap();
        assert!(FullVerifier::verify(&wrong_owner).is_err());
    }

    #[test]
    fn generic_collect_unsigned_guards_require_fresh_writers_and_exact_owners() {
        use crate::syntax::node::AssignOp;
        let program = prepare("pure gather(marker, number: Int) -> List[UInt] { let _ = marker; let rows: List[UInt] = collect { yield number }; rows }\npure foreign(value: List[Int]) -> List[Int] { value }\n");
        let (owner, scheme) = generic_owner(&program);
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let guard = range.clone().find(|instruction| match decoder.expression_view(*instruction) {
            Ok(schema::ExprView::CheckedValue { value, check: ValueCheckView::Ground(check), .. }) => check.ty.materialize().unwrap().has_unsigned_constraint() && program.store.tags[value.raw() as usize] == super::super::super::FullTag::ExprValueBlock,
            _ => false,
        }).unwrap();
        let parameters = program.store.scheme(scheme).unwrap().params.bounds(program.store.scheme_params.len()).unwrap();
        let integer = program.store.scheme_types[program.store.scheme_params[parameters.start + 1].ty.index()];
        assert_eq!(integer.tag, FullSchemeTypeTag::Ground);
        let mut wrong_guard = program.clone();
        let words = wrong_guard.store.data[guard].range().bounds(wrong_guard.store.extra.len()).unwrap();
        wrong_guard.store.extra[words.start + 2] = integer.first;
        assert!(FullVerifier::verify(&wrong_guard).is_err());
        let writer = range.clone().find(|instruction| matches!(decoder.statement_view(*instruction), Ok(schema::StmtView::Assign { op: AssignOp::Add, .. }))).unwrap();
        let mut wrong_writer = program.clone();
        let words = wrong_writer.store.data[writer].range().bounds(wrong_writer.store.extra.len()).unwrap();
        let operation = wrong_writer.store.extra[words.start + 1] as usize;
        wrong_writer.store.assign_ops[operation] = AssignOp::Set;
        let changed_decoder = decoder_for_function(&wrong_writer.store, owner).unwrap();
        assert!(matches!(changed_decoder.statement_view(writer).unwrap(), schema::StmtView::Assign { op: AssignOp::Set, .. }));
        assert!(FullVerifier::verify(&wrong_writer).is_err());
        let foreign = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "foreign").unwrap();
        let foreign_value = program.store.function_instruction_range(foreign).unwrap().find(|instruction| program.store.tags[*instruction] == super::super::super::FullTag::ExprParam).unwrap();
        let mut wrong_owner = program.clone();
        let words = wrong_owner.store.data[guard].range().bounds(wrong_owner.store.extra.len()).unwrap();
        wrong_owner.store.extra[words.start] = foreign_value as u32;
        assert!(FullVerifier::verify(&wrong_owner).is_err());
    }

    #[test]
    fn generic_collect_predicates_validate_present_actual_substitutions() {
        use crate::sema::sealed_operation::{SealedOperationSignature, TypeDomain};
        use crate::sema::schemes::SealedOperationBinding;
        use super::super::super::schemes::FullSealedOperation;
        let program = prepare("stream values() [] -> Stream[Int] { yield 1 }\npure gather(value, other: Stream[Int]) { collect { yield value } }\nlet rows = gather([1, 2], values())\n");
        let function = program.store.functions.iter().find(|function| program.store.string(function.name).unwrap() == "gather").unwrap();
        let FullCallableContract::Scheme(scheme) = function.contract else { panic!("gather keeps its checked scheme") };
        let (predicate, subject) = program.store.scheme_operations.iter().enumerate().find_map(|(index, operation)| match operation {
            FullSealedOperation::TypePredicate { domain: TypeDomain::CollectItem, subject } => Some((index, *subject)),
            _ => None,
        }).unwrap();
        let mut wrong_domain = program.clone();
        wrong_domain.store.scheme_operations[predicate] = FullSealedOperation::TypePredicate { domain: TypeDomain::SortKey, subject };
        for binding in &mut wrong_domain.store.call_operation_bindings {
            if matches!(binding, SealedOperationBinding::Selected(SealedOperationSignature::TypePredicate(TypeDomain::CollectItem))) {
                *binding = SealedOperationBinding::Selected(SealedOperationSignature::TypePredicate(TypeDomain::SortKey));
            }
        }
        assert!(FullVerifier::verify(&wrong_domain).is_err());
        let parameters = program.store.scheme(scheme).unwrap().params.bounds(program.store.scheme_params.len()).unwrap();
        let other = program.store.scheme_params[parameters.start + 1].ty;
        let mut wrong_subject = program.clone();
        wrong_subject.store.scheme_operations[predicate] = FullSealedOperation::TypePredicate { domain: TypeDomain::CollectItem, subject: other };
        assert!(FullVerifier::verify(&wrong_subject).is_err());
    }

    #[test]
    fn generic_pipeline_input_policy_authenticates_domain_and_error_payload() {
        let program = prepare("pure collect_values(values) { values |> collect }");
        let (_, scheme) = generic_owner(&program);
        let scheme = program.store.scheme(scheme).unwrap();
        let parameters = scheme.type_parameters.bounds(program.store.scheme_type_parameters.len()).unwrap();
        let parameter = program.store.scheme_type_parameters[parameters.clone()].iter().position(|parameter| parameter.iteration_item != IR_NONE).unwrap() + parameters.start;
        use crate::runtime::eval::indexed::full::schemes::FullIterationPolicy;
        for policy in [FullIterationPolicy::IterableInfallible as u8, FullIterationPolicy::PipelinePropagates as u8, u8::MAX] {
            let mut changed = program.clone();
            changed.store.scheme_type_parameters[parameter].iteration_policy = policy;
            assert!(FullVerifier::verify(&changed).is_err(), "policy {policy}");
        }
        let mut changed = program.clone();
        changed.store.scheme_type_parameters[parameter].iteration_error = changed.store.scheme_type_parameters[parameter].iteration_item;
        assert!(FullVerifier::verify(&changed).is_err());
        let mut changed = program.clone();
        changed.store.scheme_type_parameters[parameter].iteration_item = IR_NONE;
        changed.store.scheme_type_parameters[parameter].iteration_policy = FullIterationPolicy::IterableInfallible as u8;
        assert!(FullVerifier::verify(&changed).is_err());
    }

    #[test]
    fn generic_iteration_input_relationship_preserves_item_refinements() {
        prepare("pure paths(values: List[RelPath]) -> List[Path] { [value for value in values] }\npure check_paths(relative: RelPath) { let copied = paths([relative]); copied[0] == relative }");
        let program = prepare("pure paths(values) -> List[Path] { [value for value in values] }\npure check_paths(relative: RelPath) { let copied = paths([relative]); copied[0] == relative }");
        let (owner, scheme_id) = generic_owner(&program);
        let scheme = program.store.scheme(scheme_id).unwrap();
        let parameters = bounded(scheme.type_parameters, &program.store.scheme_type_parameters, "type parameters").unwrap();
        let source = parameters.iter().find(|parameter| parameter.iteration_item != IR_NONE).unwrap();
        let scopes = verify_function_schemes(&program.store, &mut effects::EffectCoverage::new(&program.store)).unwrap();
        let scope = scopes[scheme_id.index()].as_ref().unwrap();
        let calls = program.store.call_instantiations.iter().filter(|call| program.store.checked_call_targets.iter().any(|target| target.instruction == call.instruction && target.function.index() == owner)).collect::<Vec<_>>();
        assert_eq!(calls.len(), 1);
        let arguments = bounded(calls[0].arguments.type_arguments, &program.store.call_type_arguments, "type arguments").unwrap();
        let source = arguments.iter().find(|argument| argument.binder == source.id).unwrap();
        let mut budget = SchemeInstantiationBudget::new(&program.store);
        assert_eq!(decode_scheme_type(&program.store, scope, source.ty, &mut budget, 0).unwrap().closed_ground_type(), Some(Type::List(Box::new(Type::rel_path()))));
    }

    #[test]
    fn generic_iteration_input_relationship_authenticates_the_item_binder() {
        let program = prepare("pure copy(values, other) { [item for item in values] }");
        let (_, scheme) = generic_owner(&program);
        let mut changed = program.clone();
        let scheme = changed.store.scheme(scheme).unwrap();
        let parameters = scheme.type_parameters.bounds(changed.store.scheme_type_parameters.len()).unwrap();
        let other = bounded(scheme.params, &changed.store.scheme_params, "parameters").unwrap()[1].ty;
        let parameter = changed.store.scheme_type_parameters[parameters.clone()].iter().position(|parameter| parameter.iteration_item != IR_NONE).unwrap() + parameters.start;
        changed.store.scheme_type_parameters[parameter].iteration_item = other.raw();
        assert!(FullVerifier::verify(&changed).is_err());
    }

    #[test]
    fn generic_iteration_input_relationship_authenticates_the_result_domain() {
        let program = prepare("error Narrow = Failed\nproc copy(values, boundary: Result[Int, Narrow]) [error] { [item for item in values] }\nlet values: Result[List[Int]] = Ok([1])\nlet selected = copy(values, Ok(0))?");
        let (_, scheme) = generic_owner(&program);
        let scheme = program.store.scheme(scheme).unwrap();
        let parameters = scheme.type_parameters.bounds(program.store.scheme_type_parameters.len()).unwrap();
        let parameter = program.store.scheme_type_parameters[parameters.clone()].iter().position(|parameter| parameter.iteration_item != IR_NONE).unwrap() + parameters.start;
        for mode in [0, 1] {
            let mut changed = program.clone();
            if mode == 0 { changed.store.scheme_type_parameters[parameter].iteration_source_kind = crate::sema::schemes::IterationSourceKind::Direct; }
            else {
                let params = scheme.params.bounds(program.store.scheme_params.len()).unwrap();
                let boundary = program.store.scheme_types[program.store.scheme_params[params.start + 1].ty.index()];
                assert_eq!(boundary.tag, FullSchemeTypeTag::Result);
                let narrow = program.store.scheme_types[boundary.second as usize];
                assert_eq!(narrow.tag, FullSchemeTypeTag::Ground);
                let error = changed.store.scheme_type_parameters[parameter].iteration_error as usize;
                assert_eq!(changed.store.scheme_types[error].tag, FullSchemeTypeTag::Ground);
                changed.store.scheme_types[error].first = narrow.first;
            }
            assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, "generic substitution violates its iteration relationship");
        }
    }

    #[test]
    fn guard_arguments_authenticate_actual_payloads_handlers_writers_and_owners() {
        let program = prepare(r#"pure same(value) { value }
proc foreign(raw: Result[Str, Str]) -> Str { guard let text = raw else { return "foreign" }; text }
proc selected(raw: Result[Str, Str], other: Result[Int, Str], bad: Int) -> Str {
    var spare = 3
    let spare_other: Any = other
    let spare_bad: Any = bad
    guard let text = raw else { |failure| return same(failure) }
    spare = 7
    same(text)
}
"#);
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "selected").unwrap();
        let foreign = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "foreign").unwrap();
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let (guard, source, target, failure, handler_return) = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Guard { value, target: super::super::super::decode::CompTargetView::Slot(target), else_param_slot: Some(failure), mut else_body, .. }) => {
                let handler_return = else_body.find_map(|statement| {
                    let statement = statement.unwrap().raw() as usize;
                    matches!(decoder.statement_view(statement), Ok(schema::StmtView::Return { .. })).then_some(statement)
                }).unwrap();
                Some((instruction, value.raw() as usize, target, failure, handler_return))
            }
            _ => None,
        }).unwrap();
        let parameter = |slot| range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(actual)) if actual == slot)).unwrap();
        let assignment = range.clone().find(|instruction| matches!(decoder.statement_view(*instruction), Ok(schema::StmtView::AssignInt { .. }))).unwrap();
        let foreign_decoder = decoder_for_function(&program.store, foreign).unwrap();
        let foreign_source = program.store.function_instruction_range(foreign).unwrap().find(|instruction| matches!(foreign_decoder.expression_view(*instruction), Ok(schema::ExprView::Param(0)))).unwrap();
        assert_eq!(GroundProducerIndex::new(&program.store).unwrap().demand(), crate::runtime::eval::ProducerDemand::Needed);
        let cold = || {
            let mut changed = program.clone();
            changed.store.in_place.take();
            assert!(changed.store.in_place.get().is_none());
            changed
        };
        FullVerifier::verify(&cold()).unwrap();
        for (label, from, to) in [("payload", source, parameter(1)), ("source", source, parameter(2)), ("handler binding", parameter(target), parameter(failure))] {
            let mut changed = cold();
            changed.store.tags.swap(from, to);
            changed.store.data.swap(from, to);
            let error = FullVerifier::verify(&changed).unwrap_err();
            assert_eq!(error.message, match label { "payload" => "generic call argument does not match its checked parameter slot", "source" => "ground guard has no slot transfer contract", "handler binding" => "ground producer use has no visible lexical declaration", _ => unreachable!() }, "{label}");
        }
        let mut written = cold();
        let words = written.store.data[assignment].range().bounds(written.store.extra.len()).unwrap();
        written.store.extra[words.start] = target as u32;
        let error = FullVerifier::verify(&written).unwrap_err();
        assert_eq!(error.message, "integer writer disagrees with its ground slot contract");
        let mut continuing = cold();
        assert_eq!(continuing.store.tags[handler_return], FullTag::StmtReturn);
        continuing.store.tags[handler_return] = FullTag::StmtValue;
        let error = FullVerifier::verify(&continuing).unwrap_err();
        assert_eq!(error.message, "ground guard failure branch can complete");
        let mut premature = cold();
        let body = IrBlockId::from_raw(premature.store.functions[owner].body).unwrap();
        let words = premature.store.blocks[body.index()].instructions.bounds(premature.store.extra.len()).unwrap();
        let statements = (words.start + 1)..words.end;
        let guard_word = statements.clone().find(|word| premature.store.extra[*word] == guard as u32).unwrap();
        let return_word = statements.filter(|word| premature.store.tags[premature.store.extra[*word] as usize] == FullTag::StmtReturn).last().unwrap();
        premature.store.extra.swap(guard_word, return_word);
        let error = FullVerifier::verify(&premature).unwrap_err();
        assert_eq!(error.message, "ground producer use has no visible lexical declaration");
        let mut foreign_read = cold();
        let words = foreign_read.store.data[guard].range().bounds(foreign_read.store.extra.len()).unwrap();
        let references = words.filter(|word| foreign_read.store.extra[*word] == source as u32).collect::<Vec<_>>();
        assert_eq!(references.len(), 1);
        foreign_read.store.extra[references[0]] = foreign_source as u32;
        assert_eq!(FullVerifier::verify(&foreign_read).unwrap_err().message, "full IR instruction belongs to another function");
    }

    #[test]
    fn fresh_comprehension_arguments_authenticate_actual_sources_items_and_filters() {
        let program = prepare(r#"pure same(value) { value }
pure foreign(rows: List[Int]) -> List[Int] { rows }
pure selected(rows: List[Int], other: List[Str], selected: Bool, wrong: Int, wrong_item: Bool) -> List[Int] {
    let spare_other: Any = other
    let spare_selected: Any = selected
    let spare_wrong: Any = wrong
    same([{ let spare_item: Any = wrong_item; item } for item in rows if selected])
}
"#);
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "selected").unwrap();
        let foreign = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "foreign").unwrap();
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let (comprehension, value, source, filter, target) = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::ListComp { value, mut qualifiers, .. }) => {
                let super::super::super::decode::CompQualifierView::For { iter, target: super::super::super::decode::CompTargetView::Slot(target), .. } = qualifiers.next().unwrap().unwrap() else { panic!("the first qualifier binds the actual source") };
                let super::super::super::decode::CompQualifierView::If { condition, .. } = qualifiers.next().unwrap().unwrap() else { panic!("the second qualifier filters the actual items") };
                Some((instruction, value.raw() as usize, iter.raw() as usize, condition.raw() as usize, target))
            }
            _ => None,
        }).unwrap();
        let parameter = |slot| range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(actual)) if actual == slot)).unwrap();
        assert!(matches!(decoder.expression_view(source).unwrap(), schema::ExprView::Param(0)));
        assert!(matches!(decoder.expression_view(filter).unwrap(), schema::ExprView::Param(2)));
        let schema::ExprView::ValueBlock { mut body, .. } = decoder.expression_view(value).unwrap() else { panic!("the item has a lexical value block") };
        let mut projection = None;
        let mut wrong_item = None;
        for statement in &mut body {
            match decoder.statement_view(statement.unwrap().raw() as usize).unwrap() {
                schema::StmtView::Let { value, .. } if matches!(decoder.expression_view(value.raw() as usize), Ok(schema::ExprView::Param(4))) => wrong_item = Some(value.raw() as usize),
                schema::StmtView::Value { value } if matches!(decoder.expression_view(value.raw() as usize), Ok(schema::ExprView::Param(slot)) if slot == target) => projection = Some(value.raw() as usize),
                _ => {},
            }
        }
        let foreign_decoder = decoder_for_function(&program.store, foreign).unwrap();
        let foreign_source = program.store.function_instruction_range(foreign).unwrap().find(|instruction| matches!(foreign_decoder.expression_view(*instruction), Ok(schema::ExprView::Param(0)))).unwrap();
        assert_eq!(GroundProducerIndex::new(&program.store).unwrap().demand(), crate::runtime::eval::ProducerDemand::Needed);
        for uncached in [false, true] {
            let mut original = program.clone();
            if uncached { original.store.in_place.take(); }
            FullVerifier::verify(&original).unwrap();
            for (label, from, to) in [
                ("source item", source, parameter(1)),
                ("filter", filter, parameter(3)),
                ("projection", projection.unwrap(), wrong_item.unwrap()),
            ] {
                let mut changed = original.clone();
                if uncached { changed.store.in_place.take(); assert!(changed.store.in_place.get().is_none()); }
                changed.store.tags.swap(from, to);
                changed.store.data.swap(from, to);
                let error = FullVerifier::verify(&changed).unwrap_err();
                assert_eq!(error.message, if label == "filter" { "condition producer disagrees with its control role" } else { "generic call argument does not match its checked parameter slot" }, "{label} uncached={uncached}");
            }
            let mut foreign_read = original.clone();
            if uncached { foreign_read.store.in_place.take(); assert!(foreign_read.store.in_place.get().is_none()); }
            let words = foreign_read.store.data[comprehension].range().bounds(foreign_read.store.extra.len()).unwrap();
            let qualifiers = IrBlockId::from_raw(foreign_read.store.extra[words.start + 1]).unwrap();
            let words = foreign_read.store.blocks[qualifiers.index()].instructions.bounds(foreign_read.store.extra.len()).unwrap();
            let references = words.filter(|word| foreign_read.store.extra[*word] == source as u32).collect::<Vec<_>>();
            assert_eq!(references.len(), 1);
            foreign_read.store.extra[references[0]] = foreign_source as u32;
            let error = FullVerifier::verify(&foreign_read).unwrap_err();
            assert_eq!(error.message, "full IR instruction belongs to another function", "uncached={uncached}");
        }
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
        let (_, scheme) = generic_owner(&program);
        let fact = program.store.scheme_body_facts.iter().position(|fact| fact.kind() == FullSchemeBodyFactKind::LocalImmutable && program.store.tags[fact.instruction as usize] == super::super::super::FullTag::StmtLetRecord).unwrap();
        let mut missing = program.clone();
        missing.store.scheme_body_facts[fact].value = super::super::super::schemes::FullSchemeBodyFactValue::Expression(missing.store.scheme_body_facts[fact].ty().unwrap());
        assert_eq!(FullVerifier::verify(&missing).unwrap_err().message, "generic body fact has an invalid instruction kind");
        let mut changed = program.clone();
        let mut conflicting = program.store.scheme_body_facts[fact];
        conflicting.value = super::super::super::schemes::FullSchemeBodyFactValue::LocalMutable(conflicting.ty().unwrap());
        append_scheme_body_fact(&mut changed, scheme, conflicting);
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
        assert_eq!(FullVerifier::verify(&changed).unwrap_err().message, format!("executable operation requires effect env outside its owner contract: executable operation requires effects outside its owner contract at callable {owner}, instruction {call} ExprCall"));
    }


    fn guarded_record_fixture() -> FullProgram {
        let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/generic-record-field-guards.xsh")).unwrap();
        prepare(&source)
    }

    fn guarded_record_fact(program: &FullProgram, name: &str, narrowed: bool) -> usize {
        let _symbols = program.symbol_owner().enter();
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == name).unwrap();
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let scopes = verify_function_schemes(&program.store, &mut effects::EffectCoverage::new(&program.store)).unwrap();
        let FullCallableContract::Scheme(scheme) = program.store.functions[owner].contract else { panic!("the record reader retains its generic marker") };
        let scope = scopes[scheme.index()].as_ref().unwrap();
        let mut budget = SchemeInstantiationBudget::new(&program.store);
        program.store.scheme_body_facts.iter().position(|fact| {
            if fact.function.index() != owner || fact.kind() != FullSchemeBodyFactKind::Expression
                || !matches!(decoder.expression_view(fact.instruction as usize), Ok(schema::ExprView::Param(_))) { return false; }
            let Some(Type::Record(fields)) = decode_scheme_type(&program.store, scope, fact.ty().unwrap(), &mut budget, 0).unwrap().closed_ground_type() else { return false };
            matches!(fields.get(&program.symbol_owner().intern("diagnostic")), Some(Type::Str)) == narrowed
                && fields.contains_key(&program.symbol_owner().intern("diagnostic"))
        }).unwrap()
    }

    #[test]
    fn generic_record_field_guards_authenticate_physical_field_and_condition() {
        let program = guarded_record_fixture();
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "record_guard").unwrap();
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let field = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Field { name: "diagnostic", .. }))).unwrap();
        let other = program.store.strings.iter().enumerate().find(|(index, _)| program.store.string(*index as u32 + 1).unwrap() == "other").unwrap().0 as u32 + 1;
        let mut wrong_field = program.clone();
        wrong_field.store.in_place.take();
        let words = wrong_field.store.data[field].range().bounds(wrong_field.store.extra.len()).unwrap();
        wrong_field.store.extra[words.start + 1] = other;
        assert!(matches!(decoder_for_function(&wrong_field.store, owner).unwrap().expression_view(field).unwrap(), schema::ExprView::Field { name: "other", .. }));
        assert!(FullVerifier::verify(&wrong_field).is_err());
        let read = program.store.scheme_body_facts[guarded_record_fact(&program, "record_guard", true)].instruction as usize;
        let mut wrong_root = program.clone();
        wrong_root.store.in_place.take();
        let words = wrong_root.store.data[read].range().bounds(wrong_root.store.extra.len()).unwrap();
        assert_eq!(wrong_root.store.extra[words.start], 1);
        wrong_root.store.extra[words.start] = 2;
        assert!(FullVerifier::verify(&wrong_root).is_err());
        let condition = range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::ExprMatch).unwrap();
        let schema::ExprView::MatchExpr { value, mut arms, .. } = decoder.expression_view(condition).unwrap() else { unreachable!() };
        let (_, _, absent) = arms.next().unwrap().unwrap();
        let mut null_succeeds = program.clone();
        null_succeeds.store.in_place.take();
        let words = null_succeeds.store.data[absent.raw() as usize].range().bounds(null_succeeds.store.extra.len()).unwrap();
        null_succeeds.store.extra[words.start] = 1;
        assert!(FullVerifier::verify(&null_succeeds).is_err());
        let foreign_owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "typed_record_guard").unwrap();
        let foreign_decoder = decoder_for_function(&program.store, foreign_owner).unwrap();
        let foreign = program.store.function_instruction_range(foreign_owner).unwrap().find(|&instruction| matches!(foreign_decoder.expression_view(instruction), Ok(schema::ExprView::Field { name: "diagnostic", .. }))).unwrap();
        let mut escaped = program.clone();
        escaped.store.in_place.take();
        let words = escaped.store.data[condition].range().bounds(escaped.store.extra.len()).unwrap();
        assert_eq!(escaped.store.extra[words.start], value.raw());
        escaped.store.extra[words.start] = foreign as u32;
        assert!(FullVerifier::verify(&escaped).is_err());
    }

    #[test]
    fn generic_record_field_guards_reject_nonoptional_type_changes() {
        let program = guarded_record_fixture();
        let fact = guarded_record_fact(&program, "record_guard", true);
        let record = program.store.scheme_body_facts[fact].ty().unwrap();
        let counts = record_field_type(&program.store, record, "counts");
        let unsigned = record_field_type(&program.store, counts, "bytes");
        let node = program.store.scheme_types[unsigned.index()];
        assert_eq!(node.tag, FullSchemeTypeTag::Ground);
        assert_eq!(program.store.semantic.to_type(super::super::TypeId::from_raw(node.first).unwrap()).unwrap(), Type::UInt);
        let typed = program.store.functions.iter().find(|function| program.store.string(function.name).unwrap() == "typed_record_guard").unwrap();
        let FullCallableContract::Ground(signature) = typed.contract else { unreachable!() };
        let (_, signed, _) = program.store.semantic.signature_param(signature, 0).unwrap();
        assert_eq!(program.store.semantic.to_type(signed).unwrap(), Type::Int);
        let mut changed = program.clone();
        changed.store.in_place.take();
        changed.store.scheme_types[unsigned.index()].first = signed.raw();
        assert!(FullVerifier::verify(&changed).is_err());
    }

    fn guarded_record_carriers(program: &FullProgram, owner: usize, base: usize) -> ([usize; 3], usize, usize) {
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let schema::ExprView::Param(generated_slot) = decoder.expression_view(base).unwrap() else { panic!("field base is its evaluated record temporary") };
        let writers = program.store.function_instruction_range(owner).unwrap().filter_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Let { slot, value, .. }) if slot == generated_slot => Some((instruction, value.raw() as usize)),
            _ => None,
        }).collect::<Vec<_>>();
        assert_eq!(writers.len(), 1);
        let (declaration, raw) = writers[0];
        let schema::ExprView::Param(source_slot) = decoder.expression_view(raw).unwrap() else { panic!("the record temporary reads the actual mutable source") };
        assert_ne!(source_slot, generated_slot);
        let facts = [raw, declaration, base].map(|instruction| program.store.scheme_body_facts.iter().position(|fact| {
            fact.function.index() == owner && fact.instruction as usize == instruction
                && fact.kind() == if instruction == declaration { FullSchemeBodyFactKind::LocalImmutable } else { FullSchemeBodyFactKind::Expression }
        }).unwrap());
        (facts, source_slot, generated_slot)
    }

    #[test]
    fn generic_record_field_guards_reject_refinement_after_a_source_write() {
        let program = guarded_record_fixture();
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "written_record_guard").unwrap();
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let bases = program.store.function_instruction_range(owner).unwrap().filter_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::Field { base, name: "flush", .. }) => Some(base.raw() as usize),
            _ => None,
        }).collect::<Vec<_>>();
        assert_eq!(bases.len(), 2);
        let (before, source, earlier_slot) = guarded_record_carriers(&program, owner, bases[0]);
        let (after, actual_source, later_slot) = guarded_record_carriers(&program, owner, bases[1]);
        assert_eq!(source, actual_source);
        assert_ne!(earlier_slot, later_slot);
        let mut written = program.clone();
        written.store.in_place.take();
        written.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&written).unwrap();
        for (before, after) in before.into_iter().zip(after) {
            let value = written.store.scheme_body_facts[before].value;
            written.store.scheme_body_facts[before].value = written.store.scheme_body_facts[after].value;
            written.store.scheme_body_facts[after].value = value;
        }
        assert!(FullVerifier::verify(&written).is_err());
    }


    #[test]
    fn generic_record_field_guards_reject_stale_precondition_aliases() {
        let mut program = guarded_record_fixture();
        program.store.in_place.take();
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        let _symbols = program.symbol_owner().enter();
        for name in ["stale_record_guard", "stale_block_record_guard"] {
            let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == name).unwrap();
            let decoder = decoder_for_function(&program.store, owner).unwrap();
            let bases = program.store.function_instruction_range(owner).unwrap().filter_map(|instruction| match decoder.expression_view(instruction) {
                Ok(schema::ExprView::Field { base, name: "flush", .. }) => Some(base.raw() as usize),
                _ => None,
            }).collect::<Vec<_>>();
            assert_eq!(bases.len(), 2);
            let (before, source, earlier_slot) = guarded_record_carriers(&program, owner, bases[0]);
            let (after, actual_source, later_slot) = guarded_record_carriers(&program, owner, bases[1]);
            assert_eq!(source, actual_source);
            assert_ne!(earlier_slot, later_slot);
            let mut stale = program.clone();
            stale.store.in_place.take();
            for (before, after) in before.into_iter().zip(after) {
                let value = stale.store.scheme_body_facts[before].value;
                stale.store.scheme_body_facts[before].value = stale.store.scheme_body_facts[after].value;
                stale.store.scheme_body_facts[after].value = value;
            }
            let failure = FullVerifier::verify(&stale).expect_err("a saved field cannot refine the current record after a write");
            assert_eq!(failure.message, "generic body expression disagrees with its checked type", "{name}");
        }
    }


    fn record_continuation_fixture() -> FullProgram {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let source = format!("{}\n{}", std::fs::read_to_string(root.join("tests/fixtures/runtime/generic-record-field-guards.xsh")).unwrap(), std::fs::read_to_string(root.join("tests/fixtures/runtime/generic-record-guard-continuations.xsh")).unwrap());
        prepare(&source)
    }

    #[test]
    fn generic_record_guard_continuations_authenticate_source_and_role() {
        let program = record_continuation_fixture();
        let _symbols = program.symbol_owner().enter();
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "guarded_record_continuation").unwrap();
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let field = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Field { name: "diagnostic", .. }))).unwrap();
        let schema::ExprView::Field { base, .. } = decoder.expression_view(field).unwrap() else { unreachable!() };
        let other = program.store.strings.iter().enumerate().find(|(index, _)| program.store.string(*index as u32 + 1).unwrap() == "other").unwrap().0 as u32 + 1;
        let mut wrong_field = program.clone();
        wrong_field.store.in_place.take();
        let words = wrong_field.store.data[field].range().bounds(wrong_field.store.extra.len()).unwrap();
        wrong_field.store.extra[words.start + 1] = other;
        assert!(matches!(decoder_for_function(&wrong_field.store, owner).unwrap().expression_view(field).unwrap(), schema::ExprView::Field { name: "other", .. }));
        assert!(FullVerifier::verify(&wrong_field).is_err());
        let mut wrong_source = program.clone();
        wrong_source.store.in_place.take();
        let words = wrong_source.store.data[base.raw() as usize].range().bounds(wrong_source.store.extra.len()).unwrap();
        wrong_source.store.extra[words.start] = 2;
        assert!(matches!(decoder_for_function(&wrong_source.store, owner).unwrap().expression_view(base.raw() as usize).unwrap(), schema::ExprView::Param(2)));
        assert!(FullVerifier::verify(&wrong_source).is_err());
        let guard = range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::StmtGuard).unwrap();
        let mut wrong_role = program.clone();
        wrong_role.store.in_place.take();
        let words = wrong_role.store.data[guard].range().bounds(wrong_role.store.extra.len()).unwrap();
        assert_eq!(wrong_role.store.extra[words.end - 1], 1);
        wrong_role.store.extra[words.end - 1] = 0;
        assert!(matches!(decoder_for_function(&wrong_role.store, owner).unwrap().statement_view(guard).unwrap(), schema::StmtView::Guard { optional: false, .. }));
        assert!(FullVerifier::verify(&wrong_role).is_err());
    }

    #[test]
    fn generic_record_guard_continuations_authenticate_handler_scope_and_divergence() {
        let mut program = record_continuation_fixture();
        program.store.in_place.take();
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        let _symbols = program.symbol_owner().enter();
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "handler_record_continuation").unwrap();
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let bases = program.store.function_instruction_range(owner).unwrap().filter_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::Field { base, name: "flush", .. }) => Some(base.raw() as usize),
            _ => None,
        }).collect::<Vec<_>>();
        assert_eq!(bases.len(), 2);
        let (handler, source, handler_slot) = guarded_record_carriers(&program, owner, bases[0]);
        let (success, actual_source, success_slot) = guarded_record_carriers(&program, owner, bases[1]);
        assert_eq!(source, actual_source);
        assert_ne!(handler_slot, success_slot);
        let handler_diagnostic = record_field_type(&program.store, program.store.scheme_body_facts[handler[0]].ty().unwrap(), "diagnostic");
        let success_diagnostic = record_field_type(&program.store, program.store.scheme_body_facts[success[0]].ty().unwrap(), "diagnostic");
        assert_eq!(program.store.scheme_types[handler_diagnostic.index()].tag, FullSchemeTypeTag::Optional);
        assert_eq!(program.store.scheme_types[success_diagnostic.index()].tag, FullSchemeTypeTag::Ground);
        let mut escaped = program.clone();
        escaped.store.in_place.take();
        for (handler, success) in handler.into_iter().zip(success) {
            let value = escaped.store.scheme_body_facts[handler].value;
            escaped.store.scheme_body_facts[handler].value = escaped.store.scheme_body_facts[success].value;
            escaped.store.scheme_body_facts[success].value = value;
        }
        let failure = FullVerifier::verify(&escaped).expect_err("a guard failure handler cannot borrow the success refinement");
        assert_eq!(failure.message, "generic body expression disagrees with its checked type");
        let returned = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Guard { mut else_body, .. }) => else_body.find_map(|statement| {
                let instruction = statement.unwrap().raw() as usize;
                (program.store.tags[instruction] == FullTag::StmtReturn).then_some(instruction)
            }),
            _ => None,
        }).unwrap();
        let mut completing = program.clone();
        completing.store.in_place.take();
        completing.store.tags[returned] = FullTag::StmtValue;
        let failure = FullVerifier::verify(&completing).expect_err("the guard failure branch must terminate");
        assert_eq!(failure.message, "generic guard failure branch can complete");
    }

    #[test]
    fn generic_record_guard_continuations_reject_written_and_stale_sources() {
        let mut program = record_continuation_fixture();
        program.store.in_place.take();
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        let _symbols = program.symbol_owner().enter();
        for name in ["written_record_continuation", "stale_record_continuation"] {
            let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == name).unwrap();
            let decoder = decoder_for_function(&program.store, owner).unwrap();
            let bases = program.store.function_instruction_range(owner).unwrap().filter_map(|instruction| match decoder.expression_view(instruction) {
                Ok(schema::ExprView::Field { base, name: "flush", .. }) => Some(base.raw() as usize),
                _ => None,
            }).collect::<Vec<_>>();
            assert_eq!(bases.len(), 2);
            let (before, source, earlier_slot) = guarded_record_carriers(&program, owner, bases[0]);
            let (after, actual_source, later_slot) = guarded_record_carriers(&program, owner, bases[1]);
            assert_eq!(source, actual_source);
            assert_ne!(earlier_slot, later_slot);
            let mut stale = program.clone();
            stale.store.in_place.take();
            for (before, after) in before.into_iter().zip(after) {
                let value = stale.store.scheme_body_facts[before].value;
                stale.store.scheme_body_facts[before].value = stale.store.scheme_body_facts[after].value;
                stale.store.scheme_body_facts[after].value = value;
            }
            let failure = FullVerifier::verify(&stale).expect_err("a write invalidates the original record field refinement");
            assert_eq!(failure.message, "generic body expression disagrees with its checked type", "{name}");
        }
    }


    fn direct_env_fixture() -> FullProgram {
        let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/generic-env-values.xsh")).unwrap();
        prepare(&source)
    }

    #[test]
    fn generic_env_overlay_requires_its_own_conversion_evidence() {
        let program = direct_env_fixture();
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "direct_env_pair").unwrap();
        let FullCallableContract::Scheme(scheme) = program.store.functions[owner].contract else { unreachable!() };
        let parameters = program.store.schemes[scheme.index()].type_parameters.bounds(program.store.scheme_type_parameters.len()).unwrap();
        assert_eq!(parameters.len(), 2);
        let env = 1 << (ValueConversion::Env as u8);
        for index in parameters {
            assert_eq!(program.store.scheme_type_parameters[index].conversions, env);
            for replacement in [0, 1 << (ValueConversion::ArgvOrList as u8)] {
                let mut missing = program.clone();
                missing.store.in_place.take();
                missing.store.scheme_type_parameters[index].conversions = replacement;
                assert!(missing.store.in_place.get().is_none());
                let failure = FullVerifier::verify(&missing).expect_err("the body must retain the Env conversion for each actual field");
                assert_eq!(failure.message, "generic environment overlay value has no Env conversion requirement");
            }
        }
    }

    #[test]
    fn generic_env_overlay_authenticates_fields_and_actual_parameter_owners() {
        let program = direct_env_fixture();
        let _symbols = program.symbol_owner().enter();
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "direct_env_pair").unwrap();
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let record = program.store.function_instruction_range(owner).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprRecord).unwrap();
        let schema::ExprView::Record(mut entries) = decoder.expression_view(record).unwrap() else { unreachable!() };
        assert_eq!(entries.len(), 2);
        let block = entries.block_identity_for_test();
        let super::super::super::decode::RecordEntryView::Field(left_name, left) = entries.next().unwrap().unwrap() else { unreachable!() };
        let super::super::super::decode::RecordEntryView::Field(right_name, _) = entries.next().unwrap().unwrap() else { unreachable!() };
        assert_ne!(left_name, right_name);
        let schema::ExprView::Param(temporary) = decoder.expression_view(left.raw() as usize).unwrap() else { unreachable!() };
        let raw = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Let { slot, value, .. }) if slot == temporary => Some(value.raw() as usize),
            _ => None,
        }).unwrap();
        assert!(matches!(decoder.expression_view(raw).unwrap(), schema::ExprView::Param(0)));
        let mut wrong_source = program.clone();
        wrong_source.store.in_place.take();
        let words = wrong_source.store.data[raw].range().bounds(wrong_source.store.extra.len()).unwrap();
        assert_eq!(words.len(), 1);
        wrong_source.store.extra[words.start] = 1;
        assert!(wrong_source.store.in_place.get().is_none());
        let failure = FullVerifier::verify(&wrong_source).expect_err("another Env-capable binder cannot supply the field value");
        assert_eq!(failure.message, "generic body expression disagrees with its checked type");
        let mut wrong_key = program.clone();
        wrong_key.store.in_place.take();
        let words = wrong_key.store.blocks[block.index()].instructions.bounds(wrong_key.store.extra.len()).unwrap();
        assert_eq!(words.len(), 7);
        assert_eq!(&wrong_key.store.extra[words.start..words.start + 3], &[2, 0, left_name.symbol().raw()]);
        assert_eq!(&wrong_key.store.extra[words.start + 4..words.start + 6], &[0, right_name.symbol().raw()]);
        wrong_key.store.extra[words.start + 2] = right_name.symbol().raw();
        assert!(wrong_key.store.in_place.get().is_none());
        let failure = FullVerifier::verify(&wrong_key).expect_err("the actual record fields must agree with the overlay contract");
        assert_eq!(failure.message, "generic record producer repeats a field");
        let foreign = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "direct_env_value").unwrap();
        let fact = program.store.scheme_body_facts.iter().position(|fact| fact.function.index() == owner && fact.instruction as usize == raw && fact.kind() == FullSchemeBodyFactKind::Expression).unwrap();
        let mut foreign_owner = program.clone();
        foreign_owner.store.in_place.take();
        foreign_owner.store.scheme_body_facts[fact].function = super::super::super::IrFunctionId::new(foreign).unwrap();
        assert!(foreign_owner.store.in_place.get().is_none());
        let failure = FullVerifier::verify(&foreign_owner).expect_err("a foreign function cannot own the overlay producer fact");
        assert!(failure.message.contains("owner"), "{failure:?}");
    }


    #[test]
    fn comprehension_list_builds_authenticate_members_splices_and_lexical_sources() {
        let source = r#"type Entry = {name: Str, outputs: List[Path], argv: List[Union[Str, Path]]}
pure retain(value) { value }
pure spare() -> Bool { false }
pure nested(rows: List[Entry]) -> Bool {
  let items = [{...row, argv: [p"/head", "--", @row.outputs]} for row in rows]
  retain(items).is_empty()
}
"#;
        let mut program = prepare(source);
        assert_eq!(GroundProducerIndex::new(&program.store).unwrap().demand(), crate::runtime::eval::ProducerDemand::Needed);
        program.store.in_place.take();
        assert!(program.store.in_place.get().is_none());
        FullVerifier::verify(&program).unwrap();
        program.store.in_place.take();
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "nested").unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let build = range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::ExprListBuild).unwrap();
        let schema::ExprView::ListBuild(mut entries) = decoder.expression_view(build).unwrap() else { unreachable!() };
        let block = entries.block_identity_for_test();
        assert_eq!(entries.len(), 3);
        let (head_splice, head, _) = entries.next().unwrap().unwrap();
        let (middle_splice, _, _) = entries.next().unwrap().unwrap();
        let (tail_splice, tail, _) = entries.next().unwrap().unwrap();
        assert!(!head_splice && !middle_splice && tail_splice);
        let head = head.raw() as usize;
        assert_eq!(program.store.tags[head], FullTag::ExprPath);
        let schema::ExprView::Field { base, name: "outputs", .. } = decoder.expression_view(tail.raw() as usize).unwrap() else { panic!("splice reads the actual comprehension field") };
        let base = base.raw() as usize;
        let schema::ExprView::Param(slot) = decoder.expression_view(base).unwrap() else { panic!("the field belongs to the comprehension target") };
        assert_ne!(slot, 0);
        let boolean = program.store.tags.iter().position(|tag| *tag == FullTag::ExprBool).unwrap();
        assert!(!range.contains(&boolean));
        let mut wrong_member = program.clone();
        wrong_member.store.tags[head] = FullTag::ExprBool;
        wrong_member.store.data[head] = wrong_member.store.data[boolean];
        assert!(wrong_member.store.in_place.get().is_none());
        let failure = FullVerifier::verify(&wrong_member).expect_err("Bool cannot replace a Str-or-Path member");
        assert_eq!(failure.message, "generic call argument has no closed checked type");
        let mut wrong_splice = program.clone();
        let words = wrong_splice.store.blocks[block.index()].instructions.bounds(wrong_splice.store.extra.len()).unwrap();
        assert_eq!(wrong_splice.store.extra[words.start], 3);
        assert_eq!(wrong_splice.store.extra[words.start + 1], 0);
        wrong_splice.store.extra[words.start + 1] = 1;
        assert!(wrong_splice.store.in_place.get().is_none());
        let failure = FullVerifier::verify(&wrong_splice).expect_err("a scalar Path cannot supply a list splice");
        assert_eq!(failure.message, "generic call argument has no closed checked type");
        let mut wrong_binding = program.clone();
        let words = wrong_binding.store.data[base].range().bounds(wrong_binding.store.extra.len()).unwrap();
        assert_eq!(wrong_binding.store.extra[words.start] as usize, slot);
        wrong_binding.store.extra[words.start] = 0;
        assert!(wrong_binding.store.in_place.get().is_none());
        let failure = FullVerifier::verify(&wrong_binding).expect_err("the header list cannot replace its comprehension item");
        assert_eq!(failure.message, "generic call argument has no closed checked type");
        let mut foreign_child = program.clone();
        let words = foreign_child.store.blocks[block.index()].instructions.bounds(foreign_child.store.extra.len()).unwrap();
        assert_eq!(foreign_child.store.extra[words.start + 2] as usize, head);
        foreign_child.store.extra[words.start + 2] = boolean as u32;
        assert!(foreign_child.store.in_place.get().is_none());
        let failure = FullVerifier::verify(&foreign_child).expect_err("a list cannot borrow an instruction from another function");
        assert_eq!(failure.message, "full IR instruction belongs to another function");
    }


    #[test]
    fn comprehension_list_builds_reject_possibly_empty_nonempty_evidence() {
        let mut program = prepare(r#"pure nonempty(marker, values: NonEmpty[Path]) { let _ = values; marker }
pure probe(values: List[Path]) -> Int { let spare = values; nonempty(7, [p"/head", @values]) }
"#);
        assert_eq!(GroundProducerIndex::new(&program.store).unwrap().demand(), crate::runtime::eval::ProducerDemand::Needed);
        program.store.in_place.take();
        assert!(program.store.in_place.get().is_none());
        FullVerifier::verify(&program).unwrap();
        program.store.in_place.take();
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "probe").unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let build = range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::ExprListBuild).unwrap();
        let schema::ExprView::ListBuild(mut entries) = decoder.expression_view(build).unwrap() else { unreachable!() };
        let block = entries.block_identity_for_test();
        assert_eq!(entries.len(), 2);
        let (splice, head, _) = entries.next().unwrap().unwrap();
        assert!(!splice);
        let head = head.raw() as usize;
        assert_eq!(program.store.tags[head], FullTag::ExprPath);
        let source = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(0)))).unwrap();
        let mut changed = program.clone();
        changed.store.tags[head] = FullTag::ExprParam;
        changed.store.data[head] = changed.store.data[source];
        let words = changed.store.blocks[block.index()].instructions.bounds(changed.store.extra.len()).unwrap();
        assert_eq!(&changed.store.extra[words.start..words.start + 3], &[2, 0, head as u32]);
        changed.store.extra[words.start + 1] = 1;
        let changed_decoder = decoder_for_function(&changed.store, owner).unwrap();
        let schema::ExprView::ListBuild(mut entries) = changed_decoder.expression_view(build).unwrap() else { unreachable!() };
        assert!(entries.all(|entry| entry.unwrap().0));
        assert!(changed.store.in_place.get().is_none());
        let failure = FullVerifier::verify(&changed).expect_err("two possibly empty splices cannot establish NonEmpty");
        assert_eq!(failure.message, "ground list build has no non-empty producer");
    }


    #[test]
    fn typed_set_operators_authenticate_operand_domains_results_and_owners() {
        let mut program = prepare(r#"const KINDS = {"file", "dir"}
pure allowed(kind: Str, extra: Set[Str], numbers: Set[Int], erased: Any) -> Bool { kind in KINDS | extra }
pure merged(left: Set[Str], right: Set[Str]) -> Set[Str] { left | right }
pure wrong_result(left: Set[Str], right: Set[Str]) -> Set[Int] { let _ = left; let _ = right; set.empty() }
pure other(value: Set[Str]) -> Set[Str] { value }
assert allowed("file", {"link",}, {1, 2}, "erased")
assert "file" in merged(KINDS, {"link",})
"#);
        assert_eq!(GroundProducerIndex::new(&program.store).unwrap().demand(), crate::runtime::eval::ProducerDemand::Needed);
        program.store.in_place.take();
        assert!(program.store.in_place.get().is_none());
        FullVerifier::verify(&program).unwrap();
        program.store.in_place.take();
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "allowed").unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let decoder = decoder_for_function(&program.store, owner).unwrap();
        let (binary, right) = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::Binary { op: crate::syntax::node::BinaryOp::Union, right, .. }) => Some((instruction, right.raw() as usize)),
            _ => None,
        }).unwrap();
        assert!(matches!(decoder.expression_view(right).unwrap(), schema::ExprView::Param(1)));
        for slot in [2, 3] {
            let mut changed = program.clone();
            let words = changed.store.data[right].range().bounds(changed.store.extra.len()).unwrap();
            assert_eq!(words.len(), 1);
            changed.store.extra[words.start] = slot;
            assert!(changed.store.in_place.get().is_none());
            let failure = FullVerifier::verify(&changed).expect_err("a different or erased Set domain cannot supply the operand");
            assert!(failure.message.contains("membership container has no producer contract"), "{}", failure.message);
        }
        let mut wrong_operator = program.clone();
        let operation = wrong_operator.store.binary_ops.iter_mut().find(|op| **op == crate::syntax::node::BinaryOp::Union).unwrap();
        *operation = crate::syntax::node::BinaryOp::Add;
        assert!(matches!(decoder_for_function(&wrong_operator.store, owner).unwrap().expression_view(binary).unwrap(), schema::ExprView::Binary { op: crate::syntax::node::BinaryOp::Add, .. }));
        assert!(wrong_operator.store.in_place.get().is_none());
        let failure = FullVerifier::verify(&wrong_operator).expect_err("arithmetic addition cannot stand in for set union");
        assert!(failure.message.contains("ground producer form has no executable type proof: ExprBinary"), "{}", failure.message);
        let foreign = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "other").unwrap();
        let foreign_decoder = decoder_for_function(&program.store, foreign).unwrap();
        let foreign_source = program.store.function_instruction_range(foreign).unwrap().find(|&instruction| matches!(foreign_decoder.expression_view(instruction), Ok(schema::ExprView::Param(0)))).unwrap();
        let mut wrong_owner = program.clone();
        let words = wrong_owner.store.data[binary].range().bounds(wrong_owner.store.extra.len()).unwrap();
        assert_eq!(words.len(), 4);
        assert_eq!(wrong_owner.store.extra[words.start + 2] as usize, right);
        wrong_owner.store.extra[words.start + 2] = foreign_source as u32;
        assert!(wrong_owner.store.in_place.get().is_none());
        let failure = FullVerifier::verify(&wrong_owner).expect_err("a set operator cannot borrow an operand from another function");
        assert_eq!(failure.message, "full IR instruction belongs to another function");
        let merged = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "merged").unwrap();
        let wrong = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "wrong_result").unwrap();
        let mut wrong_result = program.clone();
        wrong_result.store.functions[merged].contract = wrong_result.store.functions[wrong].contract;
        wrong_result.store.functions[merged].checked_return_type = wrong_result.store.functions[wrong].checked_return_type;
        assert!(wrong_result.store.in_place.get().is_none());
        let failure = FullVerifier::verify(&wrong_result).expect_err("a Set[Str] operation cannot establish a Set[Int] return contract");
        assert_eq!(failure.message, "membership operands have no checked domain");
    }

}

#[cfg(test)]
mod structured_pipeline_tests {
    use super::super::FullSchemeBodyFactKind;
    use super::*;
    use super::super::super::{FullBuilder, FullVerifier};

    fn prepare(source: &str) -> super::super::super::FullProgram {
        let mut sources = crate::source::SourceMap::default();
        let source_id = sources.add_file("generic-structured-pipeline.xsh", source.to_owned());
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = crate::sema::check::Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
        let program = FullBuilder::build_compact(&parsed.arena, &checked, source, Arc::new(sources), source_id).unwrap();
        FullVerifier::verify(&program).unwrap();
        program
    }

    #[test]
    fn noncompleting_map_sort_requires_its_cold_callback_and_owner() {
        let source = "pure target() -> Int { let ignored = [1] |> map { |item| return item } |> sort |> take(0) }\npure foreign() -> Int { let ignored = [2] |> map { |item| return item } |> sort |> take(0) }\n";
        let mut sources = crate::source::SourceMap::default();
        let source_id = sources.add_file("noncompleting-map-sort.xsh", source.to_owned());
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = crate::sema::check::Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
        let mut program = FullBuilder::build_compact(&parsed.arena, &checked, source, Arc::new(sources), source_id).unwrap();
        program.store.in_place.take();
        assert!(program.store.in_place.get().is_none());
        super::super::prepare_tables(&program.store).unwrap();
        let mut ground_functions = program.store.functions.iter().filter(|function| program.store.string(function.name).is_ok_and(|name| name == "target" || name == "foreign"));
        assert_eq!(ground_functions.clone().count(), 2);
        assert!(ground_functions.all(|function| matches!(function.contract, super::super::super::FullCallableContract::Ground(_))));
        FullVerifier::verify(&program).unwrap();

        let map_stage = |program: &super::super::super::FullProgram, name: &str| {
            let owner = program.store.functions.iter().position(|function| {
                program.store.string(function.name).is_ok_and(|function_name| function_name == name)
            }).unwrap();
            let decoder = super::super::decoder_for_function(&program.store, owner).unwrap();
            let pipeline = program.store.function_instruction_range(owner).unwrap().find(|&instruction| {
                matches!(decoder.expression_view(instruction), Ok(schema::ExprView::ListPipeline { .. }))
            }).unwrap();
            let schema::ExprView::ListPipeline { mut stages, .. } = decoder.expression_view(pipeline).unwrap() else { unreachable!() };
            while let Some(stage) = stages.next_with_id() {
                let stage = stage.unwrap();
                let decoded = super::super::super::StageView::decode(
                    super::super::super::DecodeContext::from_decoder(&decoder),
                    stage.id.raw(),
                ).unwrap();
                match decoded {
                    super::super::super::StageView::Map { value, .. }
                    | super::super::super::StageView::MapBlock { value, .. } => {
                        return (stage.id.raw() as usize, value.raw() as usize);
                    }
                    _ => {}
                }
            }
            panic!("{name} has a map stage");
        };

        let (target_stage, _) = map_stage(&program, "target");
        let (_, foreign_value) = map_stage(&program, "foreign");
        let stage_range = program.store.stage_data[target_stage]
            .range()
            .bounds(program.store.extra.len())
            .unwrap();

        let mut value_instead_of_return = program.clone();
        value_instead_of_return.store.in_place.take();
        assert!(value_instead_of_return.store.in_place.get().is_none());
        let target_owner = value_instead_of_return.store.functions.iter().position(|function| {
            value_instead_of_return.store.string(function.name).is_ok_and(|name| name == "target")
        }).unwrap();
        let returned = value_instead_of_return.store.function_instruction_range(target_owner).unwrap()
            .find(|&instruction| value_instead_of_return.store.tags[instruction] == FullTag::StmtReturn)
            .unwrap();
        value_instead_of_return.store.tags[returned] = FullTag::StmtValue;
        super::super::prepare_tables(&value_instead_of_return.store).unwrap();
        let value_denial = FullVerifier::verify(&value_instead_of_return).expect_err("a Value statement cannot claim the noncompleting map callback");
        assert_eq!(value_denial.message, "function 0 body does not terminate with a return");

        let mut foreign_callback = program;
        foreign_callback.store.in_place.take();
        assert!(foreign_callback.store.in_place.get().is_none());
        foreign_callback.store.extra[stage_range.start as usize + 1] = foreign_value as u32;
        super::super::prepare_tables(&foreign_callback.store).unwrap();
        let foreign_denial = FullVerifier::verify(&foreign_callback).expect_err("a map stage cannot borrow a callback value from another function");
        assert_eq!(foreign_denial.message, "full IR instruction belongs to another function");
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn generic_pipeline_reduce_by_executes_after_disposal_on_both_routes() {
        let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/generic-pipeline-reduce-by.xsh")).unwrap();
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("generic-pipeline-reduce-by.xsh", crate::loader::entry_source_from_text("generic-pipeline-reduce-by.xsh", source), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = crate::sema::check::Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test("generic_reduce_by", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
            assert!(matches!(&result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
            for (entry, message) in [("invalid_reduce_key", "reduce-by key must be Str, Int, or Bool"), ("invalid_reduce_mode", "reduce-by requires exactly one enabled reduction mode")] {
                let result = prepared.eval_test(entry, crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
                assert_ne!(result.output.status, 0, "{entry}, force_frames={force_frames}: {:?}", result.result);
                assert!(result.output.diagnostics.iter().any(|diagnostic| diagnostic.message.contains(message)), "{entry}, force_frames={force_frames}: {:?}", result.output.diagnostics);
            }
        }
    }

    #[test]
    fn generic_pipeline_reduce_by_authenticates_callback_and_configuration_scopes() {
        let program = prepare(r#"pure scoped(value, marker) { let first = [marker] |> reduce-by(min: true) { |item| { key: "bucket", value: item } }; [value] |> reduce-by(max: true) { |item| { key: "bucket", value: item } } }"#);
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "scoped").unwrap();
        let decoder = super::super::decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let slots = range.clone().filter_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::ListPipeline { mut stages, .. }) => match stages.next().unwrap().unwrap() {
                super::super::super::StageView::ReduceByConfigured { item_slot, .. } => Some(item_slot),
                _ => None,
            },
            _ => None,
        }).collect::<Vec<_>>();
        assert_eq!(slots.len(), 2);
        let read = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(slot)) if slot == slots[1])).unwrap();
        let mut escaped = program.clone();
        escaped.store.extra[program.store.data[read].range().start as usize] = slots[0] as u32;
        assert!(FullVerifier::verify(&escaped).is_err());
        let configured = prepare("pure configured(value, flag: Bool) { [value] |> reduce-by(min: flag) { |item| { key: \"bucket\", value: item } } }\n");
        let owner = configured.store.functions.iter().position(|function| configured.store.string(function.name).unwrap() == "configured").unwrap();
        let decoder = super::super::decoder_for_function(&configured.store, owner).unwrap();
        let range = configured.store.function_instruction_range(owner).unwrap();
        let item_slot = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::ListPipeline { mut stages, .. }) => match stages.next().unwrap().unwrap() {
                super::super::super::StageView::ReduceByConfigured { item_slot, .. } => Some(item_slot),
                _ => None,
            },
            _ => None,
        }).unwrap();
        let flag = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(1)))).unwrap();
        let mut escaped = configured.clone();
        escaped.store.extra[configured.store.data[flag].range().start as usize] = item_slot as u32;
        assert!(FullVerifier::verify(&escaped).is_err());
        for source in [
            r#"pure invalid(value) { [value] |> reduce-by(min: true) { |item| { key: "bucket" } } }"#,
            r#"pure invalid(value) { [value] |> reduce-by(min: true, max: true) { |item| { key: "bucket", value: item } } }"#,
            r#"pure invalid(value) { [value] |> reduce-by(min: true, jobs: 0) { |item| { key: "bucket", value: item } } }"#,
        ] {
            let mut sources = crate::source::SourceMap::default();
            let source_id = sources.add_file("invalid-reduce-by.xsh", source.to_owned());
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = crate::sema::check::Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
            assert!(!checked.diagnostics.is_empty(), "{source}");
        }
    }

    #[test]
    fn generic_pipeline_reduce_by_retains_actual_value_correlation() {
        prepare("pure reduced(value) { [value] |> reduce-by(min: true) { |item| { key: \"bucket\", value: item } } }\n");
        prepare("pure reduced(row) { [row] |> reduce-by(min: true) { |item| item } }\n");
    }

    #[test]
    fn typed_pipeline_reduce_by_converts_actual_integer_keys() {
        prepare("pure keyed(values: List[Int]) { values |> reduce-by(sum: true) { |item| { key: item, value: 1 } } }\n");
        prepare(r#"type Row = {key: Str, value: Int}
pure nominal(values: List[Int]) { values |> reduce-by(sum: true) { |item| Row(key: "bucket", value: item) } }"#);
    }

    #[test]
    fn generic_pipeline_adapters_reject_forged_sources_configuration_and_owners() {
        let program = prepare("pure chunks(marker, input: Bytes, size: Int, other: Bool) { let _ = marker; input |> bytes.chunks(size) }\npure foreign(marker, input: Bytes, size: Int, other: Bool) { let _ = marker; input |> bytes.chunks(size) }\n");
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "chunks").unwrap();
        let foreign = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "foreign").unwrap();
        let decoder = super::super::decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let input = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::ListPipeline { input, .. }) => Some(input.raw() as usize),
            _ => None,
        }).unwrap();
        let source_read = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(1)))).unwrap();
        let mut wrong_source = program.clone();
        wrong_source.store.extra[program.store.data[source_read].range().start as usize] = 3;
        assert!(FullVerifier::verify(&wrong_source).is_err());
        let size = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(2)))).unwrap();
        let mut wrong_size = program.clone();
        wrong_size.store.extra[program.store.data[size].range().start as usize] = 3;
        assert!(FullVerifier::verify(&wrong_size).is_err());
        let mut wrong_owner = program.clone();
        let fact = wrong_owner.store.scheme_body_facts.iter_mut().find(|fact| fact.function.index() == owner && fact.instruction as usize == input).unwrap();
        fact.function = super::super::super::IrFunctionId::new(foreign).unwrap();
        assert!(FullVerifier::verify(&wrong_owner).is_err());
    }

    #[test]
    fn generic_pipeline_table_rejects_forged_row_configuration_and_owner_facts() {
        let program = prepare("proc display(marker, row, columns: List[Str], wrong: Bool) [] -> Unit { let _ = marker; [row] |> table.print(columns: columns) }\nproc foreign(marker, row, columns: List[Str], wrong: Bool) [] -> Unit { let _ = marker; [row] |> table.print(columns: columns) }\n");
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "display").unwrap();
        let foreign = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "foreign").unwrap();
        let decoder = super::super::decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        for slot in [1, 2] {
            let read = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(actual)) if actual == slot)).unwrap();
            let mut wrong = program.clone();
            wrong.store.extra[program.store.data[read].range().start as usize] = 3;
            assert!(FullVerifier::verify(&wrong).is_err(), "forged slot {slot}");
        }
        let input = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::ListPipeline { input, .. }) => Some(input.raw() as usize),
            _ => None,
        }).unwrap();
        let mut wrong_owner = program.clone();
        let fact = wrong_owner.store.scheme_body_facts.iter_mut().find(|fact| fact.function.index() == owner && fact.instruction as usize == input).unwrap();
        fact.function = super::super::super::IrFunctionId::new(foreign).unwrap();
        assert!(FullVerifier::verify(&wrong_owner).is_err());
    }

    fn opaque_literal_list_source(typed: bool) -> String {
        let source = r#"type ColdRecord = {value: Int}
pure identity(value) { value }
pure observed(value: List[Any], spare: Bool, unknown: Any) -> List[List[Any]] { let _ = spare; let _ = unknown; identity([value]) }
pure text() -> Str { "wrong" }
pure erased(value: Any) -> Any { value }
pure flags(flag: Bool) -> List[Bool] { [flag] }
pure row() -> ColdRecord { {value: 1} }
let opaque: Any = 7
let nested: List[Any] = [opaque]
let kept = observed(nested, false, opaque)
"#;
        if typed { source.replace("pure identity(value)", "pure identity(value: List[List[Any]]) -> List[List[Any]]") } else { source.to_owned() }
    }

    #[test]
    fn typed_literal_list_arguments_preserve_nested_opaque_data() {
        prepare(&opaque_literal_list_source(true));
    }

    #[test]
    fn generic_literal_list_arguments_preserve_nested_opaque_data() {
        prepare(&opaque_literal_list_source(false));
    }

    #[test]
    fn literal_list_data_contexts_require_actual_members_roles_and_owners() {
        use super::super::super::{ExecutableOwner, FullCallableContract, FullCheckedExprPurpose, FullCheckedExprType, IrFunctionId};
        let program = prepare(&opaque_literal_list_source(false));
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "observed").unwrap();
        let owner_id = IrFunctionId::new(owner).unwrap();
        let context = program.store.checked_expr_types.iter().position(|fact| matches!(fact.purpose, FullCheckedExprPurpose::ProducerContext(ExecutableOwner::Function(function)) if function == owner_id)).unwrap();
        let instruction = program.store.checked_expr_types[context].instruction as usize;
        assert_eq!(program.store.tags[instruction], FullTag::ExprList);
        let return_type = |name| {
            let function = program.store.functions.iter().find(|function| program.store.string(function.name).unwrap() == name).unwrap();
            let FullCallableContract::Ground(signature) = function.contract else { panic!("the cold types are ordinary authored contracts") };
            program.store.semantic.signature_return_type(signature).unwrap()
        };
        for name in ["text", "erased", "flags"] {
            let mut wrong_type = program.clone();
            wrong_type.store.in_place.take();
            wrong_type.store.checked_expr_types[context].type_id = return_type(name);
            assert!(FullVerifier::verify(&wrong_type).is_err(), "forged context {name}");
        }
        let foreign = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "flags").unwrap();
        let mut wrong_owner = program.clone();
        wrong_owner.store.in_place.take();
        wrong_owner.store.checked_expr_types[context].purpose = FullCheckedExprPurpose::ProducerContext(ExecutableOwner::Function(IrFunctionId::new(foreign).unwrap()));
        assert!(FullVerifier::verify(&wrong_owner).is_err());
        let decoder = super::super::decoder_for_function(&program.store, owner).unwrap();
        let schema::ExprView::List(mut values) = decoder.expression_view(instruction).unwrap() else { unreachable!() };
        let child = values.next().unwrap().unwrap().raw() as usize;
        assert!(matches!(decoder.expression_view(child).unwrap(), schema::ExprView::Param(0)));
        let mut wrong_child = program.clone();
        wrong_child.store.in_place.take();
        wrong_child.store.extra[program.store.data[child].range().start as usize] = 1;
        assert!(FullVerifier::verify(&wrong_child).is_err());
        let opaque = program.store.function_instruction_range(owner).unwrap().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(2)))).unwrap();
        for ty in [program.store.checked_expr_types[context].type_id, return_type("row")] {
            let mut invented_shape = program.clone();
            invented_shape.store.in_place.take();
            let fact = FullCheckedExprType { instruction: opaque as u32, type_id: ty, purpose: FullCheckedExprPurpose::ProducerContext(ExecutableOwner::Function(owner_id)) };
            match invented_shape.store.checked_expr_types.binary_search_by_key(&(opaque as u32), |fact| fact.instruction) {
                Ok(index) => invented_shape.store.checked_expr_types[index] = fact,
                Err(index) => invented_shape.store.checked_expr_types.insert(index, fact),
            }
            assert!(FullVerifier::verify(&invented_shape).is_err());
        }
        let decoder = super::super::decoder_for_function(&program.store, foreign).unwrap();
        let unused = program.store.function_instruction_range(foreign).unwrap().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::List(_)))).unwrap();
        let mut unused_context = program.clone();
        unused_context.store.in_place.take();
        let fact = FullCheckedExprType { instruction: unused as u32, type_id: return_type("flags"), purpose: FullCheckedExprPurpose::ProducerContext(ExecutableOwner::Function(IrFunctionId::new(foreign).unwrap())) };
        match unused_context.store.checked_expr_types.binary_search_by_key(&(unused as u32), |fact| fact.instruction) {
            Ok(index) => unused_context.store.checked_expr_types[index] = fact,
            Err(index) => unused_context.store.checked_expr_types.insert(index, fact),
        }
        assert!(FullVerifier::verify(&unused_context).unwrap_err().message.contains("unused"));
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn generic_pipeline_aggregates_execute_after_disposal_on_both_routes() {
        let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/generic-pipeline-aggregates.xsh")).unwrap();
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("generic-pipeline-aggregates.xsh", crate::loader::entry_source_from_text("generic-pipeline-aggregates.xsh", source), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = crate::sema::check::Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test("generic_aggregates", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
            assert!(matches!(&result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
        }
    }

    #[test]
    fn generic_pipeline_aggregate_predicates_authenticate_domains_subjects_and_inputs() {
        use crate::sema::sealed_operation::{SealedOperationSignature, TypeDomain};
        use crate::sema::schemes::SealedOperationBinding;
        use super::super::super::schemes::FullSealedOperation;
        let program = prepare("pure selected(value, other) { let _ = other; [value] |> min() }\nlet kept = selected(7, true)\n");
        let (predicate, subject) = program.store.scheme_operations.iter().enumerate().find_map(|(index, operation)| match operation {
            FullSealedOperation::TypePredicate { domain: TypeDomain::MinMaxItem, subject } => Some((index, *subject)),
            _ => None,
        }).unwrap();
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "selected").unwrap();
        let super::super::super::FullCallableContract::Scheme(scheme) = program.store.functions[owner].contract else { panic!("selected is generic") };
        let other = program.store.scheme_params[program.store.schemes[scheme.index()].params.start as usize + 1].ty;
        assert_ne!(subject, other);
        let cold = || { let mut program = program.clone(); program.store.in_place.take(); program };
        let mut wrong_domain = cold();
        wrong_domain.store.scheme_operations[predicate] = FullSealedOperation::TypePredicate { domain: TypeDomain::CountKey, subject };
        for binding in &mut wrong_domain.store.call_operation_bindings {
            if matches!(binding, SealedOperationBinding::Selected(SealedOperationSignature::TypePredicate(TypeDomain::MinMaxItem))) {
                *binding = SealedOperationBinding::Selected(SealedOperationSignature::TypePredicate(TypeDomain::CountKey));
            }
        }
        FullVerifier::verify(&wrong_domain).expect_err("a min stage must retain its own canonical item domain");
        let mut wrong_subject = cold();
        wrong_subject.store.scheme_operations[predicate] = FullSealedOperation::TypePredicate { domain: TypeDomain::MinMaxItem, subject: other };
        FullVerifier::verify(&wrong_subject).expect_err("another valid binder cannot prove the actual min item");
        let decoder = super::super::decoder_for_function(&program.store, owner).unwrap();
        let input = program.store.function_instruction_range(owner).unwrap().find(|instruction|
            matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(0))))
            .expect("the staged list input reads its actual header item");
        let mut wrong_input = cold();
        wrong_input.store.extra[program.store.data[input].range().start as usize] = 1;
        FullVerifier::verify(&wrong_input).expect_err("stage result facts cannot replace the actual source item");
    }

    #[test]
    fn generic_pipeline_aggregate_domains_reject_outer_any_and_preserve_contract_text() {
        use crate::diagnostic::{DiagnosticCode, Severity};
        for stage in ["min", "max"] {
            let source = format!("pure selected(value) {{ [value] |> {stage}() }}\nlet unchecked: Any = 7\nlet kept = selected(unchecked)\n");
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = crate::sema::check::Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckDynamicBoundary)), "{source}\n{:?}", checked.diagnostics);
        }
        let source = "pure selected(values) { values |> min() }\n";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        let checked = crate::sema::check::Checker::check_arena(&parsed.arena, source);
        assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error), "{:?}", checked.diagnostics);
        let contract = checked.checked_declarations.functions.values().find(|function| function.name.as_str() == "selected").unwrap().callable_contract().unwrap();
        let shown = contract.to_string();
        assert!(shown.contains("MinMaxItem(") && shown.contains("Result["), "{shown}");
    }

    #[test]
    fn generic_pipeline_aggregate_null_item_preserves_closed_null() {
        prepare("pure minimum(values) { values |> min() }\nassert minimum([null])? == null\n");
    }

    #[test]
    fn typed_pipeline_aggregates_preserve_existing_item_domains() {
        prepare(r#"
pure total(values: List[Int]) -> Int { values |> sum() }
pure least(values: List[Result[Int, Str]]) -> Result[Result[Int, Str]] { values |> min() }
pure greatest(values: List[List[Int]]) -> Result[List[Int]] { values |> max() }
pure least_float(values: List[Float]) -> Result[Float] { values |> min() }
let opaque: Result[Int, Str] = Err("data")
assert total([1, 2, 3]) == 6
assert least([opaque]) == Ok(opaque)
assert greatest([[2], [1]]) == Ok([1])
assert least_float([9.0, 1.0]) == Ok(9.0)
"#);
    }

    #[test]
    fn inferred_pipeline_aggregates_preserve_item_and_result_relationships() {
        prepare(r#"
pure total(marker, values: List[Int]) -> Int { let _ = marker; values |> sum() }
pure least(value) { [value] |> min() }
pure greatest(value) { [value] |> max() }
let opaque: Result[Int, Str] = Err("data")
assert total(opaque, [1, 2, 3]) == 6
assert least(opaque) == Ok(opaque)
assert greatest([1]) == Ok([1])
"#);
    }

    #[test]
    fn generic_pipeline_callbacks_keep_stage_items_lexically_owned() {
        let program = prepare("pure mapped(value) { [value] |> map { |item| item } |> map { |item| item } }\n");
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "mapped").unwrap();
        let decoder = super::super::decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let slots = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::ListPipeline { mut stages, .. }) => Some(stages.by_ref().map(|stage| match stage.unwrap() { super::super::super::StageView::Map { slot, .. } => slot, _ => panic!("map callback") }).collect::<Vec<_>>()),
            _ => None,
        }).unwrap();
        assert_eq!(slots.len(), 2);
        let read = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(slot)) if slot == slots[1])).unwrap();
        let mut escaped = program.clone();
        escaped.store.extra[program.store.data[read].range().start as usize] = slots[0] as u32;
        assert!(FullVerifier::verify(&escaped).is_err());
        let program = prepare("pure mapped(value) { [value] |> map { |item| var local = item; local = item; local } }\n");
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "mapped").unwrap();
        let decoder = super::super::decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let item_slot = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::ListPipeline { mut stages, .. }) => match stages.next().unwrap().unwrap() { super::super::super::StageView::Map { slot, .. } => Some(slot), _ => panic!("map callback") },
            _ => None,
        }).unwrap();
        let assignment = range.clone().find(|instruction| matches!(decoder.statement_view(*instruction), Ok(schema::StmtView::Assign { .. }))).unwrap();
        let mut mutated_item = program.clone();
        mutated_item.store.extra[program.store.data[assignment].range().start as usize] = item_slot as u32;
        assert!(FullVerifier::verify(&mutated_item).is_err());
    }

    #[test]
    fn generic_pipeline_nullable_callbacks_retain_their_actual_item_proof() {
        let source = "pure optional_callback(value: Int?) -> Bool { value != null }\npure routed(value, other) { let action = optional_callback; let _ = other; [value] |> map(action) }\n";
        let program = prepare(source);
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "routed").unwrap();
        let super::super::super::FullCallableContract::Scheme(scheme) = program.store.functions[owner].contract else { panic!("nullable owner is quantified"); };
        let decoder = super::super::decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let item_slot = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::ListPipeline { mut stages, .. }) => match stages.next().unwrap().unwrap() {
                super::super::super::StageView::Map { slot, .. } => Some(slot),
                _ => None,
            },
            _ => None,
        }).unwrap();
        let read = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(slot)) if slot == item_slot)).unwrap();
        let mut wrong_binding = program.clone();
        wrong_binding.store.extra[program.store.data[read].range().start as usize] = 1;
        assert!(FullVerifier::verify(&wrong_binding).is_err());
        let list = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::List(_)))).unwrap();
        let schema::ExprView::List(mut values) = decoder.expression_view(list).unwrap() else { unreachable!() };
        let value = values.next().unwrap().unwrap().raw() as usize;
        let mut wrong_source = program.clone();
        wrong_source.store.extra[program.store.data[value].range().start as usize] = 1;
        assert!(FullVerifier::verify(&wrong_source).is_err());
        let mut missing_relation = program.clone();
        missing_relation.store.schemes[scheme.index()].applications.len = 0;
        assert!(FullVerifier::verify(&missing_relation).is_err());
        let source = format!("{source}let rejected = routed(\"wrong\", 1)\n");
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = crate::sema::check::Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error), "{:?}", checked.diagnostics);
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn generic_pipeline_callbacks_execute_after_disposal_on_both_routes() {
        let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/generic-pipeline-callbacks.xsh")).unwrap();
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("generic-pipeline-callbacks.xsh", crate::loader::entry_source_from_text("generic-pipeline-callbacks.xsh", source), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = crate::sema::check::Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test("generic_callbacks", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
            assert!(matches!(&result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
        }
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn generic_pipeline_folds_execute_after_disposal_on_both_routes() {
        let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/generic-pipeline-folds.xsh")).unwrap();
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("generic-pipeline-folds.xsh", crate::loader::entry_source_from_text("generic-pipeline-folds.xsh", source), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = crate::sema::check::Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test("generic_folds", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
            assert!(matches!(&result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
        }
    }

    #[test]
    fn generic_pipeline_folds_authenticate_accumulators_and_lexical_owners() {
        for source in [
            "pure folded(value) { [value] |> fold(0) { |acc, item| acc } }\n",
            "pure folded(value) { let first = [value] |> fold(value) { |acc, item| item }; [first] |> fold(first) { |acc, item| item } }\n",
        ] {
            let program = prepare(source);
            let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "folded").unwrap();
            let decoder = super::super::decoder_for_function(&program.store, owner).unwrap();
            let range = program.store.function_instruction_range(owner).unwrap();
            let slots = range.clone().filter_map(|instruction| match decoder.expression_view(instruction) {
                Ok(schema::ExprView::ListPipeline { mut stages, .. }) => match stages.next().unwrap().unwrap() {
                    super::super::super::StageView::Fold { acc_slot, item_slot, .. } => Some((acc_slot, item_slot)),
                    _ => None,
                },
                _ => None,
            }).collect::<Vec<_>>();
            let (original, replacement) = match slots.as_slice() {
                [(acc, item)] => (*acc, *item),
                [(_, first), (_, second)] => (*second, *first),
                _ => panic!("one or two fold stages"),
            };
            let read = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(slot)) if slot == original)).unwrap();
            let mut escaped = program.clone();
            escaped.store.extra[program.store.data[read].range().start as usize] = replacement as u32;
            assert!(FullVerifier::verify(&escaped).is_err());
        }
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn generic_pipeline_parallel_callbacks_execute_after_disposal_on_both_routes() {
        let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/generic-pipeline-parallel.xsh")).unwrap();
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("generic-pipeline-parallel.xsh", crate::loader::entry_source_from_text("generic-pipeline-parallel.xsh", source), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = crate::sema::check::Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test("generic_parallel", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
            assert!(matches!(&result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
        }
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn generic_pipeline_keys_execute_after_disposal_on_both_routes() {
        let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/generic-pipeline-keys.xsh")).unwrap();
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("generic-pipeline-keys.xsh", crate::loader::entry_source_from_text("generic-pipeline-keys.xsh", source), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = crate::sema::check::Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test("generic_keys", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
            assert!(matches!(&result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
        }
    }

    #[test]
    fn generic_parallel_and_key_stages_keep_their_callback_item_owners() {
        for stage in ["par-map(jobs: 1)", "group-by", "unique-by"] {
            let program = prepare(&format!("pure observed(value) {{ let first = [value] |> {stage} {{ |item| item }}; [value] |> {stage} {{ |item| item }} }}\n"));
            let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "observed").unwrap();
            let decoder = super::super::decoder_for_function(&program.store, owner).unwrap();
            let range = program.store.function_instruction_range(owner).unwrap();
            let slots = range.clone().filter_map(|instruction| match decoder.expression_view(instruction) {
                Ok(schema::ExprView::ListPipeline { mut stages, .. }) => match stages.next().unwrap().unwrap() {
                    super::super::super::StageView::ParMap { slot, .. }
                    | super::super::super::StageView::GroupBy { slot, .. }
                    | super::super::super::StageView::UniqueBy { slot, .. } => Some(slot),
                    _ => None,
                },
                _ => None,
            }).collect::<Vec<_>>();
            assert_eq!(slots.len(), 2);
            let read = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(slot)) if slot == slots[1])).unwrap();
            let mut escaped = program.clone();
            escaped.store.extra[program.store.data[read].range().start as usize] = slots[0] as u32;
            assert!(FullVerifier::verify(&escaped).is_err(), "{stage}");
        }
    }

    #[test]
    fn generic_pipeline_key_predicates_authenticate_actual_subjects_and_bindings() {
        use crate::sema::sealed_operation::{SealedOperationSignature, TypeDomain};
        use crate::sema::schemes::SealedOperationBinding;
        use super::super::super::schemes::FullSealedOperation;
        let program = prepare("pure ordered(value, marker) { let _ = [marker] |> count { |item| item }; [value] |> sort }\nlet rows = ordered(7, true)\n");
        let (sort, subject) = program.store.scheme_operations.iter().enumerate().find_map(|(index, operation)| match operation {
            FullSealedOperation::TypePredicate { domain: TypeDomain::SortKey, subject } => Some((index, *subject)),
            _ => None,
        }).unwrap();
        let other = program.store.scheme_operations.iter().find_map(|operation| match operation {
            FullSealedOperation::TypePredicate { domain: TypeDomain::CountKey, subject: other } if *other != subject => Some(*other),
            _ => None,
        }).unwrap();
        let mut wrong_domain = program.clone();
        wrong_domain.store.scheme_operations[sort] = FullSealedOperation::TypePredicate { domain: TypeDomain::CountKey, subject };
        assert!(FullVerifier::verify(&wrong_domain).is_err());
        let mut wrong_subject = program.clone();
        wrong_subject.store.scheme_operations[sort] = FullSealedOperation::TypePredicate { domain: TypeDomain::SortKey, subject: other };
        assert!(FullVerifier::verify(&wrong_subject).is_err());
        let mut wrong_binding = program.clone();
        let binding = wrong_binding.store.call_operation_bindings.iter_mut().find(|binding| matches!(binding, SealedOperationBinding::Selected(SealedOperationSignature::TypePredicate(TypeDomain::SortKey)))).unwrap();
        *binding = SealedOperationBinding::Selected(SealedOperationSignature::TypePredicate(TypeDomain::CountKey));
        assert!(FullVerifier::verify(&wrong_binding).is_err());
    }

    #[test]
    fn generic_pipeline_observers_keep_unit_consumption_and_item_scopes() {
        let program = prepare("pure observed(value) { let first = [value] |> tee { |item| let _ = item }; [value] |> tee { |item| let _ = item } }\n");
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "observed").unwrap();
        let decoder = super::super::decoder_for_function(&program.store, owner).unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let slots = range.clone().filter_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::ListPipeline { mut stages, .. }) => match stages.next().unwrap().unwrap() {
                super::super::super::StageView::Tee { slot, .. } => Some(slot),
                _ => None,
            },
            _ => None,
        }).collect::<Vec<_>>();
        assert_eq!(slots.len(), 2);
        let read = range.clone().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(slot)) if slot == slots[1])).unwrap();
        let mut escaped = program.clone();
        escaped.store.extra[program.store.data[read].range().start as usize] = slots[0] as u32;
        assert!(FullVerifier::verify(&escaped).is_err());
        for stage in ["each", "tee"] {
            for callback in ["proc callback(value: Int) -> Int { value }", "proc callback(value: Int) -> Result[Int, Str] { Ok(value) }"] {
                let source = format!("{callback}\nproc rejected(value) {{ [value] |> {stage}(callback) }}\n");
                let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), &source);
                assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
                let checked = crate::sema::check::Checker::check_arena(&parsed.arena, &source);
                assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error), "{source}\n{:?}", checked.diagnostics);
            }
        }
        for (stage, result) in [("each", "Unit"), ("tee", "List[Int]")] {
            for declaration in ["proc rejected(value: Int) []".to_owned(), format!("pure rejected(value: Int) -> {result}")] {
                let source = format!("pure fallible(value: Int) -> Result[Unit, Str] {{ Err(\"failure\") }}\n{declaration} {{ [value] |> {stage}(fallible) }}\n");
                let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), &source);
                assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
                let checked = crate::sema::check::Checker::check_arena(&parsed.arena, &source);
                assert!(checked.diagnostics.iter().any(|diagnostic| matches!(diagnostic.code, Some(crate::diagnostic::DiagnosticCode::CheckEffectViolation | crate::diagnostic::DiagnosticCode::CheckTryContext))), "{source}\n{:?}", checked.diagnostics);
            }
        }
        let program = prepare("proc observe(value) -> Result[Unit, Str] { Ok() }\nproc typed(value: Str, other: Int) { let _ = other; [value] |> tee(observe) }\n");
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "typed").unwrap();
        let decoder = super::super::decoder_for_function(&program.store, owner).unwrap();
        let call = program.store.function_instruction_range(owner).unwrap().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Call { .. }))).unwrap();
        let instance = program.store.call_instantiations.binary_search_by_key(&(call as u32), |instance| instance.instruction).unwrap();
        let mut missing_instance = program.clone();
        missing_instance.store.call_instantiations.remove(instance);
        assert!(FullVerifier::verify(&missing_instance).is_err());
        let list = program.store.function_instruction_range(owner).unwrap().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::List(_)))).unwrap();
        let schema::ExprView::List(mut values) = decoder.expression_view(list).unwrap() else { unreachable!() };
        let read = values.next().unwrap().unwrap().raw() as usize;
        assert!(matches!(decoder.expression_view(read), Ok(schema::ExprView::Param(0))));
        let mut wrong_typed_source = program.clone();
        wrong_typed_source.store.extra[program.store.data[read].range().start as usize] = 1;
        assert!(FullVerifier::verify(&wrong_typed_source).is_err());
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn generic_pipeline_observers_execute_after_disposal_on_both_routes() {
        let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/generic-pipeline-observers.xsh")).unwrap();
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("generic-pipeline-observers.xsh", crate::loader::entry_source_from_text("generic-pipeline-observers.xsh", source), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = crate::sema::check::Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test("generic_observers", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
            assert!(matches!(&result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
        }
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn generic_pipeline_shapes_execute_after_disposal_on_both_routes() {
        let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/generic-pipeline-shapes.xsh")).unwrap();
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("generic-pipeline-shapes.xsh", crate::loader::entry_source_from_text("generic-pipeline-shapes.xsh", source), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = crate::sema::check::Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test("generic_shapes", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
            assert!(matches!(&result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
        }
    }

    #[test]
    fn generic_pipeline_shapes_authenticate_configuration_scopes_and_argv_modes() {
        let program = prepare("pure batched(value) { [value] |> batch(count: 1, max_argv: false) }\n");
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "batched").unwrap();
        let decoder = super::super::decoder_for_function(&program.store, owner).unwrap();
        let flag = program.store.function_instruction_range(owner).unwrap().find(|instruction| matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Bool(false)))).unwrap();
        let mut enabled = program.clone();
        enabled.store.extra[program.store.data[flag].range().start as usize] = 1;
        assert!(FullVerifier::verify(&enabled).is_err());
        let program = prepare("pure ranged(value) { let first = [value] |> range(0, 2); [value] |> range(3, 5) }\n");
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "ranged").unwrap();
        let decoder = super::super::decoder_for_function(&program.store, owner).unwrap();
        let ends = program.store.function_instruction_range(owner).unwrap().filter_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::ListPipeline { mut stages, .. }) => match stages.next().unwrap().unwrap() {
                super::super::super::StageView::Range { end, .. } => Some(end.raw() as usize),
                _ => None,
            },
            _ => None,
        }).collect::<Vec<_>>();
        assert_eq!(ends.len(), 2);
        let schema::ExprView::Param(first) = decoder.expression_view(ends[0]).unwrap() else { panic!("range end retains its configured slot"); };
        let mut escaped = program.clone();
        escaped.store.extra[program.store.data[ends[1]].range().start as usize] = first as u32;
        assert!(FullVerifier::verify(&escaped).is_err());
    }

    #[test]
    fn generic_pipeline_shapes_reject_wrong_domains_and_retain_argv_bounds() {
        use crate::sema::check::Checker;
        use crate::syntax::parser::Parser;
        for body in [
            "[value] |> batch(count: \"wrong\")",
            "[value] |> batch(max_argv: 1)",
            "[value] |> zip(\"wrong\")",
            "[value] |> repeat(\"wrong\")",
            "[value] |> range(\"wrong\", 1)",
            "[value] |> shuffle(seed: true)",
        ] {
            let source = format!("pure rejected(value) {{ {body} }}\n");
            let parsed = Parser::parse_source_arena_only(crate::source::SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error), "{source}\n{:?}", checked.diagnostics);
        }
        for (ty, valid) in [("Str", true), ("Int", true), ("UInt", true), ("Bool", true), ("Path", true), ("Duration", true), ("Words", true), ("Bytes", false), ("Any", false), ("Result[Int, Str]", false), ("List[Int]", false), ("FsLock", false), ("BadWords", false)] {
            let source = format!("type Words = Union[Str, Int]\ntype BadWords = Union[Str, Bytes]\npure bounded(value) {{ [value] |> batch(max_bytes: 32) }}\npure observed(value: {ty}) {{ bounded(value) }}\n");
            let parsed = Parser::parse_source_arena_only(crate::source::SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert_eq!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error), valid, "{source}\n{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn generic_structured_pipeline_authenticates_its_item_and_configuration() {
        let program = prepare("pure keep(value) { [value, value] |> drop(1) |> enumerate }\npure closed(value) { let rows = [1, 2] |> enumerate; {value, rows} }\n");
        let owner = program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == "keep").unwrap();
        let range = program.store.function_instruction_range(owner).unwrap();
        let input = range.clone().find(|instruction| program.store.tags[*instruction] == FullTag::ExprList).unwrap();
        let count = range.clone().find(|instruction| program.store.tags[*instruction] == FullTag::ExprInt).unwrap();
        let fact = |instruction| program.store.scheme_body_facts.iter().position(|fact| fact.instruction as usize == instruction && fact.kind() == FullSchemeBodyFactKind::Expression).unwrap();
        let mut wrong_input = program.clone();
        wrong_input.store.scheme_body_facts[fact(input)].value = program.store.scheme_body_facts[fact(count)].value;
        assert!(FullVerifier::verify(&wrong_input).is_err());
        let mut wrong_count = program.clone();
        wrong_count.store.scheme_body_facts[fact(count)].value = program.store.scheme_body_facts[fact(input)].value;
        assert!(FullVerifier::verify(&wrong_count).is_err());
        let pipeline = range.clone().find(|instruction| program.store.tags[*instruction] == FullTag::ExprPipeline).unwrap();
        let mut wrong_output = program.clone();
        wrong_output.store.scheme_body_facts[fact(pipeline)].value = program.store.scheme_body_facts[fact(count)].value;
        assert!(FullVerifier::verify(&wrong_output).is_err());
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn generic_structured_pipeline_executes_after_disposal_on_both_routes() {
        let source = "var configured_count = 0\nvar cleaned_count = 0\nstream values(value) { defer { cleaned_count += 1 }; yield value; yield value }\nproc consume(value) { values(value) |> take(1) }\nproc next_count() -> Int { configured_count += 1; 1 }\nproc bad_count() -> Result[Int, Str] { Err(\"bad count\") }\npure keep(value) { [value, value] |> drop(1) |> enumerate }\npure first_value(value) { [value] |> first }\npure last_value(value) { [value, value] |> last }\npure collect_values(value) { [value] |> collect }\npure count_values(value) { [value, value] |> count }\nproc attach(value) { let rows = [1, 2, 3] |> drop(1) |> take(1); {value, rows} }\nproc configured(value) { [value, value] |> drop(next_count()) }\nproc failing(value) { [value] |> drop(bad_count()?) }\ntest generic_pipeline { let integer = keep(7); assert integer[0].index == 0; assert integer[0].value == 7; let text = keep(\"kept\"); assert text[0].value == \"kept\"; let row = attach(7)?; assert row.value == 7; assert row.rows == [2]; assert first_value(7)? == 7; assert last_value(\"kept\")? == \"kept\"; assert collect_values(7) == [7]; assert count_values(\"kept\") == 2; assert configured(7)? == [7]; assert configured_count == 1; assert configured(\"kept\")? == [\"kept\"]; assert configured_count == 2; assert consume(7)? == [7]; assert cleaned_count == 1; assert consume(\"kept\")? == [\"kept\"]; assert cleaned_count == 2; match failing(7) { Err(error) => assert error == \"bad count\"; Ok(_) => assert false } }\n";
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("generic-structured-pipeline.xsh", crate::loader::entry_source_from_text("generic-structured-pipeline.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = crate::sema::check::Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test("generic_pipeline", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
            assert!(matches!(&result.result, Some(crate::runtime::value::Value::Unit))
                || matches!(&result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
        }
    }
}
