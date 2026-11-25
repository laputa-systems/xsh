use super::{ArenaProgram, ExprId, FxHashMap, Span};

#[derive(Clone, Debug, Default)]
pub(super) struct FactSlots {
    spans: Vec<Span>,
    by_span: FxHashMap<Span, u32>,
    by_expr: Vec<u32>,
}

impl FactSlots {
    pub(super) fn new(program: &ArenaProgram) -> Self {
        let mut result = Self::default();
        result.by_expr.reserve(program.arena.expr_tags.len());
        for index in 0..program.arena.expr_tags.len() {
            let span = program.arena.expr_span(ExprId::from_index(index));
            let slot = *result.by_span.entry(span).or_insert_with(|| {
                let slot = u32::try_from(result.spans.len()).expect("expression fact slot overflow");
                result.spans.push(span);
                slot
            });
            result.by_expr.push(slot);
        }
        result
    }

    pub(super) fn for_expr(&self, id: ExprId) -> usize {
        self.by_expr[id.index()] as usize
    }

    pub(super) fn for_span(&self, span: &Span) -> Option<usize> {
        self.by_span.get(span).map(|&slot| slot as usize)
    }

}

#[derive(Clone, Debug, Default)]
pub(super) struct ExpressionTypes {
    slots: std::sync::Arc<FactSlots>,
    values: Vec<Option<super::Type>>,
    first_slot: usize,
    touched: Vec<u64>,
    active: Vec<u32>,
    other: std::collections::BTreeMap<Span, super::Type>,
}

impl ExpressionTypes {
    pub(super) fn attach(&mut self, slots: std::sync::Arc<FactSlots>) {
        let existing = std::mem::take(self).into_map();
        self.slots = slots;
        for (span, ty) in existing { self.insert(span, ty); }
    }

    pub(super) fn get(&self, span: &Span) -> Option<&super::Type> {
        match self.slots.for_span(span) {
            Some(slot) => self.value(slot),
            None => self.other.get(span),
        }
    }

    pub(super) fn get_expr(&self, program: &ArenaProgram, id: ExprId) -> Option<&super::Type> {
        if self.slots.by_expr.is_empty() { return self.get(&program.arena.expr_span(id)); }
        self.value(self.slots.for_expr(id))
    }

    fn value(&self, slot: usize) -> Option<&super::Type> {
        slot.checked_sub(self.first_slot).and_then(|index| self.values.get(index)).and_then(Option::as_ref)
    }

    fn insert_slot(&mut self, slot: usize, ty: super::Type) -> Option<super::Type> {
        if self.values.is_empty() {
            self.first_slot = slot;
        } else if slot < self.first_slot {
            let growth = (self.first_slot - slot).max(self.values.len()).min(self.first_slot);
            let mut values = vec![None; growth];
            values.append(&mut self.values);
            self.values = values;
            self.first_slot -= growth;
        }
        let index = slot - self.first_slot;
        if index >= self.values.len() { self.values.resize(index + 1, None); }
        let word = slot / 64;
        if word >= self.touched.len() { self.touched.resize(word + 1, 0); }
        let bit = 1u64 << (slot % 64);
        if self.touched[word] & bit == 0 {
            self.touched[word] |= bit;
            self.active.push(u32::try_from(slot).expect("expression fact slot overflow"));
        }
        self.values[index].replace(ty)
    }

    pub(super) fn insert_expr(&mut self, program: &ArenaProgram, id: ExprId, ty: super::Type) -> Option<super::Type> {
        if self.slots.by_expr.is_empty() { return self.insert(program.arena.expr_span(id), ty); }
        self.insert_slot(self.slots.for_expr(id), ty)
    }

    pub(super) fn insert(&mut self, span: Span, ty: super::Type) -> Option<super::Type> {
        match self.slots.for_span(&span) {
            Some(slot) => self.insert_slot(slot, ty),
            None => self.other.insert(span, ty),
        }
    }

    pub(super) fn remove(&mut self, span: &Span) -> Option<super::Type> {
        match self.slots.for_span(span) {
            Some(slot) => slot.checked_sub(self.first_slot).and_then(|index| self.values.get_mut(index)).and_then(Option::take),
            None => self.other.remove(span),
        }
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&Span, &super::Type)> {
        let mut pairs: Vec<_> = self.active.iter().filter_map(|&slot| {
            let index = slot as usize;
            self.value(index).map(|value| (&self.slots.spans[index], value))
        }).chain(self.other.iter()).collect();
        pairs.sort_unstable_by_key(|(span, _)| **span);
        pairs.into_iter()
    }

    pub(super) fn for_each_sorted_mut(&mut self, mut visit: impl FnMut(&Span, &mut super::Type)) {
        let mut order: Vec<_> = self.active.iter().filter_map(|&slot| {
            let index = slot as usize;
            self.value(index).map(|_| (self.slots.spans[index], Some(index)))
        }).chain(self.other.keys().map(|&span| (span, None))).collect();
        order.sort_unstable_by_key(|(span, _)| *span);
        for (span, slot) in order {
            let value = match slot {
                Some(slot) => self.values[slot - self.first_slot].as_mut(),
                None => self.other.get_mut(&span),
            }.expect("sorted expression fact remains present");
            visit(&span, value);
        }
    }

    pub(super) fn retain(&mut self, mut keep: impl FnMut(&Span, &mut super::Type) -> bool) {
        for &slot in &self.active {
            let index = slot as usize;
            let value = &mut self.values[index - self.first_slot];
            if value.as_mut().is_some_and(|ty| !keep(&self.slots.spans[index], ty)) { *value = None; }
        }
        self.other.retain(keep);
    }

    pub(super) fn into_map(mut self) -> std::collections::BTreeMap<Span, super::Type> {
        let mut pairs = Vec::with_capacity(self.active.len() + self.other.len());
        for slot in self.active {
            let index = slot as usize;
            if let Some(value) = self.values[index - self.first_slot].take() { pairs.push((self.slots.spans[index], value)); }
        }
        pairs.extend(self.other);
        pairs.sort_unstable_by_key(|(span, _)| *span);
        pairs.into_iter().collect()
    }

}

#[derive(Clone, Debug, Default)]
pub(super) struct ExpressionSpans {
    slots: std::sync::Arc<FactSlots>,
    bits: Vec<u64>,
    other: std::collections::BTreeSet<Span>,
}

impl ExpressionSpans {
    pub(super) fn attach(&mut self, slots: std::sync::Arc<FactSlots>) {
        let existing = std::mem::take(self).into_set();
        self.slots = slots;
        for span in existing { self.insert(span); }
    }

    pub(super) fn insert(&mut self, span: Span) -> bool {
        match self.slots.for_span(&span) {
            Some(slot) => {
                let word = slot / 64;
                if word >= self.bits.len() { self.bits.resize(word + 1, 0); }
                let bit = 1u64 << (slot % 64);
                let inserted = self.bits[word] & bit == 0;
                self.bits[word] |= bit;
                inserted
            }
            None => self.other.insert(span),
        }
    }

    fn contains_slot(&self, slot: usize) -> bool {
        self.bits.get(slot / 64).is_some_and(|word| word & (1u64 << (slot % 64)) != 0)
    }

    pub(super) fn contains(&self, span: &Span) -> bool {
        if self.bits.is_empty() { return self.other.contains(span); }
        self.slots.for_span(span).map_or_else(|| self.other.contains(span), |slot| self.contains_slot(slot))
    }

    pub(super) fn contains_expr(&self, id: ExprId, span: &Span) -> bool {
        if self.slots.by_expr.is_empty() { return self.other.contains(span); }
        self.contains_slot(self.slots.for_expr(id))
    }

    pub(super) fn into_set(self) -> std::collections::BTreeSet<Span> {
        let mut spans = Vec::new();
        for (word, &bits) in self.bits.iter().enumerate() {
            let mut remaining = bits;
            while remaining != 0 {
                let bit = remaining.trailing_zeros() as usize;
                spans.push(self.slots.spans[word * 64 + bit]);
                remaining &= remaining - 1;
            }
        }
        spans.extend(self.other);
        spans.sort_unstable();
        spans.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;
    use crate::syntax::arena::ArenaExprKind;
    use crate::syntax::parser::Parser;

    #[test]
    fn fact_storage_preserves_out_of_order_slots() {
        let spans = [Span::new(SourceId::new(0), 1, 2), Span::new(SourceId::new(0), 3, 4), Span::new(SourceId::new(0), 5, 6)];
        let slots = FactSlots {
            spans: spans.to_vec(),
            by_span: spans.iter().copied().enumerate().map(|(index, span)| (span, index as u32)).collect(),
            by_expr: Vec::new(),
        };
        let mut facts = ExpressionTypes::default();
        facts.attach(std::sync::Arc::new(slots));
        facts.insert(spans[2], super::super::Type::Int);
        assert_eq!(facts.values.len(), 1);
        facts.insert(spans[0], super::super::Type::Bool);
        assert_eq!(facts.get(&spans[2]), Some(&super::super::Type::Int));
        assert_eq!(facts.iter().map(|(span, _)| *span).collect::<Vec<_>>(), [spans[0], spans[2]]);
        facts.remove(&spans[0]);
        facts.insert(spans[0], super::super::Type::Bool);
        assert_eq!(facts.into_map().len(), 2);
    }

    #[test]
    fn pattern_fact_slots_share_written_types() {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), "pure value(x: Int?) -> Bool { x is Int }\n");
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let program = parsed.arena;
        let condition = (0..program.arena.expr_tags.len()).map(ExprId::from_index)
            .find(|&id| matches!(program.arena.expr_kind(id), ArenaExprKind::PatternTest { .. })).unwrap();
        let ArenaExprKind::PatternTest { arms, .. } = program.arena.expr_kind(condition) else { unreachable!() };
        let arms = program.arena.match_expr_arms(arms);
        let slots = std::sync::Arc::new(FactSlots::new(&program));
        let mut facts = ExpressionTypes::default();
        facts.attach(std::sync::Arc::clone(&slots));
        facts.insert(program.arena.expr_span(condition), super::super::Type::Bool);
        let mut propagating = ExpressionSpans::default();
        propagating.attach(slots);
        assert!(propagating.insert(program.arena.expr_span(condition)));
        for arm in arms {
            assert_eq!(facts.get_expr(&program, arm.value), Some(&super::super::Type::Bool));
            assert!(propagating.contains_expr(arm.value, &program.arena.expr_span(arm.value)));
        }
        assert_eq!(propagating.into_set().len(), 1);
        facts.remove(&program.arena.expr_span(condition));
        facts.insert_expr(&program, condition, super::super::Type::Bool);
        assert_eq!(facts.into_map().len(), 1);
    }
}

#[cfg(test)]
impl std::ops::Index<&Span> for ExpressionTypes {
    type Output = super::Type;
    fn index(&self, span: &Span) -> &Self::Output { self.get(span).expect("expression type not found") }
}

#[cfg(test)]
impl ExpressionTypes {
    pub(super) fn len(&self) -> usize {
        self.active.iter().filter(|&&slot| self.value(slot as usize).is_some()).count() + self.other.len()
    }
}

#[cfg(test)]
impl From<std::collections::BTreeMap<Span, super::Type>> for ExpressionTypes {
    fn from(other: std::collections::BTreeMap<Span, super::Type>) -> Self {
        Self { other, ..Self::default() }
    }
}
