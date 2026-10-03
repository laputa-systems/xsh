use super::*;
use super::super::super::generic::{PreparedRunProducer, RunProducerOperand, RunProducerArgument, PreparedRunPacket, PreparedRunEnvironment, PreparedRunStdin};

impl FullBuilder {
    pub(super) fn prepare_original_run_packet(&mut self, original: Option<&BuildRunPacketSource>, environment: &[crate::runtime::eval::LoweredRunEnv], redirects: &[crate::runtime::eval::LoweredRunRedirection], owner: InstructionOwner, caller: Option<crate::sema::check::DeclarationIdentity>, scratch: &BuildScratch, solved: &crate::sema::check::SolvedTypes, operands: &mut Vec<RunProducerOperand>, blocks: &mut Vec<(IrBlockId, Box<[u32]>)>, texts: &mut Vec<(u32, Arc<str>)>) -> Result<Option<PreparedRunPacket>, IrBuildError> {
        use crate::runtime::eval::LoweredRunArgKind;
        let Some(original) = original else {
            if !environment.is_empty() || !redirects.is_empty() { return Err(context_problem("run_packet_original_source_missing")); }
            return Ok(None);
        };
        if original.environment.len() != environment.len() || original.stdin.len() != redirects.len() { return Err(context_problem("run_packet_original_directive_count_changed")); }
        let mut prepared = PreparedRunPacket { environment: Vec::new(), stdin: Vec::new() };
        for (source, actual) in original.environment.iter().zip(environment) {
            let LoweredRunArgKind::Single(row) = actual.value.kind else { return Err(context_problem("run_packet_environment_mode_changed")); };
            let text = match &scratch.expressions[row.index()] {
                BuildExprRow::Str(text) => text.to_string(),
                BuildExprRow::PathFmtString { parts, .. } => parts.iter().map(|part| match part { crate::runtime::eval::LoweredFmtPart::Text(text) => Ok(text.as_ref()), _ => Err(context_problem("run_packet_environment_literal_changed")) }).collect::<Result<String, _>>()?,
                _ => return Err(context_problem("run_packet_environment_literal_changed")),
            };
            if actual.name != source.name || actual.value.span != source.argument_span || text != source.text.as_ref() { return Err(context_problem("run_packet_environment_source_changed")); }
            let (literal_operands, literal_blocks, literal_texts) = self.prepare_literal_run_operands(&actual.value, &[], scratch)?;
            let instruction = *self.active_encoded_expressions.get(&row).ok_or_else(|| context_problem("run_packet_environment_not_encoded"))?;
            operands.extend(literal_operands); blocks.extend(literal_blocks); texts.extend(literal_texts);
            prepared.environment.push(PreparedRunEnvironment { name: source.name, instruction, span: source.span, argument_span: source.argument_span, text: source.text.clone() });
        }
        for (source, actual) in original.stdin.iter().zip(redirects) {
            let (row, mode) = match actual.target.kind { LoweredRunArgKind::Single(row) => (row, 0), LoweredRunArgKind::SingleOrSplice(row) => (row, 1), _ => return Err(context_problem("run_packet_stdin_mode_changed")) };
            if source.kind != crate::syntax::node::RedirectionKind::StdinRead || actual.kind != source.kind || actual.span != source.span || actual.target.span != source.argument_span || mode != source.mode
                || self.active_expression_origins.get(&row) != Some(&source.origin) || solved.expressions.get(&source.origin) != Some(&source.source_type.ty)
                || solved.expression_owners.get(&source.origin).copied() != caller || solved.expression_scope(source.origin, caller).ok() != Some(source.source_type.scope) {
                return Err(context_problem("run_packet_stdin_original_relationship_changed"));
            }
            let original_type = super::super::super::generic::graph_ground_type(&solved.graph, source.source_type.ty).map_err(|_| context_problem("run_packet_stdin_type_not_ground"))?;
            if original_type != Type::Path { return Err(context_problem("run_packet_stdin_domain_not_prepared")); }
            let instruction = *self.active_encoded_expressions.get(&row).ok_or_else(|| context_problem("run_packet_stdin_not_encoded"))?;
            let root = ContextProducerRoot { origin: super::super::super::generic::OperationSourceOrigin::Expression(source.origin), instruction, ty: self.context_ground_root(solved, source.source_type)?, original_type };
            self.generic_evidence_mut().register_instruction_origin(instruction, root.origin, owner).map_err(|_| context_problem("run_packet_stdin_source_registration"))?;
            operands.push(RunProducerOperand { instruction, tag: self.store.tags[instruction as usize], payload: self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| context_problem("run_packet_stdin_payload"))?.to_vec().into_boxed_slice() });
            prepared.stdin.push(PreparedRunStdin { kind: source.kind, span: source.span, argument_span: source.argument_span, mode, source: source.source_type, root });
        }
        Ok(Some(prepared))
    }

    pub(super) fn prepare_checked_run_operands(&mut self, target: &crate::runtime::eval::LoweredRunArg, args: &[crate::runtime::eval::LoweredRunArg], guards: &[crate::sema::check::RunArgumentGuard], owner: InstructionOwner, caller: Option<crate::sema::check::DeclarationIdentity>, scratch: &BuildScratch, solved: &crate::sema::check::SolvedTypes) -> Result<(Vec<RunProducerOperand>, Vec<(IrBlockId, Box<[u32]>)>, Vec<(u32, Arc<str>)>, Vec<RunProducerArgument>), IrBuildError> {
        use crate::runtime::eval::LoweredRunArgKind;
        use crate::sema::check::{RunArgumentMode, RunArgumentSource};
        use crate::sema::inference::{Eligibility, RequirementTemplate, ScopedRequirementRoot};
        let mut operands = Vec::new(); let mut blocks = Vec::new(); let mut texts = Vec::new(); let mut arguments = Vec::new();
        for (word, argument) in std::iter::once(target).chain(args.iter()).enumerate() {
            let (row, mode, predicate) = match argument.kind {
                LoweredRunArgKind::Single(row) => (row, RunArgumentMode::Single, Eligibility::ArgvItem),
                LoweredRunArgKind::SingleOrSplice(row) => (row, RunArgumentMode::Expansion, Eligibility::ArgvExpansion),
                LoweredRunArgKind::Splice(row) => (row, RunArgumentMode::Splice, Eligibility::ArgvItem),
            };
            let original = self.active_expression_origins.get(&row).copied();
            let guard = original.and_then(|origin| guards.iter().find(|guard| guard.source == RunArgumentSource::Expression(origin)));
            let Some(guard) = guard else {
                let (literal_operands, literal_blocks, literal_texts) = self.prepare_literal_run_operands(argument, &[], scratch)?;
                operands.extend(literal_operands); blocks.extend(literal_blocks); texts.extend(literal_texts);
                continue;
            };
            let origin = original.ok_or_else(|| context_problem("run_argument_original_source_missing"))?;
            let ty = *solved.expressions.get(&origin).ok_or_else(|| context_problem("run_argument_original_type_missing"))?;
            let scope = solved.expression_scope(origin, caller).map_err(|_| context_problem("run_argument_original_scope"))?;
            if guard.mode != mode || solved.expression_owners.get(&origin).copied() != caller || solved.graph.resolved(ty).ok() != solved.graph.resolved(guard.actual).ok()
                || solved.graph.requirement_template(guard.requirement).ok() != Some(RequirementTemplate::Eligibility { predicate, ty: guard.operand })
                || !solved.graph.eligibility_satisfied(guard.requirement).map_err(|_| context_problem("run_argument_original_requirement"))? {
                return Err(context_problem("run_argument_original_guard_changed"));
            }
            solved.graph.validate_requirement_scoped(ScopedRequirementRoot { requirement: guard.requirement, scope }).map_err(|_| context_problem("run_argument_original_requirement_scope"))?;
            let source = ScopedRoot { ty, scope };
            let original_type = super::super::super::generic::graph_ground_type(&solved.graph, ty).map_err(|_| context_problem("run_argument_original_type_not_ground"))?;
            let original_operand = super::super::super::generic::graph_ground_type(&solved.graph, guard.operand).map_err(|_| context_problem("run_argument_original_operand_not_ground"))?;
            let expected_operand = match (&original_type, mode) {
                (Type::Path | Type::Str, RunArgumentMode::Single | RunArgumentMode::Expansion) => original_type.clone(),
                (Type::List(item), RunArgumentMode::Splice) if matches!(item.as_ref(), Type::Path | Type::Str) => item.as_ref().clone(),
                (Type::List(item), RunArgumentMode::Expansion) if matches!(item.as_ref(), Type::Path | Type::Str) => original_type.clone(),
                _ => return Err(context_problem("run_argument_ground_domain_not_prepared")),
            };
            if original_operand != expected_operand { return Err(context_problem("run_argument_original_operand_changed")); }
            let instruction = *self.active_encoded_expressions.get(&row).ok_or_else(|| context_problem("run_argument_original_not_encoded"))?;
            let root = ContextProducerRoot { origin: super::super::super::generic::OperationSourceOrigin::Expression(origin), instruction, ty: self.context_ground_root(solved, source)?, original_type };
            let operand = self.context_ground_root(solved, ScopedRoot { ty: guard.operand, scope })?;
            self.generic_evidence_mut().register_instruction_origin(instruction, root.origin, owner).map_err(|_| context_problem("run_argument_original_registration"))?;
            operands.push(RunProducerOperand { instruction, tag: self.store.tags[instruction as usize], payload: self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| context_problem("run_argument_original_payload"))?.to_vec().into_boxed_slice() });
            arguments.push(RunProducerArgument { word: word as u32, mode, requirement: guard.requirement, source, operand, original_operand, root });
        }
        if arguments.len() != guards.len() { return Err(context_problem("run_argument_original_sequence_changed")); }
        if arguments.iter().zip(guards).any(|(argument, guard)| argument.root.origin != match guard.source { crate::sema::check::RunArgumentSource::Expression(origin) => super::super::super::generic::OperationSourceOrigin::Expression(origin), _ => return true }) {
            return Err(context_problem("run_argument_original_order_changed"));
        }
        Ok((operands, blocks, texts, arguments))
    }

    pub(super) fn prepare_literal_run_acceptance(&mut self, row: Option<BuildExprId>, owner: InstructionOwner, caller: Option<crate::sema::check::DeclarationIdentity>, scratch: &BuildScratch, solved: &crate::sema::check::SolvedTypes, operands: &mut Vec<RunProducerOperand>, blocks: &mut Vec<(IrBlockId, Box<[u32]>)>) -> Result<Option<ContextProducerRoot>, IrBuildError> {
        let Some(row) = row else { return Ok(None); };
        let BuildExprRow::List(items) = &scratch.expressions[row.index()] else { return Err(context_problem("run_acceptance_literal_list_required")); };
        if items.is_empty() || !items.iter().all(|item| matches!(scratch.expressions[item.index()], BuildExprRow::Int(value) if (0..=255).contains(&value))) {
            return Err(context_problem("run_acceptance_literal_exit_codes_required"));
        }
        let origin = *self.active_expression_origins.get(&row).ok_or_else(|| context_problem("run_acceptance_original_source_missing"))?;
        let ty = *solved.expressions.get(&origin).ok_or_else(|| context_problem("run_acceptance_original_type_missing"))?;
        if solved.expression_owners.get(&origin).copied() != caller { return Err(context_problem("run_acceptance_original_owner_changed")); }
        let scope = solved.expression_scope(origin, caller).map_err(|_| context_problem("run_acceptance_original_scope"))?;
        let original_type = super::super::super::generic::graph_ground_type(&solved.graph, ty).map_err(|_| context_problem("run_acceptance_original_type_not_ground"))?;
        if original_type != Type::List(Box::new(Type::Int)) { return Err(context_problem("run_acceptance_original_type_changed")); }
        let instruction = *self.active_encoded_expressions.get(&row).ok_or_else(|| context_problem("run_acceptance_original_not_encoded"))?;
        let root = ContextProducerRoot { origin: super::super::super::generic::OperationSourceOrigin::Expression(origin), instruction, ty: self.context_ground_root(solved, ScopedRoot { ty, scope })?, original_type };
        self.generic_evidence_mut().register_instruction_origin(instruction, root.origin, owner).map_err(|_| context_problem("run_acceptance_original_registration"))?;
        for source in std::iter::once(&row).chain(items.iter()) {
            let encoded = *self.active_encoded_expressions.get(source).ok_or_else(|| context_problem("run_acceptance_item_not_encoded"))?;
            let payload = self.store.payload(self.store.data[encoded as usize].range()).map_err(|_| context_problem("run_acceptance_item_payload"))?.to_vec().into_boxed_slice();
            if *source == row {
                let id = payload.first().copied().and_then(IrBlockId::from_raw).ok_or_else(|| context_problem("run_acceptance_list_block"))?;
                blocks.push((id, self.store.payload(self.store.blocks[id.index()].instructions).map_err(|_| context_problem("run_acceptance_list_payload"))?.to_vec().into_boxed_slice()));
            }
            operands.push(RunProducerOperand { instruction: encoded, tag: self.store.tags[encoded as usize], payload });
        }
        Ok(Some(root))
    }
}

impl FullVerifier {
    pub(super) fn validate_run_packet_encoding(store: &FullStore, value: &PreparedRunProducer) -> Result<(), IrVerifyError> {
        let offset = if value.spawn.is_some() { 4 } else { 5 };
        let packet_block = |offset| -> Result<&[u32], IrVerifyError> {
            let id = value.payload.get(offset).copied().and_then(IrBlockId::from_raw).ok_or_else(|| IrVerifyError::new("run packet directive block is missing"))?;
            let block = store.blocks.get(id.index()).ok_or_else(|| IrVerifyError::new("run packet directive block is missing"))?;
            store.payload(block.instructions)
        };
        let environment = packet_block(offset)?;
        let stdin = packet_block(offset + 1)?;
        let Some(packet) = &value.packet else {
            if environment != [0] || stdin != [0] { return Err(IrVerifyError::new("run packet directives have no original source receipt")); }
            return Ok(());
        };
        let original_location = |word: u32, span: Span| -> bool {
            let Some(id) = super::super::super::IrLocationId::from_raw(word) else { return false; };
            store.location_sources.get(id.index()) == Some(&span.source_id)
                && store.locations.get(id.index()).is_some_and(|location| location.start as usize == span.start() && location.len as usize == span.end() - span.start())
        };
        if environment.first().copied() != Some(packet.environment.len() as u32) || environment.len() != 1 + packet.environment.len() * 4
            || stdin.first().copied() != Some(packet.stdin.len() as u32) || stdin.len() != 1 + packet.stdin.len() * 5 {
            return Err(IrVerifyError::new("run packet changes its original directive sequence"));
        }
        for (words, source) in environment[1..].chunks_exact(4).zip(&packet.environment) {
            if words[0] != source.name.symbol().raw() || words[1] != 0 || words[2] != source.instruction || !original_location(words[3], source.argument_span) {
                return Err(IrVerifyError::new("run environment changes its original name, value or location"));
            }
            let payload = store.payload(store.data[source.instruction as usize].range())?;
            let text = match store.tags[source.instruction as usize] {
                FullTag::ExprStr => store.string(payload[0])?.to_owned(),
                FullTag::ExprPathFmtString => {
                    let id = payload.first().copied().and_then(IrBlockId::from_raw).ok_or_else(|| IrVerifyError::new("run environment text block is missing"))?;
                    let parts = store.payload(store.blocks[id.index()].instructions)?;
                    if parts.first().copied().is_none_or(|count| parts.len() != 1 + count as usize * 2) { return Err(IrVerifyError::new("run environment text shape changed")); }
                    parts[1..].chunks_exact(2).map(|part| if part[0] == 0 { store.string(part[1]) } else { Err(IrVerifyError::new("run environment contains an unauthenticated expression")) }).collect::<Result<String, _>>()?
                }
                _ => return Err(IrVerifyError::new("run environment changes its original literal value")),
            };
            if text != source.text.as_ref() { return Err(IrVerifyError::new("run environment changes its original literal bytes")); }
        }
        for (words, source) in stdin[1..].chunks_exact(5).zip(&packet.stdin) {
            if store.redirection_kinds.get(words[0] as usize) != Some(&source.kind) || words[1] != source.mode || words[2] != source.root.instruction
                || !original_location(words[3], source.argument_span) || !original_location(words[4], source.span) {
                return Err(IrVerifyError::new("run stdin changes its original directive, Path operand or location"));
            }
        }
        Ok(())
    }

    pub(super) fn validate_run_argument_encoding(store: &FullStore, value: &PreparedRunProducer) -> Result<(), IrVerifyError> {
        use crate::sema::check::RunArgumentMode;
        let offset = if value.spawn.is_some() { 0 } else { 1 };
        let id = value.payload.get(offset + 3).copied().and_then(IrBlockId::from_raw).ok_or_else(|| IrVerifyError::new("run argument sequence has no original block"))?;
        let args = store.blocks.get(id.index()).ok_or_else(|| IrVerifyError::new("run argument sequence block is missing"))?;
        let words = store.payload(args.instructions)?;
        for argument in &value.arguments {
            let encoded = if argument.word == 0 { &value.payload[offset..offset + 2] } else {
                let start = 1 + (argument.word as usize - 1) * 3;
                words.get(start..start + 2).ok_or_else(|| IrVerifyError::new("run argument leaves its original command word"))?
            };
            let mode = match argument.mode { RunArgumentMode::Single => 0, RunArgumentMode::Expansion => 1, RunArgumentMode::Splice => 2, _ => return Err(IrVerifyError::new("run argument has an unprepared rendering mode")) };
            if encoded != [mode, argument.root.instruction] { return Err(IrVerifyError::new("run argument changes its original word, mode or expression")); }
        }
        Ok(())
    }

    pub(super) fn validate_run_acceptance_encoding(store: &FullStore, value: &PreparedRunProducer) -> Result<(), IrVerifyError> {
        if let Some(root) = &value.accept {
            let offset = if value.spawn.is_some() { 9 } else { 10 };
            if value.payload.get(offset) != Some(&root.instruction) || store.semantic.to_type(root.ty)? != root.original_type
                || root.original_type != Type::List(Box::new(Type::Int)) || store.tags.get(root.instruction as usize) != Some(&FullTag::ExprList) {
                return Err(IrVerifyError::new("run producer changes its original acceptance operand"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "run_prepare/tests.rs"]
mod tests;

impl FullBuilder {
    pub(super) fn prepare_literal_run_operands(&self, target: &crate::runtime::eval::LoweredRunArg, args: &[crate::runtime::eval::LoweredRunArg], scratch: &BuildScratch) -> Result<(Vec<RunProducerOperand>, Vec<(IrBlockId, Box<[u32]>)>, Vec<(u32, Arc<str>)>), IrBuildError> {
        let mut operands = Vec::new(); let mut texts = Vec::new(); let mut blocks = Vec::new();
        for argument in std::iter::once(target).chain(args.iter()) {
            let crate::runtime::eval::LoweredRunArgKind::Single(row) = argument.kind else { return Err(context_problem("run_literal_argument_mode")); };
            let encoded = *self.active_encoded_expressions.get(&row).ok_or_else(|| context_problem("run_literal_argument_not_encoded"))?;
            let words = self.store.payload(self.store.data[encoded as usize].range()).map_err(|_| context_problem("run_literal_argument_payload"))?.to_vec().into_boxed_slice();
            match &scratch.expressions[row.index()] {
                BuildExprRow::PathFmtString { parts, .. } if parts.iter().all(|part| matches!(part, crate::runtime::eval::LoweredFmtPart::Text(_))) => {
                    let id = words.first().copied().and_then(IrBlockId::from_raw).ok_or_else(|| context_problem("run_literal_parts_block"))?;
                    let part_words = self.store.payload(self.store.blocks[id.index()].instructions).map_err(|_| context_problem("run_literal_parts_payload"))?.to_vec().into_boxed_slice();
                    for pair in part_words[1..].chunks_exact(2) {
                        if pair[0] != 0 { return Err(context_problem("run_literal_parts_changed")); }
                        texts.push((pair[1], Arc::from(self.store.string(pair[1]).map_err(|_| context_problem("run_literal_text"))?)));
                    }
                    blocks.push((id, part_words));
                }
                BuildExprRow::Str(text) => { texts.push((*words.first().ok_or_else(|| context_problem("run_literal_text"))?, text.clone())); }
                _ => return Err(context_problem("run_literal_argument_source")),
            }
            operands.push(RunProducerOperand { instruction: encoded, tag: self.store.tags[encoded as usize], payload: words });
        }
        Ok((operands, blocks, texts))
    }

    pub(super) fn stage_original_spawn_run(&mut self, original: &BuildRunProducerOrigin, expression: BuildExprId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        use crate::sema::check::SpawnTarget;
        use crate::sema::operation_graph::PreparedLanguageOperation;
        use super::super::super::generic::{PreparedOperationAuthority, PreparedSpawnRunSource};
        let Some(source) = &original.spawn else { return Ok(()); };
        let solved = self.solved.clone().ok_or_else(|| context_problem("spawn_original_graph_missing"))?;
        let spawn = solved.spawn_operations.get(&source.origin).ok_or_else(|| context_problem("spawn_original_source_missing"))?;
        let operation = solved.operations.get(&source.origin).ok_or_else(|| context_problem("spawn_original_operation_missing"))?;
        let SpawnTarget::Run(target) = spawn.target else { return Err(context_problem("spawn_original_target_changed")); };
        if spawn.arguments.iter().any(|argument| !matches!(argument.source, crate::sema::check::RunArgumentSource::Expression(_)) || !matches!(argument.mode, crate::sema::check::RunArgumentMode::Single | crate::sema::check::RunArgumentMode::Expansion | crate::sema::check::RunArgumentMode::Splice)) { return Ok(()); }
        if target != source.target || original.run != target.run || target.source != source.origin.source || target.namespace != source.origin.namespace
            || original.source != ProducerFlowSource::Expression(source.origin) || original.capture != expression || original.continuation != expression
            || self.active_expression_origins.get(&expression) != Some(&source.origin) || solved.expressions.get(&source.origin) != Some(&source.source_type.ty)
            || operation.result != source.source_type.ty || operation.requirement != spawn.requirement
            || solved.expression_owners.get(&source.origin).copied() != operation.caller || solved.expression_scope(source.origin, operation.caller).ok() != Some(source.source_type.scope) {
            return Err(context_problem("spawn_original_relationship_changed"));
        }
        let BuildExprRow::SpawnRun(row) = &scratch.expressions[expression.index()] else { return Err(context_problem("spawn_original_row_changed")); };
        if row.timeout.is_some() || row.cpu_max.is_some() || (original.packet.is_none() && (!row.env.is_empty() || !row.redirections.is_empty())) { return Ok(()); }
        if let Some(accept) = row.accept {
            let BuildExprRow::List(items) = &scratch.expressions[accept.index()] else { return Ok(()); };
            if items.is_empty() || !items.iter().all(|item| matches!(scratch.expressions[item.index()], BuildExprRow::Int(value) if (0..=255).contains(&value))) { return Ok(()); }
        }
        if operation.receiver.is_some() || !operation.actual_arguments.is_empty() || !operation.binding.supplied_slots.is_empty() || !operation.binding.default_slots.is_empty()
            || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some() || !operation.argument_coercions.is_empty() {
            return Err(context_problem("spawn_original_binding_changed"));
        }
        solved.graph.validate_scoped(source.source_type).map_err(|_| context_problem("spawn_original_result_scope"))?;
        solved.graph.validate_requirement_scoped(crate::sema::inference::ScopedRequirementRoot { requirement: operation.requirement, scope: source.source_type.scope }).map_err(|_| context_problem("spawn_original_requirement_scope"))?;
        let selected = solved.graph.candidate_evidence(spawn.requirement).map_err(|_| context_problem("spawn_original_selection"))?.ok_or_else(|| context_problem("spawn_original_selection_missing"))?;
        let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate).map_err(|_| context_problem("spawn_original_authority"))? else { return Err(context_problem("spawn_original_authority_kind")); };
        if metadata.operation != (PreparedLanguageOperation::Spawn { command: false }) || solved.graph.resolved(selected.result).ok() != solved.graph.resolved(operation.result).ok() {
            return Err(context_problem("spawn_original_authority_changed"));
        }
        let crate::sema::inference::EffectSummary::Closed(effects) = operation.effects else { return Err(context_problem("spawn_original_effects_not_closed")); };
        if effects != crate::sema::inference::EffectSet::PROCESS { return Err(context_problem("spawn_original_effects_changed")); }
        let original_carrier = super::super::super::generic::graph_ground_type(&solved.graph, source.source_type.ty).map_err(|_| context_problem("spawn_original_result_not_ground"))?;
        if original_carrier != Type::Result(Box::new(Type::ProcessHandle), Box::new(Type::ProcessError)) { return Err(context_problem("spawn_original_result_changed")); }
        let carrier = self.context_ground_root(&solved, source.source_type)?;
        let raw = self.current_owner.ok_or_else(|| context_problem("spawn_original_owner_missing"))?;
        let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| context_problem("spawn_original_owner_invalid"))?) };
        if let Some(caller) = operation.caller {
            if self.declaration_functions.get(&caller).copied().map(InstructionOwner::Function) != Some(owner) { return Err(context_problem("spawn_original_declaration_owner_changed")); }
        } else if !matches!(owner, InstructionOwner::Driver(_)) { return Err(context_problem("spawn_original_declaration_owner_missing")); }
        let payload = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| context_problem("spawn_original_payload"))?.to_vec().into_boxed_slice();
        let (mut operands, mut blocks, mut texts, arguments) = self.prepare_checked_run_operands(row.target.as_ref(), &row.args, &spawn.arguments, owner, operation.caller, scratch, &solved)?;
        for offset in [3, 4, 5] {
            let id = payload.get(offset).copied().and_then(IrBlockId::from_raw).ok_or_else(|| context_problem("spawn_original_argument_block"))?;
            blocks.push((id, self.store.payload(self.store.blocks[id.index()].instructions).map_err(|_| context_problem("spawn_original_argument_payload"))?.to_vec().into_boxed_slice()));
        }
        let accept = self.prepare_literal_run_acceptance(row.accept, owner, operation.caller, scratch, &solved, &mut operands, &mut blocks)?;
        let packet = self.prepare_original_run_packet(original.packet.as_ref(), &row.env, &row.redirections, owner, operation.caller, scratch, &solved, &mut operands, &mut blocks, &mut texts)?;
        let authority = PreparedOperationAuthority::Language { identity: metadata.identity, authority: metadata.authority, operation: metadata.operation, argument_order: metadata.argument_order, statement_result_is_unit: metadata.statement_result_is_unit };
        let receipt = PreparedRunProducer { source: original.source, run: target.run, capture: instruction, continuation: instruction, owner, result: carrier, carrier,
            original_result: original_carrier.clone(), original_carrier, authority, effects, accept, spawn: Some(PreparedSpawnRunSource { origin: source.origin, source_type: source.source_type, target }),
            continuation_payload: payload.clone(), payload, operands, blocks, texts, arguments, packet };
        FullVerifier::validate_spawn_run_encoding(&self.store, &receipt).map_err(|error| IrBuildError::verification("spawn_original_encoding_changed", error))?;
        self.generic_evidence_mut().register_instruction_origin(instruction, super::super::super::generic::OperationSourceOrigin::Expression(source.origin), owner).map_err(|_| context_problem("spawn_original_parent_registration"))?;
        self.generic_evidence_mut().add_run_producer(receipt).map_err(|_| context_problem("spawn_original_receipt_capacity"))
    }
}

impl FullVerifier {
    pub(super) fn validate_spawn_run_encoding(store: &FullStore, value: &PreparedRunProducer) -> Result<(), IrVerifyError> {
        let Some(_) = &value.spawn else { return Err(IrVerifyError::new("spawn run has no original source relationship")); };
        if store.tags.get(value.capture as usize) != Some(&FullTag::ExprSpawnRun) || value.capture != value.continuation
            || store.payload(store.data[value.capture as usize].range())? != value.payload.as_ref() || value.continuation_payload != value.payload {
            return Err(IrVerifyError::new("spawn run changes its original execution row"));
        }
        let policy = value.accept.is_some();
        if value.payload.len() != (if policy { 11 } else { 10 }) || value.payload[0] != 0 || value.payload[6..8] != [0, 0] || value.payload[8] != u32::from(policy) {
            return Err(IrVerifyError::new("spawn run changes its original command or acceptance protocol"));
        }
        Self::validate_run_acceptance_encoding(store, value)?;
        Self::validate_run_argument_encoding(store, value)?;
        Self::validate_run_packet_encoding(store, value)?;
        for (id, original) in &value.blocks {
            let block = store.blocks.get(id.index()).ok_or_else(|| IrVerifyError::new("spawn run argument block is missing"))?;
            if block.flags != BLOCK_LIST || store.payload(block.instructions)? != original.as_ref() { return Err(IrVerifyError::new("spawn run changes its original argument sequence")); }
        }
        for operand in &value.operands {
            if store.tags.get(operand.instruction as usize) != Some(&operand.tag) || store.payload(store.data[operand.instruction as usize].range())? != operand.payload.as_ref() {
                return Err(IrVerifyError::new("spawn run changes its original literal operand"));
            }
        }
        for (id, text) in &value.texts { if store.string(*id)? != text.as_ref() { return Err(IrVerifyError::new("spawn run changes its original literal text")); } }
        Ok(())
    }
}

#[cfg(test)]
#[path = "run_prepare/spawn_tests.rs"]
mod spawn_tests;

#[cfg(test)]
#[path = "run_prepare/dynamic_tests.rs"]
mod dynamic_tests;

#[cfg(test)]
#[path = "run_prepare/packet_tests.rs"]
mod packet_tests;
