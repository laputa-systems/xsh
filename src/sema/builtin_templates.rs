use super::constraints::{ConstraintConflict, TypeConstraints};
use super::types::Type;
use crate::modules::signature::ModuleFnSig;
use crate::source::Span;
use std::collections::BTreeMap;
use xsh_registry::types::BuiltinTypeParameter;

/// A builtin occurrence owns fresh substitutions. Receiver facts anchor its
/// parameters before arguments are checked; dynamic Any stays an erased domain.
pub(crate) struct BuiltinInstantiation {
    pub signature: ModuleFnSig,
}

impl BuiltinInstantiation {
    pub fn new(
        signature: &ModuleFnSig,
        receiver_template: Option<&Type>,
        receiver: Option<&Type>,
        constraints: &mut TypeConstraints,
        span: Span,
    ) -> Result<Self, ConstraintConflict> {
        let mut parameters = BTreeMap::new();
        if let (Some(template), Some(actual)) = (receiver_template, receiver) {
            seed_receiver_domains(template, actual, &mut parameters);
        }
        if let Some(receiver) = receiver {
            parameters.insert(BuiltinTypeParameter::Receiver, receiver.clone());
        }
        let mut signature = signature.clone();
        for parameter in &mut signature.params {
            parameter.ty = instantiate_type(&parameter.ty, &mut parameters, constraints, span);
        }
        signature.return_ty =
            instantiate_type(&signature.return_ty, &mut parameters, constraints, span);
        if let (Some(template), Some(actual)) = (receiver_template, receiver) {
            let template = instantiate_type(template, &mut parameters, constraints, span);
            constraints.constrain(&template, actual, span)?;
        }
        let mut instance = Self { signature };
        instance.resolve(constraints);
        Ok(instance)
    }

    pub fn constrain_result(
        &mut self,
        expected: &Type,
        constraints: &mut TypeConstraints,
        span: Span,
    ) -> Result<(), ConstraintConflict> {
        if matches!(expected, Type::Unit | Type::Unknown | Type::Invalid) {
            return Ok(());
        }
        // A Result-returning function supplies its success expectation to its
        // tail. Preserve a builtin's Result envelope while solving that payload.
        let result = if !expected.is_result() {
            self.signature
                .return_ty
                .result_ok()
                .unwrap_or(&self.signature.return_ty)
        } else {
            &self.signature.return_ty
        };
        if expected.contains_inference() {
            constraints.constrain(expected, result, span)?;
        } else {
            constraints.constrain_context(expected, result, span)?;
        }
        self.resolve(constraints);
        Ok(())
    }

    pub fn invalid_map_key(&self) -> Option<Type> {
        let mut pending = vec![&self.signature.return_ty];
        pending.extend(self.signature.params.iter().map(|parameter| &parameter.ty));
        while let Some(ty) = pending.pop() {
            match ty {
                Type::Map(key, value) => {
                    if !key.is_map_key()
                        && !matches!(
                            key.as_ref(),
                            Type::Inference(_) | Type::Unknown | Type::Invalid
                        )
                    {
                        return Some((**key).clone());
                    }
                    pending.push(value);
                }
                Type::List(inner)
                | Type::Stream(inner)
                | Type::Optional(inner)
                | Type::Set(inner) => pending.push(inner),
                Type::Result(ok, error) => {
                    pending.push(ok);
                    pending.push(error);
                }
                Type::Record(fields) => pending.extend(fields.values()),
                _ => {}
            }
        }
        None
    }

    pub fn resolve(&mut self, constraints: &TypeConstraints) {
        for parameter in &mut self.signature.params {
            if let Ok(resolved) = constraints.resolve(&parameter.ty) {
                parameter.ty = resolved;
            }
        }
        if let Ok(resolved) = constraints.resolve(&self.signature.return_ty) {
            self.signature.return_ty = resolved;
        }
    }
}

fn instantiate_type(
    ty: &Type,
    parameters: &mut BTreeMap<BuiltinTypeParameter, Type>,
    constraints: &mut TypeConstraints,
    span: Span,
) -> Type {
    match ty {
        Type::BuiltinParameter(parameter) => parameters
            .entry(*parameter)
            .or_insert_with(|| constraints.fresh(span))
            .clone(),
        Type::List(inner) => Type::List(Box::new(instantiate_type(
            inner,
            parameters,
            constraints,
            span,
        ))),
        Type::Map(key, value) => Type::Map(
            Box::new(instantiate_type(key, parameters, constraints, span)),
            Box::new(instantiate_type(value, parameters, constraints, span)),
        ),
        Type::Stream(inner) => Type::Stream(Box::new(instantiate_type(
            inner,
            parameters,
            constraints,
            span,
        ))),
        Type::Set(inner) => Type::Set(Box::new(instantiate_type(
            inner,
            parameters,
            constraints,
            span,
        ))),
        Type::Optional(inner) => Type::Optional(Box::new(instantiate_type(
            inner,
            parameters,
            constraints,
            span,
        ))),
        Type::Result(ok, error) => Type::Result(
            Box::new(instantiate_type(ok, parameters, constraints, span)),
            Box::new(instantiate_type(error, parameters, constraints, span)),
        ),
        Type::Record(fields) => Type::Record(
            fields
                .iter()
                .map(|(name, ty)| (*name, instantiate_type(ty, parameters, constraints, span)))
                .collect::<BTreeMap<_, _>>()
                .into(),
        ),
        Type::Validated(validated) => Type::Validated(Box::new(
            validated.map_base(|base| instantiate_type(base, parameters, constraints, span)),
        )),
        ty => ty.clone(),
    }
}

/// Checked receiver domains are authoritative even when they are empty records
/// or Null. Reuse local inference identities; allocate no new receiver holes.
fn seed_receiver_domains(
    template: &Type,
    actual: &Type,
    parameters: &mut BTreeMap<BuiltinTypeParameter, Type>,
) {
    match (template, actual) {
        (Type::BuiltinParameter(parameter), actual) => {
            parameters.insert(*parameter, actual.clone());
        }
        (template, actual @ (Type::Any | Type::Unknown | Type::Invalid)) => {
            seed_fixed_parameters(template, actual, parameters)
        }
        (Type::List(left), Type::List(right))
        | (Type::Stream(left), Type::Stream(right))
        | (Type::Set(left), Type::Set(right))
        | (Type::Optional(left), Type::Optional(right)) => {
            seed_receiver_domains(left, right, parameters)
        }
        (Type::Map(left_key, left_value), Type::Map(right_key, right_value)) => {
            seed_receiver_domains(left_key, right_key, parameters);
            seed_receiver_domains(left_value, right_value, parameters);
        }
        (Type::Result(left_ok, left_error), Type::Result(right_ok, right_error)) => {
            seed_receiver_domains(left_ok, right_ok, parameters);
            seed_receiver_domains(left_error, right_error, parameters);
        }
        (Type::Validated(left), Type::Validated(right)) => {
            seed_receiver_domains(left.base(), right.base(), parameters)
        }
        _ => {}
    }
}

fn seed_fixed_parameters(
    template: &Type,
    domain: &Type,
    parameters: &mut BTreeMap<BuiltinTypeParameter, Type>,
) {
    match template {
        Type::BuiltinParameter(parameter) => {
            parameters.insert(*parameter, domain.clone());
        }
        Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) | Type::Set(inner) => {
            seed_fixed_parameters(inner, domain, parameters)
        }
        Type::Map(key, value) | Type::Result(key, value) => {
            seed_fixed_parameters(key, domain, parameters);
            seed_fixed_parameters(value, domain, parameters);
        }
        Type::Record(fields) => {
            for field in fields.values() {
                seed_fixed_parameters(field, domain, parameters);
            }
        }
        Type::Validated(validated) => seed_fixed_parameters(validated.base(), domain, parameters),
        _ => {}
    }
}

/// Materializes a registered method signature from its exact receiver type.
/// Argument-only parameters remain unresolved, so callers cannot mistake an
/// unknown generic result for a checked return type.
pub(crate) fn concrete_method_signature(
    method: &crate::modules::signature::MethodSig,
    receiver: &Type,
) -> Option<ModuleFnSig> {
    let mut constraints = TypeConstraints::default();
    let mut instance = BuiltinInstantiation::new(
        &method.sig,
        method.receiver_ty.as_ref(),
        Some(receiver),
        &mut constraints,
        Span::at(crate::source::SourceId::new(0), 0),
    )
    .ok()?;
    instance.resolve(&constraints);
    (!instance.signature.return_ty.contains_inference()).then_some(instance.signature)
}

pub(crate) fn callable_parameters(signature: &ModuleFnSig) -> Vec<super::types::CallableParamType> {
    signature
        .params
        .iter()
        .map(|parameter| super::types::CallableParamType {
            name: crate::symbol::Name::intern(parameter.name),
            ty: parameter.ty.clone(),
            defaulted: parameter.defaulted,
            rest: false,
        })
        .collect()
}

/// Projects declared receiver applications through the selected signature's
/// parameter relationships. Concrete record shapes never supply application identity.
pub(crate) fn parameter_schema_contexts(
    signature: &ModuleFnSig,
    receiver_template: Option<&Type>,
    receiver_context: Option<&super::constants::SchemaExpectation>,
) -> Vec<Option<super::constants::SchemaExpectation>> {
    use super::constants::{SchemaComponent, SchemaExpectation};
    fn children(ty: &Type) -> Vec<(SchemaComponent, &Type)> {
        match ty.unvalidated() {
            Type::List(inner) | Type::Stream(inner) | Type::Set(inner) => {
                vec![(SchemaComponent::Item, inner)]
            }
            Type::Optional(inner) => vec![(SchemaComponent::Optional, inner)],
            Type::Map(key, value) => {
                vec![(SchemaComponent::Key, key), (SchemaComponent::Value, value)]
            }
            Type::Result(ok, error) => vec![
                (SchemaComponent::Success, ok),
                (SchemaComponent::Error, error),
            ],
            Type::Record(fields) => fields
                .iter()
                .map(|(name, ty)| (SchemaComponent::Field(*name), ty))
                .collect(),
            _ => Vec::new(),
        }
    }
    fn collect(
        ty: &Type,
        context: &SchemaExpectation,
        parameters: &mut BTreeMap<BuiltinTypeParameter, SchemaExpectation>,
    ) {
        if let Type::BuiltinParameter(parameter) = ty {
            parameters.insert(*parameter, context.clone());
        } else {
            for (component, child) in children(ty) {
                if let Some(context) = context.children.get(&component) {
                    collect(child, context, parameters);
                }
            }
        }
    }
    fn project(
        ty: &Type,
        parameters: &BTreeMap<BuiltinTypeParameter, SchemaExpectation>,
    ) -> Option<SchemaExpectation> {
        if let Type::BuiltinParameter(parameter) = ty {
            return parameters.get(parameter).cloned();
        }
        if matches!(
            ty,
            Type::Any | Type::Unknown | Type::Invalid | Type::Inference(_)
        ) {
            return None;
        }
        let mut result = SchemaExpectation::default();
        for (component, child) in children(ty) {
            if let Some(context) = project(child, parameters) {
                result.insert_child(component, context);
            }
        }
        Some(result)
    }
    let mut parameters = BTreeMap::new();
    if let Some(context) = receiver_context {
        parameters.insert(BuiltinTypeParameter::Receiver, context.clone());
        if let Some(template) = receiver_template {
            collect(template, context, &mut parameters);
        }
    }
    signature
        .params
        .iter()
        .map(|parameter| project(&parameter.ty, &parameters))
        .collect()
}

pub(crate) fn registered_uint_map_key(receiver: Option<&Type>) -> bool {
    matches!(receiver, Some(Type::Map(key, _)) if **key == Type::UInt)
}

pub(crate) fn registered_argument_check(receiver: Option<&Type>, slot: usize, parameter: &Type) -> Option<super::schemes::RegisteredArgumentCheck> {
    use super::schemes::RegisteredArgumentCheck;
    if slot == 0 && registered_uint_map_key(receiver) {
        Some(RegisteredArgumentCheck::UIntMapKey)
    } else if parameter.has_unsigned_constraint() {
        Some(RegisteredArgumentCheck::Unsigned(parameter.clone()))
    } else { None }
}

pub(crate) fn registered_call_is_executable(plan: &super::schemes::RegisteredCallPlan) -> bool {
    use crate::modules::signature::{ApiArgCheck, ImplBinding, MethodReceiver, SemanticRule};
    let signature = plan.signature;
    if plan.argument_checks.len() != signature.params.len() || plan.argument_checks.iter().flatten().any(|check| matches!(check, super::schemes::RegisteredArgumentCheck::Unsigned(ty) if ty.contains_inference() || ty.contains_recovery())) { return false; }
    if !signature.pure || !signature.effects.is_empty()
        || !matches!(signature.binding, ImplBinding::Native)
        || signature.arg_check != ApiArgCheck::Standard
        || signature.semantic_rule != SemanticRule::Standard
    { return false; }
    let slots = plan.argument_slots.iter().copied().collect::<std::collections::BTreeSet<_>>();
    if slots.len() != plan.argument_slots.len()
        || slots.iter().any(|slot| *slot >= signature.params.len())
        || signature.params.iter().enumerate().any(|(slot, parameter)| !parameter.defaulted && !slots.contains(&slot))
        || plan.receiver.is_some() && slots.iter().copied().ne(0..slots.len())
    { return false; }
    match plan.receiver {
        Some(domain) => matches!(domain, MethodReceiver::Result | MethodReceiver::Path | MethodReceiver::Int | MethodReceiver::Float | MethodReceiver::List | MethodReceiver::Map | MethodReceiver::Record | MethodReceiver::Str | MethodReceiver::Bytes | MethodReceiver::Status | MethodReceiver::Digest | MethodReceiver::Regex | MethodReceiver::NonEmpty | MethodReceiver::RelPath | MethodReceiver::Set)
            && crate::modules::api_spec().method_overloads(domain, &plan.name).is_some_and(|methods| methods.iter().any(|method| std::ptr::eq(&method.sig, signature))),
        None => crate::modules::api_spec().module_entries().any(|(_, module)| module.functions.iter().any(|function| function.name == plan.name.as_ref() && function.overloads.iter().any(|candidate| std::ptr::eq(candidate, signature)))),
    }
}

pub(crate) fn verify_registered_call(
    plan: &super::schemes::RegisteredCallPlan,
    receiver: Option<&super::schemes::SchemeType>,
    arguments: &[Option<&super::schemes::SchemeType>],
    result: &super::schemes::SchemeType,
) -> bool {
    use super::schemes::SchemeType;
    use crate::modules::{api_spec, MethodReceiver};
    let signature = plan.signature;
    if !registered_call_is_executable(plan) || arguments.len() > signature.params.len() { return false; }
    let mut parameters = BTreeMap::new();
    match (plan.receiver, receiver) {
        (Some(domain), Some(actual)) => {
            let Some(method) = api_spec().method_overloads(domain, &plan.name)
                .and_then(|methods| methods.iter().find(|method| std::ptr::eq(&method.sig, signature)))
            else { return false };
            parameters.insert(BuiltinTypeParameter::Receiver, actual.clone());
            if let Some(template) = &method.receiver_ty {
                if !bind_symbolic_template(template, actual, &mut parameters, 0) { return false; }
            } else {
                let valid = match domain {
                    MethodReceiver::Record => matches!(actual, SchemeType::Record { .. }) || matches!(actual.closed_ground_type(), Some(Type::Record(_))),
                    MethodReceiver::Int => matches!(actual.closed_ground_type().as_ref().map(Type::unvalidated), Some(Type::Int | Type::UInt)),
                    _ => false,
                };
                if !valid { return false; }
            }
        }
        (None, None) => {
            if !api_spec().module_entries().any(|(_, module)| module.functions.iter().any(|function| function.name == plan.name.as_ref() && function.overloads.iter().any(|candidate| std::ptr::eq(candidate, signature)))) { return false; }
        }
        _ => return false,
    }
    for (index, parameter) in signature.params.iter().enumerate() {
        match arguments.get(index).copied().flatten() {
            Some(actual) => {
                if !bind_symbolic_template(&parameter.ty, actual, &mut parameters, 0) { return false; }
            }
            None if parameter.defaulted => {}
            None => return false,
        }
    }
    if !bind_symbolic_template(&signature.return_ty, result, &mut parameters, 0) { return false; }
    let Some(expected) = symbolic_template_type(&signature.return_ty, &parameters, 0) else { return false };
    if result != &expected && !matches!((result.closed_ground_type(), expected.closed_ground_type()), (Some(actual), Some(expected)) if actual == expected) { return false; }
    let receiver = receiver.and_then(SchemeType::closed_ground_type);
    signature.params.iter().enumerate().all(|(slot, parameter)| {
        let Some(expected) = symbolic_template_type(&parameter.ty, &parameters, 0) else { return false };
        let check = expected.closed_ground_type().and_then(|expected| registered_argument_check(receiver.as_ref(), slot, &expected));
        if plan.argument_checks.get(slot) != Some(&check) { return false; }
        if let (Some(check), Some(actual)) = (check, arguments.get(slot).copied().flatten()) {
            let expected = match check {
                super::schemes::RegisteredArgumentCheck::Unsigned(expected) => expected,
                super::schemes::RegisteredArgumentCheck::UIntMapKey => Type::UInt,
            };
            if actual.closed_ground_type() != Some(expected) { return false; }
        }
        true
    })
}

fn symbolic_type_matches(actual: &super::schemes::SchemeType, expected: &super::schemes::SchemeType) -> bool {
    if actual == expected { return true; }
    match (actual.closed_ground_type(), expected.closed_ground_type()) {
        (Some(actual), Some(expected)) => actual.matches_expected(&expected),
        _ => false,
    }
}

fn bind_symbolic_template(
    template: &Type,
    actual: &super::schemes::SchemeType,
    parameters: &mut BTreeMap<BuiltinTypeParameter, super::schemes::SchemeType>,
    depth: usize,
) -> bool {
    use super::schemes::SchemeType as S;
    if depth > super::constraints::MAX_TYPE_DEPTH { return false; }
    if let Type::BuiltinParameter(parameter) = template {
        return if let Some(expected) = parameters.get(parameter) { symbolic_type_matches(actual, expected) }
            else { parameters.insert(*parameter, actual.clone()); true };
    }
    match (template, actual) {
        (Type::List(left), S::List(right)) | (Type::Set(left), S::Set(right))
        | (Type::Stream(left), S::Stream(right)) | (Type::Optional(left), S::Optional(right)) => bind_symbolic_template(left, right, parameters, depth + 1),
        (Type::Map(left_key, left_value), S::Map(right_key, right_value))
        | (Type::Result(left_key, left_value), S::Result(right_key, right_value)) => bind_symbolic_template(left_key, right_key, parameters, depth + 1) && bind_symbolic_template(left_value, right_value, parameters, depth + 1),
        (Type::Validated(left), S::Validated(validation, right)) if left.validation() == *validation => bind_symbolic_template(left.base(), right, parameters, depth + 1),
        _ => symbolic_template_type(template, parameters, depth + 1).is_some_and(|expected| symbolic_type_matches(actual, &expected)),
    }
}

fn symbolic_template_type(
    template: &Type,
    parameters: &BTreeMap<BuiltinTypeParameter, super::schemes::SchemeType>,
    depth: usize,
) -> Option<super::schemes::SchemeType> {
    use super::schemes::{SchemeRowTail, SchemeType as S};
    if depth > super::constraints::MAX_TYPE_DEPTH { return None; }
    let child = |ty: &Type| symbolic_template_type(ty, parameters, depth + 1).map(Box::new);
    Some(match template {
        Type::BuiltinParameter(parameter) => parameters.get(parameter)?.clone(),
        Type::List(inner) => S::List(child(inner)?),
        Type::Set(inner) => S::Set(child(inner)?),
        Type::Stream(inner) => S::Stream(child(inner)?),
        Type::Optional(inner) => S::Optional(child(inner)?),
        Type::Map(key, value) => S::Map(child(key)?, child(value)?),
        Type::Result(ok, error) => S::Result(child(ok)?, child(error)?),
        Type::Validated(validated) => S::Validated(validated.validation(), child(validated.base())?),
        Type::Record(fields) => S::Record { fields: fields.iter().map(|(&name, ty)| Some((name, *child(ty)?))).collect::<Option<_>>()?, tail: SchemeRowTail::Closed },
        Type::Any | Type::Unknown | Type::Invalid | Type::Inference(_) => return None,
        _ => S::Ground(template.clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::signature::{MethodReceiver, api_spec};
    use crate::source::SourceId;

    fn span() -> Span {
        Span::at(SourceId::new(0), 0)
    }

    #[test]
    fn symbolic_registered_plans_preserve_fixed_template_relationships() {
        use crate::sema::schemes::{RegisteredCallPlan, SchemeType, SchemeTypeParameterId};
        let method = &api_spec().method_overloads(MethodReceiver::List, "push").unwrap()[0];
        let plan = RegisteredCallPlan {
            receiver: Some(MethodReceiver::List), name: "push".into(),
            signature: &method.sig, argument_slots: std::sync::Arc::from([0]), argument_checks: std::sync::Arc::from([None]),
        };
        let item = SchemeType::Parameter(SchemeTypeParameterId(0));
        let other = SchemeType::Parameter(SchemeTypeParameterId(1));
        let list = SchemeType::List(Box::new(item.clone()));
        assert!(verify_registered_call(&plan, Some(&list), &[Some(&item)], &list));
        assert!(!verify_registered_call(&plan, Some(&list), &[Some(&other)], &list));
        assert!(!verify_registered_call(&plan, Some(&list), &[Some(&item)], &SchemeType::List(Box::new(other))));
        let mut validated = plan.clone();
        validated.receiver = Some(MethodReceiver::NonEmpty);
        validated.signature = &api_spec().method_overloads(MethodReceiver::NonEmpty, "push").unwrap()[0].sig;
        assert!(!verify_registered_call(&validated, Some(&list), &[Some(&item)], &list));
        let mut layout = plan.clone();
        layout.argument_slots = std::sync::Arc::from([1]);
        assert!(!registered_call_is_executable(&layout));
    }

    #[test]
    fn symbolic_registered_unsigned_checks_require_converted_argument_evidence() {
        use crate::sema::schemes::{RegisteredArgumentCheck, RegisteredCallPlan, SchemeType};
        let method = &api_spec().method_overloads(MethodReceiver::Map, "get").unwrap()[0];
        let plan = RegisteredCallPlan {
            receiver: Some(MethodReceiver::Map), name: "get".into(),
            signature: &method.sig, argument_slots: std::sync::Arc::from([0]),
            argument_checks: std::sync::Arc::from([Some(RegisteredArgumentCheck::UIntMapKey)]),
        };
        let receiver = SchemeType::Map(Box::new(SchemeType::Ground(Type::UInt)), Box::new(SchemeType::Ground(Type::Str)));
        let result = SchemeType::Result(Box::new(SchemeType::Ground(Type::Str)), Box::new(SchemeType::Ground(Type::Error)));
        assert!(verify_registered_call(&plan, Some(&receiver), &[Some(&SchemeType::Ground(Type::UInt))], &result));
        assert!(!verify_registered_call(&plan, Some(&receiver), &[Some(&SchemeType::Ground(Type::Int))], &result));
        let mut missing = plan.clone();
        missing.argument_checks = std::sync::Arc::from([None]);
        assert!(!verify_registered_call(&missing, Some(&receiver), &[Some(&SchemeType::Ground(Type::UInt))], &result));
        let mut wrong = plan;
        wrong.argument_checks = std::sync::Arc::from([Some(RegisteredArgumentCheck::Unsigned(Type::Int))]);
        assert!(!verify_registered_call(&wrong, Some(&receiver), &[Some(&SchemeType::Ground(Type::UInt))], &result));
    }

    #[test]
    fn symbolic_registered_plans_match_canonical_ground_method_domains() {
        use crate::sema::schemes::RegisteredCallPlan;
        let mut domains = Vec::new();
        for (domain, methods) in api_spec().method_entries() {
            for named in methods {
                for method in &named.overloads {
                    let mut plan = RegisteredCallPlan {
                        receiver: Some(domain), name: named.name.into(), signature: &method.sig,
                        argument_slots: (0..method.sig.params.len()).collect::<Vec<_>>().into(),
                        argument_checks: vec![None; method.sig.params.len()].into(),
                    };
                    if !registered_call_is_executable(&plan) { continue; }
                    let mut constraints = TypeConstraints::default();
                    let mut parameters = BTreeMap::from([
                        (BuiltinTypeParameter::Receiver, Type::Int),
                        (BuiltinTypeParameter::Element, Type::Int),
                        (BuiltinTypeParameter::Key, Type::Str),
                        (BuiltinTypeParameter::Value, Type::Int),
                        (BuiltinTypeParameter::Error, Type::Error),
                    ]);
                    let receiver = match &method.receiver_ty {
                        Some(template) => instantiate_type(template, &mut parameters, &mut constraints, span()),
                        None if domain == MethodReceiver::Int => Type::UInt,
                        None if domain == MethodReceiver::Record => Type::Record(BTreeMap::from([(crate::symbol::Name::intern("value"), Type::Int)]).into()),
                        None => panic!("executable method has no representable receiver: {domain:?}"),
                    };
                    let instance = BuiltinInstantiation::new(&method.sig, method.receiver_ty.as_ref(), Some(&receiver), &mut constraints, span()).unwrap();
                    plan.argument_checks = instance.signature.params.iter().enumerate().map(|(slot, parameter)| registered_argument_check(Some(&receiver), slot, &parameter.ty)).collect::<Vec<_>>().into();
                    let types = std::iter::once(receiver)
                        .chain(instance.signature.params.iter().map(|parameter| parameter.ty.clone()))
                        .chain(std::iter::once(instance.signature.return_ty.clone())).collect::<Vec<_>>();
                    if types.iter().any(|ty| ty.contains_any() || ty.contains_inference() || ty.contains_recovery()) { continue; }
                    let generalized = constraints.generalize_types(&types, 0).unwrap();
                    let receiver = &generalized.types[0];
                    let result = generalized.types.last().unwrap();
                    let arguments = generalized.types[1..generalized.types.len() - 1].iter().map(Some).collect::<Vec<_>>();
                    assert!(verify_registered_call(&plan, Some(receiver), &arguments, result), "{domain:?}.{}", named.name);
                    if !domains.contains(&domain) {
                        domains.push(domain);
                    }
                }
            }
        }
        let expected = [MethodReceiver::Path, MethodReceiver::Int, MethodReceiver::Float, MethodReceiver::List, MethodReceiver::Map, MethodReceiver::Record, MethodReceiver::Str, MethodReceiver::Bytes, MethodReceiver::Status, MethodReceiver::Digest, MethodReceiver::Regex, MethodReceiver::NonEmpty, MethodReceiver::RelPath, MethodReceiver::Set];
        assert_eq!(domains.len(), expected.len());
        assert!(expected.iter().all(|domain| domains.contains(domain)));
    }

    #[test]
    fn map_empty_occurrences_have_independent_key_and_value_parameters() {
        let signature = &api_spec().module_overloads("map", "empty").unwrap()[0];
        let mut constraints = TypeConstraints::default();
        let mut first =
            BuiltinInstantiation::new(signature, None, None, &mut constraints, span()).unwrap();
        let mut second =
            BuiltinInstantiation::new(signature, None, None, &mut constraints, span()).unwrap();
        let ints = Type::Map(Box::new(Type::Str), Box::new(Type::Int));
        let strings = Type::Map(Box::new(Type::Int), Box::new(Type::Str));
        first
            .constrain_result(&ints, &mut constraints, span())
            .unwrap();
        second
            .constrain_result(&strings, &mut constraints, span())
            .unwrap();
        first.resolve(&constraints);
        assert_eq!(first.signature.return_ty, ints);
        assert_eq!(second.signature.return_ty, strings);
    }

    #[test]
    fn explicit_dynamic_value_domain_anchors_an_empty_map() {
        let signature = &api_spec().module_overloads("map", "empty").unwrap()[0];
        let mut constraints = TypeConstraints::default();
        let mut instance =
            BuiltinInstantiation::new(signature, None, None, &mut constraints, span()).unwrap();
        let expected = Type::Map(Box::new(Type::Str), Box::new(Type::Any));
        instance
            .constrain_result(&expected, &mut constraints, span())
            .unwrap();
        assert_eq!(instance.signature.return_ty, expected);
        assert!(!instance.signature.return_ty.contains_inference());
    }

    #[test]
    fn dynamic_list_receiver_cannot_acquire_a_concrete_item_proof() {
        let method = &api_spec()
            .method_overloads(MethodReceiver::List, "get")
            .unwrap()[0];
        let receiver = Type::List(Box::new(Type::Any));
        let mut constraints = TypeConstraints::default();
        let mut instance = BuiltinInstantiation::new(
            &method.sig,
            method.receiver_ty.as_ref(),
            Some(&receiver),
            &mut constraints,
            span(),
        )
        .unwrap();
        let expected = Type::Result(Box::new(Type::Int), Box::new(Type::Error));
        let conflict = instance
            .constrain_result(&expected, &mut constraints, span())
            .expect_err("an erased item cannot establish a concrete result domain");
        assert_eq!(conflict.expected, Type::Int);
        assert_eq!(conflict.actual, Type::Any);
        instance.resolve(&constraints);
        assert_eq!(
            instance.signature.return_ty,
            Type::Result(Box::new(Type::Any), Box::new(Type::Error))
        );
    }

    #[test]
    fn nested_result_and_nominal_error_domains_survive_receiver_instantiation() {
        let error = Type::ErrorFamily(crate::symbol::Name::intern("Error"));
        let item = Type::Result(Box::new(Type::List(Box::new(Type::Int))), Box::new(error));
        let receiver = Type::List(Box::new(item.clone()));
        let methods = api_spec()
            .method_overloads(MethodReceiver::List, "get")
            .unwrap();
        assert_eq!(
            concrete_method_signature(&methods[0], &receiver)
                .unwrap()
                .return_ty,
            Type::Result(Box::new(item.clone()), Box::new(Type::Error))
        );
        let push = &api_spec()
            .method_overloads(MethodReceiver::List, "push")
            .unwrap()[0];
        let push = concrete_method_signature(push, &receiver).unwrap();
        assert_eq!(push.return_ty, receiver);
        assert_eq!(push.params[0].ty, item);
    }
    #[test]
    fn success_expectation_preserves_a_declared_result_envelope() {
        let method = &api_spec()
            .method_overloads(MethodReceiver::Str, "parse_int")
            .unwrap()[0];
        let mut constraints = TypeConstraints::default();
        let mut instance = BuiltinInstantiation::new(
            &method.sig,
            method.receiver_ty.as_ref(),
            Some(&Type::Str),
            &mut constraints,
            span(),
        )
        .unwrap();
        instance
            .constrain_result(&Type::Int, &mut constraints, span())
            .unwrap();
        assert_eq!(
            instance.signature.return_ty,
            Type::Result(Box::new(Type::Int), Box::new(Type::Error))
        );
    }

    #[test]
    fn wider_destinations_preserve_concrete_builtin_result_domains() {
        for (name, expected, result) in [
            ("trim", Type::Optional(Box::new(Type::Str)), Type::Str),
            ("trim", Type::Any, Type::Str),
            (
                "parse_int",
                Type::Result(Box::new(Type::Any), Box::new(Type::Error)),
                Type::Result(Box::new(Type::Int), Box::new(Type::Error)),
            ),
        ] {
            let method = &api_spec()
                .method_overloads(MethodReceiver::Str, name)
                .unwrap()[0];
            let mut constraints = TypeConstraints::default();
            let mut instance = BuiltinInstantiation::new(
                &method.sig,
                method.receiver_ty.as_ref(),
                Some(&Type::Str),
                &mut constraints,
                span(),
            )
            .unwrap();
            instance
                .constrain_result(&expected, &mut constraints, span())
                .unwrap();
            assert_eq!(instance.signature.return_ty, result);
        }
    }

    #[test]
    fn concrete_inert_receiver_domains_do_not_become_fresh_holes() {
        let method = &api_spec()
            .method_overloads(MethodReceiver::List, "get")
            .unwrap()[0];
        for item in [Type::Null, Type::Record(BTreeMap::new().into())] {
            let receiver = Type::List(Box::new(item.clone()));
            let signature = concrete_method_signature(method, &receiver).unwrap();
            assert_eq!(signature.return_ty.result_ok(), Some(&item));
            assert!(!signature.return_ty.contains_inference());
        }
    }

    #[test]
    fn discarded_unit_context_does_not_invent_an_empty_map_domain() {
        let signature = &api_spec().module_overloads("map", "empty").unwrap()[0];
        let mut constraints = TypeConstraints::default();
        let mut instance =
            BuiltinInstantiation::new(signature, None, None, &mut constraints, span()).unwrap();
        instance
            .constrain_result(&Type::Unit, &mut constraints, span())
            .unwrap();
        assert!(instance.signature.return_ty.contains_inference());
    }
}
