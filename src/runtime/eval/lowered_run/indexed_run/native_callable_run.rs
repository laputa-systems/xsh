use super::*;
use crate::runtime::eval::indexed::generic::{NativeInvocationPlanId, PreparedOperationAuthority};

impl Evaluator {
    pub(super) fn eval_indexed_native_callable(
        &mut self, execution: &FullExecution<'_>, plan: NativeInvocationPlanId,
        callee: &LoweredValue, arguments: IndexedCallArguments, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let LoweredValue::NativeCallable(handle) = callee else {
            return Err(RuntimeError::new("indexed-ir", "prepared native invocation requires its original callable value").with_span(span));
        };
        let program = self.indexed_program.as_ref().ok_or_else(|| RuntimeError::new("indexed-ir", "native invocation has no installed program").with_span(span))?;
        if !Arc::ptr_eq(program, handle.program()) {
            return Err(RuntimeError::new("indexed-ir", "native callable belongs to another program").with_span(span));
        }
        let evidence = execution.generic_evidence().ok_or_else(|| RuntimeError::new("indexed-ir", "native invocation has no prepared evidence").with_span(span))?;
        evidence.validate_native_invocation(handle.id(), plan).map_err(|error| indexed_error(error, span))?;
        let call = &evidence.native_invocation_plan(plan).map_err(|error| indexed_error(error, span))?.contract.call;
        let PreparedOperationAuthority::Registry { operation, .. } = call.authority else {
            return Err(RuntimeError::new("indexed-ir", "native invocation has no registry operation").with_span(span));
        };
        let count = call.argument_sources.len();
        if call.binding.rest_slot.is_some() || call.binding.dynamic.is_some()
            || count > 65536 || arguments.values.len() != call.binding.supplied_slots.len() {
            return Err(RuntimeError::new("indexed-ir", "native argument packet changes its prepared shape").with_span(span));
        }
        let mut omitted = vec![false; count];
        let mut defaulted = vec![false; count];
        for &slot in &call.binding.default_slots {
            let entry = defaulted.get_mut(slot as usize).ok_or_else(|| RuntimeError::new("indexed-ir", "native default slot is invalid").with_span(span))?;
            if *entry { return Err(RuntimeError::new("indexed-ir", "native default slot is repeated").with_span(span)); }
            *entry = true;
        }
        let encoded_count = arguments.values.len() + arguments.omitted_parameters.len();
        for slot in arguments.omitted_parameters {
            let entry = omitted.get_mut(slot).ok_or_else(|| RuntimeError::new("indexed-ir", "native omission has an invalid parameter position").with_span(span))?;
            if *entry || !defaulted[slot] {
                return Err(RuntimeError::new("indexed-ir", "native omission changes its prepared default mask").with_span(span));
            }
            *entry = true;
        }
        let mut values = arguments.values.into_iter();
        let mut packet = Vec::with_capacity(count);
        for (slot, source) in call.argument_sources.iter().enumerate() {
            if source.is_some() {
                if omitted[slot] { return Err(RuntimeError::new("indexed-ir", "native supplied argument is marked omitted").with_span(span)); }
                packet.push(Some(values.next().ok_or_else(|| RuntimeError::new("indexed-ir", "native supplied argument is missing").with_span(span))?));
            } else {
                if !defaulted[slot] || (slot < encoded_count && !omitted[slot]) {
                    return Err(RuntimeError::new("indexed-ir", "native packet omits a required argument").with_span(span));
                }
                packet.push(None);
            }
        }
        if values.next().is_some() {
            return Err(RuntimeError::new("indexed-ir", "native packet contains excess supplied arguments").with_span(span));
        }
        self.eval_indexed_module_call_values(operation, super::super::NativeArgumentValues::new(packet), span, None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::Checker;
    use crate::syntax::parser::Parser;
    use crate::source::SourceMap;

    #[test]
    fn native_alias_named_packets_preserve_evaluation_and_defaults_on_both_routes() {
        crate::runtime::eval::run_eval(|| {
            let source = r#"proc flag() [io] -> Bool { print "flag"; false }
proc integer() [io] -> Int { print "value"; 42 }
proc caller() [io,error] -> Unit {
    let encode = json.encode
    print ${encode(pretty: flag(), value: integer())?}
    print ${encode(value: integer())?}
}
caller()
"#;
            for recursive in [false, true] {
                let mut sources = SourceMap::new();
                let source_id = sources.add_file("native-alias-packets.xsh", source);
                let parsed = Parser::parse_source_arena_only(source_id, source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let checked = Checker::check_arena(&parsed.arena, source);
                assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                let weak = Arc::downgrade(&checked.solved);
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
                let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked)
                    .expect("native packet uses its original checked authority");
                drop(checked);
                drop(parsed);
                assert!(weak.upgrade().is_none());
                let symbols = evaluator.indexed_program.as_ref().unwrap().symbol_owner().clone();
                let run = || symbols.with_current(|| {
                    assert_eq!(crate::runtime::eval::lowered_run::recursive_fast_path_forced(), recursive);
                    evaluator.try_eval_installed_compact_indexed_only_inner(plan)
                        .unwrap_or_else(|_| panic!("prepared native program remains installed"))
                });
                let output = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(run) } else { run() };
                assert_eq!(output.status, 0, "{:?}; {:?}", output.diagnostics, output.traceback);
                assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                assert_eq!(output.stdout, b"flag\nvalue\n42\nvalue\n42\n", "recursive={recursive}");
            }
        });
    }
}
