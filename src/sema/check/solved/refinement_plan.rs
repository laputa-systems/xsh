use super::*;
use std::sync::Arc;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolvedRefinementAlias {
    pub binding: BindingIdentity,
    pub statement: StatementIdentity,
    pub initializer: ExpressionIdentity,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolvedRefinementWrite {
    pub statement: StatementIdentity,
    pub revision: u64,
    pub path: Arc<[Name]>,
}

/// A checked read keeps the original predicate and the exiting guard that made
/// its record projection precise. Mutable storage retains its separate type.
#[derive(Clone, Debug)]
pub struct SolvedRefinedRead {
    pub binding: BindingIdentity,
    pub predicate: ExpressionIdentity,
    pub subject: ExpressionIdentity,
    pub predicate_nonnull_when_true: bool,
    pub path: Arc<[Name]>,
    pub read_path: Arc<[Name]>,
    pub aliases: Vec<SolvedRefinementAlias>,
    pub guard: StatementIdentity,
    pub guard_condition: ExpressionIdentity,
    pub caller: Option<DeclarationIdentity>,
    pub invariant: ScopedRoot,
    pub narrowed: ScopedRoot,
    pub revision: u64,
    pub writes: Vec<SolvedRefinementWrite>,
}

impl PartialEq for SolvedRefinedRead {
    fn eq(&self, other: &Self) -> bool {
        self.binding == other.binding && self.predicate == other.predicate && self.subject == other.subject
            && self.predicate_nonnull_when_true == other.predicate_nonnull_when_true
            && self.path == other.path && self.read_path == other.read_path && self.aliases == other.aliases
            && self.guard == other.guard && self.guard_condition == other.guard_condition && self.caller == other.caller
            && self.invariant.ty == other.invariant.ty && self.invariant.scope == other.invariant.scope
            && self.narrowed.ty == other.narrowed.ty && self.narrowed.scope == other.narrowed.scope
            && self.revision == other.revision && self.writes == other.writes
    }
}
impl Eq for SolvedRefinedRead {}

impl<Graph> SolvedTypes<Graph> {
    pub(crate) fn checked_refined_read(&self, read: ExpressionIdentity) -> Result<&SolvedRefinedRead, InferenceError> {
        let original = self.original_refined_reads.get(&read).ok_or(InferenceError::InvalidScheme)?;
        let current = self.refined_reads.get(&read).ok_or(InferenceError::InvalidScheme)?;
        if !Arc::ptr_eq(original, current) && original.as_ref() != current.as_ref() { return Err(InferenceError::InvalidScheme); }
        if read.source != original.binding.source || read.namespace != original.binding.namespace
            || original.predicate.source != read.source || original.predicate.namespace != read.namespace
            || original.subject.source != read.source || original.subject.namespace != read.namespace
            || original.guard.source != read.source || original.guard.namespace != read.namespace
            || original.guard_condition.source != read.source || original.guard_condition.namespace != read.namespace
            || self.expressions.get(&read) != Some(&original.narrowed.ty)
            || self.expression_owners.get(&read).copied() != original.caller
            || self.expression_scope(read, original.caller)? != original.narrowed.scope
            || self.bindings.get(&original.binding).map(|binding| binding.ty) != Some(original.invariant.ty)
            || self.bindings.get(&original.binding).map(|binding| binding.scheme.or(binding.owner.and_then(|owner| self.declarations.get(&owner).map(|declaration| declaration.scheme)))) != Some(original.invariant.scope)
            || !original.path.starts_with(&original.read_path)
            || !self.statements.contains_key(&original.guard)
            || !self.expressions.contains_key(&original.predicate)
            || !self.expressions.contains_key(&original.subject)
            || !self.expressions.contains_key(&original.guard_condition)
            || original.aliases.iter().any(|alias| alias.binding.source != read.source || alias.binding.namespace != read.namespace
                || alias.statement.source != read.source || alias.statement.namespace != read.namespace
                || alias.initializer.source != read.source || alias.initializer.namespace != read.namespace
                || !self.bindings.contains_key(&alias.binding) || !self.statements.contains_key(&alias.statement) || !self.expressions.contains_key(&alias.initializer))
            || original.writes.iter().any(|write| write.statement.source != read.source || write.statement.namespace != read.namespace
                || write.revision <= original.revision || !self.statements.contains_key(&write.statement)
                || super::super::proof::paths_overlap(&write.path, &original.path)) { return Err(InferenceError::InvalidScheme); }
        Ok(original)
    }

    pub(super) fn refined_read_payload_bytes(&self) -> usize {
        use std::mem::size_of;
        let mut records = std::collections::BTreeSet::new();
        let mut paths = std::collections::BTreeSet::new();
        self.refined_reads.values().chain(self.original_refined_reads.values()).filter(|record| records.insert(Arc::as_ptr(record) as usize)).map(|record| {
            let path_bytes = std::iter::once(&record.path).chain(std::iter::once(&record.read_path)).chain(record.writes.iter().map(|write| &write.path))
                .filter(|path| paths.insert(Arc::as_ptr(path) as *const Name as usize))
                .map(|path| path.len() * size_of::<Name>() + 2 * size_of::<usize>()).sum::<usize>();
            size_of::<SolvedRefinedRead>() + 2 * size_of::<usize>() + path_bytes
                + record.aliases.capacity() * size_of::<SolvedRefinementAlias>()
                + record.writes.capacity() * size_of::<SolvedRefinementWrite>()
        }).sum()
    }
}

impl SolvedTypes<InferenceContext> {
    pub(super) fn seal_refined_reads(&mut self) -> Result<(), InferenceError> {
        for (&read, record) in &mut self.refined_reads {
            let scope = self.expression_schemes.get(&read).copied().or_else(|| self.expression_value_scopes.get(&read).copied())
                .or(record.caller.map(|owner| self.declarations.get(&owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?);
            let binding = self.bindings.get(&record.binding).ok_or(InferenceError::InvalidScheme)?;
            let invariant_scope = binding.scheme.or(binding.owner.map(|owner| self.declarations.get(&owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?);
            let record = Arc::make_mut(record);
            record.narrowed.scope = scope;
            record.invariant.scope = invariant_scope;
        }
        self.original_refined_reads = self.refined_reads.clone();
        for &read in self.refined_reads.keys() { self.checked_refined_read(read)?; }
        Ok(())
    }
}
