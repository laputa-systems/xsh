#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn span() -> Span { Span::new(SourceId::new(0), 0, 1) }

    struct Probe {
        input: std::io::Cursor<Vec<u8>>,
        reads: Arc<AtomicUsize>,
        drops: Arc<AtomicUsize>,
        chunk: usize,
    }

    impl Read for Probe {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            let count = output.len().min(self.chunk);
            self.input.read(&mut output[..count])
        }
    }

    impl Drop for Probe {
        fn drop(&mut self) { self.drops.fetch_add(1, Ordering::SeqCst); }
    }

    #[test]
    fn sequence_construction_is_lazy_and_cancel_drops_the_reader() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _scope = symbols.enter();
        let reads = Arc::new(AtomicUsize::new(0));
        let drops = Arc::new(AtomicUsize::new(0));
        let stream = from_reader(Format::Json, Probe { input: std::io::Cursor::new(b"1\n2\n".to_vec()), reads: reads.clone(), drops: drops.clone(), chunk: 1 }, None, span()).unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), 0);
        assert_eq!(stream.next_live(span()).unwrap(), Some(Value::ok(Value::Int(1))));
        stream.source.as_ref().unwrap().cancel(span()).unwrap();
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn sequence_json_lines_recovers_and_keeps_stream_locations() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _scope = symbols.enter();
        let stream = decode_text(Format::Json, "\n1\nbad\n2\n".into(), span()).unwrap();
        assert_eq!(stream.next_live(span()).unwrap(), Some(Value::ok(Value::Int(1))));
        let Some(Value::Result(ResultValue::Err(error))) = stream.next_live(span()).unwrap() else { panic!("item error") };
        let Value::Error(error) = *error else { panic!("codec error") };
        assert_eq!(error.family, "CodecError");
        assert_eq!(error.payload.get("line"), Some(&Value::Int(3)));
        assert_eq!(error.payload.get("value_path"), Some(&Value::Str("[2]".into())));
        assert_eq!(stream.next_live(span()).unwrap(), Some(Value::ok(Value::Int(2))));
        assert_eq!(stream.next_live(span()).unwrap(), None);
    }

    #[test]
    fn sequence_emitted_budget_charges_numbers_keys_and_errors() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _scope = symbols.enter();
        assert_eq!(emitted_cost(&Value::ok(Value::Int(7))), 10);
        let value = Value::Record([("x".into(), Value::Bool(true))].into());
        assert_eq!(emitted_cost(&value), 21);
        let error = Value::err(Value::Error(Box::new(super::super::failure("Syntax", "json", "bad", span()))));
        assert!(emitted_cost(&error) > 100);
        let mut budget = EmittedBudget::new(Format::Json, None, span());
        let allowance = OUTPUT_LIMIT as u64 - budget.reserve;
        budget.used = allowance - 10;
        assert!(budget.accept(&Value::ok(Value::Int(7))));
        assert!(!budget.accept(&Value::Null));
        assert!(budget.used + emitted_cost(&budget.limit_error(1)) <= OUTPUT_LIMIT as u64);
    }

    #[test]
    fn sequence_chunked_formats_retain_parser_state_and_close_on_exhaustion() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _scope = symbols.enter();
        for format in [Format::Json, Format::Yaml, Format::Cbor, Format::Msgpack] {
            let encoded = encode_all(format, [1i64, 2, 3], span()).unwrap();
            let reads = Arc::new(AtomicUsize::new(0));
            let drops = Arc::new(AtomicUsize::new(0));
            let stream = from_reader(format, Probe { input: Cursor::new(encoded), reads, drops: drops.clone(), chunk: 1 }, None, span()).unwrap();
            for value in 1..=3 { assert_eq!(stream.next_live(span()).unwrap(), Some(Value::ok(Value::Int(value))), "{}", format.identifier()); }
            assert_eq!(stream.next_live(span()).unwrap(), None);
            assert_eq!(drops.load(Ordering::SeqCst), 1);
        }
    }

    #[test]
    fn sequence_csv_utf8_record_errors_recover_but_headers_are_terminal() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _scope = symbols.enter();
        for (bytes, kind) in [(&b"a,a\n1,2\n"[..], "DuplicateKey"), (&b"a,a\n\xff,2\n"[..], "DuplicateKey"), (&b"\xff\n1\n"[..], "Encoding"), (&b"\xff\xfea\0\n\0"[..], "Encoding")] {
            let stream = decode_bytes(Format::Csv, bytes.into(), span()).unwrap();
            let Some(Value::Result(ResultValue::Err(error))) = stream.next_live(span()).unwrap() else { panic!("header error") };
            let Value::Error(error) = *error else { panic!("codec error") };
            assert_eq!(error.variant, kind);
            assert_eq!(stream.next_live(span()).unwrap(), None);
        }
        let stream = decode_bytes(Format::Csv, b"a\n\xff\nok\n".as_slice().into(), span()).unwrap();
        let Some(Value::Result(ResultValue::Err(error))) = stream.next_live(span()).unwrap() else { panic!("record error") };
        let Value::Error(error) = *error else { panic!("codec error") };
        assert_eq!(error.variant, "Encoding");
        assert!(matches!(stream.next_live(span()).unwrap(), Some(Value::Result(ResultValue::Ok(_)))));
    }

    #[test]
    fn sequence_terminal_errors_drop_readers_before_the_error_is_published() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _scope = symbols.enter();
        for (format, bytes) in [(Format::Cbor, &b"\xff"[..]), (Format::Msgpack, &b"\xc1"[..]), (Format::Yaml, &b"--- [broken\n--- 2\n"[..])] {
            let drops = Arc::new(AtomicUsize::new(0));
            let stream = from_reader(format, Probe { input: Cursor::new(bytes.to_vec()), reads: Arc::new(AtomicUsize::new(0)), drops: drops.clone(), chunk: 1 }, None, span()).unwrap();
            assert!(matches!(stream.next_live(span()).unwrap(), Some(Value::Result(ResultValue::Err(_)))));
            assert_eq!(drops.load(Ordering::SeqCst), 1);
            assert_eq!(stream.next_live(span()).unwrap(), None);
        }
    }

    #[test]
    fn sequence_mid_read_host_errors_keep_errno_and_close() {
        struct FailedRead(Arc<AtomicUsize>);
        impl Drop for FailedRead { fn drop(&mut self) { self.0.fetch_add(1, Ordering::SeqCst); } }
        impl Read for FailedRead {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> { Err(std::io::Error::from_raw_os_error(libc::EIO)) }
        }
        let symbols = crate::symbol::SymbolOwner::new();
        let _scope = symbols.enter();
        for path in [None, Some(PathValue::new(b"retained-codec-input".to_vec()).unwrap())] {
            let drops = Arc::new(AtomicUsize::new(0));
            let stream = from_reader(Format::Json, FailedRead(drops.clone()), path.clone(), span()).unwrap();
            let error = stream.next_live(span()).unwrap_err();
            assert_eq!(error.errno(), Some(i64::from(libc::EIO)));
            assert!(error.facets.iter().any(|facet| facet == "HostIo"));
            if path.is_some() { assert!(error.message.contains("retained-codec-input"), "{}", error.message); }
            assert_eq!(drops.load(Ordering::SeqCst), 1);
            assert_eq!(stream.next_live(span()).unwrap(), None);
        }
    }

    #[test]
    fn sequence_csv_encoder_retains_one_header_and_exact_row_keysets() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _scope = symbols.enter();
        let rows = [std::collections::BTreeMap::from([("a", "1"), ("b", "2")]), std::collections::BTreeMap::from([("a", "3"), ("b", "4")])];
        assert_eq!(encode_all(Format::Csv, rows, span()).unwrap(), b"a,b\n1,2\n3,4\n");
        let wrong = [std::collections::BTreeMap::from([("a", 1), ("b", 2)]), std::collections::BTreeMap::from([("a", 3)])];
        assert_eq!(encode_all(Format::Csv, wrong, span()).unwrap_err().variant, "UnsupportedValue");
    }

    #[test]
    fn sequence_read_all_keeps_owned_paths_and_independent_readers() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _scope = symbols.enter();
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".work/tmp");
        std::fs::create_dir_all(&root).unwrap();
        let directory = tempfile::tempdir_in(root).unwrap();
        let path = directory.path().join("items");
        std::fs::write(&path, "bad\n2\n").unwrap();
        let logical = PathValue::new(b"logical-input".to_vec()).unwrap();
        for _ in 0..2 {
            let stream = read_all(Format::Json, std::fs::File::open(&path).unwrap(), logical.clone(), span()).unwrap();
            let Some(Value::Result(ResultValue::Err(error))) = stream.next_live(span()).unwrap() else { panic!("item error") };
            let Value::Error(error) = *error else { panic!("codec error") };
            assert_eq!(error.payload.get("input_path"), Some(&Value::Path(logical.clone())));
            assert_eq!(stream.next_live(span()).unwrap(), Some(Value::ok(Value::Int(2))));
            assert_eq!(stream.next_live(span()).unwrap(), None);
        }
    }


    #[test]
    fn sequence_exact_item_limit_allows_eof_and_rejects_the_next_frame() {
        let config = deser_json::DeserializerConfig::builder().trailing(deser_json::Trailing::Newline).build();
        let mut parser = Framed { inner: deser_json::StreamDeserializer::with_config(config), fatal: false, encoding: false, value_start: None, items: ITEM_LIMIT - 1 };
        assert!(matches!(parser.frame(b"1\n", true).unwrap(), Frame::Value { .. }));
        assert!(matches!(parser.frame(b"", true).unwrap(), Frame::End));
        assert_eq!(parser.frame(b"2\n", true).unwrap_err().kind(), ErrorKind::LimitExceeded);
    }

    #[test]
    fn sequence_output_allowance_counts_error_payload_and_numeric_containers() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _scope = symbols.enter();
        let error = Value::err(Value::Error(Box::new(super::super::failure("Syntax", "json", "invalid item", span()))));
        let mut budget = EmittedBudget::new(Format::Json, None, span());
        budget.used = OUTPUT_LIMIT as u64 - budget.reserve - emitted_cost(&error);
        assert!(budget.accept(&error));
        assert!(!budget.accept(&error));
        let list = Value::ok(Value::List(vec![Value::Int(0); 1_000_000]));
        let mut budget = EmittedBudget::new(Format::Cbor, None, span());
        for _ in 0..14 { assert!(budget.accept(&list)); }
        assert!(!budget.accept(&list));
        assert!(budget.used + emitted_cost(&budget.limit_error(15)) <= OUTPUT_LIMIT as u64);
    }


    #[test]
    fn sequence_cumulative_input_is_bounded_even_when_every_line_is_blank() {
        struct BlankInput { remaining: usize, read: Arc<AtomicUsize> }
        impl Read for BlankInput {
            fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
                let count = output.len().min(self.remaining);
                output[..count].fill(b'\n');
                self.remaining -= count;
                self.read.fetch_add(count, Ordering::SeqCst);
                Ok(count)
            }
        }
        let symbols = crate::symbol::SymbolOwner::new();
        let _scope = symbols.enter();
        for extra in [0, 1] {
            let read = Arc::new(AtomicUsize::new(0));
            let stream = from_reader(Format::Json, BlankInput { remaining: STREAM_INPUT_LIMIT + extra, read: read.clone() }, None, span()).unwrap();
            let value = stream.next_live(span()).unwrap();
            if extra == 0 { assert_eq!(value, None); }
            else {
                let Some(Value::Result(ResultValue::Err(error))) = value else { panic!("input limit") };
                let Value::Error(error) = *error else { panic!("codec error") };
                assert_eq!(error.variant, "Limit");
                assert_eq!(stream.next_live(span()).unwrap(), None);
            }
            assert_eq!(read.load(Ordering::SeqCst), STREAM_INPUT_LIMIT + extra);
        }
    }

    #[test]
    fn sequence_frame_limit_accepts_exact_eof_and_yaml_marker_lookahead() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _scope = symbols.enter();
        for extra in [0, 1] {
            let mut bytes = Vec::with_capacity(INPUT_LIMIT + extra);
            bytes.extend_from_slice(b"1 #");
            bytes.resize(INPUT_LIMIT + extra, b' ');
            let stream = from_reader(Format::Yaml, Cursor::new(bytes), None, span()).unwrap();
            let value = stream.next_live(span()).unwrap();
            if extra == 0 { assert_eq!(value, Some(Value::ok(Value::Int(1)))); }
            else {
                let Some(Value::Result(ResultValue::Err(error))) = value else { panic!("frame limit") };
                let Value::Error(error) = *error else { panic!("codec error") };
                assert_eq!(error.variant, "Limit");
            }
            assert_eq!(stream.next_live(span()).unwrap(), None);
        }
        let mut bytes = Vec::with_capacity(INPUT_LIMIT + 7);
        bytes.extend_from_slice(b"1 #");
        bytes.resize(INPUT_LIMIT - 1, b' ');
        bytes.extend_from_slice(b"\n--- 2\n");
        let stream = from_reader(Format::Yaml, Cursor::new(bytes), None, span()).unwrap();
        assert_eq!(stream.next_live(span()).unwrap(), Some(Value::ok(Value::Int(1))));
        assert_eq!(stream.next_live(span()).unwrap(), Some(Value::ok(Value::Int(2))));
        assert_eq!(stream.next_live(span()).unwrap(), None);
    }


    #[test]
    fn sequence_dsl_api_adopts_returned_streams_and_preserves_effect_checks() {
        std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
            let source = r#"
proc records() -> Result[Stream[Result[Any, CodecError]], CodecError] {
  codec.decode_all("json", "1\nbad\n2\n")
}
let values = records()? |> collect()
assert values.len() == 3
assert values[0]? == 1
assert values[2]? == 2
match values[1] {
  Err(CodecError.Syntax {value_path, ..}) => assert value_path == "[2]"
  else => assert false, "missing line error"
}
"#;
            let mut sources = crate::source::SourceMap::default();
            let source_id = sources.add_file("sequence-owner.xsh", source.to_owned());
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let result = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).eval(&parsed.arena, source_id);
            assert_eq!(result.status, 0, "{:?}: {:?}", result.diagnostics, result.stderr);
            let source = "proc denied(path: Path) [] -> Result[Stream[Result[Any, CodecError]]] { codec.read_all(\"json\", path) }\n";
            let mut sources = crate::source::SourceMap::default();
            let source_id = sources.add_file("sequence-effects.xsh", source.to_owned());
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = crate::sema::check::Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckEffectViolation)), "{:?}", checked.diagnostics);
        }).unwrap().join().unwrap();
    }

    #[test]
    fn sequence_one_byte_chunks_preserve_utf8_and_quoted_csv_record_boundaries() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _scope = symbols.enter();
        for (format, bytes, expected) in [
            (Format::Json, "\"café\"\n".as_bytes(), Value::Str("café".into())),
            (Format::Csv, "name\n\"café\nnext\"\n".as_bytes(), Value::Record([("name".into(), Value::Str("café\nnext".into()))].into())),
        ] {
            let stream = from_reader(format, Probe { input: Cursor::new(bytes.to_vec()), reads: Arc::new(AtomicUsize::new(0)), drops: Arc::new(AtomicUsize::new(0)), chunk: 1 }, None, span()).unwrap();
            assert_eq!(stream.next_live(span()).unwrap(), Some(Value::ok(expected)));
            assert_eq!(stream.next_live(span()).unwrap(), None);
        }
    }

    #[test]
    fn sequence_input_cutoff_preserves_every_in_budget_prefix_byte() {
        let mut input = CountedInput { inner: Cursor::new(b"1\n2\n3\n"), bytes: STREAM_INPUT_LIMIT - 4, exceeded: false, io_error: None };
        let mut output = [0u8; 8];
        assert_eq!(input.read(&mut output).unwrap(), 4);
        assert_eq!(&output[..4], b"1\n2\n");
        assert!(input.read(&mut output).is_err());
        assert!(input.exceeded);
        assert_eq!(input.bytes, STREAM_INPUT_LIMIT + 1);
    }

    #[test]
    fn sequence_eof_probe_host_error_is_not_replaced_by_the_item_limit() {
        struct FailedRead;
        impl Read for FailedRead {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> { Err(std::io::Error::from_raw_os_error(libc::EIO)) }
        }
        let symbols = crate::symbol::SymbolOwner::new();
        let _scope = symbols.enter();
        let config = deser_json::DeserializerConfig::builder().trailing(deser_json::Trailing::Newline).context(super::super::decode_context(Format::Json)).build();
        let input = CountedInput { inner: FailedRead, bytes: ITEM_LIMIT * 2, exceeded: false, io_error: None };
        let parser = Framed { inner: deser_json::StreamDeserializer::with_config(config), fatal: false, encoding: false, value_start: None, items: ITEM_LIMIT };
        let mut sequence = Sequence { reader: Some(deser::io::Reader::new(input, parser)), format: Format::Json, path: None, items: ITEM_LIMIT, budget: EmittedBudget::new(Format::Json, None, span()) };
        let error = sequence.next(span()).unwrap_err();
        assert_eq!(error.errno(), Some(i64::from(libc::EIO)));
        assert!(sequence.reader.is_none());
    }

    #[test]
    fn sequence_yaml_alias_replay_budget_is_cumulative_across_documents() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _scope = symbols.enter();
        let mut text = String::from("--- [0, 0, 0]\n");
        for count in [6000, 4000, 1] {
            text.push_str("--- [&a 0");
            for _ in 0..count { text.push_str(", *a"); }
            text.push_str("]\n");
        }
        let stream = decode_text(Format::Yaml, text.into(), span()).unwrap();
        for _ in 0..3 { assert!(matches!(stream.next_live(span()).unwrap(), Some(Value::Result(ResultValue::Ok(_))))); }
        let Some(Value::Result(ResultValue::Err(error))) = stream.next_live(span()).unwrap() else { panic!("cumulative alias limit") };
        let Value::Error(error) = *error else { panic!("codec error") };
        assert_eq!(error.variant, "Limit");
        assert_eq!(error.payload.get("value_path"), Some(&Value::Str("[4]".into())));
        assert_eq!(stream.next_live(span()).unwrap(), None);
    }

}

use super::{Decoded, EncodeLimits, Format, INPUT_LIMIT, OUTPUT_LIMIT, Output, failure, runtime_error};
use crate::map_key::MapKey;
use crate::runtime::value::{LiveStream, PathValue, ResultValue, RuntimeError, StreamValue, Value};
use crate::source::Span;
use deser::de::{DeserializeDriver, Frame, Progress, StreamDeserializer};
use deser::{Error, ErrorKind, Serialize};
use std::io::{Read, Cursor};
use std::sync::Arc;

const STREAM_INPUT_LIMIT: usize = 256 * 1024 * 1024;
const ITEM_LIMIT: usize = 1_000_000;
const FRAME_LOOKAHEAD: usize = 16;

struct TextInput { text: Arc<str>, position: usize }
impl Read for TextInput {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let count = output.len().min(self.text.len() - self.position);
        output[..count].copy_from_slice(&self.text.as_bytes()[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

pub(crate) fn decode_text(format: Format, text: Arc<str>, span: Span) -> Result<StreamValue, RuntimeError> {
    if format.is_binary() { return Err(failure("UnsupportedFormat", format.identifier(), "binary sequences require Bytes", span)); }
    if text.len() > INPUT_LIMIT { return Err(failure("Limit", format.identifier(), "codec input exceeds 64 MiB", span)); }
    from_reader(format, TextInput { text, position: 0 }, None, span)
}

pub(crate) fn decode_bytes(format: Format, bytes: Arc<[u8]>, span: Span) -> Result<StreamValue, RuntimeError> {
    if bytes.len() > INPUT_LIMIT { return Err(failure("Limit", format.identifier(), "codec input exceeds 64 MiB", span)); }
    from_reader(format, Cursor::new(bytes), None, span)
}

pub(crate) fn read_all(format: Format, file: std::fs::File, path: PathValue, span: Span) -> Result<StreamValue, RuntimeError> {
    from_reader(format, file, Some(path), span)
}

struct CountedInput<R> { inner: R, bytes: usize, exceeded: bool, io_error: Option<std::io::Error> }
impl<R: Read> Read for CountedInput<R> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if output.is_empty() { return Ok(0); }
        if self.exceeded { return Err(std::io::Error::other("codec input limit")); }
        let remaining = STREAM_INPUT_LIMIT - self.bytes;
        let mut sentinel = [0u8; 1];
        let result = if remaining == 0 {
            self.inner.read(&mut sentinel)
        } else {
            let allowed = output.len().min(8192).min(remaining);
            self.inner.read(&mut output[..allowed])
        };
        match result {
            Ok(count) => {
                self.bytes += count;
                if self.bytes > STREAM_INPUT_LIMIT { self.exceeded = true; Err(std::io::Error::other("codec input limit")) } else { Ok(count) }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => Err(error),
            Err(error) => { self.io_error = Some(error); Err(std::io::Error::other("codec input read failed")) }
        }
    }
}

struct Framed<D> { inner: D, fatal: bool, encoding: bool, value_start: Option<usize>, items: usize }
impl<D: StreamDeserializer> StreamDeserializer for Framed<D> {
    fn context(&self) -> deser::Context { self.inner.context() }
    fn is_text(&self) -> bool { self.inner.is_text() }
    fn frame(&mut self, input: &[u8], eof: bool) -> Result<Frame, Error> {
        self.encoding = false;
        let result = self.inner.frame(input, eof);
        let frame = match result {
            Ok(frame) => frame,
            Err(error) => {
                self.fatal = true;
                self.encoding = self.is_text() && std::error::Error::source(&error)
                    .is_some_and(|source| source.is::<std::str::Utf8Error>());
                return Err(error);
            }
        };
        if matches!(frame, Frame::Value { .. }) {
            self.items += 1;
            if self.items > ITEM_LIMIT { self.fatal = true; return Err(Error::new(ErrorKind::LimitExceeded, "codec sequence exceeds 1000000 items")); }
        }
        let (length, consumed) = match &frame {
            Frame::Value { start, end, consumed } => (end - start, *consumed),
            Frame::Incomplete { consumed } if *consumed > 0 => (*consumed, *consumed),
            Frame::Incomplete { .. } => (input.len().saturating_sub(FRAME_LOOKAHEAD), 0),
            Frame::End => (0, input.len()),
        };
        if length > INPUT_LIMIT { self.fatal = true; return Err(Error::new(ErrorKind::LimitExceeded, "codec frame exceeds 64 MiB")); }
        if self.is_text() && consumed > 0 && !matches!(frame, Frame::Value { .. }) {
            if let Err(error) = std::str::from_utf8(&input[..consumed]) {
                self.encoding = true;
                self.fatal = true;
                return Err(Error::with_offset(ErrorKind::Syntax, "invalid UTF-8", error.valid_up_to()));
            }
        }
        Ok(frame)
    }
    fn drive_frame<'de>(&mut self, frame: &'de [u8], driver: &mut DeserializeDriver<'_, 'de>) -> Result<(), Error> {
        if self.is_text() {
            if let Err(error) = std::str::from_utf8(frame) {
                self.encoding = true;
                return Err(Error::with_offset(ErrorKind::Syntax, "invalid UTF-8", error.valid_up_to()));
            }
        }
        self.inner.drive_frame(frame, driver)
    }
    fn supports_partial(&self) -> bool { self.inner.supports_partial() }
    fn peek(&mut self, input: &[u8], eof: bool) -> Result<Option<Progress>, Error> { self.inner.peek(input, eof) }
    fn drive_partial(&mut self, input: &[u8], offset: usize, eof: bool, driver: &mut DeserializeDriver<'_, '_>) -> Result<Progress, Error> {
        if self.value_start.is_none() && !input.is_empty() {
            self.items += 1;
            if self.items > ITEM_LIMIT { self.fatal = true; return Err(Error::new(ErrorKind::LimitExceeded, "codec sequence exceeds 1000000 items")); }
            self.value_start = Some(offset);
        }
        let start = self.value_start.unwrap_or(offset);
        let progress = self.inner.drive_partial(input, offset, eof, driver)?;
        let consumed = match progress { Progress::Done { consumed } | Progress::NeedMore { consumed } => consumed, Progress::End => 0 };
        if offset + consumed - start > INPUT_LIMIT {
            self.fatal = true;
            return Err(Error::new(ErrorKind::LimitExceeded, "codec value exceeds 64 MiB"));
        }
        if matches!(progress, Progress::Done { .. } | Progress::End) { self.value_start = None; }
        Ok(progress)
    }
}

struct Sequence<R, D: StreamDeserializer> {
    reader: Option<deser::io::Reader<CountedInput<R>, Framed<D>>>,
    format: Format,
    path: Option<PathValue>,
    items: usize,
    budget: EmittedBudget,
}

fn from_reader<R: Read + Send + 'static>(format: Format, input: R, path: Option<PathValue>, span: Span) -> Result<StreamValue, RuntimeError> {
    fn source<R: Read + Send + 'static, D: StreamDeserializer + Send + 'static>(format: Format, input: R, path: Option<PathValue>, parser: D, span: Span) -> StreamValue {
        let input = CountedInput { inner: input, bytes: 0, exceeded: false, io_error: None };
        let mut reader = deser::io::Reader::new(input, Framed { inner: parser, fatal: false, encoding: false, value_start: None, items: 0 });
        reader.set_input_buffer_limit(INPUT_LIMIT + FRAME_LOOKAHEAD);
        let budget = EmittedBudget::new(format, path.as_ref(), span);
        StreamValue::from_live("codec sequence", Sequence { reader: Some(reader), format, path, items: 0, budget })
    }
    Ok(match format {
        Format::Json => {
            let config = deser_json::DeserializerConfig::builder().trailing(deser_json::Trailing::Newline).context(super::decode_context(format)).build();
            source(format, input, path, deser_json::StreamDeserializer::with_config(config), span)
        }
        Format::Yaml => source(format, input, path, deser_yaml::StreamDeserializer::with_config(super::config::yaml_sequence_config()), span),
        Format::Csv | Format::Tsv => source(format, input, path, deser_csv::StreamDeserializer::with_config(super::table::decode_config(format, super::table::DecodeOptions::defaults(format))), span),
        Format::Cbor => source(format, input, path, deser_cbor::StreamDeserializer::with_config(super::binary::cbor_config()), span),
        Format::Msgpack => source(format, input, path, deser_msgpack::StreamDeserializer::with_config(super::binary::msgpack_config()), span),
        _ => return Err(failure("UnsupportedFormat", format.identifier(), "format does not support sequences", span)),
    })
}

impl<R: Read + Send + 'static, D: StreamDeserializer + Send + 'static> LiveStream for Sequence<R, D> {
    fn next(&mut self, span: Span) -> Result<Option<Value>, RuntimeError> {
        let Some(reader) = self.reader.as_mut() else { return Ok(None) };
        let format = self.format;
        let result = reader.read_with::<Decoded, _>(|driver| match format {
            Format::Yaml => super::config::setup_decode(format, driver),
            Format::Csv | Format::Tsv => super::table::setup_decode(driver),
            Format::Cbor | Format::Msgpack => super::binary::setup_decode(format, driver),
            _ => {}
        });
        if matches!(result, Ok(None)) { self.reader.take(); return Ok(None); }
        self.items += 1;
        let mut terminal = matches!(format, Format::Yaml | Format::Cbor | Format::Msgpack);
        let item = if reader.get_ref().exceeded {
            terminal = true;
            self.budget.limit_error(self.items)
        } else if let Some(error) = reader.get_mut().io_error.take() {
            self.reader.take();
            return Err(match self.path.as_ref() {
                Some(path) => {
                    use std::os::unix::ffi::OsStrExt;
                    super::input_io_error(std::path::Path::new(std::ffi::OsStr::from_bytes(&path.bytes)), error, span)
                }
                None => RuntimeError::host("read", &error).with_span(span),
            });
        } else if self.items > ITEM_LIMIT {
            terminal = true;
            self.budget.limit_error(self.items)
        } else {
            match result {
                Ok(Some(value)) => { terminal = false; Value::ok(value.0) }
                Err(error) => {
                    terminal |= reader.deserializer().fatal || error.kind() == ErrorKind::LimitExceeded;
                    let encoding = reader.deserializer().encoding;
                    let mut error = runtime_error(error, format.identifier(), span);
                    if encoding {
                        error = RuntimeError::structured("CodecError", "Encoding", error.payload, vec!["InvalidData".into()], error.message).with_span(span);
                    }
                    attribute(&mut error, self.path.as_ref(), self.items);
                    Value::err(Value::Error(Box::new(error)))
                }
                Ok(None) => unreachable!("exhaustion handled"),
            }
        };
        let item = if self.budget.accept(&item) { item } else { terminal = true; self.budget.limit_error(self.items) };
        if terminal { self.reader.take(); }
        Ok(Some(item))
    }
}

fn attribute(error: &mut RuntimeError, path: Option<&PathValue>, item: usize) {
    if let Some(path) = path { error.payload.insert("input_path".into(), Value::Path(path.clone())); }
    error.payload.insert("value_path".into(), Value::Str(format!("[{item}]").into()));
}

struct EmittedBudget { used: u64, reserve: u64, format: Format, path: Option<PathValue>, span: Span }
impl EmittedBudget {
    fn new(format: Format, path: Option<&PathValue>, span: Span) -> Self {
        let mut budget = Self { used: 0, reserve: 0, format, path: path.cloned(), span };
        budget.reserve = emitted_cost(&budget.limit_error(ITEM_LIMIT + 1));
        budget
    }
    fn limit_error(&self, item: usize) -> Value {
        let mut error = failure("Limit", self.format.identifier(), "codec stream resource limit exceeded", self.span);
        attribute(&mut error, self.path.as_ref(), item);
        Value::err(Value::Error(Box::new(error)))
    }
    fn accept(&mut self, value: &Value) -> bool {
        match self.used.checked_add(emitted_cost(value)) {
            Some(total) if total <= OUTPUT_LIMIT as u64 - self.reserve => { self.used = total; true }
            _ => false,
        }
    }
}

fn emitted_cost(value: &Value) -> u64 {
    fn text(value: &str) -> u64 { 9 + value.len() as u64 }
    fn key(key: &MapKey) -> u64 {
        match key {
            MapKey::Str(value) => text(value),
            MapKey::Bytes(value) | MapKey::Path(value) => 9 + value.len() as u64,
            MapKey::Bool(_) => 2,
            MapKey::Int(_) | MapKey::Duration(_) => 9,
        }
    }
    fn record(record: &crate::runtime::value::RecordMap) -> u64 {
        9 + record.iter().map(|(name, value)| text(name) + emitted_cost(value)).sum::<u64>()
    }
    match value {
        Value::Null | Value::Unit => 1,
        Value::Bool(_) => 2,
        Value::Int(_) | Value::Float(_) | Value::Duration(_) => 9,
        Value::Str(value) => text(value),
        Value::Bytes(value) => 9 + value.len() as u64,
        Value::Path(value) => 9 + value.bytes.len() as u64,
        Value::List(values) => 9 + values.iter().map(emitted_cost).sum::<u64>(),
        Value::Map(values) => 9 + values.iter().map(|(name, value)| key(name) + emitted_cost(value)).sum::<u64>(),
        Value::Record(value) => record(value),
        Value::Result(ResultValue::Ok(value) | ResultValue::Err(value)) => 1 + emitted_cost(value),
        Value::Error(error) => 1 + text(&error.family) + text(&error.variant) + text(&error.kind) + text(&error.message) + record(&error.payload) + 9 + error.facets.iter().map(|facet| text(facet)).sum::<u64>(),
        _ => unreachable!("decoded codec value domain"),
    }
}

pub(crate) fn encode_all<I, T>(format: Format, values: I, span: Span) -> Result<Vec<u8>, RuntimeError>
where I: IntoIterator<Item = T>, T: Serialize {
    fn encode<I, T, S>(format: Format, values: I, serializer: S, span: Span) -> Result<Vec<u8>, RuntimeError>
    where I: IntoIterator<Item = T>, T: Serialize, S: deser::ser::StreamSerializer {
        let mut output = Output::new();
        let result = (|| {
            let mut writer = deser::io::Writer::new(&mut output, serializer);
            for (index, value) in values.into_iter().enumerate() {
                if index >= ITEM_LIMIT { return Err(Error::new(ErrorKind::LimitExceeded, "codec sequence exceeds 1000000 items")); }
                writer.write_with(&value, |driver| {
                    if format == Format::Yaml { super::config::setup_encode(format, driver); }
                    else { driver.push_layer(EncodeLimits::default()); }
                })?;
            }
            Ok(())
        })();
        output.finish_bytes(result).map_err(|error| runtime_error(error, format.identifier(), span))
    }
    match format {
        Format::Json => {
            let config = deser_json::SerializerConfig::builder().trailing(deser_json::Trailing::Newline).build();
            encode(format, values, deser_json::Serializer::with_config(config), span)
        }
        Format::Yaml => {
            let mut config = deser_yaml::SerializerConfig::new();
            config.set_end_documents(true);
            encode(format, values, deser_yaml::Serializer::with_config(config), span)
        }
        Format::Csv | Format::Tsv => super::table::encode_all(format, values, span),
        Format::Cbor => encode(format, values, deser_cbor::Serializer::new(), span),
        Format::Msgpack => encode(format, values, deser_msgpack::Serializer::new(), span),
        _ => Err(failure("UnsupportedFormat", format.identifier(), "format does not support sequences", span)),
    }
}
