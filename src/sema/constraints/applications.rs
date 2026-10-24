use super::{Binding, ConstraintChange, ConstraintConflict, ConstraintResolutionError, RowTail, RowVariableId, TypeConstraints, TypeVariableId, Variable, MAX_TYPE_DEPTH};
use crate::sema::arguments::{ArgumentExpansionError, StaticArgumentBinding, StaticArgumentKind};
use crate::sema::callable_application::{CallableApplication, CallableInvocation, CallableKindRequirement, KnownCallArguments};
use crate::sema::check::EffectDeclarationId;
use crate::sema::effects::{EffectContract, EffectSet};
use crate::sema::types::{CallableKind, Type};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_APPLICATION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct CallableApplicationId(u64);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum CallableApplicationScope {
    Declaration(EffectDeclarationId),
    Guard(super::WrapperOperationId),
    Invocation,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct CallableApplicationReference {
    pub(crate) scope: CallableApplicationScope,
    pub(crate) ordinal: u32,
}

#[derive(Clone, Debug)]
pub(crate) enum CallableApplicationError {
    Resolution(ConstraintResolutionError),
    Constraint(ConstraintConflict),
    Layout(ArgumentExpansionError),
    Kind { required: CallableKindRequirement, actual: CallableKind },
    NotCallable(Type),
    ReturnShape(Type),
    SourceShape(crate::source::Span),
    Effects(super::EffectConstraintError),
    Operation(super::SealedOperationError),
    Wrapper(super::WrapperConstraintError),
    Method(super::method_constraints::MethodConstraintError),
}

impl From<ConstraintResolutionError> for CallableApplicationError {
    fn from(error: ConstraintResolutionError) -> Self { Self::Resolution(error) }
}

impl std::fmt::Display for CallableApplicationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resolution(ConstraintResolutionError::ForeignVariable) => formatter.write_str("application refers to a type outside its inference scope"),
            Self::Resolution(ConstraintResolutionError::TypeDepth) => formatter.write_str("application exceeds the type-depth limit"),
            Self::Constraint(conflict) => write!(formatter, "expected `{}`, found `{}`", conflict.expected, conflict.actual),
            Self::Layout(error) => formatter.write_str(&error.message),
            Self::Kind { required, actual } => {
                let required = match required { CallableKindRequirement::Pure => "pure", CallableKindRequirement::Effectful => "pure or proc" };
                let actual = match actual { CallableKind::Pure => "pure", CallableKind::Proc => "proc", CallableKind::Stream => "stream" };
                write!(formatter, "requires a {required} callable, found a {actual} callable")
            }
            Self::NotCallable(actual) => write!(formatter, "`{actual}` has no checked callable signature"),
            Self::ReturnShape(actual) => write!(formatter, "proc return envelope is not determined by `{actual}`"),
            Self::SourceShape(span) => write!(formatter, "application argument layout changed at source span {span:?}"),
            Self::Effects(error) => match error {
                super::EffectConstraintError::Resolution(ConstraintResolutionError::ForeignVariable) => formatter.write_str("application effects refer to another inference scope"),
                super::EffectConstraintError::Resolution(ConstraintResolutionError::TypeDepth) => formatter.write_str("application effects exceed the type-depth limit"),
                super::EffectConstraintError::Mismatch => formatter.write_str("application effects violate the required contract"),
                super::EffectConstraintError::Unresolved => formatter.write_str("application effect relationship remains unresolved"),
            },
            Self::Operation(error) => std::fmt::Display::fmt(error, formatter),
            Self::Wrapper(error) => std::fmt::Display::fmt(error, formatter),
            Self::Method(error) => std::fmt::Display::fmt(error, formatter),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ScopedCallableApplication {
    pub(super) owner: CallableApplicationScope,
    ordinal: u32,
    pub(super) application: CallableApplication<Type, super::EffectVariableId>,
    pub(super) binding: Option<CallableApplicationBinding>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum CallableApplicationBinding {
    PendingArguments(StaticArgumentBinding),
    Settled(StaticArgumentBinding),
}

impl CallableApplicationBinding {
    fn layout(&self) -> &StaticArgumentBinding {
        match self {
            Self::PendingArguments(layout) | Self::Settled(layout) => layout,
        }
    }

    fn arguments_pending(&self) -> bool {
        matches!(self, Self::PendingArguments(_))
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum ApplicationDependency {
    Type(TypeVariableId),
    Row(RowVariableId),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct CallableApplications {
    pub(super) entries: BTreeMap<CallableApplicationId, ScopedCallableApplication>,
    scopes: BTreeMap<CallableApplicationScope, Vec<CallableApplicationId>>,
    source_sites: BTreeMap<(CallableApplicationScope, crate::source::Span), CallableApplicationId>,
    watchers: BTreeMap<ApplicationDependency, BTreeSet<CallableApplicationId>>,
    pending: VecDeque<CallableApplicationId>,
    queued: BTreeSet<CallableApplicationId>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::callable_application::CallableApplicationArgument;
    use crate::sema::types::{CallableEffects, CallableParamType, CallableType, TypedCallable};
    use crate::source::{SourceId, Span};
    use crate::symbol::{Name, SymbolOwner};
    use crate::syntax::node::Effect;
    use std::sync::Arc;

    fn span() -> Span { Span::new(SourceId::new(0), 0, 1) }
    fn owner() -> CallableApplicationScope { CallableApplicationScope::Declaration(EffectDeclarationId { namespace: None, body: span() }) }
    fn callback(pure: bool, name: &str, ty: Type, default: bool, rest: bool) -> Type {
        Type::Callable(Arc::new(TypedCallable { kind: if pure { crate::sema::types::CallableKind::Pure } else { crate::sema::types::CallableKind::Proc }, sig: CallableType {
            params: vec![CallableParamType { name: Name::intern(name), ty: if rest { Type::List(Box::new(ty.clone())) } else { ty.clone() }, defaulted: false, rest }]
                .into_iter().chain(default.then(|| CallableParamType { name: Name::intern("scale"), ty: Type::Int, defaulted: true, rest: false })).collect(),
            return_ty: Box::new(if rest { Type::List(Box::new(ty)) } else { ty }),
            effects: if pure { CallableEffects::unknown() } else { CallableEffects::finite([Effect::Env, Effect::Error]) },
        } }))
    }
    fn application(constraints: &mut TypeConstraints, callee: Type, values: Vec<(StaticArgumentKind, Type)>, kind: CallableKindRequirement) -> CallableInvocation<Type, super::super::EffectVariableId> {
        CallableInvocation {
            callee, arguments: values.into_iter().map(|(kind, ty)| CallableApplicationArgument { kind, ty, span: span() }).collect::<Vec<_>>().into(),
            result: constraints.fresh_at_level(span(), 1),
            effects: EffectContract::variable(constraints.fresh_effect_at_level(span(), 1)), proc_returns: Arc::from([]), kind, span: span(),
        }
    }

    fn generic_application_scheme(constraints: &mut TypeConstraints) -> crate::sema::schemes::CallableScheme {
        generic_application_scheme_with_scope(constraints).0
    }

    fn generic_application_scheme_with_scope(constraints: &mut TypeConstraints) -> (crate::sema::schemes::CallableScheme, super::super::CallableGeneralizationScope) {
        let callee = constraints.fresh_at_level(span(), 1);
        let value = constraints.fresh_at_level(span(), 1);
        let application = application(constraints, callee.clone(), vec![(StaticArgumentKind::Positional, value.clone())], CallableKindRequirement::Effectful);
        constraints.register_callable_application(owner(), application).unwrap();
        let CallableApplicationScope::Declaration(declaration) = owner() else { unreachable!() };
        let (generalized, _, _, _) = constraints.generalize_callable_body_with_calls::<crate::source::Span>(
            &[callee.clone(), value, callee], &BTreeMap::new(), &BTreeMap::new(), &BTreeMap::new(), 0, declaration,
        ).unwrap();
        assert_eq!(generalized.type_parameters.len(), 3);
        assert_eq!(generalized.requirements.effect_parameters.len(), 1);
        let mut types = generalized.types.into_iter();
        let params = ["action", "value"].into_iter().map(|name| crate::sema::schemes::SchemeParameter {
            name: Name::intern(name), ty: types.next().unwrap(), schema_expectation: None, defaulted: false, rest: false,
        }).collect();
        let scheme = crate::sema::schemes::CallableScheme::new(generalized.type_parameters, generalized.row_parameters, params, types.next().unwrap(), None, false, generalized.requirements).unwrap();
        (scheme, generalized.generalization_scope)
    }

    #[test]
    fn callable_requirement_monomorphic_rebinding_keeps_original_cells_and_owner() {
        SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let (scheme, scope) = generic_application_scheme_with_scope(&mut constraints);
            let CallableApplicationScope::Declaration(owner) = scope.owner else { unreachable!() };
            let counts = (constraints.variables.len(), constraints.rows.len(), constraints.applications.entries.len());
            let first = constraints.reuse_callable_instance(&scheme, &scope, owner, span()).unwrap();
            let second = constraints.reuse_callable_instance(&scheme, &scope, owner, span()).unwrap();
            assert_eq!(first.type_arguments, second.type_arguments);
            assert_eq!(first.row_arguments, second.row_arguments);
            assert_eq!(first.effect_arguments, second.effect_arguments);
            assert_eq!(first.applications, second.applications);
            assert_eq!(first.params, second.params);
            assert_eq!(first.return_type, second.return_type);
            assert_eq!(counts, (constraints.variables.len(), constraints.rows.len(), constraints.applications.entries.len()));
            assert_eq!(constraints.callable_application_reference(first.applications[0]).unwrap().scope, scope.owner);
            let other = EffectDeclarationId { namespace: None, body: Span::new(SourceId::new(0), 2, 3) };
            assert!(constraints.reuse_callable_instance(&scheme, &scope, other, span()).is_err());
            assert!(TypeConstraints::default().reuse_callable_instance(&scheme, &scope, owner, span()).is_err());
        });
    }

    #[test]
    fn recursive_peer_application_scopes_preserve_cells_and_reject_forged_relationships() {
        SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let first_owner = EffectDeclarationId { namespace: None, body: Span::new(SourceId::new(0), 1, 2) };
            let second_owner = EffectDeclarationId { namespace: None, body: Span::new(SourceId::new(0), 3, 4) };
            let first_callback = constraints.fresh_at_level(span(), 1);
            let second_callback = constraints.fresh_at_level(span(), 1);
            let first_value = constraints.fresh_at_level(span(), 1);
            let second_value = constraints.fresh_at_level(span(), 1);
            let first_application = application(&mut constraints, first_callback.clone(), vec![(StaticArgumentKind::Positional, first_value.clone())], CallableKindRequirement::Pure);
            let second_application = application(&mut constraints, second_callback.clone(), vec![(StaticArgumentKind::Named(Name::intern("payload")), second_value.clone())], CallableKindRequirement::Effectful);
            let first_id = constraints.register_callable_application(CallableApplicationScope::Declaration(first_owner), first_application).unwrap();
            let second_id = constraints.register_callable_application(CallableApplicationScope::Declaration(second_owner), second_application).unwrap();
            let owners = BTreeSet::from([first_owner, second_owner]);
            let types = [first_callback, first_value, second_callback, second_value, Type::Int];
            let build = |constraints: &TypeConstraints, owner, types: &[Type]| {
                let (generalized, _, _, _) = constraints.generalize_callable_body_with_calls_in_component::<crate::source::Span>(
                    types, &BTreeMap::new(), &BTreeMap::new(), &BTreeMap::new(), 0, owner, &owners, &|_| CallableApplicationScope::Declaration(owner),
                ).unwrap();
                let mut types = generalized.types;
                let result = types.pop().unwrap();
                let params = ["first", "left", "second", "right"].into_iter().zip(types).map(|(name, ty)| crate::sema::schemes::SchemeParameter {
                    name: Name::intern(name), ty, schema_expectation: None, defaulted: false, rest: false,
                }).collect();
                let callable = crate::sema::schemes::CallableScheme::new(generalized.type_parameters, generalized.row_parameters, params, result, None, false, generalized.requirements).unwrap();
                (callable, generalized.generalization_scope)
            };
            let (first_scheme, first_scope) = build(&constraints, first_owner, &types);
            let reordered = [types[2].clone(), types[3].clone(), types[0].clone(), types[1].clone(), Type::Int];
            let (second_scheme, second_scope) = build(&constraints, second_owner, &reordered);
            let counts = (constraints.variables.len(), constraints.rows.len(), constraints.applications.entries.len());
            let first_instance = constraints.reuse_callable_instance(&first_scheme, &first_scope, first_owner, span()).unwrap();
            let second_instance = constraints.reuse_callable_instance(&second_scheme, &second_scope, second_owner, span()).unwrap();
            assert_eq!(first_instance.applications, vec![first_id, second_id]);
            assert_eq!(second_instance.applications, first_instance.applications);
            assert_eq!(counts, (constraints.variables.len(), constraints.rows.len(), constraints.applications.entries.len()));
            let call = constraints.generalize_scoped_call_instantiation(&first_scheme, &first_scope, &second_scheme, &second_instance).unwrap();
            assert_eq!(call.application_bindings(), &[crate::sema::schemes::CallableApplicationBinding::Forwarded(0), crate::sema::schemes::CallableApplicationBinding::Forwarded(1)]);
            assert_ne!(first_instance.type_arguments, second_instance.type_arguments);
            let peer_reference = constraints.callable_application_reference(second_id).unwrap();
            assert_eq!(peer_reference.ordinal, 0);
            assert_eq!(constraints.generalized_application_ordinal(peer_reference, first_owner, &first_scheme, &first_scope).unwrap(), Some(1));
            let mut wrong_binder = first_scope.clone();
            wrong_binder.owner = CallableApplicationScope::Declaration(second_owner);
            assert!(constraints.generalized_application_ordinal(peer_reference, first_owner, &first_scheme, &wrong_binder).is_err());
            let mut wrong_owner = first_scope.clone();
            wrong_owner.application_sources.get_mut(&second_id).unwrap().scope = CallableApplicationScope::Declaration(first_owner);
            assert!(matches!(constraints.reuse_callable_instance(&first_scheme, &wrong_owner, first_owner, span()), Err(super::super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedCallableApplication))));
            assert!(constraints.generalized_application_ordinal(peer_reference, first_owner, &first_scheme, &wrong_owner).is_err());
            let mut wrong_ordinal = first_scope.clone();
            wrong_ordinal.applications.insert(first_id, 1);
            wrong_ordinal.applications.insert(second_id, 0);
            assert!(matches!(constraints.generalize_scoped_call_instantiation(&first_scheme, &wrong_ordinal, &second_scheme, &second_instance), Err(super::super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedCallableApplication))));
            assert!(constraints.generalized_application_ordinal(peer_reference, first_owner, &first_scheme, &wrong_ordinal).is_err());
            let mut wrong_type = first_scope.clone();
            let ids = wrong_type.types.keys().copied().take(2).collect::<Vec<_>>();
            let first = wrong_type.types[&ids[0]];
            let second = wrong_type.types[&ids[1]];
            wrong_type.types.insert(ids[0], second);
            wrong_type.types.insert(ids[1], first);
            assert!(constraints.reuse_callable_instance(&first_scheme, &wrong_type, first_owner, span()).is_err());
            assert!(constraints.generalized_application_ordinal(peer_reference, first_owner, &first_scheme, &wrong_type).is_err());
            let mut wrong_effect = first_scope;
            let effects = wrong_effect.effects.keys().copied().collect::<Vec<_>>();
            let first = wrong_effect.effects[&effects[0]];
            let second = wrong_effect.effects[&effects[1]];
            wrong_effect.effects.insert(effects[0], second);
            wrong_effect.effects.insert(effects[1], first);
            assert!(constraints.reuse_callable_instance(&first_scheme, &wrong_effect, first_owner, span()).is_err());
            assert!(constraints.generalized_application_ordinal(peer_reference, first_owner, &first_scheme, &wrong_effect).is_err());
        });
    }

    #[test]
    fn known_nullable_argument_requirements_preserve_pending_sources_and_rollback() {
        SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let input = constraints.fresh_at_level(span(), 1);
            let requirement = KnownCallArguments {
                parameters: vec![crate::sema::callable_application::KnownArgumentParameter {
                    layout: crate::sema::arguments::ArgumentParameterLayout { name: Name::intern("value"), defaulted: false, rest: false },
                    ty: Type::Optional(Box::new(Type::Int)),
                }].into(),
                arguments: vec![CallableApplicationArgument { kind: StaticArgumentKind::Positional, ty: input.clone(), span: span() }].into(),
                span: span(),
            };
            let id = constraints.register_known_call_arguments(owner(), requirement).unwrap();
            constraints.solve_callable_applications().unwrap();
            assert_eq!(constraints.resolve(&input).unwrap(), input);
            assert!(constraints.settled_callable_application_binding(id).unwrap().is_none());
            let before = constraints.clone();
            let rejected = constraints.commit_constraint_transaction(|this| {
                this.constrain(&input, &Type::Str, span()).map_err(CallableApplicationError::Constraint)?;
                this.solve_callable_applications()
            });
            assert!(matches!(rejected, Err(CallableApplicationError::Constraint(_))));
            assert_eq!(constraints.resolve(&input).unwrap(), input);
            assert_eq!(constraints.applications, before.applications);
            assert_eq!(constraints.effects, before.effects);
            constraints.constrain(&input, &Type::Int, span()).unwrap();
            constraints.solve_callable_applications().unwrap();
            assert!(constraints.settled_callable_application_binding(id).unwrap().is_some());
        });
    }

    #[test]
    fn callable_requirement_source_identity_is_journaled_without_merging_invocations() {
        SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let callee = constraints.fresh_at_level(span(), 1);
            let requirement = application(&mut constraints, callee, vec![(StaticArgumentKind::Positional, Type::Int)], CallableKindRequirement::Pure);
            let source = constraints.register_source_callable_application(owner(), requirement.clone()).unwrap();
            assert_eq!(constraints.register_source_callable_application(owner(), requirement.clone()).unwrap(), source);
            assert_eq!(constraints.applications.source_sites.len(), 1);
            assert_eq!(constraints.applications.entries.len(), 1);
            let activation = constraints.register_callable_application(owner(), requirement.clone()).unwrap();
            assert_ne!(activation, source);
            assert_eq!(constraints.applications.source_sites.len(), 1);
            let before = constraints.applications.clone();
            let mut changed = requirement.clone();
            Arc::make_mut(&mut changed.arguments)[0].kind = StaticArgumentKind::Named(Name::intern("invented"));
            assert!(matches!(constraints.register_source_callable_application(owner(), changed), Err(CallableApplicationError::SourceShape(_))));
            assert_eq!(constraints.applications, before);
            constraints.trial(|this| {
                let mut other = requirement;
                other.span = Span::new(SourceId::new(0), 2, 3);
                this.register_source_callable_application(owner(), other).unwrap();
                Ok(())
            }).unwrap();
            assert_eq!(constraints.applications, before);
        });
    }

    #[test]
    fn callable_requirement_return_envelopes_follow_actual_application_results() {
        SymbolOwner::new().with_current(|| {
            let first = Type::ErrorFamily(Name::intern("First"));
            let second = Type::ErrorFamily(Name::intern("Second"));
            for (pure, returned, errors, expected) in [
                (true, Type::Int, Vec::new(), Type::Result(Box::new(Type::Int), Box::new(Type::Error))),
                (false, Type::Int, Vec::new(), Type::Result(Box::new(Type::Int), Box::new(Type::Error))),
                (true, Type::Result(Box::new(Type::Int), Box::new(first.clone())), Vec::new(), Type::Result(Box::new(Type::Int), Box::new(first.clone()))),
                (false, Type::Result(Box::new(Type::Int), Box::new(first.clone())), vec![second], Type::Result(Box::new(Type::Int), Box::new(Type::Error))),
            ] {
                let mut constraints = TypeConstraints::default();
                let callee = constraints.fresh_at_level(span(), 1);
                let requirement = application(&mut constraints, callee.clone(), vec![(StaticArgumentKind::Positional, Type::Int)], CallableKindRequirement::Effectful);
                let application_id = constraints.register_callable_application(owner(), requirement.clone()).unwrap();
                let alias = constraints.fresh_at_level(span(), 1);
                constraints.constrain(&alias, &requirement.result, span()).unwrap();
                let envelope = constraints.defer_proc_application_return(application_id, &errors, span(), 1).unwrap();
                let mut actual = callback(pure, "payload", Type::Int, false, false);
                let Type::Callable(callable) = &mut actual else { unreachable!() };
                *Arc::make_mut(callable).sig.return_ty = returned;
                constraints.constrain(&callee, &actual, span()).unwrap();
                constraints.solve_callable_applications().unwrap();
                assert_eq!(constraints.resolve(&envelope).unwrap(), expected);
            }
            let inner = Type::Result(Box::new(Type::Int), Box::new(first));
            assert_eq!(crate::sema::callable_application::inferred_proc_return_envelope(inner.clone(), &[]), inner);
        });
    }

    #[test]
    fn callable_requirement_return_envelopes_generalize_once_and_rollback() {
        SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let callee = constraints.fresh_at_level(span(), 1);
            let value = constraints.fresh_at_level(span(), 1);
            let requirement = application(&mut constraints, callee.clone(), vec![(StaticArgumentKind::Positional, value.clone())], CallableKindRequirement::Effectful);
            let application_id = constraints.register_callable_application(owner(), requirement.clone()).unwrap();
            let CallableApplicationScope::Declaration(declaration) = owner() else { unreachable!() };
            let before = constraints.applications.clone();
            constraints.trial(|this| {
                this.defer_proc_application_return(application_id, &[], span(), 1).unwrap();
                Ok(())
            }).unwrap();
            assert_eq!(constraints.applications, before);
            let reference = constraints.callable_application_reference(application_id).unwrap();
            assert_eq!(reference.scope, owner());
            assert_eq!(reference.ordinal, 0);
            assert_eq!(constraints.referenced_callable_application(reference).unwrap(), application_id);
            assert!(TypeConstraints::default().referenced_callable_application(reference).is_err());
            let envelope = constraints.defer_proc_application_return(application_id, &[], span(), 1).unwrap();
            assert_eq!(constraints.defer_proc_application_return(application_id, &[], span(), 1).unwrap(), envelope);
            assert_eq!(constraints.applications.entries[&application_id].application.invocation().unwrap().proc_returns.len(), 1);
            let (generalized, _, _, _) = constraints.generalize_callable_body_with_calls::<crate::source::Span>(&[callee, value, envelope], &BTreeMap::new(), &BTreeMap::new(), &BTreeMap::new(), 0, declaration).unwrap();
            assert_eq!(generalized.type_parameters.len(), 4);
            let mut types = generalized.types.into_iter();
            let params = ["action", "value"].into_iter().map(|name| crate::sema::schemes::SchemeParameter {
                name: Name::intern(name), ty: types.next().unwrap(), schema_expectation: None, defaulted: false, rest: false,
            }).collect();
            let scheme = crate::sema::schemes::CallableScheme::new(generalized.type_parameters, generalized.row_parameters, params, types.next().unwrap(), None, false, generalized.requirements).unwrap();
            for returned in [Type::Int, Type::Result(Box::new(Type::Int), Box::new(Type::Error))] {
                let instance = constraints.instantiate_callable(&scheme, 1, span(), CallableApplicationScope::Invocation).unwrap();
                let mut actual = callback(false, "payload", Type::Int, false, false);
                let Type::Callable(callable) = &mut actual else { unreachable!() };
                *Arc::make_mut(callable).sig.return_ty = returned;
                constraints.constrain(&instance.params[0], &actual, span()).unwrap();
                constraints.constrain(&instance.params[1], &Type::Int, span()).unwrap();
                constraints.solve_callable_applications().unwrap();
                assert_eq!(constraints.resolve(&instance.return_type).unwrap(), Type::Result(Box::new(Type::Int), Box::new(Type::Error)));
                assert_eq!(constraints.resolve_call_instantiation(&scheme, &instance).unwrap().type_arguments().len(), 4);
            }
        });
    }

    #[test]
    fn callable_requirement_nested_effects_share_bound_scope_and_fresh_instances() {
        SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let effect = constraints.fresh_effect_at_level(span(), 1);
            let mut actual = callback(false, "payload", Type::Int, false, false);
            let Type::Callable(callable) = &mut actual else { unreachable!() };
            Arc::make_mut(callable).sig.effects = CallableEffects::from_contract(EffectContract::variable(effect));
            let CallableApplicationScope::Declaration(declaration) = owner() else { unreachable!() };
            let (generalized, _, _, _) = constraints.generalize_callable_body_with_calls::<crate::source::Span>(
                &[actual.clone(), actual], &BTreeMap::new(), &BTreeMap::new(), &BTreeMap::new(), 0, declaration,
            ).unwrap();
            assert_eq!(generalized.requirements.effect_parameters.len(), 1);
            assert!(generalized.types[0].closed_ground_type().is_none());
            assert!(generalized.types[0].contains_parameter());
            let rebound = constraints.resolve_generalized_effect(&EffectContract::variable(effect), &generalized.generalization_scope).unwrap();
            let crate::sema::schemes::SchemeType::Callable { effects, .. } = &generalized.types[0] else { unreachable!() };
            assert_eq!(&rebound, effects);
            let foreign = TypeConstraints::default().fresh_effect_at_level(span(), 1);
            assert!(constraints.resolve_generalized_effect(&EffectContract::variable(foreign), &generalized.generalization_scope).is_err());
            let later = constraints.fresh_effect_at_level(span(), 1);
            assert!(matches!(constraints.resolve_generalized_effect(&EffectContract::variable(later), &generalized.generalization_scope), Err(super::super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::BodyTypeNotBound))));
            let mut types = generalized.types.into_iter();
            let parameter = crate::sema::schemes::SchemeParameter {
                name: Name::intern("action"), ty: types.next().unwrap(), schema_expectation: None, defaulted: false, rest: false,
            };
            let scheme = crate::sema::schemes::CallableScheme::new(
                generalized.type_parameters, generalized.row_parameters, vec![parameter.clone()], types.next().unwrap(), None, false, generalized.requirements,
            ).unwrap();
            assert!(scheme.is_quantified());
            assert!(scheme.type_parameters().is_empty());
            assert!(scheme.row_parameters().is_empty());
            assert_eq!(crate::sema::schemes::CallableScheme::new(Vec::new(), Vec::new(), vec![parameter], scheme.return_type().clone(), None, false,
                crate::sema::schemes::CallableRequirements::default()), Err(crate::sema::schemes::SchemeBuildError::UnboundEffectParameter));
            let mut previous = None;
            for effects in [EffectSet::new([Effect::Env, Effect::Error]), EffectSet::new([Effect::Time])] {
                let instance = constraints.instantiate_callable(&scheme, 1, span(), CallableApplicationScope::Invocation).unwrap();
                let variable = *instance.effect_arguments.values().next().unwrap();
                assert_ne!(previous, Some(variable));
                previous = Some(variable);
                constraints.constrain_effect(super::super::EffectPredicate::Equal(EffectContract::variable(variable), EffectContract::finite(effects))).unwrap();
                let returned = constraints.resolve(&instance.return_type).unwrap();
                assert_eq!(returned, constraints.resolve(&instance.params[0]).unwrap());
                let Type::Callable(returned) = returned else { unreachable!() };
                assert_eq!(returned.sig.effects.concrete().unwrap(), Some(effects.iter().collect()));
                assert_eq!(constraints.resolve_call_instantiation(&scheme, &instance).unwrap().effect_arguments().values().next().unwrap().closed(), Some(effects));
            }
        });
    }

    #[test]
    fn callable_requirement_forwarded_instances_retain_caller_obligations() {
        SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let selected = Arc::new(generic_application_scheme(&mut constraints));
            let caller = EffectDeclarationId { namespace: None, body: Span::new(SourceId::new(0), 2, 3) };
            let instance = constraints.instantiate_callable(&selected, 1, span(), CallableApplicationScope::Declaration(caller)).unwrap();
            assert!(constraints.applications.entries[&instance.applications[0]].binding.is_none());
            let calls = BTreeMap::from([(span(), crate::sema::value_materialization::InstantiationEvidence::Invocation((Arc::clone(&selected), instance.clone())))]);
            let result = constraints.generalize_callable_body_with_calls::<crate::source::Span>(
                &[instance.params[0].clone(), instance.params[1].clone(), instance.return_type.clone()],
                &BTreeMap::new(), &BTreeMap::new(), &calls, 0, caller,
            );
            let (generalized, _, _, calls) = result.expect("a forwarded application remains the same caller-qualified obligation");
            assert_eq!(generalized.requirements.applications.len(), 1);
            assert_eq!(calls.len(), 1);
            let evidence = calls[&span()].invocation().expect("a forwarded call retains invocation evidence");
            assert_eq!(evidence.application_bindings(), &[crate::sema::schemes::CallableApplicationBinding::Forwarded(0)]);
            for (binding, requirements) in [
                (crate::sema::schemes::CallableApplicationBinding::Forwarded(0), &[][..]),
                (crate::sema::schemes::CallableApplicationBinding::Forwarded(1), generalized.requirements.applications.as_slice()),
            ] {
                assert_eq!(crate::sema::schemes::CallInstantiation::new(
                    &selected, evidence.type_arguments().clone(), evidence.row_arguments().clone(), evidence.effect_arguments().clone(), vec![binding], Vec::new(), Vec::new(),
                    &generalized.type_parameters, &generalized.row_parameters, &generalized.requirements.effect_parameters, requirements, &[], &[], &[], &BTreeMap::new()), Err(crate::sema::schemes::SchemeBuildError::UnresolvedCallableApplication));
            }
        });
    }

    #[test]
    fn callable_requirement_instances_share_one_scope_and_keep_clients_fresh() {
        SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let scheme = generic_application_scheme(&mut constraints);
            let mut previous_effect = None;
            for (pure, name, ty) in [(false, "input", Type::Str), (true, "payload", Type::Int)] {
                let instance = constraints.instantiate_callable(&scheme, 1, span(), CallableApplicationScope::Invocation).unwrap();
                let actual = callback(pure, name, ty.clone(), true, false);
                constraints.constrain(&instance.params[0], &actual, span()).unwrap();
                constraints.constrain(&instance.params[1], &ty, span()).unwrap();
                constraints.solve_callable_applications().unwrap();
                assert_eq!(constraints.resolve(&instance.return_type).unwrap(), actual);
                let evidence = constraints.resolve_call_instantiation(&scheme, &instance).unwrap();
                assert_eq!(evidence.type_arguments().len(), 3);
                assert_eq!(evidence.application_bindings(), &[crate::sema::schemes::CallableApplicationBinding::Selected(StaticArgumentBinding { argument_slots: vec![0], omitted_slots: vec![1] })]);
                assert_eq!(evidence.effect_arguments().values().next().unwrap().closed(), Some(if pure { EffectSet::default() } else { EffectSet::new([Effect::Env, Effect::Error]) }));
                let fresh = *instance.effect_arguments.values().next().unwrap();
                assert_ne!(previous_effect, Some(fresh));
                previous_effect = Some(fresh);
            }
        });
    }

    #[test]
    fn callable_requirement_instances_restore_complete_transaction_ownership() {
        SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let scheme = generic_application_scheme(&mut constraints);
            let variable_count = constraints.variables.len();
            let row_count = constraints.rows.len();
            let effects = constraints.effects.clone();
            let applications = constraints.applications.clone();
            let failed = constraints.commit_constraint_transaction(|this| {
                this.instantiate_callable(&scheme, 1, span(), CallableApplicationScope::Invocation)?;
                Err::<(), _>(super::super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedSchema))
            });
            assert!(failed.is_err());
            assert_eq!(constraints.variables.len(), variable_count);
            assert_eq!(constraints.rows.len(), row_count);
            assert_eq!(constraints.effects, effects);
            assert_eq!(constraints.applications, applications);
            let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = constraints.commit_constraint_transaction::<(), super::super::SchemeOperationError>(|this| {
                    this.instantiate_callable(&scheme, 1, span(), CallableApplicationScope::Invocation)?;
                    panic!("failed activation after qualified instantiation")
                });
            }));
            assert!(panic.is_err());
            assert!(constraints.trial_changes.is_none());
            assert_eq!(constraints.variables.len(), variable_count);
            assert_eq!(constraints.effects, effects);
            assert_eq!(constraints.applications, applications);
            constraints.trial(|this| {
                this.instantiate_callable(&scheme, 1, span(), CallableApplicationScope::Invocation).unwrap();
                Ok(())
            }).unwrap();
            assert_eq!(constraints.variables.len(), variable_count);
            assert_eq!(constraints.effects, effects);
            assert_eq!(constraints.applications, applications);
        });
    }

    #[test]
    fn callable_requirement_instances_reject_captured_effect_and_missing_evidence() {
        SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let callee = constraints.fresh_at_level(span(), 1);
            let value = constraints.fresh_at_level(span(), 1);
            let mut requirement = application(&mut constraints, callee.clone(), vec![(StaticArgumentKind::Positional, value.clone())], CallableKindRequirement::Effectful);
            requirement.effects = EffectContract::variable(constraints.fresh_effect_at_level(span(), 0));
            constraints.register_callable_application(owner(), requirement).unwrap();
            let CallableApplicationScope::Declaration(declaration) = owner() else { unreachable!() };
            assert!(matches!(constraints.generalize_callable_body_with_calls::<crate::source::Span>(&[callee.clone(), value, callee], &BTreeMap::new(), &BTreeMap::new(), &BTreeMap::new(), 0, declaration),
                Err(super::super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedEffects))));
            let mut constraints = TypeConstraints::default();
            let scheme = generic_application_scheme(&mut constraints);
            assert!(matches!(crate::sema::schemes::CallInstantiation::new(&scheme, BTreeMap::new(), BTreeMap::new(), BTreeMap::new(), Vec::new(), Vec::new(), Vec::new(), &[], &[], &[], &[], &[], &[], &[], &BTreeMap::new()),
                Err(crate::sema::schemes::SchemeBuildError::UnboundTypeParameter)));
            let instance = constraints.instantiate_callable(&scheme, 1, span(), CallableApplicationScope::Invocation).unwrap();
            assert!(matches!(constraints.resolve_call_instantiation(&scheme, &instance), Err(super::super::SchemeOperationError::Build(_))));
        });
    }

    #[test]
    fn callable_application_constraints_preserve_full_default_labels_and_alias_identity() {
        SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            for (name, ty) in [("payload", Type::Int), ("input", Type::Str)].into_iter().rev() {
                let callee = constraints.fresh_at_level(span(), 1);
                let alias = constraints.fresh_at_level(span(), 1);
                let requirement = application(&mut constraints, alias.clone(), vec![(StaticArgumentKind::Positional, ty.clone())], CallableKindRequirement::Pure);
                let id = constraints.register_callable_application(owner(), requirement.clone()).unwrap();
                constraints.solve_callable_applications().unwrap();
                constraints.constrain(&alias, &callee, span()).unwrap();
                let actual = callback(true, name, ty.clone(), true, false);
                constraints.constrain(&callee, &actual, span()).unwrap();
                constraints.solve_callable_applications().unwrap();
                assert_eq!(constraints.resolve(&callee).unwrap(), actual);
                assert_eq!(constraints.resolve(&alias).unwrap(), actual);
                assert_eq!(constraints.resolve(&requirement.result).unwrap(), ty);
                assert_eq!(constraints.resolve_effect(&requirement.effects).unwrap().closed(), Some(EffectSet::default()));
                assert_eq!(constraints.applications.entries[&id].binding.as_ref().unwrap().layout(), &StaticArgumentBinding { argument_slots: vec![0], omitted_slots: vec![1] });
            }
        });
    }

    #[test]
    fn callable_application_arguments_bind_resolved_composite_inputs() {
        SymbolOwner::new().with_current(|| {
            for item in [Type::Int, Type::Str] {
                let mut constraints = TypeConstraints::default();
                let selected = Arc::new(generic_application_scheme(&mut constraints));
                let caller = EffectDeclarationId { namespace: None, body: Span::new(SourceId::new(0), 2, 3) };
                let instance = constraints.instantiate_callable(&selected, 1, span(), CallableApplicationScope::Declaration(caller)).unwrap();
                let input = constraints.fresh_at_level(span(), 1);
                let payload = constraints.fresh_at_level(span(), 1);
                let actual = Type::Result(Box::new(input.clone()), Box::new(Type::Error));
                constraints.constrain(&instance.params[0], &callback(true, "payload", payload.clone(), false, false), span()).unwrap();
                constraints.constrain(&instance.params[1], &actual, span()).unwrap();
                constraints.solve_callable_applications().unwrap();
                assert!(matches!(constraints.resolve_call_instantiation(&selected, &instance), Err(super::super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedCallableApplication))));
                let calls = BTreeMap::from([(span(), crate::sema::value_materialization::InstantiationEvidence::Invocation((Arc::clone(&selected), instance.clone())))]);
                let (_, _, _, published) = constraints.generalize_callable_body_with_calls(std::slice::from_ref(&input), &BTreeMap::new(), &BTreeMap::new(), &calls, 0, caller).unwrap();
                assert_eq!(published[&span()].invocation().unwrap().application_bindings(), &[crate::sema::schemes::CallableApplicationBinding::Forwarded(0)]);
                constraints.constrain(&input, &item, span()).unwrap();
                constraints.solve_callable_applications().unwrap();
                let expected = Type::Result(Box::new(item), Box::new(Type::Error));
                assert_eq!(constraints.resolve(&payload).unwrap(), expected);
                let resolved = constraints.resolve_call_instantiation(&selected, &instance).unwrap();
                assert_eq!(resolved.application_bindings(), &[crate::sema::schemes::CallableApplicationBinding::Selected(StaticArgumentBinding { argument_slots: vec![0], omitted_slots: vec![] })]);
            }
        });
    }

    #[test]
    fn callable_application_constraints_share_one_monomorphic_callee_and_rollback_failure() {
        SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let callee = constraints.fresh_at_level(span(), 1);
            let first = application(&mut constraints, callee.clone(), vec![(StaticArgumentKind::Positional, Type::Int)], CallableKindRequirement::Pure);
            let second = application(&mut constraints, callee.clone(), vec![(StaticArgumentKind::Positional, Type::Str)], CallableKindRequirement::Pure);
            constraints.register_callable_application(owner(), first.clone()).unwrap();
            constraints.register_callable_application(owner(), second).unwrap();
            constraints.constrain(&callee, &callback(true, "number", Type::Int, false, false), span()).unwrap();
            let before = constraints.clone();
            assert!(matches!(constraints.solve_callable_applications(), Err(CallableApplicationError::Constraint(_))));
            assert_eq!(constraints.applications, before.applications);
            assert_eq!(constraints.effects, before.effects);
            assert_eq!(constraints.resolve(&first.result).unwrap(), first.result);
            assert_eq!(constraints.resolve_effect(&first.effects).unwrap(), first.effects);
        });
    }

    #[test]
    fn callable_application_constraints_use_actual_rest_and_reject_names_missing_formals_and_kind() {
        SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let actual = callback(true, "numbers", Type::Int, false, true);
            let requirement = application(&mut constraints, actual.clone(), vec![(StaticArgumentKind::Positional, Type::Int), (StaticArgumentKind::Positional, Type::Int)], CallableKindRequirement::Effectful);
            let id = constraints.register_callable_application(owner(), requirement.clone()).unwrap();
            constraints.solve_callable_applications().unwrap();
            assert_eq!(constraints.resolve(&requirement.result).unwrap(), Type::List(Box::new(Type::Int)));
            assert_eq!(constraints.applications.entries[&id].binding.as_ref().unwrap().layout().argument_slots, [0, 0]);
            for (callee, kind, values) in [
                (callback(true, "payload", Type::Int, false, false), CallableKindRequirement::Pure, vec![(StaticArgumentKind::Named(Name::intern("wrong")), Type::Int)]),
                (callback(true, "payload", Type::Int, false, false), CallableKindRequirement::Pure, Vec::new()),
                (callback(false, "payload", Type::Int, false, false), CallableKindRequirement::Pure, vec![(StaticArgumentKind::Positional, Type::Int)]),
            ] {
                let mut constraints = TypeConstraints::default();
                let requirement = application(&mut constraints, callee, values, kind);
                constraints.register_callable_application(owner(), requirement).unwrap();
                assert!(matches!(constraints.solve_callable_applications(), Err(CallableApplicationError::Layout(_) | CallableApplicationError::Kind { .. })));
            }
        });
    }

    #[test]
    fn callable_application_constraints_retain_full_actual_proc_effects_and_reject_foreign_cells() {
        SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let actual = callback(false, "payload", Type::Int, true, false);
            let requirement = application(&mut constraints, actual, vec![(StaticArgumentKind::Positional, Type::Int)], CallableKindRequirement::Effectful);
            constraints.register_callable_application(owner(), requirement.clone()).unwrap();
            constraints.solve_callable_applications().unwrap();
            assert_eq!(constraints.resolve_effect(&requirement.effects).unwrap().closed(), Some(EffectSet::new([Effect::Env, Effect::Error])));
            let foreign = TypeConstraints::default().fresh(span());
            let requirement = application(&mut constraints, foreign, Vec::new(), CallableKindRequirement::Effectful);
            assert!(matches!(constraints.register_callable_application(owner(), requirement), Err(CallableApplicationError::Resolution(ConstraintResolutionError::ForeignVariable))));
        });
    }

    #[test]
    fn callable_application_constraint_trials_restore_ownership_layout_watchers_and_panic() {
        SymbolOwner::new().with_current(|| {
            for panic in [false, true] {
                let mut constraints = TypeConstraints::default();
                let before = constraints.clone();
                let mut id = None;
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| constraints.trial(|constraints| {
                    let callee = constraints.fresh_at_level(span(), 1);
                    let requirement = application(constraints, callee.clone(), vec![(StaticArgumentKind::Positional, Type::Int)], CallableKindRequirement::Effectful);
                    id = Some(constraints.register_callable_application(owner(), requirement).unwrap());
                    constraints.constrain(&callee, &callback(true, "payload", Type::Int, true, false), span())?;
                    constraints.solve_callable_applications().unwrap();
                    if panic { panic!("failed application candidate"); }
                    constraints.constrain(&Type::Int, &Type::Str, span())
                })));
                if panic { assert!(outcome.is_err()); } else { assert_eq!(outcome.unwrap(), Ok(false)); }
                assert_eq!(constraints.applications, before.applications);
                assert_eq!(constraints.effects, before.effects);
                assert!(!constraints.applications.entries.contains_key(&id.unwrap()));
            }
        });
    }
}

#[derive(Clone, Debug)]
pub(super) enum ApplicationChange {
    Fresh(CallableApplicationId),
    Source((CallableApplicationScope, crate::source::Span), CallableApplicationId),
    Binding(CallableApplicationId, Option<CallableApplicationBinding>),
    ReturnEnvelopes(CallableApplicationId, std::sync::Arc<[crate::sema::callable_application::ProcReturnEnvelope<Type>]>),
    Watcher(ApplicationDependency, CallableApplicationId),
    Queued(CallableApplicationId),
    Dequeued(CallableApplicationId),
}

impl CallableApplications {
    pub(super) fn rollback(&mut self, change: ApplicationChange) {
        match change {
            ApplicationChange::Fresh(id) => {
                let application = self.entries.remove(&id).expect("application belongs to its journal");
                let scope = self.scopes.get_mut(&application.owner).expect("application scope belongs to its journal");
                assert_eq!(scope.pop(), Some(id));
                if scope.is_empty() { self.scopes.remove(&application.owner); }
            }
            ApplicationChange::Source(site, id) => { assert_eq!(self.source_sites.remove(&site), Some(id)); }
            ApplicationChange::Binding(id, binding) => {
                self.entries.get_mut(&id).expect("application belongs to its journal").binding = binding;
            }
            ApplicationChange::ReturnEnvelopes(id, envelopes) => {
                self.entries.get_mut(&id).expect("application belongs to its journal").application.invocation_mut().expect("return envelope belongs to an invocation").proc_returns = envelopes;
            }
            ApplicationChange::Watcher(dependency, id) => {
                let watchers = self.watchers.get_mut(&dependency).expect("application watcher belongs to its journal");
                assert!(watchers.remove(&id));
                if watchers.is_empty() { self.watchers.remove(&dependency); }
            }
            ApplicationChange::Queued(id) => {
                assert_eq!(self.pending.pop_back(), Some(id));
                assert!(self.queued.remove(&id));
            }
            ApplicationChange::Dequeued(id) => {
                self.pending.push_front(id);
                assert!(self.queued.insert(id));
            }
        }
    }

    fn enqueue(
        &mut self,
        id: CallableApplicationId,
        changes: &mut Vec<ConstraintChange>,
        counters: &super::ConstraintCounters,
    ) {
        if self.queued.insert(id) {
            self.pending.push_back(id);
            changes.push(ConstraintChange::Application(ApplicationChange::Queued(id)));
            counters.record_application_worklist_wakeup();
        }
    }

    fn watch(&mut self, dependency: ApplicationDependency, id: CallableApplicationId, changes: &mut Vec<ConstraintChange>) {
        if self.watchers.entry(dependency).or_default().insert(id) {
            changes.push(ConstraintChange::Application(ApplicationChange::Watcher(dependency, id)));
        }
    }

    pub(super) fn enqueue_changes(
        &mut self,
        from: usize,
        changes: &mut Vec<ConstraintChange>,
        counters: &super::ConstraintCounters,
    ) {
        let dependencies = changes[from..].iter().filter_map(|change| match change {
            ConstraintChange::Type(id, _) | ConstraintChange::FreshType(id) => Some(ApplicationDependency::Type(*id)),
            ConstraintChange::Row(id, _) => Some(ApplicationDependency::Row(*id)),
            ConstraintChange::Effect(_) | ConstraintChange::Application(_) | ConstraintChange::Operation(_) | ConstraintChange::Method(_) | ConstraintChange::Wrapper(_) | ConstraintChange::ConstructorDefault(_) => None,
        }).collect::<BTreeSet<_>>();
        for dependency in dependencies {
            let watchers = self.watchers.get(&dependency).into_iter().flatten().copied().collect::<Vec<_>>();
            for id in watchers {
                if self.entries.get(&id).is_some_and(|application| {
                    application.binding.as_ref().is_none_or(CallableApplicationBinding::arguments_pending)
                        || application.application.invocation().is_some_and(|application| !application.proc_returns.is_empty())
                }) {
                    self.enqueue(id, changes, counters);
                }
            }
        }
    }
}

impl TypeConstraints {
    pub(super) fn known_nullable_argument_source(
        &self,
        instance: &super::InstantiatedScheme,
        expected: &Type,
    ) -> Result<bool, ConstraintResolutionError> {
        let expected = self.resolve(expected)?;
        for id in &instance.applications {
            let application = &self.applications.entries.get(id).ok_or(ConstraintResolutionError::ForeignVariable)?.application;
            let CallableApplication::KnownArguments(application) = application else { continue; };
            let parameters = application.parameters.iter().map(|parameter| parameter.layout).collect::<Vec<_>>();
            let binding = crate::sema::arguments::bind_parameter_layout(&parameters,
                application.arguments.iter().map(|argument| (argument.kind, argument.span)))
                .map_err(|_| ConstraintResolutionError::ForeignVariable)?;
            for (argument, &slot) in application.arguments.iter().zip(&binding.argument_slots) {
                let parameter = &application.parameters[slot];
                let required = if parameter.layout.rest && argument.kind != StaticArgumentKind::Splice {
                    let Type::List(item) = &parameter.ty else { return Err(ConstraintResolutionError::ForeignVariable); };
                    item.as_ref()
                } else { &parameter.ty };
                if matches!(self.resolve(required)?, Type::Optional(_)) && self.resolve(&argument.ty)? == expected { return Ok(true); }
            }
        }
        Ok(false)
    }

    pub(crate) fn callable_application_reference(&self, id: CallableApplicationId) -> Result<CallableApplicationReference, CallableApplicationError> {
        let application = self.applications.entries.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?;
        Ok(CallableApplicationReference { scope: application.owner, ordinal: application.ordinal })
    }

    pub(crate) fn settled_callable_application_binding(&self, id: CallableApplicationId) -> Result<Option<StaticArgumentBinding>, CallableApplicationError> {
        let application = self.applications.entries.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?;
        Ok(match application.binding.as_ref() {
            Some(CallableApplicationBinding::Settled(binding)) => Some(binding.clone()),
            Some(CallableApplicationBinding::PendingArguments(_)) | None => None,
        })
    }

    pub(crate) fn referenced_callable_application(&self, reference: CallableApplicationReference) -> Result<CallableApplicationId, CallableApplicationError> {
        self.applications.scopes.get(&reference.scope).and_then(|scope| scope.get(reference.ordinal as usize)).copied()
            .ok_or_else(|| ConstraintResolutionError::ForeignVariable.into())
    }

    pub(crate) fn referenced_callable_application_result(&self, reference: CallableApplicationReference) -> Result<crate::sema::types::Type, ConstraintResolutionError> {
        let id = self.applications.scopes.get(&reference.scope).and_then(|scope| scope.get(reference.ordinal as usize)).copied()
            .ok_or(ConstraintResolutionError::ForeignVariable)?;
        let application = self.applications.entries.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?;
        let invocation = application.application.invocation().ok_or(ConstraintResolutionError::ForeignVariable)?;
        self.resolve(&invocation.result)
    }

    pub(crate) fn source_callable_application_reference(&self, scope: CallableApplicationScope, span: crate::source::Span) -> Result<Option<CallableApplicationReference>, CallableApplicationError> {
        self.applications.source_sites.get(&(scope, span)).copied()
            .filter(|id| self.applications.entries[id].application.invocation().is_some())
            .map(|id| self.callable_application_reference(id)).transpose()
    }

    pub(crate) fn proc_return_envelope(&self, reference: CallableApplicationReference, span: crate::source::Span) -> Result<u32, CallableApplicationError> {
        let id = self.referenced_callable_application(reference)?;
        let entry = &self.applications.entries[&id];
        let invocation = entry.application.invocation().ok_or(ConstraintResolutionError::ForeignVariable)?;
        let index = invocation.proc_returns.iter().position(|envelope| envelope.span == span)
            .ok_or(ConstraintResolutionError::ForeignVariable)?;
        u32::try_from(index).map_err(|_| ConstraintResolutionError::TypeDepth.into())
    }

    pub(crate) fn generalized_application_ordinal(&self, reference: CallableApplicationReference, owner: EffectDeclarationId, callable: &crate::sema::schemes::CallableScheme, scope: &super::CallableGeneralizationScope) -> Result<Option<u32>, super::SchemeOperationError> {
        if scope.owner != CallableApplicationScope::Declaration(owner) {
            return Err(ConstraintResolutionError::ForeignVariable.into());
        }
        self.validate_generalized_applications(callable, scope)?;
        let id = self.referenced_callable_application(reference).map_err(super::SchemeOperationError::Application)?;
        if scope.application_sources.get(&id) == Some(&reference) {
            return scope.applications.get(&id).copied().map(Some)
                .ok_or(super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedCallableApplication));
        }
        if reference.scope == scope.owner && self.applications.entries[&id].binding.is_some() { return Ok(None); }
        Err(ConstraintResolutionError::ForeignVariable.into())
    }

    pub(crate) fn has_deferred_proc_returns(&self, owner: EffectDeclarationId) -> bool {
        self.applications.scopes.get(&CallableApplicationScope::Declaration(owner)).into_iter().flatten()
            .any(|id| self.applications.entries[id].application.invocation().is_some_and(|application| !application.proc_returns.is_empty()))
    }

    pub(crate) fn source_callable_application(&self, scope: CallableApplicationScope, span: crate::source::Span) -> Option<&CallableInvocation<Type, super::EffectVariableId>> {
        self.applications.source_sites.get(&(scope, span)).and_then(|id| self.applications.entries.get(id)).and_then(|entry| entry.application.invocation())
    }

    pub(crate) fn register_source_callable_application(
        &mut self,
        scope: CallableApplicationScope,
        application: CallableInvocation<Type, super::EffectVariableId>,
    ) -> Result<CallableApplicationId, CallableApplicationError> {
        let site = (scope, application.span);
        self.commit_constraint_transaction(|this| {
            if let Some(&id) = this.applications.source_sites.get(&site) {
                let previous = this.applications.entries[&id].application.invocation().cloned().ok_or(CallableApplicationError::SourceShape(application.span))?;
                if previous.kind != application.kind || previous.arguments.len() != application.arguments.len()
                    || previous.arguments.iter().zip(application.arguments.iter()).any(|(old, new)| old.kind != new.kind || old.span != new.span)
                { return Err(CallableApplicationError::SourceShape(application.span)); }
                this.apply_journaled(|this, changes| {
                    for (old, new) in std::iter::once((&previous.callee, &application.callee))
                        .chain(std::iter::once((&previous.result, &application.result)))
                        .chain(previous.arguments.iter().zip(application.arguments.iter()).map(|(old, new)| (&old.ty, &new.ty)))
                    {
                        this.constrain_inner(old, new, application.span, false, 0, changes)
                            .map_err(|(expected, actual, resolution_error)| CallableApplicationError::Constraint(ConstraintConflict {
                                expected, actual, contribution: application.span, initializer: None, established: None, resolution_error,
                            }))?;
                    }
                    this.constrain_effect_inner(super::EffectPredicate::Equal(previous.effects.clone(), application.effects.clone()), changes).map_err(CallableApplicationError::Effects)?;
                    this.applications.enqueue(id, changes, &this.counters);
                    Ok::<_, CallableApplicationError>(())
                })?;
                return Ok(id);
            }
            let id = this.register_callable_application(scope, application)?;
            this.apply_journaled(|this, changes| {
                assert!(this.applications.source_sites.insert(site, id).is_none());
                changes.push(ConstraintChange::Application(ApplicationChange::Source(site, id)));
                Ok::<_, CallableApplicationError>(())
            })?;
            Ok(id)
        })
    }

    pub(crate) fn register_known_call_arguments(
        &mut self,
        scope: CallableApplicationScope,
        arguments: KnownCallArguments<Type>,
    ) -> Result<CallableApplicationId, CallableApplicationError> {
        let site = (scope, arguments.span);
        self.commit_constraint_transaction(|this| {
            if let Some(&id) = this.applications.source_sites.get(&site) {
                let previous = this.applications.entries[&id].application.clone();
                if previous.arguments().len() != arguments.arguments.len()
                    || previous.arguments().iter().zip(arguments.arguments.iter()).any(|(left, right)| left.kind != right.kind || left.span != right.span)
                { return Err(CallableApplicationError::SourceShape(arguments.span)); }
                this.apply_journaled(|this, changes| {
                    match &previous {
                        CallableApplication::KnownArguments(previous) => {
                            if previous.parameters.len() != arguments.parameters.len()
                                || previous.parameters.iter().zip(arguments.parameters.iter()).any(|(left, right)| left.layout != right.layout)
                            { return Err(CallableApplicationError::SourceShape(arguments.span)); }
                            for (left, right) in previous.parameters.iter().zip(arguments.parameters.iter()) {
                                this.constrain_inner(&left.ty, &right.ty, arguments.span, false, 0, changes)
                                    .map_err(|(expected, actual, resolution_error)| CallableApplicationError::Constraint(ConstraintConflict {
                                        expected, actual, contribution: arguments.span, initializer: None, established: None, resolution_error,
                                    }))?;
                            }
                        }
                        CallableApplication::Invocation(_) => {}
                    }
                    for (left, right) in previous.arguments().iter().zip(arguments.arguments.iter()) {
                        this.constrain_inner(&left.ty, &right.ty, arguments.span, false, 0, changes)
                            .map_err(|(expected, actual, resolution_error)| CallableApplicationError::Constraint(ConstraintConflict {
                                expected, actual, contribution: arguments.span, initializer: None, established: None, resolution_error,
                            }))?;
                    }
                    this.applications.enqueue(id, changes, &this.counters);
                    Ok::<_, CallableApplicationError>(())
                })?;
                return Ok(id);
            }
            let id = this.register_callable_application(scope, arguments)?;
            this.apply_journaled(|this, changes| {
                assert!(this.applications.source_sites.insert(site, id).is_none());
                changes.push(ConstraintChange::Application(ApplicationChange::Source(site, id)));
                Ok::<_, CallableApplicationError>(())
            })?;
            Ok(id)
        })
    }


    pub(crate) fn defer_proc_application_return(
        &mut self,
        id: CallableApplicationId,
        propagated_errors: &[Type],
        span: crate::source::Span,
        level: u32,
    ) -> Result<Type, CallableApplicationError> {
        if !self.applications.entries.contains_key(&id) { return Err(ConstraintResolutionError::ForeignVariable.into()); }
        self.commit_constraint_transaction(|this| {
            let invocation = this.applications.entries[&id].application.invocation().ok_or(CallableApplicationError::SourceShape(span))?;
            if let Some(previous) = invocation.proc_returns.iter().find(|envelope| envelope.span == span).cloned() {
                if previous.propagated_errors.len() != propagated_errors.len() { return Err(CallableApplicationError::SourceShape(span)); }
                this.apply_journaled(|this, changes| {
                    for (expected, actual) in previous.propagated_errors.iter().zip(propagated_errors) {
                        this.constrain_inner(expected, actual, span, false, 0, changes)
                            .map_err(|(expected, actual, resolution_error)| CallableApplicationError::Constraint(ConstraintConflict {
                                expected, actual, contribution: span, initializer: None, established: None, resolution_error,
                            }))?;
                    }
                    Ok::<_, CallableApplicationError>(())
                })?;
                return Ok(previous.result);
            }
            let result = this.fresh_at_level(span, level);
            let envelope = crate::sema::callable_application::ProcReturnEnvelope {
                result: result.clone(), propagated_errors: propagated_errors.to_vec().into(), span,
            };
            this.apply_journaled(|this, changes| {
                let application = this.applications.entries.get_mut(&id).expect("application belongs to its owner").application.invocation_mut().ok_or(CallableApplicationError::SourceShape(span))?;
                let mut envelopes = application.proc_returns.to_vec();
                envelopes.push(envelope);
                let previous = std::mem::replace(&mut application.proc_returns, envelopes.into());
                changes.push(ConstraintChange::Application(ApplicationChange::ReturnEnvelopes(id, previous)));
                let scoped = &this.applications.entries[&id];
                for dependency in this.application_dependencies(&scoped.application)? { this.applications.watch(dependency, id, changes); }
                this.applications.enqueue(id, changes, &this.counters);
                Ok::<_, CallableApplicationError>(())
            })?;
            Ok(result)
        })
    }


    pub(super) fn callable_application_bindings(&self, instance: &super::InstantiatedRequirements, generalization: Option<&super::Generalization>) -> Result<Vec<crate::sema::schemes::CallableApplicationBinding>, super::SchemeOperationError> {
        instance.applications.iter().map(|id| {
            if let Some(binding) = self.settled_callable_application_binding(*id).map_err(super::SchemeOperationError::Application)? {
                return Ok(crate::sema::schemes::CallableApplicationBinding::Selected(binding));
            }
            generalization.and_then(|generalization| generalization.applications.get(id)).copied()
                .map(crate::sema::schemes::CallableApplicationBinding::Forwarded)
                .ok_or(super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedCallableApplication))
        }).collect()
    }


    pub(super) fn generalize_callable_requirements(
        &self,
        owners: &BTreeSet<CallableApplicationScope>,
        environment_level: u32,
        generalization: &mut super::Generalization,
    ) -> Result<crate::sema::schemes::CallableRequirements, super::SchemeOperationError> {
        let mut applications = Vec::new();
        for (&id, entry) in self.applications.entries.iter().filter(|(_, entry)| owners.contains(&entry.owner)) {
            if matches!(entry.application, CallableApplication::KnownArguments(_))
                && matches!(entry.binding, Some(CallableApplicationBinding::Settled(_)))
            {
                let types = entry.application.types().map(|ty| self.resolve(ty)).collect::<Result<Vec<_>, _>>()?;
                if types.iter().all(|ty| !ty.contains_inference() && !ty.contains_recovery()) { continue; }
            }
            generalization.applications.insert(id, u32::try_from(applications.len()).expect("application ordinals fit their existing source range"));
            applications.push(self.generalize_callable_application(id, environment_level, generalization)?);
        }
        let operations = self.generalize_sealed_operations(owners, environment_level, generalization)?;
        let methods = self.generalize_method_families(owners, environment_level, generalization)?;
        let wrappers = self.generalize_wrappers(owners, environment_level, generalization)?;
        let guard_calls = self.generalize_guard_calls(owners, environment_level, generalization)?;
        let iteration_parameters = self.generalized_iterations(environment_level, generalization)?;
        let effect_predicates = self.generalize_effect_predicates(environment_level, generalization)?;
        let mut requirements = crate::sema::schemes::CallableRequirements {
            iteration_parameters, applications, operations, wrappers, effect_predicates, effect_parameters: generalization.effect_parameters.clone(),
            map_key_parameters: self.generalized_map_keys(generalization)?,
            conversion_parameters: self.generalized_conversions(generalization)?, methods,
            guards: Vec::new(),
            guard_calls,
        };
        requirements.guards = self.generalize_guard_requirements(owners, environment_level, generalization)?;
        Ok(requirements)
    }


    pub(super) fn generalize_callable_application(
        &self,
        id: CallableApplicationId,
        environment_level: u32,
        generalization: &mut super::Generalization,
    ) -> Result<CallableApplication<crate::sema::schemes::SchemeType, crate::sema::schemes::SchemeEffectParameterId>, super::SchemeOperationError> {
        let application = &self.applications.entries.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?.application;
        application.try_map_types(|ty| self.generalize_type(ty, environment_level, generalization, 0))?
            .try_map_effects(|effects| self.generalize_effect_contract(&effects, environment_level, generalization))
    }

    fn application_dependencies(&self, application: &CallableApplication<Type, super::EffectVariableId>) -> Result<BTreeSet<ApplicationDependency>, ConstraintResolutionError> {
        self.application_type_dependencies(application.types())
    }

    pub(super) fn application_type_dependencies<'a>(&self, types: impl Iterator<Item = &'a Type>) -> Result<BTreeSet<ApplicationDependency>, ConstraintResolutionError> {
        enum Pending<'a> { Type(&'a Type, usize), Row(RowTail, usize) }
        let mut pending = types.map(|ty| Pending::Type(ty, 0)).collect::<Vec<_>>();
        let mut dependencies = BTreeSet::new();
        while let Some(node) = pending.pop() {
            match node {
                Pending::Type(ty, depth) => {
                    if depth > MAX_TYPE_DEPTH { return Err(ConstraintResolutionError::TypeDepth); }
                    if let Type::Inference(id) = ty {
                        let root = self.representative_root(*id)?;
                        if !dependencies.insert(ApplicationDependency::Type(root)) { continue; }
                        let Some(Variable::Representative(variable)) = self.variables.get(&root) else { unreachable!() };
                        match &variable.binding {
                            Some(Binding::Type { ty, .. }) => pending.push(Pending::Type(ty, depth)),
                            Some(Binding::Record(record)) => {
                                pending.extend(record.shape.fields.values().map(|ty| Pending::Type(ty, depth + 1)));
                                pending.push(Pending::Row(record.shape.tail.clone(), depth));
                            }
                            #[cfg(test)]
                            Some(Binding::Rigid { .. }) => {},
                            None => {},
                        }
                    } else {
                        ty.for_each_child(|child| pending.push(Pending::Type(child, depth + 1)));
                    }
                }
                Pending::Row(tail, depth) => {
                    if depth > MAX_TYPE_DEPTH { return Err(ConstraintResolutionError::TypeDepth); }
                    let RowTail::Variable(id) = tail else { continue; };
                    let root = self.row_representative_root(id)?;
                    if !dependencies.insert(ApplicationDependency::Row(root)) { continue; }
                    let Some(super::RowVariable::Representative(variable)) = self.rows.get(&root) else { unreachable!() };
                    if let Some(binding) = &variable.binding {
                        pending.extend(binding.shape.fields.values().map(|ty| Pending::Type(ty, depth + 1)));
                        pending.push(Pending::Row(binding.shape.tail.clone(), depth));
                    }
                }
            }
        }
        Ok(dependencies)
    }

    pub(crate) fn register_callable_application(
        &mut self, owner: CallableApplicationScope, application: impl Into<CallableApplication<Type, super::EffectVariableId>>,
    ) -> Result<CallableApplicationId, CallableApplicationError> {
        let application = application.into();
        let dependencies = self.application_dependencies(&application)?;
        if let Some(invocation) = application.invocation() { self.resolve_effect(&invocation.effects)?; }
        let id = CallableApplicationId(NEXT_APPLICATION.fetch_add(1, Ordering::Relaxed));
        self.apply_journaled(|this, changes| {
            let scope = this.applications.scopes.entry(owner).or_default();
            let ordinal = u32::try_from(scope.len()).map_err(|_| ConstraintResolutionError::TypeDepth)?;
            scope.push(id);
            assert!(this.applications.entries.insert(id, ScopedCallableApplication { owner, ordinal, application, binding: None }).is_none());
            changes.push(ConstraintChange::Application(ApplicationChange::Fresh(id)));
            for dependency in dependencies { this.applications.watch(dependency, id, changes); }
            this.applications.enqueue(id, changes, &this.counters);
            Ok::<_, CallableApplicationError>(())
        })?;
        Ok(id)
    }

    pub(crate) fn solve_callable_applications(&mut self) -> Result<(), CallableApplicationError> {
        self.commit_constraint_transaction(|this| {
            loop {
                this.solve_pending_callable_applications()?;
                this.solve_sealed_operations().map_err(CallableApplicationError::Operation)?;
                this.solve_wrapper_operations().map_err(CallableApplicationError::Wrapper)?;
                this.solve_method_families().map_err(CallableApplicationError::Method)?;
                if this.applications.pending.is_empty() && !this.operations.is_pending() && !this.wrappers.is_pending() && !this.methods.is_pending() { return Ok(()); }
            }
        })
    }

    fn solve_pending_callable_applications(&mut self) -> Result<(), CallableApplicationError> {
        self.apply_journaled(|this, changes| {
            while let Some(id) = this.applications.pending.pop_front() {
                assert!(this.applications.queued.remove(&id));
                changes.push(ConstraintChange::Application(ApplicationChange::Dequeued(id)));
                let scoped = this.applications.entries.get(&id).expect("queued application belongs to its owner").clone();
                let application = &scoped.application;
                for dependency in this.application_dependencies(application)? { this.applications.watch(dependency, id, changes); }
                let start = changes.len();
                if let CallableApplication::KnownArguments(application) = application {
                    let parameters = application.parameters.iter().map(|parameter| parameter.layout).collect::<Vec<_>>();
                    let binding = crate::sema::arguments::bind_parameter_layout(&parameters,
                        application.arguments.iter().map(|argument| (argument.kind, argument.span))).map_err(CallableApplicationError::Layout)?;
                    if scoped.binding.as_ref().is_some_and(|previous| previous.layout() != &binding) {
                        return Err(CallableApplicationError::SourceShape(application.span));
                    }
                    let mut pending = false;
                    for (argument, &slot) in application.arguments.iter().zip(&binding.argument_slots) {
                        let parameter = &application.parameters[slot];
                        let required = if parameter.layout.rest && argument.kind != StaticArgumentKind::Splice {
                            let Type::List(item) = &parameter.ty else { return Err(CallableApplicationError::SourceShape(application.span)); };
                            item.as_ref()
                        } else { &parameter.ty };
                        pending |= this.constrain_application_argument(required, &argument.ty, argument.span, changes)?;
                    }
                    let next = if pending { CallableApplicationBinding::PendingArguments(binding) } else { CallableApplicationBinding::Settled(binding) };
                    if scoped.binding.as_ref() != Some(&next) {
                        let previous = this.applications.entries.get_mut(&id).unwrap().binding.replace(next);
                        changes.push(ConstraintChange::Application(ApplicationChange::Binding(id, previous)));
                    }
                    this.applications.enqueue_changes(start, changes, &this.counters);
                    continue;
                }
                let CallableApplication::Invocation(application) = application else { unreachable!() };
                let callee = this.resolve(&application.callee)?;
                if matches!(callee, Type::Inference(_)) {
                    if scoped.binding.is_some() { return Err(CallableApplicationError::SourceShape(application.span)); }
                    continue;
                }
                let Type::Callable(callable) = callee else { return Err(CallableApplicationError::NotCallable(callee)); };
                if scoped.binding.is_none() && !application.kind.accepts(callable.kind) {
                    return Err(CallableApplicationError::Kind { required: application.kind, actual: callable.kind });
                }
                let binding = crate::sema::arguments::bind_argument_layout(&callable.sig.params,
                    application.arguments.iter().map(|argument| (argument.kind, argument.span))).map_err(CallableApplicationError::Layout)?;
                if let Some(previous) = &scoped.binding
                    && previous.layout() != &binding { return Err(CallableApplicationError::SourceShape(application.span)); }
                let mut arguments_pending = false;
                for (argument, &slot) in application.arguments.iter().zip(&binding.argument_slots) {
                    let parameter = &callable.sig.params[slot];
                    let required = if parameter.rest && argument.kind != StaticArgumentKind::Splice {
                        let Type::List(item) = &parameter.ty else { return Err(CallableApplicationError::NotCallable(Type::Callable(callable.clone()))); };
                        item.as_ref()
                    } else { &parameter.ty };
                    arguments_pending |= this.constrain_application_argument(required, &argument.ty, argument.span, changes)?;
                }
                if scoped.binding.is_none() {
                    this.constrain_inner(&application.result, &callable.sig.return_ty, application.span, false, 0, changes)
                        .map_err(|(expected, actual, resolution_error)| CallableApplicationError::Constraint(ConstraintConflict {
                            expected, actual, contribution: application.span, initializer: None, established: None, resolution_error,
                        }))?;
                    let effects = if callable.kind == crate::sema::types::CallableKind::Pure { EffectContract::finite(EffectSet::default()) } else { callable.sig.effects.contract().clone() };
                    this.constrain_effect_inner(super::EffectPredicate::Equal(application.effects.clone(), effects), changes)
                        .map_err(CallableApplicationError::Effects)?;
                }
                let next_binding = if arguments_pending {
                    CallableApplicationBinding::PendingArguments(binding)
                } else {
                    CallableApplicationBinding::Settled(binding)
                };
                if scoped.binding.as_ref() != Some(&next_binding) {
                    let previous = this.applications.entries.get_mut(&id).unwrap().binding.replace(next_binding);
                    changes.push(ConstraintChange::Application(ApplicationChange::Binding(id, previous)));
                }
                for envelope in application.proc_returns.iter() {
                    let value = this.resolve(&application.result)?;
                    if matches!(value, Type::Inference(_)) { continue; }
                    if matches!(value, Type::Unknown | Type::Invalid | Type::Any) { return Err(CallableApplicationError::ReturnShape(value)); }
                    let errors = envelope.propagated_errors.iter().map(|error| this.resolve(error)).collect::<Result<Vec<_>, _>>()?;
                    if errors.iter().any(Type::contains_inference)
                        || (!errors.is_empty() && matches!(&value, Type::Result(_, error) if error.contains_inference())) { continue; }
                    let expected = crate::sema::callable_application::inferred_proc_return_envelope(value, &errors);
                    this.constrain_inner(&envelope.result, &expected, envelope.span, false, 0, changes)
                        .map_err(|(expected, actual, resolution_error)| CallableApplicationError::Constraint(ConstraintConflict {
                            expected, actual, contribution: envelope.span, initializer: None, established: None, resolution_error,
                        }))?;
                }
                this.applications.enqueue_changes(start, changes, &this.counters);
            }
            Ok(())
        })
    }

    fn constrain_application_argument(
        &mut self,
        required: &Type,
        actual: &Type,
        span: crate::source::Span,
        changes: &mut Vec<ConstraintChange>,
    ) -> Result<bool, CallableApplicationError> {
        let required = self.resolve(required)?;
        let actual = self.resolve(actual)?;
        if self.unresolved_method_result(&actual)? { return Ok(true); }
        let target = if let (Type::Optional(payload), Type::Inference(id)) = (&required, &actual) {
            if self.record_binding(*id)?.is_none() { return Ok(true); }
            payload.as_ref()
        } else { &required };
        self.constrain_inner(target, &actual, span, true, 0, changes)
            .map_err(|(expected, actual, resolution_error)| CallableApplicationError::Constraint(ConstraintConflict {
                expected, actual, contribution: span, initializer: None, established: None, resolution_error,
            }))?;
        Ok(self.resolve(&required)?.contains_inference() || self.resolve(&actual)?.contains_inference())
    }
}
