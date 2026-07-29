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
    pub location: Option<IrLocation>,
    pub attempted_instructions: usize,
    pub committed_instructions: usize,
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
            location: span.and_then(|span| IrLocation::from_span(span).ok()),
            attempted_instructions,
            committed_instructions,
        }
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
    pub(super) fn collect_xsh_paths(path: &std::path::Path, paths: &mut Vec<std::path::PathBuf>) {
        if path.is_file() {
            if path.extension().is_some_and(|extension| extension == "xsh") {
                paths.push(path.to_path_buf());
            }
            return;
        }
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            collect_xsh_paths(&entry.path(), paths);
        }
    }

    pub(super) fn read_xsh_source(path: &std::path::Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }
}
