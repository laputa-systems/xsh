use super::*;
use crate::sema::check::ExpressionIdentity;
use crate::sema::inference::ScopedRoot;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct TryCaptureSourceId { index: u32, proof: OwnerProof }

/// The original error boundary owns its completion and propagated carrier
/// independently of the caller that later consumes the captured Result.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct TryCaptureSource {
    pub origin: ExpressionIdentity,
    pub block: crate::syntax::arena::BlockId,
    pub source_type: ScopedRoot,
    pub completion_origin: ExpressionIdentity,
    pub completion_type: ScopedRoot,
    pub propagation_origin: Option<(ExpressionIdentity, ScopedRoot)>,
    pub owner: InstructionOwner,
    pub instruction: u32,
    pub carrier: GroundTypeId,
    pub completion: GroundTypeId,
    pub propagation: Option<GroundTypeId>,
    pub original_carrier: crate::sema::types::Type,
    pub original_completion: crate::sema::types::Type,
    pub original_propagation: Option<crate::sema::types::Type>,
    pub retry: Option<RetryCapturePolicy>,
    pub error_capture: Option<Arc<super::super::full::PreparedCaptureErrorRelation>>,
    pub body_words: Box<[u32]>,
    pub body: u32,
    pub statement: u32,
    pub tail: u32,
    pub tail_code: (super::super::full::FullTag, Box<[u32]>),
    pub producer: Option<u32>,
    pub producer_source: Option<u32>,
    pub producer_code: Option<(super::super::full::FullTag, Box<[u32]>)>,
    pub payload: Box<[u32]>,
}


#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct RetryCaptureDelay {
    pub origin: ExpressionIdentity,
    pub source_type: ScopedRoot,
    pub instruction: u32,
    pub ty: GroundTypeId,
    pub original_type: crate::sema::types::Type,
    pub code: (super::super::full::FullTag, Box<[u32]>),
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct RetryCapturePolicy {
    pub delays: Vec<RetryCaptureDelay>,
    pub delays_block: u32,
    pub delays_words: Box<[u32]>,
    pub selection: Option<(crate::sema::check::PatternIdentity, ScopedRoot, u32)>,
    pub completion_is_result: bool,
}

#[derive(Clone, Debug, Default)]
pub(super) struct TryCaptureEvidence {
    sources: Vec<Entry<Arc<TryCaptureSource>>>,
    originals: Vec<Arc<TryCaptureSource>>,
    instructions: Vec<(u32, TryCaptureSourceId)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TryCaptureCheckpoint { sources: usize }

impl TryCaptureEvidence {
    pub(super) fn checkpoint(&self) -> TryCaptureCheckpoint { TryCaptureCheckpoint { sources: self.sources.len() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: TryCaptureCheckpoint, serial_limit: u64) -> Result<(), IrVerifyError> {
        if checkpoint.sources > self.sources.len() { return Err(failure("try capture checkpoint references retired entries")); }
        if self.sources.get(checkpoint.sources.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial_limit) {
            return Err(failure("try capture checkpoint references replacement entries"));
        }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: TryCaptureCheckpoint) {
        self.sources.truncate(checkpoint.sources); self.originals.truncate(checkpoint.sources); self.instructions.clear();
    }
    pub(super) fn finish(&mut self, root: u64) {
        self.instructions = self.sources.iter().enumerate().map(|(index, entry)| (entry.value.instruction,
            TryCaptureSourceId { index: index as u32, proof: OwnerProof { root, serial: entry.serial } })).collect();
        self.instructions.sort_unstable_by_key(|entry| entry.0);
    }
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.sources.capacity() * size_of::<Entry<Arc<TryCaptureSource>>>()
            + self.originals.capacity() * size_of::<Arc<TryCaptureSource>>()
            + self.sources.len() * (size_of::<TryCaptureSource>() + 2 * size_of::<usize>())
            + self.sources.iter().map(|entry| (entry.value.payload.len() + entry.value.body_words.len()) * size_of::<u32>() + entry.value.original_carrier.retained_bytes().saturating_sub(size_of::<crate::sema::types::Type>()) + entry.value.original_completion.retained_bytes().saturating_sub(size_of::<crate::sema::types::Type>()) + entry.value.original_propagation.as_ref().map_or(0, |ty| ty.retained_bytes().saturating_sub(size_of::<crate::sema::types::Type>())) + entry.value.retry.as_ref().map_or(0, |retry| retry.delays.capacity() * size_of::<RetryCaptureDelay>() + retry.delays_words.len() * size_of::<u32>() + retry.delays.iter().map(|delay| delay.code.1.len() * size_of::<u32>() + delay.original_type.retained_bytes().saturating_sub(size_of::<crate::sema::types::Type>())).sum::<usize>())).sum::<usize>()
            + self.sources.iter().map(|entry| entry.value.tail_code.1.len() * size_of::<u32>()).sum::<usize>()
            + self.sources.iter().filter_map(|entry| entry.value.producer_code.as_ref()).map(|(_, payload)| payload.len() * size_of::<u32>()).sum::<usize>()
            + self.instructions.capacity() * size_of::<(u32, TryCaptureSourceId)>()
            + self.sources.iter().filter_map(|entry| entry.value.error_capture.as_ref()).map(|relation| relation.retained_bytes()).sum::<usize>()
    }
    pub(super) fn shrink_to_fit(&mut self) { self.sources.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); }
}

impl GenericEvidenceStore {
    pub fn has_try_captures(&self) -> bool { !self.try_captures.sources.is_empty() || !self.try_captures.originals.is_empty() }
    pub fn try_capture_source(&self, id: TryCaptureSourceId) -> Result<&TryCaptureSource, IrVerifyError> {
        let source = owned(self.root, &self.try_captures.sources, id.index, id.proof)?;
        if !self.try_captures.originals.get(id.index as usize).is_some_and(|original| Arc::ptr_eq(source, original)) {
            return Err(failure("try capture differs from its original source receipt"));
        }
        Ok(source.as_ref())
    }
    pub fn try_capture_sources(&self) -> impl Iterator<Item = (TryCaptureSourceId, &TryCaptureSource)> {
        self.try_captures.sources.iter().enumerate().map(|(index, entry)| (TryCaptureSourceId {
            index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn try_capture_source_at(&self, instruction: u32) -> Result<Option<TryCaptureSourceId>, IrVerifyError> {
        if self.try_captures.sources.len() != self.try_captures.originals.len() { return Err(failure("capture original ledger is incomplete")); }
        self.try_captures.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| {
            let id = self.try_captures.instructions[index].1; self.try_capture_source(id)?; Ok(id)
        }).transpose()
    }
    pub(super) fn verify_try_capture_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        if self.try_captures.sources.len() != self.try_captures.originals.len() { return Err(failure("try capture original ledger is incomplete")); }
        let mut instructions = Vec::new();
        for (id, _) in self.try_capture_sources() {
            let source = self.try_capture_source(id)?;
            if source.origin.source != source.completion_origin.source || source.origin.namespace != source.completion_origin.namespace
                || owners.get(source.instruction as usize) != Some(&Some(source.owner))
                || owners.get(source.statement as usize) != Some(&Some(source.owner))
                || owners.get(source.tail as usize) != Some(&Some(source.owner))
                || self.registered_instruction_origin(source.instruction, false) != Some((OperationSourceOrigin::Expression(source.origin), source.owner))
                || self.registered_instruction_origin(source.tail, false) != Some((OperationSourceOrigin::Expression(source.completion_origin), source.owner)) {
                return Err(failure("try capture changes its original source, completion, or owner"));
            }
            Self::verify_type(pools, source.carrier)?; Self::verify_type(pools, source.completion)?;
            let Some((success, error)) = pools.type_children(source.carrier)? else { return Err(failure("try capture carrier has no Result relationship")); };
            if pools.to_type(source.carrier)? != source.original_carrier || pools.to_type(source.completion)? != source.original_completion
                || source.propagation.map(|ty| pools.to_type(ty)).transpose()? != source.original_propagation {
                return Err(failure("capture differs from its original checked result or error aggregate"));
            }
            let completion_is_result = source.retry.as_ref().is_some_and(|retry| retry.completion_is_result);
            if let Some(relation) = &source.error_capture { relation.verify(source)?; }
            if pools.type_tag(source.carrier)? != TypeTag::Result || if completion_is_result { source.carrier != source.completion } else { success != source.completion } {
                return Err(failure("try capture changes its original completion relationship"));
            }
            match (source.propagation_origin, source.propagation, source.producer, source.producer_source, &source.producer_code) {
                (Some((origin, _)), Some(propagation), Some(producer), Some(material), Some(_)) => {
                    if origin.source != source.origin.source || origin.namespace != source.origin.namespace
                        || owners.get(producer as usize) != Some(&Some(source.owner)) || owners.get(material as usize) != Some(&Some(source.owner))
                        || self.registered_instruction_origin(material, false) != Some((OperationSourceOrigin::Expression(origin), source.owner))
                        || pools.type_tag(propagation)? != TypeTag::Result
                        || (!completion_is_result && pools.type_children(propagation)?.map(|children| children.0) != Some(source.completion))
                        || !pools.to_type(propagation)?.matches_expected(&source.original_carrier) {
                        return Err(failure("try capture changes its original propagated carrier relationship"));
                    }
                }
                (None, None, None, None, None) if source.error_capture.is_some() || completion_is_result || error.is_some_and(|error| pools.type_tag(error).is_ok_and(|tag| tag == TypeTag::Error)) => {},
                _ => return Err(failure("try capture propagation proof is incomplete")),
            }
            if let Some(retry) = &source.retry {
                for delay in &retry.delays {
                    if delay.origin.source != source.origin.source || delay.origin.namespace != source.origin.namespace
                        || owners.get(delay.instruction as usize) != Some(&Some(source.owner))
                        || self.registered_instruction_origin(delay.instruction, false) != Some((OperationSourceOrigin::Expression(delay.origin), source.owner))
                        || pools.to_type(delay.ty)? != delay.original_type || delay.original_type != crate::sema::types::Type::Duration {
                        return Err(failure("retry delay changes its original source, owner or Duration contract"));
                    }
                }
                if let Some((origin, _, pattern)) = retry.selection {
                    let Some((_, selected)) = self.pattern_sources().find(|(_, selected)| selected.origin == origin) else { return Err(failure("retry selection lacks its original checked pattern")); };
                    if origin.source != source.origin.source || origin.namespace != source.origin.namespace
                        || self.registered_pattern_origin(pattern) != Some((origin, source.owner)) || !selected.captures.is_empty() {
                        return Err(failure("retry selection changes its original source, owner or nonbinding policy"));
                    }
                    let TypeRef::Ground(input) = selected.input else { return Err(failure("retry selection input is not ground")); };
                    if pools.type_children(source.carrier)?.and_then(|children| children.1) != Some(input) { return Err(failure("retry selection changes its checked error aggregate")); }
                }
            }
            instructions.push((source.instruction, id));
        }
        instructions.sort_unstable_by_key(|entry| entry.0);
        if instructions.windows(2).any(|pair| pair[0].0 == pair[1].0) || instructions != self.try_captures.instructions {
            return Err(failure("try capture instruction index is ambiguous or stale"));
        }
        Ok(())
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_try_capture_sources(&mut self) { self.try_captures.sources.clear(); self.try_captures.instructions.clear(); }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_try_capture_source_mut(&mut self, id: TryCaptureSourceId) -> Result<&mut TryCaptureSource, IrVerifyError> {
        self.try_capture_source(id)?; Ok(Arc::make_mut(&mut self.try_captures.sources[id.index as usize].value))
    }
}

impl GenericEvidenceBuilder {
    pub(in crate::runtime::eval) fn retry_selection_origins(&self) -> Vec<crate::sema::check::PatternIdentity> {
        self.store.try_captures.sources.iter().filter_map(|entry| entry.value.retry.as_ref().and_then(|policy| policy.selection.map(|selection| selection.0))).collect()
    }
    pub fn add_try_capture_source(&mut self, value: TryCaptureSource) -> Result<TryCaptureSourceId, IrVerifyError> {
        if self.store.try_captures.sources.len() >= 2_000_000 { return Err(failure("try captures exceed their work limit")); }
        let index = u32::try_from(self.store.try_captures.sources.len()).map_err(|_| failure("try capture source id overflow"))?;
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let value = Arc::new(value);
        self.store.try_captures.originals.push(Arc::clone(&value));
        self.store.try_captures.sources.push(Entry { serial, value });
        Ok(TryCaptureSourceId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
}
