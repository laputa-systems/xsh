use super::schema::{FullTag, InPlaceClass};
use std::ops::Range;

pub(super) const HEIGHT_LIMIT: u16 = 128;
const UNMARKED: u16 = 0;
const TOO_DEEP: u16 = HEIGHT_LIMIT + 1;

/// Proof accumulated by the generated instruction verifier as it walks the
/// actual operand IDs in an owner's instruction graph.
pub(super) struct InstructionProof {
    range: Range<usize>,
    marks: Vec<bool>,
    open: Vec<(usize, u16)>,
}

impl InstructionProof {
    pub(super) fn new(range: Range<usize>) -> Self {
        let len = range.len();
        Self {
            range,
            marks: vec![false; len],
            open: Vec::new(),
        }
    }

    pub(super) fn enter(&mut self, instruction: usize, tag: FullTag) {
        debug_assert!(self.range.contains(&instruction));
        let height = match tag.in_place_class() {
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
}
