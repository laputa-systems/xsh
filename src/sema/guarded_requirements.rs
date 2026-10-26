use super::schemes::{CallInstantiation, CallableRequirements, SchemeBuildError, SchemeEffectParameterId, SchemeRowParameter, SchemeRowParameterId, SchemeType, SchemeTypeParameterId};
use super::types::Type;
use crate::symbol::Name;
use bbtree::unsync::{BBTreeMap as BTreeMap, BBTreeSet as BTreeSet};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct GuardScopeId(u32);

impl GuardScopeId {
    pub(crate) fn from_index(index: usize) -> Result<Self, SchemeBuildError> {
        u32::try_from(index).map(Self).map_err(|_| SchemeBuildError::UnresolvedGuardScope)
    }

    pub(crate) fn index(self) -> usize { self.0 as usize }

    pub(crate) fn raw(self) -> u32 { self.0 }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct GuardCallId(u32);

impl GuardCallId {
    pub(crate) fn from_index(index: usize) -> Result<Self, SchemeBuildError> {
        u32::try_from(index).map(Self).map_err(|_| SchemeBuildError::UnresolvedGuardScope)
    }

    pub(crate) fn index(self) -> usize { self.0 as usize }

    pub(crate) fn raw(self) -> u32 { self.0 }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GuardCallTarget { Scope(GuardScopeId), Call(GuardCallId) }

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GuardCallRequirement {
    pub(crate) source: super::schemes::InstantiationSite<crate::source::Span>,
    pub(crate) target: GuardCallTarget,
    pub(crate) callee: std::sync::Arc<super::schemes::CallableScheme>,
    pub(crate) template: std::sync::Arc<GuardRequirements>,
    pub(crate) type_arguments: BTreeMap<SchemeTypeParameterId, SchemeType>,
    pub(crate) row_arguments: BTreeMap<SchemeRowParameterId, super::schemes::RowEvidence>,
    pub(crate) effect_arguments: BTreeMap<SchemeEffectParameterId, super::effects::EffectContract<SchemeEffectParameterId>>,
}

impl GuardCallRequirement {
    pub(crate) fn validate_target(&self) -> Result<(), SchemeBuildError> {
        let actual = match self.target {
            GuardCallTarget::Scope(scope) => self.callee.requirements().guard(scope),
            GuardCallTarget::Call(call) => self.callee.requirements().guard_calls.get(call.index()).map(|call| call.template.as_ref()),
        }.ok_or(SchemeBuildError::UnresolvedGuardScope)?;
        if actual != self.template.as_ref() { return Err(SchemeBuildError::UnresolvedGuardScope); }
        let free = self.template.free_parameters()?;
        if self.type_arguments.keys().copied().collect::<BTreeSet<_>>() != free.types
            || self.row_arguments.keys().copied().collect::<BTreeSet<_>>() != free.rows
            || self.effect_arguments.keys().copied().collect::<BTreeSet<_>>() != free.effects
        { return Err(SchemeBuildError::UnresolvedGuardScope); }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GuardEffects<V> {
    pub(crate) outward: super::effects::EffectContract<V>,
    pub(crate) local: super::effects::EffectContract<V>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GuardOutputSite {
    Expression(crate::source::Span),
    Statement(crate::source::Span),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GuardOutput<T> {
    pub(crate) outward: T,
    pub(crate) present: T,
    pub(crate) absent: T,
    pub(crate) site: GuardOutputSite,
    pub(crate) nonnull_when: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GuardRequirements {
    pub(crate) scope: GuardScopeId,
    pub(crate) source: SchemeType,
    pub(crate) view: SchemeType,
    pub(crate) subject: crate::source::Span,
    pub(crate) comparison: crate::source::Span,
    pub(crate) reads: Vec<GuardReadRequirement>,
    pub(crate) output: Option<GuardOutput<SchemeType>>,
    pub(crate) effects: Option<GuardEffects<SchemeEffectParameterId>>,
    pub(crate) type_parameters: Vec<SchemeTypeParameterId>,
    pub(crate) row_parameters: Vec<SchemeRowParameter>,
    pub(crate) requirements: Box<CallableRequirements>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GuardReadRequirement {
    pub(crate) source: SchemeType,
    pub(crate) view: SchemeType,
    pub(crate) subject: crate::source::Span,
}

#[derive(Default)]
pub(super) struct GuardDependencies {
    pub(super) types: BTreeSet<SchemeTypeParameterId>,
    pub(super) rows: BTreeSet<SchemeRowParameterId>,
    pub(super) effects: BTreeSet<SchemeEffectParameterId>,
}

impl GuardDependencies {
    fn add_type(&mut self, ty: &SchemeType) {
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            match ty {
                SchemeType::Ground(_) | SchemeType::SchemeCallable(_) => {},
                SchemeType::Parameter(id) => { self.types.insert(*id); },
                SchemeType::List(inner) | SchemeType::Stream(inner) | SchemeType::Set(inner) | SchemeType::Optional(inner) | SchemeType::Validated(_, inner) => pending.push(inner),
                SchemeType::Map(left, right) | SchemeType::Result(left, right) => pending.extend([left.as_ref(), right.as_ref()]),
                SchemeType::Record { fields, tail } => {
                    pending.extend(fields.values());
                    if let super::schemes::SchemeRowTail::Parameter(id) = tail { self.rows.insert(*id); }
                }
                SchemeType::Union(members) => pending.extend(members),
                SchemeType::Callable { monotype, effects, .. } => {
                    pending.push(monotype.return_type());
                    if let Some(schema) = monotype.return_schema() { self.add_schema(schema); }
                    for parameter in monotype.params() {
                        pending.push(&parameter.ty);
                        if let Some(schema) = &parameter.schema_expectation { self.add_schema(schema); }
                    }
                    self.add_effects(effects);
                }
            }
        }
    }

    fn add_schema(&mut self, schema: &super::constants::SchemaExpectation<SchemeType>) {
        for instance in &schema.instances { for argument in &instance.arguments { self.add_type(argument); } }
        for child in schema.children.values() { self.add_schema(child); }
    }

    fn add_effects(&mut self, contract: &super::effects::EffectContract<SchemeEffectParameterId>) {
        if let super::effects::EffectContract::Known(expression) = contract { self.effects.extend(expression.variables().map(|(id, _)| *id)); }
    }
}

impl GuardRequirements {
    pub(super) fn free_parameters(&self) -> Result<GuardDependencies, SchemeBuildError> {
        let mut dependencies = GuardDependencies::default();
        let mut owned = GuardDependencies::default();
        let mut pending = vec![self];
        while let Some(guard) = pending.pop() {
            owned.types.extend(guard.type_parameters.iter().copied());
            owned.rows.extend(guard.row_parameters.iter().map(|parameter| parameter.id));
            owned.effects.extend(guard.requirements.effect_parameters.iter().copied());
            if let Some(effects) = &guard.effects { dependencies.add_effects(&effects.outward); dependencies.add_effects(&effects.local); }
            dependencies.add_type(&guard.source);
            dependencies.add_type(&guard.view);
            if let Some(output) = &guard.output {
                dependencies.add_type(&output.outward);
                dependencies.add_type(&output.present);
                dependencies.add_type(&output.absent);
            }
            for read in &guard.reads { dependencies.add_type(&read.source); dependencies.add_type(&read.view); }
            super::schemes::visit_requirement_terms(&guard.requirements, |term| {
                match term {
                    super::schemes::RequirementTerm::Type(ty) => dependencies.add_type(ty),
                    super::schemes::RequirementTerm::Parameter(id) => { dependencies.types.insert(id); },
                    super::schemes::RequirementTerm::RowParameter(id) => { dependencies.rows.insert(id); },
                    super::schemes::RequirementTerm::Effects(contract) => dependencies.add_effects(contract),
                }
                Ok(())
            })?;
            pending.extend(guard.requirements.guards.iter().map(std::sync::Arc::as_ref));
        }
        dependencies.types.retain(|id| !owned.types.contains(id));
        dependencies.rows.retain(|id| !owned.rows.contains(id));
        dependencies.effects.retain(|id| !owned.effects.contains(id));
        Ok(dependencies)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct GuardBodyScopes {
    pub(crate) expressions: BTreeMap<crate::syntax::arena::ExprId, GuardScopeId>,
    pub(crate) statements: BTreeMap<crate::syntax::arena::StmtId, GuardScopeId>,
    pub(crate) sites: BTreeMap<crate::syntax::arena::ExprId, Vec<GuardBodySite>>,
    pub(crate) statement_sites: BTreeMap<crate::syntax::arena::StmtId, Vec<GuardBodySite>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GuardBodySite {
    Source(GuardScopeId),
    Comparison(GuardScopeId),
    Output(GuardScopeId),
    Read { scope: GuardScopeId, ordinal: u32 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum GuardBinding {
    NonNull(Box<CallInstantiation>),
    Optional(Box<CallInstantiation>),
    Null,
    Call(GuardCallId),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct GuardBindings {
    pub(crate) scopes: Vec<GuardBinding>,
    pub(crate) calls: Vec<GuardBinding>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BinderOwner {
    Root,
    Guard(GuardScopeId),
}

pub(super) struct GuardOwnership<'a> {
    type_owners: BTreeMap<SchemeTypeParameterId, BinderOwner>,
    row_owners: BTreeMap<SchemeRowParameterId, BinderOwner>,
    effect_owners: BTreeMap<SchemeEffectParameterId, BinderOwner>,
    types: BTreeSet<SchemeTypeParameterId>,
    rows: BTreeSet<SchemeRowParameterId>,
    effects: BTreeSet<SchemeEffectParameterId>,
    row_lacks: BTreeMap<SchemeRowParameterId, &'a BTreeSet<Name>>,
    parents: Vec<Option<GuardScopeId>>,
}

impl<'a> GuardOwnership<'a> {
    pub(super) fn new(types: &BTreeSet<SchemeTypeParameterId>, rows: &BTreeSet<SchemeRowParameterId>, effects: &BTreeSet<SchemeEffectParameterId>) -> Self {
        Self {
            type_owners: types.iter().map(|id| (*id, BinderOwner::Root)).collect(),
            row_owners: rows.iter().map(|id| (*id, BinderOwner::Root)).collect(),
            effect_owners: effects.iter().map(|id| (*id, BinderOwner::Root)).collect(),
            types: types.clone(), rows: rows.clone(), effects: effects.clone(),
            row_lacks: BTreeMap::new(), parents: Vec::new(),
        }
    }

    pub(super) fn collect(&mut self, guards: &'a [std::sync::Arc<GuardRequirements>]) -> Result<(), SchemeBuildError> {
        let mut pending = guards.iter().rev().map(|guard| (guard, None, 0)).collect::<Vec<_>>();
        while let Some((guard, parent, depth)) = pending.pop() {
            if depth > super::constraints::MAX_TYPE_DEPTH || guard.scope.index() != self.parents.len() { return Err(SchemeBuildError::UnresolvedGuardScope); }
            self.parents.push(parent);
            let owner = BinderOwner::Guard(guard.scope);
            for parameter in &guard.type_parameters {
                if self.type_owners.insert(*parameter, owner).is_some() { return Err(SchemeBuildError::DuplicateTypeParameter); }
                self.types.insert(*parameter);
            }
            for parameter in &guard.row_parameters {
                if self.row_owners.insert(parameter.id, owner).is_some() { return Err(SchemeBuildError::DuplicateRowParameter); }
                self.rows.insert(parameter.id);
                self.row_lacks.insert(parameter.id, &parameter.lacks);
            }
            for parameter in &guard.requirements.effect_parameters {
                if self.effect_owners.insert(*parameter, owner).is_some() { return Err(SchemeBuildError::DuplicateEffectParameter); }
                self.effects.insert(*parameter);
            }
            pending.extend(guard.requirements.guards.iter().rev().map(|child| (child, Some(guard.scope), depth + 1)));
        }
        Ok(())
    }

    pub(super) fn check_scope(&self, scope: GuardScopeId) -> Result<(), SchemeBuildError> {
        if scope.index() < self.parents.len() { Ok(()) } else { Err(SchemeBuildError::UnresolvedGuardScope) }
    }

    pub(super) fn types(&self) -> &BTreeSet<SchemeTypeParameterId> { &self.types }
    pub(super) fn rows(&self) -> &BTreeSet<SchemeRowParameterId> { &self.rows }
    pub(super) fn effects(&self) -> &BTreeSet<SchemeEffectParameterId> { &self.effects }

    fn visible(&self, owner: BinderOwner, mut scope: Option<GuardScopeId>) -> bool {
        match owner {
            BinderOwner::Root => true,
            BinderOwner::Guard(owner) => {
                while let Some(current) = scope {
                    if current == owner { return true; }
                    scope = self.parents[current.index()];
                }
                false
            },
        }
    }

    pub(super) fn check_type(&self, scope: Option<GuardScopeId>, parameter: SchemeTypeParameterId) -> Result<(), SchemeBuildError> {
        match self.type_owners.get(&parameter) {
            Some(owner) if self.visible(*owner, scope) => Ok(()),
            _ => Err(SchemeBuildError::UnboundTypeParameter),
        }
    }

    pub(super) fn check_types(&self, scope: Option<GuardScopeId>, parameters: &BTreeSet<SchemeTypeParameterId>) -> Result<(), SchemeBuildError> {
        for parameter in parameters { self.check_type(scope, *parameter)?; }
        Ok(())
    }

    pub(super) fn check_rows(&self, scope: Option<GuardScopeId>, parameters: &BTreeSet<SchemeRowParameterId>) -> Result<(), SchemeBuildError> {
        for parameter in parameters {
            match self.row_owners.get(parameter) {
                Some(owner) if self.visible(*owner, scope) => {},
                _ => return Err(SchemeBuildError::UnboundRowParameter),
            }
        }
        Ok(())
    }

    pub(super) fn check_effects(&self, scope: Option<GuardScopeId>, parameters: &BTreeSet<SchemeEffectParameterId>) -> Result<(), SchemeBuildError> {
        for parameter in parameters {
            match self.effect_owners.get(parameter) {
                Some(owner) if self.visible(*owner, scope) => {},
                _ => return Err(SchemeBuildError::UnboundEffectParameter),
            }
        }
        Ok(())
    }

    pub(super) fn check_used(&self, types: &BTreeSet<SchemeTypeParameterId>, rows: &BTreeSet<SchemeRowParameterId>, effects: &BTreeSet<SchemeEffectParameterId>, required_lacks: &BTreeMap<SchemeRowParameterId, BTreeSet<Name>>) -> Result<(), SchemeBuildError> {
        if self.type_owners.iter().any(|(parameter, owner)| matches!(owner, BinderOwner::Guard(_)) && !types.contains(parameter)) { return Err(SchemeBuildError::UnusedTypeParameter); }
        if self.row_owners.iter().any(|(parameter, owner)| matches!(owner, BinderOwner::Guard(_)) && !rows.contains(parameter)) { return Err(SchemeBuildError::UnusedRowParameter); }
        if self.effect_owners.iter().any(|(parameter, owner)| matches!(owner, BinderOwner::Guard(_)) && !effects.contains(parameter)) { return Err(SchemeBuildError::UnusedEffectParameter); }
        if self.row_lacks.iter().any(|(parameter, lacks)| !required_lacks.get(parameter).is_some_and(|required| required.is_subset(lacks))) { return Err(SchemeBuildError::RowLacksMismatch); }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NonNullSource<'a> {
    NonNull(&'a Type),
    Optional(&'a Type),
    Null,
}

pub(crate) fn non_null_source(mut source: &Type) -> Result<NonNullSource<'_>, SchemeBuildError> {
    if source.contains_inference() { return Err(SchemeBuildError::UnresolvedType); }
    if source.contains_recovery() { return Err(SchemeBuildError::RecoveryType); }
    let mut optional = false;
    while let Type::Optional(inner) = source {
        optional = true;
        source = inner;
    }
    match source {
        Type::Null => Ok(NonNullSource::Null),
        Type::Any => Err(SchemeBuildError::DynamicType),
        _ if optional => Ok(NonNullSource::Optional(source)),
        _ => Ok(NonNullSource::NonNull(source)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guard(scope: usize, source: SchemeType, view: SchemeTypeParameterId) -> GuardRequirements {
        let mut requirements = CallableRequirements::default();
        let mut conversions = super::super::schemes::ValueConversionSet::default();
        conversions.insert(super::super::schemes::ValueConversion::Display);
        requirements.conversion_parameters.insert(view, conversions);
        GuardRequirements { scope: GuardScopeId::from_index(scope).unwrap(), source, view: SchemeType::Parameter(view), subject: crate::source::Span::new(crate::source::SourceId::new(0), 0, 1), comparison: crate::source::Span::new(crate::source::SourceId::new(0), 0, 1), reads: Vec::new(), output: None, effects: None, type_parameters: vec![view], row_parameters: Vec::new(), requirements: Box::new(requirements) }
    }

    fn callable(guards: Vec<GuardRequirements>, returned: SchemeType) -> Result<super::super::schemes::CallableScheme, SchemeBuildError> {
        let root = SchemeTypeParameterId(0);
        super::super::schemes::CallableScheme::new(vec![root], Vec::new(), vec![super::super::schemes::SchemeParameter {
            name: Name::intern("value"), ty: SchemeType::Parameter(root), schema_expectation: None, defaulted: false, rest: false,
        }], returned, None, true, CallableRequirements { guards: guards.into_iter().map(std::sync::Arc::new).collect(), ..CallableRequirements::default() })
    }

    #[test]
    fn guarded_binders_are_owned_once_and_visible_only_to_descendants() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let root = SchemeTypeParameterId(0);
            let first = SchemeTypeParameterId(1);
            let second = SchemeTypeParameterId(2);
            let returned = SchemeType::Ground(Type::Str);
            let mut outer = guard(0, SchemeType::Parameter(root), first);
            outer.requirements.guards.push(std::sync::Arc::new(guard(1, SchemeType::Parameter(first), second)));
            assert!(callable(vec![outer], returned.clone()).is_ok());
            let outer = guard(0, SchemeType::Parameter(root), first);
            let sibling = guard(1, SchemeType::Parameter(first), second);
            assert_eq!(callable(vec![outer.clone(), sibling], returned.clone()), Err(SchemeBuildError::UnboundTypeParameter));
            assert_eq!(callable(vec![outer.clone()], SchemeType::Parameter(first)), Err(SchemeBuildError::UnboundTypeParameter));
            assert_eq!(callable(vec![guard(0, SchemeType::Parameter(first), first)], returned.clone()), Err(SchemeBuildError::UnboundTypeParameter));
            assert_eq!(callable(vec![guard(0, SchemeType::Parameter(second), first), guard(1, SchemeType::Parameter(root), second)], returned.clone()), Err(SchemeBuildError::UnboundTypeParameter));
            assert_eq!(callable(vec![guard(0, SchemeType::Parameter(root), root)], returned.clone()), Err(SchemeBuildError::DuplicateTypeParameter));
            let mut unused = outer;
            unused.type_parameters.push(second);
            assert_eq!(callable(vec![unused], returned), Err(SchemeBuildError::UnusedTypeParameter));
        });
    }

    #[test]
    fn non_null_sources_preserve_actual_domains_and_absence() {
        assert_eq!(non_null_source(&Type::Str), Ok(NonNullSource::NonNull(&Type::Str)));
        assert_eq!(non_null_source(&Type::Null), Ok(NonNullSource::Null));
        let optional = Type::Optional(Box::new(Type::Str));
        assert_eq!(non_null_source(&optional), Ok(NonNullSource::Optional(&Type::Str)));
        let nested = Type::Optional(Box::new(optional));
        assert_eq!(non_null_source(&nested), Ok(NonNullSource::Optional(&Type::Str)));
        assert_eq!(non_null_source(&Type::Optional(Box::new(Type::Null))), Ok(NonNullSource::Null));
        let relative = Type::rel_path();
        assert_eq!(non_null_source(&relative), Ok(NonNullSource::NonNull(&relative)));
        let optional_relative = Type::Optional(Box::new(relative.clone()));
        assert_eq!(non_null_source(&optional_relative), Ok(NonNullSource::Optional(&relative)));
        crate::symbol::SymbolOwner::new().with_current(|| {
            let record = Type::Record(std::sync::Arc::new(BTreeMap::from([(Name::intern("name"), Type::Str), (Name::intern("opaque"), Type::Any)])));
            assert_eq!(non_null_source(&record), Ok(NonNullSource::NonNull(&record)));
            let optional = Type::Optional(Box::new(record.clone()));
            assert_eq!(non_null_source(&optional), Ok(NonNullSource::Optional(&record)));
        });
        let union = Type::Union(vec![Type::Str, Type::Int]);
        assert!(crate::sema::types::union_member_error(union.union_members().unwrap()).is_none());
        assert_eq!(non_null_source(&union), Ok(NonNullSource::NonNull(&union)));
    }

    #[test]
    fn non_null_sources_reject_dynamic_recovery_and_unresolved_domains() {
        assert_eq!(non_null_source(&Type::Any), Err(SchemeBuildError::DynamicType));
        assert_eq!(non_null_source(&Type::Optional(Box::new(Type::Any))), Err(SchemeBuildError::DynamicType));
        assert_eq!(non_null_source(&Type::Unknown), Err(SchemeBuildError::RecoveryType));
        assert_eq!(non_null_source(&Type::Invalid), Err(SchemeBuildError::RecoveryType));
        let variable = crate::sema::constraints::TypeConstraints::default().fresh(crate::source::Span::at(crate::source::SourceId::new(0), 0));
        assert_eq!(non_null_source(&variable), Err(SchemeBuildError::UnresolvedType));
    }
}
