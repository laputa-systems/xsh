use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

static NEXT_CACHE_OWNER: AtomicU64 = AtomicU64::new(1);
static TRACE_SINK: OnceLock<Option<Arc<Mutex<TraceSink>>>> = OnceLock::new();

struct TraceSink {
    next_event: u64,
    writer: Box<dyn Write + Send>,
}

/// A debug-only identity for the module-record map that owns a cache key.
///
/// Set `XSH_W9_CACHE_TRACE` to a fresh output path to write TSV events. The
/// trace is separate from diagnostics so it cannot change their rendering.
#[derive(Clone)]
pub(super) struct CacheTrace {
    owner: u64,
    kind: &'static str,
    sink: Arc<Mutex<TraceSink>>,
}

impl CacheTrace {
    pub(super) fn private() -> Option<Self> {
        Self::new("private")
    }

    pub(super) fn shared() -> Option<Self> {
        Self::new("shared")
    }

    fn new(kind: &'static str) -> Option<Self> {
        let sink = trace_sink()?;
        let trace = Self {
            owner: NEXT_CACHE_OWNER.fetch_add(1, Ordering::Relaxed),
            kind,
            sink,
        };
        trace.write("cache", None);
        Some(trace)
    }

    pub(super) fn event(
        &self,
        event: &'static str,
        module_key: &str,
        source_id: &str,
        start: u32,
        len: u32,
        role: &'static str,
        args_domain: &'static str,
    ) {
        self.write(
            event,
            Some((module_key, source_id, start, len, role, args_domain)),
        );
    }

    fn write(
        &self,
        event: &'static str,
        key: Option<(&str, &str, u32, u32, &'static str, &'static str)>,
    ) {
        let mut sink = self.sink.lock().expect("W9 cache trace sink poisoned");
        let sequence = sink.next_event;
        sink.next_event += 1;
        let (module_key, source_id, start, len, role, args_domain) = match key {
            Some((module, source, start, len, role, args)) => (
                module.to_owned(),
                source.to_owned(),
                start.to_string(),
                len.to_string(),
                role,
                args,
            ),
            None => (
                "-".to_owned(),
                "-".to_owned(),
                "-".to_owned(),
                "-".to_owned(),
                "-",
                "-",
            ),
        };
        writeln!(
            sink.writer,
            "W9CACHE\t1\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            std::process::id(),
            sequence,
            self.owner,
            self.kind,
            event,
            module_key,
            source_id,
            start,
            len,
            role,
            args_domain,
        )
        .expect("write W9 cache trace event");
        sink.writer.flush().expect("flush W9 cache trace event");
    }

    #[cfg(test)]
    pub(super) fn capture(owner: u64, kind: &'static str) -> (Self, Arc<Mutex<Vec<u8>>>) {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let trace = Self {
            owner,
            kind,
            sink: Arc::new(Mutex::new(TraceSink {
                next_event: 0,
                writer: Box::new(CaptureWriter(Arc::clone(&bytes))),
            })),
        };
        trace.write("cache", None);
        (trace, bytes)
    }
}

fn trace_sink() -> Option<Arc<Mutex<TraceSink>>> {
    TRACE_SINK
        .get_or_init(|| {
            let path = std::env::var_os("XSH_W9_CACHE_TRACE")?;
            let file = File::create(&path).unwrap_or_else(|error| {
                panic!(
                    "create XSH_W9_CACHE_TRACE file {}: {error}",
                    std::path::Path::new(&path).display()
                )
            });
            Some(Arc::new(Mutex::new(TraceSink {
                next_event: 0,
                writer: Box::new(BufWriter::new(file)),
            })))
        })
        .clone()
}

#[cfg(test)]
struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

#[cfg(test)]
impl Write for CaptureWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("W9 capture buffer poisoned")
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::CacheTrace;

    #[test]
    fn trace_rows_include_a_distinct_cache_owner_and_complete_key() {
        let (first, first_bytes) = CacheTrace::capture(4, "private");
        let (second, second_bytes) = CacheTrace::capture(5, "shared");
        for trace in [&first, &second] {
            trace.event(
                "prepared",
                "\"module\\tname\"",
                "SourceId(7)",
                11,
                3,
                "User",
                "Bytes",
            );
            trace.event(
                "checked",
                "\"module\\tname\"",
                "SourceId(7)",
                11,
                3,
                "User",
                "Bytes",
            );
        }

        let first_text = String::from_utf8(
            first_bytes.lock().expect("capture buffer").clone(),
        )
        .expect("UTF-8 trace");
        let second_text = String::from_utf8(
            second_bytes.lock().expect("capture buffer").clone(),
        )
        .expect("UTF-8 trace");
        assert!(first_text.contains("\t4\tprivate\tprepared\t\"module\\tname\"\tSourceId(7)\t11\t3\tUser\tBytes"));
        assert!(first_text.contains("\t4\tprivate\tchecked\t\"module\\tname\"\tSourceId(7)\t11\t3\tUser\tBytes"));
        assert!(second_text.contains("\t5\tshared\tprepared\t\"module\\tname\"\tSourceId(7)\t11\t3\tUser\tBytes"));
        assert!(second_text.contains("\t5\tshared\tchecked\t\"module\\tname\"\tSourceId(7)\t11\t3\tUser\tBytes"));
    }
}
