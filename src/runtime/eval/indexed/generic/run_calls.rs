use super::*;

impl GenericEvidenceStore {
    pub(super) fn verify_run_argument_roots(&self, value: &PreparedRunProducer, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        use crate::sema::check::RunArgumentMode;
        use crate::sema::types::Type;
        let (source, namespace) = match value.source {
            ProducerFlowSource::Expression(origin) => (origin.source, origin.namespace),
            ProducerFlowSource::Statement(origin) => (origin.source, origin.namespace),
            _ => return Err(failure("run argument has no original command parent")),
        };
        let mut prior = None;
        for argument in &value.arguments {
            let OperationSourceOrigin::Expression(origin) = argument.root.origin else { return Err(failure("run argument has no original expression root")); };
            if prior.is_some_and(|word| argument.word <= word) || (origin.source, origin.namespace) != (source, namespace)
                || owners.get(argument.root.instruction as usize) != Some(&Some(value.owner))
                || self.registered_instruction_origin(argument.root.instruction, false) != Some((argument.root.origin, value.owner))
                || pools.to_type(argument.root.ty)? != argument.root.original_type || pools.to_type(argument.operand)? != argument.original_operand
                || !value.operands.iter().any(|operand| operand.instruction == argument.root.instruction) {
                return Err(failure("run argument changes its original source, sequence, owner or checked type"));
            }
            let expected = match (&argument.root.original_type, argument.mode) {
                (Type::Path | Type::Str, RunArgumentMode::Single | RunArgumentMode::Expansion) => &argument.root.original_type,
                (Type::List(item), RunArgumentMode::Splice) if matches!(item.as_ref(), Type::Path | Type::Str) => item.as_ref(),
                (Type::List(item), RunArgumentMode::Expansion) if matches!(item.as_ref(), Type::Path | Type::Str) => &argument.root.original_type,
                _ => return Err(failure("run argument changes its checked rendering domain")),
            };
            if &argument.original_operand != expected { return Err(failure("run argument changes its checked rendering operand")); }
            prior = Some(argument.word);
        }
        Ok(())
    }

    pub(super) fn verify_run_acceptance_root(&self, value: &PreparedRunProducer, policy: bool, propagate: bool, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        use crate::sema::inference::EffectSet;
        let expected_effects = EffectSet(EffectSet::PROCESS.0 | if propagate { EffectSet::ERROR.0 } else { 0 });
        if value.effects != expected_effects || value.accept.is_some() != policy { return Err(failure("run producer changes its original effects or acceptance policy")); }
        let Some(root) = &value.accept else { return Ok(()); };
        let OperationSourceOrigin::Expression(origin) = root.origin else { return Err(failure("run acceptance policy has no original expression")); };
        let (source, namespace) = match value.source {
            ProducerFlowSource::Expression(parent) => (parent.source, parent.namespace),
            ProducerFlowSource::Statement(parent) => (parent.source, parent.namespace),
            _ => return Err(failure("run acceptance policy has no original parent")),
        };
        if origin.source != source || origin.namespace != namespace || owners.get(root.instruction as usize) != Some(&Some(value.owner))
            || self.registered_instruction_origin(root.instruction, false) != Some((root.origin, value.owner))
            || pools.to_type(root.ty)? != root.original_type || root.original_type != crate::sema::types::Type::List(Box::new(crate::sema::types::Type::Int)) {
            return Err(failure("run acceptance policy changes its original source, owner or checked type"));
        }
        Ok(())
    }
}

impl GenericEvidenceStore {
    pub(super) fn verify_spawn_run_evidence(&self, value: &PreparedRunProducer, pools: &SemanticPools) -> Result<(), IrVerifyError> {
        let Some(spawn) = &value.spawn else { return Err(failure("spawn run has no original source receipt")); };
        let PreparedOperationAuthority::Language { operation: crate::sema::operation_graph::PreparedLanguageOperation::Spawn { command: false }, .. } = value.authority else {
            return Err(failure("spawn run changes its original selected authority"));
        };
        let expected = crate::sema::types::Type::Result(Box::new(crate::sema::types::Type::ProcessHandle), Box::new(crate::sema::types::Type::ProcessError));
        if value.source != ProducerFlowSource::Expression(spawn.origin) || value.run != spawn.target.run || spawn.target.source != spawn.origin.source || spawn.target.namespace != spawn.origin.namespace
            || value.capture != value.continuation || value.result != value.carrier || value.original_result != expected || value.original_carrier != expected
            || pools.to_type(value.result)? != expected || pools.to_type(value.carrier)? != expected || value.effects != crate::sema::inference::EffectSet::PROCESS {
            return Err(failure("spawn run changes its original target, result or process effects"));
        }
        if let Some(root) = &value.accept {
            let OperationSourceOrigin::Expression(origin) = root.origin else { return Err(failure("spawn acceptance policy has no original expression")); };
            if origin.source != spawn.origin.source || origin.namespace != spawn.origin.namespace
                || self.registered_instruction_origin(root.instruction, false) != Some((root.origin, value.owner))
                || pools.to_type(root.ty)? != root.original_type || root.original_type != crate::sema::types::Type::List(Box::new(crate::sema::types::Type::Int)) {
                return Err(failure("spawn acceptance policy changes its original source or checked type"));
            }
        }
        Ok(())
    }
}
