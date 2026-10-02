use super::*;
use std::collections::BTreeMap;

#[test]
fn validated_record_layout_survives_nested_host_containers_and_rejects_invalid_fields() {
    let symbols = crate::symbol::SymbolOwner::new();
    symbols.with_current(|| {
        let payload = Name::intern("payload");
        let required = Name::intern("required");
        let schema = PreparedSchema::compile(&Type::Record(BTreeMap::from([
            (payload, Type::List(Box::new(Type::Record(BTreeMap::from([(required, Type::Int)]))))),
            (required, Type::Int),
        ])), &PreparedWireEnums::default());
        let raw = LoweredValue::Record(Arc::new(BTreeMap::from([
            (Arc::from("aardvark"), LoweredValue::Bool(true)),
            (Arc::from("required"), LoweredValue::Int(7)),
            (Arc::from("payload"), LoweredValue::List(vec![LoweredValue::Record(Arc::new(BTreeMap::from([
                (Arc::from("aardvark"), LoweredValue::Str(Arc::from("extra"))),
                (Arc::from("required"), LoweredValue::Int(9)),
            ])))])),
        ])));
        let evaluator = Evaluator::new(Vec::new());
        let span = Span::new(crate::source::SourceId::new(0), 0, 0);
        let decoded = schema.decode(&evaluator, raw, "$", span).unwrap();
        let host = LoweredValue::ResultOk(Box::new(decoded.clone())).into_value();
        let restored = super::super::lowered_ops::lowered_value_from_runtime_any(&host).unwrap();
        assert_eq!(restored, LoweredValue::ResultOk(Box::new(decoded.clone())), "host conversion must retain physical field order recursively");
        let LoweredValue::ResultOk(restored) = restored else { unreachable!() };
        let LoweredValue::RecordVec(fields) = restored.as_ref() else { panic!("validated host record must retain numeric storage"); };
        assert_eq!(fields.last().unwrap().0.as_str().as_str(), "aardvark");
        let nested = fields.iter().find(|(name, _)| *name == payload).unwrap();
        let LoweredValue::List(items) = &nested.1 else { unreachable!() };
        let LoweredValue::RecordVec(nested) = &items[0] else { panic!("nested host record must retain numeric storage"); };
        assert_eq!(nested[0], (required, LoweredValue::Int(9)));
        assert_eq!(nested[1].0.as_str().as_str(), "aardvark");
        let invalid = LoweredValue::Record(Arc::new(BTreeMap::from([
            (Arc::from("required"), LoweredValue::Int(7)),
            (Arc::from("payload"), LoweredValue::List(vec![LoweredValue::Record(Arc::new(BTreeMap::from([
                (Arc::from("required"), LoweredValue::Str(Arc::from("wrong"))),
            ])))])),
        ])));
        let failure = schema.decode(&evaluator, invalid, "$", span).unwrap_err();
        assert_eq!(failure.kind, "schema");
        assert!(failure.message.contains("payload[0].required"), "{}", failure.message);
        assert!(schema.decode(&evaluator, LoweredValue::Record(Arc::new(BTreeMap::new())), "$", span).unwrap_err().message.contains("missing required field"));
        let dynamic = super::super::Value::Record(crate::runtime::value::RecordMap::Dynamic(BTreeMap::from([
            (Arc::from("required"), super::super::Value::Int(7)),
        ])));
        assert!(matches!(super::super::lowered_ops::lowered_value_from_runtime_any(&dynamic), Some(LoweredValue::Record(_))), "a dynamic host record has no retained numeric layout");
    });
}
