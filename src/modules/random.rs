use crate::runtime::value::RuntimeError;
use crate::source::Span;

pub(crate) fn bytes(count: i64, span: Span) -> Result<Vec<u8>, RuntimeError> {
    if count < 0 {
        return Err(RuntimeError::new("random-bytes", "count cannot be negative").with_span(span));
    }
    let count = usize::try_from(count).map_err(|_| {
        RuntimeError::host("random-bytes", &std::io::Error::from_raw_os_error(libc::ENOMEM)).with_span(span)
    })?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(count).map_err(|_| {
        RuntimeError::host("random-bytes", &std::io::Error::from_raw_os_error(libc::ENOMEM)).with_span(span)
    })?;
    bytes.resize(count, 0);
    fastrand::fill(&mut bytes);
    Ok(bytes)
}
