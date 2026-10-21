use super::{Binding, ConstraintChange, ConstraintConflict, ConstraintResolutionError, RowTail, RowVariableId, TypeConstraints, TypeVariableId, Variable, MAX_TYPE_DEPTH};
use crate::sema::arguments::{ArgumentExpansionError, StaticArgumentBinding, StaticArgumentKind};
use crate::sema::callable_application::{CallableApplication, CallableKindRequirement};
use crate::sema::check::EffectDeclarationId;
use crate::sema::effects::{EffectContract, EffectSet};
use crate::sema::types::{CallableKind, Type};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_APPLICATION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct CallableApplicationId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CallableApplicationScope {
    Declaration(EffectDeclarationId),
    Invocation,
}

#[derive(Clone, Debug)]
pub(crate) enum CallableApplicationError {
    Resolution(ConstraintResolutionError),
    Constraint(ConstraintConflict),
    Layout(ArgumentExpansionError),
    Kind { required: CallableKindRequirement, actual: CallableKind },
    NotCallable(Type),
    Effects(super::EffectConstraintError),
}

impl From<ConstraintResolutionError> for CallableApplicationError {
    fn from(error: ConstraintResolutionError) -> Self { Self::Resolution(error) }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ScopedCallableApplication {
    pub(super) owner: CallableApplicationScope,
    pub(super) application: CallableApplication<Type, super::EffectVariableId>,
    pub(super) binding: Option<StaticArgumentBinding>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum ApplicationDependency {
    Type(TypeVariableId),
    Row(RowVariableId),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct CallableApplications {
    pub(super) entries: BTreeMap<CallableApplicationId, ScopedCallableApplication>,
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
        Type::Callable(Arc::new(TypedCallable { pure, sig: CallableType {
            params: vec![CallableParamType { name: Name::intern(name), ty: if rest { Type::List(Box::new(ty.clone())) } else { ty.clone() }, defaulted: false, rest }]
                .into_iter().chain(default.then(|| CallableParamType { name: Name::intern("scale"), ty: Type::Int, defaulted: true, rest: false })).collect(),
            return_ty: Box::new(if rest { Type::List(Box::new(ty)) } else { ty }),
            effects: if pure { CallableEffects::unknown() } else { CallableEffects::finite([Effect::Env, Effect::Error]) },
        } }))
    }
    fn application(constraints: &mut TypeConstraints, callee: Type, values: Vec<(StaticArgumentKind, Type)>, kind: CallableKindRequirement) -> CallableApplication<Type, super::super::EffectVariableId> {
        CallableApplication {
            callee, arguments: values.into_iter().map(|(kind, ty)| CallableApplicationArgument { kind, ty, span: span() }).collect::<Vec<_>>().into(),
            result: constraints.fresh_at_level(span(), 1),
            effects: EffectContract::variable(constraints.fresh_effect_at_level(span(), 1)), kind, span: span(),
        }
    }

    fn generic_application_scheme(constraints: &mut TypeConstraints) -> crate::sema::schemes::CallableScheme {
        let callee = constraints.fresh_at_level(span(), 1);
        let value = constraints.fresh_at_level(span(), 1);
        let application = application(constraints, callee.clone(), vec![(StaticArgumentKind::Positional, value.clone())], CallableKindRequirement::Immediate);
        constraints.register_callable_application(owner(), application).unwrap();
        let CallableApplicationScope::Declaration(declaration) = owner() else { unreachable!() };
        let (generalized, _, _, _) = constraints.generalize_callable_body_with_calls(
            &[callee.clone(), value, callee], &BTreeMap::new(), &BTreeMap::new(), &BTreeMap::new(), 0, declaration,
        ).unwrap();
        assert_eq!(generalized.type_parameters.len(), 3);
        assert_eq!(generalized.requirements.effect_parameters.len(), 1);
        let mut types = generalized.types.into_iter();
        let params = ["action", "value"].into_iter().map(|name| crate::sema::schemes::SchemeParameter {
            name: Name::intern(name), ty: types.next().unwrap(), schema_expectation: None, defaulted: false, rest: false,
        }).collect();
        crate::sema::schemes::CallableScheme::new(generalized.type_parameters, generalized.row_parameters, params, types.next().unwrap(), None, false, generalized.requirements).unwrap()
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
                assert_eq!(evidence.application_bindings(), &[StaticArgumentBinding { argument_slots: vec![0], omitted_slots: vec![1] }]);
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
            let mut requirement = application(&mut constraints, callee.clone(), vec![(StaticArgumentKind::Positional, value.clone())], CallableKindRequirement::Immediate);
            requirement.effects = EffectContract::variable(constraints.fresh_effect_at_level(span(), 0));
            constraints.register_callable_application(owner(), requirement).unwrap();
            let CallableApplicationScope::Declaration(declaration) = owner() else { unreachable!() };
            assert!(matches!(constraints.generalize_callable_body_with_calls(&[callee.clone(), value, callee], &BTreeMap::new(), &BTreeMap::new(), &BTreeMap::new(), 0, declaration),
                Err(super::super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedEffects))));
            let mut constraints = TypeConstraints::default();
            let scheme = generic_application_scheme(&mut constraints);
            assert!(matches!(crate::sema::schemes::CallInstantiation::new(&scheme, BTreeMap::new(), BTreeMap::new(), BTreeMap::new(), Vec::new(), &[], &[], &[]),
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
                assert_eq!(constraints.applications.entries[&id].binding.as_ref().unwrap(), &StaticArgumentBinding { argument_slots: vec![0], omitted_slots: vec![1] });
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
            let requirement = application(&mut constraints, actual.clone(), vec![(StaticArgumentKind::Positional, Type::Int), (StaticArgumentKind::Positional, Type::Int)], CallableKindRequirement::Immediate);
            let id = constraints.register_callable_application(owner(), requirement.clone()).unwrap();
            constraints.solve_callable_applications().unwrap();
            assert_eq!(constraints.resolve(&requirement.result).unwrap(), Type::List(Box::new(Type::Int)));
            assert_eq!(constraints.applications.entries[&id].binding.as_ref().unwrap().argument_slots, [0, 0]);
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
            let requirement = application(&mut constraints, actual, vec![(StaticArgumentKind::Positional, Type::Int)], CallableKindRequirement::Immediate);
            constraints.register_callable_application(owner(), requirement.clone()).unwrap();
            constraints.solve_callable_applications().unwrap();
            assert_eq!(constraints.resolve_effect(&requirement.effects).unwrap().closed(), Some(EffectSet::new([Effect::Env, Effect::Error])));
            let foreign = TypeConstraints::default().fresh(span());
            let requirement = application(&mut constraints, foreign, Vec::new(), CallableKindRequirement::Immediate);
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
                    let requirement = application(constraints, callee.clone(), vec![(StaticArgumentKind::Positional, Type::Int)], CallableKindRequirement::Immediate);
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
    Binding(CallableApplicationId, Option<StaticArgumentBinding>),
    Watcher(ApplicationDependency, CallableApplicationId),
    Queued(CallableApplicationId),
    Dequeued(CallableApplicationId),
}

impl CallableApplications {
    pub(super) fn rollback(&mut self, change: ApplicationChange) {
        match change {
            ApplicationChange::Fresh(id) => { self.entries.remove(&id); }
            ApplicationChange::Binding(id, binding) => {
                self.entries.get_mut(&id).expect("application belongs to its journal").binding = binding;
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

    fn enqueue(&mut self, id: CallableApplicationId, changes: &mut Vec<ConstraintChange>) {
        if self.queued.insert(id) {
            self.pending.push_back(id);
            changes.push(ConstraintChange::Application(ApplicationChange::Queued(id)));
        }
    }

    fn watch(&mut self, dependency: ApplicationDependency, id: CallableApplicationId, changes: &mut Vec<ConstraintChange>) {
        if self.watchers.entry(dependency).or_default().insert(id) {
            changes.push(ConstraintChange::Application(ApplicationChange::Watcher(dependency, id)));
        }
    }

    pub(super) fn enqueue_changes(&mut self, from: usize, changes: &mut Vec<ConstraintChange>) {
        let dependencies = changes[from..].iter().filter_map(|change| match change {
            ConstraintChange::Type(id, _) | ConstraintChange::FreshType(id) => Some(ApplicationDependency::Type(*id)),
            ConstraintChange::Row(id, _) => Some(ApplicationDependency::Row(*id)),
            ConstraintChange::Effect(_) | ConstraintChange::Application(_) => None,
        }).collect::<BTreeSet<_>>();
        for dependency in dependencies {
            let watchers = self.watchers.get(&dependency).into_iter().flatten().copied().collect::<Vec<_>>();
            for id in watchers {
                if self.entries.get(&id).is_some_and(|application| application.binding.is_none()) {
                    self.enqueue(id, changes);
                }
            }
        }
    }
}

impl TypeConstraints {
    pub(super) fn callable_application_bindings(&self, instance: &super::InstantiatedScheme) -> Result<Vec<StaticArgumentBinding>, super::SchemeOperationError> {
        instance.applications.iter().map(|id| self.applications.entries.get(id).and_then(|entry| entry.binding.clone())
            .ok_or(super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedCallableApplication))).collect()
    }


    pub(super) fn generalize_callable_requirements(
        &self,
        owner: EffectDeclarationId,
        environment_level: u32,
        generalization: &mut super::Generalization,
    ) -> Result<crate::sema::schemes::CallableRequirements, super::SchemeOperationError> {
        let mut applications = Vec::new();
        for scoped in self.applications.entries.values().filter(|entry| entry.owner == CallableApplicationScope::Declaration(owner)) {
            let application = &scoped.application;
            let callee = self.generalize_type(&application.callee, environment_level, generalization, 0)?;
            let arguments = application.arguments.iter().map(|argument| Ok(crate::sema::callable_application::CallableApplicationArgument {
                kind: argument.kind,
                ty: self.generalize_type(&argument.ty, environment_level, generalization, 0)?,
                span: argument.span,
            })).collect::<Result<Vec<_>, super::SchemeOperationError>>()?;
            let result = self.generalize_type(&application.result, environment_level, generalization, 0)?;
            let effects = self.generalize_effect_contract(&application.effects, environment_level, generalization)?;
            applications.push(CallableApplication { callee, arguments: arguments.into(), result, effects, kind: application.kind, span: application.span });
        }
        let effect_predicates = self.generalize_effect_predicates(environment_level, generalization)?;
        Ok(crate::sema::schemes::CallableRequirements {
            applications, effect_predicates, effect_parameters: generalization.effect_parameters.clone(),
        })
    }


    fn application_dependencies(&self, application: &CallableApplication<Type, super::EffectVariableId>) -> Result<BTreeSet<ApplicationDependency>, ConstraintResolutionError> {
        enum Pending<'a> { Type(&'a Type, usize), Row(RowTail, usize) }
        let mut pending = std::iter::once(&application.callee).chain(std::iter::once(&application.result))
            .chain(application.arguments.iter().map(|argument| &argument.ty)).map(|ty| Pending::Type(ty, 0)).collect::<Vec<_>>();
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
        &mut self, owner: CallableApplicationScope, application: CallableApplication<Type, super::EffectVariableId>,
    ) -> Result<CallableApplicationId, CallableApplicationError> {
        let dependencies = self.application_dependencies(&application)?;
        self.resolve_effect(&application.effects)?;
        let id = CallableApplicationId(NEXT_APPLICATION.fetch_add(1, Ordering::Relaxed));
        self.apply_journaled(|this, changes| {
            assert!(this.applications.entries.insert(id, ScopedCallableApplication { owner, application, binding: None }).is_none());
            changes.push(ConstraintChange::Application(ApplicationChange::Fresh(id)));
            for dependency in dependencies { this.applications.watch(dependency, id, changes); }
            this.applications.enqueue(id, changes);
            Ok::<_, CallableApplicationError>(())
        })?;
        Ok(id)
    }

    pub(crate) fn solve_callable_applications(&mut self) -> Result<(), CallableApplicationError> {
        self.apply_journaled(|this, changes| {
            while let Some(id) = this.applications.pending.pop_front() {
                assert!(this.applications.queued.remove(&id));
                changes.push(ConstraintChange::Application(ApplicationChange::Dequeued(id)));
                let scoped = this.applications.entries.get(&id).expect("queued application belongs to its owner").clone();
                if scoped.binding.is_some() { continue; }
                let application = &scoped.application;
                for dependency in this.application_dependencies(application)? { this.applications.watch(dependency, id, changes); }
                let callee = this.resolve(&application.callee)?;
                if matches!(callee, Type::Inference(_)) { continue; }
                let Type::Callable(callable) = callee else { return Err(CallableApplicationError::NotCallable(callee)); };
                let kind = if callable.pure { CallableKind::Pure } else { CallableKind::Proc };
                if !application.kind.accepts(kind) { return Err(CallableApplicationError::Kind { required: application.kind, actual: kind }); }
                let binding = crate::sema::arguments::bind_argument_layout(&callable.sig.params,
                    application.arguments.iter().map(|argument| (argument.kind, argument.span))).map_err(CallableApplicationError::Layout)?;
                let start = changes.len();
                for (argument, &slot) in application.arguments.iter().zip(&binding.argument_slots) {
                    let parameter = &callable.sig.params[slot];
                    let required = if parameter.rest && argument.kind != StaticArgumentKind::Splice {
                        let Type::List(item) = &parameter.ty else { return Err(CallableApplicationError::NotCallable(Type::Callable(callable.clone()))); };
                        item.as_ref()
                    } else { &parameter.ty };
                    this.constrain_inner(required, &argument.ty, argument.span, true, 0, changes)
                        .map_err(|(expected, actual, resolution_error)| CallableApplicationError::Constraint(ConstraintConflict {
                            expected, actual, contribution: argument.span, initializer: None, established: None, resolution_error,
                        }))?;
                }
                this.constrain_inner(&application.result, &callable.sig.return_ty, application.span, false, 0, changes)
                    .map_err(|(expected, actual, resolution_error)| CallableApplicationError::Constraint(ConstraintConflict {
                        expected, actual, contribution: application.span, initializer: None, established: None, resolution_error,
                    }))?;
                let effects = if callable.pure { EffectContract::finite(EffectSet::default()) } else { callable.sig.effects.contract().clone() };
                this.constrain_effect_inner(super::EffectPredicate::Equal(application.effects.clone(), effects), changes)
                    .map_err(CallableApplicationError::Effects)?;
                let previous = this.applications.entries.get_mut(&id).unwrap().binding.replace(binding);
                changes.push(ConstraintChange::Application(ApplicationChange::Binding(id, previous)));
                this.applications.enqueue_changes(start, changes);
            }
            Ok(())
        })
    }
}
