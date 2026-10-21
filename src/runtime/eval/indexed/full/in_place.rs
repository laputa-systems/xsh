use super::schema::{InPlaceClass, InstructionKind};
use std::ops::Range;

pub(super) const HEIGHT_LIMIT: u16 = 128;
const UNMARKED: u16 = 0;
const TOO_DEEP: u16 = HEIGHT_LIMIT + 1;

pub(super) struct InstructionProof {
    range: Range<usize>,
    marks: Vec<bool>,
    parents: Vec<u32>,
    statements: Vec<u32>,
    open: Vec<(usize, u16)>,
}

impl InstructionProof {
    pub(super) fn new(range: Range<usize>) -> Self {
        let len = range.len();
        Self {
            range,
            marks: vec![false; len],
            parents: vec![u32::MAX; len],
            statements: vec![u32::MAX; len],
            open: Vec::new(),
        }
    }

    pub(super) fn enter(&mut self, instruction: usize, kind: InstructionKind, class: InPlaceClass) {
        debug_assert!(self.range.contains(&instruction));
        if let Some((parent, _)) = self.open.last() {
            let parent = u32::try_from(*parent)
                .expect("instruction owner range fits a u32 parent index");
            self.parents[instruction - self.range.start] = parent;
        }
        self.statements[instruction - self.range.start] = if kind == InstructionKind::Stmt {
            (instruction - self.range.start) as u32
        } else {
            self.open.last().map_or(u32::MAX, |(parent, _)| self.statements[*parent])
        };
        let height = match class {
            InPlaceClass::Operands => 1,
            InPlaceClass::Scheduled => UNMARKED,
        };
        self.open.push((instruction - self.range.start, height));
    }

    pub(super) fn finish(&mut self, instruction: usize) {
        let local = instruction - self.range.start;
        let (finished, height) = self.open.pop().expect("instruction proof was entered");
        debug_assert_eq!(finished, local, "generated verification is properly nested");
        self.marks[local] = (1..=HEIGHT_LIMIT).contains(&height);

        if let Some((_, parent)) = self.open.last_mut() {
            if *parent == UNMARKED || height == UNMARKED {
                *parent = UNMARKED;
            } else {
                *parent = (*parent).max(height.saturating_add(1).min(TOO_DEEP));
            }
        }
    }

    pub(super) fn complete(&self) -> bool {
        self.open.is_empty()
    }

    pub(super) fn copy_marks_into(&self, output: &mut [bool]) -> bool {
        let Some(target) = output.get_mut(self.range.clone()) else {
            return false;
        };
        if !self.complete() || target.len() != self.marks.len() {
            return false;
        }
        target.copy_from_slice(&self.marks);
        true
    }

    pub(super) fn parent_of(&self, instruction: usize) -> Option<usize> {
        if !self.range.contains(&instruction) {
            return None;
        }
        let parent = self.parents[instruction - self.range.start];
        (parent != u32::MAX).then(|| self.range.start + parent as usize)
    }

    pub(super) fn statement_of(&self, instruction: usize) -> Option<usize> {
        if !self.range.contains(&instruction) { return None; }
        let statement = self.statements[instruction - self.range.start];
        (statement != u32::MAX).then(|| self.range.start + statement as usize)
    }
}
