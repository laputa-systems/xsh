#![allow(clippy::single_call_fn)]

use super::Evaluator;
use crate::runtime::value::{LiveStream, RuntimeError, StreamValue, Value};
use crate::source::Span;

impl Evaluator {
    /// The next item of a stream.
    ///
    /// A script producer is a suspended body, so pulling one resumes it against
    /// this evaluator; every other stream pulls its items from its own source.
    pub(super) fn stream_next(
        &mut self,
        stream: &mut StreamValue,
        span: Span,
    ) -> Result<Option<Value>, RuntimeError> {
        if let Some(script) = stream.script() {
            let mut producer = script.lock(span)?;
            return producer.pull(self, span);
        }
        stream.next_live(span)
    }

    /// Stops a script stream early, without draining it.
    ///
    /// A consumer that takes part of a stream (`take`, `first`, a `for` loop
    /// that breaks) leaves the producer's body suspended; this runs the defers
    /// it registered and closes the scopes it opened, exactly once. Streams with
    /// no script producer have nothing to stop.
    pub(super) fn stream_cancel(
        &mut self,
        stream: &mut StreamValue,
        span: Span,
    ) -> Result<(), RuntimeError> {
        if let Some(script) = stream.script() {
            let mut producer = script.lock(span)?;
            producer.cancel(self, span)?;
        }
        Ok(())
    }

    pub(super) fn collect_stream_values(
        &mut self,
        mut stream: StreamValue,
        span: Span,
    ) -> Result<Vec<Value>, RuntimeError> {
        // Materialized prefix items come first, then the stream is drained to
        // exhaustion: a live source through `next_live`, a suspended producer by
        // resuming it against this evaluator.
        let mut values: Vec<Value> = std::mem::take(&mut stream.items)
            .into_iter()
            .map(|item| item.value)
            .collect();
        if stream.source.is_some() || stream.script().is_some() {
            while let Some(value) = self.stream_next(&mut stream, span)? {
                values.push(value);
            }
        }
        Ok(values)
    }
}

/// Live line stream over a file opened by `Path.lines()`. Yields each line as
/// `Str`, stripping a trailing `\r?\n`.
pub(super) struct FileLineStream {
    pub(super) reader: std::io::BufReader<std::fs::File>,
    pub(super) buffer: String,
}

impl LiveStream for FileLineStream {
    fn next(&mut self, span: Span) -> Result<Option<Value>, RuntimeError> {
        use std::io::BufRead;
        self.buffer.clear();
        let bytes = self.reader.read_line(&mut self.buffer).map_err(|error| {
            let kind = if error.kind() == std::io::ErrorKind::InvalidData {
                "invalid-utf8"
            } else {
                "fs-read"
            };
            RuntimeError::new(kind, error.to_string()).with_span(span)
        })?;
        if bytes == 0 {
            return Ok(None);
        }
        if self.buffer.ends_with('\n') {
            self.buffer.pop();
            if self.buffer.ends_with('\r') {
                self.buffer.pop();
            }
        }
        Ok(Some(Value::Str(self.buffer.as_str().into())))
    }
}

/// Live byte-line stream over a file opened by `Path.bytes_lines()`.
pub(super) struct FileBytesLineStream {
    pub(super) reader: std::io::BufReader<std::fs::File>,
    pub(super) buffer: Vec<u8>,
}

impl LiveStream for FileBytesLineStream {
    fn next(&mut self, span: Span) -> Result<Option<Value>, RuntimeError> {
        use std::io::BufRead;
        self.buffer.clear();
        let bytes = self
            .reader
            .read_until(b'\n', &mut self.buffer)
            .map_err(|error| RuntimeError::new("fs-read", error.to_string()).with_span(span))?;
        if bytes == 0 {
            return Ok(None);
        }
        if self.buffer.ends_with(b"\n") {
            self.buffer.pop();
            if self.buffer.ends_with(b"\r") {
                self.buffer.pop();
            }
        }
        Ok(Some(Value::Bytes(self.buffer.clone())))
    }
}

pub(super) fn platform_arg_max() -> usize {
    let value = unsafe { libc::sysconf(libc::_SC_ARG_MAX) };
    if value > 0 {
        value as usize
    } else {
        128 * 1024
    }
}
