use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug)]
pub(super) struct PatternNominalReceipt {
    root: Option<TypeId>,
    identity: Option<QualifiedNominalIdentity>,
    required: bool,
}

impl<Graph> SolvedTypes<Graph> {
    pub(crate) fn checked_pattern_scope(&self, identity: PatternIdentity) -> Result<Option<SchemeId>, InferenceError> {
        self.pattern_scope(identity, self.checked_pattern(identity)?)
    }

    pub(crate) fn checked_pattern_nominal(&self, identity: PatternIdentity, position: PatternTypePosition) -> Result<Option<QualifiedNominalIdentity>, InferenceError> {
        self.checked_pattern(identity)?;
        let receipt = self.pattern_nominal_receipt(identity, position)?;
        if receipt.required && receipt.identity.is_none() { return Err(InferenceError::Boundary("nominal pattern root has no checked declaration owner")); }
        Ok(receipt.identity)
    }

    fn pattern_nominal_receipt(&self, identity: PatternIdentity, position: PatternTypePosition) -> Result<&PatternNominalReceipt, InferenceError> {
        let (input, tested) = self.original_pattern_nominals.get(&identity).ok_or(InferenceError::InvalidScheme)?;
        let receipt = match position { PatternTypePosition::Input => input, PatternTypePosition::Tested => tested };
        if receipt.root.and_then(|root| self.nominals.get(&root).copied()) != receipt.identity { return Err(InferenceError::InvalidScheme); }
        Ok(receipt)
    }

    pub(crate) fn checked_pattern(&self, identity: PatternIdentity) -> Result<&SolvedPattern, InferenceError> {
        let (original, value_scope) = self.original_patterns.get(&identity).ok_or(InferenceError::InvalidScheme)?;
        let current = self.patterns.get(&identity).ok_or(InferenceError::InvalidScheme)?;
        if self.pattern_value_scopes.get(&identity).copied() != *value_scope { return Err(InferenceError::InvalidScheme); }
        if !std::sync::Arc::ptr_eq(original, current) && original.as_ref() != current.as_ref() { return Err(InferenceError::InvalidScheme); }
        self.pattern_scope(identity, original)?;
        Ok(original.as_ref())
    }

    fn pattern_scope(&self, identity: PatternIdentity, pattern: &SolvedPattern) -> Result<Option<SchemeId>, InferenceError> {
        let lexical = pattern.caller.map(|caller| {
            if caller.source != identity.source || caller.namespace != identity.namespace { return Err(InferenceError::InvalidScheme); }
            self.declarations.get(&caller).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)
        }).transpose()?;
        Ok(self.pattern_value_scopes.get(&identity).copied().or(lexical))
    }

    pub(super) fn pattern_roots(&self) -> Result<Vec<ScopedRoot>, InferenceError> {
        let mut roots = Vec::new();
        for (&identity, pattern) in &self.patterns {
            let scope = self.pattern_scope(identity, pattern)?;
            roots.extend(std::iter::once(pattern.input).chain(pattern.tested).chain(pattern.captures.iter().map(|capture| capture.ty)).map(|ty| ScopedRoot { ty, scope }));
            roots.extend(Self::pattern_decision_types(&pattern.decision).into_iter().map(|ty| ScopedRoot { ty, scope }));
        }
        Ok(roots)
    }

    fn pattern_decision_types(decision: &SolvedPatternDecision) -> Vec<TypeId> {
        match decision {
            SolvedPatternDecision::Result { payload, .. } => payload.iter().copied().collect(),
            SolvedPatternDecision::TagConstructor { fields, .. } | SolvedPatternDecision::TagFields { fields } => fields.clone(),
            SolvedPatternDecision::ErrorVariant { fields, .. } => fields.iter().map(|(_, ty)| *ty).collect(),
            _ => Vec::new(),
        }
    }

    pub(super) fn pattern_payload_bytes(&self) -> usize {
        use std::mem::size_of;
        let mut allocations = BTreeSet::new();
        let mut records = BTreeSet::new();
        self.patterns.values().chain(self.original_patterns.values().map(|(pattern, _)| pattern)).filter(|pattern| records.insert(std::sync::Arc::as_ptr(pattern) as usize)).map(|pattern| {
            size_of::<SolvedPattern>() + 2 * size_of::<usize>()
                + match &pattern.shape {
                    SolvedPatternShape::Record { fields } | SolvedPatternShape::ErrorVariant { fields } => fields.capacity() * size_of::<Name>(),
                    SolvedPatternShape::Literal { value: Some(value), .. } => {
                        use crate::sema::constants::LiteralConstant;
                        use std::sync::Arc;
                        let (pointer, bytes) = match value {
                            LiteralConstant::Str(value) | LiteralConstant::Path(value) => (Arc::as_ptr(value) as *const u8 as usize, value.len()),
                            LiteralConstant::Bytes(value) => (Arc::as_ptr(value) as *const u8 as usize, value.len()),
                            LiteralConstant::Regex(value) => (Arc::as_ptr(&value.pattern) as *const u8 as usize, value.pattern.len()),
                            _ => (0, 0),
                        };
                        if pointer != 0 && allocations.insert(pointer) { bytes + 2 * size_of::<usize>() } else { 0 }
                    }
                    _ => 0,
                }
                + pattern.children.capacity() * size_of::<PatternIdentity>()
                + pattern.captures.capacity() * size_of::<SolvedPatternCapture>()
                + pattern.captures.iter().map(|capture| capture.branches.capacity() * size_of::<PatternCaptureIdentity>()).sum::<usize>()
                + match &pattern.decision {
                    SolvedPatternDecision::TagConstructor { fields, .. } | SolvedPatternDecision::TagFields { fields } => fields.capacity() * size_of::<TypeId>(),
                    SolvedPatternDecision::ErrorVariant { fields, .. } => fields.capacity() * size_of::<(Name, TypeId)>(),
                    _ => 0,
                }
        }).sum()
    }

    pub(super) fn pattern_source_edges(&self) -> u64 {
        self.patterns.values().map(|pattern| {
            1 + usize::from(pattern.tested.is_some()) + pattern.children.len()
                + match &pattern.shape {
                    SolvedPatternShape::Record { fields } | SolvedPatternShape::ErrorVariant { fields } => fields.len(),
                    SolvedPatternShape::Literal { .. } | SolvedPatternShape::Alias { .. } => 1,
                    _ => 0,
                }
                + Self::pattern_decision_types(&pattern.decision).len()
                + pattern.captures.iter().map(|capture| 1 + capture.branches.len()).sum::<usize>()
        }).sum::<usize>() as u64 + self.original_patterns.len() as u64
            + self.original_pattern_nominals.len() as u64
            + self.original_pattern_nominals.values().map(|(input, tested)| usize::from(input.root.is_some()) + usize::from(tested.root.is_some()) + usize::from(input.identity.is_some()) + usize::from(tested.identity.is_some())).sum::<usize>() as u64
            + self.original_patterns.values().filter(|(_, scope)| scope.is_some()).count() as u64
            + self.pattern_value_scopes.len() as u64
    }

    pub(super) fn validate_patterns(&self, graph: &InferenceContext) -> Result<u64, InferenceError> {
        let invalid = || InferenceError::InvalidScheme;
        if self.patterns.len() != self.original_patterns.len() { return Err(invalid()); }
        if self.original_pattern_nominals.len() != self.original_patterns.len() { return Err(invalid()); }
        for (identity, pattern) in &self.patterns {
            self.pattern_nominal_receipt(*identity, PatternTypePosition::Input)?;
            self.pattern_nominal_receipt(*identity, PatternTypePosition::Tested)?;
            let (original, value_scope) = self.original_patterns.get(identity).ok_or_else(invalid)?;
            if self.pattern_value_scopes.get(identity).copied() != *value_scope { return Err(invalid()); }
            if !std::sync::Arc::ptr_eq(pattern, original) && pattern.as_ref() != original.as_ref() { return Err(invalid()); }
        }
        let mut parents = BTreeMap::new();
        let captures = self.patterns.values().flat_map(|pattern| pattern.captures.iter().map(|capture| (capture.identity, capture.ty))).collect::<BTreeMap<_, _>>();
        let mut compared = BTreeSet::new();
        for (&identity, pattern) in &self.patterns {
            self.pattern_scope(identity, pattern)?;
            let mut children = BTreeSet::new();
            for &child in &pattern.children {
                if (child.source, child.namespace) != (identity.source, identity.namespace)
                    || !children.insert(child) || !self.patterns.contains_key(&child)
                    || parents.insert(child, identity).is_some() { return Err(invalid()); }
            }
            let mut names = BTreeSet::new();
            for capture in &pattern.captures {
                if capture.identity.pattern != identity || !names.insert(capture.identity.name) { return Err(invalid()); }
                if matches!(pattern.decision, SolvedPatternDecision::Alternation) {
                    if capture.branches.len() != pattern.children.len() || capture.branches.is_empty() { return Err(invalid()); }
                } else if !capture.branches.is_empty() { return Err(invalid()); }
                let mut branches = BTreeSet::new();
                for &branch in &capture.branches {
                    if branch.pattern == identity || branch.name != capture.identity.name || !branches.insert(branch) { return Err(invalid()); }
                    let original = *captures.get(&branch).ok_or_else(invalid)?;
                    if !capture_types_match(graph, original, capture.ty, &mut compared)? { return Err(invalid()); }
                }
                let expected = if matches!(pattern.decision, SolvedPatternDecision::Type) { pattern.tested.ok_or_else(invalid)? } else { pattern.input };
                if capture.branches.is_empty() && graph.resolved(capture.ty)? != graph.resolved(expected)? { return Err(invalid()); }
            }
        }
        if self.pattern_value_scopes.keys().any(|identity| !self.patterns.contains_key(identity)) { return Err(invalid()); }
        // A single forest traversal proves that joined captures came from
        // their original branch subtrees without rescanning each pattern.
        let mut intervals = BTreeMap::new();
        let mut seen = BTreeSet::new();
        let mut clock = 0usize;
        for &root in self.patterns.keys().filter(|identity| !parents.contains_key(identity)) {
            let mut pending = vec![(root, false)];
            while let Some((identity, exit)) = pending.pop() {
                if exit {
                    let (start, _) = intervals.get(&identity).copied().ok_or_else(invalid)?;
                    intervals.insert(identity, (start, clock));
                    clock += 1;
                } else {
                    if !seen.insert(identity) { return Err(invalid()); }
                    intervals.insert(identity, (clock, clock));
                    clock += 1;
                    pending.push((identity, true));
                    pending.extend(self.patterns[&identity].children.iter().rev().map(|&child| (child, false)));
                }
            }
        }
        if seen.len() != self.patterns.len() { return Err(invalid()); }
        for (&identity, pattern) in &self.patterns {
            let (start, end) = intervals[&identity];
            for capture in &pattern.captures {
                for (branch, child) in capture.branches.iter().zip(&pattern.children) {
                    let &(branch_start, branch_end) = intervals.get(&branch.pattern).ok_or_else(invalid)?;
                    if branch_start <= start || branch_end >= end { return Err(invalid()); }
                    let &(child_start, child_end) = intervals.get(child).ok_or_else(invalid)?;
                    if branch_start < child_start || branch_end > child_end { return Err(invalid()); }
                }
            }
        }
        Ok(compared.len() as u64)
    }
}

impl SolvedTypes<InferenceContext> {
    // Capture owners after declaration solving and original import publication.
    // Unknown nominal provenance remains unavailable instead of becoming a
    // non-nominal permission to prepare the pattern.
    pub(super) fn capture_pattern_nominals(&mut self) -> Result<(), InferenceError> {
        use crate::sema::inference::{Atom, TypeNode};
        if !self.original_pattern_nominals.is_empty() { return Err(InferenceError::InvalidScheme); }
        self.graph.charge_source_fact_nodes(self.original_patterns.len() as u64)?;
        for (&identity, (pattern, _)) in &self.original_patterns {
            let mut receipts = Vec::with_capacity(2);
            for ty in [Some(pattern.input), pattern.tested] {
                self.graph.charge_source_fact_work(1)?;
                let root = ty.map(|ty| self.graph.resolved(ty)).transpose()?;
                let required = root.map(|root| self.graph.node(root).map(|node| matches!(node,
                    TypeNode::Atom(Atom::Tag(_) | Atom::ErrorFamily(_) | Atom::ErrorVariant { .. } | Atom::ErrorFacet(_) | Atom::ProcessError)))).transpose()?.unwrap_or(false);
                let owner = root.and_then(|root| self.nominals.get(&root).copied());
                if !required && owner.is_some() { return Err(InferenceError::InvalidScheme); }
                receipts.push(PatternNominalReceipt { root, identity: owner, required });
            }
            self.graph.charge_source_fact_edges(1 + receipts.iter().map(|receipt| u64::from(receipt.root.is_some()) + u64::from(receipt.identity.is_some())).sum::<u64>())?;
            self.original_pattern_nominals.insert(identity, (receipts[0], receipts[1]));
        }
        Ok(())
    }

    pub(in crate::sema::check) fn pattern_authority_is_published(&self, identity: PatternIdentity) -> bool {
        self.original_patterns.contains_key(&identity)
    }

    pub(in crate::sema::check) fn publish_pattern_authority(&mut self, identity: PatternIdentity) -> Result<(), InferenceError> {
        let pattern = self.patterns.get(&identity).ok_or(InferenceError::InvalidScheme)?;
        if let Some((original, value_scope)) = self.original_patterns.get(&identity) {
            return if original.as_ref() == pattern.as_ref() && self.pattern_value_scopes.get(&identity).copied() == *value_scope { Ok(()) } else { Err(InferenceError::InvalidScheme) };
        }
        self.graph.charge_source_fact_nodes(1)?;
        self.graph.charge_source_fact_edges(1)?;
        self.original_patterns.insert(identity, (std::sync::Arc::clone(pattern), None));
        Ok(())
    }

    pub(in crate::sema::check) fn publish_pattern_value_scope(&mut self, identity: PatternIdentity, scope: SchemeId) -> Result<bool, InferenceError> {
        let Some((original, value_scope)) = self.original_patterns.get(&identity) else { return Ok(false); };
        let current = self.patterns.get(&identity).ok_or(InferenceError::InvalidScheme)?;
        if !std::sync::Arc::ptr_eq(original, current) && original.as_ref() != current.as_ref() { return Err(InferenceError::InvalidScheme); }
        if let Some(previous) = value_scope {
            return if *previous == scope && self.pattern_value_scopes.get(&identity) == Some(previous) { Ok(false) } else { Err(InferenceError::InvalidScheme) };
        }
        if self.pattern_value_scopes.contains_key(&identity) { return Err(InferenceError::InvalidScheme); }
        self.graph.scheme(scope)?;
        self.graph.charge_source_fact_nodes(1)?;
        self.graph.charge_source_fact_edges(2)?;
        self.original_patterns.get_mut(&identity).unwrap().1 = Some(scope);
        self.pattern_value_scopes.insert(identity, scope);
        Ok(true)
    }

    pub(in crate::sema::check) fn remove_pattern_authority(&mut self, identity: PatternIdentity) {
        self.patterns.remove(&identity);
        self.original_patterns.remove(&identity);
        self.original_pattern_nominals.remove(&identity);
        self.pattern_value_scopes.remove(&identity);
    }
}

fn capture_types_match(graph: &InferenceContext, left: TypeId, right: TypeId, compared: &mut BTreeSet<(TypeId, TypeId)>) -> Result<bool, InferenceError> {
    use crate::sema::inference::TypeNode;
    let mut pending = vec![(left, right, 0usize)];
    while let Some((left, right, depth)) = pending.pop() {
        if depth > graph.limits().structural_depth || compared.len() as u64 >= graph.limits().work_units { return Err(InferenceError::Limit("pattern capture comparison")); }
        let left = graph.resolved(left)?;
        let right = graph.resolved(right)?;
        if left == right || !compared.insert((left, right)) { continue; }
        let mut pair = |left, right| pending.push((left, right, depth + 1));
        match (graph.node(left)?, graph.node(right)?) {
            (TypeNode::Atom(left), TypeNode::Atom(right)) if left == right => {},
            (TypeNode::Rigid { scope: left, index: li, kind: lk }, TypeNode::Rigid { scope: right, index: ri, kind: rk }) if left == right && li == ri && lk == rk => {},
            (TypeNode::List(left), TypeNode::List(right)) | (TypeNode::Stream(left), TypeNode::Stream(right))
                | (TypeNode::Optional(left), TypeNode::Optional(right)) => pair(*left, *right),
            (TypeNode::Map(lk, lv), TypeNode::Map(rk, rv)) | (TypeNode::Result(lk, lv), TypeNode::Result(rk, rv)) => { pair(*lk, *rk); pair(*lv, *rv); },
            (TypeNode::Record(left), TypeNode::Record(right)) | (TypeNode::Row(left), TypeNode::Row(right)) => {
                let left = graph.row_data(*left)?;
                let right = graph.row_data(*right)?;
                if left.fields.len() != right.fields.len() || left.tail.is_some() != right.tail.is_some() { return Ok(false); }
                for (left, right) in left.fields.iter().zip(&right.fields) {
                    if left.label != right.label { return Ok(false); }
                    pair(left.ty, right.ty);
                }
                if let (Some(left), Some(right)) = (left.tail, right.tail) { pair(left, right); }
            },
            (TypeNode::Arrow(left), TypeNode::Arrow(right)) => {
                if left.kind != right.kind || left.params.len() != right.params.len() || graph.closed_effect_summary(left.effects)? != graph.closed_effect_summary(right.effects)? { return Ok(false); }
                for (left, right) in left.params.iter().zip(&right.params) {
                    if left.label != right.label || left.defaulted != right.defaulted || left.rest != right.rest { return Ok(false); }
                    pair(left.ty, right.ty);
                }
                pair(left.result, right.result);
            },
            (TypeNode::CallableChoice(left), TypeNode::CallableChoice(right)) if left.len() == right.len() => {
                for (&left, &right) in left.iter().zip(right) { pair(left, right); }
            },
            (TypeNode::NativeCallable(left), TypeNode::NativeCallable(right)) if left.alternatives == right.alternatives => pair(left.signature, right.signature),
            _ => return Ok(false),
        }
    }
    Ok(true)
}
