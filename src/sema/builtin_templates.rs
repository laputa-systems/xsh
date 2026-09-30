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
            seed_dynamic_parameters(template, actual, &mut parameters);
        }
        if let Some(receiver) = receiver {
            parameters.insert(BuiltinTypeParameter::Receiver, receiver.clone());
        }
        let mut signature = signature.clone();
        for parameter in &mut signature.params {
            parameter.ty = instantiate_type(&parameter.ty, &mut parameters, constraints, span);
        }
        signature.return_ty = instantiate_type(&signature.return_ty, &mut parameters, constraints, span);
        if let (Some(template), Some(actual)) = (receiver_template, receiver) {
            let template = instantiate_type(template, &mut parameters, constraints, span);
            constraints.constrain(&template, actual, span)?;
        }
        let mut instance = Self { signature };
        instance.resolve(constraints);
        Ok(instance)
    }

    pub fn constrain_result(
        &mut self, expected: &Type, constraints: &mut TypeConstraints, span: Span,
    ) -> Result<(), ConstraintConflict> {
        constraints.constrain(expected, &self.signature.return_ty, span)?;
        self.resolve(constraints);
        Ok(())
    }

    pub fn invalid_map_key(&self) -> Option<Type> {
        let mut pending = vec![&self.signature.return_ty];
        pending.extend(self.signature.params.iter().map(|parameter| &parameter.ty));
        while let Some(ty) = pending.pop() {
            match ty {
                Type::Map(key, value) => {
                    if !key.is_map_key() && !matches!(key.as_ref(), Type::Inference(_) | Type::Unknown | Type::Invalid) {
                        return Some((**key).clone());
                    }
                    pending.push(value);
                }
                Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) => pending.push(inner),
                Type::Result(ok, error) => { pending.push(ok); pending.push(error); }
                Type::Record(fields) => pending.extend(fields.values()),
                _ => {}
            }
        }
        None
    }

    pub fn resolve(&mut self, constraints: &TypeConstraints) {
        for parameter in &mut self.signature.params {
            if let Ok(resolved) = constraints.resolve(&parameter.ty) { parameter.ty = resolved; }
        }
        if let Ok(resolved) = constraints.resolve(&self.signature.return_ty) {
            self.signature.return_ty = resolved;
        }
    }
}

fn instantiate_type(
    ty: &Type, parameters: &mut BTreeMap<BuiltinTypeParameter, Type>,
    constraints: &mut TypeConstraints, span: Span,
) -> Type {
    match ty {
        Type::BuiltinParameter(parameter) => parameters.entry(*parameter)
            .or_insert_with(|| constraints.fresh(span)).clone(),
        Type::List(inner) => Type::List(Box::new(instantiate_type(inner, parameters, constraints, span))),
        Type::Map(key, value) => Type::Map(
            Box::new(instantiate_type(key, parameters, constraints, span)),
            Box::new(instantiate_type(value, parameters, constraints, span)),
        ),
        Type::Stream(inner) => Type::Stream(Box::new(instantiate_type(inner, parameters, constraints, span))),
        Type::Optional(inner) => Type::Optional(Box::new(instantiate_type(inner, parameters, constraints, span))),
        Type::Result(ok, error) => Type::Result(
            Box::new(instantiate_type(ok, parameters, constraints, span)),
            Box::new(instantiate_type(error, parameters, constraints, span)),
        ),
        Type::Record(fields) => Type::Record(fields.iter().map(|(name, ty)|
            (*name, instantiate_type(ty, parameters, constraints, span))).collect()),
        ty => ty.clone(),
    }
}

/// Dynamic receiver parameters are fixed Any domains. Leaving them as fresh
/// holes would let a supplied concrete operand falsely certify older elements.
fn seed_dynamic_parameters(
    template: &Type, actual: &Type, parameters: &mut BTreeMap<BuiltinTypeParameter, Type>,
) {
    match (template, actual) {
        (template, Type::Any) => seed_erased_parameters(template, parameters),
        (Type::List(left), Type::List(right))
        | (Type::Stream(left), Type::Stream(right))
        | (Type::Optional(left), Type::Optional(right)) => seed_dynamic_parameters(left, right, parameters),
        (Type::Map(left_key, left_value), Type::Map(right_key, right_value)) => {
            seed_dynamic_parameters(left_key, right_key, parameters);
            seed_dynamic_parameters(left_value, right_value, parameters);
        }
        (Type::Result(left_ok, left_error), Type::Result(right_ok, right_error)) => {
            seed_dynamic_parameters(left_ok, right_ok, parameters);
            seed_dynamic_parameters(left_error, right_error, parameters);
        }
        _ => {}
    }
}

fn seed_erased_parameters(template: &Type, parameters: &mut BTreeMap<BuiltinTypeParameter, Type>) {
    match template {
        Type::BuiltinParameter(parameter) => { parameters.insert(*parameter, Type::Any); }
        Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) => seed_erased_parameters(inner, parameters),
        Type::Map(key, value) | Type::Result(key, value) => {
            seed_erased_parameters(key, parameters);
            seed_erased_parameters(value, parameters);
        }
        Type::Record(fields) => for field in fields.values() { seed_erased_parameters(field, parameters); },
        _ => {}
    }
}

pub(crate) fn concrete_method_signature(method: &crate::modules::signature::MethodSig, receiver: &Type) -> Option<ModuleFnSig> {
    let mut constraints = TypeConstraints::default();
    let mut instance = BuiltinInstantiation::new(&method.sig, method.receiver_ty.as_ref(), Some(receiver), &mut constraints, Span::at(crate::source::SourceId::new(0), 0)).ok()?;
    instance.resolve(&constraints);
    (!instance.signature.return_ty.contains_inference()).then_some(instance.signature)
}

pub(crate) fn callable_parameters(signature: &ModuleFnSig) -> Vec<super::types::CallableParamType> {
    signature.params.iter().map(|parameter| super::types::CallableParamType {
        name: crate::symbol::Name::intern(parameter.name), ty: parameter.ty.clone(),
        defaulted: parameter.defaulted, rest: false,
    }).collect()
}
