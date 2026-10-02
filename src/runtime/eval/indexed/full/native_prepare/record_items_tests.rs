use super::*;
use crate::runtime::eval::Evaluator;
use crate::runtime::value::{PathValue, Value};
use crate::sema::check::Checker;

const SOURCE: &str = "proc observed(target: Path) [fs, error] -> Map[Str, Int] { fs.files(target, gitignore: false, stat: false) |> count { |entry| entry.ext } }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n";

fn fixture() -> FullProgram {
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        "original-native-record-items.xsh", crate::loader::entry_source_from_text("original-native-record-items.xsh", SOURCE.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let weak = Arc::downgrade(&bodies.solved);
    let counters = bodies.solved.graph.counters().clone();
    let program = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, SOURCE, Arc::new(sources), source_id).unwrap();
    assert_eq!(&counters, bodies.solved.graph.counters());
    drop(parsed); drop(declarations); drop(bodies);
    assert!(weak.upgrade().is_none());
    program
}

#[test]
fn original_native_record_items_project_lazy_filesystem_entries_after_frontend_disposal_on_both_routes() {
    crate::runtime::eval::run_eval(|| {
        use std::os::unix::ffi::OsStrExt;
        let program = Arc::new(fixture());
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        std::fs::write(root.join("first.RS"), "first").unwrap();
        std::fs::write(root.join("second.rs"), "second").unwrap();
        program.symbol_owner().with_current(|| {
            FullVerifier::verify(&program).unwrap();
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let key = LoweredFunctionKey::Name(Name::intern("observed"));
                let arguments = [Value::Path(PathValue::new(root.as_os_str().as_bytes().to_vec()).unwrap())];
                let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Proc, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                let actual = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap();
                assert_eq!(actual, Value::Map(BTreeMap::from([
                    (crate::map_key::MapKey::Str(Arc::from("RS")), Value::Int(1)),
                    (crate::map_key::MapKey::Str(Arc::from("rs")), Value::Int(1)),
                ])));
            }
        });
    });
}

#[test]
fn original_native_record_items_refuse_missing_changed_carrier_row_schema_and_foreign_sources() {
    crate::runtime::eval::run_eval(|| {
        use super::super::super::generic::{PreparedNativeRecordCarrier, PreparedOperationAuthority};
        let program = fixture();
        let foreign = fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, source) = generic.native_call_sources().find(|(_, source)| matches!(source.expected.authority,
                PreparedOperationAuthority::Registry { operation: RuntimeOp::FsFiles, .. })).unwrap();
            let layout = source.result_record_layout.as_ref().unwrap();
            assert_eq!(layout.carrier, PreparedNativeRecordCarrier::ResultStream);
            source.verify_result_record_layout(&program.store.semantic).unwrap();
            let mut missing = source.clone();
            missing.result_record_layout = None;
            assert!(missing.verify_result_record_layout(&program.store.semantic).is_err());
            let mut carrier = source.clone();
            carrier.result_record_layout.as_mut().unwrap().carrier = PreparedNativeRecordCarrier::ResultList;
            assert!(carrier.verify_result_record_layout(&program.store.semantic).is_err());
            let mut row = source.clone();
            let TypeRef::Ground(result) = source.expected.result else { unreachable!() };
            row.result_record_layout.as_mut().unwrap().record = result;
            assert!(row.verify_result_record_layout(&program.store.semantic).is_err());
            let mut schema = source.clone();
            let crate::runtime::eval::require::PreparedSchema::Record(fields) = Arc::make_mut(&mut schema.result_record_layout.as_mut().unwrap().schema) else { unreachable!() };
            fields.reverse();
            assert!(schema.verify_result_record_layout(&program.store.semantic).is_err());
            let mut shape = source.clone();
            shape.result_record_layout.as_mut().unwrap().shape = crate::runtime::value::RecordShape::new(layout.shape.field_names().iter().rev().map(|name| Arc::from(name.as_str().as_str())).collect());
            assert!(shape.verify_result_record_layout(&program.store.semantic).is_err());
            let mut public = program.store.clone();
            public.generic.as_deref_mut().unwrap().test_native_call_source_mut(id).unwrap().result_record_layout = carrier.result_record_layout;
            assert!(FullVerifier::verify_generic_evidence(&public).is_err());
            let foreign_id = foreign.generic_evidence().unwrap().native_call_sources().find(|(_, source)| matches!(source.expected.authority,
                PreparedOperationAuthority::Registry { operation: RuntimeOp::FsFiles, .. })).unwrap().0;
            assert!(generic.native_call_source(foreign_id).is_err());
            let evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            let span = Span::new(program.store.source_id, 0, 0);
            assert!(layout.materialize_result(&evaluator, LoweredValue::ResultOk(Box::new(LoweredValue::List(vec![LoweredValue::Bool(true)]))), span).is_err());
            let error = LoweredValue::ResultErr(Box::new(crate::runtime::value::error_constructor("fs", "kept")));
            assert_eq!(layout.materialize_result(&evaluator, error.clone(), span).unwrap(), error);
            assert!(layout.retained_bytes() > std::mem::size_of::<crate::runtime::eval::require::PreparedSchema>());
        });
    });
}

#[test]
fn original_native_record_items_keep_live_pulls_metadata_errors_cancellation_and_logical_equality() {
    crate::runtime::eval::run_eval(|| {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use crate::runtime::value::{FsEntryValue, LiveStream, StreamValue};
        use super::super::super::generic::PreparedOperationAuthority;
        struct Source { pulls: Arc<AtomicUsize>, stopped: Arc<AtomicBool>, value: Option<Value> }
        impl LiveStream for Source {
            fn next(&mut self, _span: Span) -> Result<Option<Value>, crate::runtime::value::RuntimeError> {
                self.pulls.fetch_add(1, Ordering::Relaxed);
                Ok(self.value.take())
            }
        }
        impl Drop for Source { fn drop(&mut self) { self.stopped.store(true, Ordering::Relaxed); } }
        let program = fixture();
        program.symbol_owner().with_current(|| {
            let (_, source) = program.generic_evidence().unwrap().native_call_sources().find(|(_, source)| matches!(source.expected.authority,
                PreparedOperationAuthority::Registry { operation: RuntimeOp::FsFiles, .. })).unwrap();
            let layout = source.result_record_layout.as_ref().unwrap();
            let pulls = Arc::new(AtomicUsize::new(0));
            let stopped = Arc::new(AtomicBool::new(false));
            let original = FsEntryValue::new(std::path::PathBuf::from("/native/item.RS"), std::fs::metadata(std::env::current_exe().unwrap()).unwrap().file_type());
            let native = StreamValue::from_live("original-record-items", Source { pulls: Arc::clone(&pulls), stopped: Arc::clone(&stopped), value: Some(Value::FsEntry(original.clone())) });
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            let span = Span::new(program.store.source_id, 0, 0);
            let lowered = layout.materialize_result(&evaluator, LoweredValue::ResultOk(Box::new(LoweredValue::Stream(Box::new(native)))), span).unwrap();
            assert_eq!(pulls.load(Ordering::Relaxed), 0);
            let LoweredValue::ResultOk(payload) = lowered else { unreachable!() };
            let LoweredValue::Stream(mut stream) = *payload else { unreachable!() };
            let Value::FsEntry(shaped) = evaluator.stream_next(&mut stream, span).unwrap().unwrap() else { panic!("the prepared item keeps its native lazy representation") };
            assert_eq!(pulls.load(Ordering::Relaxed), 1);
            assert_eq!(shaped, original, "physical shape does not change logical filesystem entry equality");
            let ext = layout.shape.field_names().iter().position(|name| *name == "ext").unwrap() as u32;
            assert_eq!(shaped.prepared_field_value(ext).unwrap().unwrap(), Value::Str(Arc::from("RS")));
            let size = layout.shape.field_names().iter().position(|name| *name == "size").unwrap() as u32;
            assert_eq!(shaped.prepared_field_value(size).unwrap().unwrap_err().kind, "metadata-unavailable");
            evaluator.stream_cancel(&mut stream, span).unwrap();
            assert!(stopped.load(Ordering::Relaxed));
            assert_eq!(pulls.load(Ordering::Relaxed), 1, "cancellation does not drain the native source");
            let buffered = StreamValue::from_values(vec![Value::FsEntry(original)]);
            let lowered = layout.materialize_result(&evaluator, LoweredValue::ResultOk(Box::new(LoweredValue::Stream(Box::new(buffered)))), span).unwrap();
            let LoweredValue::ResultOk(payload) = lowered else { unreachable!() };
            let LoweredValue::Stream(mut stream) = *payload else { unreachable!() };
            let Value::FsEntry(buffered) = evaluator.stream_next(&mut stream, span).unwrap().unwrap() else { unreachable!() };
            assert_eq!(buffered.prepared_field_value(ext).unwrap().unwrap(), Value::Str(Arc::from("RS")));
            assert!(evaluator.stream_next(&mut stream, span).unwrap().is_none());
        });
    });
}
