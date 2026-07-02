#![allow(clippy::single_call_fn)]
#![allow(dead_code)]

use crate::runtime::process::path_bytes;
use crate::runtime::value::{
    FsEntryValue, LiveStream, PathValue, RecordMap, RecordShape, RuntimeError, StreamItem,
    StreamValue, Value,
};
use crate::source::Span;
use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
use cap_std::ambient_authority;
use cap_std::fs::{
    Dir as CapDir, File as CapFile, OpenOptions as CapOpenOptions, Permissions as CapPermissions,
};
use cap_tempfile::TempFile;
use crossbeam_channel::Sender;
use rustc_hash::FxHashSet;
use rustix::fs::{
    self as rfs, AtFlags, CWD, FlockOperation, Gid, StatVfs, StatVfsMountFlags, Timespec,
    Timestamps, UTIME_NOW,
};
use std::borrow::Cow;
use std::sync::{Arc, LazyLock};

static K_PATH: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("path"));
static K_NAME: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("name"));
static K_EXT: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("ext"));
static K_KIND: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("kind"));
static K_SIZE: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("size"));
static K_MODE: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("mode"));
static K_UID: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("uid"));
static K_GID: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("gid"));
static K_MODIFIED: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("modified"));
static K_ACCESSED: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("accessed"));
static K_BLOCKS_512: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("blocks_512"));
static K_EXECUTABLE: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("executable"));
static K_GROUP_EXECUTABLE: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("group_executable"));
static K_OTHER_EXECUTABLE: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("other_executable"));
static K_OWNER_EXECUTABLE: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("owner_executable"));
static K_SETGID: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("setgid"));
static K_SETUID: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("setuid"));
static K_STICKY: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("sticky"));
static K_WORLD_WRITABLE: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("world_writable"));
static V_KIND_DIR: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("dir"));
static V_KIND_FILE: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("file"));
static V_KIND_SYMLINK: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("symlink"));
static V_KIND_OTHER: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("other"));
const FS_ENTRY_EXT_INDEX: usize = 3;
const FS_ENTRY_KIND_INDEX: usize = 6;
const FS_ENTRY_NAME_INDEX: usize = 9;
const FS_ENTRY_PATH_INDEX: usize = 12;
static FS_ENTRY_SHAPE: LazyLock<RecordShape> = LazyLock::new(|| {
    RecordShape::new(vec![
        K_ACCESSED.clone(),
        K_BLOCKS_512.clone(),
        K_EXECUTABLE.clone(),
        K_EXT.clone(),
        K_GID.clone(),
        K_GROUP_EXECUTABLE.clone(),
        K_KIND.clone(),
        K_MODE.clone(),
        K_MODIFIED.clone(),
        K_NAME.clone(),
        K_OTHER_EXECUTABLE.clone(),
        K_OWNER_EXECUTABLE.clone(),
        K_PATH.clone(),
        K_SETGID.clone(),
        K_SETUID.clone(),
        K_SIZE.clone(),
        K_STICKY.clone(),
        K_UID.clone(),
        K_WORLD_WRITABLE.clone(),
    ])
});
static FS_ENTRY_CHEAP_DEFAULTS: LazyLock<Vec<Value>> = LazyLock::new(|| {
    vec![
        Value::Int(0),
        Value::Int(0),
        Value::Bool(false),
        Value::Str(Arc::from("")),
        Value::Int(0),
        Value::Bool(false),
        Value::Str(Arc::from("")),
        Value::Int(0),
        Value::Int(0),
        Value::Str(Arc::from("")),
        Value::Bool(false),
        Value::Bool(false),
        Value::Path(PathValue::new(Vec::new()).expect("empty path contains no NUL")),
        Value::Bool(false),
        Value::Bool(false),
        Value::Int(0),
        Value::Bool(false),
        Value::Int(0),
        Value::Bool(false),
    ]
});
use std::ffi::{CString, OsString};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

#[derive(Default)]
pub(crate) struct CopyTreeStats {
    files: i64,
    dirs: i64,
    symlinks: i64,
}

#[derive(Default)]
pub(crate) struct RemoveManifestStats {
    removed: i64,
    missing: i64,
    pruned_dirs: i64,
}

pub(crate) struct FilesystemStats {
    pub(crate) blocks_1k: u64,
    pub(crate) used_1k: u64,
    pub(crate) available_1k: u64,
    pub(crate) capacity_percent: u64,
}

pub(crate) struct FsMount {
    pub(crate) filesystem: String,
    pub(crate) mounted_on: PathBuf,
    pub(crate) fstype: String,
    pub(crate) blocks_1k: u64,
    pub(crate) used_1k: u64,
    pub(crate) available_1k: u64,
    pub(crate) capacity_percent: u64,
    pub(crate) files: u64,
    pub(crate) files_used: u64,
    pub(crate) files_free: u64,
    pub(crate) files_capacity_percent: u64,
    pub(crate) readonly: bool,
}

pub(crate) struct RootedInstallOptions {
    pub(crate) mode: i64,
    pub(crate) parents: bool,
    pub(crate) overwrite: bool,
    pub(crate) span: Span,
}

pub(crate) fn mode_executable(mode: i64) -> bool {
    mode & 0o111 != 0
}

pub(crate) fn mode_world_writable(mode: i64) -> bool {
    mode & 0o002 != 0
}

pub(crate) fn mode_sticky(mode: i64) -> bool {
    mode & 0o1000 != 0
}

pub(crate) fn mode_setuid(mode: i64) -> bool {
    mode & 0o4000 != 0
}

pub(crate) fn mode_setgid(mode: i64) -> bool {
    mode & 0o2000 != 0
}

pub(crate) fn mode_owner_executable(mode: i64) -> bool {
    mode & 0o100 != 0
}

pub(crate) fn mode_group_executable(mode: i64) -> bool {
    mode & 0o010 != 0
}

pub(crate) fn mode_other_executable(mode: i64) -> bool {
    mode & 0o001 != 0
}

pub(crate) fn open_root(path: PathBuf, span: Span) -> Result<CapDir, RuntimeError> {
    let dir = CapDir::open_ambient_dir(&path, ambient_authority())
        .map_err(|error| RuntimeError::new("fs-root", error.to_string()).with_span(span))?;
    let metadata = dir
        .dir_metadata()
        .map_err(|error| RuntimeError::new("fs-root", error.to_string()).with_span(span))?;
    if !metadata.is_dir() {
        return Err(RuntimeError::new("fs-root", "root path is not a directory").with_span(span));
    }
    Ok(dir)
}

pub(crate) fn rooted_open_root(
    root: &CapDir,
    path: &Path,
    span: Span,
) -> Result<CapDir, RuntimeError> {
    rooted_check_path(path, "fs-root", span)?;
    root.open_dir(path)
        .map_err(|error| RuntimeError::new("fs-root", error.to_string()).with_span(span))
}

pub(crate) fn rooted_read(root: &CapDir, path: &Path, span: Span) -> Result<Vec<u8>, RuntimeError> {
    let mut file = rooted_open_file(root, path, RootedOpenMode::Read, "fs-root-read", span)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| RuntimeError::new("fs-root-read", error.to_string()).with_span(span))?;
    Ok(bytes)
}

pub(crate) fn rooted_write(
    root: &CapDir,
    path: &Path,
    data: &[u8],
    span: Span,
) -> Result<(), RuntimeError> {
    let mut file = rooted_open_file(
        root,
        path,
        RootedOpenMode::WriteTruncate,
        "fs-root-write",
        span,
    )?;
    file.write_all(data)
        .map_err(|error| RuntimeError::new("fs-root-write", error.to_string()).with_span(span))
}

pub(crate) fn write_path(path: PathBuf, data: &[u8], span: Span) -> Result<(), RuntimeError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| RuntimeError::new("fs-write", "path has no file name").with_span(span))?;
    let dir = CapDir::open_ambient_dir(parent, ambient_authority())
        .map_err(|error| RuntimeError::new("fs-write", error.to_string()).with_span(span))?;
    let mut file = rooted_open_file(
        &dir,
        Path::new(name),
        RootedOpenMode::WriteTruncate,
        "fs-write",
        span,
    )?;
    file.write_all(data)
        .map_err(|error| RuntimeError::new("fs-write", error.to_string()).with_span(span))
}

pub(crate) fn rooted_write_atomic(
    root: &CapDir,
    path: &Path,
    data: &[u8],
    span: Span,
) -> Result<(), RuntimeError> {
    let (parent_path, leaf) = rooted_parent_and_leaf_path(path, "fs-root-write", span)?;
    let leaf_text = leaf.to_string_lossy();
    let temp_name = OsString::from(format!(
        ".{}.tmp.{}.{}",
        leaf_text,
        std::process::id(),
        unix_time_nanos()
    ));
    let temp_path = parent_path.join(&temp_name);
    let dest_path = parent_path.join(&leaf);
    let mut temp = rooted_open_file(
        root,
        &temp_path,
        RootedOpenMode::WriteCreateNew,
        "fs-root-write",
        span,
    )?;
    if let Err(error) = temp.write_all(data) {
        let _ = root.remove_file(&temp_path);
        return Err(RuntimeError::new("fs-root-write", error.to_string()).with_span(span));
    }
    if let Err(error) = temp.sync_all() {
        let _ = root.remove_file(&temp_path);
        return Err(RuntimeError::new("fs-root-write", error.to_string()).with_span(span));
    }
    drop(temp);
    root.rename(&temp_path, root, &dest_path).map_err(|error| {
        let _ = root.remove_file(&temp_path);
        RuntimeError::new("fs-root-write", error.to_string()).with_span(span)
    })
}

pub(crate) fn rooted_metadata(
    root: &CapDir,
    path: &Path,
    span: Span,
) -> Result<Value, RuntimeError> {
    let file = rooted_open_file(root, path, RootedOpenMode::Read, "fs-root-stat", span)?;
    let metadata = file
        .metadata()
        .map_err(|error| RuntimeError::new("fs-root-stat", error.to_string()).with_span(span))?;
    fs_entry_record(path, &metadata).map_err(|error| error.with_span(span))
}

pub(crate) fn rooted_exists(root: &CapDir, path: &Path, span: Span) -> Result<bool, RuntimeError> {
    rooted_check_path(path, "fs-root-exists", span)?;
    match root.symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(RuntimeError::new("fs-root-exists", error.to_string()).with_span(span)),
    }
}

pub(crate) fn rooted_mkdir(
    root: &CapDir,
    path: &Path,
    mode: i64,
    parents: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    if !(0..=0o7777).contains(&mode) {
        return Err(RuntimeError::new("fs-root-mkdir", "mode is out of range").with_span(span));
    }
    if parents {
        rooted_check_path(path, "fs-root-mkdir", span)?;
        root.create_dir_all(path).map_err(|error| {
            RuntimeError::new("fs-root-mkdir", error.to_string()).with_span(span)
        })?;
    } else {
        rooted_check_path(path, "fs-root-mkdir", span)?;
        root.create_dir(path).map_err(|error| {
            RuntimeError::new("fs-root-mkdir", error.to_string()).with_span(span)
        })?;
    }
    root.set_permissions(
        path,
        CapPermissions::from_std(std::fs::Permissions::from_mode(mode as u32)),
    )
    .map_err(|error| RuntimeError::new("fs-root-mkdir", error.to_string()).with_span(span))
}

pub(crate) fn rooted_readlink(
    root: &CapDir,
    path: &Path,
    span: Span,
) -> Result<PathBuf, RuntimeError> {
    rooted_check_path(path, "fs-root-readlink", span)?;
    root.read_link_contents(path)
        .map_err(|error| RuntimeError::new("fs-root-readlink", error.to_string()).with_span(span))
}

pub(crate) fn rooted_symlink(
    root: &CapDir,
    target: &Path,
    path: &Path,
    parents: bool,
    overwrite: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    rooted_check_path(path, "fs-root-symlink", span)?;
    let (parent_path, _) = rooted_parent_and_leaf_path(path, "fs-root-symlink", span)?;
    if parents {
        root.create_dir_all(&parent_path).map_err(|error| {
            RuntimeError::new("fs-root-symlink", error.to_string()).with_span(span)
        })?;
    }
    if overwrite {
        let _ = root.remove_file(path);
    }
    root.symlink(target, path)
        .map_err(|error| RuntimeError::new("fs-root-symlink", error.to_string()).with_span(span))
}

pub(crate) fn rooted_chmod(
    root: &CapDir,
    path: &Path,
    mode: i64,
    span: Span,
) -> Result<(), RuntimeError> {
    if !(0..=0o7777).contains(&mode) {
        return Err(RuntimeError::new("fs-root-chmod", "mode is out of range").with_span(span));
    }
    let file = rooted_open_file(root, path, RootedOpenMode::Read, "fs-root-chmod", span)?;
    file.set_permissions(std::fs::Permissions::from_mode(mode as u32))
        .map_err(|error| RuntimeError::new("fs-root-chmod", error.to_string()).with_span(span))
}

pub(crate) fn rooted_install_file(
    source_root: &CapDir,
    source: &Path,
    dest_root: &CapDir,
    dest: &Path,
    options: RootedInstallOptions,
) -> Result<(), RuntimeError> {
    let RootedInstallOptions {
        mode,
        parents,
        overwrite,
        span,
    } = options;
    if !(0..=0o7777).contains(&mode) {
        return Err(RuntimeError::new("fs-root-install", "mode is out of range").with_span(span));
    }
    let mut input = rooted_open_file(
        source_root,
        source,
        RootedOpenMode::Read,
        "fs-root-install",
        span,
    )?;
    let (parent_path, _) = rooted_parent_and_leaf_path(dest, "fs-root-install", span)?;
    if parents {
        dest_root.create_dir_all(&parent_path).map_err(|error| {
            RuntimeError::new("fs-root-install", error.to_string()).with_span(span)
        })?;
    }
    if overwrite {
        let _ = dest_root.remove_file(dest);
    }
    let open_mode = if overwrite {
        RootedOpenMode::WriteTruncate
    } else {
        RootedOpenMode::WriteCreateNew
    };
    let mut output = rooted_open_file(dest_root, dest, open_mode, "fs-root-install", span)?;
    if let Err(error) = std::io::copy(&mut input, &mut output) {
        let _ = dest_root.remove_file(dest);
        return Err(RuntimeError::new("fs-root-install", error.to_string()).with_span(span));
    }
    output
        .set_permissions(std::fs::Permissions::from_mode(mode as u32))
        .map_err(|error| {
            let _ = dest_root.remove_file(dest);
            RuntimeError::new("fs-root-install", error.to_string()).with_span(span)
        })
}

pub(crate) fn rooted_remove(
    root: &CapDir,
    path: &Path,
    dir: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    rooted_check_path(path, "fs-root-remove", span)?;
    let result = if dir {
        root.remove_dir(path)
    } else {
        root.remove_file(path)
    };
    result.map_err(|error| RuntimeError::new("fs-root-remove", error.to_string()).with_span(span))
}

enum RootedOpenMode {
    Read,
    WriteTruncate,
    WriteCreateNew,
}

fn rooted_open_file(
    root: &CapDir,
    path: &Path,
    mode: RootedOpenMode,
    kind: &'static str,
    span: Span,
) -> Result<std::fs::File, RuntimeError> {
    rooted_check_path(path, kind, span)?;
    let mut options = CapOpenOptions::new();
    options.follow(FollowSymlinks::No);
    match mode {
        RootedOpenMode::Read => {
            options.read(true);
        }
        RootedOpenMode::WriteTruncate => {
            options.write(true).create(true).truncate(true);
        }
        RootedOpenMode::WriteCreateNew => {
            options.write(true).create_new(true);
        }
    }
    root.open_with(path, &options)
        .map(CapFile::into_std)
        .map_err(|error| RuntimeError::new(kind, error.to_string()).with_span(span))
}

fn rooted_check_path(path: &Path, kind: &'static str, span: Span) -> Result<(), RuntimeError> {
    rooted_components(path, kind, span).map(|_| ())
}

fn rooted_parent_and_leaf_path(
    path: &Path,
    kind: &'static str,
    span: Span,
) -> Result<(PathBuf, OsString), RuntimeError> {
    let components = rooted_components(path, kind, span)?;
    let Some((leaf, parent_components)) = components.split_last() else {
        return Err(
            RuntimeError::new(kind, "path must name an entry below the root").with_span(span),
        );
    };
    Ok((path_from_components(parent_components), leaf.clone()))
}

fn rooted_components(
    path: &Path,
    kind: &'static str,
    span: Span,
) -> Result<Vec<OsString>, RuntimeError> {
    if path.as_os_str().is_empty() {
        return Err(RuntimeError::new(kind, "rooted path cannot be empty").with_span(span));
    }

    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(value) => components.push(value.to_os_string()),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(RuntimeError::new(
                    kind,
                    "rooted path cannot be absolute or contain `..`",
                )
                .with_span(span));
            }
        }
    }
    Ok(components)
}

fn path_from_components(components: &[OsString]) -> PathBuf {
    let mut path = PathBuf::new();
    for component in components {
        path.push(component);
    }
    path
}

fn unix_time_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

struct MountSource {
    filesystem: String,
    mounted_on: PathBuf,
    fstype: String,
}

fn df_capacity_percent(used_1k: u64, available_1k: u64) -> u64 {
    let denominator = used_1k.saturating_add(available_1k);
    if denominator == 0 {
        return 0;
    }

    used_1k.saturating_mul(100).saturating_add(denominator - 1) / denominator
}

/// Which entry kinds `fs.walk`/`fs.files`/`fs.dirs` emit. Every kind is still
/// *traversed* (directories are descended regardless); this only gates which
/// records leave the producer.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum WalkEmit {
    All,
    Files,
    Dirs,
}

#[derive(Clone)]
struct WalkExtFilter {
    exts: WalkExts,
    include_no_ext: bool,
}

#[derive(Clone)]
enum WalkExts {
    Linear(Arc<Vec<Vec<u8>>>),
    Hashed(Arc<FxHashSet<Vec<u8>>>),
}

impl WalkExtFilter {
    fn new(exts: Vec<String>) -> Option<Self> {
        if exts.is_empty() {
            return None;
        }
        let include_no_ext = exts.iter().any(String::is_empty);
        let exts = exts
            .into_iter()
            .filter(|ext| !ext.is_empty())
            .map(String::into_bytes)
            .collect::<Vec<_>>();
        let exts = if exts.len() > 8 {
            WalkExts::Hashed(Arc::new(exts.into_iter().collect()))
        } else {
            WalkExts::Linear(Arc::new(exts))
        };
        Some(Self {
            exts,
            include_no_ext,
        })
    }

    fn matches(&self, path: &Path) -> bool {
        let Some(ext) = path.extension() else {
            return self.include_no_ext;
        };
        let ext = ext.as_bytes();
        match &self.exts {
            WalkExts::Linear(exts) => exts.iter().any(|candidate| candidate.as_slice() == ext),
            WalkExts::Hashed(exts) => exts.contains(ext),
        }
    }
}

/// Walk `root` as a lazy [`LiveStream`]: one record is produced per `next()` so
/// a downstream `where`/`take` only pays for the entries it actually pulls
/// (docs/STREAMS.md §3). Only the root `symlink_metadata` is eager — it is the
/// one error that must surface before any items are seen, matching the old
/// behavior of failing fast on a missing root.
pub(crate) fn walk_filesystem(
    root: PathBuf,
    gitignore: bool,
    stat: bool,
    hidden: bool,
    emit: WalkEmit,
    exts: Vec<String>,
    span: Span,
) -> Result<StreamValue, RuntimeError> {
    std::fs::symlink_metadata(&root)
        .map_err(|error| RuntimeError::new("fs-walk", error.to_string()).with_span(span))?;
    let ext_filter = WalkExtFilter::new(exts);
    let spec = WalkSpec {
        root,
        gitignore,
        stat,
        hidden,
        emit,
        ext_filter,
        span,
    };
    Ok(StreamValue::from_live(
        "fs.walk",
        IgnoreWalkStream::Pending(spec),
    ))
}

pub(crate) fn list_filesystem(
    root: PathBuf,
    stat: bool,
    ordered: bool,
    span: Span,
) -> Result<StreamValue, RuntimeError> {
    let mut children = std::fs::read_dir(&root)
        .map_err(|error| RuntimeError::new("fs-ls", error.to_string()).with_span(span))?
        .map(|entry| entry.map(|entry| (entry.path(), entry)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| RuntimeError::new("fs-ls", error.to_string()).with_span(span))?;
    if ordered {
        children.sort_unstable_by(|(left, _), (right, _)| {
            left.as_os_str()
                .as_bytes()
                .cmp(right.as_os_str().as_bytes())
        });
    }
    let mut items = Vec::with_capacity(children.len());
    for (path, entry) in children {
        if stat {
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|error| RuntimeError::new("fs-ls", error.to_string()).with_span(span))?;
            push_fs_entry(&mut items, &path, &metadata)?;
        } else {
            let file_type = entry
                .file_type()
                .map_err(|error| RuntimeError::new("fs-ls", error.to_string()).with_span(span))?;
            items.push(StreamItem {
                value: Value::FsEntry(FsEntryValue::new(path, file_type)),
                index: items.len(),
                source_span: None,
            });
        }
    }
    Ok(StreamValue::from_items(items))
}

pub(crate) fn disk_usage(path: PathBuf, span: Span) -> Result<i64, RuntimeError> {
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|error| RuntimeError::new("fs-du", error.to_string()).with_span(span))?;
    let mut size = metadata.len() as i64;
    if metadata.file_type().is_dir() {
        let children = std::fs::read_dir(&path)
            .map_err(|error| RuntimeError::new("fs-du", error.to_string()).with_span(span))?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| RuntimeError::new("fs-du", error.to_string()).with_span(span))?;
        for child in children {
            size += disk_usage(child, span)?;
        }
    }
    Ok(size)
}

pub(crate) fn metadata(path: PathBuf, span: Span) -> Result<Value, RuntimeError> {
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|error| RuntimeError::new("fs-metadata", error.to_string()).with_span(span))?;
    fs_entry_record(&path, &metadata).map_err(|error| error.with_span(span))
}

pub(crate) fn exists(path: PathBuf, span: Span) -> Result<bool, RuntimeError> {
    match std::fs::symlink_metadata(&path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(RuntimeError::new("fs-exists", error.to_string()).with_span(span)),
    }
}

pub(crate) fn resolve_path(path: PathBuf, span: Span) -> Result<PathValue, RuntimeError> {
    let resolved = std::fs::canonicalize(path)
        .map_err(|error| RuntimeError::new("path-resolve", error.to_string()).with_span(span))?;
    PathValue::new(path_bytes(&resolved)).map_err(|error| error.with_span(span))
}

pub(crate) fn filesystem_stats(path: &Path, span: Span) -> Result<FilesystemStats, RuntimeError> {
    let stats = statvfs(path, "fs-statvfs", span)?;
    let block_size = if stats.f_frsize == 0 {
        stats.f_bsize
    } else {
        stats.f_frsize
    };
    let to_k = |blocks: u64| blocks.saturating_mul(block_size) / 1024;
    let blocks_1k = to_k(stats.f_blocks);
    let available_1k = to_k(stats.f_bavail);
    let free_1k = to_k(stats.f_bfree);
    let used_1k = blocks_1k.saturating_sub(free_1k);
    let capacity_percent = df_capacity_percent(used_1k, available_1k);
    Ok(FilesystemStats {
        blocks_1k,
        used_1k,
        available_1k,
        capacity_percent,
    })
}

pub(crate) fn mounts(span: Span) -> Result<Vec<FsMount>, RuntimeError> {
    mount_sources(span)?
        .into_iter()
        .map(|source| mount_record(source, span))
        .collect()
}

pub(crate) fn mount_for(path: &Path, span: Span) -> Result<FsMount, RuntimeError> {
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let Some(source) = mount_sources(span)?.into_iter().max_by_key(|mount| {
        if target.starts_with(&mount.mounted_on) {
            mount.mounted_on.components().count()
        } else {
            0
        }
    }) else {
        return Err(RuntimeError::new("fs-mount", "mount not found").with_span(span));
    };
    if !target.starts_with(&source.mounted_on) {
        return Err(RuntimeError::new("fs-mount", "mount not found").with_span(span));
    }
    mount_record(source, span)
}

fn mount_record(source: MountSource, span: Span) -> Result<FsMount, RuntimeError> {
    let stats = statvfs(&source.mounted_on, "fs-mount", span)?;
    let block_size = if stats.f_frsize == 0 {
        stats.f_bsize
    } else {
        stats.f_frsize
    } as u128;
    let to_k = |blocks: u128| {
        blocks
            .saturating_mul(block_size)
            .saturating_div(1024)
            .min(u64::MAX as u128) as u64
    };
    let blocks_1k = to_k(stats.f_blocks as u128);
    let available_1k = to_k(stats.f_bavail as u128);
    let free_1k = to_k(stats.f_bfree as u128);
    let used_1k = blocks_1k.saturating_sub(free_1k);
    let files = (stats.f_files as u128).min(u64::MAX as u128) as u64;
    let files_free = (stats.f_ffree as u128).min(u64::MAX as u128) as u64;
    let files_used = files.saturating_sub(files_free);
    let files_capacity_percent = df_capacity_percent(files_used, files_free);
    Ok(FsMount {
        filesystem: source.filesystem,
        mounted_on: source.mounted_on,
        fstype: source.fstype,
        blocks_1k,
        used_1k,
        available_1k,
        capacity_percent: df_capacity_percent(used_1k, available_1k),
        files,
        files_used,
        files_free,
        files_capacity_percent,
        readonly: stats.f_flag.contains(StatVfsMountFlags::RDONLY),
    })
}

fn statvfs(path: &Path, kind: &str, span: Span) -> Result<StatVfs, RuntimeError> {
    rfs::statvfs(path).map_err(|error| RuntimeError::new(kind, error.to_string()).with_span(span))
}

#[cfg(target_os = "linux")]
fn mount_sources(span: Span) -> Result<Vec<MountSource>, RuntimeError> {
    let text = std::fs::read_to_string("/proc/self/mountinfo")
        .map_err(|error| RuntimeError::new("fs-mount", error.to_string()).with_span(span))?;
    Ok(text
        .lines()
        .filter_map(parse_linux_mountinfo_line)
        .collect())
}

#[cfg(target_os = "linux")]
fn parse_linux_mountinfo_line(line: &str) -> Option<MountSource> {
    let (left, right) = line.split_once(" - ")?;
    let left = left.split_whitespace().collect::<Vec<_>>();
    let right = right.split_whitespace().collect::<Vec<_>>();
    if left.len() < 5 || right.len() < 2 {
        return None;
    }
    Some(MountSource {
        filesystem: unescape_mount_text(right[1]),
        mounted_on: PathBuf::from(OsString::from_vec(unescape_mount_bytes(left[4]))),
        fstype: right[0].to_string(),
    })
}

#[cfg(target_os = "linux")]
fn unescape_mount_text(value: &str) -> String {
    String::from_utf8_lossy(&unescape_mount_bytes(value)).into_owned()
}

#[cfg(target_os = "linux")]
fn unescape_mount_bytes(value: &str) -> Vec<u8> {
    let mut result = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' && index + 3 < bytes.len() {
            let octal = &value[index + 1..index + 4];
            if octal
                .as_bytes()
                .iter()
                .all(|byte| (b'0'..=b'7').contains(byte))
                && let Ok(decoded) = u8::from_str_radix(octal, 8)
            {
                result.push(decoded);
                index += 4;
                continue;
            }
        }
        result.push(bytes[index]);
        index += 1;
    }
    result
}

#[cfg(target_os = "macos")]
fn mount_sources(span: Span) -> Result<Vec<MountSource>, RuntimeError> {
    let mut entries: *mut libc::statfs = std::ptr::null_mut();
    let count = unsafe { libc::getmntinfo(&mut entries, libc::MNT_NOWAIT) };
    if count <= 0 {
        return Err(
            RuntimeError::new("fs-mount", std::io::Error::last_os_error().to_string())
                .with_span(span),
        );
    }
    let entries = unsafe { std::slice::from_raw_parts(entries, count as usize) };
    Ok(entries
        .iter()
        .map(|entry| MountSource {
            filesystem: c_char_array_to_string(&entry.f_mntfromname),
            mounted_on: PathBuf::from(c_char_array_to_string(&entry.f_mntonname)),
            fstype: c_char_array_to_string(&entry.f_fstypename),
        })
        .collect())
}

#[cfg(target_os = "macos")]
fn c_char_array_to_string(chars: &[libc::c_char]) -> String {
    let bytes = chars
        .iter()
        .map(|ch| *ch as u8)
        .take_while(|byte| *byte != 0)
        .collect::<Vec<_>>();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn mount_sources(span: Span) -> Result<Vec<MountSource>, RuntimeError> {
    Err(RuntimeError::new(
        "fs-mount",
        "mount discovery is unsupported on this platform",
    )
    .with_span(span))
}

#[cfg(test)]
mod tests {
    use super::{df_capacity_percent, mount_for, mounts};
    use crate::source::{SourceId, Span};
    use std::path::Path;

    fn test_span() -> Span {
        Span::new(SourceId::new(0), 0, 0)
    }

    #[test]
    fn df_capacity_percent_matches_busybox_df() {
        assert_eq!(df_capacity_percent(74, 26), 74);
        assert_eq!(df_capacity_percent(741, 259), 75);
        assert_eq!(df_capacity_percent(0, 0), 0);
        assert_eq!(df_capacity_percent(74, 27), 74);
    }

    #[test]
    fn mounts_include_root_and_positive_counters() {
        let mounts = mounts(test_span()).expect("read mounts");
        assert!(
            mounts
                .iter()
                .any(|mount| mount.mounted_on == Path::new("/"))
        );
        let root = mount_for(Path::new("/"), test_span()).expect("root mount");
        assert_eq!(root.mounted_on, Path::new("/"));
        assert!(root.blocks_1k > 0);
    }
}

pub fn gitroot(start: PathBuf, span: Span) -> Result<PathBuf, RuntimeError> {
    let mut current = start;
    loop {
        if current.join(".git").exists() {
            return Ok(current);
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => {
                return Err(
                    RuntimeError::new("not-a-git-repo", "not a git repository").with_span(span)
                );
            }
        }
    }
}

pub(crate) fn executable(path: PathBuf, span: Span) -> Result<bool, RuntimeError> {
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|error| RuntimeError::new("fs-executable", error.to_string()).with_span(span))?;
    Ok(metadata.file_type().is_file() && metadata.permissions().mode() & 0o111 != 0)
}

pub(crate) fn copy_file(
    source: PathBuf,
    dest: PathBuf,
    overwrite: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    let source_metadata = std::fs::symlink_metadata(&source)
        .map_err(|error| RuntimeError::new("fs-copy", error.to_string()).with_span(span))?;
    if !source_metadata.file_type().is_file() {
        return Err(RuntimeError::new("fs-copy", "source is not a regular file").with_span(span));
    }
    if let Ok(dest_metadata) = std::fs::symlink_metadata(&dest) {
        if dest_metadata.file_type().is_symlink() {
            return Err(RuntimeError::new("fs-copy", "destination is a symlink").with_span(span));
        }
        if !overwrite {
            return Err(RuntimeError::new("fs-copy", "destination exists").with_span(span));
        }
    }

    let mut input = std::fs::File::open(&source)
        .map_err(|error| RuntimeError::new("fs-copy", error.to_string()).with_span(span))?;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .create_new(!overwrite)
        .truncate(overwrite)
        .open(&dest)
        .map_err(|error| RuntimeError::new("fs-copy", error.to_string()).with_span(span))?;
    std::io::copy(&mut input, &mut output)
        .map(|_| ())
        .map_err(|error| RuntimeError::new("fs-copy", error.to_string()).with_span(span))
}

pub(crate) fn copy_tree(
    source: PathBuf,
    dest: PathBuf,
    overwrite: bool,
    parents: bool,
    follow_symlinks: bool,
    span: Span,
) -> Result<Value, RuntimeError> {
    let source_metadata = std::fs::symlink_metadata(&source)
        .map_err(|error| RuntimeError::new("fs-copy-tree", error.to_string()).with_span(span))?;
    if !source_metadata.file_type().is_dir() {
        return Err(RuntimeError::new("fs-copy-tree", "source is not a directory").with_span(span));
    }
    match std::fs::symlink_metadata(&dest) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(
                RuntimeError::new("fs-copy-tree", "destination is a symlink").with_span(span),
            );
        }
        Ok(metadata) if !metadata.file_type().is_dir() => {
            return Err(
                RuntimeError::new("fs-copy-tree", "destination is not a directory").with_span(span),
            );
        }
        Ok(_) if !overwrite => {
            return Err(RuntimeError::new("fs-copy-tree", "destination exists").with_span(span));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if parents {
                let parent = dest.parent().unwrap_or_else(|| Path::new("."));
                std::fs::create_dir_all(parent).map_err(|error| {
                    RuntimeError::new("fs-copy-tree", error.to_string()).with_span(span)
                })?;
            }
        }
        Err(error) => {
            return Err(RuntimeError::new("fs-copy-tree", error.to_string()).with_span(span));
        }
    }

    let mut stats = CopyTreeStats::default();
    copy_tree_inner(&source, &dest, overwrite, follow_symlinks, span, &mut stats)?;
    Ok(copy_tree_record(stats))
}

pub(crate) fn rename_path(
    source: PathBuf,
    dest: PathBuf,
    overwrite: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    match std::fs::symlink_metadata(&dest) {
        Ok(_) if !overwrite => {
            return Err(RuntimeError::new("fs-rename", "destination exists").with_span(span));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(RuntimeError::new("fs-rename", error.to_string()).with_span(span));
        }
    }
    std::fs::rename(source, dest)
        .map_err(|error| RuntimeError::new("fs-rename", error.to_string()).with_span(span))
}

pub(crate) fn remove_path(path: PathBuf, missing_ok: bool, span: Span) -> Result<(), RuntimeError> {
    remove_path_with_policy(path, true, missing_ok, span)
}

pub(crate) fn remove_path_with_policy(
    path: PathBuf,
    recursive: bool,
    missing_ok: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    let result = match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_dir() && recursive => std::fs::remove_dir_all(&path),
        Ok(metadata) if metadata.is_dir() => std::fs::remove_dir(&path),
        Ok(_) => std::fs::remove_file(&path),
        Err(error) if missing_ok && error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    };
    result.map_err(|error| RuntimeError::new("fs-remove", error.to_string()).with_span(span))
}

pub(crate) fn mkdir_path(
    path: PathBuf,
    parents: bool,
    mode: Option<u32>,
    span: Span,
) -> Result<(), RuntimeError> {
    if let Some(mode) = mode
        && mode > 0o7777
    {
        return Err(RuntimeError::new("fs-mkdir", "mode is out of range").with_span(span));
    }
    let result = if parents {
        std::fs::create_dir_all(&path)
    } else {
        std::fs::create_dir(&path)
    };
    result.map_err(|error| RuntimeError::new("fs-mkdir", error.to_string()).with_span(span))?;
    if let Some(mode) = mode {
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
            .map_err(|error| RuntimeError::new("fs-mkdir", error.to_string()).with_span(span))?;
    }
    Ok(())
}

pub(crate) fn remove_manifest(
    root: PathBuf,
    manifest: Vec<PathValue>,
    missing_ok: bool,
    prune_dirs: bool,
    span: Span,
) -> Result<Value, RuntimeError> {
    let root_metadata = std::fs::symlink_metadata(&root).map_err(|error| {
        RuntimeError::new("fs-remove-manifest", error.to_string()).with_span(span)
    })?;
    if !root_metadata.file_type().is_dir() {
        return Err(
            RuntimeError::new("fs-remove-manifest", "root is not a directory").with_span(span),
        );
    }
    let mut stats = RemoveManifestStats::default();
    let mut parent_dirs = Vec::new();
    for entry in manifest {
        let relative = clean_relative_manifest_path(&pathbuf_from_path_value(&entry), span)?;
        let target = root.join(&relative);
        match std::fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.file_type().is_dir() => {
                std::fs::remove_dir(&target).map_err(|error| {
                    RuntimeError::new("fs-remove-manifest", error.to_string()).with_span(span)
                })?;
                stats.removed += 1;
                collect_manifest_parents(&relative, &mut parent_dirs);
            }
            Ok(_) => {
                std::fs::remove_file(&target).map_err(|error| {
                    RuntimeError::new("fs-remove-manifest", error.to_string()).with_span(span)
                })?;
                stats.removed += 1;
                collect_manifest_parents(&relative, &mut parent_dirs);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && missing_ok => {
                stats.missing += 1;
            }
            Err(error) => {
                return Err(
                    RuntimeError::new("fs-remove-manifest", error.to_string()).with_span(span)
                );
            }
        }
    }

    if prune_dirs {
        parent_dirs.sort_unstable_by(|left, right| {
            right
                .components()
                .count()
                .cmp(&left.components().count())
                .then_with(|| path_bytes(left).cmp(&path_bytes(right)))
        });
        parent_dirs.dedup();
        for relative in parent_dirs {
            match std::fs::remove_dir(root.join(relative)) {
                Ok(()) => stats.pruned_dirs += 1,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                    ) => {}
                Err(error) => {
                    return Err(
                        RuntimeError::new("fs-remove-manifest", error.to_string()).with_span(span)
                    );
                }
            }
        }
    }

    Ok(remove_manifest_record(stats))
}

pub(crate) fn remove_dir(path: PathBuf, span: Span) -> Result<(), RuntimeError> {
    std::fs::remove_dir(path)
        .map_err(|error| RuntimeError::new("fs-remove-dir", error.to_string()).with_span(span))
}

pub(crate) fn touch_path(path: PathBuf, create: bool, span: Span) -> Result<(), RuntimeError> {
    std::fs::OpenOptions::new()
        .create(create)
        .append(true)
        .open(&path)
        .map_err(|error| RuntimeError::new("fs-touch", error.to_string()).with_span(span))?;
    rfs::utimensat(
        CWD,
        &path,
        &Timestamps {
            last_access: Timespec {
                tv_sec: 0,
                tv_nsec: UTIME_NOW as _,
            },
            last_modification: Timespec {
                tv_sec: 0,
                tv_nsec: UTIME_NOW as _,
            },
        },
        AtFlags::empty(),
    )
    .map_err(|error| RuntimeError::new("fs-touch", error.to_string()).with_span(span))
}

pub(crate) fn touch_path_from(
    path: PathBuf,
    reference: &Path,
    span: Span,
) -> Result<(), RuntimeError> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| RuntimeError::new("fs-touch", error.to_string()).with_span(span))?;
    let metadata = std::fs::metadata(reference)
        .map_err(|error| RuntimeError::new("fs-touch", error.to_string()).with_span(span))?;
    rfs::utimensat(
        CWD,
        &path,
        &Timestamps {
            last_access: Timespec {
                tv_sec: metadata.atime(),
                tv_nsec: metadata.atime_nsec() as _,
            },
            last_modification: Timespec {
                tv_sec: metadata.mtime(),
                tv_nsec: metadata.mtime_nsec() as _,
            },
        },
        AtFlags::empty(),
    )
    .map_err(|error| RuntimeError::new("fs-touch", error.to_string()).with_span(span))
}

pub(crate) fn truncate_path(path: PathBuf, size: i64, span: Span) -> Result<(), RuntimeError> {
    if size < 0 {
        return Err(RuntimeError::new("fs-truncate", "size cannot be negative").with_span(span));
    }
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|error| RuntimeError::new("fs-truncate", error.to_string()).with_span(span))?;
    file.set_len(size as u64)
        .map_err(|error| RuntimeError::new("fs-truncate", error.to_string()).with_span(span))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn install_file(
    source: PathBuf,
    dest: PathBuf,
    mode: i64,
    parents: bool,
    overwrite: bool,
    owner_uid: Option<i64>,
    group_gid: Option<i64>,
    span: Span,
) -> Result<(), RuntimeError> {
    let source_metadata = std::fs::symlink_metadata(&source)
        .map_err(|error| RuntimeError::new("fs-install", error.to_string()).with_span(span))?;
    if !source_metadata.file_type().is_file() {
        return Err(
            RuntimeError::new("fs-install", "source is not a regular file").with_span(span),
        );
    }
    if !(0..=0o7777).contains(&mode) {
        return Err(RuntimeError::new("fs-install", "mode is out of range").with_span(span));
    }
    if let Ok(dest_metadata) = std::fs::symlink_metadata(&dest) {
        if dest_metadata.file_type().is_symlink() {
            return Err(RuntimeError::new("fs-install", "destination is a symlink").with_span(span));
        }
        if !overwrite {
            return Err(RuntimeError::new("fs-install", "destination exists").with_span(span));
        }
    }

    let parent = dest.parent().unwrap_or_else(|| Path::new("."));
    if parents {
        std::fs::create_dir_all(parent)
            .map_err(|error| RuntimeError::new("fs-install", error.to_string()).with_span(span))?;
    }

    let mut input = std::fs::File::open(&source)
        .map_err(|error| RuntimeError::new("fs-install", error.to_string()).with_span(span))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| RuntimeError::new("fs-install", error.to_string()).with_span(span))?;
    std::io::copy(&mut input, &mut temp)
        .map_err(|error| RuntimeError::new("fs-install", error.to_string()).with_span(span))?;
    temp.as_file()
        .set_permissions(std::fs::Permissions::from_mode(mode as u32))
        .map_err(|error| RuntimeError::new("fs-install", error.to_string()).with_span(span))?;
    if let Some(uid) = owner_uid {
        chown_path(temp.path().to_path_buf(), uid, true, span)?;
    }
    if let Some(gid) = group_gid {
        chgrp_path(temp.path().to_path_buf(), gid, true, span)?;
    }
    if overwrite {
        temp.persist(&dest).map(|_| ()).map_err(|error| {
            RuntimeError::new("fs-install", error.error.to_string()).with_span(span)
        })
    } else {
        temp.persist_noclobber(&dest).map(|_| ()).map_err(|error| {
            RuntimeError::new("fs-install", error.error.to_string()).with_span(span)
        })
    }
}

pub(crate) fn chmod_path(path: PathBuf, mode: i64, span: Span) -> Result<(), RuntimeError> {
    if !(0..=0o7777).contains(&mode) {
        return Err(RuntimeError::new("fs-chmod", "mode is out of range").with_span(span));
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode as u32))
        .map_err(|error| RuntimeError::new("fs-chmod", error.to_string()).with_span(span))
}

pub(crate) fn chown_path(
    path: PathBuf,
    uid: i64,
    follow_symlinks: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    let uid = uid_value(uid, "fs-chown", span)?;
    let metadata = metadata_for_owner_policy(&path, follow_symlinks, "fs-chown", span)?;
    if metadata.uid() == uid {
        return Ok(());
    }
    let flags = if follow_symlinks {
        AtFlags::empty()
    } else {
        AtFlags::SYMLINK_NOFOLLOW
    };
    let owner = rustix::process::Uid::from_raw(uid);
    rfs::chownat(CWD, &path, Some(owner), None, flags)
        .map_err(|error| RuntimeError::new("fs-chown", error.to_string()).with_span(span))
}

pub(crate) fn chgrp_path(
    path: PathBuf,
    gid: i64,
    follow_symlinks: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    let gid = gid_value(gid, "fs-chgrp", span)?;
    let metadata = metadata_for_owner_policy(&path, follow_symlinks, "fs-chgrp", span)?;
    if metadata.gid() == gid {
        return Ok(());
    }
    let flags = if follow_symlinks {
        AtFlags::empty()
    } else {
        AtFlags::SYMLINK_NOFOLLOW
    };
    let group = Gid::from_raw(gid);
    rfs::chownat(CWD, &path, None, Some(group), flags)
        .map_err(|error| RuntimeError::new("fs-chgrp", error.to_string()).with_span(span))
}

pub(crate) fn mkfifo_path(path: PathBuf, mode: i64, span: Span) -> Result<(), RuntimeError> {
    if !(0..=0o7777).contains(&mode) {
        return Err(RuntimeError::new("fs-mkfifo", "mode is out of range").with_span(span));
    }
    let c_path = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        RuntimeError::new("fs-mkfifo", "paths cannot contain NUL bytes").with_span(span)
    })?;
    // rustix's `mkfifoat` is `#[cfg(not(apple))]`, so there is no portable
    // rustix equivalent for this cross-platform path; stay on libc.
    let result = unsafe { libc::mkfifo(c_path.as_ptr(), mode as libc::mode_t) };
    if result == 0 {
        Ok(())
    } else {
        Err(
            RuntimeError::new("fs-mkfifo", std::io::Error::last_os_error().to_string())
                .with_span(span),
        )
    }
}

pub(crate) fn fsync_path(path: PathBuf, span: Span) -> Result<(), RuntimeError> {
    let file = std::fs::File::open(path)
        .map_err(|error| RuntimeError::new("fs-fsync", error.to_string()).with_span(span))?;
    file.sync_all()
        .map_err(|error| RuntimeError::new("fs-fsync", error.to_string()).with_span(span))
}

pub(crate) fn sync_filesystems() {
    rustix::fs::sync();
}

pub(crate) fn hardlink(source: PathBuf, path: PathBuf, span: Span) -> Result<(), RuntimeError> {
    std::fs::hard_link(source, path)
        .map_err(|error| RuntimeError::new("fs-hardlink", error.to_string()).with_span(span))
}

pub(crate) fn symlink_path(target: PathBuf, path: PathBuf, span: Span) -> Result<(), RuntimeError> {
    std::os::unix::fs::symlink(target, path)
        .map_err(|error| RuntimeError::new("fs-symlink", error.to_string()).with_span(span))
}

pub(crate) fn unlink(path: PathBuf, span: Span) -> Result<(), RuntimeError> {
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_dir() => {
            Err(RuntimeError::new("fs-unlink", "path is a directory").with_span(span))
        }
        Ok(_) => std::fs::remove_file(path)
            .map_err(|error| RuntimeError::new("fs-unlink", error.to_string()).with_span(span)),
        Err(error) => Err(RuntimeError::new("fs-unlink", error.to_string()).with_span(span)),
    }
}

pub(crate) fn readlink(path: PathBuf, span: Span) -> Result<Value, RuntimeError> {
    let target = std::fs::read_link(path)
        .map_err(|error| RuntimeError::new("fs-readlink", error.to_string()).with_span(span))?;
    Ok(Value::Path(
        PathValue::new(path_bytes(&target)).map_err(|error| error.with_span(span))?,
    ))
}

pub(crate) fn lock_path(
    path: PathBuf,
    shared: bool,
    nonblocking: bool,
    span: Span,
) -> Result<std::fs::File, RuntimeError> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|error| RuntimeError::new("fs-lock", error.to_string()).with_span(span))?;
    let operation = match (shared, nonblocking) {
        (true, true) => FlockOperation::NonBlockingLockShared,
        (true, false) => FlockOperation::LockShared,
        (false, true) => FlockOperation::NonBlockingLockExclusive,
        (false, false) => FlockOperation::LockExclusive,
    };
    if let Err(error) = rfs::flock(&file, operation) {
        Err(RuntimeError::new("fs-lock", error.to_string()).with_span(span))
    } else {
        Ok(file)
    }
}

pub(crate) fn unlock_file(file: &std::fs::File, span: Span) -> Result<(), RuntimeError> {
    rfs::flock(file, FlockOperation::Unlock)
        .map_err(|error| RuntimeError::new("fs-lock", error.to_string()).with_span(span))
}

pub(crate) fn write_atomic(path: PathBuf, data: &[u8], span: Span) -> Result<(), RuntimeError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let leaf = path
        .file_name()
        .ok_or_else(|| RuntimeError::new("fs-write", "path must name a file").with_span(span))?;
    let parent_dir = CapDir::open_ambient_dir(parent, ambient_authority())
        .map_err(|error| RuntimeError::new("fs-write", error.to_string()).with_span(span))?;
    let mut temp = TempFile::new(&parent_dir)
        .map_err(|error| RuntimeError::new("fs-write", error.to_string()).with_span(span))?;
    temp.write_all(data)
        .map_err(|error| RuntimeError::new("fs-write", error.to_string()).with_span(span))?;
    temp.as_file()
        .sync_all()
        .map_err(|error| RuntimeError::new("fs-write", error.to_string()).with_span(span))?;
    temp.replace(leaf)
        .map_err(|error| RuntimeError::new("fs-write", error.to_string()).with_span(span))
}

fn copy_tree_inner(
    source: &Path,
    dest: &Path,
    overwrite: bool,
    follow_symlinks: bool,
    span: Span,
    stats: &mut CopyTreeStats,
) -> Result<(), RuntimeError> {
    let metadata = if follow_symlinks {
        std::fs::metadata(source)
    } else {
        std::fs::symlink_metadata(source)
    }
    .map_err(|error| RuntimeError::new("fs-copy-tree", error.to_string()).with_span(span))?;

    if metadata.file_type().is_dir() {
        match std::fs::symlink_metadata(dest) {
            Ok(existing) if existing.file_type().is_symlink() => {
                return Err(
                    RuntimeError::new("fs-copy-tree", "destination is a symlink").with_span(span),
                );
            }
            Ok(existing) if !existing.file_type().is_dir() => {
                return Err(
                    RuntimeError::new("fs-copy-tree", "destination is not a directory")
                        .with_span(span),
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(dest).map_err(|error| {
                    RuntimeError::new("fs-copy-tree", error.to_string()).with_span(span)
                })?;
                stats.dirs += 1;
            }
            Err(error) => {
                return Err(RuntimeError::new("fs-copy-tree", error.to_string()).with_span(span));
            }
        }
        std::fs::set_permissions(dest, std::fs::Permissions::from_mode(metadata.mode())).map_err(
            |error| RuntimeError::new("fs-copy-tree", error.to_string()).with_span(span),
        )?;
        let mut children = std::fs::read_dir(source)
            .map_err(|error| RuntimeError::new("fs-copy-tree", error.to_string()).with_span(span))?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                RuntimeError::new("fs-copy-tree", error.to_string()).with_span(span)
            })?;
        children.sort_unstable_by_key(|path| path_bytes(path));
        for child in children {
            let name = child.file_name().ok_or_else(|| {
                RuntimeError::new("fs-copy-tree", "source entry has no file name").with_span(span)
            })?;
            copy_tree_inner(
                &child,
                &dest.join(name),
                overwrite,
                follow_symlinks,
                span,
                stats,
            )?;
        }
    } else if metadata.file_type().is_file() {
        copy_regular_file(source, dest, &metadata, overwrite, span)?;
        stats.files += 1;
    } else if metadata.file_type().is_symlink() {
        let target = std::fs::read_link(source).map_err(|error| {
            RuntimeError::new("fs-copy-tree", error.to_string()).with_span(span)
        })?;
        copy_symlink(&target, dest, overwrite, span)?;
        stats.symlinks += 1;
    } else {
        return Err(
            RuntimeError::new("fs-copy-tree", "source entry is not copyable").with_span(span),
        );
    }
    Ok(())
}

fn copy_regular_file(
    source: &Path,
    dest: &Path,
    metadata: &std::fs::Metadata,
    overwrite: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    if let Ok(existing) = std::fs::symlink_metadata(dest) {
        if existing.file_type().is_symlink() {
            return Err(
                RuntimeError::new("fs-copy-tree", "destination is a symlink").with_span(span),
            );
        }
        if existing.file_type().is_dir() {
            return Err(
                RuntimeError::new("fs-copy-tree", "destination is a directory").with_span(span),
            );
        }
        if !overwrite {
            return Err(RuntimeError::new("fs-copy-tree", "destination exists").with_span(span));
        }
    }
    let mut input = std::fs::File::open(source)
        .map_err(|error| RuntimeError::new("fs-copy-tree", error.to_string()).with_span(span))?;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .create_new(!overwrite)
        .truncate(overwrite)
        .open(dest)
        .map_err(|error| RuntimeError::new("fs-copy-tree", error.to_string()).with_span(span))?;
    std::io::copy(&mut input, &mut output)
        .map_err(|error| RuntimeError::new("fs-copy-tree", error.to_string()).with_span(span))?;
    output
        .set_permissions(std::fs::Permissions::from_mode(metadata.mode()))
        .map_err(|error| RuntimeError::new("fs-copy-tree", error.to_string()).with_span(span))
}

fn copy_symlink(
    target: &Path,
    path: &Path,
    overwrite: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    match std::fs::symlink_metadata(path) {
        Ok(existing) if existing.file_type().is_dir() => {
            return Err(
                RuntimeError::new("fs-copy-tree", "destination is a directory").with_span(span),
            );
        }
        Ok(_) if !overwrite => {
            return Err(RuntimeError::new("fs-copy-tree", "destination exists").with_span(span));
        }
        Ok(_) => {
            std::fs::remove_file(path).map_err(|error| {
                RuntimeError::new("fs-copy-tree", error.to_string()).with_span(span)
            })?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(RuntimeError::new("fs-copy-tree", error.to_string()).with_span(span));
        }
    }
    std::os::unix::fs::symlink(target, path)
        .map_err(|error| RuntimeError::new("fs-copy-tree", error.to_string()).with_span(span))
}

fn copy_tree_record(stats: CopyTreeStats) -> Value {
    Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("files"), Value::Int(stats.files)),
        (Arc::from("dirs"), Value::Int(stats.dirs)),
        (Arc::from("symlinks"), Value::Int(stats.symlinks)),
    ]))
}

fn clean_relative_manifest_path(path: &Path, span: Span) -> Result<PathBuf, RuntimeError> {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => clean.push(name),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(RuntimeError::new(
                    "fs-remove-manifest",
                    "manifest paths must be relative and cannot contain `..`",
                )
                .with_span(span));
            }
        }
    }
    if clean.as_os_str().is_empty() {
        return Err(
            RuntimeError::new("fs-remove-manifest", "manifest path cannot be empty")
                .with_span(span),
        );
    }
    Ok(clean)
}

fn collect_manifest_parents(relative: &Path, parents: &mut Vec<PathBuf>) {
    let mut current = relative.parent();
    while let Some(parent) = current {
        if parent.as_os_str().is_empty() {
            break;
        }
        parents.push(parent.to_path_buf());
        current = parent.parent();
    }
}

fn remove_manifest_record(stats: RemoveManifestStats) -> Value {
    Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("removed"), Value::Int(stats.removed)),
        (Arc::from("missing"), Value::Int(stats.missing)),
        (Arc::from("pruned_dirs"), Value::Int(stats.pruned_dirs)),
    ]))
}

fn metadata_for_owner_policy(
    path: &Path,
    follow_symlinks: bool,
    kind: &'static str,
    span: Span,
) -> Result<std::fs::Metadata, RuntimeError> {
    let result = if follow_symlinks {
        std::fs::metadata(path)
    } else {
        std::fs::symlink_metadata(path)
    };
    result.map_err(|error| RuntimeError::new(kind, error.to_string()).with_span(span))
}

fn uid_value(uid: i64, kind: &'static str, span: Span) -> Result<u32, RuntimeError> {
    u32::try_from(uid).map_err(|_| RuntimeError::new(kind, "uid is out of range").with_span(span))
}

fn gid_value(gid: i64, kind: &'static str, span: Span) -> Result<u32, RuntimeError> {
    u32::try_from(gid).map_err(|_| RuntimeError::new(kind, "gid is out of range").with_span(span))
}

fn pathbuf_from_path_value(path: &PathValue) -> PathBuf {
    PathBuf::from(OsString::from_vec(path.bytes.clone()))
}

/// Everything needed to drive an ignore-backed recursive filesystem walk.
#[derive(Clone)]
pub(crate) struct WalkSpec {
    root: PathBuf,
    gitignore: bool,
    stat: bool,
    hidden: bool,
    emit: WalkEmit,
    ext_filter: Option<WalkExtFilter>,
    span: Span,
}

impl WalkSpec {
    pub(crate) fn supports_path_entry_fold(&self) -> bool {
        !self.stat && self.emit == WalkEmit::Files
    }

    fn entry_matches(&self, path: &Path, file_type: std::fs::FileType) -> bool {
        match self.emit {
            WalkEmit::All => true,
            WalkEmit::Files => {
                file_type.is_file()
                    && self
                        .ext_filter
                        .as_ref()
                        .is_none_or(|filter| filter.matches(path))
            }
            WalkEmit::Dirs => file_type.is_dir(),
        }
    }
}

pub(crate) struct WalkEntry {
    path: PathBuf,
}

impl WalkEntry {
    pub(crate) fn ext_text(&self) -> Cow<'_, str> {
        self.path
            .extension()
            .map(|extension| extension.to_string_lossy())
            .unwrap_or(Cow::Borrowed(""))
    }
}

/// A `LiveStream` for an ignore-backed recursive walk. Lazy: traversal is not
/// started until the first `next()`, so a fusing fold consumer can take the
/// pending `WalkSpec` and re-drive the traversal with its own worker strategy.
pub(crate) enum IgnoreWalkStream {
    Pending(WalkSpec),
    Running {
        iter: Box<ignore::Walk>,
        spec: WalkSpec,
    },
    Consumed,
}

impl LiveStream for IgnoreWalkStream {
    fn next(&mut self, _span: Span) -> Result<Option<Value>, RuntimeError> {
        if let Self::Pending(spec) = self {
            let builder = ignore_walk_builder(spec, 1);
            *self = Self::Running {
                iter: Box::new(builder.build()),
                spec: spec.clone(),
            };
        }
        let Self::Running { iter, spec } = self else {
            return Ok(None);
        };
        loop {
            let Some(result) = iter.next() else {
                return Ok(None);
            };
            let item = match result {
                Ok(entry) => match raw_walk_entry(spec, &entry) {
                    Ok(Some(entry)) => entry,
                    Ok(None) => continue,
                    Err(error) => return Err(error),
                },
                Err(error) => {
                    return Err(RuntimeError::new("fs-walk", error.to_string())
                        .with_span(spec.span));
                }
            };
            return item.record(spec.stat, spec.span).map(Some);
        }
    }
}

impl IgnoreWalkStream {
    pub(crate) fn take_pending_spec(&mut self) -> Option<WalkSpec> {
        let Self::Pending(spec) = self else {
            return None;
        };
        let spec = spec.clone();
        *self = Self::Consumed;
        Some(spec)
    }
}

fn walk_pool_size() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .max(1)
}

pub(crate) fn parallel_walk_fold<A, Make, Step>(
    spec: WalkSpec,
    jobs: usize,
    make_acc: Make,
    step: Step,
) -> Vec<Result<A, RuntimeError>>
where
    A: Send,
    Make: Fn() -> A + Sync,
    Step: Fn(&mut A, Result<Value, RuntimeError>) -> Result<(), RuntimeError> + Sync,
{
    parallel_walk_ignore_value_fold(spec, jobs, make_acc, step)
}

pub(crate) fn parallel_walk_ignore_entry_fold<A, Make, Step>(
    spec: WalkSpec,
    jobs: usize,
    make_acc: Make,
    step: Step,
) -> Vec<Result<A, RuntimeError>>
where
    A: Send,
    Make: Fn() -> A + Sync,
    Step: Fn(&mut A, Result<WalkEntry, RuntimeError>) -> Result<(), RuntimeError> + Sync,
{
    parallel_walk_ignore_fold(spec, jobs, make_acc, |acc, item| {
        step(acc, item.map(|item| WalkEntry { path: item.path }))
    })
}

fn parallel_walk_ignore_value_fold<A, Make, Step>(
    spec: WalkSpec,
    jobs: usize,
    make_acc: Make,
    step: Step,
) -> Vec<Result<A, RuntimeError>>
where
    A: Send,
    Make: Fn() -> A + Sync,
    Step: Fn(&mut A, Result<Value, RuntimeError>) -> Result<(), RuntimeError> + Sync,
{
    let stat = spec.stat;
    let span = spec.span;
    parallel_walk_ignore_fold(spec, jobs, make_acc, |acc, item| {
        step(acc, item.and_then(|item| item.record(stat, span)))
    })
}

fn parallel_walk_ignore_fold<A, Make, Step>(
    spec: WalkSpec,
    jobs: usize,
    make_acc: Make,
    step: Step,
) -> Vec<Result<A, RuntimeError>>
where
    A: Send,
    Make: Fn() -> A + Sync,
    Step: Fn(&mut A, Result<RawWalkEntry, RuntimeError>) -> Result<(), RuntimeError> + Sync,
{
    let (tx, rx) = crossbeam_channel::unbounded();
    let builder = ignore_walk_builder(&spec, jobs);
    let walker = builder.build_parallel();
    let make_acc = &make_acc;
    let step = &step;
    let span = spec.span;
    walker.run(|| {
        let tx = tx.clone();
        let spec = spec.clone();
        let mut worker = IgnoreFoldWorker {
            acc: Some(make_acc()),
            failure: None,
            tx,
        };
        Box::new(move |result| {
            let item = match result {
                Ok(entry) => match raw_walk_entry(&spec, &entry) {
                    Ok(Some(entry)) => Ok(entry),
                    Ok(None) => return ignore::WalkState::Continue,
                    Err(error) => Err(error),
                },
                Err(error) => Err(RuntimeError::new("fs-walk", error.to_string()).with_span(span)),
            };
            let acc = worker.acc.as_mut().expect("ignore fold worker is live");
            match step(acc, item) {
                Ok(()) => ignore::WalkState::Continue,
                Err(error) => {
                    worker.failure = Some(error);
                    ignore::WalkState::Quit
                }
            }
        })
    });
    drop(tx);
    rx.into_iter().collect()
}

fn ignore_walk_builder(spec: &WalkSpec, jobs: usize) -> ignore::WalkBuilder {
    let mut builder = ignore::WalkBuilder::new(&spec.root);
    builder
        .hidden(!spec.hidden)
        .ignore(spec.gitignore)
        .parents(spec.gitignore)
        .git_ignore(spec.gitignore)
        .git_global(spec.gitignore)
        .git_exclude(spec.gitignore)
        .require_git(false)
        .threads(jobs);
    if spec.gitignore {
        builder.add_custom_ignore_filename(".fdignore");
    }
    builder
}

struct RawWalkEntry {
    path: PathBuf,
    file_type: std::fs::FileType,
}

impl RawWalkEntry {
    fn record(self, stat: bool, span: Span) -> Result<Value, RuntimeError> {
        if stat {
            let metadata = std::fs::symlink_metadata(&self.path)
                .map_err(|error| RuntimeError::new("fs-walk", error.to_string()).with_span(span))?;
            fs_entry_record(&self.path, &metadata)
        } else {
            Ok(Value::FsEntry(FsEntryValue::new(self.path, self.file_type)))
        }
    }
}

fn raw_walk_entry(
    spec: &WalkSpec,
    entry: &ignore::DirEntry,
) -> Result<Option<RawWalkEntry>, RuntimeError> {
    let Some(file_type) = entry.file_type() else {
        return Ok(None);
    };
    let path = entry.path();
    if !spec.entry_matches(path, file_type) {
        return Ok(None);
    }
    Ok(Some(RawWalkEntry {
        path: path.to_path_buf(),
        file_type,
    }))
}

struct IgnoreFoldWorker<A> {
    acc: Option<A>,
    failure: Option<RuntimeError>,
    tx: Sender<Result<A, RuntimeError>>,
}

impl<A> Drop for IgnoreFoldWorker<A> {
    fn drop(&mut self) {
        let result = match self.failure.take() {
            Some(error) => Err(error),
            None => Ok(self.acc.take().expect("ignore fold worker accumulator")),
        };
        let _ = self.tx.send(result);
    }
}

fn push_fs_entry(
    output: &mut Vec<StreamItem>,
    path: &Path,
    metadata: &std::fs::Metadata,
) -> Result<(), RuntimeError> {
    let index = output.len();
    output.push(StreamItem {
        value: fs_entry_record(path, metadata)?,
        index,
        source_span: None,
    });
    Ok(())
}

pub(crate) fn fs_entry_record(
    path: &Path,
    metadata: &std::fs::Metadata,
) -> Result<Value, RuntimeError> {
    let mode = metadata.mode() as i64;
    Ok(Value::Record(RecordMap::shaped(
        &FS_ENTRY_SHAPE,
        vec![
            Value::Int(metadata.atime()),
            Value::Int(metadata.blocks() as i64),
            Value::Bool(mode_executable(mode)),
            Value::Str(
                path.extension()
                    .map(|name| Arc::<str>::from(name.to_string_lossy().as_ref()))
                    .unwrap_or_else(|| "".into()),
            ),
            Value::Int(metadata.gid() as i64),
            Value::Bool(mode_group_executable(mode)),
            Value::Str(if metadata.file_type().is_dir() {
                V_KIND_DIR.clone()
            } else if metadata.file_type().is_file() {
                V_KIND_FILE.clone()
            } else if metadata.file_type().is_symlink() {
                V_KIND_SYMLINK.clone()
            } else {
                V_KIND_OTHER.clone()
            }),
            Value::Int(mode),
            Value::Int(metadata.mtime()),
            Value::Str(
                path.file_name()
                    .map(|name| Arc::<str>::from(name.to_string_lossy().as_ref()))
                    .unwrap_or_else(|| "".into()),
            ),
            Value::Bool(mode_other_executable(mode)),
            Value::Bool(mode_owner_executable(mode)),
            Value::Path(crate::runtime::value::PathValue::new(path_bytes(path))?),
            Value::Bool(mode_setgid(mode)),
            Value::Bool(mode_setuid(mode)),
            Value::Int(metadata.len() as i64),
            Value::Bool(mode_sticky(mode)),
            Value::Int(metadata.uid() as i64),
            Value::Bool(mode_world_writable(mode)),
        ],
    )))
}

pub(crate) fn fs_entry_record_cheap(
    path: &Path,
    file_type: std::fs::FileType,
) -> Result<Value, RuntimeError> {
    Ok(Value::Record(RecordMap::sparse_shaped_array(
        &FS_ENTRY_SHAPE,
        FS_ENTRY_CHEAP_DEFAULTS.as_slice(),
        [
            (
                FS_ENTRY_EXT_INDEX,
                Value::Str(
                    path.extension()
                        .map(|name| Arc::<str>::from(name.to_string_lossy().as_ref()))
                        .unwrap_or_else(|| "".into()),
                ),
            ),
            (
                FS_ENTRY_KIND_INDEX,
                Value::Str(if file_type.is_dir() {
                    V_KIND_DIR.clone()
                } else if file_type.is_file() {
                    V_KIND_FILE.clone()
                } else if file_type.is_symlink() {
                    V_KIND_SYMLINK.clone()
                } else {
                    V_KIND_OTHER.clone()
                }),
            ),
            (
                FS_ENTRY_NAME_INDEX,
                Value::Str(
                    path.file_name()
                        .map(|name| Arc::<str>::from(name.to_string_lossy().as_ref()))
                        .unwrap_or_else(|| "".into()),
                ),
            ),
            (
                FS_ENTRY_PATH_INDEX,
                Value::Path(crate::runtime::value::PathValue::new(path_bytes(path))?),
            ),
        ],
    )))
}
