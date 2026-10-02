use super::*;
use crate::sema::check::{ExpressionIdentity, PatternIdentity, PatternCaptureIdentity, StatementIdentity};
use super::super::pattern::pattern_result_relation;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct ConditionalSourceId { index: u32, proof: OwnerProof }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum ConditionalKind { Match, If, PatternTest, Not }

/// Saved receiver and argument wrappers execute around the authored value;
/// their instructions do not acquire the value's expression identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct ConditionalValue {
    pub instruction: u32,
    pub material: u32,
    pub origin: ExpressionIdentity,
    pub original_callable: Option<crate::sema::check::DeclarationIdentity>,
    pub expected: TypeRef,
    pub wrappers: Box<[ValueInitializerWrapper]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum ConditionalTerminalValue {
    Expression(ConditionalValue),
    PatternCapture { instruction: u32, identity: PatternCaptureIdentity, expected: TypeRef },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum ConditionalBody {
    Authored { value: ConditionalValue, terminal: Option<(u32, StatementIdentity, ConditionalTerminalValue)> },
    Boolean { instruction: u32, value: bool },
}

impl ConditionalBody {
    pub fn instruction(&self) -> u32 {
        match self { Self::Authored { value, .. } => value.instruction, Self::Boolean { instruction, .. } => *instruction }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct ConditionalArm {
    pub pattern: Option<(u32, PatternIdentity)>,
    pub condition: Option<ConditionalValue>,
    pub guard: Option<ConditionalValue>,
    pub body: ConditionalBody,
}

/// Result authority retains the original branch order and completing value
/// roots. Equal result storage never authorizes a different arm or tail.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct ConditionalResultSource {
    pub instruction: u32,
    pub origin: ExpressionIdentity,
    pub owner: InstructionOwner,
    pub scope: Option<SchemeScopeId>,
    pub expected: TypeRef,
    pub kind: ConditionalKind,
    pub subject: Option<ConditionalValue>,
    pub arms: Box<[ConditionalArm]>,
    pub fallback: Option<ConditionalBody>,
    pub instruction_payload: Box<[u32]>,
    pub block_flags: u8,
    pub block_payload: Box<[u32]>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ConditionalEvidence {
    sources: Vec<Entry<Arc<ConditionalResultSource>>>,
    originals: Vec<Arc<ConditionalResultSource>>,
    instructions: Vec<(u32, usize)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ConditionalCheckpoint { sources: usize }

fn value_bytes(value: &ConditionalValue) -> usize {
    value.wrappers.len() * std::mem::size_of::<ValueInitializerWrapper>()
        + value.wrappers.iter().map(|wrapper| wrapper.payload.len() * std::mem::size_of::<u32>()).sum::<usize>()
}

fn body_bytes(body: &ConditionalBody) -> usize {
    match body {
        ConditionalBody::Authored { value, terminal } => value_bytes(value) + match terminal {
            Some((_, _, ConditionalTerminalValue::Expression(value))) => value_bytes(value), _ => 0,
        },
        ConditionalBody::Boolean { .. } => 0,
    }
}

impl ConditionalEvidence {
    pub(super) fn checkpoint(&self) -> ConditionalCheckpoint { ConditionalCheckpoint { sources: self.sources.len() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: ConditionalCheckpoint, serial_limit: u64) -> Result<(), IrVerifyError> {
        if checkpoint.sources > self.sources.len() || self.sources.get(checkpoint.sources.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial_limit) {
            return Err(failure("conditional checkpoint references retired or replaced entries"));
        }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: ConditionalCheckpoint) {
        self.sources.truncate(checkpoint.sources); self.originals.truncate(checkpoint.sources); self.instructions.clear();
    }
    pub(super) fn finish(&mut self) {
        self.instructions = self.sources.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect();
        self.instructions.sort_unstable_by_key(|entry| entry.0);
    }
    pub(super) fn shrink_to_fit(&mut self) { self.sources.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); }
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.sources.capacity() * size_of::<Entry<Arc<ConditionalResultSource>>>() + self.originals.capacity() * size_of::<Arc<ConditionalResultSource>>()
            + self.instructions.capacity() * size_of::<(u32, usize)>() + self.sources.len() * (size_of::<ConditionalResultSource>() + 2 * size_of::<usize>())
            + self.sources.iter().map(|entry| {
                let source = &entry.value;
                source.arms.len() * size_of::<ConditionalArm>() + (source.instruction_payload.len() + source.block_payload.len()) * size_of::<u32>()
                    + source.subject.as_ref().map(value_bytes).unwrap_or(0) + source.fallback.as_ref().map(body_bytes).unwrap_or(0)
                    + source.arms.iter().map(|arm| arm.condition.as_ref().map(value_bytes).unwrap_or(0)
                        + arm.guard.as_ref().map(value_bytes).unwrap_or(0) + body_bytes(&arm.body)).sum::<usize>()
            }).sum::<usize>()
    }
}

impl GenericEvidenceStore {
    pub fn has_conditionals(&self) -> bool { !self.conditionals.sources.is_empty() || !self.conditionals.originals.is_empty() }
    pub fn conditional_source(&self, id: ConditionalSourceId) -> Result<&ConditionalResultSource, IrVerifyError> {
        let source = owned(self.root, &self.conditionals.sources, id.index, id.proof)?;
        if !self.conditionals.originals.get(id.index as usize).is_some_and(|original| Arc::ptr_eq(source, original)) {
            return Err(failure("conditional result differs from its original branch receipt"));
        }
        Ok(source.as_ref())
    }
    pub fn conditional_sources(&self) -> impl Iterator<Item = (ConditionalSourceId, &ConditionalResultSource)> {
        self.conditionals.sources.iter().enumerate().map(|(index, entry)| (ConditionalSourceId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn conditional_source_at(&self, instruction: u32) -> Result<Option<ConditionalSourceId>, IrVerifyError> {
        let Some(index) = self.conditionals.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| self.conditionals.instructions[index].1) else { return Ok(None); };
        let entry = self.conditionals.sources.get(index).ok_or_else(|| failure("conditional instruction index is stale"))?;
        let id = ConditionalSourceId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } };
        self.conditional_source(id)?; Ok(Some(id))
    }
    pub(super) fn verify_conditional_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        if self.conditionals.sources.len() != self.conditionals.originals.len() { return Err(failure("conditional original ledger is incomplete")); }
        let mut instructions = Vec::new();
        for (id, _) in self.conditional_sources() {
            let source = self.conditional_source(id)?;
            let check_origin = |instruction: u32, origin: ExpressionIdentity| -> Result<(), IrVerifyError> {
                if owners.get(instruction as usize) != Some(&Some(source.owner))
                    || (origin.source, origin.namespace) != (source.origin.source, source.origin.namespace)
                    || self.registered_instruction_origin(instruction, false) != Some((OperationSourceOrigin::Expression(origin), source.owner)) {
                    return Err(failure("conditional value changes its original expression or owner"));
                }
                Ok(())
            };
            let check_value = |value: &ConditionalValue| -> Result<(), IrVerifyError> {
                check_origin(value.material, value.origin)?;
                if owners.get(value.instruction as usize) != Some(&Some(source.owner)) || value.wrappers.len() > 256
                    || value.wrappers.iter().any(|wrapper| owners.get(wrapper.instruction as usize) != Some(&Some(source.owner))) {
                    return Err(failure("conditional value wrapper belongs to another original body"));
                }
                self.normalized_reference(pools, source.scope, value.expected)?;
                Ok(())
            };
            let check_body = |body: &ConditionalBody| -> Result<(), IrVerifyError> {
                match body {
                    ConditionalBody::Authored { value, terminal } => {
                        check_value(value)?;
                        pattern_result_relation(self, pools, source.scope, value.expected, source.expected)?;
                        if let Some((statement, identity, tail)) = terminal {
                            if owners.get(*statement as usize) != Some(&Some(source.owner))
                                || (identity.source, identity.namespace) != (source.origin.source, source.origin.namespace)
                                || self.registered_instruction_origin(*statement, false) != Some((OperationSourceOrigin::Statement(*identity), source.owner)) {
                                return Err(failure("conditional tail changes its original completing statement"));
                            }
                            let expected = match tail {
                                ConditionalTerminalValue::Expression(tail) => { check_value(tail)?; tail.expected }
                                ConditionalTerminalValue::PatternCapture { instruction, identity: original, expected } => {
                                    let use_ = self.pattern_use(*instruction).ok_or_else(|| failure("conditional capture tail loses its original read"))?;
                                    let capture = self.pattern_capture(use_.capture)?;
                                    let application = self.pattern_application(capture.application)?;
                                    let pattern = self.pattern_source(application.source)?;
                                    if use_.owner != source.owner || application.owner != source.owner
                                        || use_.origin != super::super::pattern::SourceUseIdentity::Statement(*identity)
                                        || capture.identity != *original || capture.expected != *expected || pattern.scope != source.scope {
                                        return Err(failure("conditional capture tail changes its original lexical authority"));
                                    }
                                    *expected
                                }
                            };
                            pattern_result_relation(self, pools, source.scope, expected, value.expected)?;
                        }
                    }
                    ConditionalBody::Boolean { instruction, .. } => {
                        if !matches!(source.kind, ConditionalKind::Not | ConditionalKind::PatternTest)
                            || owners.get(*instruction as usize) != Some(&Some(source.owner))
                            || self.normalized_reference(pools, source.scope, source.expected)? != NormalizedType::Scalar(crate::sema::types::Type::Bool) {
                            return Err(failure("conditional compiler Boolean loses its original decision"));
                        }
                    }
                }
                Ok(())
            };
            check_origin(source.instruction, source.origin)?;
            self.normalized_reference(pools, source.scope, source.expected)?;
            if let Some(scope) = source.scope && source.owner != InstructionOwner::Function(self.scope(scope)?.owner) {
                return Err(failure("conditional result belongs to another declaration scope"));
            }
            if let Some(subject) = &source.subject { check_value(subject)?; }
            if source.arms.is_empty() { return Err(failure("conditional result has no original branches")); }
            for arm in &source.arms {
                if let Some((instruction, identity)) = arm.pattern {
                    if self.registered_pattern_origin(instruction) != Some((identity, source.owner)) {
                        return Err(failure("conditional branch changes its original pattern identity"));
                    }
                }
                for condition in arm.condition.iter().chain(arm.guard.iter()) {
                    check_value(condition)?;
                    if self.normalized_reference(pools, source.scope, condition.expected)? != NormalizedType::Scalar(crate::sema::types::Type::Bool) {
                        return Err(failure("conditional decision changes its checked Boolean type"));
                    }
                }
                check_body(&arm.body)?;
            }
            if let Some(body) = &source.fallback { check_body(body)?; }
            instructions.push((source.instruction, id.index as usize));
        }
        instructions.sort_unstable_by_key(|entry| entry.0);
        if instructions.windows(2).any(|pair| pair[0].0 == pair[1].0) || instructions != self.conditionals.instructions {
            return Err(failure("conditional instruction index is ambiguous or stale"));
        }
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_conditional_source_mut(&mut self, id: ConditionalSourceId) -> Result<&mut ConditionalResultSource, IrVerifyError> {
        owned(self.root, &self.conditionals.sources, id.index, id.proof)?;
        Ok(Arc::make_mut(&mut self.conditionals.sources[id.index as usize].value))
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_clear_conditionals(&mut self) { self.conditionals.sources.clear(); self.conditionals.instructions.clear(); }
}

impl GenericEvidenceBuilder {
    pub fn add_conditional_source(&mut self, source: ConditionalResultSource) -> Result<ConditionalSourceId, IrVerifyError> {
        if self.store.conditionals.sources.len() >= 2_000_000 || source.arms.len() > 2_000_000 {
            return Err(failure("conditional evidence exceeds its work limit"));
        }
        let index = self.store.conditionals.sources.len() as u32;
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("conditional serial overflow"))?;
        let source = Arc::new(source);
        self.store.conditionals.originals.push(Arc::clone(&source)); self.store.conditionals.sources.push(Entry { serial, value: source });
        Ok(ConditionalSourceId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
}
