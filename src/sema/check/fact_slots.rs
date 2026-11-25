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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TypeId(u32);

#[derive(Clone, Copy, Debug)]
struct TypeFlags {
    inference: bool,
    recovery: bool,
    any: bool,
    scheme_callable: bool,
    deep: bool,
}

#[derive(Clone, Debug, Default)]
struct TypeTable {
    entries: Vec<(super::Type, TypeFlags)>,
    by_hash: FxHashMap<u64, Vec<TypeId>>,
}

impl TypeTable {
    fn get(&self, id: TypeId) -> &super::Type { &self.entries[id.0 as usize].0 }

    fn flags(&self, id: TypeId) -> TypeFlags { self.entries[id.0 as usize].1 }

    fn intern(&mut self, ty: super::Type) -> TypeId {
        use std::hash::{Hash, Hasher};
        fn inspect(ty: &super::Type, depth: usize, flags: &mut TypeFlags) {
            use super::Type;
            flags.deep |= depth > 100;
            flags.inference |= matches!(ty, Type::Inference(_));
            flags.recovery |= matches!(ty, Type::Unknown | Type::Invalid);
            flags.any |= matches!(ty, Type::Any);
            flags.scheme_callable |= matches!(ty, Type::SchemeCallable(_));
            match ty {
                Type::Callable(callable) => flags.inference |= callable.sig.effects.has_inference(),
                Type::Module(module) => for export in module.values() {
                    if let crate::sema::types::ModuleExportType::Callable { sig, .. } = export {
                        flags.inference |= sig.effects.has_inference();
                    }
                },
                _ => {}
            }
            ty.for_each_child(|child| inspect(child, depth + 1, flags));
        }
        let mut hasher = rustc_hash::FxHasher::default();
        std::mem::discriminant(&ty).hash(&mut hasher);
        match &ty {
            super::Type::Inference(id) => id.hash(&mut hasher),
            super::Type::SchemeCallable(id) => id.hash(&mut hasher),
            super::Type::Function(kind) => kind.hash(&mut hasher),
            super::Type::ErrorFamily(name) | super::Type::ErrorFacet(name) | super::Type::Tag(name) => name.hash(&mut hasher),
            super::Type::ErrorVariant { family, variant } => { family.hash(&mut hasher); variant.hash(&mut hasher); }
            super::Type::Record(fields) => for name in fields.keys() { name.hash(&mut hasher); },
            _ => {}
        }
        ty.for_each_child(|child| std::mem::discriminant(child).hash(&mut hasher));
        let hash = hasher.finish();
        if let Some(id) = self.by_hash.get(&hash).and_then(|ids| ids.iter().copied().find(|&id| self.get(id) == &ty)) { return id; }
        let mut flags = TypeFlags { inference: false, recovery: false, any: false, scheme_callable: false, deep: false };
        inspect(&ty, 0, &mut flags);
        let id = TypeId(u32::try_from(self.entries.len()).expect("type table overflow"));
        self.entries.push((ty, flags));
        self.by_hash.entry(hash).or_default().push(id);
        id
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct ExpressionTypes {
    slots: std::sync::Arc<FactSlots>,
    values: Vec<Option<TypeId>>,
    types: TypeTable,
    first_slot: usize,
    touched: Vec<u64>,
    active: Vec<u32>,
    other: std::collections::BTreeMap<Span, TypeId>,
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
            None => self.other.get(span).map(|&id| self.types.get(id)),
        }
    }

    pub(super) fn get_expr(&self, program: &ArenaProgram, id: ExprId) -> Option<&super::Type> {
        if self.slots.by_expr.is_empty() { return self.get(&program.arena.expr_span(id)); }
        self.value(self.slots.for_expr(id))
    }

    fn value(&self, slot: usize) -> Option<&super::Type> {
        slot.checked_sub(self.first_slot).and_then(|index| self.values.get(index)).and_then(|value| value.map(|id| self.types.get(id)))
    }

    fn insert_slot(&mut self, slot: usize, ty: super::Type) {
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
        let id = self.types.intern(ty);
        self.values[index] = Some(id);
    }

    pub(super) fn insert_expr(&mut self, program: &ArenaProgram, id: ExprId, ty: super::Type) {
        if self.slots.by_expr.is_empty() { self.insert(program.arena.expr_span(id), ty); return; }
        self.insert_slot(self.slots.for_expr(id), ty);
    }

    pub(super) fn insert(&mut self, span: Span, ty: super::Type) {
        match self.slots.for_span(&span) {
            Some(slot) => self.insert_slot(slot, ty),
            None => { let id = self.types.intern(ty); self.other.insert(span, id); },
        }
    }

    pub(super) fn remove(&mut self, span: &Span) {
        match self.slots.for_span(span) {
            Some(slot) => { if let Some(value) = slot.checked_sub(self.first_slot).and_then(|index| self.values.get_mut(index)) { *value = None; } },
            None => { self.other.remove(span); },
        }
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&Span, &super::Type)> {
        let mut pairs: Vec<_> = self.active.iter().filter_map(|&slot| {
            let index = slot as usize;
            self.value(index).map(|value| (&self.slots.spans[index], value))
        }).chain(self.other.iter().map(|(span, &id)| (span, self.types.get(id)))).collect();
        pairs.sort_unstable_by_key(|(span, _)| **span);
        pairs.into_iter()
    }

    pub(super) fn for_each_sorted_inference_mut(&mut self, mut visit: impl FnMut(&Span, &mut super::Type)) {
        let mut order: Vec<_> = self.active.iter().filter_map(|&slot| {
            let index = slot as usize;
            self.value(index).map(|_| (self.slots.spans[index], Some(index)))
        }).chain(self.other.keys().map(|&span| (span, None))).collect();
        order.sort_unstable_by_key(|(span, _)| *span);
        for (span, slot) in order {
            let id = match slot {
                Some(slot) => self.values[slot - self.first_slot],
                None => self.other.get(&span).copied(),
            }.expect("sorted expression fact remains present");
            let flags = self.types.flags(id);
            if !flags.inference && !flags.deep { continue; }
            let mut ty = self.types.get(id).clone();
            visit(&span, &mut ty);
            let replacement = self.types.intern(ty);
            match slot {
                Some(slot) => self.values[slot - self.first_slot] = Some(replacement),
                None => { self.other.insert(span, replacement); }
            }
        }
    }

    pub(super) fn retain(&mut self, mut keep: impl FnMut(&Span, &super::Type) -> bool) {
        for &slot in &self.active {
            let index = slot as usize;
            let value = &mut self.values[index - self.first_slot];
            if value.is_some_and(|id| !keep(&self.slots.spans[index], self.types.get(id))) { *value = None; }
        }
        self.other.retain(|span, id| keep(span, self.types.get(*id)));
    }

    pub(super) fn into_map(self) -> std::collections::BTreeMap<Span, super::Type> {
        let mut pairs = Vec::with_capacity(self.active.len() + self.other.len());
        for slot in self.active {
            let index = slot as usize;
            if let Some(id) = self.values[index - self.first_slot] { pairs.push((self.slots.spans[index], self.types.get(id).clone())); }
        }
        pairs.extend(self.other.into_iter().map(|(span, id)| (span, self.types.get(id).clone())));
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
    fn interned_type_flags_and_distinct_inference_identities() {
        use super::super::Type;
        let mut constraints = crate::sema::constraints::TypeConstraints::default();
        let span = Span::new(SourceId::new(0), 1, 2);
        let first = constraints.fresh(span);
        let second = constraints.fresh(span);
        let values = [
            Type::Int,
            Type::List(Box::new(Type::Any)),
            Type::Result(Box::new(first.clone()), Box::new(Type::Invalid)),
            Type::Result(Box::new(second.clone()), Box::new(Type::Invalid)),
        ];
        let mut table = TypeTable::default();
        let ids: Vec<_> = values.iter().cloned().map(|ty| table.intern(ty)).collect();
        assert_ne!(ids[2], ids[3]);
        for (ty, id) in values.into_iter().zip(ids) {
            let flags = table.flags(id);
            assert_eq!(table.get(id), &ty);
            assert_eq!(flags.inference, ty.contains_inference());
            assert_eq!(flags.recovery, ty.contains_recovery());
            assert_eq!(flags.any, ty.contains_any());
            assert_eq!(flags.scheme_callable, ty.contains_scheme_callable());
            assert_eq!(table.intern(ty), id);
        }
    }

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
        let mut result = Self::default();
        for (span, ty) in other { result.insert(span, ty); }
        result
    }
}
