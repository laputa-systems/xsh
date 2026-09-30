use super::{Name, Type};
use std::sync::Arc;

/// Clones represent the same binding; a new declaration gets a new identity.
/// Mutation stamps describe only known record paths. An empty path invalidates
/// the entire value, including every projection proof saved by a Boolean alias.
#[derive(Clone, Debug)]
pub(super) struct BindingProof {
    identity: Arc<()>,
    revision: u64,
    floor: u64,
    mutations: Vec<(u64, Arc<[Name]>)>,
}

impl Default for BindingProof {
    fn default() -> Self { Self { identity: Arc::new(()), revision: 0, floor: 0, mutations: Vec::new() } }
}

impl PartialEq for BindingProof {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.identity, &other.identity) && self.revision == other.revision
            && self.floor == other.floor && self.mutations == other.mutations
    }
}
impl Eq for BindingProof {}

impl BindingProof {
    pub(super) fn same_binding(&self, other: &Self) -> bool { Arc::ptr_eq(&self.identity, &other.identity) }
    pub(super) fn fact(&self, name: Name, path: Vec<Name>, ty: Type) -> Narrowing {
        Narrowing { name, path: path.into(), identity: Arc::clone(&self.identity), revision: self.revision, ty }
    }
    pub(super) fn accepts(&self, fact: &Narrowing) -> bool {
        Arc::ptr_eq(&self.identity, &fact.identity) && fact.revision >= self.floor && fact.revision <= self.revision
            && self.mutations.iter().all(|(revision, path)| *revision <= fact.revision || !paths_overlap(path, &fact.path))
    }
    pub(super) fn mutation_paths_since(&self, original: &Self) -> Vec<Arc<[Name]>> {
        if self.floor > original.revision { return vec![Arc::from([])]; }
        self.mutations.iter().filter(|(revision, _)| *revision > original.revision).map(|(_, path)| Arc::clone(path)).collect()
    }

    pub(super) fn mutate(&mut self, path: &[Name]) {
        self.revision = self.revision.saturating_add(1);
        if self.mutations.len() == 128 { self.floor = self.revision; self.mutations.clear(); }
        self.mutations.push((self.revision, Arc::from(path)));
    }
}

#[derive(Clone, Debug)]
pub(super) struct Narrowing {
    pub(super) name: Name,
    pub(super) path: Arc<[Name]>,
    identity: Arc<()>,
    revision: u64,
    pub(super) ty: Type,
}

impl PartialEq for Narrowing {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.path == other.path && self.ty == other.ty
            && self.revision == other.revision && Arc::ptr_eq(&self.identity, &other.identity)
    }
}
impl Eq for Narrowing {}

/// Alias declarations share an immutable proof set. Combining conditions is
/// bounded and deduplicates subjects, so alias chains do not expand expressions.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct ConditionNarrowings {
    pub(super) when_true: Vec<Narrowing>,
    pub(super) when_false: Vec<Narrowing>,
}

impl ConditionNarrowings {
    pub(super) fn and(self, right: Self) -> Self {
        Self { when_true: union(&self.when_true, &right.when_true),
            when_false: intersection(&self.when_false, &union(&self.when_true, &right.when_false)) }
    }
    pub(super) fn or(self, right: Self) -> Self {
        Self { when_true: intersection(&self.when_true, &union(&self.when_false, &right.when_true)),
            when_false: union(&self.when_false, &right.when_false) }
    }
}

fn union(left: &[Narrowing], right: &[Narrowing]) -> Vec<Narrowing> {
    let mut facts = left.to_vec();
    for fact in right { if !facts.contains(fact) { facts.push(fact.clone()); } }
    if facts.len() > 128 { Vec::new() } else { facts }
}
fn intersection(left: &[Narrowing], right: &[Narrowing]) -> Vec<Narrowing> {
    left.iter().filter(|fact| right.contains(fact)).cloned().collect()
}
pub(super) fn paths_overlap(left: &[Name], right: &[Name]) -> bool { left.starts_with(right) || right.starts_with(left) }
pub(super) fn projected_type<'a>(mut ty: &'a Type, path: &[Name]) -> Option<&'a Type> {
    for name in path { let Type::Record(fields) = ty else { return None; }; ty = fields.get(name)?; }
    Some(ty)
}
pub(super) fn replace_projection(ty: &mut Type, path: &[Name], replacement: Type) -> bool {
    let Some((name, rest)) = path.split_first() else { *ty = merge_proven_type(ty, replacement); return true; };
    let Type::Record(fields) = ty else { return false; };
    let Some(child) = fields.get_mut(name) else { return false; };
    replace_projection(child, rest, replacement)
}
fn merge_proven_type(current: &Type, mut replacement: Type) -> Type {
    if matches!(replacement, Type::Any | Type::Unknown) { return current.clone(); }
    if let (Type::Record(current), Type::Record(replacement)) = (current, &mut replacement) {
        for (name, ty) in current {
            if let Some(new) = replacement.get_mut(name) { *new = merge_proven_type(ty, new.clone()); }
            else { replacement.insert(*name, ty.clone()); }
        }
    } else if !matches!(current, Type::Any | Type::Unknown) && current.matches_expected(&replacement) {
        return current.clone();
    }
    replacement
}

pub(super) fn restore_projection(ty: &mut Type, original: &Type, path: &[Name]) {
    if path.is_empty() { *ty = original.clone(); return; }
    let (Type::Record(fields), Type::Record(original_fields)) = (ty, original) else { return; };
    let (name, rest) = path.split_first().unwrap();
    match (fields.get_mut(name), original_fields.get(name)) {
        (Some(child), Some(original)) => restore_projection(child, original, rest),
        (_, None) => { fields.remove(name); }
        _ => {}
    }
}

/// Joins keep only checked shape/type information common to every continuation.
pub(super) fn intersection_type(original: &Type, continuations: &[Type]) -> Type {
    let Some(first) = continuations.first() else { return original.clone(); };
    if continuations.iter().all(|ty| ty == first) { return first.clone(); }
    if let Type::Record(original_fields) = original {
        if continuations.iter().all(|ty| matches!(ty, Type::Record(_))) {
            let mut fields = original_fields.clone();
            for (name, ty) in &mut fields {
                let children = continuations.iter().map(|continuation| {
                    let Type::Record(fields) = continuation else { unreachable!() };
                    fields.get(name).cloned().unwrap_or_else(|| ty.clone())
                }).collect::<Vec<_>>();
                *ty = intersection_type(ty, &children);
            }
            return Type::Record(fields);
        }
    }
    original.clone()
}
