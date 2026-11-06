#![allow(clippy::single_call_fn)]

use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use diffy::apply_bytes;
use diffy::patch_set::{FileMode, FileOperation, ParseOptions, PatchKind, PatchSet};
use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use xsh_root::{FileKind, OpenOptions, RenameMode, Root};

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
    let root = open_root(&root, span)?;
    apply_in(&root, text, strip_components, overwrite, span)
}

fn apply_in(
    root: &Root,
    text: &str,
    strip_components: i64,
    overwrite: bool,
    span: Span,
) -> Result<Value, RuntimeError> {
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
                let target = output_path(root, path, strip_components as usize, span)?;
                reject_existing_output(root, &target, overwrite, span)?;
                let patched = apply_patch_bytes(&[], file_patch.patch(), span)?;
                write_output(
                    root,
                    &target,
                    &patched,
                    mode_permissions(new_mode.or(Some(FileMode::Regular)), None, span)?,
                    overwrite,
                    span,
                )?;
            }
            FileOperation::Delete(path) => {
                let target = existing_path(root, path, strip_components as usize, span)?;
                let base = read_regular(root, &target, span)?;
                let patched = apply_patch_bytes(&base, file_patch.patch(), span)?;
                if !patched.is_empty() {
                    return Err(RuntimeError::new(
                        "patch-apply",
                        "delete patch leaves file content",
                    )
                    .with_span(span));
                }
                root.remove_file(&target)
                    .map_err(|error| patch_error("patch-remove", error, span))?;
            }
            FileOperation::Modify { original, modified } => {
                let source = existing_path(root, original, strip_components as usize, span)?;
                let target = output_path(root, modified, strip_components as usize, span)?;
                if source != target {
                    reject_existing_output(root, &target, overwrite, span)?;
                }
                let previous_mode = regular_file_mode(root, &source, span)?;
                let base = read_regular(root, &source, span)?;
                let patched = apply_patch_bytes(&base, file_patch.patch(), span)?;
                write_output(
                    root,
                    &target,
                    &patched,
                    mode_permissions(new_mode, Some(previous_mode), span)?,
                    source == target || overwrite,
                    span,
                )?;
                if source != target {
                    root.remove_file(&source)
                        .map_err(|error| patch_error("patch-remove", error, span))?;
                }
            }
            FileOperation::Rename { from, to } => {
                apply_copy_or_rename(
                    root,
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
                    root,
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
    root: &Root,
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
    reject_existing_output(root, &target, overwrite, span)?;
    let previous_mode = regular_file_mode(root, &source, span)?;
    let base = read_regular(root, &source, span)?;
    let patched = apply_patch_bytes(&base, patch, span)?;
    write_output(
        root,
        &target,
        &patched,
        mode_permissions(new_mode, Some(previous_mode), span)?,
        overwrite,
        span,
    )?;
    if remove_source {
        root.remove_file(&source).map_err(|error| patch_error("patch-remove", error, span))?;
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

fn open_root(root: &Path, span: Span) -> Result<Root, RuntimeError> {
    let metadata =
        fs::symlink_metadata(root).map_err(|error| patch_error("patch-root", error, span))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(RuntimeError::new("patch-root", "root must be a directory").with_span(span));
    }
    let opened = Root::open(root).map_err(|error| patch_error("patch-root", error, span))?;
    let anchor = opened
        .metadata(".")
        .map_err(|error| patch_error("patch-root", error, span))?;
    if anchor.inode != metadata.ino() || anchor.device != metadata.dev() {
        return Err(RuntimeError::new("patch-root", "root changed while opening").with_span(span));
    }
    Ok(opened)
}

fn existing_path(
    root: &Root,
    raw: &[u8],
    strip_components: usize,
    span: Span,
) -> Result<PathBuf, RuntimeError> {
    let relative = clean_patch_path(raw, strip_components, span)?;
    validate_ancestors(root, &relative, false, span)?;
    regular_file_mode(root, &relative, span)?;
    Ok(relative)
}

fn output_path(
    root: &Root,
    raw: &[u8],
    strip_components: usize,
    span: Span,
) -> Result<PathBuf, RuntimeError> {
    let relative = clean_patch_path(raw, strip_components, span)?;
    validate_ancestors(root, &relative, true, span)?;
    Ok(relative)
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
    root: &Root,
    relative: &Path,
    create_missing: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    let mut current = PathBuf::new();
    let mut components = relative.components().peekable();
    while let Some(component) = components.next() {
        let Component::Normal(part) = component else {
            return Err(RuntimeError::new("patch-path", "invalid patch path").with_span(span));
        };
        current.push(part);
        if components.peek().is_none() {
            break;
        }
        match root.metadata(&current) {
            Ok(metadata) if metadata.kind == FileKind::Symlink => {
                return Err(RuntimeError::new("patch-escape", "symlink ancestor").with_span(span));
            }
            Ok(metadata) if metadata.kind == FileKind::Directory => {}
            Ok(_) => {
                return Err(
                    RuntimeError::new("patch-path", "ancestor is not a directory").with_span(span),
                );
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound && create_missing => {
                root.mkdir(&current, 0o777)
                    .map_err(|error| ancestor_error("patch-create-dir", error, span))?;
            }
            Err(error) => return Err(ancestor_error("patch-path", error, span)),
        }
    }
    Ok(())
}

fn ancestor_error(kind: &str, error: io::Error, span: Span) -> RuntimeError {
    if error.raw_os_error() == Some(libc::ELOOP) {
        return RuntimeError::new("patch-escape", "symlink ancestor").with_span(span);
    }
    patch_error(kind, error, span)
}

fn regular_file_mode(root: &Root, path: &Path, span: Span) -> Result<u32, RuntimeError> {
    let metadata = root
        .metadata(path)
        .map_err(|error| ancestor_error("patch-path", error, span))?;
    match metadata.kind {
        FileKind::Symlink => {
            Err(RuntimeError::new("patch-escape", "symlink target").with_span(span))
        }
        FileKind::File => Ok(metadata.mode),
        _ => Err(RuntimeError::new("patch-path", "target is not a file").with_span(span)),
    }
}

fn read_regular(root: &Root, path: &Path, span: Span) -> Result<Vec<u8>, RuntimeError> {
    let mut file = root
        .open_with(
            path,
            OpenOptions::new().read(true).no_symlinks(true).non_blocking(true),
        )
        .map_err(|error| ancestor_error("patch-read", error, span))?;
    let opened = file
        .metadata()
        .map_err(|error| patch_error("patch-read", error, span))?;
    if !opened.is_file() {
        return Err(RuntimeError::new("patch-path", "target is not a file").with_span(span));
    }
    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .map_err(|error| patch_error("patch-read", error, span))?;
    Ok(data)
}

fn reject_existing_output(
    root: &Root,
    path: &Path,
    overwrite: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    match root.metadata(path) {
        Ok(metadata) if metadata.kind == FileKind::Symlink => {
            Err(RuntimeError::new("patch-escape", "symlink target").with_span(span))
        }
        Ok(metadata) if metadata.kind == FileKind::Directory => {
            Err(RuntimeError::new("patch-path", "target is a directory").with_span(span))
        }
        Ok(_) if !overwrite => {
            Err(RuntimeError::new("patch-path", "target exists").with_span(span))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ancestor_error("patch-path", error, span)),
    }
}

static STAGE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn write_output(
    root: &Root,
    path: &Path,
    data: &[u8],
    permissions: Option<u32>,
    replace: bool,
    span: Span,
) -> Result<(), RuntimeError> {
    let staged = path.with_file_name(format!(
        ".xsh-patch-{}-{}",
        std::process::id(),
        STAGE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = root
        .open_with(
            &staged,
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .no_symlinks(true)
                .mode(0o600),
        )
        .map_err(|error| ancestor_error("patch-write", error, span))?;
    let published = file
        .write_all(data)
        .and_then(|()| match permissions {
            Some(mode) => file.set_permissions(fs::Permissions::from_mode(mode)),
            None => Ok(()),
        })
        .and_then(|()| {
            root.rename(
                &staged,
                path,
                if replace {
                    RenameMode::Replace
                } else {
                    RenameMode::NoReplace
                },
            )
        });
    match published {
        Ok(()) => Ok(()),
        Err(error) => {
            let cleanup = root.remove_file(&staged);
            let mut failure = if error.kind() == io::ErrorKind::AlreadyExists {
                RuntimeError::new("patch-path", "target exists").with_span(span)
            } else {
                patch_error("patch-write", error, span)
            };
            if let Err(cleanup) = cleanup {
                failure = RuntimeError::new(
                    "patch-write",
                    format!("{}; removing staged file failed: {cleanup}", failure.message),
                )
                .with_span(span);
            }
            Err(failure)
        }
    }
}

fn mode_permissions(
    mode: Option<FileMode>,
    previous: Option<u32>,
    span: Span,
) -> Result<Option<u32>, RuntimeError> {
    match mode {
        Some(FileMode::Regular) => Ok(Some(0o644)),
        Some(FileMode::Executable) => Ok(Some(0o755)),
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
    RuntimeError::host(kind, &error).with_span(span)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;
    use std::os::unix::fs::symlink;

    fn span() -> Span {
        Span::new(SourceId::new(0), 0, 0)
    }

    struct Fixture {
        _base: tempfile::TempDir,
        tree: PathBuf,
        outside: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let base = tempfile::tempdir().unwrap();
            let tree = base.path().join("tree");
            let outside = base.path().join("outside");
            fs::create_dir(&tree).unwrap();
            fs::create_dir(&outside).unwrap();
            fs::write(outside.join("sentinel"), "outside\n").unwrap();
            Self {
                _base: base,
                tree,
                outside,
            }
        }

        fn apply(&self, text: &str, overwrite: bool) -> Result<Value, RuntimeError> {
            apply(self.tree.clone(), text, 0, overwrite, span())
        }

        fn assert_outside_unchanged(&self) {
            let mut names: Vec<_> = fs::read_dir(&self.outside)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            names.sort();
            assert_eq!(names, vec![OsString::from("sentinel")]);
            assert_eq!(fs::read_to_string(self.outside.join("sentinel")).unwrap(), "outside\n");
        }
    }

    fn edit(path: &str) -> String {
        format!("--- {path}\n+++ {path}\n@@ -1 +1 @@\n-outside\n+changed\n")
    }

    fn create(path: &str) -> String {
        format!("--- /dev/null\n+++ {path}\n@@ -0,0 +1 @@\n+created\n")
    }

    fn kind(result: Result<Value, RuntimeError>) -> String {
        result.expect_err("patch must fail").kind
    }

    #[test]
    fn an_opened_root_survives_replacement_of_its_path_by_a_symlink() {
        let fixture = Fixture::new();
        fs::write(fixture.tree.join("file"), "outside\n").unwrap();
        let root = open_root(&fixture.tree, span()).unwrap();
        let moved = fixture.tree.with_file_name("moved");
        fs::rename(&fixture.tree, &moved).unwrap();
        symlink(&fixture.outside, &fixture.tree).unwrap();
        fs::write(fixture.outside.join("file"), "outside\n").unwrap();

        apply_in(&root, &edit("file"), 0, false, span()).unwrap();

        assert_eq!(fs::read_to_string(moved.join("file")).unwrap(), "changed\n");
        assert_eq!(fs::read_to_string(fixture.outside.join("file")).unwrap(), "outside\n");
    }

    #[test]
    fn a_symlinked_ancestor_is_refused_and_nothing_outside_changes() {
        let fixture = Fixture::new();
        symlink(&fixture.outside, fixture.tree.join("link")).unwrap();
        assert_eq!(kind(fixture.apply(&edit("link/sentinel"), true)), "patch-escape");
        assert_eq!(kind(fixture.apply(&create("link/new"), true)), "patch-escape");
        fixture.assert_outside_unchanged();
    }

    #[test]
    fn a_symlinked_leaf_is_refused_for_every_operation() {
        let fixture = Fixture::new();
        symlink(fixture.outside.join("sentinel"), fixture.tree.join("leaf")).unwrap();
        assert_eq!(kind(fixture.apply(&edit("leaf"), true)), "patch-escape");
        assert_eq!(kind(fixture.apply(&create("leaf"), true)), "patch-escape");
        let delete = "--- leaf\n+++ /dev/null\n@@ -1 +0,0 @@\n-outside\n";
        assert_eq!(kind(fixture.apply(delete, true)), "patch-escape");
        fixture.assert_outside_unchanged();
    }

    #[test]
    fn a_root_that_is_a_symlink_is_refused() {
        let fixture = Fixture::new();
        let link = fixture.tree.with_file_name("link");
        symlink(&fixture.outside, &link).unwrap();
        let result = apply(link, &create("new"), 0, false, span());
        assert_eq!(kind(result), "patch-root");
        fixture.assert_outside_unchanged();
    }

    #[test]
    fn a_fifo_target_is_refused_without_blocking() {
        let fixture = Fixture::new();
        rustix::fs::mknodat(
            rustix::fs::CWD,
            fixture.tree.join("pipe"),
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::from_raw_mode(0o600),
            0,
        )
        .unwrap();
        assert_eq!(kind(fixture.apply(&edit("pipe"), true)), "patch-path");
    }

    #[test]
    fn no_clobber_leaves_an_existing_file_and_stages_nothing() {
        let fixture = Fixture::new();
        fs::write(fixture.tree.join("kept"), "prior\n").unwrap();
        assert_eq!(kind(fixture.apply(&create("kept"), false)), "patch-path");
        assert_eq!(fs::read_to_string(fixture.tree.join("kept")).unwrap(), "prior\n");
        let names: Vec<_> = fs::read_dir(&fixture.tree).unwrap().collect();
        assert_eq!(names.len(), 1);
    }

    #[test]
    fn a_failed_patch_keeps_earlier_outputs_and_later_targets_untouched() {
        let fixture = Fixture::new();
        fs::write(fixture.tree.join("a"), "outside\n").unwrap();
        fs::write(fixture.tree.join("b"), "other\n").unwrap();
        let text = format!("{}{}", edit("a"), edit("b"));
        assert_eq!(kind(fixture.apply(&text, true)), "patch-apply");
        assert_eq!(fs::read_to_string(fixture.tree.join("a")).unwrap(), "changed\n");
        assert_eq!(fs::read_to_string(fixture.tree.join("b")).unwrap(), "other\n");
        assert_eq!(fs::read_dir(&fixture.tree).unwrap().count(), 2);
    }

    #[test]
    fn missing_ancestors_are_created_beneath_the_root_only() {
        let fixture = Fixture::new();
        fixture.apply(&create("a/b/c"), false).unwrap();
        assert_eq!(fs::read_to_string(fixture.tree.join("a/b/c")).unwrap(), "created\n");
        fixture.assert_outside_unchanged();
    }

    #[test]
    fn edits_keep_the_previous_mode() {
        let fixture = Fixture::new();
        let path = fixture.tree.join("script");
        fs::write(&path, "outside\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o750)).unwrap();
        fixture.apply(&edit("script"), true).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o7777, 0o750);
    }
}
