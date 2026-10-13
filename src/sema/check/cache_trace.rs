use std::fs::{File, OpenOptions};
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
            let file = open_new_trace_file(std::path::Path::new(&path)).unwrap_or_else(|error| {
                panic!(
                    "create new XSH_W9_CACHE_TRACE file {}: {error}",
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

/// Claim a fresh trace path so an existing file or symlink is never truncated.
fn open_new_trace_file(path: &std::path::Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        options.custom_flags(libc::O_NOFOLLOW);
    }
    options.open(path)
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
    use super::{open_new_trace_file, CacheTrace};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TRACE_PATH: AtomicU64 = AtomicU64::new(0);

    fn trace_path(label: &str) -> PathBuf {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".work/tmp");
        fs::create_dir_all(&directory).expect("create ignored W9 trace test directory");
        directory.join(format!(
            "w9-cache-trace-{label}-{}-{}",
            std::process::id(),
            NEXT_TRACE_PATH.fetch_add(1, Ordering::Relaxed)
        ))
    }

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

    #[test]
    fn trace_file_creation_rejects_existing_files_and_symlinks() {
        let existing_file = trace_path("existing");
        fs::write(&existing_file, b"keep this trace").expect("write existing trace");
        let error = open_new_trace_file(&existing_file).expect_err("existing trace must fail");
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&existing_file).expect("read existing trace"), b"keep this trace");
        fs::remove_file(&existing_file).expect("remove existing trace");

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let target = trace_path("symlink-target");
            let link = trace_path("symlink");
            fs::write(&target, b"keep symlink target").expect("write symlink target");
            symlink(&target, &link).expect("create trace symlink");
            let error = open_new_trace_file(&link).expect_err("trace symlink must fail");
            assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
            assert_eq!(fs::read(&target).expect("read symlink target"), b"keep symlink target");
            fs::remove_file(&link).expect("remove trace symlink");
            fs::remove_file(&target).expect("remove symlink target");
        }
    }
}
