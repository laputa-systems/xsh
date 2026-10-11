#![allow(clippy::single_call_fn)]

use super::Evaluator;
use crate::runtime::value::{
    LiveStream, RuntimeError, ScriptStreamState, ScriptStreamStep, StreamValue, Value,
};
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
            return self.pull_script_state(script.clone(), span);
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
            self.cancel_script_state(script.clone(), span)?;
        }
        Ok(())
    }

    /// Descend through delegation iteratively, retaining ancestor scope context
    /// while a child runs and releasing every attachment before returning.
    fn pull_script_state(
        &mut self,
        root: ScriptStreamState,
        span: Span,
    ) -> Result<Option<Value>, RuntimeError> {
        let consumer_scope = self.current_scope_id();
        let mut parents: Vec<(
            ScriptStreamState,
            Span,
            usize,
            Option<super::ScopedProducerContext>,
        )> = Vec::new();
        let mut seen = rustc_hash::FxHashSet::default();
        let mut current = root.clone();
        let mut current_span = span;
        seen.insert(current.identity());
        loop {
            let step = current
                .lock(current_span)
                .and_then(|mut producer| producer.poll(self, current_span));
            match step {
                Ok(ScriptStreamStep::Delegate {
                    child,
                    span: child_span,
                    scopes,
                    context,
                }) => {
                    if !seen.insert(child.identity()) {
                        for (_, _, count, context) in parents.iter().rev() {
                            self.detach_owned_host_scopes(*count);
                            if let Some(context) = context {
                                self.swap_producer_context(context.clone());
                            }
                        }
                        let _ = self.cancel_script_state(root, span);
                        return Err(
                            RuntimeError::new("stream-state", "cyclic stream delegation")
                                .with_span(child_span),
                        );
                    }
                    self.reattach_owned_host_scopes(&scopes);
                    let consumer = context.map(|context| self.swap_producer_context(context));
                    parents.push((current, current_span, scopes.len(), consumer));
                    current = child;
                    current_span = child_span;
                }
                Ok(ScriptStreamStep::Yielded(value)) => {
                    let escaped = parents.iter().any(|(_, _, _, context)| context.is_some())
                        && Self::context_scope_runtime_value_escapes(&value);
                    // A delegated item crosses every retained producer boundary.
                    // Validate only this item, then stop child and parent frames on failure.
                    let validated = current
                        .lock(current_span)
                        .and_then(|producer| producer.validate_item(&value, current_span))
                        .and_then(|()| {
                            for (parent, parent_span, _, _) in parents.iter().rev() {
                                parent
                                    .lock(*parent_span)?
                                    .validate_item(&value, *parent_span)?;
                            }
                            Ok(())
                        });
                    if !escaped && validated.is_ok() {
                        self.transfer_owned_host_resources_in_value(&value, consumer_scope);
                    }
                    for (_, _, count, context) in parents.iter().rev() {
                        self.detach_owned_host_scopes(*count);
                        if let Some(context) = context {
                            self.swap_producer_context(context.clone());
                        }
                    }
                    if escaped {
                        let _ = self.cancel_script_state(root, span);
                        return Err(RuntimeError::new(
                            "context-scope-escape",
                            "a delegated live producer or host handle cannot escape a context",
                        )
                        .with_span(current_span));
                    }
                    if let Err(error) = validated {
                        let _ = self.cancel_script_state(root, span);
                        return Err(error);
                    }
                    return Ok(Some(value));
                }
                Ok(ScriptStreamStep::Finished) => {
                    seen.remove(&current.identity());
                    let Some((parent, parent_span, count, context)) = parents.pop() else {
                        return Ok(None);
                    };
                    self.detach_owned_host_scopes(count);
                    if let Some(context) = context {
                        self.swap_producer_context(context);
                    }
                    if let Err(error) = parent
                        .lock(parent_span)
                        .map(|mut producer| producer.delegated_finished())
                    {
                        for (_, _, count, context) in parents.iter().rev() {
                            self.detach_owned_host_scopes(*count);
                            if let Some(context) = context {
                                self.swap_producer_context(context.clone());
                            }
                        }
                        let _ = self.cancel_script_state(root, span);
                        return Err(error);
                    }
                    current = parent;
                    current_span = parent_span;
                }
                Err(error) => {
                    let escaped = parents.iter().any(|(_, _, _, context)| context.is_some())
                        && Self::context_scope_runtime_error_escapes(&error);
                    if !escaped {
                        self.transfer_owned_host_resources_in_runtime_error(&error, consumer_scope);
                    }
                    for (_, _, count, context) in parents.iter().rev() {
                        self.detach_owned_host_scopes(*count);
                        if let Some(context) = context {
                            self.swap_producer_context(context.clone());
                        }
                    }
                    // A delegated failure stops every suspended ancestor. The
                    // child's original error remains the primary failure.
                    let _ = self.cancel_script_state(root, span);
                    if escaped {
                        self.pending_traceback = None;
                        return Err(RuntimeError::new("context-scope-escape", "a delegated resource-bearing error cannot escape a context").with_span(current_span));
                    }
                    return Err(error);
                }
            }
        }
    }

    /// Remove child links before stopping frames, then unwind from the deepest
    /// child to its parent without growing the native stack.
    pub(super) fn cancel_script_state(
        &mut self,
        root: ScriptStreamState,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let mut pending = Vec::new();
        let mut seen = rustc_hash::FxHashSet::default();
        let mut current = root;
        let mut first_error = None;
        loop {
            if !seen.insert(current.identity()) {
                break;
            }
            let delegation = current
                .lock(span)
                .map(|mut producer| producer.take_delegated());
            match delegation {
                Ok((child, scopes, context)) => {
                    let count = if child.is_some() {
                        self.reattach_owned_host_scopes(&scopes);
                        scopes.len()
                    } else {
                        0
                    };
                    let consumer = if child.is_some() {
                        context.map(|context| self.swap_producer_context(context))
                    } else {
                        None
                    };
                    pending.push((current, count, consumer));
                    let Some(child) = child else {
                        break;
                    };
                    current = child;
                }
                Err(error) => {
                    first_error.get_or_insert(error);
                    break;
                }
            }
        }
        for (state, count, context) in pending.into_iter().rev() {
            self.detach_owned_host_scopes(count);
            if let Some(context) = context {
                self.swap_producer_context(context);
            }
            if let Err(error) = state
                .lock(span)
                .and_then(|mut producer| producer.cancel(self, span))
            {
                first_error.get_or_insert(error);
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
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
            RuntimeError::host(kind, &error).with_span(span)
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
            .map_err(|error| RuntimeError::host("fs-read", &error).with_span(span))?;
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
