use super::type_view::{value_matches_static_type, ValueView};
use super::{MapKey, Type, Value};
use crate::runtime::value::{DigestValue, DurationValue, FloatValue, PathValue, RecordMap, RegexValue};
use crate::symbol::{Name, SymbolOwner};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader};
use std::sync::Arc;

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
        ];
        assert_eq!(types.len(), 20);
        assert_eq!(values.len(), 26);
        let path = std::env::var_os("XSH_MEMBERSHIP_CASES").expect("XSH_MEMBERSHIP_CASES is required");
        let mut lines = BufReader::new(std::fs::File::open(path).expect("cannot open membership corpus")).lines();
        assert_eq!(lines.next().expect("missing header").expect("invalid header"), "membership-v1");
        let mut seen = BTreeSet::new();
        let mut count = 0;
        for line in lines {
            let line = line.expect("invalid corpus input");
            assert!(count < 520, "corpus exceeds bound");
            let fields: Vec<_> = line.split(' ').collect();
            assert_eq!(fields.len(), 3, "malformed row {}: {line}", count + 2);
            let ty = types.iter().find(|(name, _)| *name == fields[0]).expect("out-of-domain type");
            let value = values.iter().find(|(name, _)| *name == fields[1]).expect("out-of-domain value");
            let expected = match fields[2] { "0" => false, "1" => true, _ => panic!("invalid decision") };
            assert!(seen.insert((fields[0].to_owned(), fields[1].to_owned())), "duplicate input at row {}: {line}", count + 2);
            assert_eq!(value_matches_static_type(ValueView::Runtime(&value.1), &ty.1), expected,
                "model/source disagreement at row {}: {line}", count + 2);
            count += 1;
        }
        assert_eq!(count, 520, "incomplete corpus");
        println!("membership rows: {count}");
    });
}
