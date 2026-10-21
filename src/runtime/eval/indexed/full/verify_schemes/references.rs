use super::{
    applications, body_types_equal, bounded, decode_scheme_schema, CallableMonotype,
    FullFunction, FullScheme, FullStore, IrVerifyError, Name, SchemeInstantiationBudget,
    SchemeParameter, SchemeScope, SchemeType,
};
use std::sync::Arc;

pub(super) fn verify(
    store: &FullStore,
    target: &FullFunction,
    scheme: &FullScheme,
    scope: &SchemeScope,
    substitution: applications::ApplicationSubstitution<'_>,
    claimed: &SchemeType,
    budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    if scheme.callable_kind == crate::sema::types::CallableKind::Stream {
        return Err(IrVerifyError::new("stream reference has no callable value representation"));
    }
    let parameters = bounded(scheme.params, &store.scheme_params, "callable reference parameter range is invalid")?;
    let physical = bounded(target.params, &store.params, "callable reference physical parameter range is invalid")?;
    if parameters.len() != physical.len() {
        return Err(IrVerifyError::new("callable reference parameter layout is inconsistent"));
    }
    budget.charge(parameters.len())?;
    let params = parameters.iter().zip(physical).map(|(parameter, physical)| {
        Ok(SchemeParameter {
            name: Name::intern(store.string(physical.name)?),
            ty: substitution.ty(store, scope, parameter.ty, budget)?,
            schema_expectation: parameter.schema.map(|schema| decode_scheme_schema(store, schema, budget, 0)).transpose()?,
            defaulted: physical.flags & 2 != 0,
            rest: physical.flags & 1 != 0,
        })
    }).collect::<Result<Vec<_>, IrVerifyError>>()?;
    let result = substitution.ty(store, scope, scheme.return_type, budget)?;
    let schema = scheme.return_schema.map(|schema| decode_scheme_schema(store, schema, budget, 0)).transpose()?;
    let effects = substitution.contract(store, scope, scheme.effects, budget)?.for_invocation(scheme.callable_kind);
    let actual = SchemeType::Callable {
        kind: scheme.callable_kind,
        monotype: Arc::new(CallableMonotype::new(params, result, schema)),
        effects,
    };
    budget.charge_scheme_clone(claimed)?;
    budget.charge_scheme_clone(&actual)?;
    if !body_types_equal(claimed, &actual) {
        return Err(IrVerifyError::new("callable reference disagrees with its actual scheme instance"));
    }
    Ok(())
}
