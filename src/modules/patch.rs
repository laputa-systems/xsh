#![allow(clippy::single_call_fn)]

use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use diffy::apply_bytes;
use diffy::patch_set::{FileMode, FileOperation, ParseOptions, PatchKind, PatchSet};
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

pub(crate) fn apply(
    root: PathBuf,
    text: &str,
    strip_components: i64,
    overwrite: bool,
    span: Span,
) -> Result<Value, RuntimeError> {
    if strip_components < 0 {
        return Err(
            RuntimeError::new("patch", "strip_components cannot be negative").with_span(span),
        );
    }
    ensure_root(&root, span)?;
    let options = if text.lines().any(|line| line.starts_with("diff --git ")) {
        ParseOptions::gitdiff()
    } else {
        ParseOptions::unidiff()
    };
    let patches = PatchSet::parse_bytes(text.as_bytes(), options);
    let mut files = 0_i64;
    let mut hunks = 0_i64;
    for file_patch in patches {
        let file_patch = file_patch
            .map_err(|error| RuntimeError::new("patch-parse", error.to_string()).with_span(span))?;
        reject_modes(file_patch.old_mode(), file_patch.new_mode(), span)?;
        let new_mode = file_patch.new_mode().copied();
        let patch_hunks = match file_patch.patch() {
            PatchKind::Text(patch) => patch.hunks().len() as i64,
            PatchKind::Binary(_) => {
                return Err(
                    RuntimeError::new("patch-binary", "binary patches are not supported")
                        .with_span(span),
                );
            }
        };
        let operation = file_patch.operation();
        match operation {
            FileOperation::Create(path) => {
                let target = output_path(&root, path, strip_components as usize, span)?;
                reject_existing_output(&target, overwrite, span)?;
                let patched = apply_patch_bytes(&[], file_patch.patch(), span)?;
                write_output(
                    &target,
                    &patched,
                    new_mode.or(Some(FileMode::Regular)),
                    None,
                    span,
                )?;
            }
            FileOperation::Delete(path) => {
                let target = existing_path(&root, path, strip_components as usize, span)?;
                let base =
                    fs::read(&target).map_err(|error| patch_error("patch-read", error, span))?;
                let patched = apply_patch_bytes(&base, file_patch.patch(), span)?;
                if !patched.is_empty() {
                    return Err(RuntimeError::new(
                        "patch-apply",
                        "delete patch leaves file content",
                    )
                    .with_span(span));
                }
                fs::remove_file(&target)
                    .map_err(|error| patch_error("patch-remove", error, span))?;
            }
            FileOperation::Modify { original, modified } => {
                let source = existing_path(&root, original, strip_components as usize, span)?;
                let target = output_path(&root, modified, strip_components as usize, span)?;
                if source != target {
                    reject_existing_output(&target, overwrite, span)?;
                }
                let metadata = checked_file_metadata(&source, span)?;
                let base =
                    fs::read(&source).map_err(|error| patch_error("patch-read", error, span))?;
                let patched = apply_patch_bytes(&base, file_patch.patch(), span)?;
                write_output(
                    &target,
                    &patched,
                    new_mode,
                    Some(metadata.permissions()),
                    span,
                )?;
                if source != target {
                    fs::remove_file(&source)
                        .map_err(|error| patch_error("patch-remove", error, span))?;
                }
            }
            FileOperation::Rename { from, to } => {
                apply_copy_or_rename(
                    &root,
                    from,
                    to,
                    strip_components as usize,
                    overwrite,
                    new_mode,
                    file_patch.patch(),
                    true,
                    span,
                )?;
            }
            FileOperation::Copy { from, to } => {
                apply_copy_or_rename(
                    &root,
                    from,
                    to,
                    strip_components as usize,
                    overwrite,
                    new_mode,
                    file_patch.patch(),
                    false,
                    span,
                )?;
            }
        }
        files += 1;
        hunks += patch_hunks;
    }
    Ok(Value::ok(Value::Record(
        crate::runtime::value::RecordMap::from([
            (Arc::from("files"), Value::Int(files)),
            (Arc::from("hunks"), Value::Int(hunks)),
        ]),
    )))
}

#[allow(clippy::too_many_arguments)]
fn apply_copy_or_rename(
    root: &Path,
    from: &[u8],
    to: &[u8],
    strip_components: usize,
    overwrite: bool,
    new_mode: Option<FileMode>,
    patch: &PatchKind<'_, [u8]>,
    remove_source: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    let source = existing_path(root, from, strip_components, span)?;
    let target = output_path(root, to, strip_components, span)?;
    reject_existing_output(&target, overwrite, span)?;
    let metadata = checked_file_metadata(&source, span)?;
    let base = fs::read(&source).map_err(|error| patch_error("patch-read", error, span))?;
    let patched = apply_patch_bytes(&base, patch, span)?;
    write_output(
        &target,
        &patched,
        new_mode,
        Some(metadata.permissions()),
        span,
    )?;
    if remove_source {
        fs::remove_file(&source).map_err(|error| patch_error("patch-remove", error, span))?;
    }
    Ok(())
}

fn apply_patch_bytes(
    base: &[u8],
    patch: &PatchKind<'_, [u8]>,
    span: Span,
) -> Result<Vec<u8>, RuntimeError> {
    match patch {
        PatchKind::Text(patch) => apply_bytes(base, patch)
            .map_err(|error| RuntimeError::new("patch-apply", error.to_string()).with_span(span)),
        PatchKind::Binary(_) => Err(RuntimeError::new(
            "patch-binary",
            "binary patches are not supported",
        )
        .with_span(span)),
    }
}

fn ensure_root(root: &Path, span: Span) -> Result<(), RuntimeError> {
    let metadata =
        fs::symlink_metadata(root).map_err(|error| patch_error("patch-root", error, span))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(RuntimeError::new("patch-root", "root must be a directory").with_span(span));
    }
    Ok(())
}

fn existing_path(
    root: &Path,
    raw: &[u8],
    strip_components: usize,
    span: Span,
) -> Result<PathBuf, RuntimeError> {
    let relative = clean_patch_path(raw, strip_components, span)?;
    validate_ancestors(root, &relative, false, span)?;
    let path = root.join(relative);
    checked_file_metadata(&path, span)?;
    Ok(path)
}

fn output_path(
    root: &Path,
    raw: &[u8],
    strip_components: usize,
    span: Span,
) -> Result<PathBuf, RuntimeError> {
    let relative = clean_patch_path(raw, strip_components, span)?;
    validate_ancestors(root, &relative, true, span)?;
    Ok(root.join(relative))
}

fn clean_patch_path(
    raw: &[u8],
    strip_components: usize,
    span: Span,
) -> Result<PathBuf, RuntimeError> {
    let mut stripped = raw;
    for _ in 0..strip_components {
        let Some(index) = stripped.iter().position(|byte| *byte == b'/') else {
            return Err(RuntimeError::new("patch-path", "empty patch path").with_span(span));
        };
        stripped = &stripped[index + 1..];
    }
    if stripped.is_empty() {
        return Err(RuntimeError::new("patch-path", "empty patch path").with_span(span));
    }
    let path = PathBuf::from(OsString::from_vec(stripped.to_vec()));
    let mut output = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => output.push(part),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(RuntimeError::new("patch-path", "invalid patch path").with_span(span));
            }
        }
    }
    if output.as_os_str().is_empty() {
        return Err(RuntimeError::new("patch-path", "empty patch path").with_span(span));
    }
    Ok(output)
}

fn validate_ancestors(
    root: &Path,
    relative: &Path,
    create_missing: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    let mut current = root.to_path_buf();
    let mut components = relative.components().peekable();
    while let Some(component) = components.next() {
        let Component::Normal(part) = component else {
            return Err(RuntimeError::new("patch-path", "invalid patch path").with_span(span));
        };
        current.push(part);
        if components.peek().is_none() {
            break;
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(RuntimeError::new("patch-escape", "symlink ancestor").with_span(span));
            }
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                return Err(
                    RuntimeError::new("patch-path", "ancestor is not a directory").with_span(span),
                );
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound && create_missing => {
                fs::create_dir(&current)
                    .map_err(|error| patch_error("patch-create-dir", error, span))?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(patch_error("patch-path", error, span));
            }
            Err(error) => return Err(patch_error("patch-path", error, span)),
        }
    }
    Ok(())
}

fn checked_file_metadata(path: &Path, span: Span) -> Result<fs::Metadata, RuntimeError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| patch_error("patch-path", error, span))?;
    if metadata.file_type().is_symlink() {
        return Err(RuntimeError::new("patch-escape", "symlink target").with_span(span));
    }
    if !metadata.is_file() {
        return Err(RuntimeError::new("patch-path", "target is not a file").with_span(span));
    }
    Ok(metadata)
}

fn reject_existing_output(path: &Path, overwrite: bool, span: Span) -> Result<(), RuntimeError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(RuntimeError::new("patch-escape", "symlink target").with_span(span))
        }
        Ok(metadata) if metadata.is_dir() => {
            Err(RuntimeError::new("patch-path", "target is a directory").with_span(span))
        }
        Ok(_) if !overwrite => {
            Err(RuntimeError::new("patch-path", "target exists").with_span(span))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(patch_error("patch-path", error, span)),
    }
}

fn write_output(
    path: &Path,
    data: &[u8],
    new_mode: Option<FileMode>,
    previous_permissions: Option<fs::Permissions>,
    span: Span,
) -> Result<(), RuntimeError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| patch_error("patch-write", error, span))?;
    temp.write_all(data)
        .map_err(|error| patch_error("patch-write", error, span))?;
    temp.persist(path)
        .map_err(|error| patch_error("patch-write", error.error, span))?;
    if let Some(mode) = mode_permissions(new_mode, previous_permissions, span)? {
        fs::set_permissions(path, mode).map_err(|error| patch_error("patch-mode", error, span))?;
    }
    Ok(())
}

fn mode_permissions(
    mode: Option<FileMode>,
    previous: Option<fs::Permissions>,
    span: Span,
) -> Result<Option<fs::Permissions>, RuntimeError> {
    match mode {
        Some(FileMode::Regular) => Ok(Some(fs::Permissions::from_mode(0o644))),
        Some(FileMode::Executable) => Ok(Some(fs::Permissions::from_mode(0o755))),
        Some(FileMode::Symlink) | Some(FileMode::Gitlink) => {
            Err(RuntimeError::new("patch-mode", "unsupported file mode").with_span(span))
        }
        None => Ok(previous),
    }
}

fn reject_modes(
    old_mode: Option<&FileMode>,
    new_mode: Option<&FileMode>,
    span: Span,
) -> Result<(), RuntimeError> {
    if matches!(old_mode, Some(FileMode::Symlink | FileMode::Gitlink))
        || matches!(new_mode, Some(FileMode::Symlink | FileMode::Gitlink))
    {
        return Err(RuntimeError::new("patch-mode", "unsupported file mode").with_span(span));
    }
    Ok(())
}

fn patch_error(kind: &str, error: io::Error, span: Span) -> RuntimeError {
    RuntimeError::new(kind, error.to_string()).with_span(span)
}
