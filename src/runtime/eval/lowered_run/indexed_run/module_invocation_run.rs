use super::*;
use crate::runtime::eval::indexed::generic::ModuleExportContract;

/// The selected export retains both programs while supplied expressions run.
/// Its implementation cannot be replaced by a later dynamic table lookup.
#[derive(Clone)]
pub(super) struct PreparedModuleCallable {
    caller: Arc<FullProgram>,
    program: Arc<FullProgram>,
    function: LoweredFunctionKey,
    kind: LoweredFunctionKind,
    contract: ModuleExportContract,
}

impl Evaluator {
    pub(super) fn checked_indexed_module_callable(&self, execution: &FullExecution<'_>, instruction: u32, receiver: &LoweredValue, span: Span) -> Result<PreparedModuleCallable, RuntimeError> {
        let invalid = |message| RuntimeError::new("indexed-ir", message).with_span(span);
        let evidence = execution.generic_evidence().ok_or_else(|| invalid("module invocation lacks prepared evidence"))?;
        let source = execution.module_invocation_source(instruction).map_err(|error| indexed_error(error, span))?.ok_or_else(|| invalid("module invocation lacks its original export source"))?;
        let caller = Arc::clone(self.indexed_program.as_ref().ok_or_else(|| invalid("module invocation has no installed caller program"))?);
        if !std::ptr::eq(caller.generic_evidence().ok_or_else(|| invalid("module invocation caller lacks evidence"))?, evidence) {
            return Err(invalid("module invocation uses another program's export source"));
        }
        let LoweredValue::Module(fields) = receiver else { return Err(invalid("module invocation requires its actual loaded module receiver")); };
        let callee = fields.get(source.field.as_ref()).ok_or_else(|| invalid("module invocation lost its original export field"))?;
        let (key, kind) = indexed_callable_identity(callee, span)?;
        let LoweredFunctionKey::Qualified(qualified) = key else { return Err(invalid("module export callable lost its loaded namespace")); };
        if qualified.member.as_str().as_str() != source.field.as_ref() {
            return Err(invalid("module invocation selected another export member"));
        }
        let namespace = qualified.namespace.as_str();
        let module_key = namespace.as_str().strip_prefix("dynamic:").ok_or_else(|| invalid("module export callable has no loaded module owner"))?;
        let original_module = self.module_value_cache.get(module_key).ok_or_else(|| invalid("module export callable has no original published module"))?;
        if receiver.clone().into_value() != Value::Module(original_module.clone()) {
            return Err(invalid("module invocation receiver changes its original published export record"));
        }
        let dynamic = self.indexed_dynamic_functions.get(&qualified).ok_or_else(|| invalid("module export callable has no published implementation"))?;
        if dynamic.kind != kind || (kind == LoweredFunctionKind::Pure) != (source.contract.kind == crate::runtime::eval::indexed::generic::CallableKind::Pure) {
            return Err(invalid("module export callable changes its original kind"));
        }
        let view = dynamic.program.function_view(dynamic.function, dynamic.kind).map_err(|error| indexed_error(error, span))?.ok_or_else(|| invalid("module export implementation is absent from its published program"))?;
        let actual = view.module_export_contract().map_err(|error| indexed_error(error, span))?;
        if actual != source.contract { return Err(invalid("module export implementation differs from the original export signature or effects")); }
        Ok(PreparedModuleCallable { caller, program: Arc::clone(&dynamic.program), function: dynamic.function, kind, contract: source.contract.clone() })
    }

    pub(super) fn eval_indexed_module_callable(&mut self, callable: PreparedModuleCallable, arguments: IndexedCallArguments, span: Span) -> Result<LoweredValue, RuntimeError> {
        let invalid = |message| RuntimeError::new("indexed-ir", message).with_span(span);
        if !self.indexed_program.as_ref().is_some_and(|program| Arc::ptr_eq(program, &callable.caller)) {
            return Err(invalid("module invocation lost its original caller program"));
        }
        if !arguments.omitted_parameters.is_empty() || arguments.values.len() != callable.contract.parameters.len() {
            return Err(invalid("module invocation operands change the prepared supplied shape"));
        }
        let actual = callable.program.function_view(callable.function, callable.kind).map_err(|error| indexed_error(error, span))?.ok_or_else(|| invalid("module invocation lost its pinned implementation"))?.module_export_contract().map_err(|error| indexed_error(error, span))?;
        if actual != callable.contract { return Err(invalid("module invocation pinned implementation contract changed")); }
        let previous = self.indexed_program.replace(callable.program);
        let result = self.eval_indexed_named_call_with_arguments(callable.function, arguments, span, None);
        self.indexed_program = previous;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::eval::indexed::generic::InstructionOwner;
    use crate::sema::check::Checker;
    use crate::source::SourceMap;
    use crate::syntax::parser::Parser;

    #[test]
    fn original_module_invocation_runtime_refuses_record_callee_and_linked_program_substitution() {
        struct ModuleFiles(std::path::PathBuf);
        impl Drop for ModuleFiles {
            fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
        }
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let files = ModuleFiles(std::env::temp_dir().join(format!("xsh-module-call-link-{}-{stamp}", std::process::id())));
        std::fs::create_dir_all(&files.0).unwrap();
        let leaf = files.0.join("leaf.xsh");
        let other = files.0.join("other.xsh");
        std::fs::write(&leaf, "##! A loaded text renderer.\n## Return the supplied text.\nexport pure render(value: Str) -> Str { value }\n").unwrap();
        std::fs::write(&other, "##! A loaded integer renderer.\n## Return the supplied integer.\nexport pure render(value: Int) -> Int { value }\n").unwrap();
        let source = format!(r#"type Plugin = module {{ export pure render(value: Str) -> Str }}
let other = module.load(p"{}")?
proc caller() [fs, error] -> Result[Str] {{
  let plugin = module.load(p"{}")?.require(Plugin)?
  plugin.render("hi")
}}
print ${{caller()?}}
"#, other.display(), leaf.display());
        crate::runtime::eval::run_eval(|| {
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("module-runtime-link.xsh", source.clone());
            let parsed = Parser::parse_source_arena_only(source_id, &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let solved = Arc::downgrade(&checked.solved);
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
            let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
            drop(parsed); drop(checked);
            assert!(solved.upgrade().is_none());
            let program = Arc::clone(evaluator.indexed_program.as_ref().unwrap());
            program.symbol_owner().with_current(|| {
                for (index, step) in plan.statements.iter().enumerate() {
                    let flow = evaluator.eval_indexed_driver_step(index, step.span).expect("the actual prepared driver remains installed").unwrap();
                    assert!(matches!(flow, None | Some(Flow::Continue(_))), "the module fixture's top-level statements do not transfer control: {flow:?}");
                }
                assert_eq!(evaluator.stdout, b"hi\n");
                assert!(evaluator.stderr.is_empty(), "{:?}", evaluator.stderr);
                let source = program.generic_evidence().unwrap().module_invocation_sources().next().unwrap();
                let InstructionOwner::Function(owner) = source.owner else { panic!("the actual caller owns the module invocation"); };
                let execution = program.function_view_by_id(owner).unwrap().execution().unwrap();
                let span = Span::new(source_id, 0, 0);
                let module = evaluator.module_value_cache.get(&crate::loader::module_key(&leaf)).unwrap().clone();
                let receiver = lowered_value_from_runtime_any(&Value::Module(module)).unwrap();
                let handle = evaluator.checked_indexed_module_callable(&execution, source.instruction, &receiver, span).unwrap();
                let LoweredValue::Module(fields) = &receiver else { panic!("the authentic module value remains typed"); };
                let record = LoweredValue::Record(Arc::clone(fields));
                assert!(evaluator.checked_indexed_module_callable(&execution, source.instruction, &record, span).is_err(), "an equal record cannot replace an actual loaded module");
                let (_, qualified) = evaluator.indexed_dynamic_functions.iter().find_map(|(qualified, dynamic)| (dynamic.kind == LoweredFunctionKind::Pure && qualified.namespace.as_str().as_str().contains("other.xsh")).then_some((dynamic, *qualified))).unwrap();
                let mut changed_fields = fields.as_ref().clone();
                changed_fields.insert(Arc::from("render"), LoweredValue::Pure(FunctionName::qualified(qualified)));
                let changed = LoweredValue::Module(Arc::new(changed_fields));
                assert!(evaluator.checked_indexed_module_callable(&execution, source.instruction, &changed, span).is_err(), "another loaded module's function cannot replace one field in the original export record");
                let original = evaluator.indexed_dynamic_functions.iter().find_map(|(qualified, dynamic)| qualified.namespace.as_str().as_str().contains("leaf.xsh").then_some((*qualified, dynamic.clone()))).unwrap();
                let other_program = Arc::clone(&evaluator.indexed_dynamic_functions[&qualified].program);
                Arc::make_mut(&mut evaluator.indexed_dynamic_functions).get_mut(&original.0).unwrap().program = other_program;
                assert!(evaluator.checked_indexed_module_callable(&execution, source.instruction, &receiver, span).is_err(), "a different program's implementation cannot replace the published export signature");
                let result = evaluator.eval_indexed_module_callable(handle, IndexedCallArguments::supplied(vec![LoweredValue::Str(Arc::from("pinned"))]), span).unwrap();
                assert!(matches!(result, LoweredValue::Str(value) if value.as_ref() == "pinned"), "supplied evaluation cannot replace an already selected implementation through the dynamic table");
            });
        });
    }
}
