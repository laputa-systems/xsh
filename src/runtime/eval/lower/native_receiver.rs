use super::*;

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn record_original_native_receiver(
        &self, call: ExprId, base: ExprId, initializer: BuildExprId, read: BuildExprId, slot: usize,
    ) -> Option<()> {
        let call = self.expression_identity(call);
        let Some(operation) = self.solved().operations.get(&call) else { return Some(()); };
        if operation.receiver.is_none() { return Some(()); }
        let origin = self.expression_identity(base);
        let source_type = crate::sema::inference::ScopedRoot {
            ty: *self.solved().expressions.get(&origin)?,
            scope: self.solved().expression_scope(origin, operation.caller).ok()?,
        };
        let receiver = operation.receiver?;
        let same_root = self.solved().graph.resolved(receiver).ok()? == self.solved().graph.resolved(source_type.ty).ok()?;
        let same_ground_type = self.solved().graph.export_type(receiver).ok().zip(self.solved().graph.export_type(source_type.ty).ok()).is_some_and(|(receiver, source)| receiver == source);
        if self.solved().expression_owners.get(&origin).copied() != operation.caller || !same_root && !same_ground_type {
            return None;
        }
        self.solved().graph.validate_scoped(source_type).ok()?;
        let mut scratch = self.scratch.borrow_mut();
        if initializer.index() >= read.index() || !matches!(scratch.expressions.get(read.index())?, BuildExprRow::Param(actual) if *actual == slot) { return None; }
        scratch.native_receiver_origins.insert(read, super::super::indexed::full::BuildSavedNativeReceiverOrigin {
            call, origin, source_type, initializer, slot, wrapper: None,
        });
        if scratch.native_receiver_initializers.insert((initializer, slot), read).is_some() { return None; }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::Checker;
    use crate::syntax::parser::Parser;

    #[test]
    fn saved_native_method_receiver_retains_original_allocation_before_encoding() {
        std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| crate::runtime::eval::run_eval(|| {
            let source = "pure saved(table: Map[Str, List[Int]], item: List[Int]) -> Map[Str, List[Int]] { table.set(key: \"right\", value: item) }\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("saved-native-receiver.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let declarations = Checker::check_compact_declarations(&parsed.arena);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let mut inspected = false;
            lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources,
                StdlibLowerLinkage::Local, |unit| {
                    let body = unit.body.as_ref().unwrap();
                    let scratch = body.scratch.borrow();
                    assert_eq!(scratch.native_receiver_origins.len(), 1, "the authored registry call supplies a saved receiver before full encoding");
                    let (&read, original) = scratch.native_receiver_origins.iter().next().unwrap();
                    assert!(original.wrapper.is_some());
                    assert!(body.expression_origins.get(&read).is_none());
                    assert_eq!(body.expression_origins.get(&original.initializer), Some(&original.origin));
                    inspected = true;
                    Ok(())
                }).unwrap();
            assert!(inspected);
        })).unwrap().join().unwrap_or_else(|payload| std::panic::resume_unwind(payload));
    }
}
