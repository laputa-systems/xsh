use super::type_view::ValueView;
use super::{LoweredValue, Value};
use crate::map_key::{MapKey, MapKeyRef};
use crate::modules::json as json_module;
use crate::runtime::value::{RecordIter, RuntimeError};
use crate::source::Span;
use crate::symbol::Name;
use miniserde::json::{Array, Object, Value as TreeValue};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;

const NON_FINITE_FLOAT_MESSAGE: &str = "non-finite Float values are not JSON-compatible";
const NON_STRING_KEY_MESSAGE: &str = "JSON objects require Str map keys";

trait JsonSink {
    fn null(&mut self);
    fn boolean(&mut self, value: bool);
    fn integer(&mut self, value: i64);
    fn float(&mut self, value: f64);
    fn string(&mut self, value: &str);
    fn begin_array(&mut self);
    fn array_item(&mut self);
    fn end_array(&mut self);
    fn begin_object(&mut self);
    fn object_field(&mut self, key: &str);
    fn end_object(&mut self);
}

enum TreeContainer {
    Array(Array),
    Object(Object, Option<String>),
}

#[derive(Default)]
struct TreeSink {
    containers: Vec<TreeContainer>,
    root: Option<TreeValue>,
}

impl TreeSink {
    fn push(&mut self, value: TreeValue) {
        match self.containers.last_mut() {
            Some(TreeContainer::Array(items)) => items.push(value),
            Some(TreeContainer::Object(fields, key)) => {
                fields.insert(key.take().expect("JSON object field has a key"), value);
            }
            None => assert!(self.root.replace(value).is_none()),
        }
    }

    fn finish(self) -> TreeValue {
        self.root.expect("JSON traversal writes a root value")
    }
}

impl JsonSink for TreeSink {
    fn null(&mut self) { self.push(TreeValue::Null); }
    fn boolean(&mut self, value: bool) { self.push(json_module::raw_json_bool(value)); }
    fn integer(&mut self, value: i64) { self.push(json_module::raw_json_i64(value)); }
    fn float(&mut self, value: f64) { self.push(json_module::raw_json_f64(value)); }
    fn string(&mut self, value: &str) { self.push(json_module::raw_json_string(value)); }
    fn begin_array(&mut self) { self.containers.push(TreeContainer::Array(Array::new())); }
    fn array_item(&mut self) {}
    fn end_array(&mut self) {
        let Some(TreeContainer::Array(items)) = self.containers.pop() else { unreachable!("JSON array closes its own container"); };
        self.push(json_module::raw_json_array(items));
    }
    fn begin_object(&mut self) { self.containers.push(TreeContainer::Object(Object::new(), None)); }
    fn object_field(&mut self, key: &str) {
        let Some(TreeContainer::Object(_, next)) = self.containers.last_mut() else { unreachable!("JSON object field belongs to an object"); };
        *next = Some(key.to_owned());
    }
    fn end_object(&mut self) {
        let Some(TreeContainer::Object(fields, None)) = self.containers.pop() else { unreachable!("JSON object closes after its last field"); };
        self.push(TreeValue::Object(fields));
    }
}

#[derive(Default)]
struct TextContainer(bool);

#[derive(Default)]
struct TextSink {
    output: String,
    containers: Vec<TextContainer>,
}

impl JsonSink for TextSink {
    fn null(&mut self) { self.output.push_str("null"); }
    fn boolean(&mut self, value: bool) { self.output.push_str(if value { "true" } else { "false" }); }
    fn integer(&mut self, value: i64) { let _ = write!(self.output, "{value}"); }
    fn float(&mut self, value: f64) { self.output.push_str(&miniserde::json::to_string(&value)); }
    fn string(&mut self, value: &str) { super::lowered_run::lowered_write_json_str(value, &mut self.output); }
    fn begin_array(&mut self) { self.output.push('['); self.containers.push(TextContainer(true)); }
    fn array_item(&mut self) { self.separator(); }
    fn end_array(&mut self) { self.containers.pop().expect("JSON array container"); self.output.push(']'); }
    fn begin_object(&mut self) { self.output.push('{'); self.containers.push(TextContainer(true)); }
    fn object_field(&mut self, key: &str) {
        self.separator();
        super::lowered_run::lowered_write_json_str(key, &mut self.output);
        self.output.push(':');
    }
    fn end_object(&mut self) { self.containers.pop().expect("JSON object container"); self.output.push('}'); }
}

impl TextSink {
    fn separator(&mut self) {
        let Some(TextContainer(first)) = self.containers.last_mut() else { unreachable!("JSON child belongs to a container"); };
        if *first { *first = false; } else { self.output.push(','); }
    }

    fn finish(self) -> String { self.output }
}

#[derive(Clone, Copy)]
enum JsonInput<'a> {
    Runtime(&'a Value),
    Lowered(&'a LoweredValue),
    Key(MapKeyRef<'a>),
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
        JsonInput::Key(key) => JsonNode::Key(key),
        JsonInput::Runtime(value) => match value {
            Value::Null => JsonNode::Null,
            Value::Bool(value) => JsonNode::Bool(*value),
            Value::Int(value) => JsonNode::Int(*value),
            Value::Float(value) => JsonNode::Float(value.0),
            Value::Str(value) => JsonNode::Str(value),
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
    if pretty {
        encode_tree(JsonInput::Lowered(value), true, span)
    } else {
        let mut sink = TextSink::default();
        write_json(JsonInput::Lowered(value), false, &mut sink, span)?;
        Ok(sink.finish())
    }
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
    let mut sink = TreeSink::default();
    write_json(value, pretty, &mut sink, span)?;
    let value = sink.finish();
    Ok(if pretty { json_module::pretty_raw_json(&value) } else { json_module::compact_raw_json(&value) })
}

fn write_json(value: JsonInput<'_>, pretty: bool, sink: &mut impl JsonSink, span: Span) -> Result<(), RuntimeError> {
    match project(value) {
        JsonNode::Null => sink.null(),
        JsonNode::Bool(value) => sink.boolean(value),
        JsonNode::Int(value) => sink.integer(value),
        JsonNode::Float(value) if value.is_finite() => sink.float(value),
        JsonNode::Float(_) => return Err(json_error(NON_FINITE_FLOAT_MESSAGE, span)),
        JsonNode::Str(value) | JsonNode::Wire(value) => sink.string(value),
        JsonNode::MissingWireVariant => return Err(json_error("wire enum variant is absent from its declared mapping", span)),
        JsonNode::Sequence(items) => return write_sequence(items, pretty, sink, span),
        JsonNode::Map(fields) => {
            if !pretty && !fields.all_string_keys() { return Err(json_error(NON_STRING_KEY_MESSAGE, span)); }
            return write_fields(fields.fields(), pretty, sink, span);
        }
        JsonNode::Object(fields) => return write_fields(fields, pretty, sink, span),
        JsonNode::Stats(stats) => return write_stats(stats, pretty, sink, span),
        JsonNode::Key(key) => return write_key(key, sink, span),
        JsonNode::Unsupported(value) => return Err(incompatible(value, span)),
    }
    Ok(())
}

fn write_sequence(items: JsonSequence<'_>, pretty: bool, sink: &mut impl JsonSink, span: Span) -> Result<(), RuntimeError> {
    sink.begin_array();
    match items {
        JsonSequence::Runtime(items) => for item in items { sink.array_item(); write_json(JsonInput::Runtime(item), pretty, sink, span)?; },
        JsonSequence::Lowered(items) => for item in items { sink.array_item(); write_json(JsonInput::Lowered(item), pretty, sink, span)?; },
        JsonSequence::Set(items) => for item in items { sink.array_item(); write_json(JsonInput::Key(item.as_ref()), pretty, sink, span)?; },
    }
    sink.end_array();
    Ok(())
}

fn write_fields(fields: JsonFields<'_>, pretty: bool, sink: &mut impl JsonSink, span: Span) -> Result<(), RuntimeError> {
    sink.begin_object();
    for (key, value) in fields {
        let key = match key {
            JsonFieldKey::Map(MapKeyRef::Str(key)) | JsonFieldKey::Str(key) => key,
            JsonFieldKey::Name(name) => {
                name.with_str(|key| sink.object_field(key));
                write_json(value, pretty, sink, span)?;
                continue;
            }
            JsonFieldKey::Map(_) => return Err(json_error(NON_STRING_KEY_MESSAGE, span)),
        };
        sink.object_field(key);
        write_json(value, pretty, sink, span)?;
    }
    sink.end_object();
    Ok(())
}

fn write_stats(stats: JsonStats<'_>, pretty: bool, sink: &mut impl JsonSink, span: Span) -> Result<(), RuntimeError> {
    sink.begin_object();
    sink.object_field("blanks");
    sink.integer(stats.blanks);
    sink.object_field("blobs");
    if let Some(blobs) = stats.blobs {
        let fields = JsonMap::Lowered(blobs);
        if !pretty && !fields.all_string_keys() { return Err(json_error(NON_STRING_KEY_MESSAGE, span)); }
        write_fields(fields.fields(), pretty, sink, span)?;
    } else {
        sink.begin_object();
        sink.end_object();
    }
    sink.object_field("code");
    sink.integer(stats.code);
    sink.object_field("comments");
    sink.integer(stats.comments);
    sink.end_object();
    Ok(())
}

fn write_key(key: MapKeyRef<'_>, sink: &mut impl JsonSink, span: Span) -> Result<(), RuntimeError> {
    match key {
        MapKeyRef::Str(value) => sink.string(value),
        MapKeyRef::Int(value) => sink.integer(value),
        MapKeyRef::Bool(value) => sink.boolean(value),
        _ => return Err(incompatible(JsonInput::Key(key), span)),
    }
    Ok(())
}

fn incompatible(value: JsonInput<'_>, span: Span) -> RuntimeError {
    let view = match value {
        JsonInput::Runtime(value) => ValueView::Runtime(value),
        JsonInput::Lowered(value) => ValueView::Lowered(value),
        JsonInput::Key(key) => ValueView::Key(key),
    };
    RuntimeError::new("json-compatible", format!("{} is not JSON-compatible; convert Path, Bytes, Status, Result, and errors explicitly", view.type_name())).with_span(span)
}

fn json_error(message: &str, span: Span) -> RuntimeError {
    RuntimeError::new("json-compatible", message).with_span(span)
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
