//! Fixture coverage for the host-text policy the embedded modules own.
//!
//! The entries that read `/proc/meminfo`, `/proc/uptime`, `/proc/modules`, and
//! the `os-release` files take their text from fixed paths, so the reading
//! boundary itself is exercised by the corpus on Linux. The *interpretation* of
//! that text — quoting and escaping, duplicate keys, unit and field rules, error
//! precedence, saturation, and the integer spellings the host parsers accept —
//! is what these tests pin down, with the text held in committed fixture files
//! rather than in the test body.
//!
//! Every call goes through [`Evaluator::probe_embedded_call`], the crate-private
//! companion of the catalog test: the module identity is resolved in the
//! compiled catalog, the module passes the ordinary preparation gate, and the
//! call runs the real embedded body through the ordinary verifier. Nothing here
//! is reachable from production or user module loading, and no fixture replaces
//! an implementation.
//!
//! These tests cover the helpers. The public entries' reading boundary — which
//! fixed path is read, in which order, and what a failed read reports for the
//! whole call rather than for one read — is exercised by the Linux container
//! route, not here.

use crate::runtime::eval::{Evaluator, LoweredFunctionKind};
use crate::runtime::value::{PathValue, ResultValue, RuntimeError, Value};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

/// Read a fixture committed under `tests/fixtures/stdlib`.
fn fixture(relative: &str) -> String {
    let path = fixture_path(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// The fixture path itself, for helpers that read a file.
fn fixture_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/stdlib")
        .join(relative)
}

/// A `Str` argument.
fn text(value: &str) -> Value {
    Value::Str(value.into())
}

/// A `Path` argument.
fn path(value: &Path) -> Value {
    Value::Path(PathValue::new(value.as_os_str().as_bytes().to_vec()).expect("fixture path"))
}

fn call_pure(module: &str, function: &str, args: &[Value]) -> Result<Value, RuntimeError> {
    Evaluator::probe_embedded_pure_call(module, function, args)
}

/// The outcome of a `Result` value: the `Ok` payload or the error it carries.
fn outcome(value: Value) -> Result<Value, RuntimeError> {
    match value {
        Value::Result(ResultValue::Ok(inner)) => Ok(*inner),
        Value::Result(ResultValue::Err(inner)) => match *inner {
            Value::Error(error) => Err(*error),
            other => panic!("expected an error value, found {other:?}"),
        },
        other => panic!("expected a Result, found {other:?}"),
    }
}

fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
    match value {
        Value::Record(fields) => fields
            .get(name)
            .unwrap_or_else(|| panic!("record has no field `{name}`")),
        other => panic!("expected a record, found {other:?}"),
    }
}

fn int(value: &Value) -> i64 {
    match value {
        Value::Int(number) => *number,
        other => panic!("expected an Int, found {other:?}"),
    }
}

fn string(value: &Value) -> &str {
    match value {
        Value::Str(text) => text,
        other => panic!("expected a Str, found {other:?}"),
    }
}

/// A list of strings, in order.
fn strings(value: &Value) -> Vec<String> {
    match value {
        Value::List(items) => items.iter().map(|item| string(item).to_string()).collect(),
        other => panic!("expected a List, found {other:?}"),
    }
}

/// The optional `Int` a helper reported, as `Option<i64>`.
fn optional_int(value: &Value) -> Option<i64> {
    match value {
        Value::Null => None,
        other => Some(int(other)),
    }
}

/// The optional `Str` a helper reported, as `Option<String>`.
fn optional_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        other => Some(string(other).to_string()),
    }
}

/// Split a two-column fixture table, skipping comment and empty lines.
fn table(relative: &str) -> Vec<(String, String)> {
    fixture(relative)
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (left, right) = line
                .split_once('\t')
                .unwrap_or_else(|| panic!("fixture line `{line}` is not tab-separated"));
            (left.to_string(), right.to_string())
        })
        .collect()
}

/// Run `parse_field_int` from both modules that carry the spelling rule.
///
/// `system` owns the values in `/proc/meminfo` and `linux_text` owns the values
/// in the text it reads; both implement the same baseline parse, so one table
/// covers both.
fn parse_field_int(spelling: &str) -> Option<i64> {
    let system =
        optional_int(&call_pure("system", "parse_field_int", &[text(spelling)]).unwrap());
    let linux =
        optional_int(&call_pure("linux_text", "parse_field_int", &[text(spelling)]).unwrap());
    assert_eq!(system, linux, "`{spelling}` parses differently per module");
    system
}

/// The `os-release` helpers resolve quoting, escaping, and duplicate keys.
///
/// The text is `tests/fixtures/stdlib/os_release/quoted_and_escaped.txt`: a
/// double-quoted value with an escaped quote and a backslash escape, a
/// single-quoted value, a key the record does not report, a line with no `=`,
/// an indented comment, and two lines for `ID`.
#[test]
fn release_values_resolve_quoting_and_last_line_wins() {
    let release_text = fixture("os_release/quoted_and_escaped.txt");
    let value = |name: &str| {
        optional_string(
            &call_pure(
                "system",
                "release_value",
                &[text(&release_text), text(name)],
            )
            .expect("release_value runs"),
        )
    };
    assert_eq!(value("NAME").as_deref(), Some("Fixture \"Quoted\" Linux"));
    assert_eq!(value("PRETTY_NAME").as_deref(), Some("Single quoted name"));
    assert_eq!(value("VERSION").as_deref(), Some("1.2\\3"));
    assert_eq!(value("VERSION_ID").as_deref(), Some("42"));
    // The last line for a key wins.
    assert_eq!(value("ID").as_deref(), Some("fixture-os-last"));
    // The key is the text before the first `=`, untrimmed, so the spaced
    // spelling is a different key that the text does not carry.
    assert_eq!(value("KEY_WITH_SPACE"), None);
    assert_eq!(value("NO_EQUALS_HERE"), None);

    let record =
        call_pure("system", "os_release_record", &[text(&release_text)]).expect("record runs");
    assert_eq!(field(&record, "name"), &text("Fixture \"Quoted\" Linux"));
    assert_eq!(field(&record, "pretty_name"), &text("Single quoted name"));
    assert_eq!(field(&record, "version_id"), &text("42"));
    assert_eq!(field(&record, "id"), &text("fixture-os-last"));
}

/// The `os-release` record defaults follow the resolved name.
#[test]
fn release_record_defaults_follow_the_resolved_name() {
    let defaults = fixture("os_release/defaults.txt");
    let record =
        call_pure("system", "os_release_record", &[text(&defaults)]).expect("record runs");
    assert_eq!(field(&record, "name"), &text("Linux"));
    assert_eq!(field(&record, "pretty_name"), &text("Linux"));
    assert_eq!(field(&record, "version"), &text(""));
    assert_eq!(field(&record, "version_id"), &text(""));
    assert_eq!(field(&record, "id"), &text("defaults-fixture"));

    let named = fixture("os_release/name_without_pretty.txt");
    let record = call_pure("system", "os_release_record", &[text(&named)]).expect("record runs");
    assert_eq!(field(&record, "name"), &text("Fixture Linux"));
    assert_eq!(field(&record, "pretty_name"), &text("Fixture Linux"));
    assert_eq!(field(&record, "version_id"), &text("9"));

    let empty = fixture("os_release/empty.txt");
    let record = call_pure("system", "os_release_record", &[text(&empty)]).expect("record runs");
    assert_eq!(field(&record, "name"), &text("Linux"));
    assert_eq!(field(&record, "id"), &text("linux"));
}

/// The reader prefers the first path and reports the second path's failure.
///
/// `read_release_text` is the helper the public entry routes its two reads
/// through: a readable first path wins, every failure of the first read falls
/// through to the second, and the failure that is reported for the call is the
/// second read's failure rather than the first's.
#[test]
fn release_text_prefers_the_first_path_and_reports_the_second_failure() {
    let read = |first: &Path, second: &Path| {
        outcome(
            Evaluator::probe_embedded_call(
                "system",
                "read_release_text",
                LoweredFunctionKind::Proc,
                &[path(first), path(second)],
            )
            .expect("read_release_text runs"),
        )
    };

    let first = fixture_path("os_release/quoted_and_escaped.txt");
    let second = fixture_path("os_release/defaults.txt");
    let value = read(&first, &second).expect("the first path is readable");
    assert_eq!(
        string(&value),
        fixture("os_release/quoted_and_escaped.txt")
    );

    // A readable second path answers when the first cannot be read.
    let missing = fixture_path("os_release/does-not-exist.txt");
    let value = read(&missing, &first).expect("the second path is readable");
    assert_eq!(
        string(&value),
        fixture("os_release/quoted_and_escaped.txt")
    );

    // When neither can be read, the reported failure is the second one's. The
    // two failures are made distinguishable: the first path does not exist and
    // the second is a directory, so a message about the missing file would mean
    // the first read was the one reported.
    let directory = fixture_path("os_release");
    let error = read(&missing, &directory).expect_err("neither path is readable");
    assert!(
        !error.message.contains("No such file"),
        "the second read's failure is the reported one: {error:?}"
    );
    assert!(error.message.contains("directory"), "{error:?}");
}

/// `/proc/meminfo` units, duplicate keys, and ignored lines.
///
/// The text is `tests/fixtures/stdlib/meminfo/valid.txt`: a later duplicate of
/// `MemFree`, a line whose unit is not `kB`, a line with too few fields, and a
/// key the record does not report.
#[test]
fn meminfo_parses_units_and_last_line_wins() {
    let meminfo_text = fixture("meminfo/valid.txt");
    let result =
        call_pure("linux_text", "parse_meminfo", &[text(&meminfo_text)]).expect("runs");
    let record = outcome(result).expect("the text holds every reported key");
    assert_eq!(int(field(&record, "total")), 16_384_000 * 1024);
    // The later duplicate wins.
    assert_eq!(int(field(&record, "free")), 2_097_152 * 1024);
    assert_eq!(int(field(&record, "available")), 8_388_608 * 1024);
    assert_eq!(int(field(&record, "buffers")), 262_144 * 1024);
    assert_eq!(int(field(&record, "cached")), 524_288 * 1024);
    assert_eq!(int(field(&record, "swap_total")), 2_097_152 * 1024);
    assert_eq!(int(field(&record, "swap_free")), 1_048_576 * 1024);
}

/// A value that is not an integer fails the call, in file order.
///
/// The first malformed line decides, and a malformed line for a key the record
/// does not report still fails the call, because the whole text is read before
/// any key is looked up.
#[test]
fn meminfo_reports_the_first_malformed_value() {
    let reported = fixture("meminfo/malformed_reported.txt");
    let result = call_pure("linux_text", "parse_meminfo", &[text(&reported)]).expect("runs");
    let error = outcome(result).expect_err("the first line has no integer");
    assert_eq!(error.kind, "linux-meminfo");
    assert!(
        error.message.contains("MemTotal") && error.message.contains("invalid"),
        "{error:?}"
    );

    let unreported = fixture("meminfo/malformed_unreported.txt");
    let result = call_pure("linux_text", "parse_meminfo", &[text(&unreported)]).expect("runs");
    let error = outcome(result).expect_err("`Unrelated` has no integer");
    assert_eq!(error.kind, "linux-meminfo");
    assert!(error.message.contains("Unrelated"), "{error:?}");
}

/// A missing key is reported in the record's field order.
///
/// Every line is accepted before any key is looked up, so the reported key is
/// the first missing one in that order rather than the first line that was not
/// seen.
#[test]
fn meminfo_reports_the_first_missing_key_in_order() {
    let missing_available = fixture("meminfo/missing_available.txt");
    let result =
        call_pure("linux_text", "parse_meminfo", &[text(&missing_available)]).expect("runs");
    let error = outcome(result).expect_err("`MemAvailable` is absent");
    assert_eq!(error.kind, "linux-meminfo");
    assert!(error.message.contains("MemAvailable"), "{error:?}");

    let missing_total = fixture("meminfo/missing_total.txt");
    let result = call_pure("linux_text", "parse_meminfo", &[text(&missing_total)]).expect("runs");
    let error = outcome(result).expect_err("`MemTotal` is absent");
    assert!(error.message.contains("MemTotal"), "{error:?}");
}

/// Scaling to bytes saturates at the `Int` bounds.
///
/// The text is `tests/fixtures/stdlib/meminfo/saturation.txt`: the largest count
/// whose byte count still fits, one kilobyte past it, the smallest count that
/// fits, one kilobyte below that, and a negative count.
#[test]
fn meminfo_saturation_matches_the_int_bounds() {
    let saturation = fixture("meminfo/saturation.txt");
    let result = call_pure("linux_text", "parse_meminfo", &[text(&saturation)]).expect("runs");
    let record = outcome(result).expect("the text holds every reported key");
    assert_eq!(int(field(&record, "total")), 9_007_199_254_740_991 * 1024);
    assert_eq!(int(field(&record, "free")), i64::MAX);
    assert_eq!(int(field(&record, "available")), i64::MIN);
    assert_eq!(int(field(&record, "buffers")), i64::MIN);
    assert_eq!(int(field(&record, "cached")), -1024);
    assert_eq!(int(field(&record, "swap_free")), i64::MAX);
}

/// The host parsers accept only the baseline's decimal spelling.
///
/// The table is `tests/fixtures/stdlib/integers/field_spellings.txt`; the
/// rejected spellings include the ones `Str.parse_int` accepts (radix prefixes
/// and `_` separators) and the values outside `Int` range, including both
/// boundaries.
#[test]
fn decimal_fields_accept_only_the_host_spelling() {
    let rows = table("integers/field_spellings.txt");
    assert!(rows.len() >= 15, "the spelling table lost rows");
    for (spelling, expected) in rows {
        let expected = match expected.as_str() {
            "null" => None,
            number => Some(
                number
                    .parse::<i64>()
                    .unwrap_or_else(|_| panic!("fixture expectation `{number}` is not an Int")),
            ),
        };
        assert_eq!(parse_field_int(&spelling), expected, "spelling `{spelling}`");
    }
}

/// Module rows report their fields, and the placeholder list is empty.
///
/// The text is `tests/fixtures/stdlib/modules/rows.txt`: a row whose `used_by`
/// list has a trailing comma, a row that uses the `-` placeholder, a blank
/// line, and a row whose `used_by` list names another module and which carries
/// fields after the address. `retained_lines` is the helper the public entry
/// drops blank lines with.
#[test]
fn module_rows_report_their_fields() {
    let rows_text = fixture("modules/rows.txt");
    let retained =
        call_pure("linux_text", "retained_lines", &[text(&rows_text)]).expect("runs");
    let lines = strings(&retained);
    assert_eq!(lines.len(), 3, "blank lines are dropped: {lines:?}");

    let parse = |line: &str| {
        let result = call_pure("linux_text", "parse_module_line", &[text(line)])
            .unwrap_or_else(|error| panic!("parse {line:?}: {error:?}"));
        outcome(result).unwrap_or_else(|error| panic!("parse {line:?}: {error:?}"))
    };
    let core = parse(&lines[0]);
    assert_eq!(field(&core, "name"), &text("core"));
    assert_eq!(int(field(&core, "size")), 4096);
    assert_eq!(strings(field(&core, "used_by")), ["dep_a", "dep_b"]);

    let dep_a = parse(&lines[1]);
    assert_eq!(field(&dep_a, "name"), &text("dep_a"));
    assert_eq!(int(field(&dep_a, "size")), 2048);
    assert_eq!(strings(field(&dep_a, "used_by")), Vec::<String>::new());

    let dep_b = parse(&lines[2]);
    assert_eq!(strings(field(&dep_b, "used_by")), ["core"]);
}

/// A row that is not a module row reports the field that is wrong.
///
/// The shape of a short row decides its failure: a name with no size is a
/// missing size, a name and size with no use count is a missing use count, a
/// size spelled as text is an invalid value, and a row that stops before the
/// module state and address is malformed.
#[test]
fn module_rows_report_malformed_fields() {
    let missing_size = fixture("modules/missing_size.txt");
    let result =
        call_pure("linux_text", "parse_module_line", &[text(missing_size.trim_end())])
            .expect("runs");
    let error = outcome(result).expect_err("the row has no size");
    assert_eq!(error.kind, "linux-modules");
    assert!(error.message.contains("missing size"), "{error:?}");

    let missing_use_count = fixture("modules/missing_use_count.txt");
    let result = call_pure(
        "linux_text",
        "parse_module_line",
        &[text(missing_use_count.trim_end())],
    )
    .expect("runs");
    let error = outcome(result).expect_err("the row has no use count");
    assert_eq!(error.kind, "linux-modules");
    assert!(error.message.contains("missing use count"), "{error:?}");

    let size = fixture("modules/malformed_size.txt");
    let result =
        call_pure("linux_text", "parse_module_line", &[text(size.trim_end())]).expect("runs");
    let error = outcome(result).expect_err("the size field is not an integer");
    assert_eq!(error.kind, "linux-modules");
    assert!(
        error.message.contains("invalid size") && error.message.contains("notanumber"),
        "{error:?}"
    );

    let short = fixture("modules/short_row.txt");
    let result =
        call_pure("linux_text", "parse_module_line", &[text(short.trim_end())]).expect("runs");
    let error = outcome(result).expect_err("the row stops before `used_by`");
    assert_eq!(error.kind, "linux-modules");
    assert!(error.message.contains("malformed"), "{error:?}");
}

/// `/proc/uptime` text reads as whole seconds, and anything else reads zero.
///
/// The table is `tests/fixtures/stdlib/uptime/spellings.txt`.
#[test]
fn uptime_reads_whole_seconds() {
    let rows = table("uptime/spellings.txt");
    assert!(rows.len() >= 8, "the uptime table lost rows");
    for (uptime_text, expected) in rows {
        let expected = expected
            .parse::<i64>()
            .unwrap_or_else(|_| panic!("fixture expectation `{expected}` is not an Int"));
        let value = call_pure("unix", "uptime_from_text", &[text(&uptime_text)])
            .unwrap_or_else(|error| panic!("uptime_from_text({uptime_text:?}): {error:?}"));
        assert_eq!(int(&value), expected, "text `{uptime_text}`");
    }
}
