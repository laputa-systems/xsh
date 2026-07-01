use crate::runtime::value::{PathValue, RuntimeError};
use crate::source::Span;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStringExt;
use std::path::{Component, Path, PathBuf};

use super::archive_error;

pub(super) fn archive_member_filters(
    members: Vec<PathValue>,
    span: Span,
) -> Result<Vec<PathBuf>, RuntimeError> {
    members
        .iter()
        .map(|member| archive_member_path(member, span))
        .collect()
}

pub(super) fn archive_member_selected(path: &Path, filters: &[PathBuf]) -> bool {
    filters.is_empty()
        || filters
            .iter()
            .any(|filter| path == filter || path.starts_with(filter))
}

pub(super) fn prepare_output_path(
    dest: &Path,
    archive_path: &Path,
    create_leaf_dir: bool,
    span: Span,
) -> Result<PathBuf, RuntimeError> {
    prepare_output_path_with_kind(dest, archive_path, create_leaf_dir, "archive-extract", span)
}

pub(super) fn prepare_output_path_with_kind(
    dest: &Path,
    archive_path: &Path,
    create_leaf_dir: bool,
    kind: &str,
    span: Span,
) -> Result<PathBuf, RuntimeError> {
    validate_archive_ancestors(dest, archive_path, create_leaf_dir, kind, span)?;
    archive_path_in(dest, archive_path, span)
}

fn validate_archive_ancestors(
    dest: &Path,
    archive_path: &Path,
    create_leaf_dir: bool,
    kind: &str,
    span: Span,
) -> Result<(), RuntimeError> {
    let mut current = dest.to_path_buf();
    let mut components = archive_path.components().peekable();
    while let Some(component) = components.next() {
        let Component::Normal(part) = component else {
            return Err(RuntimeError::new("archive-path", "invalid archive path").with_span(span));
        };
        current.push(part);
        let is_last = components.peek().is_none();
        if is_last && !create_leaf_dir {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(RuntimeError::new("archive-escape", "symlink escape").with_span(span));
            }
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                return Err(RuntimeError::new(kind, "ancestor is not a directory").with_span(span));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|error| archive_error(kind, error, span))?;
            }
            Err(error) => return Err(archive_error(kind, error, span)),
        }
    }
    Ok(())
}

pub(super) fn refuse_existing(
    path: &Path,
    overwrite: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    refuse_existing_with_kind(path, overwrite, "archive-extract", span)
}

pub(super) fn refuse_existing_with_kind(
    path: &Path,
    overwrite: bool,
    kind: &str,
    span: Span,
) -> Result<(), RuntimeError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(RuntimeError::new(kind, "destination is a symlink").with_span(span))
        }
        Ok(metadata) if metadata.is_dir() => {
            Err(RuntimeError::new(kind, "destination is a directory").with_span(span))
        }
        Ok(_) if !overwrite => Err(RuntimeError::new(kind, "destination exists").with_span(span)),
        Ok(_) => fs::remove_file(path).map_err(|error| archive_error(kind, error, span)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(archive_error(kind, error, span)),
    }
}

pub(super) fn strip_archive_path(
    path: &Path,
    strip_components: usize,
    span: Span,
) -> Result<Option<PathBuf>, RuntimeError> {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => clean.push(part),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(
                    RuntimeError::new("archive-path", "invalid archive path").with_span(span)
                );
            }
        }
    }
    if clean.as_os_str().is_empty() {
        return Ok(None);
    }
    let components = clean.components().collect::<Vec<_>>();
    if components.len() <= strip_components {
        return Ok(None);
    }
    let mut output = PathBuf::new();
    for component in components.into_iter().skip(strip_components) {
        let Component::Normal(part) = component else {
            return Err(RuntimeError::new("archive-path", "invalid archive path").with_span(span));
        };
        output.push(part);
    }
    Ok(Some(output))
}

pub(super) fn clean_archive_path(path: &Path, span: Span) -> Result<PathBuf, RuntimeError> {
    let mut output = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => output.push(part),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(
                    RuntimeError::new("archive-path", "invalid archive path").with_span(span)
                );
            }
        }
    }
    if output.as_os_str().is_empty() {
        return Err(RuntimeError::new("archive-path", "empty archive path").with_span(span));
    }
    Ok(output)
}

pub(super) fn archive_path_in(
    dest: &Path,
    archive_path: &Path,
    span: Span,
) -> Result<PathBuf, RuntimeError> {
    Ok(dest.join(clean_archive_path(archive_path, span)?))
}

pub(super) fn archive_member_path(path: &PathValue, span: Span) -> Result<PathBuf, RuntimeError> {
    let path = PathBuf::from(OsString::from_vec(path.bytes.clone()));
    let mut output = PathBuf::new();
    let mut saw_component = false;
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => {
                output.push(part);
                saw_component = true;
            }
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(
                    RuntimeError::new("archive-path", "invalid archive path").with_span(span)
                );
            }
        }
    }
    if saw_component {
        Ok(output)
    } else {
        Ok(PathBuf::new())
    }
}

pub(super) fn validate_link_target(
    link_path: &Path,
    target: &Path,
    span: Span,
) -> Result<(), RuntimeError> {
    if target.is_absolute() {
        return Err(RuntimeError::new("archive-escape", "absolute symlink target").with_span(span));
    }
    let mut parts = Vec::new();
    if let Some(parent) = link_path.parent() {
        for component in parent.components() {
            match component {
                Component::CurDir => {}
                Component::Normal(part) => parts.push(part.to_os_string()),
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(RuntimeError::new("archive-escape", "symlink target escape")
                        .with_span(span));
                }
            }
        }
    }
    for component in target.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => parts.push(part.to_os_string()),
            Component::ParentDir => {
                if parts.pop().is_none() {
                    return Err(RuntimeError::new("archive-escape", "symlink target escape")
                        .with_span(span));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(
                    RuntimeError::new("archive-escape", "symlink target escape").with_span(span)
                );
            }
        }
    }
    Ok(())
}
