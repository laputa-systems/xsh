use super::lowered_ops::lowered_value_from_runtime_any;
use super::type_view::{value_matches_static_type, ValueView};
use super::{MapKey, ResultValue, Type, Value};
use crate::map_key::MapKeyRef;
use crate::runtime::value::{DigestValue, DurationValue, FloatValue, PathValue, RecordMap, RegexValue, RunError, RuntimeError};
use crate::symbol::{Name, SymbolOwner};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader};
use std::sync::Arc;

/// Types a map key or set element may have (SPEC 4.5), plus `Any`; a key view
/// is only checked against these.
const KEY_TYPES: [&str; 8] = ["any", "bool", "int", "uint", "str", "bytes", "duration", "path"];
const KEY_ROWS: usize = 56;

#[test]
#[ignore = "manual Lean-generated production/model differential check"]
fn production_matches_lean_membership() {
    SymbolOwner::new().with_current(|| {
        let x = Name::intern("x");
        let y = Name::intern("y");
        let types = vec![
            ("any", Type::Any), ("null", Type::Null), ("bool", Type::Bool),
            ("int", Type::Int), ("uint", Type::UInt), ("float", Type::Float),
            ("str", Type::Str), ("bytes", Type::Bytes), ("duration", Type::Duration),
            ("digest", Type::Digest), ("regex", Type::Regex), ("path", Type::Path),
            ("unit", Type::Unit), ("list-int", Type::List(Box::new(Type::Int))),
            ("nonempty-int", Type::non_empty(Type::Int)),
            ("map-str-int", Type::Map(Box::new(Type::Str), Box::new(Type::Int))),
            ("map-int-int", Type::Map(Box::new(Type::Int), Box::new(Type::Int))),
            ("set-int", Type::Set(Box::new(Type::Int))),
            ("optional-int", Type::Optional(Box::new(Type::Int))),
            ("record-x-int", Type::Record(BTreeMap::from([(x, Type::Int)]).into())),
            ("union-int-str", Type::Union(vec![Type::Int, Type::Str])),
            ("result-int-str", Type::Result(Box::new(Type::Int), Box::new(Type::Str))),
            ("list-optional-int", Type::List(Box::new(Type::Optional(Box::new(Type::Int))))),
            ("map-str-record", Type::Map(Box::new(Type::Str), Box::new(Type::Record(BTreeMap::from([(x, Type::Int)]).into())))),
            ("union-record-bool", Type::Union(vec![Type::Record(BTreeMap::from([(x, Type::Int)]).into()), Type::Bool])),
            ("erased-record", Type::ErasedRecord),
            ("tag-a", Type::Tag(Name::intern("A"))), ("tag-b", Type::Tag(Name::intern("B"))),
            ("error", Type::Error), ("process-error", Type::ProcessError),
            ("family-a", Type::ErrorFamily(Name::intern("FamilyA"))),
            ("variant-a", Type::ErrorVariant { family: Name::intern("FamilyA"), variant: Name::intern("Bad") }),
            ("facet-io", Type::ErrorFacet(Name::intern("io"))),
        ];
        let key = || MapKey::Str(Arc::from("a"));
        let string = || Value::Str(Arc::from("a"));
        let values = vec![
            ("null", Value::Null), ("bool", Value::Bool(true)), ("negative", Value::Int(-1)),
            ("zero", Value::Int(0)), ("float", Value::Float(FloatValue::new(0.0))),
            ("str", string()), ("bytes", Value::Bytes(vec![1])),
            ("duration", Value::Duration(DurationValue { millis: 1 })),
            ("digest", Value::digest(DigestValue { algorithm: "sha256".into(), bytes: vec![1] })),
            ("regex", Value::Regex(RegexValue { pattern: "a".into(), regex: Arc::new(regex_lite::Regex::new("a").unwrap()) })),
            ("path", Value::Path(PathValue::new(b"a".to_vec()).unwrap())), ("unit", Value::Unit),
            ("list-empty", Value::List(vec![])), ("list-int", Value::List(vec![Value::Int(1)])),
            ("list-wrong", Value::List(vec![string()])), ("map-empty", Value::Map(BTreeMap::new())),
            ("map-str-int", Value::Map(BTreeMap::from([(key(), Value::Int(1))]))),
            ("map-int-int", Value::Map(BTreeMap::from([(MapKey::Int(1), Value::Int(1))]))),
            ("map-str-wrong", Value::Map(BTreeMap::from([(key(), string())]))),
            ("set-empty", Value::Set(BTreeSet::new())),
            ("set-int", Value::Set(BTreeSet::from([MapKey::Int(1)]))),
            ("set-str", Value::Set(BTreeSet::from([key()]))),
            ("record-empty", Value::Record(RecordMap::from_name_values(vec![]))),
            ("record-x", Value::Record(RecordMap::from_name_values(vec![(x, Value::Int(1))]))),
            ("record-wrong", Value::Record(RecordMap::from_name_values(vec![(x, string())]))),
            ("record-extra", Value::Record(RecordMap::from_name_values(vec![(x, Value::Int(1)), (y, Value::Int(1))]))),
            ("ok-int", Value::Result(ResultValue::Ok(Box::new(Value::Int(1))))),
            ("ok-str", Value::Result(ResultValue::Ok(Box::new(string())))),
            ("err-str", Value::Result(ResultValue::Err(Box::new(string())))),
            ("err-int", Value::Result(ResultValue::Err(Box::new(Value::Int(1))))),
            ("list-optional", Value::List(vec![Value::Null, Value::Int(1)])),
            ("list-optional-wrong", Value::List(vec![Value::Null, string()])),
            ("map-record", Value::Map(BTreeMap::from([(key(), Value::Record(RecordMap::from_name_values(vec![(x, Value::Int(1))])))]))),
            ("map-record-wrong", Value::Map(BTreeMap::from([(key(), Value::Record(RecordMap::from_name_values(vec![(x, string())])))]))),
            ("tag-a-empty", Value::Tag { type_name: Name::intern("A"), name: Arc::from("One"), fields: vec![], wire: None }),
            ("tag-a-fields", Value::Tag { type_name: Name::intern("A"), name: Arc::from("Two"), fields: vec![Value::Int(1)], wire: None }),
            ("tag-b-empty", Value::Tag { type_name: Name::intern("B"), name: Arc::from("One"), fields: vec![], wire: None }),
            ("error-a", Value::Error(Box::new(RuntimeError::structured("FamilyA", "Bad", RecordMap::new(), vec!["io".into()], "bad")))),
            ("error-a-other", Value::Error(Box::new(RuntimeError::structured("FamilyA", "Other", RecordMap::new(), vec![], "other")))),
            ("error-b", Value::Error(Box::new(RuntimeError::structured("FamilyB", "Bad", RecordMap::new(), vec![], "bad")))),
            ("run-error", Value::RunError(Box::new(RunError::new("failed", "failed")))),
        ];
        let lowered: Vec<_> = values.iter().map(|(_, value)| lowered_value_from_runtime_any(value).expect("fixture cannot be lowered")).collect();
        assert_eq!(types.len(), 33);
        assert_eq!(values.len(), 41);
        let path = std::env::var_os("XSH_MEMBERSHIP_CASES").expect("XSH_MEMBERSHIP_CASES is required");
        let mut lines = BufReader::new(std::fs::File::open(path).expect("cannot open membership corpus")).lines();
        assert_eq!(lines.next().expect("missing header").expect("invalid header"), "membership-v1");
        let mut seen = BTreeSet::new();
        let mut count = 0;
        let mut key_rows = 0;
        for line in lines {
            let line = line.expect("invalid corpus input");
            assert!(count < 1353, "corpus exceeds bound");
            let fields: Vec<_> = line.split(' ').collect();
            assert_eq!(fields.len(), 3, "malformed row {}: {line}", count + 2);
            let ty = types.iter().find(|(name, _)| *name == fields[0]).expect("out-of-domain type");
            let value = values.iter().find(|(name, _)| *name == fields[1]).expect("out-of-domain value");
            let expected = match fields[2] { "0" => false, "1" => true, _ => panic!("invalid decision") };
            assert!(seen.insert((fields[0].to_owned(), fields[1].to_owned())), "duplicate input at row {}: {line}", count + 2);
            assert_eq!(value_matches_static_type(ValueView::Runtime(&value.1), &ty.1), expected,
                "model/source disagreement at row {}: {line}", count + 2);
            let index = values.iter().position(|(name, _)| *name == fields[1]).unwrap();
            assert_eq!(value_matches_static_type(ValueView::Lowered(&lowered[index]), &ty.1), expected,
                "model/source disagreement in lowered view at row {}: {line}", count + 2);
            if KEY_TYPES.contains(&fields[0])
                && let Some(key) = MapKeyRef::from_value(&value.1)
            {
                assert_eq!(value_matches_static_type(ValueView::Key(key), &ty.1), expected,
                    "model/source disagreement in key view at row {}: {line}", count + 2);
                key_rows += 1;
            }
            count += 1;
        }
        assert_eq!(count, 1353, "incomplete corpus");
        assert_eq!(key_rows, KEY_ROWS, "key view rows changed");
        println!("membership rows: {count}");
    });
}
