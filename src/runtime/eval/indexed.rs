#![allow(dead_code)]

use crate::source::Span;
use std::num::NonZeroU32;

pub(super) mod full;
mod semantic;

const IR_NONE: u32 = u32::MAX;

macro_rules! ir_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        #[repr(transparent)]
        pub struct $name(NonZeroU32);

        impl $name {
            fn new(index: usize) -> Result<Self, IrBuildError> {
                let raw = index
                    .checked_add(1)
                    .and_then(|raw| u32::try_from(raw).ok())
                    .ok_or_else(|| IrBuildError::format("id_overflow", None, 0, 0))?;
                if raw == IR_NONE {
                    return Err(IrBuildError::format("id_overflow", None, 0, 0));
                }
                Ok(Self(NonZeroU32::new(raw).expect("IR ids are one-based")))
            }

            fn index(self) -> usize {
                self.0.get() as usize - 1
            }

            fn raw(self) -> u32 {
                self.0.get()
            }

            fn from_raw(raw: u32) -> Option<Self> {
                NonZeroU32::new(raw)
                    .filter(|raw| raw.get() != IR_NONE)
                    .map(Self)
            }
        }
    };
}

ir_id!(IrBlockId);
ir_id!(IrFunctionId);
ir_id!(IrStringId);
ir_id!(IrBytesId);
ir_id!(IrLocationId);
ir_id!(TypeId);
ir_id!(SignatureId);
ir_id!(ShapeId);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct IrData {
    pub lhs: u32,
    pub rhs: u32,
}

impl IrData {
    const ZERO: Self = Self { lhs: 0, rhs: 0 };

    fn new(lhs: u32, rhs: u32) -> Self {
        Self { lhs, rhs }
    }

    fn from_range(range: IrRange) -> Self {
        Self::new(range.start, range.len)
    }

    fn range(self) -> IrRange {
        IrRange::new(self.lhs, self.rhs)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct IrRange {
    pub start: u32,
    pub len: u32,
}

impl IrRange {
    const EMPTY: Self = Self { start: 0, len: 0 };

    const fn new(start: u32, len: u32) -> Self {
        Self { start, len }
    }

    fn bounds(self, total: usize) -> Option<std::ops::Range<usize>> {
        let start = self.start as usize;
        let end = start.checked_add(self.len as usize)?;
        (end <= total).then_some(start..end)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct IrLocation {
    pub start: u32,
    pub len: u32,
}

impl IrLocation {
    fn from_span(span: Span) -> Result<Self, IrBuildError> {
        Ok(Self {
            start: u32::try_from(span.start())
                .map_err(|_| IrBuildError::format("location_overflow", Some(span), 0, 0))?,
            len: u32::try_from(span.end().saturating_sub(span.start()))
                .map_err(|_| IrBuildError::format("location_overflow", Some(span), 0, 0))?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrBuildError {
    pub construct: &'static str,
    /// Keep the source ID: modules in one build have independent byte offsets.
    pub span: Option<Span>,
    pub attempted_instructions: usize,
    pub committed_instructions: usize,
    /// Why lowering gave up, in source terms, when the lowerer recorded it.
    pub detail: Option<String>,
}

impl IrBuildError {
    fn format(
        construct: &'static str,
        span: Option<Span>,
        attempted_instructions: usize,
        committed_instructions: usize,
    ) -> Self {
        Self {
            construct,
            span,
            attempted_instructions,
            committed_instructions,
            detail: None,
        }
    }

    fn with_detail(mut self, detail: Option<String>) -> Self {
        self.detail = detail;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrVerifyError {
    pub message: String,
}

impl IrVerifyError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    // Corpus selection owns disk paths, including directory links that can
    // cycle and file links whose source must remain readable through the link.
    #[test]
    fn corpus_walk_skips_directory_links_and_only_excludes_compat_work() {
        let fixture = tempfile::tempdir().expect("create corpus fixture");
        let root = fixture.path();
        let source_paths = [
            "core/nested/keep.xsh",
            "core/dev/compat/keep.xsh",
            "dev/build.xsh",
            "dev/compat/unfinished.xsh",
            "dev/compat-extra/keep.xsh",
        ];
        for relative in source_paths {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).expect("create source directory");
            std::fs::write(path, "print fixture\n").expect("write source");
        }
        std::fs::write(root.join("core/nested/notes.txt"), "notes").expect("write non-source");
        std::os::unix::fs::symlink(".", root.join("core/nested/cycle"))
            .expect("create directory cycle");
        let directory_link = root.join("linked-directory");
        std::os::unix::fs::symlink("core", &directory_link).expect("create directory link");
        let file_link = root.join("linked.xsh");
        std::os::unix::fs::symlink("core/nested/keep.xsh", &file_link).expect("create file link");
        let mut paths = Vec::new();
        collect_xsh_paths(root, root, &mut paths).expect("collect fixture sources");
        paths.sort();
        let mut expected: Vec<_> = source_paths
            .iter()
            .filter(|path| **path != "dev/compat/unfinished.xsh")
            .map(|path| root.join(path))
            .collect();
        expected.push(file_link.clone());
        expected.sort();
        assert_eq!(paths, expected);
        paths.clear();
        collect_xsh_paths(root, &directory_link, &mut paths).expect("ignore directory link");
        assert!(paths.is_empty(), "a directory link is not a corpus root");
        assert_eq!(read_xsh_source(&file_link).unwrap(), "print fixture\n");
    }

    #[test]
    fn corpus_source_read_errors_name_the_path() {
        let fixture = tempfile::tempdir().expect("create source fixture");
        let missing = fixture.path().join("missing.xsh");
        let error = read_xsh_source(&missing).expect_err("the source does not exist");
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert!(error.to_string().contains(missing.to_str().unwrap()), "{error}");
    }

    #[test]
    fn corpus_walk_reports_missing_paths() {
        let fixture = tempfile::tempdir().expect("create corpus fixture");
        let missing = fixture.path().join("missing");
        let error = collect_xsh_paths(fixture.path(), &missing, &mut Vec::new())
            .expect_err("the corpus directory does not exist");
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert!(error.to_string().contains(missing.to_str().unwrap()), "{error}");
    }

    // Directory links are fixtures rather than source trees. File links keep
    // their existing source spelling; unfinished compatibility work is omitted
    // only at the repository's exact subtree.
    pub(in crate::runtime::eval) fn collect_xsh_paths(
        root: &std::path::Path,
        path: &std::path::Path,
        paths: &mut Vec<std::path::PathBuf>,
    ) -> std::io::Result<()> {
        if path.strip_prefix(root).is_ok_and(|relative| relative == std::path::Path::new("dev/compat")) {
            return Ok(());
        }
        let kind = std::fs::symlink_metadata(path)
            .map_err(|error| std::io::Error::new(error.kind(), format!("{}: {error}", path.display())))?
            .file_type();
        if !kind.is_dir() {
            if path.extension().is_some_and(|extension| extension == "xsh")
                && (kind.is_file() || (kind.is_symlink() && path.is_file()))
            {
                paths.push(path.to_path_buf());
            }
            return Ok(());
        }
        let entries = std::fs::read_dir(path)
            .map_err(|error| std::io::Error::new(error.kind(), format!("{}: {error}", path.display())))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| std::io::Error::new(error.kind(), format!("{}: {error}", path.display())))?;
            collect_xsh_paths(root, &entry.path(), paths)?;
        }
        Ok(())
    }

    pub(in crate::runtime::eval) fn read_xsh_source(path: &std::path::Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
            .map_err(|error| std::io::Error::new(error.kind(), format!("{}: {error}", path.display())))
    }
}
