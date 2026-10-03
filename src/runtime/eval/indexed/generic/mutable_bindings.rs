use super::*;
use crate::sema::check::{BindingIdentity, ExpressionIdentity, StatementIdentity};
use crate::sema::inference::ScopedRoot;
use super::super::full::{FullTag, FullDriverTag};

// Nullary source enums have an inert storage invariant only when every member
// belongs to the original qualified declaration and has no payload fields.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct MutableNominalInvariant {
    pub identity: crate::sema::check::QualifiedNominalIdentity,
    pub family: Name,
    pub root: ScopedRoot,
    pub members: Box<[(crate::sema::check::QualifiedNominalIdentity, Arc<crate::sema::check::SolvedNominalMember>)]>,
}

impl MutableNominalInvariant {
    pub(in crate::runtime::eval) fn from_checked(solved: &crate::sema::check::SolvedTypes, root: ScopedRoot) -> Result<Option<Self>, IrVerifyError> {
        use crate::sema::check::{NominalDeclaration, NominalMemberKind, QualifiedNominalIdentity};
        let ty = graph_ground_type(&solved.graph, root.ty).map_err(|_| failure("mutable nominal root is not closed"))?;
        let Type::Tag(family) = ty else { return Ok(None); };
        solved.graph.validate_scoped(root).map_err(|_| failure("mutable nominal root changes its original scope"))?;
        let resolved = solved.graph.resolved(root.ty).map_err(|_| failure("mutable nominal root is invalid"))?;
        let identity = *solved.nominals.get(&resolved).ok_or_else(|| failure("mutable nominal root lacks original declaration authority"))?;
        let QualifiedNominalIdentity::Source { source, namespace, declaration: declaration @ NominalDeclaration::Type(_), member: None } = identity else { return Err(failure("mutable nominal root is not an original source enum")); };
        let mut members = Vec::new();
        for (&member_identity, original) in &solved.nominal_members {
            let QualifiedNominalIdentity::Source { source: member_source, namespace: member_namespace, declaration: member_declaration, member: Some(member) } = member_identity else { continue; };
            if (member_source, member_namespace, member_declaration) != (source, namespace, declaration) { continue; }
            let checked = solved.checked_nominal_member(member_identity).map_err(|_| failure("mutable nominal member changes its original declaration"))?;
            if checked.kind != NominalMemberKind::Tag || checked.family != family || checked.member != member || checked.scope.is_some() || !checked.fields.is_empty() || !checked.facets.is_empty()
                || solved.graph.resolved(checked.tested).ok() != Some(resolved) { return Err(failure("mutable enum storage requires original nullary members")); }
            members.push((member_identity, Arc::clone(original)));
        }
        if members.is_empty() { return Err(failure("mutable nominal root has no original members")); }
        Ok(Some(Self { identity, family, root, members: members.into_boxed_slice() }))
    }

    fn retained_bytes(&self) -> usize {
        self.members.len() * (std::mem::size_of::<(crate::sema::check::QualifiedNominalIdentity, Arc<crate::sema::check::SolvedNominalMember>)>() + std::mem::size_of::<crate::sema::check::SolvedNominalMember>() + 2 * std::mem::size_of::<usize>())
    }

    fn verify(&self, receipt: &MutableBindingReceipt, pools: &SemanticPools) -> Result<(), IrVerifyError> {
        use crate::sema::check::{NominalDeclaration, NominalMemberKind, QualifiedNominalIdentity};
        let QualifiedNominalIdentity::Source { source, namespace, declaration: declaration @ NominalDeclaration::Type(_), member: None } = self.identity else { return Err(failure("mutable nominal invariant loses its original declaration")); };
        if self.root.ty != receipt.binding_root.ty || self.root.scope != receipt.binding_root.scope || self.members.is_empty() || receipt.capture.is_some() || receipt.captured_path.is_some()
            || pools.to_type(receipt.binding_type)? != Type::Tag(self.family) || receipt.value_type.is_some_and(|ty| ty != receipt.binding_type)
            || receipt.assignment.is_some_and(|op| op != crate::syntax::node::AssignOp::Set) { return Err(failure("mutable nominal invariant changes its original storage relationship")); }
        let mut names = std::collections::BTreeSet::new();
        for (identity, member) in &self.members {
            if *identity != (QualifiedNominalIdentity::Source { source, namespace, declaration, member: Some(member.member) })
                || member.kind != NominalMemberKind::Tag || member.family != self.family || member.scope.is_some() || !member.fields.is_empty() || !member.facets.is_empty()
                || !names.insert(member.member) { return Err(failure("mutable nominal invariant changes its original member authority")); }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct MutableCompoundAssignment {
    pub operation: crate::sema::operation_graph::PreparedLanguageOperation,
    pub requirement: crate::sema::inference::RequirementId,
    pub scope: Option<crate::sema::inference::SchemeId>,
    pub effects: crate::sema::inference::EffectSet,
    pub left: GroundTypeId,
    pub right: GroundTypeId,
    pub result: GroundTypeId,
}

// Guarded reads keep checked storage and narrowed projection types separately.
// Their source metadata contains identities and scoped roots, never a frontend graph.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct MutableReadRefinement {
    pub source: Arc<crate::sema::check::SolvedRefinedRead>,
    pub narrowed_type: GroundTypeId,
    pub predicate: u32,
    pub subject: u32,
    pub guard: u32,
    pub condition: u32,
    pub aliases: Box<[(u32, u32)]>,
    pub writes: Box<[u32]>,
    pub rows: Box<[(u32, FullTag, Box<[u32]>) ]>,
    pub blocks: Box<[(u32, u32, u8, Box<[u32]>) ]>,
}
impl MutableReadRefinement {
    fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + std::mem::size_of::<crate::sema::check::SolvedRefinedRead>() + 2 * std::mem::size_of::<usize>()
            + (self.source.path.len() + self.source.read_path.len() + self.source.writes.iter().map(|write| write.path.len()).sum::<usize>()) * std::mem::size_of::<Name>()
            + self.source.aliases.capacity() * std::mem::size_of::<crate::sema::check::SolvedRefinementAlias>()
            + self.source.writes.capacity() * std::mem::size_of::<crate::sema::check::SolvedRefinementWrite>()
            + self.aliases.len() * std::mem::size_of::<(u32, u32)>() + self.writes.len() * std::mem::size_of::<u32>()
            + self.rows.len() * std::mem::size_of::<(u32, FullTag, Box<[u32]>)>() + self.rows.iter().map(|(_, _, payload)| payload.len() * std::mem::size_of::<u32>()).sum::<usize>()
            + self.blocks.len() * std::mem::size_of::<(u32, u32, u8, Box<[u32]>)>() + self.blocks.iter().map(|(_, _, _, payload)| payload.len() * std::mem::size_of::<u32>()).sum::<usize>()
    }
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct MutableBindingReceipt {
    pub binding: BindingIdentity,
    pub nominal: Option<MutableNominalInvariant>,
    pub capture: Option<LexicalCaptureId>,
    // A captured path write refers to the original protected path receipt at
    // this instruction. It has no whole-binding producer version or allocation.
    pub captured_path: Option<u32>,
    pub statement: Option<StatementIdentity>,
    pub read_origin: Option<OperationSourceOrigin>,
    pub refinement: Option<MutableReadRefinement>,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub tag: FullTag,
    pub payload: Box<[u32]>,
    pub binding_type: GroundTypeId,
    pub binding_root: ScopedRoot,
    pub value: Option<u32>,
    pub value_wrappers: Box<[(u32, FullTag, Box<[u32]>)]>,
    pub value_source: Option<ExpressionIdentity>,
    pub value_type: Option<GroundTypeId>,
    pub value_root: Option<ScopedRoot>,
    pub ordinal: u32,
    pub assignment: Option<crate::syntax::node::AssignOp>,
    pub compound: Option<MutableCompoundAssignment>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct MutableDriverReceipt {
    pub binding: BindingIdentity,
    pub statement: StatementIdentity,
    pub step: u32,
    pub name: Name,
    pub tag: FullDriverTag,
    pub payload: Box<[u32]>,
    pub binding_type: GroundTypeId,
    pub binding_root: ScopedRoot,
    pub value: u32,
    pub value_wrappers: Box<[(u32, FullTag, Box<[u32]>)]>,
    pub value_source: ExpressionIdentity,
    pub value_type: GroundTypeId,
    pub value_root: ScopedRoot,
    pub ordinal: u32,
    pub assignment: Option<crate::syntax::node::AssignOp>,
    pub compound: Option<MutableCompoundAssignment>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct MutableBindingEvidence {
    entries: Vec<Entry<Arc<MutableBindingReceipt>>>,
    originals: Vec<Arc<MutableBindingReceipt>>,
    instructions: Vec<(u32, usize)>,
    drivers: Vec<Entry<Arc<MutableDriverReceipt>>>,
    original_drivers: Vec<Arc<MutableDriverReceipt>>,
    driver_steps: Vec<(u32, usize)>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct MutableBindingCheckpoint { entries: usize, drivers: usize }
impl MutableBindingEvidence {
    pub(super) fn checkpoint(&self) -> MutableBindingCheckpoint { MutableBindingCheckpoint { entries: self.entries.len(), drivers: self.drivers.len() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: MutableBindingCheckpoint, serial_limit: u64) -> Result<(), IrVerifyError> {
        if checkpoint.entries > self.entries.len() || self.entries.get(checkpoint.entries.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial_limit) { return Err(failure("mutable binding checkpoint references retired or replacement entries")); }
        if checkpoint.drivers > self.drivers.len() || self.drivers.get(checkpoint.drivers.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial_limit) { return Err(failure("mutable driver checkpoint references retired or replacement entries")); }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: MutableBindingCheckpoint) { self.entries.truncate(checkpoint.entries); self.originals.truncate(checkpoint.entries); self.instructions.clear(); self.drivers.truncate(checkpoint.drivers); self.original_drivers.truncate(checkpoint.drivers); self.driver_steps.clear(); }
    pub(super) fn finish(&mut self) { self.instructions = self.entries.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect(); self.instructions.sort_unstable_by_key(|entry| entry.0); self.driver_steps = self.drivers.iter().enumerate().map(|(index, entry)| (entry.value.step, index)).collect(); self.driver_steps.sort_unstable_by_key(|entry| entry.0); }
    pub(super) fn shrink_to_fit(&mut self) { self.entries.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); self.drivers.shrink_to_fit(); self.original_drivers.shrink_to_fit(); self.driver_steps.shrink_to_fit(); }
    pub(super) fn retained_bytes(&self) -> usize {
        self.entries.capacity() * std::mem::size_of::<Entry<Arc<MutableBindingReceipt>>>() + self.originals.capacity() * std::mem::size_of::<Arc<MutableBindingReceipt>>() + self.instructions.capacity() * std::mem::size_of::<(u32, usize)>()
            + self.entries.iter().map(|entry| std::mem::size_of::<MutableBindingReceipt>() + 2 * std::mem::size_of::<usize>() + entry.value.payload.len() * std::mem::size_of::<u32>() + entry.value.value_wrappers.iter().map(|(_, _, payload)| std::mem::size_of::<(u32, FullTag, Box<[u32]>)>() + payload.len() * std::mem::size_of::<u32>()).sum::<usize>()).sum::<usize>()
            + self.entries.iter().filter_map(|entry| entry.value.refinement.as_ref()).map(MutableReadRefinement::retained_bytes).sum::<usize>()
            + self.entries.iter().filter_map(|entry| entry.value.nominal.as_ref()).map(MutableNominalInvariant::retained_bytes).sum::<usize>()
            + self.drivers.capacity() * std::mem::size_of::<Entry<Arc<MutableDriverReceipt>>>() + self.original_drivers.capacity() * std::mem::size_of::<Arc<MutableDriverReceipt>>() + self.driver_steps.capacity() * std::mem::size_of::<(u32, usize)>()
            + self.drivers.iter().map(|entry| std::mem::size_of::<MutableDriverReceipt>() + 2 * std::mem::size_of::<usize>() + entry.value.payload.len() * std::mem::size_of::<u32>() + entry.value.value_wrappers.iter().map(|(_, _, payload)| std::mem::size_of::<(u32, FullTag, Box<[u32]>)>() + payload.len() * std::mem::size_of::<u32>()).sum::<usize>()).sum::<usize>()
    }
}
impl GenericEvidenceStore {
    pub fn has_mutable_bindings(&self) -> bool { !self.mutables.entries.is_empty() || !self.mutables.originals.is_empty() || !self.mutables.drivers.is_empty() || !self.mutables.original_drivers.is_empty() }
    pub fn mutable_driver_receipts(&self) -> impl Iterator<Item = &MutableDriverReceipt> { self.mutables.drivers.iter().map(|entry| entry.value.as_ref()) }
    pub fn mutable_driver_receipt(&self, step: u32) -> Result<Option<&MutableDriverReceipt>, IrVerifyError> {
        let Some(position) = self.mutables.driver_steps.binary_search_by_key(&step, |entry| entry.0).ok() else { return Ok(None); };
        let index = self.mutables.driver_steps[position].1;
        let entry = self.mutables.drivers.get(index).ok_or_else(|| failure("mutable driver index is stale"))?;
        if !self.mutables.original_drivers.get(index).is_some_and(|original| Arc::ptr_eq(original, &entry.value)) { return Err(failure("mutable driver differs from its original receipt")); }
        Ok(Some(entry.value.as_ref()))
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_mutable_driver_receipt(&mut self, step: u32) {
        if let Some(index) = self.mutables.drivers.iter().position(|entry| entry.value.step == step) { self.mutables.drivers.remove(index); }
        self.mutables.finish();
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_mutable_driver_receipt_mut(&mut self, step: u32) -> Result<&mut MutableDriverReceipt, IrVerifyError> {
        self.mutable_driver_receipt(step)?.ok_or_else(|| failure("mutable driver receipt is missing"))?;
        let position = self.mutables.driver_steps.binary_search_by_key(&step, |entry| entry.0).unwrap();
        let index = self.mutables.driver_steps[position].1;
        Ok(Arc::make_mut(&mut self.mutables.drivers[index].value))
    }
    pub fn mutable_binding_receipts(&self) -> impl Iterator<Item = &MutableBindingReceipt> { self.mutables.entries.iter().map(|entry| entry.value.as_ref()) }
    pub fn mutable_binding_receipt(&self, instruction: u32) -> Result<Option<&MutableBindingReceipt>, IrVerifyError> {
        let Some(index) = self.mutables.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|position| self.mutables.instructions[position].1) else { return Ok(None); };
        let entry = self.mutables.entries.get(index).ok_or_else(|| failure("mutable binding instruction index is stale"))?;
        if !self.mutables.originals.get(index).is_some_and(|original| Arc::ptr_eq(original, &entry.value)) { return Err(failure("mutable binding differs from its original receipt")); }
        Ok(Some(entry.value.as_ref()))
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_mutable_binding_receipt_mut(&mut self, instruction: u32) -> Result<&mut MutableBindingReceipt, IrVerifyError> {
        self.mutable_binding_receipt(instruction)?.ok_or_else(|| failure("mutable binding receipt is missing"))?;
        let position = self.mutables.instructions.binary_search_by_key(&instruction, |entry| entry.0).unwrap();
        let index = self.mutables.instructions[position].1;
        Ok(Arc::make_mut(&mut self.mutables.entries[index].value))
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_mutable_binding_receipt(&mut self, instruction: u32) {
        if let Some(index) = self.mutables.entries.iter().position(|entry| entry.value.instruction == instruction) { self.mutables.entries.remove(index); }
        self.mutables.finish();
    }
    pub(super) fn verify_mutable_binding_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        if self.mutables.entries.len() != self.mutables.originals.len() { return Err(failure("mutable binding original receipt ledger is incomplete")); }
        if self.mutables.drivers.len() != self.mutables.original_drivers.len() { return Err(failure("mutable driver original receipt ledger is incomplete")); }
        let mut driver_steps = Vec::new();
        for (index, receipt) in self.mutable_driver_receipts().enumerate() {
            self.mutable_driver_receipt(receipt.step)?;
            if receipt.binding.source != receipt.statement.source || receipt.binding.namespace != receipt.statement.namespace
                || receipt.value_source.source != receipt.statement.source || receipt.value_source.namespace != receipt.statement.namespace
                || owners.get(receipt.value as usize) != Some(&Some(InstructionOwner::Driver(receipt.step)))
                || self.registered_instruction_origin(receipt.value, false) != Some((OperationSourceOrigin::Expression(receipt.value_source), InstructionOwner::Driver(receipt.step)))
                || (receipt.ordinal == 0 && (receipt.tag != FullDriverTag::Let || receipt.assignment.is_some() || receipt.compound.is_some()))
                || (receipt.ordinal != 0 && (receipt.tag != FullDriverTag::Assign || receipt.assignment.is_none())) { return Err(failure("mutable driver changes its original source, allocation, or role")); }
            for ty in [receipt.binding_type, receipt.value_type] { Self::verify_type(pools, ty)?; }
            if let Some(compound) = &receipt.compound {
                if compound.effects != crate::sema::inference::EffectSet::EMPTY || compound.left != receipt.binding_type || compound.right != receipt.value_type || compound.result != receipt.binding_type
                    || !matches!(compound.operation, crate::sema::operation_graph::PreparedLanguageOperation::Compound { op, .. } if Some(op) == receipt.assignment) { return Err(failure("mutable driver compound changes its original selection")); }
            }
            if receipt.assignment.is_some_and(|op| op != crate::syntax::node::AssignOp::Set) != receipt.compound.is_some() { return Err(failure("mutable driver compound loses its original selection")); }
            driver_steps.push((receipt.step, index));
        }
        driver_steps.sort_unstable_by_key(|entry| entry.0);
        if driver_steps.windows(2).any(|entries| entries[0].0 == entries[1].0) || driver_steps != self.mutables.driver_steps { return Err(failure("mutable driver index is incomplete or ambiguous")); }
        let mut instructions = std::collections::BTreeSet::new();
        for receipt in self.mutable_binding_receipts() {
            self.mutable_binding_receipt(receipt.instruction)?;
            if !instructions.insert(receipt.instruction) || owners.get(receipt.instruction as usize) != Some(&Some(receipt.owner)) { return Err(failure("mutable binding changes its original owner or allocation")); }
            if let Some(capture) = receipt.capture {
                let allocation = self.lexical_capture(capture)?;
                if !allocation.mutable || receipt.binding != allocation.binding || receipt.owner != InstructionOwner::Function(allocation.target) || receipt.binding_type != allocation.ty
                    || receipt.binding_root.ty != allocation.source_type.ty || receipt.binding_root.scope != allocation.source_type.scope || receipt.payload.first() != Some(&allocation.slot)
                    || receipt.refinement.is_some() || (receipt.read_origin.is_none() && receipt.ordinal == 0 && receipt.captured_path.is_none()) { return Err(failure("mutable capture changes its original source allocation")); }
            }
            if let Some(instruction) = receipt.captured_path {
                let path = self.mutable_path_at(instruction)?.ok_or_else(|| failure("captured path loses its original selected storage receipt"))?;
                if receipt.capture.is_none() || instruction != receipt.instruction || path.binding != receipt.binding || path.owner != receipt.owner || path.payload != receipt.payload
                    || path.binding_type != receipt.binding_type || path.binding_root.ty != receipt.binding_root.ty || path.binding_root.scope != receipt.binding_root.scope
                    || receipt.statement != Some(path.statement) || receipt.value != Some(path.value) || receipt.value_source != Some(path.value_source) || receipt.value_type != Some(path.value_type)
                    || receipt.value_root.is_none_or(|root| root.ty != path.value_root.ty || root.scope != path.value_root.scope) || receipt.compound.is_some() || receipt.ordinal != 0 || !receipt.value_wrappers.is_empty() || receipt.read_origin.is_some()
                    || !matches!(receipt.tag, FullTag::StmtAssignPath | FullTag::StmtAssignField | FullTag::StmtAssignFieldInt) { return Err(failure("captured path changes its original cell or selected storage relationship")); }
                let op = match path.compound.as_ref().map(|compound| compound.operation) { Some(crate::sema::operation_graph::PreparedLanguageOperation::Compound { op, .. }) => op, None => crate::syntax::node::AssignOp::Set, _ => return Err(failure("captured path changes its original selected operation")) };
                if receipt.assignment != Some(op) { return Err(failure("captured path changes its original assignment operator")); }
            }
            let origin = receipt.read_origin.or(receipt.statement.map(OperationSourceOrigin::Statement)).ok_or_else(|| failure("mutable binding lacks its original source"))?;
            if self.registered_instruction_origin(receipt.instruction, false) != Some((origin, receipt.owner)) { return Err(failure("mutable binding changes its original source identity")); }
            if receipt.read_origin.is_some() {
                if receipt.statement.is_some() || receipt.value.is_some() || receipt.value_source.is_some() || receipt.value_type.is_some() || receipt.value_root.is_some() || !receipt.value_wrappers.is_empty() || receipt.ordinal != 0
                    || receipt.assignment.is_some() || receipt.compound.is_some()
                    || !matches!(receipt.tag, FullTag::ExprParam | FullTag::IntSlot | FullTag::BoolSlot) { return Err(failure("mutable read changes its original role")); }
            } else if receipt.captured_path.is_none() {
                let statement = receipt.statement.ok_or_else(|| failure("mutable write lacks its original statement"))?;
                let value_source = receipt.value_source.ok_or_else(|| failure("mutable write lacks its original value source"))?;
                if receipt.value.is_none() || receipt.value_type.is_none() || receipt.value_root.is_none()
                    || statement.source != receipt.binding.source || statement.namespace != receipt.binding.namespace
                    || value_source.source != receipt.binding.source || value_source.namespace != receipt.binding.namespace
                    || (receipt.ordinal == 0 && !matches!(receipt.tag, FullTag::StmtLet | FullTag::StmtLetInt | FullTag::StmtLetBool))
                    || (receipt.ordinal == 0 && (receipt.assignment.is_some() || receipt.compound.is_some()))
                    || (receipt.ordinal != 0 && receipt.assignment.is_none())
                    || (receipt.ordinal != 0 && !matches!(receipt.tag, FullTag::StmtAssign | FullTag::StmtAssignInt | FullTag::StmtAssignBool)) { return Err(failure("mutable write changes its original source or role")); }
            }
            if let Some(refinement) = &receipt.refinement {
                if receipt.read_origin.is_none() || refinement.source.binding != receipt.binding || !refinement.source.read_path.is_empty()
                    || refinement.source.invariant.ty != receipt.binding_root.ty || refinement.source.invariant.scope != receipt.binding_root.scope
                    || refinement.guard == u32::MAX || refinement.aliases.len() != refinement.source.aliases.len() || refinement.writes.len() != refinement.source.writes.len()
                    || !refinement.source.path.starts_with(&refinement.source.read_path) { return Err(failure("mutable refinement changes its original checked relationship")); }
                Self::verify_type(pools, refinement.narrowed_type)?;
            }
            Self::verify_type(pools, receipt.binding_type)?;
            match (&receipt.nominal, pools.to_type(receipt.binding_type)?) {
                (Some(nominal), Type::Tag(_)) => nominal.verify(receipt, pools)?,
                (None, Type::Tag(_)) | (Some(_), _) => return Err(failure("mutable enum storage loses its original nominal invariant")),
                _ => {}
            }
            if let Some(ty) = receipt.value_type { Self::verify_type(pools, ty)?; }
            if let Some(compound) = &receipt.compound {
                if compound.effects != crate::sema::inference::EffectSet::EMPTY || compound.left != receipt.binding_type || Some(compound.right) != receipt.value_type || compound.result != receipt.binding_type
                    || !matches!(compound.operation, crate::sema::operation_graph::PreparedLanguageOperation::Compound { op, .. } if Some(op) == receipt.assignment) { return Err(failure("mutable compound changes its original selected relationship")); }
                for ty in [compound.left, compound.right, compound.result] { Self::verify_type(pools, ty)?; }
            }
            if receipt.captured_path.is_none() && (receipt.assignment.is_some_and(|op| op != crate::syntax::node::AssignOp::Set) != receipt.compound.is_some()) { return Err(failure("mutable compound loses its original selection")); }
        }
        let mut expected: Vec<_> = self.mutables.entries.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect();
        expected.sort_unstable_by_key(|entry| entry.0);
        if expected != self.mutables.instructions { return Err(failure("mutable binding instruction index is incomplete")); }
        Ok(())
    }
}
impl GenericEvidenceBuilder {
    pub(in crate::runtime::eval) fn mutable_refinement_preparations(&self) -> Vec<MutableBindingReceipt> {
        self.store.mutables.entries.iter().filter(|entry| entry.value.refinement.is_some()).map(|entry| entry.value.as_ref().clone()).collect()
    }
    pub(in crate::runtime::eval) fn mutable_original_instruction(&self, origin: OperationSourceOrigin, owner: InstructionOwner) -> Result<u32, IrVerifyError> {
        let mut found = None;
        for &(instruction, actual, actual_owner) in &self.store.instruction_origins {
            if actual == origin && actual_owner == owner && found.replace(instruction).is_some_and(|previous| previous != instruction) {
                return Err(failure("mutable refinement source has ambiguous physical instructions"));
            }
        }
        found.ok_or_else(|| failure("mutable refinement source lacks its original instruction"))
    }
    pub(in crate::runtime::eval) fn complete_mutable_refinement(&mut self, instruction: u32, refinement: MutableReadRefinement) -> Result<(), IrVerifyError> {
        let index = self.store.mutables.entries.iter().position(|entry| entry.value.instruction == instruction).ok_or_else(|| failure("mutable refinement read is missing"))?;
        let entry = &mut self.store.mutables.entries[index];
        if !self.store.mutables.originals.get(index).is_some_and(|original| Arc::ptr_eq(original, &entry.value)) { return Err(failure("mutable refinement read changed before publication")); }
        let mut receipt = entry.value.as_ref().clone();
        let pending = receipt.refinement.as_ref().ok_or_else(|| failure("mutable refinement source is missing"))?;
        if !Arc::ptr_eq(&pending.source, &refinement.source) || pending.narrowed_type != refinement.narrowed_type { return Err(failure("mutable refinement changes its original checked source")); }
        receipt.refinement = Some(refinement);
        entry.value = Arc::new(receipt);
        self.store.mutables.originals[index] = Arc::clone(&entry.value);
        Ok(())
    }
    pub fn add_mutable_driver_receipt(&mut self, receipt: MutableDriverReceipt) -> Result<(), IrVerifyError> {
        if self.store.mutables.drivers.len() >= 2_000_000 { return Err(failure("mutable driver bindings exceed their work limit")); }
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("mutable driver serial overflow"))?;
        let value = Arc::new(receipt);
        self.store.mutables.original_drivers.push(Arc::clone(&value));
        self.store.mutables.drivers.push(Entry { serial, value });
        Ok(())
    }
    pub fn add_mutable_binding_receipt(&mut self, receipt: MutableBindingReceipt) -> Result<(), IrVerifyError> {
        if self.store.mutables.entries.len() >= 2_000_000 { return Err(failure("mutable bindings exceed their work limit")); }
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("mutable binding serial overflow"))?;
        let value = Arc::new(receipt);
        self.store.mutables.originals.push(Arc::clone(&value));
        self.store.mutables.entries.push(Entry { serial, value });
        Ok(())
    }
}
