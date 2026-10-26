use super::type_view::ValueView;
use super::{LoweredValue, Value};
use crate::map_key::{MapKey, MapKeyRef};
use crate::runtime::value::{RecordIter, RuntimeError};
use crate::source::Span;
use crate::symbol::Name;
use std::collections::BTreeMap;
use std::borrow::Cow;
use deser::{Atom, Error, ErrorKind, Serialize, State};
use deser::ser::{Emit, SeqEmitter, SerializeHandle, StructEmitter};
use std::sync::Arc;

const NON_FINITE_FLOAT_MESSAGE: &str = "non-finite Float values are not JSON-compatible";
const NON_STRING_KEY_MESSAGE: &str = "JSON objects require Str map keys";

#[derive(Clone, Copy)]
enum JsonInput<'a> {
    Runtime(&'a Value),
    Lowered(&'a LoweredValue),
    Key(MapKeyRef<'a>),
    Path(&'a crate::runtime::value::PathValue),
    Integer(i64),
    EmptyObject,
    LoweredMap(&'a BTreeMap<MapKey, LoweredValue>),
}

enum JsonNode<'a> {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(&'a str),
    Wire(&'a str),
    MissingWireVariant,
    Sequence(JsonSequence<'a>),
    Map(JsonMap<'a>),
    Object(JsonFields<'a>),
    Stats(JsonStats<'a>),
    Key(MapKeyRef<'a>),
    Unsupported(JsonInput<'a>),
}

enum JsonSequence<'a> {
    Paths(std::slice::Iter<'a, crate::runtime::value::PathValue>),
    Runtime(std::slice::Iter<'a, Value>),
    Lowered(std::slice::Iter<'a, LoweredValue>),
    Set(std::collections::btree_set::Iter<'a, MapKey>),
}

enum JsonMap<'a> {
    Runtime(&'a BTreeMap<MapKey, Value>),
    Lowered(&'a BTreeMap<MapKey, LoweredValue>),
}

impl<'a> JsonMap<'a> {
    fn all_string_keys(&self) -> bool {
        match self {
            Self::Runtime(fields) => fields.keys().all(|key| key.as_str().is_some()),
            Self::Lowered(fields) => fields.keys().all(|key| key.as_str().is_some()),
        }
    }

    fn fields(self) -> JsonFields<'a> {
        match self {
            Self::Runtime(fields) => JsonFields::RuntimeMap(fields.iter()),
            Self::Lowered(fields) => JsonFields::LoweredMap(fields.iter()),
        }
    }
}

enum JsonFieldKey<'a> {
    Map(MapKeyRef<'a>),
    Name(Name),
    Str(&'a str),
}

enum JsonFields<'a> {
    Empty,
    RuntimeMap(std::collections::btree_map::Iter<'a, MapKey, Value>),
    LoweredMap(std::collections::btree_map::Iter<'a, MapKey, LoweredValue>),
    RuntimeRecord(RecordIter<'a>),
    LoweredRecord(std::collections::btree_map::Iter<'a, Arc<str>, LoweredValue>),
    RecordVec(std::vec::IntoIter<(&'a Name, &'a LoweredValue)>),
}

impl<'a> Iterator for JsonFields<'a> {
    type Item = (JsonFieldKey<'a>, JsonInput<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Empty => None,
            Self::RuntimeMap(fields) => fields.next().map(|(key, value)| (JsonFieldKey::Map(key.as_ref()), JsonInput::Runtime(value))),
            Self::LoweredMap(fields) => fields.next().map(|(key, value)| (JsonFieldKey::Map(key.as_ref()), JsonInput::Lowered(value))),
            Self::RuntimeRecord(fields) => fields.next().map(|(key, value)| (JsonFieldKey::Str(key), JsonInput::Runtime(value))),
            Self::LoweredRecord(fields) => fields.next().map(|(key, value)| (JsonFieldKey::Str(key), JsonInput::Lowered(value))),
            Self::RecordVec(fields) => fields.next().map(|(key, value)| (JsonFieldKey::Name(*key), JsonInput::Lowered(value))),
        }
    }
}

struct JsonStats<'a> {
    blanks: i64,
    blobs: Option<&'a BTreeMap<MapKey, LoweredValue>>,
    code: i64,
    comments: i64,
}

fn project<'a>(value: JsonInput<'a>) -> JsonNode<'a> {
    match value {
        JsonInput::LoweredMap(value) => JsonNode::Map(JsonMap::Lowered(value)),
        JsonInput::Integer(value) => JsonNode::Int(value),
        JsonInput::EmptyObject => JsonNode::Object(JsonFields::Empty),
        JsonInput::Key(key) => JsonNode::Key(key),
        JsonInput::Path(path) => JsonNode::Unsupported(JsonInput::Path(path)),
        JsonInput::Runtime(value) => match value {
            Value::Null => JsonNode::Null,
            Value::Bool(value) => JsonNode::Bool(*value),
            Value::Int(value) => JsonNode::Int(*value),
            Value::Float(value) => JsonNode::Float(value.0),
            Value::Str(value) => JsonNode::Str(value),
            Value::EnvPathList(paths) => JsonNode::Sequence(JsonSequence::Paths(paths.entries().iter())),
            Value::List(items) => JsonNode::Sequence(JsonSequence::Runtime(items.iter())),
            Value::Set(items) => JsonNode::Sequence(JsonSequence::Set(items.iter())),
            Value::Map(fields) => JsonNode::Map(JsonMap::Runtime(fields)),
            Value::Record(fields) => JsonNode::Object(JsonFields::RuntimeRecord(fields.iter())),
            Value::Tag { type_name, name, fields, wire: Some(mapping) }
                if fields.is_empty() && *type_name == mapping.type_name =>
            {
                mapping.variants.get(&Name::intern(name.as_ref())).map_or(JsonNode::MissingWireVariant, |wire| JsonNode::Wire(wire))
            }
            _ => JsonNode::Unsupported(JsonInput::Runtime(value)),
        },
        JsonInput::Lowered(value) => match value {
            LoweredValue::Null => JsonNode::Null,
            LoweredValue::Bool(value) => JsonNode::Bool(*value),
            LoweredValue::Int(value) => JsonNode::Int(*value),
            LoweredValue::Float(value) => JsonNode::Float(value.0),
            LoweredValue::Str(value) => JsonNode::Str(value),
            LoweredValue::StrView(value) => JsonNode::Str(value.as_str()),
            LoweredValue::Tag(tag) if tag.wire.is_some() && tag.fields.is_empty() && tag.type_name == tag.wire.as_ref().expect("checked mapping").type_name => {
                tag.wire.as_ref().and_then(|mapping| mapping.variants.get(&Name::intern(tag.name.as_ref()))).map_or(JsonNode::MissingWireVariant, |wire| JsonNode::Wire(wire))
            }
            LoweredValue::EnvPathList(paths) => JsonNode::Sequence(JsonSequence::Paths(paths.entries().iter())),
            LoweredValue::List(items) => JsonNode::Sequence(JsonSequence::Lowered(items.iter())),
            LoweredValue::SharedList(items) => JsonNode::Sequence(JsonSequence::Lowered(items.iter())),
            LoweredValue::Set(items) => JsonNode::Sequence(JsonSequence::Set(items.iter())),
            LoweredValue::Map(fields) => JsonNode::Map(JsonMap::Lowered(fields)),
            LoweredValue::Record(fields) => JsonNode::Object(JsonFields::LoweredRecord(fields.iter())),
            LoweredValue::RecordVec(fields) => {
                let mut ordered: Vec<_> = fields.iter().map(|(key, value)| (key, value)).collect();
                ordered.sort_unstable_by_key(|(name, _)| *name);
                JsonNode::Object(JsonFields::RecordVec(ordered.into_iter()))
            }
            LoweredValue::Stats { blanks, code, comments } => JsonNode::Stats(JsonStats { blanks: *blanks, blobs: None, code: *code, comments: *comments }),
            LoweredValue::StatsBlob(stats) => JsonNode::Stats(JsonStats { blanks: stats.blanks, blobs: Some(&stats.blobs), code: stats.code, comments: stats.comments }),
            _ => JsonNode::Unsupported(JsonInput::Lowered(value)),
        },
    }
}

pub(super) fn encode_runtime(value: &Value, pretty: bool, span: Span) -> Result<String, RuntimeError> {
    encode_tree(JsonInput::Runtime(value), pretty, span)
}

pub(super) fn encode_lowered(value: &LoweredValue, pretty: bool, span: Span) -> Result<String, RuntimeError> {
    encode_tree(JsonInput::Lowered(value), pretty, span)
}

pub(super) fn encode_lowered_for_write(value: &LoweredValue, pretty: bool, span: Span) -> Result<String, RuntimeError> {
    encode_lowered(value, pretty, span)
}

pub(super) fn encode_lowered_lines_for_write(values: &LoweredValue, span: Span) -> Result<String, RuntimeError> {
    let items = match values {
        LoweredValue::List(items) => items.as_slice(),
        LoweredValue::SharedList(items) => items.as_slice(),
        other => return Err(RuntimeError::new("type-error", format!("expected List, found {}", ValueView::Lowered(other).type_name())).with_span(span)),
    };
    let mut output = String::new();
    for item in items {
        output.push_str(&encode_lowered(item, false, span)?);
        output.push('\n');
    }
    Ok(output)
}

fn encode_tree(value: JsonInput<'_>, pretty: bool, span: Span) -> Result<String, RuntimeError> {
    let mut config = deser_json::SerializerConfig::new();
    if pretty { config.set_pretty(deser_json::Indent::Spaces(2)); }
    let mut output = crate::modules::codec::Output::new();
    let result = config.writer(&mut output).write_with(&BorrowedValue { value, pretty, span }, |driver| {
        driver.push_layer(crate::modules::codec::EncodeLimits::default());
    });
    let output = output.finish(result)
        .map_err(|error| crate::modules::codec::runtime_error(error, "json", span))?;
    Ok(output)
}

struct BorrowedValue<'a> {
    value: JsonInput<'a>,
    pretty: bool,
    span: Span,
}

fn rejected(message: impl Into<Cow<'static, str>>) -> Error {
    Error::new(ErrorKind::UnsupportedType, message)
}

impl Serialize for BorrowedValue<'_> {
    fn serialize<'a>(value: &'a Self, state: &mut State) -> Result<Emit<'a>, Error> {
        if state.depth() > 128 { return Err(Error::new(ErrorKind::LimitExceeded, "codec nesting exceeds 128")); }
        Ok(match project(value.value) {
            JsonNode::Null => Emit::Atom(Atom::Null),
            JsonNode::Bool(v) => Emit::Atom(Atom::Bool(v)),
            JsonNode::Int(v) => Emit::Atom(Atom::I64(v)),
            JsonNode::Float(v) if v.is_finite() => Emit::Atom(Atom::F64(v)),
            JsonNode::Float(_) => return Err(rejected(NON_FINITE_FLOAT_MESSAGE)),
            JsonNode::Str(v) | JsonNode::Wire(v) => {
                if v.len() > 16 * 1024 * 1024 { return Err(Error::new(ErrorKind::LimitExceeded, "codec string exceeds 16 MiB")); }
                Emit::Atom(Atom::Str(v.into()))
            }
            JsonNode::MissingWireVariant => return Err(rejected("wire enum variant is absent from its declared mapping")),
            JsonNode::Sequence(items) => Emit::seq(Items { items, pretty: value.pretty, span: value.span }, state),
            JsonNode::Map(fields) => {
                if !value.pretty && !fields.all_string_keys() { return Err(rejected(NON_STRING_KEY_MESSAGE)); }
                Emit::structure(Fields { fields: fields.fields(), pretty: value.pretty, span: value.span }, state)
            }
            JsonNode::Object(fields) => Emit::structure(Fields { fields, pretty: value.pretty, span: value.span }, state),
            JsonNode::Stats(stats) => Emit::structure(StatsFields { stats, index: 0, pretty: value.pretty, span: value.span }, state),
            JsonNode::Key(MapKeyRef::Str(v)) => Emit::Atom(Atom::Str(v.into())),
            JsonNode::Key(MapKeyRef::Int(v)) => Emit::Atom(Atom::I64(v)),
            JsonNode::Key(MapKeyRef::Bool(v)) => Emit::Atom(Atom::Bool(v)),
            JsonNode::Key(key) => return Err(rejected(incompatible(JsonInput::Key(key), value.span).message)),
            JsonNode::Unsupported(v) => return Err(rejected(incompatible(v, value.span).message)),
        })
    }
}

struct Items<'a> { items: JsonSequence<'a>, pretty: bool, span: Span }
impl SeqEmitter for Items<'_> {
    fn next(&mut self, state: &mut State) -> Result<Option<SerializeHandle<'_>>, Error> {
        let value = match &mut self.items {
            JsonSequence::Paths(items) => items.next().map(JsonInput::Path),
            JsonSequence::Runtime(items) => items.next().map(JsonInput::Runtime),
            JsonSequence::Lowered(items) => items.next().map(JsonInput::Lowered),
            JsonSequence::Set(items) => items.next().map(|v| JsonInput::Key(v.as_ref())),
        };
        Ok(value.map(|value| SerializeHandle::arena(BorrowedValue { value, pretty: self.pretty, span: self.span }, state)))
    }
}

struct Fields<'a> { fields: JsonFields<'a>, pretty: bool, span: Span }
impl StructEmitter for Fields<'_> {
    fn next(&mut self, state: &mut State) -> Result<Option<(Cow<'_, str>, SerializeHandle<'_>)>, Error> {
        let Some((key, value)) = self.fields.next() else { return Ok(None); };
        let key = match key {
            JsonFieldKey::Str(key) | JsonFieldKey::Map(MapKeyRef::Str(key)) => Cow::Borrowed(key),
            JsonFieldKey::Name(name) => Cow::Owned(name.to_string()),
            JsonFieldKey::Map(_) => return Err(rejected(NON_STRING_KEY_MESSAGE)),
        };
        Ok(Some((key, SerializeHandle::arena(BorrowedValue { value, pretty: self.pretty, span: self.span }, state))))
    }
}

struct StatsFields<'a> { stats: JsonStats<'a>, index: usize, pretty: bool, span: Span }
impl StructEmitter for StatsFields<'_> {
    fn next(&mut self, state: &mut State) -> Result<Option<(Cow<'_, str>, SerializeHandle<'_>)>, Error> {
        let (key, value) = match self.index {
            0 => ("blanks", JsonInput::Integer(self.stats.blanks)),
            1 => ("blobs", self.stats.blobs.map_or(JsonInput::EmptyObject, |v| JsonInput::LoweredMap(v))),
            2 => ("code", JsonInput::Integer(self.stats.code)),
            3 => ("comments", JsonInput::Integer(self.stats.comments)),
            _ => return Ok(None),
        };
        self.index += 1;
        Ok(Some((Cow::Borrowed(key), SerializeHandle::arena(BorrowedValue { value, pretty: self.pretty, span: self.span }, state))))
    }
}

fn incompatible(value: JsonInput<'_>, span: Span) -> RuntimeError {
    let view = match value {
        JsonInput::Runtime(value) => ValueView::Runtime(value),
        JsonInput::Lowered(value) => ValueView::Lowered(value),
        JsonInput::Key(key) => ValueView::Key(key),
        JsonInput::Path(path) => ValueView::Path(&path.bytes),
        JsonInput::Integer(_) | JsonInput::EmptyObject | JsonInput::LoweredMap(_) => unreachable!("synthetic JSON nodes are representable"),
    };
    RuntimeError::new("json-compatible", format!("{} is not JSON-compatible; convert Path, Bytes, Status, Result, and errors explicitly", view.type_name())).with_span(span)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::eval::LoweredTagValue;
    use crate::symbol::SymbolOwner;
    use crate::source::SourceId;
    use std::any::Any;
    use std::process::{Command, Stdio};
    use std::sync::Barrier;
    use std::time::{Duration, Instant};

    const RECORD_SORT_WORKER: &str = "XSH_JSON_RECORD_SORT_LOCK_WORKER";
    const RECORD_SORT_DEADLINE: Duration = Duration::from_secs(10);

    fn span() -> Span {
        Span::new(SourceId::new(0), 0, 1)
    }

    #[test]
    fn nested_runtime_and_lowered_values_share_the_borrowed_encoder() {
        let input = r#"{"z":[null,true,-9223372036854775808,0.125,"\u0000"],"a":{"empty":[]}}"#;
        let runtime = crate::modules::codec::decode_json(input, span()).unwrap();
        let lowered = super::super::lowered_ops::lowered_value_from_runtime_any(&runtime).unwrap();
        for pretty in [false, true] {
            let encoded = encode_runtime(&runtime, pretty, span()).unwrap();
            assert_eq!(encoded, encode_lowered(&lowered, pretty, span()).unwrap());
            assert_eq!(crate::modules::codec::decode_json(&encoded, span()).unwrap(), runtime);
        }
        assert_eq!(encode_runtime(&runtime, false, span()).unwrap(), r#"{"a":{"empty":[]},"z":[null,true,-9223372036854775808,0.125,"\u0000"]}"#);
    }

    #[test]
    fn dynamic_record_vec_json_completes_while_symbols_are_interned() {
        if std::env::var_os(RECORD_SORT_WORKER).is_some() {
            run_concurrent_record_sort_worker();
            return;
        }

        let test_name = std::thread::current()
            .name()
            .expect("named test thread")
            .to_string();
        let mut child = Command::new(std::env::current_exe().expect("test executable"))
            .arg("--exact")
            .arg(&test_name)
            .arg("--nocapture")
            .env(RECORD_SORT_WORKER, "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start concurrent record sort worker");
        let deadline = Instant::now() + RECORD_SORT_DEADLINE;
        let mut timed_out = false;
        let mut poll_error = None;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => std::thread::yield_now(),
                Ok(None) => {
                    timed_out = true;
                    break;
                }
                Err(error) => {
                    poll_error = Some(error);
                    break;
                }
            }
        }
        let kill_error = (timed_out || poll_error.is_some())
            .then(|| child.kill().err())
            .flatten();
        let output = child.wait_with_output();
        if let Some(error) = poll_error {
            panic!(
                "poll concurrent record sort worker failed: {error}; kill error: {kill_error:?}; output: {output:?}"
            );
        }
        let output = output.unwrap_or_else(|error| {
            panic!("collect concurrent record sort worker output: {error}; kill error: {kill_error:?}")
        });
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !timed_out,
            "concurrent record sort worker exceeded {:?}; kill error: {:?}; status={:?}\nstdout:\n{}\nstderr:\n{}",
            RECORD_SORT_DEADLINE,
            kill_error,
            output.status,
            stdout,
            stderr,
        );
        assert!(
            stdout.contains(&format!("test {test_name} ... ok")),
            "child did not run exactly the requested test successfully\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        assert!(
            output.status.success(),
            "concurrent record sort worker failed ({:?})\nstdout:\n{}\nstderr:\n{}",
            output.status,
            stdout,
            stderr,
        );
    }

    fn run_concurrent_record_sort_worker() {
        const ENCODERS: usize = 8;
        const INTERNERS: usize = 4;
        const FIELD_COUNT: usize = 16;
        const ENCODE_ITERATIONS: usize = 512;
        const INTERN_ITERATIONS: usize = 1024;

        let ready = Arc::new(Barrier::new(ENCODERS + INTERNERS));
        std::thread::scope(|scope| {
            let encoder_handles = (0..ENCODERS)
                .map(|encoder| {
                    let ready = Arc::clone(&ready);
                    scope.spawn(move || {
                        let owner = SymbolOwner::new();
                        let fields = owner.with_current(|| {
                            (0..FIELD_COUNT)
                                .rev()
                                .map(|field| {
                                    (
                                        Name::intern(format!("record_{encoder}_{field:02}")),
                                        LoweredValue::Int(field as i64),
                                    )
                                })
                                .collect::<Vec<_>>()
                        });
                        let value = LoweredValue::RecordVec(Arc::new(fields));
                        let expected = format!(
                            "{{{}}}",
                            (0..FIELD_COUNT)
                                .map(|field| format!("\"record_{encoder}_{field:02}\":{field}"))
                                .collect::<Vec<_>>()
                                .join(",")
                        );
                        ready.wait();
                        for _ in 0..ENCODE_ITERATIONS {
                            assert_eq!(encode_lowered(&value, false, span()).expect("encode record"), expected);
                        }
                    })
                })
                .collect::<Vec<_>>();
            let interner_handles = (0..INTERNERS)
                .map(|interner| {
                    let ready = Arc::clone(&ready);
                    scope.spawn(move || {
                        ready.wait();
                        for symbol in 0..INTERN_ITERATIONS {
                            let owner = SymbolOwner::new();
                            owner.with_current(|| {
                                Name::intern(format!("writer_{interner}_{symbol}"));
                            });
                            drop(owner);
                        }
                    })
                })
                .collect::<Vec<_>>();

            let failures = encoder_handles
                .into_iter()
                .enumerate()
                .filter_map(|(index, handle)| {
                    handle
                        .join()
                        .err()
                        .map(|panic| format!("encoder {index}: {}", panic_message(panic)))
                })
                .chain(interner_handles.into_iter().enumerate().filter_map(|(index, handle)| {
                    handle
                        .join()
                        .err()
                        .map(|panic| format!("interner {index}: {}", panic_message(panic)))
                }))
                .collect::<Vec<_>>();
            assert!(failures.is_empty(), "worker failures: {}", failures.join("; "));
        });
    }

    fn panic_message(panic: Box<dyn Any + Send>) -> String {
        match panic.downcast::<String>() {
            Ok(message) => *message,
            Err(panic) => match panic.downcast::<&'static str>() {
                Ok(message) => (*message).to_string(),
                Err(_) => "non-string panic".to_string(),
            },
        }
    }

    #[test]
    fn compact_map_checks_all_keys_before_values_and_pretty_walks_in_order() {
        let fields = BTreeMap::from([
            (MapKey::Str(Arc::from("a")), Value::Unit),
            (MapKey::Int(1), Value::Null),
        ]);
        let runtime = Value::Map(fields.clone());
        let lowered = LoweredValue::Map(Arc::new(fields.into_iter().map(|(key, value)| {
            (key, match value { Value::Unit => LoweredValue::Unit, _ => LoweredValue::Null })
        }).collect()));

        for result in [encode_runtime(&runtime, false, span()), encode_lowered(&lowered, false, span())] {
            let error = result.expect_err("compact maps check every key before any value");
            assert_eq!(error.message, NON_STRING_KEY_MESSAGE);
        }
        for result in [encode_runtime(&runtime, true, span()), encode_lowered(&lowered, true, span())] {
            let error = result.expect_err("pretty maps visit keys and values in order");
            assert!(error.message.starts_with("Unit is not JSON-compatible"), "{}", error.message);
        }
    }

    #[test]
    fn filesystem_entries_use_the_public_record_compatibility_diagnostic() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/runtime/value.rs");
        let entry = crate::runtime::value::FsEntryValue::new(
            source.clone(),
            std::fs::metadata(source).expect("fixture metadata").file_type(),
        );
        let runtime = Value::FsEntry(entry.clone());
        let lowered = LoweredValue::FsEntry(entry);
        let expected = "Record is not JSON-compatible; convert Path, Bytes, Status, Result, and errors explicitly";

        for result in [
            encode_runtime(&runtime, false, span()),
            encode_runtime(&runtime, true, span()),
            encode_lowered(&lowered, false, span()),
            encode_lowered(&lowered, true, span()),
        ] {
            assert_eq!(result.expect_err("filesystem entries are not JSON-compatible").message, expected);
        }
    }

    #[test]
    fn missing_wire_variant_has_one_diagnostic_for_both_value_views() {
        SymbolOwner::new().with_current(|| {
            let type_name = Name::intern("JsonWireState");
            let mapping = Arc::new(crate::sema::wire_enums::WireEnumMapping {
                type_name,
                variants: BTreeMap::new(),
            });
            let runtime = Value::Tag {
                type_name,
                name: Arc::from("Missing"),
                fields: Vec::new(),
                wire: Some(Arc::clone(&mapping)),
            };
            let lowered = LoweredValue::Tag(Box::new(LoweredTagValue {
                type_name,
                name: Arc::from("Missing"),
                fields: Vec::new(),
                wire: Some(mapping),
            }));
            for result in [encode_runtime(&runtime, false, span()), encode_lowered(&lowered, false, span())] {
                let error = result.expect_err("missing wire mapping entry");
                assert_eq!(error.message, "wire enum variant is absent from its declared mapping");
            }
            let malformed_runtime = Value::Tag {
                type_name,
                name: Arc::from("Missing"),
                fields: Vec::new(),
                wire: None,
            };
            let malformed_lowered = LoweredValue::Tag(Box::new(LoweredTagValue {
                type_name,
                name: Arc::from("Missing"),
                fields: vec![LoweredValue::Int(1)],
                wire: Some(Arc::new(crate::sema::wire_enums::WireEnumMapping {
                    type_name,
                    variants: BTreeMap::new(),
                })),
            }));
            for result in [
                encode_runtime(&malformed_runtime, false, span()),
                encode_lowered(&malformed_lowered, false, span()),
            ] {
                let error = result.expect_err("malformed tags use the generic compatibility diagnostic");
                assert!(error.message.starts_with("Tag is not JSON-compatible"), "{}", error.message);
            }
        });
    }
}
