use crate::syntax::node::Effect;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallableEffects {
    contract: EffectContract<crate::sema::constraints::EffectVariableId>,
}

/// A checked declaration owns its bound effect parameter ordinals.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CheckedEffectScope {
    owner: Option<crate::sema::check::EffectDeclarationId>,
    parameters: std::sync::Arc<[crate::sema::schemes::SchemeEffectParameterId]>,
}

impl CheckedEffectScope {
    pub fn owner(&self) -> Option<crate::sema::check::EffectDeclarationId> { self.owner }

    pub fn parameter_count(&self) -> usize { self.parameters.len() }

    pub(crate) fn new(owner: crate::sema::check::EffectDeclarationId, parameters: std::sync::Arc<[crate::sema::schemes::SchemeEffectParameterId]>) -> Self {
        Self { owner: Some(owner), parameters }
    }

    pub(crate) fn parameters(&self) -> &[crate::sema::schemes::SchemeEffectParameterId] { &self.parameters }

    pub(crate) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + std::mem::size_of_val(self.parameters.as_ref())
    }
}

/// Bound effect parameters are local to the owning checked callable contract.
/// Concrete projection distinguishes an unknown contract from a bound expression.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedEffects {
    contract: EffectContract<crate::sema::schemes::SchemeEffectParameterId>,
}

impl Default for CheckedEffects {
    fn default() -> Self { Self::unknown() }
}

impl CheckedEffects {
    pub fn finite(effects: impl IntoIterator<Item = Effect>) -> Self {
        Self::from_contract(EffectContract::finite(EffectSet::new(effects)))
    }

    pub fn unknown() -> Self { Self::from_contract(EffectContract::Unknown) }

    pub fn concrete(&self) -> Result<Option<Vec<Effect>>, UnresolvedCallableEffects> {
        self.contract.concrete()
    }

    pub fn is_known(&self) -> bool { matches!(self.contract, EffectContract::Known(_)) }

    pub fn is_symbolic(&self) -> bool {
        matches!(&self.contract, EffectContract::Known(expression) if expression.closed().is_none())
    }

    pub(crate) fn from_clause(effects: Option<impl IntoIterator<Item = Effect>>) -> Self {
        Self::from_contract(EffectContract::from_clause(effects))
    }

    pub(crate) fn from_contract(contract: EffectContract<crate::sema::schemes::SchemeEffectParameterId>) -> Self {
        Self { contract }
    }

    pub(crate) fn contract(&self) -> &EffectContract<crate::sema::schemes::SchemeEffectParameterId> { &self.contract }

    pub(crate) fn retained_bytes(&self) -> usize { self.contract.retained_bytes() }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnresolvedCallableEffects;

impl CallableEffects {
    pub fn finite(effects: impl IntoIterator<Item = Effect>) -> Self {
        Self { contract: EffectContract::finite(EffectSet::new(effects)) }
    }

    pub fn unknown() -> Self {
        Self { contract: EffectContract::Unknown }
    }

    pub fn concrete(&self) -> Result<Option<Vec<Effect>>, UnresolvedCallableEffects> {
        self.contract.concrete()
    }

    pub(crate) fn contract(&self) -> &EffectContract<crate::sema::constraints::EffectVariableId> {
        &self.contract
    }

    pub(crate) fn from_contract(contract: EffectContract<crate::sema::constraints::EffectVariableId>) -> Self {
        Self { contract }
    }

    pub(crate) fn from_clause(effects: Option<impl IntoIterator<Item = Effect>>) -> Self {
        Self::from_contract(EffectContract::from_clause(effects))
    }

    pub(crate) fn has_inference(&self) -> bool {
        matches!(&self.contract, EffectContract::Known(expression) if expression.closed().is_none())
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + match &self.contract {
            EffectContract::Unknown => 0,
            EffectContract::Known(expression) => expression.variables.len()
                * std::mem::size_of::<(crate::sema::constraints::EffectVariableId, EffectSet)>(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum EffectPredicate<V> {
    Equal(EffectContract<V>, EffectContract<V>),
    Included(EffectContract<V>, EffectContract<V>),
    Covered(EffectContract<V>, EffectContract<V>),
    Excluded(EffectContract<V>, EffectSet),
}

impl<V: Ord> EffectPredicate<V> {
    pub(crate) fn satisfaction(&self) -> Option<bool> {
        match self {
            Self::Equal(left, right) => {
                if left == right { return Some(true); }
                if matches!(left, EffectContract::Unknown) || matches!(right, EffectContract::Unknown) {
                    return Some(false);
                }
                Some(left.closed()? == right.closed()?)
            }
            Self::Included(left, right) | Self::Covered(left, right) => {
                if matches!(right, EffectContract::Unknown) { return Some(true); }
                if matches!(left, EffectContract::Unknown) { return Some(false); }
                if left == right { return Some(true); }
                let (EffectContract::Known(left), EffectContract::Known(right)) = (left, right) else { unreachable!() };
                let direct_covers = |required: &Effect| match self {
                    Self::Included(_, _) => right.direct().iter().any(|permission| &permission == required),
                    Self::Covered(_, _) => right.direct().iter().any(|permission| effect_covers(&permission, required)),
                    _ => unreachable!(),
                };
                let covered = left.direct().iter().all(|required| direct_covers(&required)) && left.variables().all(|(variable, mask)| {
                    let retained = right.variables.get(variable).copied().unwrap_or_default();
                    mask.iter().all(|effect| direct_covers(&effect) || retained.iter().any(|candidate| candidate == effect))
                });
                if covered { Some(true) }
                else if left.closed().is_some() && right.closed().is_some() { Some(false) }
                else { None }
            }
            Self::Excluded(contract, excluded) => {
                let EffectContract::Known(expression) = contract else { return Some(false) };
                let prohibited = excluded.prohibited_requirements();
                if !expression.direct().intersection(prohibited).is_empty() { return Some(false); }
                expression.variables().all(|(_, mask)| mask.intersection(prohibited).is_empty()).then_some(true)
            }
        }
    }
}

pub(crate) fn effect_predicate_implies<V: Ord + Eq + Clone>(owned: &EffectPredicate<V>, required: &EffectPredicate<V>) -> bool {
    if owned == required { return true; }
    let (EffectPredicate::Covered(owned_left, owned_right), EffectPredicate::Covered(required_left, required_right)) = (owned, required) else {
        return false;
    };
    let (Some(owned_permissions), Some(required_permissions)) = (owned_right.closed(), required_right.closed()) else {
        return false;
    };
    let available = if owned_left == required_left {
        owned_permissions
    } else if required_left == &owned_left.clone().capture_error() {
        owned_permissions.without(EffectSet::singleton(Effect::Error))
    } else {
        return false;
    };
    available.iter().all(|effect| {
        required_permissions.iter().any(|permission| effect_covers(&permission, &effect))
    })
}

impl<V> EffectPredicate<V> {
    pub(crate) fn try_map_contracts<W, E>(
        &self,
        mut map: impl FnMut(&EffectContract<V>) -> Result<EffectContract<W>, E>,
    ) -> Result<EffectPredicate<W>, E> {
        Ok(match self {
            Self::Equal(left, right) => EffectPredicate::Equal(map(left)?, map(right)?),
            Self::Included(left, right) => EffectPredicate::Included(map(left)?, map(right)?),
            Self::Covered(left, right) => EffectPredicate::Covered(map(left)?, map(right)?),
            Self::Excluded(contract, excluded) => EffectPredicate::Excluded(map(contract)?, *excluded),
        })
    }

    pub(crate) fn contracts(&self) -> impl Iterator<Item = &EffectContract<V>> {
        match self {
            Self::Equal(left, right) | Self::Included(left, right) | Self::Covered(left, right) => [Some(left), Some(right)],
            Self::Excluded(contract, _) => [Some(contract), None],
        }.into_iter().flatten()
    }
}

pub(crate) fn effect_covers(permission: &Effect, required: &Effect) -> bool {
    permission == required || (*permission == Effect::Io
        && matches!(required, Effect::Fs | Effect::Net | Effect::Process | Effect::Env))
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct EffectSet(u8);

impl EffectSet {
    pub(crate) fn bits(self) -> u8 { self.0 }

    pub(crate) fn from_bits(bits: u8) -> Option<Self> {
        (bits & !Self::all().0 == 0).then_some(Self(bits))
    }

    pub(crate) fn new(effects: impl IntoIterator<Item = Effect>) -> Self {
        effects.into_iter().fold(Self::default(), |set, effect| {
            set.union(Self::singleton(effect))
        })
    }

    pub(crate) fn all() -> Self {
        Self::new(Effect::ALL)
    }

    pub(crate) fn singleton(effect: Effect) -> Self {
        Self(1 << Effect::ALL.iter().position(|candidate| candidate == &effect)
            .expect("every effect belongs to Effect::ALL"))
    }

    pub(crate) fn contains(self, effect: Effect) -> bool {
        self.intersection(Self::singleton(effect)) != Self::default()
    }

    pub(crate) fn is_empty(self) -> bool {
        self == Self::default()
    }

    pub(crate) fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub(crate) fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    pub(crate) fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    pub(crate) fn prohibited_requirements(self) -> Self {
        Self::new(Effect::ALL.into_iter().filter(|required| {
            self.iter().any(|excluded| effect_covers(required, &excluded))
        }))
    }

    pub(crate) fn iter(self) -> impl Iterator<Item = Effect> {
        Effect::ALL.into_iter().filter(move |effect| self.contains(effect.clone()))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EffectExpr<V> {
    direct: EffectSet,
    variables: BTreeMap<V, EffectSet>,
}

impl<V: Ord> EffectExpr<V> {
    pub(crate) fn finite(direct: EffectSet) -> Self {
        Self { direct, variables: BTreeMap::new() }
    }

    pub(crate) fn variable(variable: V) -> Self {
        Self { direct: EffectSet::default(), variables: BTreeMap::from([(variable, EffectSet::all())]) }
    }

    pub(crate) fn direct(&self) -> EffectSet {
        self.direct
    }

    pub(crate) fn variables(&self) -> impl Iterator<Item = (&V, EffectSet)> {
        self.variables.iter().map(|(variable, mask)| (variable, *mask))
    }

    pub(crate) fn closed(&self) -> Option<EffectSet> {
        self.variables.is_empty().then_some(self.direct)
    }

    pub(crate) fn single_variable(&self) -> Option<&V> {
        if !self.direct.is_empty() || self.variables.len() != 1 {
            return None;
        }
        let (variable, mask) = self.variables.first_key_value()?;
        (*mask == EffectSet::all()).then_some(variable)
    }

    pub(crate) fn union(mut self, other: Self) -> Self {
        self.direct = self.direct.union(other.direct);
        for (variable, mask) in other.variables {
            self.variables.entry(variable).and_modify(|current| *current = current.union(mask))
                .or_insert(mask);
        }
        self
    }

    pub(crate) fn masked(mut self, mask: EffectSet) -> Self {
        self.direct = self.direct.intersection(mask);
        self.variables.retain(|_, variable_mask| {
            *variable_mask = variable_mask.intersection(mask);
            !variable_mask.is_empty()
        });
        self
    }

    pub(crate) fn substitute<W: Ord, E>(
        &self,
        mut variable: impl FnMut(&V) -> Result<EffectContract<W>, E>,
    ) -> Result<EffectContract<W>, E> {
        let mut result = EffectContract::finite(self.direct);
        for (id, mask) in &self.variables {
            result = result.union(variable(id)?.masked(*mask));
        }
        Ok(result)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum EffectContract<V> {
    Unknown,
    Known(EffectExpr<V>),
}

impl<V: Ord> EffectContract<V> {
    pub(crate) fn for_invocation(self, kind: crate::sema::types::CallableKind) -> Self {
        match kind {
            crate::sema::types::CallableKind::Pure => Self::finite(EffectSet::default()),
            crate::sema::types::CallableKind::Proc | crate::sema::types::CallableKind::Stream => self,
        }
    }

    pub(crate) fn from_clause(effects: Option<impl IntoIterator<Item = Effect>>) -> Self {
        match effects {
            Some(effects) => Self::finite(EffectSet::new(effects)),
            None => Self::Unknown,
        }
    }

    pub(crate) fn concrete(&self) -> Result<Option<Vec<Effect>>, UnresolvedCallableEffects> {
        match self {
            Self::Unknown => Ok(None),
            Self::Known(expression) => expression.closed()
                .map(|effects| Some(effects.iter().collect())).ok_or(UnresolvedCallableEffects),
        }
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + match self {
            Self::Unknown => 0,
            Self::Known(expression) => expression.variables.len() * std::mem::size_of::<(V, EffectSet)>(),
        }
    }

    pub(crate) fn finite(effects: EffectSet) -> Self {
        Self::Known(EffectExpr::finite(effects))
    }

    pub(crate) fn variable(variable: V) -> Self {
        Self::Known(EffectExpr::variable(variable))
    }

    pub(crate) fn closed(&self) -> Option<EffectSet> {
        match self {
            Self::Unknown => None,
            Self::Known(expression) => expression.closed(),
        }
    }

    pub(crate) fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::Known(left), Self::Known(right)) => Self::Known(left.union(right)),
            _ => Self::Unknown,
        }
    }

    pub(crate) fn masked(self, mask: EffectSet) -> Self {
        match self {
            Self::Unknown => Self::Unknown,
            Self::Known(expression) => Self::Known(expression.masked(mask)),
        }
    }

    pub(crate) fn capture_error(self) -> Self {
        self.masked(EffectSet::all().without(EffectSet::singleton(Effect::Error)))
    }

    pub(crate) fn substitute<W: Ord, E>(
        &self,
        variable: impl FnMut(&V) -> Result<EffectContract<W>, E>,
    ) -> Result<EffectContract<W>, E> {
        match self {
            Self::Unknown => Ok(EffectContract::Unknown),
            Self::Known(expression) => expression.substitute(variable),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Contract = EffectContract<u32>;

    #[test]
    fn symbolic_effect_predicates_preserve_assignment_independent_coverage() {
        let variable = Contract::variable(7);
        let time = Contract::finite(EffectSet::singleton(Effect::Time));
        let union = time.clone().union(variable.clone());
        assert_eq!(EffectPredicate::Covered(time.clone(), union.clone()).satisfaction(), Some(true));
        assert_eq!(EffectPredicate::Included(variable.clone().capture_error(), variable.clone()).satisfaction(), Some(true));
        assert_eq!(EffectPredicate::Included(union, variable.clone()).satisfaction(), None);
        assert_eq!(EffectPredicate::Covered(time, variable.clone()).satisfaction(), None);
        let fs = EffectSet::singleton(Effect::Fs);
        let io = EffectSet::singleton(Effect::Io);
        assert_eq!(EffectPredicate::Covered(variable.clone().masked(fs), variable.clone().masked(io)).satisfaction(), None);
        assert_eq!(EffectPredicate::Covered(variable.clone().masked(fs), Contract::finite(io)).satisfaction(), Some(true));
        assert_eq!(EffectPredicate::Included(variable.masked(fs), Contract::finite(io)).satisfaction(), None);
        assert_eq!(EffectPredicate::Covered(Contract::Unknown, Contract::finite(EffectSet::default())).satisfaction(), Some(false));
    }

    #[test]
    fn effect_coverage_implication_requires_the_same_subject_and_finite_permissions() {
        let subject = Contract::variable(7);
        let error = Contract::finite(EffectSet::singleton(Effect::Error));
        let error_and_fs = Contract::finite(EffectSet::new([Effect::Error, Effect::Fs]));
        let fs = Contract::finite(EffectSet::singleton(Effect::Fs));
        let owned = EffectPredicate::Covered(subject.clone(), error.clone());
        assert!(effect_predicate_implies(&owned, &owned));
        assert!(effect_predicate_implies(&owned, &EffectPredicate::Covered(subject.clone(), error_and_fs)));
        assert!(!effect_predicate_implies(&owned, &EffectPredicate::Covered(subject.clone(), fs)));
        assert!(!effect_predicate_implies(&owned, &EffectPredicate::Covered(Contract::variable(8), error.clone())));
        assert!(!effect_predicate_implies(&owned, &EffectPredicate::Covered(subject.clone(), Contract::variable(9))));

        let capture = subject.clone().capture_error();
        let empty = Contract::finite(EffectSet::default());
        assert!(effect_predicate_implies(&owned, &EffectPredicate::Covered(capture.clone(), empty.clone())));
        let error_and_process = EffectPredicate::Covered(
            subject.clone(),
            Contract::finite(EffectSet::new([Effect::Error, Effect::Process])),
        );
        assert!(!effect_predicate_implies(&error_and_process, &EffectPredicate::Covered(capture.clone(), empty.clone())));
        assert!(!effect_predicate_implies(&owned, &EffectPredicate::Covered(subject.clone(), empty)));
        assert!(!effect_predicate_implies(&owned, &EffectPredicate::Covered(Contract::variable(8).capture_error(), Contract::finite(EffectSet::default()))));
        assert!(!effect_predicate_implies(&owned, &EffectPredicate::Covered(subject.clone().masked(EffectSet::singleton(Effect::Fs)), Contract::finite(EffectSet::default()))));

        let io = EffectPredicate::Covered(subject.clone(), Contract::finite(EffectSet::singleton(Effect::Io)));
        let io_permissions = EffectPredicate::Covered(
            subject.clone(),
            Contract::finite(EffectSet::new([Effect::Fs, Effect::Net, Effect::Process, Effect::Env])),
        );
        assert!(!effect_predicate_implies(&io, &io_permissions));
        assert!(effect_predicate_implies(&io_permissions, &io));
        assert!(!effect_predicate_implies(&io, &EffectPredicate::Covered(subject.clone(), Contract::finite(EffectSet::singleton(Effect::Error)))));
        assert!(!effect_predicate_implies(&owned, &EffectPredicate::Covered(subject, Contract::finite(EffectSet::singleton(Effect::Io)))));

    }

    #[test]
    fn symbolic_effect_capture_preserves_unknown_and_uncaptured_contributions() {
        let unknown = Contract::Unknown;
        assert_eq!(unknown.clone().capture_error(), unknown);
        assert_ne!(unknown, Contract::finite(EffectSet::default()));
        let called = Contract::variable(7);
        assert_eq!(called.clone().capture_error().union(called.clone()), called);
        let mixed = Contract::variable(7).capture_error().union(Contract::variable(8));
        let actual = mixed.substitute(|id| Ok::<_, ()>(Contract::finite(EffectSet::new(
            if *id == 7 { vec![Effect::Env, Effect::Error] } else { vec![Effect::Time, Effect::Error] }
        )))).unwrap();
        assert_eq!(actual.closed(), Some(EffectSet::new([Effect::Env, Effect::Time, Effect::Error])));
    }

    #[test]
    fn symbolic_effect_capture_commutes_with_actual_substitution() {
        let expression = Contract::variable(1).union(Contract::finite(EffectSet::new([Effect::Io, Effect::Error])));
        let actual = Contract::finite(EffectSet::new([Effect::Fs, Effect::Error]));
        let substituted = expression.substitute(|_| Ok::<_, ()>(actual.clone())).unwrap().capture_error();
        let captured = expression.capture_error().substitute(|_| Ok::<_, ()>(actual.clone())).unwrap();
        assert_eq!(substituted, captured);
        assert_eq!(captured.closed(), Some(EffectSet::new([Effect::Io, Effect::Fs])));
        assert_ne!(EffectSet::new([Effect::Io]), EffectSet::new([Effect::Fs, Effect::Net, Effect::Process, Effect::Env, Effect::Time]));
    }

    #[test]
    fn symbolic_effect_substitution_requires_each_live_variable() {
        let expression = Contract::variable(1).capture_error();
        assert_eq!(expression.substitute::<u32, _>(|_| Err("foreign effect variable")), Err("foreign effect variable"));
        assert_eq!(expression.substitute(|_| Ok::<_, ()>(Contract::Unknown)).unwrap(), Contract::Unknown);
        let zero = Contract::variable(1).masked(EffectSet::default());
        assert_eq!(zero.substitute::<u32, ()>(|_| panic!("erased dependency must not be read")).unwrap().closed(), Some(EffectSet::default()));
        assert!(EffectSet::new([Effect::Env]).without(EffectSet::new([Effect::Env, Effect::Error])).is_empty());
        assert_eq!(EffectSet::all().iter().collect::<Vec<_>>(), Effect::ALL);
        let mut constraints = crate::sema::constraints::TypeConstraints::default();
        let variable = constraints.fresh_effect_at_level(crate::source::Span::new(crate::source::SourceId::new(0), 0, 1), 1);
        assert_eq!(CallableEffects::from_contract(EffectContract::variable(variable)).concrete(), Err(UnresolvedCallableEffects));
        assert_eq!(CallableEffects::unknown().concrete(), Ok(None));
        assert_eq!(CallableEffects::finite([]).concrete(), Ok(Some(Vec::new())));
    }
}
