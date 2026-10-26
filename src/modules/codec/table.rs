use super::{EncodeLimits, Format, Output, charge_scalar_bytes, decode_context, failure, runtime_error};
use crate::runtime::value::RuntimeError;
use crate::source::Span;
use deser::de::{DeserializeDriver, Layer as DecodeLayer};
use deser::ser::Layer as EncodeLayer;
use deser::{Atom, Error, ErrorKind, Event, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub(crate) struct DecodeOptions {
    pub(crate) headers: bool,
    pub(crate) delimiter: u8,
}

impl DecodeOptions {
    pub(crate) fn defaults(format: Format) -> Self {
        Self {
            headers: true,
            delimiter: match format {
                Format::Csv => b',',
                Format::Tsv => b'\t',
                _ => unreachable!("table format"),
            },
        }
    }
}

pub(crate) struct EncodeOptions {
    pub(crate) headers: bool,
    pub(crate) delimiter: u8,
    pub(crate) columns: Option<Vec<String>>,
    pub(crate) escape_formulas: bool,
}

impl EncodeOptions {
    pub(crate) fn defaults(format: Format) -> Self {
        Self {
            headers: true,
            delimiter: match format {
                Format::Csv => b',',
                Format::Tsv => b'\t',
                _ => unreachable!("table format"),
            },
            columns: None,
            escape_formulas: false,
        }
    }
}

pub(crate) fn parse_delimiter(value: &str, span: Span) -> Result<u8, RuntimeError> {
    let bytes = value.as_bytes();
    match bytes {
        [delimiter] if delimiter.is_ascii() && !matches!(*delimiter, 0 | b'\r' | b'\n' | b'"') => Ok(*delimiter),
        _ => Err(failure(
            "Options",
            Format::Csv.identifier(),
            "CSV delimiter must be one ASCII byte other than NUL, CR, LF, or quote",
            span,
        )),
    }
}

pub(super) fn decode_config(format: Format, options: DecodeOptions) -> deser_csv::DeserializerConfig {
    let mut config = match format {
        Format::Csv => deser_csv::DeserializerConfig::new(),
        Format::Tsv => deser_csv::DeserializerConfig::tsv(),
        _ => unreachable!("table format"),
    };
    config.set_delimiter(options.delimiter);
    config.set_headers(if options.headers {
        deser_csv::Headers::First
    } else {
        deser_csv::Headers::None
    });
    config.set_context(decode_context(format));
    config
}

pub(super) fn setup_decode(driver: &mut DeserializeDriver<'_, '_>) {
    driver.push_layer(DecodeGuard { scalar_bytes: 0 });
}

pub(crate) fn drive<'de>(
    format: Format,
    text: &'de str,
    options: DecodeOptions,
    driver: &mut DeserializeDriver<'_, 'de>,
    span: Span,
) -> Result<(), RuntimeError> {
    if !matches!(format, Format::Csv | Format::Tsv) {
        return Err(failure("UnsupportedValue", format.identifier(), "table format required", span));
    }
    if text.len() > super::INPUT_LIMIT {
        return Err(runtime_error(Error::new(ErrorKind::LimitExceeded, "codec input exceeds 64 MiB"), format.identifier(), span));
    }
    validate_delimiter(format, options.delimiter, span)?;
    let config = decode_config(format, options);
    setup_decode(driver);
    let mut parser = deser_csv::Deserializer::from_str_with_config(text, config);
    parser
        .drive(driver)
        .map_err(|error| table_error(error, format, span))
}

pub(crate) fn encode<T: Serialize + ?Sized>(
    format: Format,
    value: &T,
    options: EncodeOptions,
    span: Span,
) -> Result<String, RuntimeError> {
    if !matches!(format, Format::Csv | Format::Tsv) {
        return Err(failure("UnsupportedValue", format.identifier(), "table format required", span));
    }
    validate_delimiter(format, options.delimiter, span)?;
    let columns = options.columns.as_deref();
    let explicit_columns = columns.is_some();
    let column_indices = match columns {
        Some(columns) => Some(column_indices(columns, format, span)?),
        None => None,
    };
    let mut config = match format {
        Format::Csv => deser_csv::SerializerConfig::new(),
        Format::Tsv => deser_csv::SerializerConfig::tsv(),
        _ => unreachable!("table format"),
    };
    config.set_delimiter(options.delimiter);
    config.set_headers(options.headers);
    config.set_escape_formulas(options.escape_formulas);
    let serializer = match columns {
        Some(columns) => deser_csv::Serializer::document_with_headers(
            config,
            columns.iter().map(String::as_str),
        ),
        None => deser_csv::Serializer::document(config),
    };
    let mut output = Output::new();
    let result = {
        let mut writer = deser::io::Writer::new(&mut output, serializer);
        writer.write_with(value, |driver| {
            driver.push_layer(EncodeLimits::default());
            driver.push_layer(EncodeGuard::new(column_indices, explicit_columns));
        })
    };
    output
        .finish(result)
        .map_err(|error| table_error(error, format, span))
}

struct DecodeGuard {
    scalar_bytes: usize,
}

impl DecodeLayer for DecodeGuard {
    fn event<'de>(
        &mut self,
        event: deser::de::LayerEvent<'_, 'de>,
        next: &mut deser::de::Next<'_, 'de>,
    ) -> Result<(), Error> {
        charge_scalar_bytes(&mut self.scalar_bytes, event.event())?;
        next.emit(event)
    }
}

struct EncodeGuard {
    columns: Option<HashMap<String, usize>>,
    seen: Vec<bool>,
    explicit_columns: bool,
    collecting_columns: bool,
    row_open: bool,
    row_kind: Option<bool>,
    root_seen: bool,
}

impl EncodeGuard {
    fn new(columns: Option<HashMap<String, usize>>, explicit_columns: bool) -> Self {
        let seen = vec![false; columns.as_ref().map_or(0, HashMap::len)];
        Self {
            columns,
            seen,
            explicit_columns,
            collecting_columns: false,
            row_open: false,
            row_kind: None,
            root_seen: false,
        }
    }

    fn event(&mut self, event: &Event<'_>, depth: usize, is_map_key: bool) -> Result<(), Error> {
        if !self.root_seen {
            if matches!(event, Event::SeqStart(_)) && depth == 1 {
                self.root_seen = true;
            } else {
                return Err(Error::new(ErrorKind::UnsupportedType, "CSV documents are sequences of records"));
            }
        }
        if depth == 1 && !is_map_key && matches!(event, Event::Atom(_)) {
            return Err(Error::new(ErrorKind::UnsupportedType, "CSV records must be maps or sequences"));
        }
        if depth == 2 && !self.row_open {
            match event {
                Event::MapStart(_) => {
                    if self.row_kind == Some(false) {
                        return Err(Error::new(ErrorKind::UnsupportedType, "CSV records must use one row shape"));
                    }
                    self.row_kind = Some(true);
                    if self.columns.is_some() {
                        self.seen.fill(false);
                    } else {
                        self.columns = Some(HashMap::new());
                        self.seen.clear();
                        self.collecting_columns = true;
                    }
                    self.row_open = true;
                }
                Event::SeqStart(_) => {
                    if self.explicit_columns {
                        return Err(Error::new(ErrorKind::UnsupportedType, "explicit CSV columns require record rows"));
                    }
                    if self.row_kind == Some(true) {
                        return Err(Error::new(ErrorKind::UnsupportedType, "CSV records must use one row shape"));
                    }
                    self.row_kind = Some(false);
                }
                _ => {}
            }
        }
        if self.row_open && depth == 2 && is_map_key {
            let key = match event {
                Event::Atom(Atom::Str(key) | Atom::Lexical(key)) => key.as_ref(),
                _ => return Err(Error::new(ErrorKind::UnsupportedType, "CSV record keys must be strings")),
            };
            if self.collecting_columns {
                let columns = self.columns.as_mut().expect("inferred columns");
                let index = columns.len();
                if columns.insert(key.to_owned(), index).is_some() {
                    return Err(Error::new(ErrorKind::DuplicateKey, format!("duplicate field `{key}`")));
                }
                self.seen.push(true);
            } else if let Some(columns) = &self.columns {
                let Some(&index) = columns.get(key) else {
                    return Err(Error::new(ErrorKind::UnsupportedType, format!("field `{key}` is not a column")));
                };
                if self.seen[index] {
                    return Err(Error::new(ErrorKind::DuplicateKey, format!("duplicate field `{key}`")));
                }
                self.seen[index] = true;
            }
        }
        if self.row_open && depth == 1 && matches!(event, Event::MapEnd) {
            if !self.collecting_columns && self.columns.is_some() {
                if let Some((index, _)) = self.seen.iter().enumerate().find(|(_, seen)| !**seen) {
                    let name = self.columns.as_ref().expect("table columns").iter().find_map(|(key, value)| (*value == index).then_some(key)).expect("column index");
                    return Err(Error::new(ErrorKind::UnsupportedType, format!("record is missing column `{name}`")));
                }
            }
            self.collecting_columns = false;
            self.row_open = false;
        }
        if matches!(event, Event::Atom(Atom::F64(value)) if !value.is_finite())
            || matches!(event, Event::Atom(Atom::F32(value)) if !value.is_finite())
            || matches!(event, Event::Atom(Atom::Ext(_)))
        {
            return Err(Error::new(ErrorKind::UnsupportedType, "CSV cannot represent this scalar"));
        }
        Ok(())
    }
}

impl EncodeLayer for EncodeGuard {
    fn event(&mut self, event: Event<'_>, next: &mut deser::ser::Next<'_>) -> Result<(), Error> {
        self.event(&event, next.state().depth(), next.state().is_map_key())?;
        next.emit(event)
    }
}

struct RowGuard(std::sync::Arc<std::sync::Mutex<EncodeGuard>>);

impl EncodeLayer for RowGuard {
    fn event(&mut self, event: Event<'_>, next: &mut deser::ser::Next<'_>) -> Result<(), Error> {
        self.0.lock().expect("CSV row guard owner").event(&event, next.state().depth() + 1, next.state().is_map_key())?;
        next.emit(event)
    }
}

pub(super) fn encode_all<I, T>(format: Format, values: I, span: Span) -> Result<Vec<u8>, RuntimeError>
where I: IntoIterator<Item = T>, T: Serialize {
    let config = match format {
        Format::Csv => deser_csv::SerializerConfig::new(),
        Format::Tsv => deser_csv::SerializerConfig::tsv(),
        _ => unreachable!("table format"),
    };
    let mut guard = EncodeGuard::new(None, false);
    guard.root_seen = true;
    let guard = std::sync::Arc::new(std::sync::Mutex::new(guard));
    let mut output = Output::new();
    let result = (|| {
        let mut writer = deser::io::Writer::new(&mut output, deser_csv::Serializer::with_config(config));
        for (index, value) in values.into_iter().enumerate() {
            if index >= 1_000_000 { return Err(Error::new(ErrorKind::LimitExceeded, "codec sequence exceeds 1000000 records")); }
            writer.write_with(&value, |driver| {
                driver.push_layer(EncodeLimits::default());
                driver.push_layer(RowGuard(guard.clone()));
            })?;
        }
        Ok(())
    })();
    output.finish_bytes(result).map_err(|error| table_error(error, format, span))
}

fn column_indices(columns: &[String], format: Format, span: Span) -> Result<HashMap<String, usize>, RuntimeError> {
    if columns.len() > 1_000_000 {
        return Err(runtime_error(
            Error::new(ErrorKind::LimitExceeded, "codec container exceeds 1000000 entries"),
            format.identifier(),
            span,
        ));
    }
    let mut indices = HashMap::with_capacity(columns.len());
    let mut payload_bytes = 0usize;
    for (index, column) in columns.iter().enumerate() {
        if column.len() > 16 * 1024 * 1024 {
            return Err(runtime_error(
                Error::new(ErrorKind::LimitExceeded, "codec string exceeds 16 MiB"),
                format.identifier(),
                span,
            ));
        }
        payload_bytes = payload_bytes
            .checked_add(column.len())
            .filter(|total| *total <= super::OUTPUT_LIMIT)
            .ok_or_else(|| runtime_error(Error::new(ErrorKind::LimitExceeded, "codec scalar payload exceeds 128 MiB"), format.identifier(), span))?;
        if indices.insert(column.clone(), index).is_some() {
            return Err(failure("Options", format.identifier(), format!("duplicate column `{column}`"), span));
        }
    }
    Ok(indices)
}

fn validate_delimiter(format: Format, delimiter: u8, span: Span) -> Result<(), RuntimeError> {
    if delimiter.is_ascii() && !matches!(delimiter, 0 | b'\r' | b'\n' | b'"') {
        Ok(())
    } else {
        Err(failure(
            "Options",
            format.identifier(),
            "delimiter must be one ASCII byte other than NUL, CR, LF, or quote",
            span,
        ))
    }
}

fn table_error(error: Error, format: Format, span: Span) -> RuntimeError {
    if error.kind() == ErrorKind::Configuration {
        failure("Options", format.identifier(), error.message().to_string(), span)
    } else {
        runtime_error(error, format.identifier(), span)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::Decoded;
    use crate::runtime::value::Value;
    use crate::source::SourceId;
    use deser::de::Deserialize;

    fn span() -> Span {
        Span::new(SourceId::new(0), 0, 1)
    }

    fn decode(format: Format, text: &str, options: DecodeOptions) -> Result<Value, RuntimeError> {
        let mut output = None;
        let mut driver = DeserializeDriver::from_fn(|state| Decoded::deserialize_into(&mut output, state));
        drive(format, text, options, &mut driver, span())?;
        output.map(|value| value.0).ok_or_else(|| failure("Syntax", format.identifier(), "empty table input", span()))
    }

    #[test]
    fn csv_decodes_records_or_headerless_rows_and_rejects_table_defects() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _scope = symbols.enter();
        let decoded = decode(Format::Csv, "name,age\nAda,37\n", DecodeOptions::defaults(Format::Csv)).unwrap();
        assert_eq!(decoded, Value::List(vec![Value::Record([("name".into(), Value::Str("Ada".into())), ("age".into(), Value::Str("37".into()))].into())]));
        let rows = decode(Format::Csv, "name,age\nAda,37\n", DecodeOptions { headers: false, delimiter: b',' }).unwrap();
        assert_eq!(rows, Value::List(vec![Value::List(vec![Value::Str("name".into()), Value::Str("age".into())]), Value::List(vec![Value::Str("Ada".into()), Value::Str("37".into())])]));
        for (text, variant) in [("a,a\n", "DuplicateKey"), ("a,b\n1\n", "Syntax"), ("a\n\"x\"tail\n", "Syntax")] {
            assert_eq!(decode(Format::Csv, text, DecodeOptions::defaults(Format::Csv)).unwrap_err().variant, variant, "{text}");
        }
    }

    #[test]
    fn tsv_uses_backslash_escaping_and_text_nulls() {
        let value = decode(Format::Tsv, "name\tvalue\nAda\tx\\ty\n", DecodeOptions::defaults(Format::Tsv)).unwrap();
        let Value::List(rows) = value else { panic!("rows") };
        let Value::Record(fields) = &rows[0] else { panic!("record") };
        assert_eq!(fields.get("value"), Some(&Value::Str("x\ty".into())));
        let value = decode(Format::Tsv, "name\tvalue\nAda\t\\N\n", DecodeOptions::defaults(Format::Tsv)).unwrap();
        let Value::List(rows) = value else { panic!("rows") };
        let Value::Record(fields) = &rows[0] else { panic!("record") };
        assert_eq!(fields.get("value"), Some(&Value::Null));
    }

    #[test]
    fn explicit_columns_drive_order_formula_escape_and_exact_keysets() {
        let value = vec![
            bbtree::unsync::BBTreeMap::from([("a", "one"), ("b", "two")]),
            bbtree::unsync::BBTreeMap::from([("a", "three"), ("b", "four")]),
        ];
        let output = encode(Format::Csv, &value, EncodeOptions { headers: true, delimiter: b',', columns: Some(vec!["b".into(), "a".into()]), escape_formulas: false }, span()).unwrap();
        assert_eq!(output, "b,a\ntwo,one\nfour,three\n");
        let invalid = vec![bbtree::unsync::BBTreeMap::from([("a", "one")])];
        assert_eq!(encode(Format::Csv, &invalid, EncodeOptions { headers: true, delimiter: b',', columns: Some(vec!["a".into(), "b".into()]), escape_formulas: false }, span()).unwrap_err().variant, "UnsupportedValue");
        let missing = vec![
            bbtree::unsync::BBTreeMap::from([("a", "one"), ("b", "two")]),
            bbtree::unsync::BBTreeMap::from([("a", "three")]),
        ];
        assert_eq!(encode(Format::Csv, &missing, EncodeOptions::defaults(Format::Csv), span()).unwrap_err().variant, "UnsupportedValue");
        let extra = vec![
            bbtree::unsync::BBTreeMap::from([("a", "one"), ("b", "two")]),
            bbtree::unsync::BBTreeMap::from([("a", "three"), ("b", "four"), ("c", "five")]),
        ];
        assert_eq!(encode(Format::Csv, &extra, EncodeOptions::defaults(Format::Csv), span()).unwrap_err().variant, "UnsupportedValue");
        let formulas = vec![bbtree::unsync::BBTreeMap::from([("value", "=1+2")])];
        assert_eq!(encode(Format::Csv, &formulas, EncodeOptions { headers: true, delimiter: b',', columns: None, escape_formulas: true }, span()).unwrap(), "value\n\"'=1+2\"\n");
    }
}
