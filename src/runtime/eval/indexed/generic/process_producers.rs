use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct PreparedProcessArgvRow {
    pub instruction: u32,
    pub tag: super::super::full::FullTag,
    pub payload: Box<[u32]>,
}

/// A command factory keeps its authored argv and policy before a later
/// process consumer can create a child or perform output effects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct PreparedProcessCommandArgv {
    pub payload: Box<[u32]>,
    pub transports: Box<[(u32, u32)]>,
    pub rows: Box<[PreparedProcessArgvRow]>,
    pub blocks: Box<[(super::super::IrBlockId, Box<[u32]>)]>,
    pub texts: Box<[(u32, Arc<str>)]>,
    pub bytes: Box<[(u32, Arc<[u8]>)]>,
}

impl PreparedProcessCommandArgv {
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.payload.len() * size_of::<u32>() + self.transports.len() * size_of::<(u32, u32)>() + self.rows.len() * size_of::<PreparedProcessArgvRow>()
            + self.rows.iter().map(|row| row.payload.len() * size_of::<u32>()).sum::<usize>()
            + self.blocks.len() * size_of::<(super::super::IrBlockId, Box<[u32]>)>()
            + self.blocks.iter().map(|(_, words)| words.len() * size_of::<u32>()).sum::<usize>()
            + self.texts.len() * size_of::<(u32, Arc<str>)>() + self.texts.iter().map(|(_, value)| value.len()).sum::<usize>()
            + self.bytes.len() * size_of::<(u32, Arc<[u8]>)>() + self.bytes.iter().map(|(_, value)| value.len()).sum::<usize>()
    }
}

impl GenericEvidenceStore {
    // A heterogeneous argv keeps each original Str or Path child independently
    // of its erased list carrier. The argv[0] value retains its authored position.
    pub(in crate::runtime::eval::indexed) fn verify_process_argv_container(&self, pools: &SemanticPools,
        source: &NativeCallSource, contract: &GroundNativeCallContract, ty: GroundTypeId,
    ) -> Result<bool, IrVerifyError> {
        let Some(snapshot) = &contract.process_command_argv else { return Ok(false); };
        let Some(argv) = contract.argument_sources.get(1).copied().flatten() else { return Ok(false); };
        let material = snapshot.transports.iter().find_map(|&(read, material)| (read == argv).then_some(material)).unwrap_or(argv);
        let Some(id) = self.ground_container_at(material)? else { return Ok(false); };
        let container = self.ground_container_source(id)?;
        if container.owner != source.owner || container.result != ty || container.kind != ContainerKind::List
            || container.operands.is_empty() || container.operands.len() > 65535 {
            return Ok(false);
        }
        let crate::sema::types::Type::List(item) = pools.to_type(ty)? else { return Ok(false); };
        if !matches!(item.as_ref(), crate::sema::types::Type::Str | crate::sema::types::Type::Path | crate::sema::types::Type::Any) {
            return Ok(false);
        }
        for (index, operand) in container.operands.iter().enumerate() {
            let actual = pools.to_type(operand.source_type)?;
            if operand.role != ContainerOperandRole::ListItem(index as u32) || operand.source_type != operand.ty
                || !matches!(actual, crate::sema::types::Type::Str | crate::sema::types::Type::Path)
                || item.as_ref() != &crate::sema::types::Type::Any && item.as_ref() != &actual
                || !matches!(operand.origin, ContainerOperandOrigin::Expression(origin)
                    if self.registered_instruction_origin(operand.source_instruction, false) == Some((OperationSourceOrigin::Expression(origin), source.owner))) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn verify_process_command_argv_contract(&self, pools: &SemanticPools, source: &NativeCallSource, contract: &GroundNativeCallContract, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        let selected = matches!(contract.authority, PreparedOperationAuthority::Registry {
            operation: crate::modules::RuntimeOp::ProcessCommandArgv,
            argument_check: crate::modules::signature::ApiArgCheck::CommandArgv,
            binding: crate::modules::signature::ImplBinding::Native,
            semantic_rule: crate::modules::signature::SemanticRule::Standard, ..
        });
        let Some(snapshot) = &contract.process_command_argv else {
            if selected { return Err(failure("command argv factory lacks its original operand and policy receipt")); }
            return Ok(());
        };
        if !selected || contract.receiver.is_some() || contract.cli_descriptor.is_some()
            || contract.kind != CallableKind::Pure || contract.effects.creation != crate::sema::inference::EffectSet::EMPTY
            || !contract.effects.inputs.is_empty() || !contract.effects.outputs.is_empty()
            || contract.argument_sources.len() != 15 || snapshot.rows.len() > 65536 || snapshot.blocks.len() > 2 {
            return Err(failure("command argv factory changes its original authority or effect boundary"));
        }
        let TypeRef::Ground(result) = contract.result else { return Err(failure("command argv result is not ground")); };
        if pools.type_tag(result)? != TypeTag::Command || contract.argument_sources.get(0).copied().flatten().is_none()
            || contract.argument_sources.get(1).copied().flatten().is_none()
            || contract.argument_sources[2..9].iter().chain(&contract.argument_sources[10..14]).any(Option::is_some) {
            return Err(failure("command argv factory changes its prepared option or result contract"));
        }
        for (argument, &slot) in contract.arguments.iter().zip(&contract.binding.supplied_slots) {
            let TypeRef::Ground(ty) = argument.ty else { return Err(failure("command argv operand requires its own scoped protocol")); };
            let valid = match slot {
                0 => matches!(pools.type_tag(ty)?, TypeTag::Str | TypeTag::Path),
                1 => self.verify_process_argv_container(pools, source, contract, ty)?,
                9 => pools.type_tag(ty)? == TypeTag::Duration,
                14 => pools.to_type(ty)? == crate::sema::types::Type::List(Box::new(crate::sema::types::Type::Int)),
                _ => false,
            };
            if !valid { return Err(failure("command argv operand changes its checked eligibility domain")); }
        }
        if snapshot.transports.len() > contract.arguments.len() { return Err(failure("command argv saved operand transport is excessive")); }
        for &(read, material) in &snapshot.transports {
            let saved = self.original_argument_binding(read).ok_or_else(|| failure("command argv saved operand lacks its original receipt"))?;
            if !contract.argument_sources.iter().any(|operand| *operand == Some(read)) || saved.owner != source.owner
                || saved.call != source.origin || saved.initializer_source_instruction != material
                || owners.get(material as usize) != Some(&Some(source.owner)) {
                return Err(failure("command argv saved operand changes its original call, owner or source"));
            }
        }
        if snapshot.rows.iter().any(|row| owners.get(row.instruction as usize) != Some(&Some(source.owner))) {
            return Err(failure("command argv factory operand belongs to another owner"));
        }
        Ok(())
    }
}
