use super::*;

impl InferenceContext {
    pub(super) fn representative(&self, mut id: MetaId) -> Result<MetaId, InferenceError> {
        for _ in 0..=self.limits.structural_depth {
            let meta = self.meta(id)?;
            if meta.parent == id { return Ok(id); }
            id = meta.parent;
        }
        Err(InferenceError::Limit("variable depth"))
    }
    pub fn resolved(&self, mut ty: TypeId) -> Result<TypeId, InferenceError> {
        for _ in 0..=self.limits.structural_depth {
            let TypeNode::Meta(id) = self.node(ty)? else { return Ok(ty) };
            let meta = self.meta(self.representative(*id)?)?;
            if let Some(binding) = meta.binding { ty = binding; } else { return Ok(meta.ty); }
        }
        Err(InferenceError::Limit("substitution depth"))
    }
    pub fn variable(&self, ty: TypeId) -> Result<Option<VariableView<'_>>, InferenceError> {
        let TypeNode::Meta(id) = self.node(self.resolved(ty)?)? else { return Ok(None) };
        let id = self.representative(*id)?; let meta = self.meta(id)?;
        Ok(Some(VariableView { id, kind: meta.kind, level: meta.level, origin: meta.origin, lacks: &meta.lacks }))
    }
    pub fn unify(&mut self, left: TypeId, right: TypeId, reason: ReasonId) -> Result<(), InferenceError> {
        self.reason_data(reason)?;
        self.constraint()?;
        self.counters.unifications += 1;
        self.probe(|graph| { graph.contribute(ConstraintRelation::Equality { left, right }, reason)?; graph.unify_inner(left, right, 0) })
    }
    pub fn assignable(&mut self, expected: TypeId, actual: TypeId, reason: ReasonId) -> Result<(), InferenceError> {
        self.reason_data(reason)?;
        self.constraint()?;
        self.probe(|graph| { graph.contribute(ConstraintRelation::Assignable { expected, actual }, reason)?; graph.assign_inner(expected, actual, 0) })
    }
    fn depth(&self, depth: usize) -> Result<(), InferenceError> {
        if depth > self.limits.structural_depth { Err(InferenceError::Limit("structural depth")) } else { Ok(()) }
    }
    pub(super) fn unify_inner(&mut self, left: TypeId, right: TypeId, depth: usize) -> Result<(), InferenceError> {
        self.work()?; self.depth(depth)?;
        let left = self.resolved(left)?; let right = self.resolved(right)?;
        let left_node = self.clone_node(left)?; let right_node = self.clone_node(right)?;
        if matches!(left_node, TypeNode::Poison) { return Err(InferenceError::Recovery(left)); }
        if matches!(right_node, TypeNode::Poison) { return Err(InferenceError::Recovery(right)); }
        if left == right { return Ok(()); }
        if self.type_kind(left)? != self.type_kind(right)? { return Err(InferenceError::KindMismatch); }
        match (left_node, right_node) {
            (TypeNode::Meta(a), TypeNode::Meta(b)) => self.union(a, b),
            (TypeNode::Meta(variable), _) => self.bind(variable, right),
            (_, TypeNode::Meta(variable)) => self.bind(variable, left),
            (TypeNode::Atom(a), TypeNode::Atom(b)) if a == b => Ok(()),
            (TypeNode::Rigid { scope: a, index: ai, kind: ak }, TypeNode::Rigid { scope: b, index: bi, kind: bk }) if a == b && ai == bi && ak == bk => Ok(()),
            (TypeNode::List(a), TypeNode::List(b)) | (TypeNode::Optional(a), TypeNode::Optional(b)) | (TypeNode::Stream(a), TypeNode::Stream(b)) => self.unify_inner(a, b, depth + 1),
            (TypeNode::Map(a, b), TypeNode::Map(c, d)) | (TypeNode::Result(a, b), TypeNode::Result(c, d)) => { self.unify_inner(a, c, depth + 1)?; self.unify_inner(b, d, depth + 1) }
            (TypeNode::Record(a), TypeNode::Record(b)) | (TypeNode::Row(a), TypeNode::Row(b)) => self.unify_rows(a, b, depth + 1),
            (TypeNode::Module(a), TypeNode::Module(b)) => {
                if a.len() != b.len() { return Err(InferenceError::TypeMismatch { left, right }); }
                for (a, b) in a.iter().zip(&b) {
                    if a.label != b.label || a.optional != b.optional { return Err(InferenceError::TypeMismatch { left, right }); }
                    self.unify_inner(a.ty, b.ty, depth + 1)?;
                }
                Ok(())
            }
            (TypeNode::Arrow(a), TypeNode::Arrow(b)) => {
                if a.kind != b.kind || a.params.len() != b.params.len() { return Err(InferenceError::TypeMismatch { left, right }); }
                for (a, b) in a.params.iter().zip(&b.params) {
                    if a.label != b.label || a.defaulted != b.defaulted || a.rest != b.rest { return Err(InferenceError::TypeMismatch { left, right }); }
                    self.unify_inner(a.ty, b.ty, depth + 1)?;
                }
                self.unify_inner(a.result, b.result, depth + 1)?;
                self.unify_effects(a.effects, b.effects)
            }
            (TypeNode::NonCompletion, TypeNode::NonCompletion) => Ok(()),
            _ => Err(InferenceError::TypeMismatch { left, right }),
        }
    }
    fn union(&mut self, a: MetaId, b: MetaId) -> Result<(), InferenceError> {
        let mut a = self.representative(a)?; let mut b = self.representative(b)?;
        if a == b { return Ok(()); }
        if self.meta(a)?.kind != self.meta(b)?.kind { return Err(InferenceError::KindMismatch); }
        if self.meta(a)?.rank < self.meta(b)?.rank { std::mem::swap(&mut a, &mut b); }
        let root = self.meta(a)?; let child = self.meta(b)?;
        let units = root.lacks.len() + child.lacks.len() + root.watchers.len() + child.watchers.len();
        let child_level = child.level; let child_rank = child.rank;
        self.work_many(units.saturating_mul(2))?;
        let lacks = merge_unique(&self.meta(a)?.lacks, &self.meta(b)?.lacks);
        if lacks.len() > self.limits.row_labels { return Err(InferenceError::Limit("row labels")); }
        let watchers = merge_unique(&self.meta(a)?.watchers, &self.meta(b)?.watchers);
        self.trail_meta(a)?; self.trail_meta(b)?;
        self.metas[b.index()].value.parent = a;
        let root = &mut self.metas[a.index()].value;
        if root.rank == child_rank { root.rank += 1; }
        root.level = root.level.min(child_level);
        root.lacks = lacks; root.watchers = watchers;
        self.wake(a)
    }
    fn bind(&mut self, variable: MetaId, value: TypeId) -> Result<(), InferenceError> {
        let variable = self.representative(variable)?;
        self.work_many(self.meta(variable)?.lacks.len() + self.meta(variable)?.watchers.len())?;
        let meta = self.meta(variable)?.clone();
        if meta.kind != self.type_kind(value)? { return Err(InferenceError::KindMismatch); }
        self.occurs_and_lower(variable, value, meta.level)?;
        if meta.kind == VariableKind::Row { self.add_lacks(value, &meta.lacks, 0)?; }
        self.trail_meta(variable)?;
        self.metas[variable.index()].value.binding = Some(value);
        self.wake(variable)
    }
    fn occurs_and_lower(&mut self, variable: MetaId, value: TypeId, level: u32) -> Result<(), InferenceError> {
        self.lower_effect_levels(value, level)?;
        let mut pending = vec![(value, 0usize)]; let mut visited = FxHashSet::default();
        while let Some((ty, depth)) = pending.pop() {
            self.counters.occurs_steps += 1;
            self.work()?; self.depth(depth)?;
            let ty = self.resolved(ty)?;
            if !visited.insert(ty) { continue; }
            match self.clone_node(ty)? {
                TypeNode::Meta(id) => {
                    let id = self.representative(id)?;
                    if id == variable { return Err(InferenceError::Occurs { variable, within: value }); }
                    if self.meta(id)?.level > level { self.trail_meta(id)?; self.metas[id.index()].value.level = level; self.counters.level_lowerings += 1; }
                }
                TypeNode::Rigid { scope, .. } => { if self.scheme(scope)?.scope_level > level { return Err(InferenceError::ScopeEscape); } }
                TypeNode::Poison => return Err(InferenceError::Recovery(ty)),
                _ => for child in self.children(ty)? { pending.push((child, depth + 1)); },
            }
        }
        Ok(())
    }
    pub fn capture(&mut self, ty: TypeId, level: u32, reason: ReasonId) -> Result<(), InferenceError> {
        self.reason_data(reason)?;
        self.constraint()?;
        self.probe(|graph| {
            graph.contribute(ConstraintRelation::Capture { ty, level }, reason)?;
            graph.lower_effect_levels(ty, level)?;
            let mut pending = vec![(ty, 0usize)]; let mut seen = FxHashSet::default();
            while let Some((ty, depth)) = pending.pop() {
                graph.work()?; graph.depth(depth)?; let ty = graph.resolved(ty)?;
                if !seen.insert(ty) { continue; }
                if let TypeNode::Meta(id) = graph.clone_node(ty)? {
                    if graph.meta(id)?.level > level { graph.trail_meta(id)?; graph.metas[id.index()].value.level = level; graph.counters.level_lowerings += 1; }
                } else if let TypeNode::Rigid { scope, .. } = graph.node(ty)? {
                    if graph.scheme(*scope)?.scope_level > level { return Err(InferenceError::ScopeEscape); }
                } else { for child in graph.children(ty)? { pending.push((child, depth + 1)); } }
            }
            Ok(())
        })
    }
    pub(super) fn children(&self, ty: TypeId) -> Result<Vec<TypeId>, InferenceError> {
        Ok(match self.node(ty)? {
            TypeNode::List(item) | TypeNode::Optional(item) | TypeNode::Stream(item) => vec![*item],
            TypeNode::Map(key, value) | TypeNode::Result(key, value) => vec![*key, *value],
            TypeNode::Arrow(arrow) => arrow.params.iter().map(|parameter| parameter.ty).chain(std::iter::once(arrow.result)).collect(),
            TypeNode::Module(exports) => exports.iter().map(|field| field.ty).collect(),
            TypeNode::Record(row) | TypeNode::Row(row) => { let row = self.row_data(*row)?; row.fields.iter().map(|field| field.ty).chain(row.tail).collect() }
            _ => Vec::new(),
        })
    }
    pub fn row(&mut self, fields: Vec<RowField>, tail: Option<TypeId>) -> Result<RowId, InferenceError> {
        self.probe(|graph| graph.make_row(fields, tail))
    }
    fn make_row(&mut self, mut fields: Vec<RowField>, tail: Option<TypeId>) -> Result<RowId, InferenceError> {
        if fields.len() > self.limits.row_labels { return Err(InferenceError::Limit("row labels")); }
        let mut comparisons = 0usize;
        fields.sort_by(|left, right| { comparisons += 1; left.label.cmp(&right.label) });
        self.work_many(comparisons + fields.len())?;
        for pair in fields.windows(2) { if pair[0].label == pair[1].label { return Err(InferenceError::DuplicateLabel(pair[0].label)); } }
        for field in &fields { self.value_type(field.ty)?; }
        if let Some(tail) = tail {
            if self.type_kind(tail)? != VariableKind::Row { return Err(InferenceError::KindMismatch); }
            self.add_lacks(tail, &fields.iter().map(|field| field.label).collect::<Vec<_>>(), 0)?;
        }
        self.counters.attempted_nodes += 1;
        if self.counters.attempted_nodes > self.limits.type_row_nodes as u64 { return Err(InferenceError::Limit("type and row nodes")); }
        let id = RowId { index: self.rows.len() as u32, generation: Self::generation()? };
        self.rows.push(Slot { generation: id.generation, value: Row { fields, tail } });
        Ok(id)
    }
    fn add_lacks(&mut self, ty: TypeId, labels: &[Name], depth: usize) -> Result<(), InferenceError> {
        self.work()?; self.depth(depth)?;
        let ty = self.resolved(ty)?;
        match self.clone_node(ty)? {
            TypeNode::Meta(id) => {
                if self.meta(id)?.kind != VariableKind::Row { return Err(InferenceError::KindMismatch); }
                self.work_many((self.meta(id)?.lacks.len() + labels.len()).saturating_mul(2))?;
                let lacks = merge_unique(&self.meta(id)?.lacks, labels);
                if lacks.len() > self.limits.row_labels { return Err(InferenceError::Limit("row labels")); }
                if lacks.len() != self.meta(id)?.lacks.len() {
                    self.trail_meta(id)?;
                    self.metas[id.index()].value.lacks = lacks;
                }
                Ok(())
            }
            TypeNode::Row(row) => {
                let row = self.clone_row(row)?;
                self.work_many(labels.len().saturating_mul((usize::BITS - row.fields.len().leading_zeros()) as usize))?;
                for label in labels { if row.fields.binary_search_by_key(label, |field| field.label).is_ok() { return Err(InferenceError::Lacks(*label)); } }
                if let Some(tail) = row.tail { self.add_lacks(tail, labels, depth + 1)?; }
                Ok(())
            }
            TypeNode::Rigid { scope, index, kind: VariableKind::Row } => {
                let count = self.scheme(scope)?.quantifiers.get(index as usize).ok_or(InferenceError::InvalidScheme)?.lacks.len();
                self.work_many(labels.len().saturating_mul((usize::BITS - count.leading_zeros()) as usize))?;
                let quantifier = &self.scheme(scope)?.quantifiers[index as usize];
                for label in labels { if quantifier.lacks.binary_search(label).is_err() { return Err(InferenceError::Lacks(*label)); } }
                Ok(())
            }
            _ => Err(InferenceError::KindMismatch),
        }
    }
    fn expanded_row(&mut self, row: RowId, depth: usize) -> Result<Row, InferenceError> {
        self.depth(depth)?;
        let mut row = self.clone_row(row)?; let mut seen = FxHashSet::default();
        let mut tail_depth = depth;
        while let Some(tail) = row.tail {
            self.work()?; tail_depth += 1; self.depth(tail_depth)?;
            let tail = self.resolved(tail)?;
            if !seen.insert(tail) { return Err(InferenceError::InvalidScheme); }
            if let TypeNode::Row(next) = self.clone_node(tail)? {
                let next = self.row_data(next)?;
                if row.fields.len() + next.fields.len() > self.limits.row_labels { return Err(InferenceError::Limit("row labels")); }
                row.fields.extend_from_slice(&next.fields); row.tail = next.tail;
            } else { row.tail = Some(tail); break; }
        }
        let mut comparisons = 0usize;
        row.fields.sort_by(|left, right| { comparisons += 1; left.label.cmp(&right.label) });
        self.work_many(comparisons + row.fields.len())?;
        for pair in row.fields.windows(2) { if pair[0].label == pair[1].label { return Err(InferenceError::DuplicateLabel(pair[0].label)); } }
        Ok(row)
    }
    fn unify_rows(&mut self, left: RowId, right: RowId, depth: usize) -> Result<(), InferenceError> {
        self.counters.row_steps += 1;
        self.work()?; self.depth(depth)?;
        if left == right { return Ok(()); }
        let a = self.expanded_row(left, depth)?; let b = self.expanded_row(right, depth)?;
        let mut left_only = Vec::new(); let mut right_only = Vec::new(); let (mut ai, mut bi) = (0, 0);
        while ai < a.fields.len() || bi < b.fields.len() {
            self.work()?;
            match (a.fields.get(ai), b.fields.get(bi)) {
                (Some(af), Some(bf)) if af.label == bf.label => { self.unify_inner(af.ty, bf.ty, depth + 1)?; ai += 1; bi += 1; }
                (Some(af), Some(bf)) if af.label < bf.label => { left_only.push(*af); ai += 1; }
                (Some(_), Some(bf)) => { right_only.push(*bf); bi += 1; }
                (Some(af), None) => { left_only.push(*af); ai += 1; }
                (None, Some(bf)) => { right_only.push(*bf); bi += 1; }
                (None, None) => break,
            }
        }
        if a.tail.is_none() && !right_only.is_empty() { return Err(InferenceError::MissingField(right_only[0].label)); }
        if b.tail.is_none() && !left_only.is_empty() { return Err(InferenceError::MissingField(left_only[0].label)); }
        match (a.tail, b.tail) {
            (None, None) => Ok(()),
            (Some(tail), None) => { let row = self.make_row(right_only, None)?; let row = self.row_type(row)?; self.unify_inner(tail, row, depth + 1) }
            (None, Some(tail)) => { let row = self.make_row(left_only, None)?; let row = self.row_type(row)?; self.unify_inner(tail, row, depth + 1) }
            (Some(a_tail), Some(b_tail)) if left_only.is_empty() && right_only.is_empty() => self.unify_inner(a_tail, b_tail, depth + 1),
            (Some(a_tail), Some(b_tail)) => {
                if self.resolved(a_tail)? == self.resolved(b_tail)? { return Err(InferenceError::Lacks(left_only.first().or(right_only.first()).unwrap().label)); }
                let level = self.variable(a_tail)?.map(|meta| meta.level).unwrap_or(0).min(self.variable(b_tail)?.map(|meta| meta.level).unwrap_or(0));
                let origin = self.variable(a_tail)?.map(|meta| meta.origin).or(self.variable(b_tail)?.map(|meta| meta.origin)).ok_or(InferenceError::ScopeEscape)?;
                let shared = self.fresh_row(level, origin)?;
                let ar = self.make_row(right_only, Some(shared))?; let ar = self.row_type(ar)?;
                let br = self.make_row(left_only, Some(shared))?; let br = self.row_type(br)?;
                self.unify_inner(a_tail, ar, depth + 1)?; self.unify_inner(b_tail, br, depth + 1)
            }
        }
    }
    pub fn require_field(&mut self, record: TypeId, label: Name, level: u32, reason: ReasonId) -> Result<TypeId, InferenceError> {
        let origin = self.reason_data(reason)?.span;
        self.constraint()?;
        self.probe(|graph| {
            let original_record = record;
            let record = graph.resolved(record)?;
            let result = match graph.clone_node(record)? {
                TypeNode::Record(row) => graph.project_row(row, label, level, origin, 0),
                TypeNode::Meta(_) => {
                    let field = graph.fresh(level, origin)?; let tail = graph.fresh_row(level, origin)?;
                    let row = graph.make_row(vec![RowField { label, ty: field }], Some(tail))?;
                    let required = graph.record(row)?; graph.unify_inner(record, required, 0)?; Ok(field)
                }
                _ => Err(InferenceError::TypeMismatch { left: record, right: record }),
            }?;
            graph.contribute(ConstraintRelation::Projection { record: original_record, label, result }, reason)?;
            Ok(result)
        })
    }
    fn project_row(&mut self, row: RowId, label: Name, level: u32, origin: Span, depth: usize) -> Result<TypeId, InferenceError> {
        self.counters.row_steps += 1;
        self.work()?; self.depth(depth)?;
        self.work_many((usize::BITS - self.row_data(row)?.fields.len().leading_zeros()) as usize)?;
        let descriptor = self.row_data(row)?;
        if let Ok(index) = descriptor.fields.binary_search_by_key(&label, |field| field.label) { return Ok(descriptor.fields[index].ty); }
        let tail = descriptor.tail.ok_or(InferenceError::MissingField(label))?; let tail = self.resolved(tail)?;
        match self.clone_node(tail)? {
            TypeNode::Row(row) => self.project_row(row, label, level, origin, depth + 1),
            TypeNode::Meta(_) => {
                let field = self.fresh(level, origin)?; let rest = self.fresh_row(level, origin)?;
                let row = self.make_row(vec![RowField { label, ty: field }], Some(rest))?; let row = self.row_type(row)?;
                self.unify_inner(tail, row, depth + 1)?; Ok(field)
            }
            _ => Err(InferenceError::MissingField(label)),
        }
    }
    fn assign_inner(&mut self, expected: TypeId, actual: TypeId, depth: usize) -> Result<(), InferenceError> {
        self.work()?; self.depth(depth)?;
        let expected = self.resolved(expected)?; let actual = self.resolved(actual)?;
        let expected_node = self.clone_node(expected)?; let actual_node = self.clone_node(actual)?;
        if matches!(expected_node, TypeNode::Poison) { return Err(InferenceError::Recovery(expected)); }
        if matches!(actual_node, TypeNode::Poison) { return Err(InferenceError::Recovery(actual)); }
        if expected == actual { return Ok(()); }
        match (expected_node, actual_node) {
            (TypeNode::Atom(Atom::Any), _) => self.value_type(actual),
            (TypeNode::Atom(Atom::ErasedRecord), TypeNode::Record(_)) => Ok(()),
            (TypeNode::Atom(Atom::Pure), TypeNode::Arrow(arrow)) if arrow.kind == CallableKind::Pure => Ok(()),
            (TypeNode::Atom(Atom::Proc), TypeNode::Arrow(arrow)) if arrow.kind == CallableKind::Proc => Ok(()),
            (TypeNode::Atom(Atom::Int), TypeNode::Atom(Atom::UInt)) | (TypeNode::Atom(Atom::UInt), TypeNode::Atom(Atom::Int)) => Ok(()),
            (TypeNode::Atom(Atom::Error), TypeNode::Atom(Atom::ErrorFamily(_) | Atom::ErrorVariant { .. } | Atom::ErrorFacet(_) | Atom::ProcessError)) => Ok(()),
            (TypeNode::Atom(Atom::ErrorFamily(expected)), TypeNode::Atom(Atom::ErrorVariant { family, .. })) if expected == family => Ok(()),
            (TypeNode::Optional(_), TypeNode::Atom(Atom::Null)) => Ok(()),
            (TypeNode::Optional(e), TypeNode::Optional(a)) => self.unify_inner(e, a, depth + 1),
            (TypeNode::Optional(e), _) => self.assign_inner(e, actual, depth + 1),
            (TypeNode::Result(es, ee), TypeNode::Result(as_, ae)) => { self.assign_inner(es, as_, depth + 1)?; self.assign_inner(ee, ae, depth + 1) }
            (TypeNode::Arrow(expected_arrow), TypeNode::Arrow(actual_arrow)) => {
                if expected_arrow.kind != actual_arrow.kind || expected_arrow.params.len() != actual_arrow.params.len() { return Err(InferenceError::TypeMismatch { left: expected, right: actual }); }
                for (expected, actual) in expected_arrow.params.iter().zip(&actual_arrow.params) {
                    if expected.label != actual.label || expected.defaulted != actual.defaulted || expected.rest != actual.rest { return Err(InferenceError::InvalidScheme); }
                    self.unify_inner(expected.ty, actual.ty, depth + 1)?;
                }
                self.assign_inner(expected_arrow.result, actual_arrow.result, depth + 1)?;
                self.include_effects_inner(actual_arrow.effects, expected_arrow.effects)
            }
            (TypeNode::Record(e), TypeNode::Record(a)) => {
                let expected_row = self.expanded_row(e, depth + 1)?;
                if expected_row.tail.is_some() { return self.unify_rows(e, a, depth + 1); }
                for field in expected_row.fields {
                    let origin = Span::at(crate::source::SourceId::new(0), 0);
                    let actual_field = self.project_row(a, field.label, 0, origin, depth + 1)?;
                    self.assign_inner(field.ty, actual_field, depth + 1)?;
                }
                Ok(())
            }
            (TypeNode::Module(e), TypeNode::Module(a)) => {
                for expected_field in e {
                    if let Ok(index) = a.binary_search_by_key(&expected_field.label, |field| field.label) { self.assign_inner(expected_field.ty, a[index].ty, depth + 1)?; }
                    else if !expected_field.optional { return Err(InferenceError::MissingField(expected_field.label)); }
                }
                Ok(())
            }
            _ => self.unify_inner(expected, actual, depth + 1),
        }
    }
}

/// Both inputs are sorted unique descriptors. A union traverses each element
/// once, including identical labels, rather than repeatedly searching a vector.
fn merge_unique<T: Copy + Ord>(left: &[T], right: &[T]) -> Vec<T> {
    let mut merged = Vec::with_capacity(left.len() + right.len());
    let (mut a, mut b) = (0, 0);
    while a < left.len() || b < right.len() {
        match (left.get(a), right.get(b)) {
            (Some(x), Some(y)) if x == y => { merged.push(*x); a += 1; b += 1; }
            (Some(x), Some(y)) if x < y => { merged.push(*x); a += 1; }
            (Some(_), Some(y)) => { merged.push(*y); b += 1; }
            (Some(x), None) => { merged.push(*x); a += 1; }
            (None, Some(y)) => { merged.push(*y); b += 1; }
            (None, None) => break,
        }
    }
    merged
}
