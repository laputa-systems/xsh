use super::*;
use crate::runtime::value::RecordShape;

fn invoke_host(value: Value) -> Result<BTreeMap<Arc<str>, LoweredValue>, RuntimeError> {
    let span = Span::new(crate::source::SourceId::new(0), 0, 1);
    let mut evaluator = Evaluator::new_with_sources(Vec::new(), crate::source::SourceMap::new())
        .with_native_test_host(Arc::new(move |_| Ok(value.clone())));
    let ctx = RecordMap::from([(Arc::from("temp_root"), Value::Path(PathValue::new(b"/tmp".to_vec()).unwrap()))]);
    evaluator.lowered_native_test_run(NativeTestRunKind::Xsh, &ctx, "", &[], &[], &BTreeMap::new(), &[], "host.xsh", span, "test-run-xsh")
}

#[test]
fn native_test_host_record_retains_shaped_fields_and_extra_nested_payloads() {
    crate::symbol::SymbolOwner::new().with_current(|| {
        let nested = RecordMap::from_name_values(vec![(Name::intern("detail"), Value::Str(Arc::from("kept")))]);
        let values = vec![Value::Bool(false), Value::Str(Arc::from("checker diagnostic")), Value::Bytes(b"output".to_vec()), Value::Record(nested.clone())];
        let shape = RecordShape::new(vec![Arc::from("success"), Arc::from("stderr"), Arc::from("stdout_bytes"), Arc::from("extra")]);
        let record = RecordMap::shaped(&shape, values);
        let output = invoke_host(Value::Record(record)).unwrap();
        assert_eq!(output.len(), 4);
        assert_eq!(output["success"], LoweredValue::Bool(false));
        assert_eq!(output["stderr"], LoweredValue::Str(Arc::from("checker diagnostic")));
        assert_eq!(output["stdout_bytes"].clone().into_value(), Value::Bytes(b"output".to_vec()));
        assert_eq!(output["extra"].clone().into_value(), Value::Record(nested));
    });
}

#[test]
fn native_test_host_record_retains_sparse_defaults_and_rejects_nonrecord_carriers() {
    crate::symbol::SymbolOwner::new().with_current(|| {
        static DEFAULTS: [Value; 2] = [Value::Bool(true), Value::Int(0)];
        let shape = RecordShape::new(vec![Arc::from("success"), Arc::from("exit_code")]);
        let sparse = RecordMap::sparse_shaped_indices(&shape, &DEFAULTS, vec![(1, Value::Int(7))]);
        let output = invoke_host(Value::Record(sparse)).unwrap();
        assert_eq!(output.len(), 2);
        assert_eq!(output["success"], LoweredValue::Bool(true));
        assert_eq!(output["exit_code"], LoweredValue::Int(7));
        for value in [Value::Int(7), Value::Map(BTreeMap::new()), Value::Module(RecordMap::new()), Value::ok(Value::Record(RecordMap::new()))] {
            let error = invoke_host(value).unwrap_err();
            assert_eq!(error.kind, "test-run-xsh");
            assert!(error.message.contains("expected Record"));
        }
    });
}
