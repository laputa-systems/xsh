use super::*;
use super::super::generic::{PreparedProcessArgvRow, PreparedProcessCommandArgv};

fn process_problem(construct: &'static str) -> IrBuildError { IrBuildError::format(construct, None, 0, 0) }

pub(super) fn encoded_process_command_argv_arguments(store: &FullStore, instruction: u32, parameter_count: usize) -> Result<(RuntimeOp, Vec<Option<u32>>, u32), IrVerifyError> {
    if parameter_count != 15 || store.tags.get(instruction as usize) != Some(&FullTag::ExprProcessCommandArgv) {
        return Err(IrVerifyError::new("command argv proof is attached to another factory protocol"));
    }
    let data = store.data.get(instruction as usize).ok_or_else(|| IrVerifyError::new("command argv factory row is missing"))?;
    let mut cursor = FullCursor::new(store.payload(data.range())?);
    let mut arguments = vec![Some(cursor.raw()?), Some(cursor.raw()?)];
    for _ in 2..15 {
        arguments.push(match cursor.raw()? { 0 => None, 1 => Some(cursor.raw()?), _ => return Err(IrVerifyError::new("command argv optional operand presence is invalid")) });
    }
    let location = cursor.raw()?;
    cursor.finish()?;
    Ok((RuntimeOp::ProcessCommandArgv, arguments, location))
}

impl FullBuilder {
    pub(super) fn prepare_process_command_argv_descriptor(&self, instruction: u32) -> Result<Option<PreparedProcessCommandArgv>, IrBuildError> {
        let (_, arguments, _) = encoded_process_command_argv_arguments(&self.store, instruction, 15).map_err(|_| process_problem("command_argv_original_instruction"))?;
        if arguments[2..9].iter().chain(&arguments[10..14]).any(Option::is_some) { return Ok(None); }
        // Saved arguments execute once in their compiler slots. The factory
        // retains those reads and the original material operands independently.
        let mut material_arguments = arguments.clone();
        let mut transports = Vec::new();
        let mut saved_rows = Vec::new();
        for (slot, argument) in arguments.iter().enumerate() {
            let Some(read) = *argument else { continue; };
            if self.store.tags.get(read as usize) != Some(&FullTag::ExprParam) { continue; }
            let Some(saved) = self.prepared_saved_argument_bindings.get(&read) else {
                if slot == 9 { continue; }
                return Ok(None);
            };
            let payload = self.store.payload(self.store.data[read as usize].range()).map_err(|_| process_problem("command_argv_saved_read_payload"))?;
            if payload != [saved.slot] { return Err(process_problem("command_argv_saved_read_changed")); }
            let (material, wrappers) = self.argument_initializer_lineage(saved.initializer, saved.owner)?;
            if material != saved.initializer_source_instruction || wrappers != saved.initializer_wrappers {
                return Err(process_problem("command_argv_saved_initializer_changed"));
            }
            material_arguments[slot] = Some(material);
            transports.push((read, material));
            saved_rows.push(PreparedProcessArgvRow { instruction: read, tag: FullTag::ExprParam, payload: payload.to_vec().into_boxed_slice() });
        }
        let mut rows = saved_rows;
        let mut blocks = Vec::new();
        let mut texts = Vec::new();
        let mut bytes = Vec::new();
        let mut snapshot = |instruction: u32, list: bool, integer: bool, argv_child: bool| -> Result<bool, IrBuildError> {
            let tag = *self.store.tags.get(instruction as usize).ok_or_else(|| process_problem("command_argv_operand_missing"))?;
            let payload = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| process_problem("command_argv_operand_payload"))?.to_vec().into_boxed_slice();
            if list {
                if tag != FullTag::ExprList || payload.len() != 1 { return Ok(false); }
                let id = IrBlockId::from_raw(payload[0]).ok_or_else(|| process_problem("command_argv_list_block"))?;
                let block = self.store.blocks.get(id.index()).ok_or_else(|| process_problem("command_argv_list_block"))?;
                let values = self.store.payload(block.instructions).map_err(|_| process_problem("command_argv_list_payload"))?;
                if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST || values.len() < 2 || values.len() > 65536
                    || values.first().copied().map(|count| count as usize) != Some(values.len() - 1) { return Ok(false); }
                blocks.push((id, values.to_vec().into_boxed_slice()));
            } else if integer {
                if tag != FullTag::ExprInt || payload.len() != 2 { return Ok(false); }
                let value = ((payload[1] as u64) << 32 | payload[0] as u64) as i64;
                if !(0..=255).contains(&value) { return Ok(false); }
            } else {
                match (tag, payload.as_ref()) {
                    (FullTag::ExprStr, [literal]) => texts.push((*literal, Arc::from(self.store.string(*literal).map_err(|_| process_problem("command_argv_literal_text"))?))),
                    (FullTag::ExprPath, [literal]) => bytes.push((*literal, Arc::from(self.store.bytes(*literal).map_err(|_| process_problem("command_argv_literal_path"))?))),
                    _ if argv_child => {},
                    _ => return Ok(false),
                }
            }
            if rows.len() >= 65536 { return Err(process_problem("command_argv_operand_receipt_limit")); }
            rows.push(PreparedProcessArgvRow { instruction, tag, payload });
            Ok(true)
        };
        let target = material_arguments[0].ok_or_else(|| process_problem("command_argv_target_missing"))?;
        if !snapshot(target, false, false, false)? { return Ok(None); }
        for (slot, integer) in [(1, false), (14, true)] {
            let Some(root) = material_arguments[slot] else { continue; };
            if !snapshot(root, true, integer, false)? { return Ok(None); }
            let list = self.store.payload(self.store.data[root as usize].range()).map_err(|_| process_problem("command_argv_list_payload"))?;
            let block = IrBlockId::from_raw(list[0]).ok_or_else(|| process_problem("command_argv_list_block"))?;
            let items = self.store.payload(self.store.blocks[block.index()].instructions).map_err(|_| process_problem("command_argv_list_payload"))?;
            for &item in &items[1..] { if !snapshot(item, false, integer, slot == 1)? { return Ok(None); } }
        }
        if let Some(timeout) = material_arguments[9] {
            let tag = *self.store.tags.get(timeout as usize).ok_or_else(|| process_problem("command_argv_timeout_missing"))?;
            let payload = self.store.payload(self.store.data[timeout as usize].range()).map_err(|_| process_problem("command_argv_timeout_payload"))?;
            // The timeout's native argument lineage retains its checked
            // Duration producer, including arithmetic and immutable reads.
            if rows.len() >= 65536 { return Err(process_problem("command_argv_operand_receipt_limit")); }
            rows.push(PreparedProcessArgvRow { instruction: timeout, tag, payload: payload.to_vec().into_boxed_slice() });
        }
        Ok(Some(PreparedProcessCommandArgv {
            payload: self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| process_problem("command_argv_original_payload"))?.to_vec().into_boxed_slice(),
            transports: transports.into_boxed_slice(), rows: rows.into_boxed_slice(), blocks: blocks.into_boxed_slice(), texts: texts.into_boxed_slice(), bytes: bytes.into_boxed_slice(),
        }))
    }
}

impl FullVerifier {
    pub(super) fn verify_process_command_argv_descriptor(store: &FullStore, instruction: u32, snapshot: &PreparedProcessCommandArgv) -> Result<(), IrVerifyError> {
        if store.tags.get(instruction as usize) != Some(&FullTag::ExprProcessCommandArgv)
            || store.payload(store.data[instruction as usize].range())? != snapshot.payload.as_ref() {
            return Err(IrVerifyError::new("command argv factory changes its original operand or policy slots"));
        }
        let generic = store.generic.as_deref().ok_or_else(|| IrVerifyError::new("command argv operand lineage has no original receipts"))?;
        for &(read, material) in &snapshot.transports {
            let saved = generic.original_argument_binding(read).ok_or_else(|| IrVerifyError::new("command argv saved operand lacks its original binding receipt"))?;
            if saved.initializer_source_instruction != material {
                return Err(IrVerifyError::new("command argv saved operand changes its original material source"));
            }
            let (_, _, body) = super::argument_prepare::saved_argument_wrapper(store, saved.wrapper)?;
            Self::verify_compiler_argument_wrapper(store, saved.wrapper, saved.initializer, saved.pattern, body, saved.slot)?;
            Self::verify_argument_initializer_lineage(store, generic, saved.initializer, material, &saved.initializer_wrappers, saved.owner)?;
        }
        for row in &snapshot.rows {
            if store.tags.get(row.instruction as usize) != Some(&row.tag)
                || store.payload(store.data.get(row.instruction as usize).ok_or_else(|| IrVerifyError::new("command argv original operand is missing"))?.range())? != row.payload.as_ref() {
                return Err(IrVerifyError::new("command argv factory changes its original argv or acceptance operand"));
            }
        }
        for (id, words) in &snapshot.blocks {
            let block = store.blocks.get(id.index()).ok_or_else(|| IrVerifyError::new("command argv original list block is missing"))?;
            if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST || store.payload(block.instructions)? != words.as_ref() {
                return Err(IrVerifyError::new("command argv factory changes its original argv or acceptance list"));
            }
        }
        for (id, value) in &snapshot.texts { if store.string(*id)? != value.as_ref() { return Err(IrVerifyError::new("command argv factory changes its original text")); } }
        for (id, value) in &snapshot.bytes { if store.bytes(*id)? != value.as_ref() { return Err(IrVerifyError::new("command argv factory changes its original path")); } }
        let (_, arguments, _) = encoded_process_command_argv_arguments(store, instruction, 15)?;
        let id = generic.ground_native_call_at(instruction)?.ok_or_else(|| IrVerifyError::new("command argv lacks its original factory proof"))?;
        let proof = generic.ground_native_call(id)?;
        let source = generic.native_call_source(proof.source)?;
        let argument = proof.contract.arguments.iter().zip(&proof.contract.binding.supplied_slots)
            .find_map(|(argument, slot)| (*slot == 1).then_some(argument)).ok_or_else(|| IrVerifyError::new("command argv lacks its original argv type"))?;
        let TypeRef::Ground(ty) = argument.ty else { return Err(IrVerifyError::new("command argv requires a closed argv producer")); };
        if !generic.verify_process_argv_container(&store.semantic, source, &proof.contract, ty)? {
            return Err(IrVerifyError::new("command argv loses its original finite Str or Path children"));
        }
        let argv = arguments[1].ok_or_else(|| IrVerifyError::new("command argv original argv is missing"))?;
        let material = snapshot.transports.iter().find_map(|&(read, material)| (read == argv).then_some(material)).unwrap_or(argv);
        Self::verify_generic_source(store, generic, material, source.owner, &store.semantic.to_type(ty)?, None, &mut vec![instruction])?;
        if let Some(timeout) = arguments[9] {
            let material = snapshot.transports.iter().find_map(|&(read, material)| (read == timeout).then_some(material)).unwrap_or(timeout);
            Self::verify_generic_source(store, generic, material, source.owner, &Type::Duration, None, &mut vec![instruction])?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "process_prepare/tests.rs"]
mod tests;

impl FullProgram {
    pub(in crate::runtime::eval) fn verify_process_command_argv_execution(&self, instruction: u32) -> Result<(), IrVerifyError> {
        let generic = self.generic_evidence().ok_or_else(|| IrVerifyError::new("command argv execution lacks its original registry proof"))?;
        let proof = generic.ground_native_call_at(instruction)?.ok_or_else(|| IrVerifyError::new("command argv execution lacks its original factory proof"))?;
        let proof = generic.ground_native_call(proof)?;
        let source = generic.native_call_source(proof.source)?;
        if proof.contract.process_command_argv.is_none()
            || FullVerifier::native_call_result(&self.store, generic, instruction, source.owner)? != Type::Command {
            return Err(IrVerifyError::new("command argv execution changes its original factory authority"));
        }
        Ok(())
    }
}
