//! Bounded, streaming directory scan with per-entry size annotations.

use crate::runtime::value::{
    LiveStream, PathValue, RecordMap, RecordShape, RuntimeError, StreamValue, Value,
};
use crate::source::Span;
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::Metadata;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, LazyLock};

static K_ALLOCATED: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("allocated"));
static K_APPARENT: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("apparent"));
static K_DONE: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("done"));
static K_ERROR: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("error"));
static K_INDEX: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("index"));
static K_KIND: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("kind"));
static K_MODIFIED: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("modified"));
static K_NAME: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("name"));
static K_NLINK: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("nlink"));
static K_OMITTED: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("omitted"));
static K_PARENT: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("parent"));
static K_PATH: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("path"));
static K_STATE: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("state"));

static SCAN_ENTRY_SHAPE: LazyLock<RecordShape> = LazyLock::new(|| {
    RecordShape::new(vec![
        K_ALLOCATED.clone(),
        K_APPARENT.clone(),
        K_DONE.clone(),
        K_ERROR.clone(),
        K_INDEX.clone(),
        K_KIND.clone(),
        K_MODIFIED.clone(),
        K_NAME.clone(),
        K_NLINK.clone(),
        K_OMITTED.clone(),
        K_PARENT.clone(),
        K_PATH.clone(),
        K_STATE.clone(),
    ])
});

struct PendingDirectory {
    path: PathBuf,
    relative: Vec<u8>,
    index: usize,
    parent: i64,
    entries: std::vec::IntoIter<OsString>,
    incomplete: bool,
    omitted: bool,
    error: Option<String>,
}

struct ExclusionSet {
    paths: HashSet<Vec<u8>>,
}

impl ExclusionSet {
    fn new(paths: &[PathBuf]) -> Result<Self, String> {
        let mut set = HashSet::with_capacity(paths.len());
        for path in paths {
            let mut normalized = Vec::new();
            let mut named = false;
            for component in path.components() {
                match component {
                    Component::CurDir => {}
                    Component::Normal(name) => {
                        if named {
                            normalized.push(b'/');
                        }
                        normalized.extend_from_slice(name.as_bytes());
                        named = true;
                    }
                    Component::ParentDir => {
                        return Err("exclude paths cannot traverse above the scan root".to_owned());
                    }
                    Component::RootDir | Component::Prefix(_) => {
                        return Err("exclude paths must be relative to the scan root".to_owned());
                    }
                }
            }
            if !named {
                return Err("exclude paths must name an entry below the scan root".to_owned());
            }
            set.insert(normalized);
        }
        Ok(Self { paths: set })
    }

    fn contains(&self, relative: &[u8]) -> bool {
        self.paths.contains(relative)
    }
}

pub(crate) fn scan_filesystem(
    root: PathBuf,
    exclude: Vec<PathBuf>,
    one_file_system: bool,
    span: Span,
) -> Result<StreamValue, RuntimeError> {
    let exclusions = ExclusionSet::new(&exclude)
        .map_err(|message| RuntimeError::new("fs-scan", message).with_span(span))?;
    let root_metadata = std::fs::metadata(&root)
        .map_err(|error| RuntimeError::host("fs-scan", &error).with_span(span))?;
    if !root_metadata.is_dir() {
        return Err(RuntimeError::new("fs-scan", "scan root is not a directory").with_span(span));
    }
    let entries = std::fs::read_dir(&root)
        .map_err(|error| RuntimeError::host("fs-scan", &error).with_span(span))?;
    let mut seen = HashSet::new();
    seen.insert((root_metadata.dev(), root_metadata.ino()));
    let (entries, error) = sorted_names(entries.map(|entry| entry.map(|entry| entry.file_name())));
    let root = PendingDirectory {
        path: root,
        relative: Vec::new(),
        index: 0,
        parent: -1,
        entries,
        incomplete: error.is_some(),
        omitted: false,
        error,
    };
    Ok(StreamValue::from_live(
        "fs.scan",
        ScanStream {
            stack: vec![root],
            seen,
            exclusions,
            root_device: root_metadata.dev(),
            one_file_system,
            next_index: 1,
            root_entry_emitted: false,
            finished: false,
        },
    ))
}

fn sorted_names(entries: impl IntoIterator<Item = std::io::Result<OsString>>) -> (std::vec::IntoIter<OsString>, Option<String>) {
    let mut names = Vec::new();
    let mut error = None;
    for entry in entries {
        match entry {
            Ok(name) => names.push(name),
            Err(cause) => {
                if error.is_none() {
                    error = Some(cause.to_string());
                }
            }
        }
    }
    names.sort_unstable_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    (names.into_iter(), error)
}

fn state_record(
    index: usize,
    parent: i64,
    name: Vec<u8>,
    kind: &'static str,
    path: Option<&Path>,
    state: &'static str,
    size: (i64, i64),
    modified: Option<i64>,
    nlink: i64,
    omitted: bool,
    error: Option<String>,
    done: bool,
) -> Result<Value, RuntimeError> {
    let path = PathValue::new(if done {
        Vec::new()
    } else {
        path.map_or_else(Vec::new, |path| path.as_os_str().as_bytes().to_vec())
    })?;
    Ok(Value::Record(RecordMap::shaped(
        &SCAN_ENTRY_SHAPE,
        vec![
            Value::Int(size.1),
            Value::Int(size.0),
            Value::Bool(done),
            error.map_or(Value::Null, |message| Value::Str(Arc::from(message))),
            Value::Int(i64::try_from(index).unwrap_or(i64::MAX)),
            Value::Str(Arc::from(kind)),
            modified.map_or(Value::Null, Value::Int),
            Value::Bytes(name),
            Value::Int(nlink),
            Value::Bool(omitted),
            Value::Int(parent),
            Value::Path(path),
            Value::Str(Arc::from(state)),
        ],
    )))
}

fn entry_record(
    index: usize,
    parent: usize,
    name: &[u8],
    path: &Path,
    metadata: &Metadata,
    kind: &'static str,
    state: &'static str,
) -> Result<Value, RuntimeError> {
    let (apparent, allocated) = if kind == "dir" {
        (0, 0)
    } else {
        (
            i64::try_from(metadata.len()).unwrap_or(i64::MAX),
            i64::try_from(metadata.blocks().saturating_mul(512)).unwrap_or(i64::MAX),
        )
    };
    state_record(
        index,
        i64::try_from(parent).unwrap_or(i64::MAX),
        name.to_vec(),
        kind,
        Some(path),
        state,
        (apparent, allocated),
        Some(metadata.mtime()),
        i64::try_from(metadata.nlink()).unwrap_or(i64::MAX),
        false,
        None,
        false,
    )
}

fn failed_entry_record(
    index: usize,
    parent: usize,
    name: &[u8],
    path: &Path,
    message: String,
) -> Result<Value, RuntimeError> {
    state_record(
        index,
        i64::try_from(parent).unwrap_or(i64::MAX),
        name.to_vec(),
        "other",
        Some(path),
        "incomplete",
        (0, 0),
        None,
        0,
        false,
        Some(message),
        false,
    )
}

fn done_record(directory: &PendingDirectory) -> Result<Value, RuntimeError> {
    let incomplete =
        directory.incomplete || directory.error.is_some() || directory.omitted;
    state_record(
        directory.index,
        directory.parent,
        Vec::new(),
        "dir",
        None,
        if incomplete { "incomplete" } else { "complete" },
        (0, 0),
        None,
        0,
        directory.omitted,
        directory.error.clone(),
        true,
    )
}

fn kind_name(metadata: &Metadata) -> &'static str {
    let file_type = metadata.file_type();
    if file_type.is_dir() {
        "dir"
    } else if file_type.is_file() {
        "file"
    } else if file_type.is_symlink() {
        "symlink"
    } else {
        "other"
    }
}

fn relative_child(parent: &[u8], name: &[u8]) -> Vec<u8> {
    let mut relative = Vec::with_capacity(parent.len() + name.len() + 1);
    relative.extend_from_slice(parent);
    if !relative.is_empty() {
        relative.push(b'/');
    }
    relative.extend_from_slice(name);
    relative
}

struct ScanStream {
    stack: Vec<PendingDirectory>,
    seen: HashSet<(u64, u64)>,
    exclusions: ExclusionSet,
    root_device: u64,
    one_file_system: bool,
    next_index: usize,
    root_entry_emitted: bool,
    finished: bool,
}

fn descend_into(
    path: PathBuf,
    relative: Vec<u8>,
    index: usize,
    parent: usize,
) -> PendingDirectory {
    match std::fs::read_dir(&path) {
        Ok(entries) => {
            let (entries, error) = sorted_names(entries.map(|entry| entry.map(|entry| entry.file_name())));
            PendingDirectory {
                path,
                relative,
                index,
                parent: i64::try_from(parent).unwrap_or(i64::MAX),
                entries,
                incomplete: error.is_some(),
                omitted: false,
                error,
            }
        }
        Err(error) => PendingDirectory {
            path,
            relative,
            index,
            parent: i64::try_from(parent).unwrap_or(i64::MAX),
            entries: Vec::new().into_iter(),
            incomplete: true,
            omitted: false,
            error: Some(error.to_string()),
        },
    }
}

impl LiveStream for ScanStream {
    fn next(&mut self, _span: Span) -> Result<Option<Value>, RuntimeError> {
        if self.finished {
            return Ok(None);
        }
        if !self.root_entry_emitted {
            self.root_entry_emitted = true;
            let root = &self.stack[0];
            let metadata = std::fs::metadata(&root.path)
                .map_err(|error| RuntimeError::host("fs-scan", &error))?;
            let name = root.path.as_os_str().as_bytes().to_vec();
            return state_record(
                0,
                -1,
                name,
                "root",
                Some(&root.path),
                "pending",
                (0, 0),
                Some(metadata.mtime()),
                i64::try_from(metadata.nlink()).unwrap_or(i64::MAX),
                false,
                None,
                false,
            )
            .map(Some);
        }
        loop {
            let Some(top) = self.stack.last_mut() else {
                self.finished = true;
                return Ok(None);
            };
            let Some(name) = top.entries.next() else {
                let directory = self.stack.pop().expect("the top directory exists");
                if directory.incomplete || directory.error.is_some() || directory.omitted {
                    if let Some(parent) = self.stack.last_mut() {
                        parent.incomplete = true;
                        if parent.error.is_none() {
                            parent.error = directory.error.clone();
                        }
                    }
                }
                return done_record(&directory).map(Some);
            };
            let path = top.path.join(&name);
            let relative = relative_child(&top.relative, name.as_bytes());
            let index = self.next_index;
            self.next_index += 1;
            let parent = top.index;
            if self.exclusions.contains(&relative) {
                top.omitted = true;
                top.incomplete = true;
                continue;
            }
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    top.incomplete = true;
                    return failed_entry_record(index, parent, name.as_bytes(), &path, error.to_string())
                        .map(Some);
                }
            };
            let kind = kind_name(&metadata);
            let mut state = "complete";
            let mut descend = None;
            if kind == "dir" {
                let identity = (metadata.dev(), metadata.ino());
                if self.one_file_system && metadata.dev() != self.root_device {
                    state = "mount";
                    top.incomplete = true;
                } else if !self.seen.insert(identity) {
                    state = "alias";
                    top.incomplete = true;
                } else {
                    state = "pending";
                    descend = Some(descend_into(
                        path.clone(),
                        relative.clone(),
                        index,
                        parent,
                    ));
                }
            }
            let record = entry_record(index, parent, name.as_bytes(), &path, &metadata, kind, state)?;
            if state != "complete" && state != "pending" {
                top.incomplete = true;
            }
            if let Some(directory) = descend {
                self.stack.push(directory);
            }
            return Ok(Some(record));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;

    #[test]
    fn iteration_failure_preserves_children_and_closes_ancestors() {
        for failed in [true, false] {
            let temp = tempfile::tempdir().unwrap();
            let nested = temp.path().join("a/b");
            std::fs::create_dir_all(&nested).unwrap();
            std::fs::write(nested.join("z"), b"z").unwrap();
            std::fs::write(nested.join("a"), b"a").unwrap();
            let mut input = vec![Ok(OsString::from("z"))];
            if failed {
                input.push(Err(std::io::Error::from_raw_os_error(libc::EIO)));
            }
            input.push(Ok(OsString::from("a")));
            let (entries, error) = sorted_names(input);
            let directory = |path: PathBuf, index, parent, entries, error: Option<String>| PendingDirectory {
                path, relative: Vec::new(), index, parent, entries,
                incomplete: error.is_some(), omitted: false, error,
            };
            let mut stream = ScanStream {
                stack: vec![
                    directory(temp.path().to_path_buf(), 0, -1, Vec::new().into_iter(), None),
                    directory(temp.path().join("a"), 1, 0, Vec::new().into_iter(), None),
                    directory(nested, 2, 1, entries, error),
                ],
                seen: HashSet::new(), exclusions: ExclusionSet::new(&[]).unwrap(),
                root_device: 0, one_file_system: false, next_index: 3,
                root_entry_emitted: true, finished: false,
            };
            let span = Span::new(SourceId::new(0), 0, 0);
            let mut records = Vec::new();
            while let Some(Value::Record(record)) = stream.next(span).unwrap() {
                records.push(record);
            }
            assert_eq!(records.len(), 5);
            assert_eq!(records[0].get("name"), Some(&Value::Bytes(b"a".to_vec())));
            assert_eq!(records[1].get("name"), Some(&Value::Bytes(b"z".to_vec())));
            let expected_error = std::io::Error::from_raw_os_error(libc::EIO).to_string();
            for (record, index) in records[2..].iter().zip([2, 1, 0]) {
                assert_eq!(record.get("index"), Some(&Value::Int(index)));
                assert_eq!(record.get("done"), Some(&Value::Bool(true)));
                assert_eq!(record.get("state"), Some(&Value::Str(Arc::from(if failed { "incomplete" } else { "complete" }))));
                assert_eq!(record.get("error"), Some(&if failed { Value::Str(Arc::from(expected_error.as_str())) } else { Value::Null }));
            }
        }
    }
}
