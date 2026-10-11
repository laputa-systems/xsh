use crate::runtime::value::{RecordMap, RuntimeError, Value};
use crate::source::Span;

unsafe extern "C" {
    fn xsh_long_double(input: *const libc::c_char, conversion: libc::c_char,
        precision: libc::c_int, alternate: libc::c_int, output: *mut libc::c_char,
        capacity: usize, consumed: *mut usize, range_error: *mut libc::c_int,
        key: *mut libc::c_char) -> libc::c_int;
    fn xsh_long_double_precision() -> libc::c_int;
}

pub(crate) fn precision() -> i64 {
    i64::from(unsafe { xsh_long_double_precision() })
}

fn failure(message: impl Into<String>, span: Span) -> RuntimeError {
    RuntimeError::new("numeric-long-double", message).with_span(span)
}

fn input_bytes(text: &str, span: Span) -> Result<Vec<u8>, RuntimeError> {
    let size = text.len().checked_add(1).ok_or_else(|| failure("numeric input is too large", span))?;
    let mut input = Vec::new();
    input.try_reserve_exact(size).map_err(|error| failure(error.to_string(), span))?;
    input.extend_from_slice(text.as_bytes());
    input.push(0);
    Ok(input)
}

fn fields(consumed: usize, range_error: libc::c_int, span: Span) -> Result<RecordMap, RuntimeError> {
    let consumed = i64::try_from(consumed).map_err(|_| failure("consumed input exceeds Int", span))?;
    Ok(RecordMap::from([
        ("consumed".into(), Value::Int(consumed)),
        ("range_error".into(), Value::Bool(range_error != 0)),
    ]))
}

pub(crate) fn parse(text: &str, span: Span) -> Result<Value, RuntimeError> {
    let input = input_bytes(text, span)?;
    let mut key = [0u8; 162];
    let mut consumed = 0;
    let mut range_error = 0;
    let status = unsafe { xsh_long_double(input.as_ptr().cast(), 0, -1, 0,
        std::ptr::null_mut(), 0, &mut consumed, &mut range_error, key.as_mut_ptr().cast()) };
    if status < 0 { return Err(failure(std::io::Error::last_os_error().to_string(), span)); }
    let length = key.iter().position(|byte| *byte == 0).ok_or_else(|| failure("numeric order key exceeds capacity", span))?;
    let key = std::str::from_utf8(&key[..length]).map_err(|error| failure(error.to_string(), span))?;
    let mut record = fields(consumed, range_error, span)?;
    record.insert("order_key".into(), Value::Str(key.into()));
    Ok(Value::Record(record))
}

pub(crate) fn format(text: &str, conversion: &str, precision: Option<i64>, alternate: bool,
    span: Span) -> Result<Value, RuntimeError> {
    let conversion = match conversion.as_bytes() {
        [byte] if b"fFeEgGaA".contains(byte) => *byte,
        _ => return Err(failure("conversion must be f, F, e, E, g, G, a, or A", span)),
    };
    let precision = match precision {
        None => -1,
        Some(value) if value >= 0 => i32::try_from(value).map_err(|_| failure("precision exceeds C int", span))?,
        Some(_) => return Err(failure("precision must be nonnegative", span)),
    };
    let input = input_bytes(text, span)?;
    let mut consumed = 0;
    let mut range_error = 0;
    let length = unsafe { xsh_long_double(input.as_ptr().cast(), conversion as libc::c_char,
        precision, i32::from(alternate), std::ptr::null_mut(), 0, &mut consumed,
        &mut range_error, std::ptr::null_mut()) };
    if length < 0 { return Err(failure(std::io::Error::last_os_error().to_string(), span)); }
    let capacity = (length as usize).checked_add(1).ok_or_else(|| failure("formatted output is too large", span))?;
    let mut output = Vec::new();
    output.try_reserve_exact(capacity).map_err(|error| failure(error.to_string(), span))?;
    output.resize(capacity, 0);
    let actual = unsafe { xsh_long_double(input.as_ptr().cast(), conversion as libc::c_char,
        precision, i32::from(alternate), output.as_mut_ptr().cast(), capacity, &mut consumed,
        &mut range_error, std::ptr::null_mut()) };
    if actual != length { return Err(failure("numeric formatting length changed", span)); }
    output.truncate(length as usize);
    let output = String::from_utf8(output).map_err(|error| failure(error.to_string(), span))?;
    let mut record = fields(consumed, range_error, span)?;
    record.insert("text".into(), Value::Str(output.into()));
    Ok(Value::Record(record))
}
