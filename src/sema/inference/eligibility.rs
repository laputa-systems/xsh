use super::*;

impl Atom {
    pub(crate) fn display_eligible(self) -> bool {
        matches!(self, Atom::Any | Atom::Str | Atom::Int | Atom::UInt | Atom::Bool | Atom::Path | Atom::Duration | Atom::Float)
    }
}

impl Eligibility {
    /// Prepared Display witnesses use the same scalar policy as inference.
    pub(crate) fn accepts_closed_display(self, ty: &crate::sema::types::Type) -> bool {
        self == Eligibility::Display && Atom::from_type(ty).is_some_and(Atom::display_eligible)
    }
}

impl InferenceContext {
    pub fn allow_wire_tag(&mut self, tag: Name) -> Result<(), InferenceError> {
        self.work()?;
        if self.wire_tags.insert(tag) && self.transactions > 0 { self.trail.push(Trail::WireTag(tag)); }
        Ok(())
    }
    pub fn require_equality_compatible(&mut self, left: TypeId, right: TypeId, reason: ReasonId) -> Result<RequirementId, InferenceError> {
        self.probe(|graph| { graph.value_type(left)?; graph.value_type(right)?; graph.new_requirement(RequirementTemplate::EqualityCompatible { left, right }, reason) })
    }
    pub(super) fn solve_equality_compatible(&mut self, id: RequirementId, left: TypeId, right: TypeId) -> Result<(), InferenceError> {
        for ty in [left, right] {
            if !self.free_metas(ty)?.is_empty() || !self.rigid_nodes(ty)?.is_empty() || !self.free_effects(ty)?.is_empty() || !self.rigid_effects(ty)?.is_empty() { return Ok(()); }
        }
        let reason = self.requirement(id)?.reason;
        match self.trial(|graph| graph.assignable(left, right, reason)) {
            Ok(()) => {},
            Err(InferenceError::Limit(bound)) => return Err(InferenceError::Limit(bound)),
            Err(_) => self.trial(|graph| graph.assignable(right, left, reason))?,
        }
        self.trail_requirement(id)?; self.requirements[id.index()].value.eligibility = true; Ok(())
    }
    pub fn require_eligibility(&mut self, predicate: Eligibility, ty: TypeId, reason: ReasonId) -> Result<RequirementId, InferenceError> {
        self.probe(|graph| { graph.value_type(ty)?; graph.new_requirement(RequirementTemplate::Eligibility { predicate, ty }, reason) })
    }
    pub fn eligibility_satisfied(&self, id: RequirementId) -> Result<bool, InferenceError> { Ok(self.requirement(id)?.eligibility) }
    pub fn check_eligibility(&mut self, predicate: Eligibility, ty: TypeId) -> Result<bool, InferenceError> { self.eligibility_state(predicate, ty) }
    pub(super) fn solve_eligibility(&mut self, id: RequirementId, predicate: Eligibility, ty: TypeId) -> Result<(), InferenceError> {
        let complete = self.eligibility_state(predicate, ty).map_err(|error| match error { InferenceError::Boundary(_) => InferenceError::UnsupportedOperation(id), other => other })?;
        if complete {
            self.trail_requirement(id)?; self.requirements[id.index()].value.eligibility = true;
        }
        Ok(())
    }
    pub(super) fn eligibility_state(&mut self, predicate: Eligibility, root: TypeId) -> Result<bool, InferenceError> {
        let mut pending = vec![(predicate, root, 0usize)]; let mut seen = FxHashSet::default(); let mut complete = true;
        while let Some((predicate, ty, depth)) = pending.pop() {
            self.work()?; if depth > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
            let ty = self.resolved(ty)?; if !seen.insert((predicate, ty)) { continue; }
            match (predicate, self.clone_node(ty)?) {
                (_, TypeNode::Meta(_) | TypeNode::Rigid { .. }) => complete = false,
                (_, TypeNode::Poison | TypeNode::NonCompletion) => return Err(InferenceError::Recovery(ty)),
                (Eligibility::Display, TypeNode::Atom(atom)) if atom.display_eligible() => {},
                (Eligibility::Error, TypeNode::Atom(Atom::Error | Atom::ProcessError | Atom::ErrorFamily(_) | Atom::ErrorVariant { .. } | Atom::ErrorFacet(_))) => {},
                (Eligibility::CommandTarget, TypeNode::Atom(Atom::Str | Atom::Path | Atom::Any)) => {},
                (Eligibility::CommandArgv, TypeNode::Atom(Atom::Any)) => {},
                (Eligibility::CommandArgv, TypeNode::List(item)) => match self.node(self.resolved(item)?)? {
                    TypeNode::Atom(Atom::Str | Atom::Path | Atom::Any) => {},
                    TypeNode::Meta(_) | TypeNode::Rigid { .. } => complete = false,
                    _ => return Err(InferenceError::Boundary("native command argv items must be Str or Path")),
                },
                (Eligibility::YieldItem, TypeNode::Stream(_)) => return Err(InferenceError::Boundary("Stream must be delegated instead of yielded as an item")),
                (Eligibility::YieldItem, _) => {},
                (Eligibility::NonUnit, TypeNode::Atom(Atom::Unit)) => return Err(InferenceError::Boundary("Unit is not a stream item")),
                (Eligibility::NonUnit, _) => {},
                (Eligibility::ArgvItem | Eligibility::ArgvExpansion, TypeNode::Atom(Atom::Str | Atom::Path | Atom::Int | Atom::UInt | Atom::Bool | Atom::Duration | Atom::Any)) => {},
                (Eligibility::ArgvExpansion, TypeNode::List(item)) => pending.push((Eligibility::ArgvItem, item, depth + 1)),
                (Eligibility::CountKey, TypeNode::Atom(Atom::Str | Atom::Int | Atom::UInt | Atom::Bool)) => {},
                (Eligibility::Record, TypeNode::Record(_)) => {},
                (Eligibility::SortableKey, TypeNode::Atom(Atom::Any)) => {},
                (Eligibility::Sortable | Eligibility::SortableKey, TypeNode::Atom(Atom::Int | Atom::Str | Atom::Bool | Atom::Path)) => {},
                (Eligibility::Sortable | Eligibility::SortableKey, TypeNode::Record(row) | TypeNode::Row(row)) => {
                    let row = self.clone_row(row)?; for field in row.fields { pending.push((predicate, field.ty, depth + 1)); } if let Some(tail) = row.tail { pending.push((predicate, tail, depth + 1)); }
                },
                (Eligibility::MapKey, TypeNode::Atom(Atom::Str | Atom::Int | Atom::UInt | Atom::Bool | Atom::Bytes | Atom::Path | Atom::Duration)) => {},
                (Eligibility::JsonCompatible, TypeNode::Atom(Atom::Null | Atom::Bool | Atom::Int | Atom::UInt | Atom::Float | Atom::Str | Atom::Any | Atom::ErasedRecord)) => {},
                (Eligibility::JsonCompatible, TypeNode::Atom(Atom::Tag(tag))) if self.wire_tags.contains(&tag) => {},
                // JSON traverses materialized data. Producers require explicit
                // collection before their item types can certify an array.
                (Eligibility::JsonCompatible, TypeNode::Optional(item) | TypeNode::List(item)) => pending.push((predicate, item, depth + 1)),
                (Eligibility::JsonCompatible, TypeNode::Map(key, value)) => {
                    match self.node(self.resolved(key)?)? { TypeNode::Atom(Atom::Str) => {}, TypeNode::Meta(_) | TypeNode::Rigid { .. } => complete = false, _ => return Err(InferenceError::Boundary("JSON Map key must be Str")) }
                    pending.push((predicate, value, depth + 1));
                }
                (Eligibility::JsonCompatible, TypeNode::Record(row) | TypeNode::Row(row)) => {
                    let row = self.clone_row(row)?;
                    for field in row.fields { pending.push((predicate, field.ty, depth + 1)); } if let Some(tail) = row.tail { pending.push((predicate, tail, depth + 1)); }
                }
                _ => return Err(InferenceError::Boundary("unsupported builtin operand eligibility")),
            }
        }
        Ok(complete)
    }
}
