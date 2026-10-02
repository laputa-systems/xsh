use super::*;
use super::super::generic::{MutablePathReceipt, MutablePathStep, OperationSourceOrigin, graph_ground_type};
use super::super::generic::MutablePathEncoding;
use crate::runtime::eval::lower::mutable_path::{BuildMutablePathStep, BuildMutablePathValue};

fn path_problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

impl FullBuilder {
    pub(super) fn stage_mutable_path_statement(&mut self, row: BuildStmtId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(original) = scratch.mutable_path_writes.get(&row) else { return Ok(()); };
        let binding_root = scratch.mutable_binding_origins.get(&original.binding).map(|definition| definition.source_type).or_else(|| scratch.mutable_driver_bindings.get(&original.binding).map(|definition| definition.source_type)).ok_or_else(|| path_problem("mutable_path_original_binding"))?;
        let (slot, path, op, value, check, field_encoding) = match scratch.statements.get(row.index()).ok_or_else(|| path_problem("mutable_path_original_row"))? {
            BuildStmtRow::AssignPath { slot, path, op, value, check, .. } => (*slot, path.0.clone(), *op, BuildMutablePathValue::Value(*value), check.as_ref().map(|check| &check.ty), None),
            BuildStmtRow::AssignField { slot, field, op, value, .. } => (*slot, vec![LoweredAssignStep::Field(Name::intern(field))], *op, BuildMutablePathValue::Value(*value), None, Some((Name::intern(field), false))),
            BuildStmtRow::AssignFieldInt { slot, field, op, value, .. } => (*slot, vec![LoweredAssignStep::Field(Name::intern(field))], *op, BuildMutablePathValue::Integer(*value), None, Some((Name::intern(field), true))),
            _ => return Err(path_problem("mutable_path_row_changed")),
        };
        if slot != original.slot || op != original.op || value != original.value || check != original.check.as_ref() || path.len() != original.steps.len() { return Err(path_problem("mutable_path_original_shape_changed")); }
        for (step, actual) in original.steps.iter().zip(&path) {
            if !match (step, actual) { (BuildMutablePathStep::Field { name, .. }, LoweredAssignStep::Field(actual)) => name == actual, (BuildMutablePathStep::Index { expression, .. }, LoweredAssignStep::Index(actual)) => expression == actual, _ => false } { return Err(path_problem("mutable_path_original_selector_changed")); }
        }
        let raw = self.current_owner.ok_or_else(|| path_problem("mutable_path_owner"))?;
        let owner = if let Some(driver) = driver_owner_index(raw) { InstructionOwner::Driver(driver as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| path_problem("mutable_path_owner"))?) };
        let solved = self.solved.clone().ok_or_else(|| path_problem("mutable_path_solved"))?;
        let binding = solved.bindings.get(&original.binding).ok_or_else(|| path_problem("mutable_path_binding"))?;
        if !binding.mutable || binding.ty != binding_root.ty || solved.expressions.get(&original.value_source) != Some(&original.value_type.ty) || solved.expression_owners.get(&original.value_source).copied() != binding.owner { return Err(path_problem("mutable_path_original_checked_value")); }
        let mut roots = vec![binding_root, original.selected_type, original.value_type];
        for step in original.steps.iter() {
            match step { BuildMutablePathStep::Field { input, output, .. } => roots.extend([*input, *output]), BuildMutablePathStep::Index { checked, input, output, .. } => roots.extend([*checked, *input, *output]) }
        }
        for root in roots { solved.graph.validate_scoped(root).map_err(|_| path_problem("mutable_path_scope"))?; }
        let intern = |builder: &mut Self, root: crate::sema::inference::ScopedRoot| builder.intern_generic_ground_type(&graph_ground_type(&solved.graph, root.ty).map_err(|_| path_problem("mutable_path_ground_root"))?);
        let binding_type = intern(self, binding_root)?;
        let selected_ground = graph_ground_type(&solved.graph, original.selected_type.ty).map_err(|_| path_problem("mutable_path_selected_root"))?;
        if original.check.as_ref() != selected_ground.has_unsigned_constraint().then_some(&selected_ground) { return Err(path_problem("mutable_path_storage_constraint_changed")); }
        let selected_type = intern(self, original.selected_type)?;
        let value_type = intern(self, original.value_type)?;
        let compound = if let Some(operation) = &original.compound {
            Some(self.prepare_mutable_compound_contract(original.statement, operation, original.op, selected_type, value_type, selected_type)?)
        } else { None };
        let expected_tag = match field_encoding { None => FullTag::StmtAssignPath, Some((_, false)) => FullTag::StmtAssignField, Some((_, true)) => FullTag::StmtAssignFieldInt };
        if self.store.tags.get(instruction as usize) != Some(&expected_tag) { return Err(path_problem("mutable_path_encoded_kind")); }
        let payload = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| path_problem("mutable_path_payload"))?.to_vec().into_boxed_slice();
        let (encoding, selectors) = if let Some((name, integer)) = field_encoding {
            if self.store.string(*payload.get(1).ok_or_else(|| path_problem("mutable_field_name"))?).map_err(|_| path_problem("mutable_field_name"))? != name.as_str().as_str() { return Err(path_problem("mutable_field_name_changed")); }
            (MutablePathEncoding::Field { name, integer }, vec![[0, name.symbol().raw()]])
        } else {
            let path_block = *payload.get(1).ok_or_else(|| path_problem("mutable_path_block"))?;
            let block = IrBlockId::from_raw(path_block).and_then(|block| self.store.blocks.get(block.index())).ok_or_else(|| path_problem("mutable_path_block"))?;
            let path_payload = self.store.payload(block.instructions).map_err(|_| path_problem("mutable_path_selectors"))?.to_vec().into_boxed_slice();
            if path_payload.first().copied() != Some(original.steps.len() as u32) || path_payload.len() != 1 + 2 * original.steps.len() { return Err(path_problem("mutable_path_encoded_shape")); }
            let selectors = path_payload[1..].chunks_exact(2).map(|step| [step[0], step[1]]).collect();
            (MutablePathEncoding::Block { block: path_block, owner: block.owner, flags: block.flags, payload: path_payload }, selectors)
        };
        if payload.first().copied() != Some(original.slot as u32) { return Err(path_problem("mutable_path_encoded_shape")); }
        let value = *payload.get(3).ok_or_else(|| path_problem("mutable_path_rhs"))?;
        if match original.value { BuildMutablePathValue::Value(value) => self.active_expression_origins.get(&value) != Some(&original.value_source), BuildMutablePathValue::Integer(value) => scratch.int_expression_origins.get(&value) != Some(&original.value_source) } { return Err(path_problem("mutable_path_rhs_wrapper_not_prepared")); }
        let value_tag = *self.store.tags.get(value as usize).ok_or_else(|| path_problem("mutable_path_rhs_kind"))?;
        let value_payload = self.store.payload(self.store.data[value as usize].range()).map_err(|_| path_problem("mutable_path_rhs_payload"))?.to_vec().into_boxed_slice();
        let mut steps = Vec::new();
        for (authored, encoded) in original.steps.iter().zip(&selectors) {
            let (input_root, output_root) = match authored { BuildMutablePathStep::Field { input, output, .. } | BuildMutablePathStep::Index { input, output, .. } => (*input, *output) };
            let input = intern(self, input_root)?;
            let output = intern(self, output_root)?;
            let step = match authored {
                BuildMutablePathStep::Field { name, .. } => {
                    if encoded[0] != 0 || Name::from_symbol(Symbol::from_raw(encoded[1])) != *name { return Err(path_problem("mutable_path_field_changed")); }
                    MutablePathStep::Field { name: *name, input, output, input_root, output_root }
                }
                BuildMutablePathStep::Index { source, checked, .. } => {
                    if encoded[0] != 1 || solved.expressions.get(source) != Some(&checked.ty) || solved.expression_owners.get(source).copied() != binding.owner || solved.expression_scope(*source, binding.owner).ok() != Some(checked.scope) { return Err(path_problem("mutable_path_index_changed")); }
                    let checked_type = intern(self, *checked)?;
                    let selector = encoded[1];
                    let tag = *self.store.tags.get(selector as usize).ok_or_else(|| path_problem("mutable_selector_instruction"))?;
                    let selector_payload = self.store.payload(self.store.data[selector as usize].range()).map_err(|_| path_problem("mutable_selector_payload"))?.to_vec().into_boxed_slice();
                    self.generic_evidence_mut().register_instruction_origin(selector, OperationSourceOrigin::Expression(*source), owner).map_err(|_| path_problem("mutable_selector_source"))?;
                    MutablePathStep::Index { instruction: selector, source: *source, checked: checked_type, checked_root: *checked, input, output, input_root, output_root, tag, payload: selector_payload }
                }
            };
            steps.push(step);
        }
        self.generic_evidence_mut().register_instruction_origin(value, OperationSourceOrigin::Expression(original.value_source), owner).map_err(|_| path_problem("mutable_path_rhs_source"))?;
        self.generic_evidence_mut().register_instruction_origin(instruction, OperationSourceOrigin::Statement(original.statement), owner).map_err(|_| path_problem("mutable_path_statement_source"))?;
        self.generic_evidence_mut().add_mutable_path(MutablePathReceipt { binding: original.binding, statement: original.statement, target: original.target, instruction, owner, slot: original.slot as u32, payload, encoding, steps: steps.into_boxed_slice(), binding_type, binding_root, selected_type, selected_root: original.selected_type, value, value_tag, value_payload, value_source: original.value_source, value_type, value_root: original.value_type, compound }).map_err(|_| path_problem("mutable_path_receipt"))
    }
}

impl FullVerifier {
    // Graph child roots select the original storage path. Ground carriers only
    // validate that its encoding and checked replacement preserve that path.
    pub(super) fn verify_mutable_path_contract(store: &FullStore, generic: &GenericEvidenceStore, receipt: &MutablePathReceipt) -> Result<(), IrVerifyError> {
        generic.mutable_path_at(receipt.instruction)?.ok_or_else(|| IrVerifyError::new("mutable path loses its original receipt"))?;
        let tag = match receipt.encoding { MutablePathEncoding::Block { .. } => FullTag::StmtAssignPath, MutablePathEncoding::Field { integer: false, .. } => FullTag::StmtAssignField, MutablePathEncoding::Field { integer: true, .. } => FullTag::StmtAssignFieldInt };
        if store.tags.get(receipt.instruction as usize) != Some(&tag) || store.payload(store.data[receipt.instruction as usize].range())? != receipt.payload.as_ref() { return Err(IrVerifyError::new("mutable path changes its original assignment")); }
        let op = match receipt.compound.as_ref().map(|compound| compound.operation) {
            Some(crate::sema::operation_graph::PreparedLanguageOperation::Compound { op, .. }) => op,
            None => AssignOp::Set,
            _ => return Err(IrVerifyError::new("mutable path changes its original compound selection")),
        };
        Self::verify_mutable_assignment_operator(store, receipt.instruction, op, 2)?;
        match &receipt.encoding {
            MutablePathEncoding::Block { block, owner, flags, payload } => {
                let block = IrBlockId::from_raw(*block).and_then(|block| store.blocks.get(block.index())).ok_or_else(|| IrVerifyError::new("mutable path block is invalid"))?;
                if block.owner != *owner || block.flags != *flags || store.payload(block.instructions)? != payload.as_ref() { return Err(IrVerifyError::new("mutable path changes its original selectors")); }
            }
            MutablePathEncoding::Field { name, integer } => {
                if receipt.steps.len() != 1 || !matches!(receipt.steps[0], MutablePathStep::Field { name: selected, .. } if selected == *name)
                    || store.string(*receipt.payload.get(1).ok_or_else(|| IrVerifyError::new("mutable field name is missing"))?)? != name.as_str().as_str()
                    || (*integer && store.semantic.to_type(receipt.value_type)? != Type::Int)
                    || store.semantic.to_type(receipt.selected_type)?.has_unsigned_constraint() { return Err(IrVerifyError::new("mutable field changes its original named storage projection")); }
            }
        }
        if let Some(compound) = &receipt.compound {
            if compound.effects != crate::sema::inference::EffectSet::EMPTY || compound.left != receipt.selected_type || compound.right != receipt.value_type || compound.result != receipt.selected_type { return Err(IrVerifyError::new("mutable compound changes its original operand, result, or effect relationship")); }
        }
        let mut selected = store.semantic.to_type(receipt.binding_type)?;
        for step in receipt.steps.iter() {
            let (input, output) = match step { MutablePathStep::Field { input, output, .. } | MutablePathStep::Index { input, output, .. } => (*input, *output) };
            if selected != store.semantic.to_type(input)? { return Err(IrVerifyError::new("mutable path loses its invariant input type")); }
            selected = match step {
                MutablePathStep::Field { name, .. } => {
                    let Type::Record(fields) = selected else { return Err(IrVerifyError::new("mutable field requires its original record")); };
                    fields.get(name).cloned().ok_or_else(|| IrVerifyError::new("mutable field loses its original member"))?
                }
                MutablePathStep::Index { instruction, checked, tag, payload, .. } => {
                    if store.tags.get(*instruction as usize) != Some(tag) || store.payload(store.data[*instruction as usize].range())? != payload.as_ref() { return Err(IrVerifyError::new("mutable selector changes its original checked expression")); }
                    Self::verify_generic_source(store, generic, *instruction, receipt.owner, &store.semantic.to_type(*checked)?, None, &mut Vec::new())?;
                    match selected {
                        Type::List(item) if matches!(store.semantic.to_type(*checked)?, Type::Int | Type::UInt) => *item,
                        Type::Map(key, value) if store.semantic.to_type(*checked)?.matches_expected(&key) => *value,
                        _ => return Err(IrVerifyError::new("mutable index requires its original collection and key relationship")),
                    }
                }
            };
            if selected != store.semantic.to_type(output)? { return Err(IrVerifyError::new("mutable selector changes its original child type")); }
        }
        if !store.semantic.to_type(receipt.value_type)?.matches_expected(&selected) { return Err(IrVerifyError::new("mutable replacement changes its original selected storage relationship")); }
        if selected != store.semantic.to_type(receipt.selected_type)? { return Err(IrVerifyError::new("mutable path changes its selected storage type")); }
        if store.tags.get(receipt.value as usize) != Some(&receipt.value_tag) || store.payload(store.data[receipt.value as usize].range())? != receipt.value_payload.as_ref() { return Err(IrVerifyError::new("mutable path changes its original checked replacement")); }
        Self::verify_generic_source(store, generic, receipt.value, receipt.owner, &store.semantic.to_type(receipt.value_type)?, None, &mut Vec::new())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::eval::Evaluator;
    use crate::runtime::value::Value;
    use crate::sema::check::Checker;
    use crate::syntax::parser::Parser;

    fn fixture(source: &str) -> FullProgram {
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("mutable-path-proof.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let solved = Arc::downgrade(&checked.solved);
        let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        let prepared = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id);
        drop(parsed); drop(checked); drop(declarations); drop(bodies);
        assert!(solved.upgrade().is_none());
        prepared.unwrap()
    }

    fn execute(source: &str, stdout: &[u8]) {
        for recursive in [false, true] {
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("mutable-path-routes.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone();
            symbols.with_current(|| {
                let checked = Checker::check_arena(&parsed.arena, source);
                assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                let solved = Arc::downgrade(&checked.solved);
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
                let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
                drop(checked); drop(parsed);
                assert!(solved.upgrade().is_none());
                let work = || evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("mutable path program remains installed"));
                let output = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern("updated")), recursive, work);
                assert_eq!(output.stdout, stdout, "{:?}", output.diagnostics);
                assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
            });
        }
    }

    #[test]
    fn mutable_record_fields_preserve_original_names_extras_and_aliases_both_routes() {
        crate::runtime::eval::run_eval(|| {
            execute("type Inner = {value: Str?, count: Int}\ntype Outer = {inner: Inner}\npure updated() -> Int { var report: Outer = {inner: {value: \"ready\", count: 1}}; let older = report; report.inner.count = 2; report.inner.count + older.inner.count }\nprint updated()\n", b"3\n");
            execute("type Row = {count: Int, spare: Int}\npure updated() -> Int { var row: Row = {count: 1, spare: 7}; let older = row; row.count = 3; row.count += 2; row.count + older.count + row.spare }\nprint updated()\n", b"13\n");
            execute("pure updated() -> Str { var row = {label: \"old\", spare: 7}; let older = row; row.label = \"next\"; row.label + older.label }\nprint updated()\n", b"nextold\n");
        });
    }

    #[test]
    fn mutable_record_uint_field_retains_checked_path_before_host_effect_both_routes() {
        crate::runtime::eval::run_eval(|| {
            for update in ["row.count = -1", "row.count -= 2"] {
                let source = format!("type Row = {{count: UInt, spare: Int}}\npure updated(input: Row) -> Int {{ var row = input; {update}; row.spare }}\nproc entry(input: Row) [] -> Int {{ let result = updated(input); print \"forbidden\"; result }}\n");
                let program = Arc::new(fixture(&source));
                program.symbol_owner().with_current(|| {
                    let path = program.generic_evidence().unwrap().mutable_paths().next().unwrap();
                    assert!(matches!(path.encoding, MutablePathEncoding::Block { .. }), "unsigned fields keep their checked assignment opcode");
                    let args = [Value::Record(BTreeMap::from([(Arc::from("count"), Value::Int(1)), (Arc::from("spare"), Value::Int(7))]).into())];
                    for recursive in [false, true] {
                        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                        evaluator.indexed_program = Some(Arc::clone(&program));
                        let work = || evaluator.call_indexed_direct(LoweredFunctionKey::Name(Name::intern("entry")), LoweredFunctionKind::Proc, &args, Span::new(program.store.source_id, 0, 0)).expect("original typed entry exists");
                        let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern("updated")), recursive, work);
                        let error = result.unwrap_err();
                        assert_eq!(error.kind, "type-error");
                        assert!(error.message.contains("UInt"), "{}", error.message);
                        assert!(evaluator.stdout.is_empty());
                    }
                });
            }
        });
    }

    #[test]
    fn mutable_record_field_refuses_same_typed_name_rhs_slot_and_operator_rewrites() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture("pure updated() -> Int { var row = {count: 1, spare: 7}; row.count += 3; row.count }\n");
            program.symbol_owner().with_current(|| {
                FullVerifier::verify(&program).unwrap();
                let path = program.generic_evidence().unwrap().mutable_paths().next().unwrap();
                assert!(matches!(path.encoding, MutablePathEncoding::Field { integer: true, .. }));
                let mut name = program.clone();
                let string = IrStringId::from_raw(path.payload[1]).unwrap();
                let range = name.store.strings[string.index()].bounds(name.store.string_bytes.len()).unwrap();
                name.store.string_bytes[range].copy_from_slice(b"spare");
                assert!(FullVerifier::verify_mutable_path_contract(&name.store, name.generic_evidence().unwrap(), path).unwrap_err().message.contains("named storage projection"));
                assert!(FullVerifier::verify(&name).is_err(), "same typed member cannot replace the original named projection");
                let mut rhs = program.clone();
                let range = rhs.store.data[path.value as usize].range();
                rhs.store.extra[range.start as usize] = 8;
                assert!(FullVerifier::verify(&rhs).is_err());
                let mut slot = program.clone();
                let range = slot.store.data[path.instruction as usize].range();
                slot.store.extra[range.start as usize] = path.slot + 1;
                assert!(FullVerifier::verify(&slot).is_err());
                let mut op = program.clone();
                op.store.assign_ops[path.payload[2] as usize] = AssignOp::Set;
                assert!(FullVerifier::verify(&op).is_err());
                let mut agreeing = program.clone();
                agreeing.store.generic.as_deref_mut().unwrap().test_mutable_path_mut(path.instruction).unwrap().payload[0] = path.slot + 1;
                let range = agreeing.store.data[path.instruction as usize].range();
                agreeing.store.extra[range.start as usize] = path.slot + 1;
                assert!(FullVerifier::verify(&agreeing).is_err());
            });
        });
    }

    #[test]
    fn mutable_list_paths_preserve_aliases_and_selector_order_both_routes_after_frontend_disposal() {
        crate::runtime::eval::run_eval(|| {
            execute("pure updated() -> Int { var rows = [1, 2]; let earlier = rows; rows[1] = 9; rows[0] + rows[1] + earlier[1] }\nprint updated()\n", b"12\n");
            let source = format!("{}\nprint updated()\n", include_str!("../../../../../tests/fixtures/frontend-indexed/list-assignment.xsh"));
            execute(&source, b"16\n");
            execute("proc selector() [] -> Int { print \"selector\"; 0 }\nproc replacement() [] -> Int { print \"rhs\"; 9 }\nproc updated() [] -> Int { var rows = [1]; rows[selector()] = replacement(); rows[0] }\nprint updated()\n", b"selector\nrhs\n9\n");
        });
    }

    #[test]
    fn mutable_uint_paths_reject_compound_and_nested_replacements_before_host_effect_both_routes() {
        crate::runtime::eval::run_eval(|| {
            for (source, nested) in [
                ("pure updated(input: List[UInt]) -> UInt { var rows = input; rows[0] -= 2; rows[0] }\nproc entry(input: List[UInt]) [] -> UInt { let result = updated(input); print \"forbidden\"; result }\n", false),
                ("type Row = {count: UInt}\npure updated(input: List[Row]) -> UInt { var rows = input; rows[0].count = -1; rows[0].count }\nproc entry(input: List[Row]) [] -> UInt { let result = updated(input); print \"forbidden\"; result }\n", true),
            ] {
                let program = Arc::new(fixture(source));
                program.symbol_owner().with_current(|| {
                    let initial = if nested { Value::Record(BTreeMap::from([(Arc::from("count"), Value::Int(1))]).into()) } else { Value::Int(1) };
                    let args = [Value::List(vec![initial])];
                    for recursive in [false, true] {
                        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                        evaluator.indexed_program = Some(Arc::clone(&program));
                        let work = || evaluator.call_indexed_direct(LoweredFunctionKey::Name(Name::intern("entry")), LoweredFunctionKind::Proc, &args, Span::new(program.store.source_id, 0, 0)).expect("original typed entry exists");
                        let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern("updated")), recursive, work);
                        let error = result.unwrap_err();
                        assert_eq!(error.kind, "type-error");
                        assert!(error.message.contains("UInt"), "{}", error.message);
                        assert!(evaluator.stdout.is_empty());
                    }
                });
            }
        });
    }

    #[test]
    fn mutable_map_paths_preserve_typed_keys_and_aliases_both_routes_after_frontend_disposal() {
        crate::runtime::eval::run_eval(|| {
            execute("pure updated() -> Int { var values: Map[Int, Int] = {[20]: 2, [3]: 1}; let older = values; values[3] = 9; let keys: List[Int] = values.keys(); keys[0] + older[3] + (values.get(3) ?? 0) }\nprint updated()\n", b"13\n");
        });
    }

    #[test]
    fn mutable_map_selectors_and_replacements_execute_once_in_original_order_both_routes() {
        crate::runtime::eval::run_eval(|| {
            execute("proc selector() [] -> Int { print \"selector\"; 3 }\nproc replacement() [] -> Int { print \"rhs\"; 9 }\nproc updated() [] -> Int { var values: Map[Int, Int] = {[3]: 1}; let older = values; values[selector()] = replacement(); values[3] + older[3] }\nprint updated()\n", b"selector\nrhs\n10\n");
        });
    }

    #[test]
    fn mutable_list_path_rejects_changed_selector_rhs_slot_and_agreeing_receipt() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture("pure updated() -> Int { var rows = [1, 2]; rows[1] = 9; rows[0] }\n");
            program.symbol_owner().with_current(|| {
                FullVerifier::verify(&program).unwrap();
                let path = program.generic_evidence().unwrap().mutable_paths().next().unwrap();
                let MutablePathStep::Index { instruction: selector, .. } = path.steps[0] else { panic!("List index expected"); };
                let mut changed = program.clone();
                let range = changed.store.data[selector as usize].range();
                changed.store.extra[range.start as usize] = 0;
                assert!(FullVerifier::verify(&changed).is_err());
                let mut value = program.clone();
                let range = value.store.data[path.value as usize].range();
                value.store.extra[range.start as usize] = 8;
                assert!(FullVerifier::verify(&value).is_err());
                let mut rhs = program.clone();
                let range = rhs.store.data[path.instruction as usize].range();
                rhs.store.extra[range.start as usize + 3] = selector;
                assert!(FullVerifier::verify(&rhs).is_err());
                let mut slot = program.clone();
                let range = slot.store.data[path.instruction as usize].range();
                slot.store.extra[range.start as usize] = path.slot.checked_add(1).unwrap();
                assert!(FullVerifier::verify(&slot).is_err());
                let mut agreeing = program.clone();
                agreeing.store.generic.as_deref_mut().unwrap().test_mutable_path_mut(path.instruction).unwrap().payload[3] = selector;
                let range = agreeing.store.data[path.instruction as usize].range();
                agreeing.store.extra[range.start as usize + 3] = selector;
                assert!(agreeing.generic_evidence().unwrap().mutable_path_at(path.instruction).unwrap_err().message.contains("original receipt"));
                assert!(FullVerifier::verify(&agreeing).is_err());
                let mut missing = program.clone();
                missing.store.generic.as_deref_mut().unwrap().test_remove_mutable_path(path.instruction);
                assert!(FullVerifier::verify_generic_evidence(&missing.store).unwrap_err().message.contains("original receipt ledger"));
                assert!(FullVerifier::verify(&missing).is_err());
            });
        });
    }

    #[test]
    fn mutable_list_path_compound_rejects_operator_pool_reinterpretation() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture("pure updated() -> Int { var rows = [1]; rows[0] += 2; rows[0] }\n");
            program.symbol_owner().with_current(|| {
                FullVerifier::verify(&program).unwrap();
                let path = program.generic_evidence().unwrap().mutable_paths().next().unwrap();
                let mut changed = program.clone();
                changed.store.assign_ops[path.payload[2] as usize] = AssignOp::Set;
                assert!(FullVerifier::verify(&changed).is_err(), "element compound keeps its original selected assignment operator");
            });
        });
    }

    #[test]
    fn mutable_uint_path_rejects_semantic_reinterpretation_with_original_ground_ids() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture("type Row = {count: UInt}\npure updated(input: List[Row]) -> UInt { var rows = input; rows[0].count = 2; rows[0].count }\n");
            program.symbol_owner().with_current(|| {
                FullVerifier::verify_generic_evidence(&program.store).unwrap();
                let path = program.generic_evidence().unwrap().mutable_paths().next().unwrap();
                let mut changed = program.clone();
                changed.store.semantic.test_reinterpret_uint_as_int();
                let unchanged = changed.generic_evidence().unwrap().mutable_path_at(path.instruction).unwrap().unwrap();
                assert_eq!(unchanged.binding_type, path.binding_type);
                assert_eq!(unchanged.selected_type, path.selected_type);
                assert_eq!(unchanged.value_type, path.value_type);
                assert!(FullVerifier::verify_generic_evidence(&changed.store).is_err(), "original UInt storage cannot be reinterpreted through unchanged type IDs");
            });
        });
    }

    #[test]
    fn mutable_list_assignment_moved_to_sibling_scope_loses_original_binding() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture("pure updated() -> Int { { var left = [1]; left[0] = 2 }; { var right = [1]; right[0] = 3 }; 0 }\n");
            program.symbol_owner().with_current(|| {
                let paths: Vec<_> = program.generic_evidence().unwrap().mutable_paths().collect();
                assert_eq!(paths.len(), 2);
                let positions: Vec<_> = paths.iter().map(|path| program.store.blocks.iter().find_map(|block| {
                    if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_STATEMENTS { return None; }
                    let words = program.store.payload(block.instructions).unwrap();
                    words.iter().position(|instruction| *instruction == path.instruction).map(|position| block.instructions.start as usize + position)
                }).unwrap()).collect();
                let mut sibling = program.clone();
                sibling.store.extra[positions[0]] = paths[1].instruction;
                sibling.store.extra[positions[1]] = paths[0].instruction;
                let error = FullVerifier::verify(&sibling).unwrap_err();
                assert!(error.message.contains("outside its original binding scope"), "{}", error.message);
            });
        });
    }

}
