use super::type_view::ValueView;
use crate::modules::codec::Format;
use super::{LoweredValue, Value};
use crate::map_key::{MapKey, MapKeyRef};
use crate::runtime::value::{RecordIter, RuntimeError};
use crate::source::Span;
use crate::symbol::Name;
use std::collections::BTreeMap;
use std::borrow::Cow;
use deser::{Atom, Error, ErrorKind, Serialize, State};
use deser::ser::{Emit, MapEmitter, SeqEmitter, SerializeHandle, StructEmitter};
use std::sync::Arc;

const NON_FINITE_FLOAT_MESSAGE: &str = "non-finite Float values are not JSON-compatible";
const NON_STRING_KEY_MESSAGE: &str = "JSON objects require Str map keys";

#[derive(Clone, Copy)]
enum CodecInput<'a> {
    Runtime(&'a Value),
    Record(&'a crate::runtime::value::RecordMap),
    Lowered(&'a LoweredValue),
    Key(MapKeyRef<'a>),
    Path(&'a crate::runtime::value::PathValue),
    Integer(i64),
    EmptyObject,
    LoweredMap(&'a BTreeMap<MapKey, LoweredValue>),
}

enum CodecNode<'a> {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(&'a str),
    Bytes(&'a [u8]),
    Wire(&'a str),
    MissingWireVariant,
    Sequence(CodecSequence<'a>),
    Map(CodecMap<'a>),
    Object(CodecFields<'a>),
    Stats(CodecStats<'a>),
    Key(MapKeyRef<'a>),
    Unsupported(CodecInput<'a>),
}

enum CodecSequence<'a> {
    Paths(std::slice::Iter<'a, crate::runtime::value::PathValue>),
    Runtime(std::slice::Iter<'a, Value>),
    Lowered(std::slice::Iter<'a, LoweredValue>),
    Set(std::collections::btree_set::Iter<'a, MapKey>),
}

enum CodecMap<'a> {
    Runtime(&'a BTreeMap<MapKey, Value>),
    Lowered(&'a BTreeMap<MapKey, LoweredValue>),
}

impl<'a> CodecMap<'a> {
    fn all_string_keys(&self) -> bool {
        match self {
            Self::Runtime(fields) => fields.keys().all(|key| key.as_str().is_some()),
            Self::Lowered(fields) => fields.keys().all(|key| key.as_str().is_some()),
        }
    }

    fn fields(self) -> CodecFields<'a> {
        match self {
            Self::Runtime(fields) => CodecFields::RuntimeMap(fields.iter()),
            Self::Lowered(fields) => CodecFields::LoweredMap(fields.iter()),
        }
    }
}

enum CodecFieldKey<'a> {
    Map(MapKeyRef<'a>),
    Name(Name),
    Str(&'a str),
}

enum CodecFields<'a> {
    Empty,
    RuntimeMap(std::collections::btree_map::Iter<'a, MapKey, Value>),
    LoweredMap(std::collections::btree_map::Iter<'a, MapKey, LoweredValue>),
    RuntimeRecord(RecordIter<'a>),
    LoweredRecord(std::collections::btree_map::Iter<'a, Arc<str>, LoweredValue>),
    RecordVec(std::vec::IntoIter<(&'a Name, &'a LoweredValue)>),
}

impl<'a> Iterator for CodecFields<'a> {
    type Item = (CodecFieldKey<'a>, CodecInput<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Empty => None,
            Self::RuntimeMap(fields) => fields.next().map(|(key, value)| (CodecFieldKey::Map(key.as_ref()), CodecInput::Runtime(value))),
            Self::LoweredMap(fields) => fields.next().map(|(key, value)| (CodecFieldKey::Map(key.as_ref()), CodecInput::Lowered(value))),
            Self::RuntimeRecord(fields) => fields.next().map(|(key, value)| (CodecFieldKey::Str(key), CodecInput::Runtime(value))),
            Self::LoweredRecord(fields) => fields.next().map(|(key, value)| (CodecFieldKey::Str(key), CodecInput::Lowered(value))),
            Self::RecordVec(fields) => fields.next().map(|(key, value)| (CodecFieldKey::Name(*key), CodecInput::Lowered(value))),
        }
    }
}

struct CodecStats<'a> {
    blanks: i64,
    blobs: Option<&'a BTreeMap<MapKey, LoweredValue>>,
    code: i64,
    comments: i64,
}

fn project<'a>(value: CodecInput<'a>) -> CodecNode<'a> {
    match value {
        CodecInput::LoweredMap(value) => CodecNode::Map(CodecMap::Lowered(value)),
        CodecInput::Integer(value) => CodecNode::Int(value),
        CodecInput::EmptyObject => CodecNode::Object(CodecFields::Empty),
        CodecInput::Key(key) => CodecNode::Key(key),
        CodecInput::Path(path) => CodecNode::Unsupported(CodecInput::Path(path)),
        CodecInput::Record(fields) => CodecNode::Object(CodecFields::RuntimeRecord(fields.iter())),
        CodecInput::Runtime(value) => match value {
            Value::Null => CodecNode::Null,
            Value::Bool(value) => CodecNode::Bool(*value),
            Value::Int(value) => CodecNode::Int(*value),
            Value::Float(value) => CodecNode::Float(value.0),
            Value::Str(value) => CodecNode::Str(value),
            Value::Bytes(value) => CodecNode::Bytes(value),
            Value::EnvPathList(paths) => CodecNode::Sequence(CodecSequence::Paths(paths.entries().iter())),
            Value::List(items) => CodecNode::Sequence(CodecSequence::Runtime(items.iter())),
            Value::Set(items) => CodecNode::Sequence(CodecSequence::Set(items.iter())),
            Value::Map(fields) => CodecNode::Map(CodecMap::Runtime(fields)),
            Value::Record(fields) => CodecNode::Object(CodecFields::RuntimeRecord(fields.iter())),
            Value::Tag { type_name, name, fields, wire: Some(mapping) }
                if fields.is_empty() && *type_name == mapping.type_name =>
            {
                mapping.variants.get(&Name::intern(name.as_ref())).map_or(CodecNode::MissingWireVariant, |wire| CodecNode::Wire(wire))
            }
            _ => CodecNode::Unsupported(CodecInput::Runtime(value)),
        },
        CodecInput::Lowered(value) => match value {
            LoweredValue::Null => CodecNode::Null,
            LoweredValue::Bool(value) => CodecNode::Bool(*value),
            LoweredValue::Int(value) => CodecNode::Int(*value),
            LoweredValue::Float(value) => CodecNode::Float(value.0),
            LoweredValue::Str(value) => CodecNode::Str(value),
            LoweredValue::Bytes(value) => CodecNode::Bytes(value),
            LoweredValue::BytesView(value) => CodecNode::Bytes(value.as_slice()),
            LoweredValue::StrView(value) => CodecNode::Str(value.as_str()),
            LoweredValue::Tag(tag) if tag.wire.is_some() && tag.fields.is_empty() && tag.type_name == tag.wire.as_ref().expect("checked mapping").type_name => {
                tag.wire.as_ref().and_then(|mapping| mapping.variants.get(&Name::intern(tag.name.as_ref()))).map_or(CodecNode::MissingWireVariant, |wire| CodecNode::Wire(wire))
            }
            LoweredValue::EnvPathList(paths) => CodecNode::Sequence(CodecSequence::Paths(paths.entries().iter())),
            LoweredValue::List(items) => CodecNode::Sequence(CodecSequence::Lowered(items.iter())),
            LoweredValue::SharedList(items) => CodecNode::Sequence(CodecSequence::Lowered(items.iter())),
            LoweredValue::Set(items) => CodecNode::Sequence(CodecSequence::Set(items.iter())),
            LoweredValue::Map(fields) => CodecNode::Map(CodecMap::Lowered(fields)),
            LoweredValue::Record(fields) => CodecNode::Object(CodecFields::LoweredRecord(fields.iter())),
            LoweredValue::RecordVec(fields) => {
                let mut ordered: Vec<_> = fields.iter().map(|(key, value)| (key, value)).collect();
                ordered.sort_unstable_by_key(|(name, _)| *name);
                CodecNode::Object(CodecFields::RecordVec(ordered.into_iter()))
            }
            LoweredValue::Stats { blanks, code, comments } => CodecNode::Stats(CodecStats { blanks: *blanks, blobs: None, code: *code, comments: *comments }),
            LoweredValue::StatsBlob(stats) => CodecNode::Stats(CodecStats { blanks: stats.blanks, blobs: Some(&stats.blobs), code: stats.code, comments: stats.comments }),
            _ => CodecNode::Unsupported(CodecInput::Lowered(value)),
        },
    }
}

pub(super) fn encode_runtime(value: &Value, pretty: bool, span: Span) -> Result<String, RuntimeError> {
    encode_tree(CodecInput::Runtime(value), pretty, span)
}

pub(super) fn encode_lowered(value: &LoweredValue, pretty: bool, span: Span) -> Result<String, RuntimeError> {
    encode_tree(CodecInput::Lowered(value), pretty, span)
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

fn encode_tree(value: CodecInput<'_>, pretty: bool, span: Span) -> Result<String, RuntimeError> {
    let mut config = deser_json::SerializerConfig::new();
    if pretty { config.set_pretty(deser_json::Indent::Spaces(2)); }
    let mut output = crate::modules::codec::Output::new();
    let result = config.writer(&mut output).write_with(&BorrowedValue { value, pretty, span, format: Format::Json }, |driver| {
        driver.push_layer(crate::modules::codec::EncodeLimits::default());
    });
    let output = output.finish(result)
        .map_err(|error| crate::modules::codec::runtime_error(error, "json", span))?;
    Ok(output)
}

struct BorrowedValue<'a> {
    value: CodecInput<'a>,
    format: Format,
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
            CodecNode::Null => Emit::Atom(Atom::Null),
            CodecNode::Bool(v) => Emit::Atom(Atom::Bool(v)),
            CodecNode::Int(v) => Emit::Atom(Atom::I64(v)),
            CodecNode::Float(v) if v.is_finite() => Emit::Atom(Atom::F64(v)),
            CodecNode::Float(_) => return Err(rejected(if value.format == Format::Json { NON_FINITE_FLOAT_MESSAGE } else { "codec Float values must be finite" })),
            CodecNode::Bytes(bytes) if value.format.supports_bytes() => Emit::Atom(Atom::Bytes(deser::Bytes::borrowed(bytes))),
            CodecNode::Bytes(_) => return Err(rejected(unsupported_type("Bytes", value.format))),
            CodecNode::Str(v) | CodecNode::Wire(v) => {
                if v.len() > 16 * 1024 * 1024 { return Err(Error::new(ErrorKind::LimitExceeded, "codec string exceeds 16 MiB")); }
                Emit::Atom(Atom::Str(v.into()))
            }
            CodecNode::MissingWireVariant => return Err(rejected("wire enum variant is absent from its declared mapping")),
            CodecNode::Sequence(items) => Emit::seq(Items { items, pretty: value.pretty, span: value.span, format: value.format }, state),
            CodecNode::Map(fields) if value.format.is_binary() => Emit::map(BinaryFields { fields: fields.fields(), pending: None, format: value.format, pretty: value.pretty, span: value.span }, state),
            CodecNode::Map(fields) => {
                if !value.pretty && !fields.all_string_keys() { return Err(rejected(NON_STRING_KEY_MESSAGE)); }
                Emit::structure(Fields { fields: fields.fields(), pretty: value.pretty, span: value.span, format: value.format }, state)
            }
            CodecNode::Object(fields) => Emit::structure(Fields { fields, pretty: value.pretty, span: value.span, format: value.format }, state),
            CodecNode::Stats(stats) => Emit::structure(StatsFields { stats, index: 0, pretty: value.pretty, span: value.span, format: value.format }, state),
            CodecNode::Key(MapKeyRef::Str(v)) => Emit::Atom(Atom::Str(v.into())),
            CodecNode::Key(MapKeyRef::Int(v)) => Emit::Atom(Atom::I64(v)),
            CodecNode::Key(MapKeyRef::Bool(v)) => Emit::Atom(Atom::Bool(v)),
            CodecNode::Key(MapKeyRef::Bytes(v)) if value.format.supports_bytes() => Emit::Atom(Atom::Bytes(deser::Bytes::borrowed(v))),
            CodecNode::Key(key) => return Err(rejected(incompatible(CodecInput::Key(key), value.format))),
            CodecNode::Unsupported(v) => return Err(rejected(incompatible(v, value.format))),
        })
    }
}

struct Items<'a> { items: CodecSequence<'a>, pretty: bool, span: Span, format: Format }
impl SeqEmitter for Items<'_> {
    fn next(&mut self, state: &mut State) -> Result<Option<SerializeHandle<'_>>, Error> {
        let value = match &mut self.items {
            CodecSequence::Paths(items) => items.next().map(CodecInput::Path),
            CodecSequence::Runtime(items) => items.next().map(CodecInput::Runtime),
            CodecSequence::Lowered(items) => items.next().map(CodecInput::Lowered),
            CodecSequence::Set(items) => items.next().map(|v| CodecInput::Key(v.as_ref())),
        };
        Ok(value.map(|value| SerializeHandle::arena(BorrowedValue { value, pretty: self.pretty, span: self.span, format: self.format }, state)))
    }
}

struct Fields<'a> { fields: CodecFields<'a>, pretty: bool, span: Span, format: Format }
impl StructEmitter for Fields<'_> {
    fn next(&mut self, state: &mut State) -> Result<Option<(Cow<'_, str>, SerializeHandle<'_>)>, Error> {
        let Some((key, value)) = self.fields.next() else { return Ok(None); };
        let key = match key {
            CodecFieldKey::Str(key) | CodecFieldKey::Map(MapKeyRef::Str(key)) => Cow::Borrowed(key),
            CodecFieldKey::Name(name) => Cow::Owned(name.to_string()),
            CodecFieldKey::Map(_) => return Err(rejected(NON_STRING_KEY_MESSAGE)),
        };
        Ok(Some((key, SerializeHandle::arena(BorrowedValue { value, pretty: self.pretty, span: self.span, format: self.format }, state))))
    }
}

struct StatsFields<'a> { stats: CodecStats<'a>, index: usize, pretty: bool, span: Span, format: Format }
impl StructEmitter for StatsFields<'_> {
    fn next(&mut self, state: &mut State) -> Result<Option<(Cow<'_, str>, SerializeHandle<'_>)>, Error> {
        let (key, value) = match self.index {
            0 => ("blanks", CodecInput::Integer(self.stats.blanks)),
            1 => ("blobs", self.stats.blobs.map_or(CodecInput::EmptyObject, |v| CodecInput::LoweredMap(v))),
            2 => ("code", CodecInput::Integer(self.stats.code)),
            3 => ("comments", CodecInput::Integer(self.stats.comments)),
            _ => return Ok(None),
        };
        self.index += 1;
        Ok(Some((Cow::Borrowed(key), SerializeHandle::arena(BorrowedValue { value, pretty: self.pretty, span: self.span, format: self.format }, state))))
    }
}

fn unsupported_type(name: &str, format: Format) -> String {
    if format == Format::Json {
        format!("{name} is not JSON-compatible; convert Path, Bytes, Status, Result, and errors explicitly")
    } else { format!("{name} is not representable in {}", format.identifier()) }
}

fn incompatible(value: CodecInput<'_>, format: Format) -> String {
    let view = match value {
        CodecInput::Runtime(value) => ValueView::Runtime(value),
        CodecInput::Lowered(value) => ValueView::Lowered(value),
        CodecInput::Key(key) => ValueView::Key(key),
        CodecInput::Path(path) => ValueView::Path(&path.bytes),
        CodecInput::Record(_) | CodecInput::Integer(_) | CodecInput::EmptyObject | CodecInput::LoweredMap(_) => unreachable!("synthetic codec nodes are representable"),
    };
    unsupported_type(view.type_name(), format)
}

pub(super) fn borrowed_record<'a>(format: Format, record: &'a crate::runtime::value::RecordMap, span: Span) -> impl Serialize + 'a {
    BorrowedValue { value: CodecInput::Record(record), format, pretty: false, span }
}

pub(super) fn borrowed_runtime<'a>(format: Format, value: &'a Value, span: Span) -> impl Serialize + 'a {
    BorrowedValue { value: CodecInput::Runtime(value), format, pretty: false, span }
}

pub(super) fn borrowed_lowered<'a>(format: Format, value: &'a LoweredValue, span: Span) -> impl Serialize + 'a {
    BorrowedValue { value: CodecInput::Lowered(value), format, pretty: false, span }
}

struct BinaryFields<'a> {
    fields: CodecFields<'a>,
    pending: Option<CodecInput<'a>>,
    format: Format,
    pretty: bool,
    span: Span,
}

impl MapEmitter for BinaryFields<'_> {
    fn next_key(&mut self, state: &mut State) -> Result<Option<SerializeHandle<'_>>, Error> {
        let Some((key, value)) = self.fields.next() else { return Ok(None); };
        let CodecFieldKey::Map(key) = key else { unreachable!("binary map scalar key") };
        self.pending = Some(value);
        Ok(Some(SerializeHandle::arena(BorrowedValue { value: CodecInput::Key(key), format: self.format, pretty: self.pretty, span: self.span }, state)))
    }
    fn next_value(&mut self, state: &mut State) -> Result<SerializeHandle<'_>, Error> {
        let value = self.pending.take().expect("binary map value after key");
        Ok(SerializeHandle::arena(BorrowedValue { value, format: self.format, pretty: self.pretty, span: self.span }, state))
    }
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
