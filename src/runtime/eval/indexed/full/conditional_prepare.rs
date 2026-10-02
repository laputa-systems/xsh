use super::*;
use super::super::generic::{ConditionalArm, ConditionalBody, ConditionalKind, ConditionalResultSource, ConditionalTerminalValue, ConditionalValue, ValueInitializerWrapper, ValueInitializerWrapperKind};
use super::super::super::{BuildPatternResultBody, BuildPatternResultSource, BuildPatternResultTerminalSource};
use crate::sema::check::{ExpressionIdentity, PatternIdentity};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum BuildConditionalBody {
    Authored(BuildPatternResultBody),
    Boolean { instruction: BuildExprId, value: bool },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct BuildConditionalArm {
    pub pattern: Option<(BuildPatternId, PatternIdentity)>,
    pub condition: Option<(BuildExprId, BuildPatternResultSource)>,
    pub guard: Option<(BuildExprId, BuildPatternResultSource)>,
    pub body: BuildConditionalBody,
}

/// Source relationships are captured while the original syntax and checked
/// roots are present. Encoding must represent these particular branches.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct BuildConditionalResult {
    pub instruction: BuildExprId,
    pub source: BuildPatternResultSource,
    pub kind: ConditionalKind,
    pub subject: Option<(BuildExprId, BuildPatternResultSource)>,
    pub arms: Box<[BuildConditionalArm]>,
    pub fallback: Option<BuildConditionalBody>,
}

fn problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }
fn invalid() -> IrVerifyError { IrVerifyError::new("conditional value disagrees with its original completing branch receipt") }

fn conditional_block<'a>(store: &'a FullStore, source: &ConditionalResultSource) -> Result<&'a FullBlock, IrVerifyError> {
    let index = if matches!(source.kind, ConditionalKind::Match | ConditionalKind::PatternTest) { 1 } else { 0 };
    let raw = *source.instruction_payload.get(index).ok_or_else(invalid)?;
    IrBlockId::from_raw(raw).and_then(|id| store.blocks.get(id.index())).ok_or_else(invalid)
}

fn verify_layout(store: &FullStore, source: &ConditionalResultSource, snapshot: bool) -> Result<(), IrVerifyError> {
    let tag = if matches!(source.kind, ConditionalKind::Match | ConditionalKind::PatternTest) { FullTag::ExprMatch } else { FullTag::ExprIf };
    if store.tags.get(source.instruction as usize) != Some(&tag) { return Err(invalid()); }
    let words = store.payload(store.data.get(source.instruction as usize).ok_or_else(invalid)?.range())?;
    if words != source.instruction_payload.as_ref() || words.len() != 3 { return Err(invalid()); }
    let block = conditional_block(store, source)?;
    let branches = store.payload(block.instructions)?;
    if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST
        || snapshot && (block.flags != source.block_flags || branches != source.block_payload.as_ref()) { return Err(invalid()); }
    let mut input = FullCursor::new(branches);
    if input.raw()? as usize != source.arms.len() { return Err(invalid()); }
    if matches!(source.kind, ConditionalKind::Match | ConditionalKind::PatternTest) {
        if source.subject.as_ref().map(|subject| subject.instruction) != words.first().copied() || source.fallback.is_some() { return Err(invalid()); }
        for arm in &source.arms {
            let (pattern, _) = arm.pattern.ok_or_else(invalid)?;
            let actual_pattern = input.raw()?;
            let guard = match input.raw()? { 0 => None, 1 => Some(input.raw()?), _ => return Err(invalid()) };
            let body = input.raw()?;
            if actual_pattern != pattern || guard != arm.guard.as_ref().map(|guard| guard.instruction)
                || body != arm.body.instruction() || arm.condition.is_some() { return Err(invalid()); }
        }
    } else {
        if source.subject.is_some() || source.fallback.as_ref().map(ConditionalBody::instruction) != words.get(1).copied() { return Err(invalid()); }
        for arm in &source.arms {
            if arm.pattern.is_some() || arm.guard.is_some()
                || input.raw()? != arm.condition.as_ref().ok_or_else(invalid)?.instruction
                || input.raw()? != arm.body.instruction() { return Err(invalid()); }
        }
    }
    input.finish()?;
    if source.kind == ConditionalKind::Not {
        if source.arms.len() != 1
            || !matches!(source.arms[0].body, ConditionalBody::Boolean { value: false, .. })
            || !matches!(source.fallback, Some(ConditionalBody::Boolean { value: true, .. })) { return Err(invalid()); }
    }
    Ok(())
}

fn verify_body_layout(store: &FullStore, body: &ConditionalBody) -> Result<(), IrVerifyError> {
    match body {
        ConditionalBody::Boolean { instruction, value } => {
            if store.tags.get(*instruction as usize) != Some(&FullTag::ExprBool)
                || store.payload(store.data.get(*instruction as usize).ok_or_else(invalid)?.range())? != [u32::from(*value)] { return Err(invalid()); }
        }
        ConditionalBody::Authored { value, terminal: Some((statement, _, tail)) } => {
            if store.tags.get(value.instruction as usize) != Some(&FullTag::ExprValueBlock) { return Err(invalid()); }
            let words = store.payload(store.data[value.instruction as usize].range())?;
            let block = words.first().copied().and_then(IrBlockId::from_raw).and_then(|id| store.blocks.get(id.index())).ok_or_else(invalid)?;
            let statements = store.payload(block.instructions)?;
            let tail = match tail { ConditionalTerminalValue::Expression(value) => value.instruction, ConditionalTerminalValue::PatternCapture { instruction, .. } => *instruction };
            if statements.first().copied().map(|count| count as usize) != Some(statements.len() - 1)
                || statements.last() != Some(statement) || store.tags.get(*statement as usize) != Some(&FullTag::StmtValue)
                || store.payload(store.data.get(*statement as usize).ok_or_else(invalid)?.range())? != [tail] { return Err(invalid()); }
        }
        ConditionalBody::Authored { terminal: None, .. } => {}
    }
    Ok(())
}

impl FullBuilder {
    fn conditional_value_lineage(&self, instruction: u32, owner: InstructionOwner) -> Result<(u32, Box<[ValueInitializerWrapper]>), IrBuildError> {
        // Body ranges commit after their instructions. Staged compiler receipts
        // authenticate this chain now; the finished owner range is checked later.
        let mut material = instruction;
        let mut wrappers = Vec::new();
        loop {
            let compiler = self.compiler_argument_wrapper(material, owner)?;
            let checked = self.store.tags.get(material as usize) == Some(&FullTag::ExprCheckedValue);
            if !checked && compiler.is_none() { break; }
            if wrappers.len() >= 256 { return Err(problem("conditional_value_wrapper_depth")); }
            let payload = self.store.payload(self.store.data.get(material as usize).ok_or_else(|| problem("conditional_value_wrapper_missing"))?.range())
                .map_err(|_| problem("conditional_value_wrapper_payload"))?.to_vec().into_boxed_slice();
            let (child, kind) = if checked {
                (*payload.first().ok_or_else(|| problem("conditional_checked_value_missing"))?, ValueInitializerWrapperKind::CheckedValue)
            } else {
                let original = compiler.ok_or_else(|| problem("conditional_compiler_wrapper_receipt_missing"))?;
                (original.body, ValueInitializerWrapperKind::CompilerArgument { initializer: original.initializer, pattern: original.pattern, body: original.body, slot: original.slot })
            };
            if child >= material { return Err(problem("conditional_value_wrapper_cycle")); }
            wrappers.push(ValueInitializerWrapper { instruction: material, payload, kind });
            material = child;
        }
        Ok((material, wrappers.into_boxed_slice()))
    }

    fn prepare_conditional_value(
        &mut self, instruction: BuildExprId, original: &BuildPatternResultSource,
        caller: Option<crate::sema::check::DeclarationIdentity>, scope: Option<SchemeScopeId>, owner: InstructionOwner,
    ) -> Result<ConditionalValue, IrBuildError> {
        let solved = self.solved.clone().ok_or_else(|| problem("conditional_checked_source_missing"))?;
        if original.caller != caller || solved.expression_owners.get(&original.origin).copied() != caller
            || solved.expressions.get(&original.origin) != Some(&original.ty)
            || solved.non_completing_expressions.contains(&original.origin)
            || solved.expression_scope(original.origin, caller).map_err(|_| problem("conditional_original_scope"))? != original.scope {
            return Err(problem("conditional_original_completing_value_changed"));
        }
        solved.graph.validate_scoped(crate::sema::inference::ScopedRoot { ty: original.ty, scope: original.scope })
            .map_err(|_| problem("conditional_original_root_scope"))?;
        let instruction = *self.active_encoded_expressions.get(&instruction).ok_or_else(|| problem("conditional_original_value_not_encoded"))?;
        let (material, wrappers) = self.conditional_value_lineage(instruction, owner)?;
        if !self.generic_expression_rows.iter().any(|row| *row == (material, original.origin, owner)) {
            return Err(problem("conditional_value_loses_original_expression"));
        }
        let expected = self.call_reference(&solved, scope, original.ty, false)?;
        let original_callable = solved.expression_callables.get(&original.origin).and_then(|callable| callable.declaration);
        Ok(ConditionalValue { instruction, material, origin: original.origin, original_callable, expected, wrappers })
    }

    fn prepare_conditional_body(
        &mut self, original: &BuildConditionalBody, caller: Option<crate::sema::check::DeclarationIdentity>,
        scope: Option<SchemeScopeId>, owner: InstructionOwner,
    ) -> Result<ConditionalBody, IrBuildError> {
        match original {
            BuildConditionalBody::Boolean { instruction, value } => Ok(ConditionalBody::Boolean {
                instruction: *self.active_encoded_expressions.get(instruction).ok_or_else(|| problem("conditional_boolean_decision_not_encoded"))?, value: *value,
            }),
            BuildConditionalBody::Authored(original) => {
                let value = self.prepare_conditional_value(original.instruction, &original.source, caller, scope, owner)?;
                let terminal = original.terminal.as_ref().map(|(statement, instruction, original, identity)| {
                    let statement = *self.active_pattern_statements.get(statement).ok_or_else(|| problem("conditional_original_tail_not_encoded"))?;
                    let tail = match original {
                        BuildPatternResultTerminalSource::Expression(original) => ConditionalTerminalValue::Expression(self.prepare_conditional_value(*instruction, original, caller, scope, owner)?),
                        BuildPatternResultTerminalSource::PatternCapture(capture) => {
                            let solved = self.solved.clone().ok_or_else(|| problem("conditional_capture_checked_source_missing"))?;
                            let pattern = solved.checked_pattern(capture.pattern).map_err(|_| problem("conditional_capture_original_pattern"))?;
                            let original = pattern.captures.iter().find(|original| original.identity == *capture).ok_or_else(|| problem("conditional_capture_original_binding"))?;
                            let instruction = *self.active_encoded_expressions.get(instruction).ok_or_else(|| problem("conditional_capture_original_read_not_encoded"))?;
                            if pattern.caller != caller || !self.generic_pattern_statement_use_rows.iter().any(|row| *row == (instruction, *identity, *capture, owner)) {
                                return Err(problem("conditional_capture_original_statement_changed"));
                            }
                            let original_scope = solved.checked_pattern_scope(capture.pattern).map_err(|_| problem("conditional_capture_original_scope"))?;
                            solved.graph.validate_scoped(crate::sema::inference::ScopedRoot { ty: original.ty, scope: original_scope })
                                .map_err(|_| problem("conditional_capture_original_root_scope"))?;
                            let expected = self.call_reference(&solved, scope, original.ty, false)?;
                            ConditionalTerminalValue::PatternCapture { instruction, identity: *capture, expected }
                        }
                    };
                    Ok::<_, IrBuildError>((statement, *identity, tail))
                }).transpose()?;
                Ok(ConditionalBody::Authored { value, terminal })
            }
        }
    }

    pub(super) fn stage_conditional_result(
        &mut self, instruction: u32, origin: ExpressionIdentity, owner: InstructionOwner, scratch: &BuildScratch,
    ) -> Result<(), IrBuildError> {
        let Some(original) = scratch.conditional_result_origins.get(&origin) else { return Ok(()); };
        if self.active_encoded_expressions.get(&original.instruction) != Some(&instruction) || original.source.origin != origin {
            return Err(problem("conditional_original_instruction_changed"));
        }
        let solved = self.solved.clone().ok_or_else(|| problem("conditional_checked_source_missing"))?;
        let caller = original.source.caller;
        let scope = caller.and_then(|caller| self.generic_declarations.get(&caller).copied());
        if solved.expression_owners.get(&origin).copied() != caller
            || solved.expressions.get(&origin) != Some(&original.source.ty)
            || solved.non_completing_expressions.contains(&origin)
            || solved.expression_scope(origin, caller).map_err(|_| problem("conditional_original_result_scope"))? != original.source.scope {
            return Err(problem("conditional_original_result_changed"));
        }
        solved.graph.validate_scoped(crate::sema::inference::ScopedRoot { ty: original.source.ty, scope: original.source.scope })
            .map_err(|_| problem("conditional_original_result_root_scope"))?;
        let expected = self.call_reference(&solved, scope, original.source.ty, false)?;
        let subject = original.subject.as_ref().map(|(instruction, source)| self.prepare_conditional_value(*instruction, source, caller, scope, owner)).transpose()?;
        let mut arms = Vec::new();
        for arm in &original.arms {
            let pattern = arm.pattern.map(|(row, identity)| {
                self.generic_pattern_rows.iter().find_map(|&(instruction, original, actual_owner)| {
                    (original == identity && actual_owner == owner && scratch.pattern_origins.get(&row) == Some(&identity)).then_some((instruction, identity))
                }).ok_or_else(|| problem("conditional_original_pattern_not_encoded"))
            }).transpose()?;
            let condition = arm.condition.as_ref().map(|(instruction, source)| self.prepare_conditional_value(*instruction, source, caller, scope, owner)).transpose()?;
            let guard = arm.guard.as_ref().map(|(instruction, source)| self.prepare_conditional_value(*instruction, source, caller, scope, owner)).transpose()?;
            let body = self.prepare_conditional_body(&arm.body, caller, scope, owner)?;
            arms.push(ConditionalArm { pattern, condition, guard, body });
        }
        let fallback = original.fallback.as_ref().map(|body| self.prepare_conditional_body(body, caller, scope, owner)).transpose()?;
        let instruction_payload = self.store.payload(self.store.data.get(instruction as usize).ok_or_else(|| problem("conditional_original_payload_missing"))?.range())
            .map_err(|_| problem("conditional_original_payload_invalid"))?.to_vec().into_boxed_slice();
        let mut source = ConditionalResultSource {
            instruction, origin, owner, scope, expected, kind: original.kind, subject, arms: arms.into_boxed_slice(), fallback,
            instruction_payload, block_flags: 0, block_payload: Box::new([]),
        };
        verify_layout(&self.store, &source, false).map_err(|_| problem("conditional_encoded_branches_changed"))?;
        let block = conditional_block(&self.store, &source).map_err(|_| problem("conditional_original_branches_missing"))?;
        source.block_flags = block.flags;
        source.block_payload = self.store.payload(block.instructions).map_err(|_| problem("conditional_original_branches_invalid"))?.to_vec().into_boxed_slice();
        self.conditional_result_rows.push(source);
        Ok(())
    }

    pub(super) fn prepare_conditional_results(&mut self) -> Result<(), IrBuildError> {
        for source in self.conditional_result_rows.clone() {
            for body in source.arms.iter().map(|arm| &arm.body).chain(source.fallback.iter()) {
                if let ConditionalBody::Authored { terminal: Some((statement, identity, _)), .. } = body {
                    self.generic_evidence_mut().register_instruction_origin(*statement, super::super::generic::OperationSourceOrigin::Statement(*identity), source.owner)
                        .map_err(|_| problem("conditional_original_tail_origin_changed"))?;
                }
            }
            self.generic_evidence_mut().add_conditional_source(source).map_err(|_| problem("conditional_original_receipt_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    fn conditional_type(store: &FullStore, generic: &GenericEvidenceStore, source: &ConditionalResultSource, reference: TypeRef, instance: Option<InstantiationId>) -> Result<Type, IrVerifyError> {
        if let TypeRef::Ground(ty) = reference { return store.semantic.to_type(ty); }
        let scope = source.scope.ok_or_else(invalid)?;
        let frame = generic.instance(instance.ok_or_else(|| IrVerifyError::new("conditional value requires its original declaration instance"))?)?;
        if frame.scope != scope { return Err(invalid()); }
        generic.expand(&store.semantic, reference, &frame.substitutions, &mut FxHashMap::default())
    }

    fn verify_conditional_value(store: &FullStore, generic: &GenericEvidenceStore, source: &ConditionalResultSource, value: &ConditionalValue, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
        Self::verify_argument_initializer_lineage(store, generic, value.instruction, value.material, &value.wrappers, source.owner)?;
        let expected = Self::conditional_type(store, generic, source, value.expected, instance)?;
        Self::verify_generic_source(store, generic, value.instruction, source.owner, &expected, instance, active)
    }

    fn verify_conditional_body(store: &FullStore, generic: &GenericEvidenceStore, source: &ConditionalResultSource, body: &ConditionalBody, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
        verify_body_layout(store, body)?;
        match body {
            ConditionalBody::Boolean { .. } => {}
            ConditionalBody::Authored { value, terminal } => {
                if let Some((_, _, tail)) = terminal {
                    match tail {
                        ConditionalTerminalValue::Expression(value) => Self::verify_conditional_value(store, generic, source, value, instance, active)?,
                        ConditionalTerminalValue::PatternCapture { instruction, expected, .. } => {
                            let expected = Self::conditional_type(store, generic, source, *expected, instance)?;
                            Self::verify_generic_source(store, generic, *instruction, source.owner, &expected, instance, active)?;
                        }
                    }
                } else { Self::verify_conditional_value(store, generic, source, value, instance, active)?; }
            }
        }
        Ok(())
    }

    pub(super) fn verify_conditionals(store: &FullStore, _tree: &super::super::pattern::PatternTree) -> Result<(), IrVerifyError> {
        let Some(generic) = store.generic.as_deref().filter(|generic| generic.has_conditionals()) else { return Ok(()); };
        for (id, _) in generic.conditional_sources() {
            let source = generic.conditional_source(id)?;
            verify_layout(store, source, true)?;
            for value in source.subject.iter().chain(source.arms.iter().flat_map(|arm| arm.condition.iter().chain(arm.guard.iter()))) {
                Self::verify_argument_initializer_lineage(store, generic, value.instruction, value.material, &value.wrappers, source.owner)?;
            }
            for body in source.arms.iter().map(|arm| &arm.body).chain(source.fallback.iter()) {
                verify_body_layout(store, body)?;
                if let ConditionalBody::Authored { value, terminal } = body {
                    Self::verify_argument_initializer_lineage(store, generic, value.instruction, value.material, &value.wrappers, source.owner)?;
                    if let Some((_, _, ConditionalTerminalValue::Expression(value))) = terminal {
                        Self::verify_argument_initializer_lineage(store, generic, value.instruction, value.material, &value.wrappers, source.owner)?;
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn verify_conditional_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        let Some(id) = generic.conditional_source_at(instruction)? else { return Ok(false); };
        let source = generic.conditional_source(id)?;
        if source.owner != owner || Self::conditional_type(store, generic, source, source.expected, instance)? != *expected { return Err(invalid()); }
        verify_layout(store, source, true)?;
        if let Some(subject) = &source.subject { Self::verify_conditional_value(store, generic, source, subject, instance, active)?; }
        for arm in &source.arms {
            for value in arm.condition.iter().chain(arm.guard.iter()) { Self::verify_conditional_value(store, generic, source, value, instance, active)?; }
            Self::verify_conditional_body(store, generic, source, &arm.body, instance, active)?;
        }
        if let Some(body) = &source.fallback { Self::verify_conditional_body(store, generic, source, body, instance, active)?; }
        Ok(true)
    }

    pub(super) fn verify_conditional_symbolic_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, scope: SchemeScopeId, expected: TypeRef, active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        let Some(id) = generic.conditional_source_at(instruction)? else { return Ok(false); };
        let source = generic.conditional_source(id)?;
        if source.scope != Some(scope) || source.owner != InstructionOwner::Function(generic.scope(scope)?.owner)
            || !generic.references_equal(&store.semantic, scope, source.expected, expected)? { return Err(invalid()); }
        verify_layout(store, source, true)?;
        for value in source.subject.iter().chain(source.arms.iter().flat_map(|arm| arm.condition.iter().chain(arm.guard.iter()))) {
            Self::verify_argument_initializer_lineage(store, generic, value.instruction, value.material, &value.wrappers, source.owner)?;
            Self::verify_generic_symbolic_source(store, generic, value.instruction, scope, value.expected, active)?;
        }
        for body in source.arms.iter().map(|arm| &arm.body).chain(source.fallback.iter()) {
            verify_body_layout(store, body)?;
            match body {
                ConditionalBody::Boolean { .. } => {}
                ConditionalBody::Authored { value, terminal: None } => {
                    Self::verify_argument_initializer_lineage(store, generic, value.instruction, value.material, &value.wrappers, source.owner)?;
                    Self::verify_generic_symbolic_source(store, generic, value.instruction, scope, value.expected, active)?;
                }
                ConditionalBody::Authored { terminal: Some((_, _, tail)), .. } => {
                    match tail {
                        ConditionalTerminalValue::Expression(value) => {
                            Self::verify_argument_initializer_lineage(store, generic, value.instruction, value.material, &value.wrappers, source.owner)?;
                            Self::verify_generic_symbolic_source(store, generic, value.instruction, scope, value.expected, active)?;
                        }
                        ConditionalTerminalValue::PatternCapture { instruction, expected, .. } => {
                            Self::verify_generic_symbolic_source(store, generic, *instruction, scope, *expected, active)?;
                        }
                    }
                }
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::Checker;

    fn prepared(source: &str) -> FullProgram {
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
            "conditional-results.xsh", crate::loader::entry_source_from_text("conditional-results.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = SourceMap::files(&sources).first().unwrap().id();
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let weak = Arc::downgrade(&checked.solved);
        let counters = checked.solved.graph.counters().clone();
        let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
        evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
        assert_eq!(checked.solved.graph.counters(), &counters);
        drop(checked); drop(parsed);
        assert!(weak.upgrade().is_none());
        evaluator.indexed_program.as_ref().unwrap().as_ref().clone()
    }

    fn execute(source: &str, expected: &[u8]) {
        for recursive in [false, true] {
            let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
                "conditional-runtime.xsh", crate::loader::entry_source_from_text("conditional-runtime.xsh", source.to_owned()), Vec::new());
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let source_id = SourceMap::files(&sources).first().unwrap().id();
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let weak = Arc::downgrade(&checked.solved);
            let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
            let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
            drop(checked); drop(parsed);
            assert!(weak.upgrade().is_none());
            let symbols = evaluator.indexed_program.as_ref().unwrap().symbol_owner().clone();
            let run = || symbols.with_current(|| {
                assert_eq!(crate::runtime::eval::lowered_run::recursive_fast_path_forced(), recursive);
                evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("prepared conditional program remains installed"))
            });
            let output = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(run) } else { run() };
            assert_eq!(output.status, 0, "{:?}; {:?}", output.diagnostics, output.traceback);
            assert_eq!(output.stdout, expected, "recursive={recursive}: {:?}", output.diagnostics);
            assert!(output.stderr.is_empty()); assert!(output.diagnostics.is_empty());
        }
    }

    #[test]
    fn completing_matches_keep_original_capture_roots_for_distinct_generic_instances() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure identity(value) { value }\npure selected(value) { identity(match value { original => original }) }\nprint ${selected(7)}\nprint ${selected(\"word\")}\n";
            let program = prepared(source);
            let _symbols = program.symbol_owner().enter();
            let generic = program.store.generic.as_ref().unwrap();
            let (_, result) = generic.conditional_sources().find(|(_, source)| source.kind == ConditionalKind::Match).unwrap();
            assert!(result.scope.is_some());
            assert!(matches!(result.expected, TypeRef::Rigid(_)));
            assert!(FullVerifier::verify(&program).is_ok());
            let types = generic.instances().filter(|(_, instance)| Some(instance.scope) == result.scope).map(|(_, instance)| program.store.semantic.to_type(instance.parameter_types[0]).unwrap()).collect::<Vec<_>>();
            assert!(types.contains(&Type::Int)); assert!(types.contains(&Type::Str));
            execute(source, b"7\nword\n");
        });
    }

    #[test]
    fn completing_match_results_refuse_missing_foreign_and_coupled_same_type_arm_receipts() {
        crate::runtime::eval::run_eval(|| {
            let program = prepared("pure identity(value) { value }\npure selected(value: Int) -> Int { identity(match value { 0 => 1, _ => value }) }\nprint ${selected(0)}\n");
            let _symbols = program.symbol_owner().enter();
            let generic = program.store.generic.as_ref().unwrap();
            let (id, result) = generic.conditional_sources().find(|(_, source)| source.kind == ConditionalKind::Match).unwrap();
            assert!(FullVerifier::verify(&program).is_ok());
            let mut removed = program.clone();
            removed.store.generic.as_mut().unwrap().test_clear_conditionals();
            assert!(FullVerifier::verify(&removed).is_err(), "typed match consumers cannot manufacture missing result authority from the encoded arms");
            let mut outsider = GenericEvidenceBuilder::default();
            let foreign = outsider.add_conditional_source(result.clone()).unwrap();
            assert!(generic.conditional_source(foreign).is_err(), "a same-shaped conditional receipt from another program is foreign");
            let block = conditional_block(&program.store, result).unwrap().instructions;
            let replacement = result.arms[1].body.instruction();
            let mut changed = program.clone();
            let range = block.bounds(changed.store.extra.len()).unwrap();
            assert_eq!(changed.store.extra[range.start + 3], result.arms[0].body.instruction());
            changed.store.extra[range.start + 3] = replacement;
            assert!(FullVerifier::verify(&changed).is_err(), "the same Int result cannot replace the original zero arm");
            let mut coupled = changed.clone();
            let original = coupled.store.generic.as_mut().unwrap().test_conditional_source_mut(id).unwrap();
            original.arms[0].body = original.arms[1].body.clone();
            original.block_payload[3] = replacement;
            assert!(FullVerifier::verify(&coupled).is_err(), "an encoded arm and rewritten expected arm cannot authorize each other");
        });
    }

    #[test]
    fn original_not_decisions_refuse_replaced_compiler_boolean_tails() {
        crate::runtime::eval::run_eval(|| {
            let original_source = "pure identity(value) { value }\npure selected(value: Bool) -> Bool { identity(!value) }\nprint ${selected(false)}\n";
            let program = prepared(original_source);
            let _symbols = program.symbol_owner().enter();
            let generic = program.store.generic.as_ref().unwrap();
            let (id, source) = generic.conditional_sources().find(|(_, source)| source.kind == ConditionalKind::Not).unwrap();
            let ConditionalBody::Boolean { instruction, value: false } = source.arms[0].body else { panic!("Not retains its false decision for a true operand"); };
            let mut changed = program.clone();
            let range = changed.store.data[instruction as usize].range().bounds(changed.store.extra.len()).unwrap();
            changed.store.extra[range.start] = 1;
            assert!(FullVerifier::verify(&changed).is_err());
            let mut coupled = changed.clone();
            let original = coupled.store.generic.as_mut().unwrap().test_conditional_source_mut(id).unwrap();
            original.arms[0].body = ConditionalBody::Boolean { instruction, value: true };
            assert!(FullVerifier::verify(&coupled).is_err(), "a rewritten compiler decision cannot change the original Not operator");
            execute(original_source, b"true\n");
        });
    }
}
