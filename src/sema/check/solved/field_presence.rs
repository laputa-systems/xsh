use super::*;
use std::sync::Arc;

/// A field-presence read retains the checked record before the predicate and the
/// independently checked record inside the predicate's successful branch.
#[derive(Clone, Debug)]
pub struct SolvedFieldPresenceRead {
    pub read: ExpressionIdentity,
    pub binding: BindingIdentity,
    pub predicate: ExpressionIdentity,
    pub subject: ExpressionIdentity,
    pub key: ExpressionIdentity,
    pub field: Name,
    pub control: StatementIdentity,
    pub branch: u32,
    pub caller: Option<DeclarationIdentity>,
    pub material: ScopedRoot,
    pub subject_type: ScopedRoot,
    pub narrowed: ScopedRoot,
    pub revision: u64,
    pub writes: Vec<super::SolvedRefinementWrite>,
}

impl PartialEq for SolvedFieldPresenceRead {
    fn eq(&self, other: &Self) -> bool {
        self.read == other.read && self.binding == other.binding && self.predicate == other.predicate && self.subject == other.subject
            && self.key == other.key && self.field == other.field && self.control == other.control
            && self.branch == other.branch && self.caller == other.caller && self.revision == other.revision
            && self.material.ty == other.material.ty && self.material.scope == other.material.scope
            && self.subject_type.ty == other.subject_type.ty && self.subject_type.scope == other.subject_type.scope
            && self.narrowed.ty == other.narrowed.ty && self.narrowed.scope == other.narrowed.scope && self.writes == other.writes
    }
}
impl Eq for SolvedFieldPresenceRead {}

impl<Graph> SolvedTypes<Graph> {
    pub(crate) fn checked_field_presence_read(&self, read: ExpressionIdentity) -> Result<Option<&SolvedFieldPresenceRead>, InferenceError> {
        let original = self.original_field_presence_reads.get(&read);
        let current = self.field_presence_reads.get(&read);
        let (Some(original), Some(current)) = (original, current) else {
            return if original.is_none() && current.is_none() { Ok(None) } else { Err(InferenceError::InvalidScheme) };
        };
        if !Arc::ptr_eq(original, current) && original.as_ref() != current.as_ref() { return Err(InferenceError::InvalidScheme); }
        let definition = self.bindings.get(&original.binding).ok_or(InferenceError::InvalidScheme)?;
        let material_scope = definition.scheme.or(definition.owner.and_then(|owner| self.declarations.get(&owner).map(|declaration| declaration.scheme)));
        if original.read != read || definition.mutable || definition.owner != original.caller || definition.ty != original.material.ty || material_scope != original.material.scope
            || self.expressions.get(&read) != Some(&original.narrowed.ty)
            || self.expression_owners.get(&read).copied() != original.caller
            || self.expression_scope(read, original.caller)? != original.narrowed.scope
            || self.expressions.get(&original.subject) != Some(&original.subject_type.ty)
            || self.expression_owners.get(&original.subject).copied() != original.caller
            || self.expression_scope(original.subject, original.caller)? != original.subject_type.scope
            || self.expression_owners.get(&original.predicate).copied() != original.caller
            || self.expression_owners.get(&original.key).copied() != original.caller
            || !self.expressions.contains_key(&original.predicate) || !self.expressions.contains_key(&original.key)
            || !self.statements.contains_key(&original.control)
            || self.statement_owners.get(&original.control).copied() != original.caller
            || [original.predicate, original.subject, original.key, read].iter().any(|identity| identity.source != original.binding.source || identity.namespace != original.binding.namespace)
            || original.control.source != original.binding.source || original.control.namespace != original.binding.namespace
            || !original.writes.is_empty() {
            return Err(InferenceError::InvalidScheme);
        }
        Ok(Some(original))
    }

    pub(super) fn validate_field_presence_reads(&self) -> Result<(), InferenceError> {
        for &read in self.field_presence_reads.keys().chain(self.original_field_presence_reads.keys()) {
            self.checked_field_presence_read(read)?.ok_or(InferenceError::InvalidScheme)?;
        }
        Ok(())
    }

    pub(super) fn field_presence_payload_bytes(&self) -> usize {
        let mut records = std::collections::BTreeSet::new();
        self.field_presence_reads.values().chain(self.original_field_presence_reads.values())
            .filter(|record| records.insert(Arc::as_ptr(record) as usize)).map(|record| {
                std::mem::size_of::<SolvedFieldPresenceRead>() + 2 * std::mem::size_of::<usize>()
                    + record.writes.capacity() * std::mem::size_of::<super::SolvedRefinementWrite>()
            }).sum()
    }
}

impl SolvedTypes<InferenceContext> {
    pub(super) fn seal_field_presence_reads(&mut self) -> Result<(), InferenceError> {
        for (&read, original) in &mut self.field_presence_reads {
            let scope = self.expression_schemes.get(&read).copied().or_else(|| self.expression_value_scopes.get(&read).copied())
                .or(original.caller.map(|owner| self.declarations.get(&owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?);
            let subject_scope = self.expression_schemes.get(&original.subject).copied().or_else(|| self.expression_value_scopes.get(&original.subject).copied())
                .or(original.caller.map(|owner| self.declarations.get(&owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?);
            let binding = self.bindings.get(&original.binding).ok_or(InferenceError::InvalidScheme)?;
            let material_scope = binding.scheme.or(binding.owner.map(|owner| self.declarations.get(&owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?);
            let original = Arc::make_mut(original);
            original.narrowed.scope = scope;
            original.subject_type.scope = subject_scope;
            original.material.scope = material_scope;
        }
        self.original_field_presence_reads = self.field_presence_reads.clone();
        self.validate_field_presence_reads()
    }
}
