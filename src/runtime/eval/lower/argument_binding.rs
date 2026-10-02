use super::*;

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn original_native_static_plan(&self, id: ExprId) -> Option<(crate::sema::registry_graph::RegistryCandidate, crate::sema::check::SolvedOperation, crate::sema::inference::Arrow)> {
        use crate::modules::signature::{ApiArgCheck, ImplBinding, SemanticRule};
        use crate::sema::registry_graph::RegistryOwner;
        let solved = self.solved();
        let operation = solved.operations.get(&self.expression_identity(id))?;
        let selected = solved.graph.candidate_evidence(operation.requirement).ok()??;
        let crate::sema::check::SolvedOperationAuthority::Registry(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate).ok()? else { return None; };
        if metadata.binding != ImplBinding::Native || metadata.semantic_rule != SemanticRule::Standard
            || !matches!(metadata.argument_check, ApiArgCheck::Standard | ApiArgCheck::JsonCompatible)
            || !matches!(metadata.owner, RegistryOwner::Module(_) | RegistryOwner::Method(_))
            || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some()
            || !operation.argument_coercions.is_empty() { return None; }
        let signature = solved.graph.resolved(selected.signature).ok()?;
        let crate::sema::inference::TypeNode::Arrow(arrow) = solved.graph.node(signature).ok()? else { return None; };
        if arrow.params.iter().any(|parameter| parameter.rest) { return None; }
        if matches!(metadata.owner, RegistryOwner::Method(_)) {
            let parameter_count = arrow.params.len().checked_sub(1)?;
            let first_default = operation.binding.default_slots.iter().copied().min().unwrap_or(parameter_count);
            if operation.binding.supplied_slots.iter().any(|&slot| slot >= first_default) { return None; }
        }
        Some((metadata.clone(), operation.clone(), arrow.clone()))
    }

    pub(super) fn lower_original_native_named_call(
        &mut self, id: ExprId, callee: ExprId, slots: &mut SlotScope,
        current_function: Option<Name>, item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        use crate::sema::registry_graph::RegistryOwner;
        let (metadata, operation, arrow) = self.original_native_static_plan(id)?;
        let sources = self.solved().argument_sources.get(&self.expression_identity(id)).cloned()?;
        let offset = usize::from(operation.receiver.is_some());
        let parameters = arrow.params.get(offset..)?;
        if sources.len() != operation.binding.supplied_slots.len() || metadata.parameters.len() != parameters.len()
            || parameters.iter().zip(&metadata.parameters).any(|(formal, original)| formal.label != original.label || formal.defaulted != original.defaulted) { return None; }
        let span = self.program.arena.expr(id).span;
        let mut bindings = Vec::new();
        let receiver = match metadata.owner {
            RegistryOwner::Method(_) if offset == 1 => {
                let receiver = match self.program.arena.expr(callee).kind {
                    ArenaExprKind::Field { base, .. } => self.lower_expr(base, slots, current_function, item_slot)?,
                    ArenaExprKind::NullSafeField { base, .. } => self.lower_postfix_receiver(base, slots, current_function, item_slot)?,
                    _ => return None,
                };
                let slot = slots.reserve("native call receiver");
                bindings.push((receiver, slot));
                Some(push_build_row!(self, expr, BuildExprRow::Param(slot)))
            }
            RegistryOwner::Module(_) if offset == 0 => None,
            _ => return None,
        };
        let lowered = self.lower_source_argument_values(sources.iter().map(|source| (source.entry_index, source.value, source.span)),
            slots, current_function, item_slot)?;
        self.record_original_argument_bindings(id, &sources, &lowered)?;
        bindings.extend(lowered.bindings);
        let mut arguments = vec![None; parameters.len()];
        for ((source, value), &slot) in sources.iter().zip(lowered.values).zip(&operation.binding.supplied_slots) {
            let parameter = parameters.get(slot)?;
            if source.name.is_some_and(|name| name != parameter.label)
                || matches!(source.value, crate::sema::arguments::ArgumentValueSource::PositionalSplice(_))
                || arguments.get(slot)?.is_some() { return None; }
            let ty = self.solved_type(parameter.ty)?;
            let value = if matches!(metadata.owner, RegistryOwner::Method(MethodReceiver::Map)) && slot == 0 && ty == Type::UInt {
                self.require_uint_key(value, source.span)
            } else { self.checked_unsigned_value(value, &ty, source.span) };
            arguments[slot] = Some(value);
        }
        for &slot in &operation.binding.default_slots {
            if !parameters.get(slot)?.defaulted || arguments.get(slot)?.is_some() { return None; }
        }
        if arguments.iter().enumerate().any(|(slot, value)| value.is_none() != operation.binding.default_slots.contains(&slot)) { return None; }
        let value = if let Some(receiver) = receiver {
            // Method rows represent trailing host defaults by their omitted tail.
            // An interior omission needs a separate optional-slot protocol.
            let supplied = arguments.iter().take_while(|value| value.is_some()).count();
            if arguments[supplied..].iter().any(Option::is_some) { return None; }
            push_build_row!(self, expr, BuildExprRow::Method {
                receiver, name: crate::symbol::NameText::Dynamic(Arc::from(metadata.entry)), args: arguments.into_iter().take(supplied).flatten().collect(), span,
            })
        } else {
            push_build_row!(self, expr, BuildExprRow::ModuleCall { cli_plan: None, op: metadata.operation, args: arguments, span })
        };
        Some(self.wrap_argument_bindings(value, bindings, span))
    }

    pub(super) fn record_original_callable_receiver(
        &self, base: ExprId, initializer: BuildExprId, read: BuildExprId, slot: usize,
    ) -> Option<()> {
        let origin = self.expression_identity(base);
        let mut scratch = self.scratch.borrow_mut();
        let Some(&binding_identity) = scratch.callable_binding_uses.get(&origin) else { return Some(()); };
        let binding = self.solved().bindings.get(&binding_identity)?;
        let definition = scratch.callable_binding_origins.get(&binding_identity)?;
        if binding.mutable || definition.source_type.ty != binding.ty
            || self.solved().expression_owners.get(&origin).copied() != binding.owner
            || !matches!(scratch.expressions.get(read.index())?, BuildExprRow::Param(actual) if *actual == slot) { return None; }
        let ty = *self.solved().expressions.get(&origin)?;
        self.solved().graph.validate_scoped(crate::sema::inference::ScopedRoot {
            ty, scope: self.solved().expression_scope(origin, binding.owner).ok()?,
        }).ok()?;
        scratch.callable_receiver_origins.insert(read, super::super::BuildCallableReceiverOrigin {
            origin, binding: binding_identity, initializer, slot, wrapper: None,
        });
        if scratch.callable_receiver_initializers.insert((initializer, slot), read).is_some() { return None; }
        Some(())
    }

    pub(super) fn original_source_instruction(&self, mut instruction: BuildExprId) -> Option<BuildExprId> {
        loop {
            let next = {
                let scratch = self.scratch.borrow();
                match scratch.expressions.get(instruction.index())? {
                    BuildExprRow::CheckedValue { value, .. } => Some(*value),
                    BuildExprRow::MatchExpr { arms, .. } if arms.len() == 1 && arms[0].1.is_none()
                        && matches!(scratch.patterns.get(arms[0].0.index())?, BuildPatternRow::Bind { .. }) => Some(arms[0].2),
                    _ => None,
                }
            };
            let Some(next) = next.filter(|next| next.index() < instruction.index()) else { return Some(instruction); };
            instruction = next;
        }
    }

    // A saved read carries the operand's value, but its initializer owns the
    // original operation. Attaching that operation to a Param changes its proof.
    pub(super) fn checked_saved_argument_read(&self, original: ExpressionIdentity, read: BuildExprId) -> Option<()> {
        let initializer = {
            let scratch = self.scratch.borrow();
            let argument = scratch.argument_binding_origins.get(&read)?;
            if self.solved().argument_sources.get(&argument.call)?.get(argument.ordinal)? != &argument.recipe
                || !matches!(scratch.expressions.get(read.index())?, BuildExprRow::Param(slot) if *slot == argument.slot)
                || argument.initializer.index() >= read.index() { return None; }
            let expression = match argument.recipe.value {
                crate::sema::arguments::ArgumentValueSource::Expression(expression)
                | crate::sema::arguments::ArgumentValueSource::PositionalSplice(expression) => expression,
                crate::sema::arguments::ArgumentValueSource::RecordField { .. } => return None,
            };
            if (ExpressionIdentity { expression, ..argument.call }) != original { return None; }
            argument.initializer
        };
        let instruction = self.original_source_instruction(initializer)?;
        if self.expression_origins.get(&instruction) != Some(&original) { return None; }
        Some(())
    }

    // Temporary adapter syntax has no checked source identity. Its field read
    // is authorized only by the original recipe and saved record entry.
    pub(super) fn checked_legacy_argument_projection_read(&self, projection: ExprId, read: BuildExprId) -> Option<()> {
        let scratch = self.scratch.borrow();
        let argument = scratch.argument_binding_origins.get(&read)?;
        if self.solved().argument_sources.get(&argument.call)?.get(argument.ordinal)? != &argument.recipe
            || !matches!(scratch.expressions.get(read.index())?, BuildExprRow::Param(slot) if *slot == argument.slot) { return None; }
        let crate::sema::arguments::ArgumentValueSource::RecordField { record, field } = argument.recipe.value else { return None; };
        if !matches!(self.program.arena.expr(projection).kind, ArenaExprKind::Field { base, name } if base == record && name == field) { return None; }
        let BuildExprRow::Field { base, name, .. } = scratch.expressions.get(argument.initializer.index())? else { return None; };
        if name.as_str() != field.as_str().as_str() { return None; }
        let saved_record = scratch.argument_record_binding_origins.get(base)?;
        if saved_record.call != argument.call || saved_record.entry_index != argument.recipe.entry_index
            || saved_record.record != (ExpressionIdentity { expression: record, ..argument.call })
            || self.solved().expressions.get(&saved_record.record) != Some(&saved_record.source_type.ty)
            || !matches!(scratch.expressions.get(base.index())?, BuildExprRow::Param(slot) if *slot == saved_record.slot)
            || self.expression_origins.get(&self.original_source_instruction(saved_record.initializer)?) != Some(&saved_record.record) { return None; }
        self.solved().graph.validate_scoped(saved_record.source_type).ok()?;
        if let Some((wrapper, pattern)) = saved_record.wrapper {
            if !matches!(scratch.patterns.get(pattern.index())?, BuildPatternRow::Bind { slot } if *slot == saved_record.slot)
                || !matches!(scratch.expressions.get(wrapper.index())?, BuildExprRow::MatchExpr { value, arms, .. }
                    if *value == saved_record.initializer && arms.len() == 1 && arms[0].0 == pattern && arms[0].1.is_none()) { return None; }
        } else if scratch.argument_record_binding_initializers.get(&(saved_record.initializer, saved_record.slot)) != Some(base) { return None; }
        Some(())
    }

    pub(super) fn record_original_argument_bindings(
        &self, call: ExprId, supplied: &[crate::sema::check::SolvedArgumentSource],
        lowered: &LoweredArgumentValues,
    ) -> Option<()> {
        let call = self.expression_identity(call);
        let recipes = self.solved().argument_sources.get(&call)?;
        if recipes != supplied || recipes.len() != lowered.values.len() { return None; }
        let initializers = lowered.bindings.iter().map(|&(value, slot)| (slot, value)).collect::<FxHashMap<_, _>>();
        let mut authored_entries = FxHashMap::default();
        let mut record_count = 0;
        for recipe in recipes {
            let record = match recipe.value {
                crate::sema::arguments::ArgumentValueSource::RecordField { record, .. } => Some(record),
                _ => None,
            };
            match authored_entries.entry(recipe.entry_index) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(record);
                    record_count += usize::from(record.is_some());
                }
                std::collections::hash_map::Entry::Occupied(entry) => {
                    if record.is_none() || *entry.get() != record { return None; }
                }
            }
        }
        let mut scratch = self.scratch.borrow_mut();
        let mut record_entries = std::collections::BTreeSet::new();
        for binding in &lowered.record_bindings {
            if !record_entries.insert(binding.entry_index) { return None; }
            if authored_entries.get(&binding.entry_index) != Some(&Some(binding.record)) { return None; }
            let record = ExpressionIdentity { expression: binding.record, ..call };
            let caller = if let Some(call) = self.solved().calls.get(&call) { call.caller }
                else if let Some(invocation) = self.solved().invocations.get(&call) { invocation.caller }
                else if let Some(operation) = self.solved().operations.get(&call) { operation.caller }
                else if let Some(constructor) = self.solved().constructor_applications.get(&call) { constructor.caller }
                else { return None; };
            if self.solved().expression_owners.get(&record).copied() != caller { return None; }
            let source_type = crate::sema::inference::ScopedRoot {
                ty: *self.solved().expressions.get(&record)?, scope: self.solved().expression_scope(record, caller).ok()?,
            };
            self.solved().graph.validate_scoped(source_type).ok()?;
            if storage::checked_storage_view(&self.solved().graph, Some(source_type)).ok()?.kind != LoweredType::Record { return None; }
            if !matches!(scratch.expressions.get(binding.read.index())?, BuildExprRow::Param(slot) if *slot == binding.slot)
                || initializers.get(&binding.slot) != Some(&binding.initializer) { return None; }
            scratch.argument_record_binding_origins.insert(binding.read, super::super::BuildArgumentRecordBindingOrigin {
                call, entry_index: binding.entry_index, record, initializer: binding.initializer,
                slot: binding.slot, source_type, wrapper: None,
            });
            if scratch.argument_record_binding_initializers.insert((binding.initializer, binding.slot), binding.read).is_some() { return None; }
        }
        if record_entries.len() != record_count { return None; }
        for (ordinal, (recipe, &read)) in recipes.iter().zip(&lowered.values).enumerate() {
            let BuildExprRow::Param(slot) = scratch.expressions.get(read.index())? else { return None; };
            let slot = *slot;
            let initializer = *initializers.get(&slot)?;
            scratch.argument_binding_origins.insert(read, super::super::BuildArgumentBindingOrigin {
                call, ordinal, recipe: recipe.clone(), initializer, slot, wrapper: None,
            });
            if scratch.argument_binding_initializers.insert((initializer, slot), read).is_some() { return None; }
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::Checker;
    use crate::syntax::parser::Parser;

    #[test]
    fn saved_native_named_arguments_do_not_impersonate_the_original_operand_operation() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure skipped() -> Bool { let absent: Map[Int]? = null; let result = absent?.set(value: 1 / 0, key: \"absent\"); result == null }\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("saved-native-operand-operation.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            let mut functions = Vec::new();
            lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources,
                StdlibLowerLinkage::Local, |unit| {
                    assert!(unit.is_lowered(), "{:?}", unit.blocker_detail);
                    functions.push((unit.key, unit.body.unwrap())); Ok(())
                }).unwrap();
            drop(parsed);
            checked.solved.validate().unwrap();
            let function = &functions.iter().find(|(key, _)| *key == LoweredFunctionKey::Name(Name::intern("skipped"))).unwrap().1;
            let scratch = function.scratch.borrow();
            let (read, argument) = scratch.argument_binding_origins.iter().find(|(_, argument)| argument.recipe.name == Some(Name::intern("value"))).unwrap();
            let crate::sema::arguments::ArgumentValueSource::Expression(expression) = argument.recipe.value else { panic!(); };
            let operand = ExpressionIdentity { expression, ..argument.call };
            assert!(checked.solved.operations.contains_key(&operand));
            assert!(matches!(scratch.expressions[read.index()], BuildExprRow::Param(slot) if slot == argument.slot));
            assert!(!function.expression_origins.contains_key(read), "a saved native argument read cannot carry its initializer's original arithmetic operation");
            assert!(function.expression_origins.iter().any(|(instruction, original)| *original == operand
                && !matches!(scratch.expressions[instruction.index()], BuildExprRow::Param(_))));
        });
    }

    #[test]
    fn original_native_finite_spread_keeps_the_checked_record_recipe() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected() -> Int { let values: Map[Int] = {left: 4}; let arguments = {key: \"left\"}; values.get(...arguments) ?? 0 }\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("original-native-record-recipe.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            let mut functions = Vec::new();
            lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources,
                StdlibLowerLinkage::Local, |unit| {
                    assert!(unit.is_lowered(), "{:?}", unit.blocker_detail);
                    functions.push(unit.body.unwrap()); Ok(())
                }).unwrap();
            drop(parsed);
            checked.solved.validate().unwrap();
            let function = &functions[0];
            let scratch = function.scratch.borrow();
            assert_eq!(scratch.argument_record_binding_origins.len(), 1);
            let (_, argument) = scratch.argument_binding_origins.iter().find(|(_, argument)|
                matches!(argument.recipe.value, crate::sema::arguments::ArgumentValueSource::RecordField { .. })).unwrap();
            assert_eq!(checked.solved.argument_sources[&argument.call][argument.ordinal], argument.recipe);
            assert!(matches!(scratch.expressions[argument.initializer.index()], BuildExprRow::Field { .. }));
            assert!(!function.expression_origins.contains_key(&argument.initializer));
        });
    }

    #[test]
    fn saved_named_arguments_keep_original_recipes_and_actual_binding_wrappers_after_source_disposal() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure combine(left: Int, right: Int) -> Int { left + right }\npure caller(value: Int) -> Int { let callback = combine; callback(right: value, left: 7) }\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("saved-call-arguments.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            let mut functions = Vec::new();
            lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources,
                StdlibLowerLinkage::Local, |unit| {
                    assert!(unit.is_lowered(), "{:?}", unit.blocker_detail);
                    functions.push((unit.key, unit.body.unwrap())); Ok(())
                }).unwrap();
            drop(parsed);
            checked.solved.validate().unwrap();
            let function = &functions.iter().find(|(key, _)| *key == LoweredFunctionKey::Name(Name::intern("caller"))).unwrap().1;
            let scratch = function.scratch.borrow();
            assert_eq!(scratch.argument_binding_origins.len(), 2);
            let mut origins = scratch.argument_binding_origins.iter().collect::<Vec<_>>();
            origins.sort_by_key(|(_, origin)| origin.ordinal);
            assert_eq!(origins[0].1.recipe.name, Some(Name::intern("right")));
            assert_eq!(origins[1].1.recipe.name, Some(Name::intern("left")));
            for (read, origin) in origins {
                assert_eq!(origin.recipe, checked.solved.argument_sources[&origin.call][origin.ordinal]);
                assert!(matches!(scratch.expressions[read.index()], BuildExprRow::Param(slot) if slot == origin.slot));
                let (wrapper, pattern) = origin.wrapper.expect("each saved argument has an actual wrapper");
                assert!(matches!(scratch.patterns[pattern.index()], BuildPatternRow::Bind { slot } if slot == origin.slot));
                assert!(matches!(&scratch.expressions[wrapper.index()], BuildExprRow::MatchExpr { value, arms, .. }
                    if *value == origin.initializer && arms.len() == 1 && arms[0].0 == pattern && arms[0].1.is_none()));
                let crate::sema::arguments::ArgumentValueSource::Expression(expression) = origin.recipe.value else { panic!("ordinary arguments retain exact expression recipes"); };
                let source_identity = ExpressionIdentity { expression, ..origin.call };
                assert_eq!(function.expression_origins.get(&origin.initializer), Some(&source_identity));
                assert!(!function.expression_origins.contains_key(read), "a compiler read cannot impersonate an original expression");
            }
        });
    }

    #[test]
    fn saved_finite_record_entries_keep_original_roots_and_projection_reads_after_source_disposal() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure sum(left: Int, right: Int) -> Int { left + right }\npure caller(value: Int) -> Int { sum(...{right: value, left: 7}) }\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("saved-record-entry.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            let mut functions = Vec::new();
            lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources,
                StdlibLowerLinkage::Local, |unit| {
                    assert!(unit.is_lowered(), "{:?}", unit.blocker_detail);
                    functions.push((unit.key, unit.body.unwrap())); Ok(())
                }).unwrap();
            drop(parsed);
            checked.solved.validate().unwrap();
            let function = &functions.iter().find(|(key, _)| *key == LoweredFunctionKey::Name(Name::intern("caller"))).unwrap().1;
            let scratch = function.scratch.borrow();
            assert_eq!(scratch.argument_record_binding_origins.len(), 1, "the original authored spread entry has one separate saved-record authority");
            assert_eq!(scratch.argument_binding_origins.len(), 2);
            let (&read, origin) = scratch.argument_record_binding_origins.iter().next().unwrap();
            assert_eq!(origin.entry_index, 0);
            let recipe = &checked.solved.argument_sources[&origin.call];
            assert_eq!(recipe.len(), 2);
            assert!(recipe.iter().all(|source| source.entry_index == origin.entry_index
                && matches!(source.value, crate::sema::arguments::ArgumentValueSource::RecordField { record, .. } if record == origin.record.expression)));
            assert_eq!(origin.source_type.ty, checked.solved.expressions[&origin.record]);
            assert_eq!(origin.source_type.scope, checked.solved.expression_scope(origin.record, function.solved_declaration).unwrap());
            checked.solved.graph.validate_scoped(origin.source_type).unwrap();
            assert_eq!(storage::checked_storage_view(&checked.solved.graph, Some(origin.source_type)).unwrap().kind, LoweredType::Record);
            assert!(matches!(scratch.expressions[read.index()], BuildExprRow::Param(slot) if slot == origin.slot));
            let (wrapper, pattern) = origin.wrapper.expect("the original record is saved by a real dominating wrapper");
            assert!(matches!(scratch.patterns[pattern.index()], BuildPatternRow::Bind { slot } if slot == origin.slot));
            assert!(matches!(&scratch.expressions[wrapper.index()], BuildExprRow::MatchExpr { value, arms, .. }
                if *value == origin.initializer && arms.len() == 1 && arms[0].0 == pattern && arms[0].1.is_none()));
            assert_eq!(function.expression_origins.get(&origin.initializer), Some(&origin.record));
            assert!(!function.expression_origins.contains_key(&read), "the generated saved-record read has no invented source expression");
            for argument in scratch.argument_binding_origins.values() {
                let crate::sema::arguments::ArgumentValueSource::RecordField { record, field } = argument.recipe.value else { panic!(); };
                assert_eq!(record, origin.record.expression);
                assert!(matches!(&scratch.expressions[argument.initializer.index()], BuildExprRow::Field { base, name, .. }
                    if *base == read && name.as_str() == field.as_str().as_str()));
                assert!(!function.expression_origins.contains_key(&argument.initializer), "synthetic field projections retain their original recipe rather than an invented projection identity");
            }
        });
    }

    #[test]
    fn saved_finite_record_entries_refuse_missing_original_root_or_lexical_owner() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure sum(left: Int, right: Int) -> Int { left + right }\npure caller(value: Int) -> Int { sum(...{right: value, left: 7}) }\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("missing-saved-record-entry.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let mut declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let mut bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            let record = bodies.solved.argument_sources.iter().find_map(|(call, recipes)| recipes.iter().find_map(|recipe| {
                if let crate::sema::arguments::ArgumentValueSource::RecordField { record, .. } = recipe.value {
                    Some(ExpressionIdentity { expression: record, ..*call })
                } else { None }
            })).unwrap();
            let caller = Name::intern("caller");
            let lower_caller = |bodies: &CompactBodyProbeOutput, declarations: &CompactDeclOutput| {
                let mut result = None;
                lower_compact_function_units_into(&parsed.arena, declarations, bodies, source, &sources,
                    StdlibLowerLinkage::Local, |unit| {
                        if unit.key == LoweredFunctionKey::Name(caller) { result = Some(unit.is_lowered()); }
                        Ok(())
                    }).unwrap();
                result.unwrap()
            };
            assert!(lower_caller(&bodies, &declarations));
            drop(checked);
            declarations.solved = Default::default();
            let actual = Arc::get_mut(&mut bodies.solved).unwrap().expressions.remove(&record).unwrap();
            assert!(!lower_caller(&bodies, &declarations), "a finite tree mirror cannot invent the original spread record root");
            Arc::get_mut(&mut bodies.solved).unwrap().expressions.insert(record, actual);
            let owner = Arc::get_mut(&mut bodies.solved).unwrap().expression_owners.remove(&record).unwrap();
            assert!(!lower_caller(&bodies, &declarations), "a closed storage shell cannot authorize a missing original lexical owner");
            Arc::get_mut(&mut bodies.solved).unwrap().expression_owners.insert(record, owner);
            assert!(lower_caller(&bodies, &declarations));
            drop(parsed);
            bodies.solved.validate().unwrap();
        });
    }
}
