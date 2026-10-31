use super::{Binding, ConstraintChange, ConstraintConflict, ConstraintResolutionError, RowTail, TypeConstraints, Variable, MAX_TYPE_DEPTH};
use super::application_constraints::ApplicationDependency;
use crate::sema::sealed_operation::{ArithmeticDomain, ArithmeticSignature, SealedOperation, SealedOperationSignature, index_selection, slice_selection, record_override_selection, arithmetic_domain_accepts, arithmetic_result, arithmetic_selection};
use crate::sema::types::Type;
use crate::source::Span;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_OPERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct SealedOperationId(NonZeroU64);

#[derive(Clone, Debug)]
pub(crate) enum SealedOperationError {
    Resolution(ConstraintResolutionError),
    Constraint(ConstraintConflict),
    MapKey(super::MapKeyConstraintError),
    UnsupportedTypePredicate { domain: crate::sema::sealed_operation::TypeDomain, subject: Type },
    UnsupportedIndex { receiver: Type, key: Type },
    UnsupportedMembership { container: Type, needle: Type },
    UnsupportedSlice { receiver: Type },
    UnsupportedRecordSpread { receiver: Type },
    Unsupported { operator: crate::sema::sealed_operation::ArithmeticOperator, left: Type, right: Type },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::sealed_operation::SealedBinaryOperation;

    #[test]
    fn checked_unsigned_contexts_distinguish_arithmetic_results_from_operands() {
        let span = Span::at(crate::source::SourceId::new(0), 0);
        let mut constraints = TypeConstraints::default();
        let left = constraints.fresh(span);
        let result = constraints.fresh(span);
        constraints.register_sealed_operation(super::super::CallableApplicationScope::Invocation, SealedBinaryOperation { operator: crate::sema::sealed_operation::ArithmeticOperator::Divide, left: left.clone(), right: Type::Int, result: result.clone(), span }).unwrap();
        constraints.constrain_context(&Type::UInt, &result, span).unwrap();
        assert_eq!(constraints.resolve(&result).unwrap(), Type::Int);
        assert_eq!(constraints.resolve(&left).unwrap(), left);
        constraints.constrain_context(&Type::UInt, &left, span).unwrap();
        assert_eq!(constraints.resolve(&left).unwrap(), Type::UInt);
        let unrelated = constraints.fresh(span);
        constraints.constrain_context(&Type::UInt, &unrelated, span).unwrap();
        assert_eq!(constraints.resolve(&unrelated).unwrap(), Type::UInt);
        constraints.solve_sealed_operations().unwrap();
        assert_eq!(constraints.resolve(&result).unwrap(), Type::Int);
    }

    #[test]
    fn arithmetic_result_contexts_follow_unions_and_rollback_watches() {
        let span = Span::at(crate::source::SourceId::new(0), 0);
        let mut constraints = TypeConstraints::default();
        let alias = constraints.fresh(span);
        let result = constraints.fresh(span);
        let left = constraints.fresh(span);
        let id = constraints.register_sealed_operation(super::super::CallableApplicationScope::Invocation, SealedBinaryOperation { operator: crate::sema::sealed_operation::ArithmeticOperator::Divide, left: left.clone(), right: Type::Int, result: result.clone(), span }).unwrap();
        constraints.solve_sealed_operations().unwrap();
        assert!(!constraints.operations.queued.contains(&id));
        let before = constraints.operations.clone();
        assert!(!constraints.trial(|constraints| {
            constraints.constrain(&alias, &result, span)?;
            constraints.constrain_context(&Type::UInt, &alias, span)?;
            assert_eq!(constraints.resolve(&result).unwrap(), Type::Int);
            constraints.constrain(&Type::Bool, &Type::Str, span)
        }).unwrap());
        assert_eq!(constraints.operations, before);
        assert_eq!(constraints.resolve(&alias).unwrap(), alias);
        assert_eq!(constraints.resolve(&result).unwrap(), result);
        constraints.constrain(&alias, &result, span).unwrap();
        let Type::Inference(alias_id) = alias else { unreachable!() };
        let Type::Inference(result_id) = result else { unreachable!() };
        assert_eq!(constraints.representative_root(alias_id).unwrap(), alias_id);
        assert_eq!(constraints.representative_root(result_id).unwrap(), alias_id);
        assert!(constraints.operations.watchers[&ApplicationDependency::Type(alias_id)].contains(&id));
        constraints.constrain_context(&Type::UInt, &alias, span).unwrap();
        assert_eq!(constraints.resolve(&result).unwrap(), Type::Int);
        assert!(constraints.operations.queued.contains(&id));
        constraints.constrain(&left, &Type::Int, span).unwrap();
        constraints.solve_sealed_operations().unwrap();
        assert_eq!(constraints.operations.entries[&id].selection, Some(SealedOperationSignature::Binary(ArithmeticSignature::Integer)));
    }

    #[test]
    fn nested_record_contexts_keep_arithmetic_results_intrinsic() {
        crate::symbol::SymbolOwner::new().with_current(|| {
        let span = Span::at(crate::source::SourceId::new(0), 0);
        let mut constraints = TypeConstraints::default();
        let left = constraints.fresh(span);
        let result = constraints.fresh(span);
        constraints.register_sealed_operation(super::super::CallableApplicationScope::Invocation, SealedBinaryOperation { operator: crate::sema::sealed_operation::ArithmeticOperator::Divide, left, right: Type::Int, result: result.clone(), span }).unwrap();
        let field = crate::symbol::Name::intern("item");
        let nested = |ty| Type::Record(BTreeMap::from([(field, Type::Record(BTreeMap::from([(field, ty)]).into()))]).into());
        constraints.constrain_context(&nested(Type::UInt), &nested(result.clone()), span).unwrap();
        assert_eq!(constraints.resolve(&result).unwrap(), Type::Int);
        });
    }

    #[test]
    fn sealed_postfix_operations_preserve_input_ambiguity_and_atomic_rollback() {
        let span = Span::at(crate::source::SourceId::new(0), 0);
        let owner = super::super::CallableApplicationScope::Invocation;
        let mut constraints = TypeConstraints::default();
        let receiver = constraints.fresh(span);
        constraints.register_sealed_operation(owner, SealedOperation::Index { receiver: receiver.clone(), key: Type::Int, result: Type::Str, syntax: crate::sema::sealed_operation::IndexSyntax::Ordinary, constant_key: None, span }).unwrap();
        constraints.solve_sealed_operations().unwrap();
        assert_eq!(constraints.resolve(&receiver).unwrap(), receiver);
        constraints.constrain(&receiver, &Type::Map(Box::new(Type::Int), Box::new(Type::Str)), span).unwrap();
        constraints.solve_sealed_operations().unwrap();

        let mut constraints = TypeConstraints::default();
        let key = constraints.fresh(span);
        constraints.register_sealed_operation(owner, SealedOperation::Index { receiver: Type::List(Box::new(Type::Int)), key: key.clone(), result: Type::Bool, syntax: crate::sema::sealed_operation::IndexSyntax::Ordinary, constant_key: None, span }).unwrap();
        let before = constraints.operations.clone();
        assert!(constraints.solve_sealed_operations().is_err());
        assert_eq!(constraints.operations, before);
        assert_eq!(constraints.resolve(&key).unwrap(), key);

        let mut constraints = TypeConstraints::default();
        let receiver = constraints.fresh(span);
        constraints.register_sealed_operation(owner, SealedOperation::Slice { receiver: receiver.clone(), result: Type::Str, bounds: crate::sema::sealed_operation::SliceBounds::Unbounded, span }).unwrap();
        constraints.solve_sealed_operations().unwrap();
        assert_eq!(constraints.resolve(&receiver).unwrap(), receiver);
    }

    #[test]
    fn sealed_postfix_record_projection_preserves_rollback_and_stable_wakeups() {
        let span = Span::at(crate::source::SourceId::new(0), 0);
        let owner = super::super::CallableApplicationScope::Invocation;
        let mut constraints = TypeConstraints::default();
        let receiver = constraints.fresh(span);
        let name = crate::symbol::Name::intern("name");
        let item = crate::symbol::Name::intern("item");
        constraints.project_record_field(&receiver, name, 1, span).unwrap();
        let result = constraints.fresh(span);
        constraints.register_sealed_operation(owner, SealedOperation::Index { receiver: receiver.clone(), key: Type::Str, result: result.clone(), syntax: crate::sema::sealed_operation::IndexSyntax::Ordinary, constant_key: Some(item), span }).unwrap();
        let operations = constraints.operations.clone();
        let fields = constraints.contextual_record_fields(&receiver).unwrap();
        let rows = constraints.rows.len();
        let variables = constraints.variables.len();
        constraints.trial(|trial| {
            trial.solve_sealed_operations().unwrap();
            assert!(trial.contextual_record_fields(&receiver).unwrap().unwrap().contains_key(&item));
            Ok(())
        }).unwrap();
        assert_eq!(constraints.operations, operations);
        assert_eq!(constraints.contextual_record_fields(&receiver).unwrap(), fields);
        assert_eq!(constraints.rows.len(), rows);
        assert_eq!(constraints.variables.len(), variables);
        constraints.solve_sealed_operations().unwrap();
        let fields = constraints.contextual_record_fields(&receiver).unwrap().unwrap();
        assert_eq!(fields.get(&item), Some(&result));
        let rows = constraints.rows.len();
        constraints.solve_sealed_operations().unwrap();
        assert_eq!(constraints.rows.len(), rows);
    }

    #[test]
    fn sealed_record_overrides_keep_distinct_remainders_and_atomic_rollback() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let span = Span::at(crate::source::SourceId::new(0), 0);
            let name = crate::symbol::Name::intern("name");
            let index = crate::symbol::Name::intern("index");
            let extra = crate::symbol::Name::intern("extra");
            let mut constraints = TypeConstraints::default();
            let source = constraints.fresh_at_level(span, 1);
            constraints.project_record_field(&source, name, 1, span).unwrap();
            let result = constraints.fresh_at_level(span, 1);
            constraints.register_sealed_operation(super::super::CallableApplicationScope::Invocation, SealedOperation::RecordOverride {
                receiver: source.clone(), result: result.clone(), span,
                overrides: vec![crate::sema::sealed_operation::RecordOverrideField { name, ty: Type::Str }, crate::sema::sealed_operation::RecordOverrideField { name: index, ty: Type::Bool }].into(),
            }).unwrap();
            let operations = constraints.operations.clone();
            let rows = constraints.rows.len();
            constraints.trial(|candidate| {
                candidate.solve_sealed_operations().unwrap();
                let super::super::ConstraintType::Record(input) = candidate.resolve_constraint(&super::super::ConstraintType::Ground(source.clone())).unwrap() else { panic!("input remains an open record") };
                let super::super::ConstraintType::Record(output) = candidate.resolve_constraint(&super::super::ConstraintType::Ground(result.clone())).unwrap() else { panic!("output retains a separate open record") };
                assert_ne!(input.tail, output.tail);
                let RowTail::Variable(input) = input.tail else { unreachable!() };
                let RowTail::Variable(output) = output.tail else { unreachable!() };
                let super::super::RowVariable::Representative(input) = &candidate.rows[&input] else { unreachable!() };
                let super::super::RowVariable::Representative(output) = &candidate.rows[&output] else { unreachable!() };
                assert!(!input.lacks.contains(&index));
                assert!(output.lacks.contains(&index));
                Ok(())
            }).unwrap();
            assert_eq!(constraints.operations, operations);
            assert_eq!(constraints.rows.len(), rows);
            assert_eq!(constraints.resolve(&result).unwrap(), result);
            constraints.solve_sealed_operations().unwrap();
            let rows = constraints.rows.len();
            constraints.solve_sealed_operations().unwrap();
            assert_eq!(constraints.rows.len(), rows);
            constraints.constrain(&source, &Type::Record(BTreeMap::from([(name, Type::Int), (index, Type::Str), (extra, Type::UInt)]).into()), span).unwrap();
            constraints.solve_sealed_operations().unwrap();
            assert_eq!(constraints.resolve(&result).unwrap(), Type::Record(BTreeMap::from([(name, Type::Str), (index, Type::Bool), (extra, Type::UInt)]).into()));
        });
    }

    #[test]
    fn sealed_record_override_census_counts_owned_field_allocations() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let span = Span::at(crate::source::SourceId::new(0), 0);
            let owner = super::super::CallableApplicationScope::Invocation;
            let mut baseline = TypeConstraints::default();
            let receiver = baseline.fresh(span);
            let result = baseline.fresh(span);
            baseline.register_sealed_operation(owner, SealedOperation::Slice { receiver, result, bounds: crate::sema::sealed_operation::SliceBounds::Unbounded, span }).unwrap();
            let baseline = baseline.operations.retained_bytes();
            for count in [1, 16] {
                let mut constraints = TypeConstraints::default();
                let receiver = constraints.fresh(span);
                let result = constraints.fresh(span);
                constraints.register_sealed_operation(owner, SealedOperation::RecordOverride {
                    receiver, result, span,
                    overrides: (0..count).map(|_| crate::sema::sealed_operation::RecordOverrideField { name: crate::symbol::Name::intern("index"), ty: Type::Int }).collect::<Vec<_>>().into(),
                }).unwrap();
                assert_eq!(constraints.operations.retained_bytes() - baseline, 2 * std::mem::size_of::<usize>() + count * std::mem::size_of::<crate::sema::sealed_operation::RecordOverrideField<Type>>());
            }
        });
    }

    #[test]
    fn sealed_binary_operations_reject_recursive_list_results_without_binding_the_result() {
        let span = Span::at(crate::source::SourceId::new(0), 0);
        let mut constraints = TypeConstraints::default();
        let result = constraints.fresh_at_level(span, 1);
        let items = Type::List(Box::new(result.clone()));
        constraints.register_sealed_operation(super::super::CallableApplicationScope::Invocation, SealedBinaryOperation { operator: crate::sema::sealed_operation::ArithmeticOperator::Add, left: items.clone(), right: items, result: result.clone(), span }).unwrap();
        assert!(constraints.solve_sealed_operations().is_err());
        assert_eq!(constraints.resolve(&result).unwrap(), result);
    }

    #[test]
    fn sealed_binary_operations_preserve_ambiguity_descendant_wakeups_and_atomic_rollback() {
        let span = Span::at(crate::source::SourceId::new(0), 0);
        let owner = super::super::CallableApplicationScope::Invocation;
        let mut constraints = TypeConstraints::default();
        let left = constraints.fresh(span);
        let right = constraints.fresh(span);
        constraints.register_sealed_operation(owner, SealedBinaryOperation { operator: crate::sema::sealed_operation::ArithmeticOperator::Add, left: left.clone(), right: right.clone(), result: Type::Int, span }).unwrap();
        constraints.solve_sealed_operations().unwrap();
        assert_eq!(constraints.resolve(&left).unwrap(), left);
        assert_eq!(constraints.resolve(&right).unwrap(), right);
        constraints.constrain(&left, &Type::Str, span).unwrap();
        let before = constraints.operations.clone();
        assert!(constraints.solve_sealed_operations().is_err());
        assert_eq!(constraints.operations, before);
        assert_eq!(constraints.resolve(&right).unwrap(), right);

        let mut constraints = TypeConstraints::default();
        let source = constraints.fresh(span);
        constraints.register_sealed_operation(owner, SealedBinaryOperation { operator: crate::sema::sealed_operation::ArithmeticOperator::Subtract, left: source.clone(), right: Type::Set(Box::new(Type::Int)), result: Type::Bool, span }).unwrap();
        let operations = constraints.operations.clone();
        let variables = constraints.variables.len();
        assert!(constraints.solve_sealed_operations().is_err());
        assert_eq!(constraints.operations, operations);
        assert_eq!(constraints.variables.len(), variables);
        assert_eq!(constraints.resolve(&source).unwrap(), source);

        let mut constraints = TypeConstraints::default();
        let item = constraints.fresh(span);
        let result = constraints.fresh(span);
        let list = Type::List(Box::new(item.clone()));
        let id = constraints.register_sealed_operation(owner, SealedBinaryOperation { operator: crate::sema::sealed_operation::ArithmeticOperator::Add, left: list.clone(), right: list, result: result.clone(), span }).unwrap();
        constraints.solve_sealed_operations().unwrap();
        assert_eq!(constraints.operations.entries[&id].selection, None);
        constraints.constrain(&item, &Type::Bool, span).unwrap();
        constraints.solve_sealed_operations().unwrap();
        assert_eq!(constraints.resolve(&result).unwrap(), Type::List(Box::new(Type::Bool)));
        assert_eq!(constraints.operations.entries[&id].selection, Some(SealedOperationSignature::Binary(ArithmeticSignature::ListConcatenation)));
    }
}

impl From<ConstraintResolutionError> for SealedOperationError {
    fn from(error: ConstraintResolutionError) -> Self { Self::Resolution(error) }
}

impl std::fmt::Display for SealedOperationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resolution(error) => write!(formatter, "operation type resolution failed: {error:?}"),
            Self::Constraint(error) => write!(formatter, "operation requires {}, found {}", error.expected, error.actual),
            Self::MapKey(error) => write!(formatter, "operation Set item requires MapKey: {error:?}"),
            Self::UnsupportedTypePredicate { domain, subject } => write!(formatter, "{domain:?} does not support {subject}"),
            Self::UnsupportedMembership { container, needle } => write!(formatter, "membership is not defined for {needle} in {container}"),
            Self::UnsupportedIndex { receiver, key } => write!(formatter, "indexing is not defined for {receiver} with key {:?}", Some(key)),
            Self::UnsupportedSlice { receiver } => write!(formatter, "slicing is not defined for {receiver} with key None"),
            Self::UnsupportedRecordSpread { receiver } => write!(formatter, "record spread is not defined for {receiver} with key None"),
            Self::Unsupported { operator, left, right } => write!(formatter, "{operator:?} is not defined for {left} and {right}"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ScopedOperation {
    pub(super) owner: super::CallableApplicationScope,
    pub(super) operation: SealedOperation<Type>,
    pub(super) selection: Option<SealedOperationSignature>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct SealedOperations {
    pub(super) entries: BTreeMap<SealedOperationId, ScopedOperation>,
    source_sites: BTreeMap<(super::CallableApplicationScope, Span, Option<crate::sema::sealed_operation::TypeDomain>), SealedOperationId>,
    watchers: BTreeMap<ApplicationDependency, BTreeSet<SealedOperationId>>,
    pending: VecDeque<SealedOperationId>,
    queued: BTreeSet<SealedOperationId>,
}

#[derive(Clone, Debug)]
pub(super) enum OperationChange {
    Fresh(SealedOperationId),
    Source((super::CallableApplicationScope, Span, Option<crate::sema::sealed_operation::TypeDomain>), SealedOperationId),
    Selection(SealedOperationId, Option<SealedOperationSignature>),
    Watcher(ApplicationDependency, SealedOperationId),
    Queued(SealedOperationId),
    Dequeued(SealedOperationId),
}

impl SealedOperations {
    #[cfg(test)]
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.entries.len() * size_of::<(SealedOperationId, ScopedOperation)>()
            + self.entries.values().flat_map(|entry| entry.operation.types()).map(super::type_owned_payload_bytes).sum::<usize>()
            + self.entries.values().map(|entry| entry.operation.owned_fields_bytes()).sum::<usize>()
            + self.source_sites.len() * size_of::<((super::CallableApplicationScope, Span, Option<crate::sema::sealed_operation::TypeDomain>), SealedOperationId)>()
            + self.watchers.len() * size_of::<(ApplicationDependency, BTreeSet<SealedOperationId>)>()
            + self.watchers.values().map(|ids| ids.len() * size_of::<SealedOperationId>()).sum::<usize>()
            + self.pending.capacity() * size_of::<SealedOperationId>()
            + self.queued.len() * size_of::<SealedOperationId>()
    }
    pub(super) fn rollback(&mut self, change: OperationChange) {
        match change {
            OperationChange::Fresh(id) => { self.entries.remove(&id).expect("operation belongs to its journal"); }
            OperationChange::Source(site, id) => { assert_eq!(self.source_sites.remove(&site), Some(id)); }
            OperationChange::Selection(id, selection) => { self.entries.get_mut(&id).expect("operation belongs to its journal").selection = selection; }
            OperationChange::Watcher(dependency, id) => {
                let watchers = self.watchers.get_mut(&dependency).expect("operation watcher belongs to its journal");
                assert!(watchers.remove(&id));
                if watchers.is_empty() { self.watchers.remove(&dependency); }
            }
            OperationChange::Queued(id) => { assert_eq!(self.pending.pop_back(), Some(id)); assert!(self.queued.remove(&id)); }
            OperationChange::Dequeued(id) => { self.pending.push_front(id); assert!(self.queued.insert(id)); }
        }
    }

    fn enqueue(&mut self, id: SealedOperationId, changes: &mut Vec<ConstraintChange>) {
        if self.queued.insert(id) {
            self.pending.push_back(id);
            changes.push(ConstraintChange::Operation(OperationChange::Queued(id)));
        }
    }

    fn watch(&mut self, dependency: ApplicationDependency, id: SealedOperationId, changes: &mut Vec<ConstraintChange>) {
        if self.watchers.entry(dependency).or_default().insert(id) { changes.push(ConstraintChange::Operation(OperationChange::Watcher(dependency, id))); }
    }

    pub(super) fn inherit_type_watchers(&mut self, child: super::TypeVariableId, root: super::TypeVariableId, changes: &mut Vec<ConstraintChange>) {
        let inherited = self.watchers.get(&ApplicationDependency::Type(child)).into_iter().flatten().copied().collect::<Vec<_>>();
        for id in inherited { self.watch(ApplicationDependency::Type(root), id, changes); }
    }

    pub(super) fn enqueue_changes(&mut self, from: usize, changes: &mut Vec<ConstraintChange>) {
        let dependencies = changes[from..].iter().filter_map(|change| match change {
            ConstraintChange::Type(id, _) | ConstraintChange::FreshType(id) => Some(ApplicationDependency::Type(*id)),
            ConstraintChange::Row(id, _) => Some(ApplicationDependency::Row(*id)),
            _ => None,
        }).collect::<BTreeSet<_>>();
        for dependency in dependencies {
            for id in self.watchers.get(&dependency).into_iter().flatten().copied().collect::<Vec<_>>() { self.enqueue(id, changes); }
        }
    }

    pub(super) fn is_pending(&self) -> bool { !self.pending.is_empty() }
}

impl TypeConstraints {
    pub(super) fn is_unresolved_slice_result(&self, variable: super::TypeVariableId) -> Result<bool, ConstraintResolutionError> {
        let root = self.representative_root(variable)?;
        let Some(Variable::Representative(actual)) = self.variables.get(&root) else { unreachable!() };
        if actual.binding.is_some() { return Ok(false); }
        for id in self.operations.watchers.get(&ApplicationDependency::Type(root)).into_iter().flatten() {
            let entry = self.operations.entries.get(id).ok_or(ConstraintResolutionError::ForeignVariable)?;
            let SealedOperation::Slice { result: Type::Inference(result), .. } = &entry.operation else { continue; };
            if entry.selection.is_none() && self.representative_root(*result)? == root { return Ok(true); }
        }
        Ok(false)
    }

    pub(super) fn is_unresolved_arithmetic_result(&self, variable: super::TypeVariableId) -> Result<bool, ConstraintResolutionError> {
        let root = self.representative_root(variable)?;
        let Some(Variable::Representative(actual)) = self.variables.get(&root) else { unreachable!() };
        if actual.binding.is_some() { return Ok(false); }
        for id in self.operations.watchers.get(&ApplicationDependency::Type(root)).into_iter().flatten() {
            let entry = self.operations.entries.get(id).ok_or(ConstraintResolutionError::ForeignVariable)?;
            let SealedOperation::Binary(operation) = &entry.operation else { continue; };
            if entry.selection.is_some() { continue; }
            if let Type::Inference(result) = &operation.result
                && self.representative_root(*result)? == root
            { return Ok(true); }
        }
        Ok(false)
    }

    pub(crate) fn require_type_predicate(&mut self, owner: super::CallableApplicationScope, domain: crate::sema::sealed_operation::TypeDomain, subject: &Type, span: Span) -> Result<(), SealedOperationError> {
        self.commit_constraint_transaction(|constraints| {
            constraints.register_source_sealed_operation(owner, SealedOperation::TypePredicate { domain, subject: subject.clone(), span })?;
            constraints.solve_sealed_operations()
        })
    }

    fn operation_dependencies(&self, operation: &SealedOperation<Type>) -> Result<BTreeSet<ApplicationDependency>, ConstraintResolutionError> {
        enum Pending<'a> { Type(&'a Type, usize), Row(RowTail, usize) }
        let mut pending = operation.types().map(|ty| Pending::Type(ty, 0)).collect::<Vec<_>>();
        let mut dependencies = BTreeSet::new();
        while let Some(node) = pending.pop() {
            match node {
                Pending::Type(ty, depth) => {
                    if depth > MAX_TYPE_DEPTH { return Err(ConstraintResolutionError::TypeDepth); }
                    if let Type::Inference(id) = ty {
                        let root = self.representative_root(*id)?;
                        if !dependencies.insert(ApplicationDependency::Type(root)) { continue; }
                        let Some(Variable::Representative(variable)) = self.variables.get(&root) else { unreachable!() };
                        match &variable.binding {
                            Some(Binding::Type { ty, .. }) => pending.push(Pending::Type(ty, depth)),
                            Some(Binding::Record(record)) => {
                                pending.extend(record.shape.fields.values().map(|ty| Pending::Type(ty, depth + 1)));
                                pending.push(Pending::Row(record.shape.tail.clone(), depth));
                            }
                            #[cfg(test)]
                            Some(Binding::Rigid { .. }) => {},
                            None => {},
                        }
                    } else {
                        ty.for_each_child(|child| pending.push(Pending::Type(child, depth + 1)));
                    }
                }
                Pending::Row(tail, depth) => {
                    if depth > MAX_TYPE_DEPTH { return Err(ConstraintResolutionError::TypeDepth); }
                    let RowTail::Variable(id) = tail else { continue; };
                    let root = self.row_representative_root(id)?;
                    if !dependencies.insert(ApplicationDependency::Row(root)) { continue; }
                    let Some(super::RowVariable::Representative(variable)) = self.rows.get(&root) else { unreachable!() };
                    if let Some(binding) = &variable.binding {
                        pending.extend(binding.shape.fields.values().map(|ty| Pending::Type(ty, depth + 1)));
                        pending.push(Pending::Row(binding.shape.tail.clone(), depth));
                    }
                }
            }
        }
        Ok(dependencies)
    }

    pub(crate) fn register_sealed_operation(&mut self, owner: super::CallableApplicationScope, operation: impl Into<SealedOperation<Type>>) -> Result<SealedOperationId, SealedOperationError> {
        let operation = operation.into();
        let dependencies = self.operation_dependencies(&operation)?;
        let id = SealedOperationId(NonZeroU64::new(NEXT_OPERATION.fetch_add(1, Ordering::Relaxed)).expect("operation identities are nonzero"));
        self.apply_journaled(|this, changes| {
            assert!(this.operations.entries.insert(id, ScopedOperation { owner, operation, selection: None }).is_none());
            changes.push(ConstraintChange::Operation(OperationChange::Fresh(id)));
            for dependency in dependencies { this.operations.watch(dependency, id, changes); }
            this.operations.enqueue(id, changes);
            Ok::<_, SealedOperationError>(())
        })?;
        Ok(id)
    }

    pub(crate) fn solve_sealed_operations(&mut self) -> Result<(), SealedOperationError> {
        self.commit_constraint_transaction(|this| {
            while this.operations.is_pending() { this.solve_pending_sealed_operations()?; }
            Ok(())
        })
    }

    fn constrain_operation_type(&mut self, expected: &Type, actual: &Type, span: Span, changes: &mut Vec<ConstraintChange>) -> Result<(), SealedOperationError> {
        self.constrain_inner(expected, actual, span, false, 0, changes).map_err(|(expected, actual, resolution_error)| SealedOperationError::Constraint(ConstraintConflict { expected, actual, resolution_error, contribution: span, initializer: None, established: None }))
    }

    fn solve_pending_sealed_operations(&mut self) -> Result<(), SealedOperationError> {
        self.apply_journaled(|this, changes| {
            while let Some(id) = this.operations.pending.pop_front() {
                assert!(this.operations.queued.remove(&id));
                changes.push(ConstraintChange::Operation(OperationChange::Dequeued(id)));
                let operation = this.operations.entries.get(&id).expect("queued operation belongs to its owner").operation.clone();
                let dependencies = this.operation_dependencies(&operation)?;
                let level = dependencies.iter().filter_map(|dependency| match dependency {
                    ApplicationDependency::Type(id) => Some(this.type_level(*id)),
                    ApplicationDependency::Row(_) => None,
                }).collect::<Result<Vec<_>, _>>()?.into_iter().min();
                if let Some(level) = level {
                    for ty in operation.types() { this.lower_levels_in_type(ty, level, changes)?; }
                }
                for dependency in dependencies { this.operations.watch(dependency, id, changes); }
                let operation = match operation {
                    SealedOperation::TypePredicate { domain, subject: key, .. } => {
                        let key = this.resolve_with_changes(&key, 0, changes)?;
                        if !domain.may_accept(&key) { return Err(SealedOperationError::UnsupportedTypePredicate { domain, subject: key }); }
                        if let Type::Inference(variable) = key.unvalidated() {
                            if let Some(record) = this.record_binding(*variable)? {
                                let fields = this.resolve_row(&record.shape)?.fields;
                                match domain {
                                    crate::sema::sealed_operation::TypeDomain::SortKey => {
                                        for ty in fields.values() {
                                            if !domain.may_accept(ty) { return Err(SealedOperationError::UnsupportedTypePredicate { domain, subject: ty.clone() }); }
                                        }
                                    }
                                    crate::sema::sealed_operation::TypeDomain::CollectItem | crate::sema::sealed_operation::TypeDomain::MinMaxItem => {},
                                    crate::sema::sealed_operation::TypeDomain::CountKey | crate::sema::sealed_operation::TypeDomain::OrderingOperand | crate::sema::sealed_operation::TypeDomain::NegationOperand => return Err(SealedOperationError::UnsupportedTypePredicate { domain, subject: key }),
                                }
                            }
                            continue;
                        }
                        if key.contains_inference() { continue; }
                        if !domain.accepts(&key) { return Err(SealedOperationError::UnsupportedTypePredicate { domain, subject: key }); }
                        let selection = SealedOperationSignature::TypePredicate(domain);
                        let previous = this.operations.entries.get_mut(&id).unwrap().selection.replace(selection);
                        if previous != Some(selection) { changes.push(ConstraintChange::Operation(OperationChange::Selection(id, previous))); }
                        continue;
                    }
                    SealedOperation::Membership { needle, container, span } => {
                        let container = this.resolve_with_changes(&container, 0, changes)?;
                        if matches!(container.unvalidated(), Type::Inference(_)) { continue; }
                        let needle = this.resolve_with_changes(&needle, 0, changes)?;
                        use crate::sema::types::MembershipCheck;
                        match container.membership_needle_type(&needle) {
                            MembershipCheck::Expected(required) | MembershipCheck::Compared(required) => this.constrain_operation_type(required, &needle, span, changes)?,
                            _ => return Err(SealedOperationError::UnsupportedMembership { container, needle }),
                        }
                        let needle = this.resolve_with_changes(&needle, 0, changes)?;
                        if needle.contains_inference() || container.contains_inference() { continue; }
                        let selected = crate::sema::sealed_operation::membership_selection(&container, &needle)
                            .ok_or_else(|| SealedOperationError::UnsupportedMembership { container, needle })?;
                        let selected = SealedOperationSignature::Membership(selected);
                        let previous = this.operations.entries.get_mut(&id).unwrap().selection.replace(selected);
                        if previous != Some(selected) { changes.push(ConstraintChange::Operation(OperationChange::Selection(id, previous))); }
                        continue;
                    }
                    SealedOperation::Binary(operation) => operation,
                    SealedOperation::Index { receiver, key, result, constant_key, span, .. } => {
                        let receiver = this.resolve_with_changes(&receiver, 0, changes)?;
                        if let Type::Inference(variable) = receiver.unvalidated() {
                            if let Some(field) = constant_key
                                && let Some(record) = this.record_binding(*variable)? {
                                let fields = this.resolve_row(&record.shape)?.fields;
                                this.constrain_operation_type(&Type::Str, &key, span, changes)?;
                                if let Some(selected) = fields.get(&field) {
                                    this.constrain_operation_type(&result, selected, span, changes)?;
                                } else {
                                    this.project_record_field_with_changes(&receiver, field, &result, level.unwrap_or(0), span, changes).map_err(SealedOperationError::Constraint)?;
                                }
                            }
                            continue;
                        }
                        let required = match receiver.unvalidated() {
                            Type::List(_) => Type::Int,
                            Type::Map(key, _) => (**key).clone(),
                            Type::Record(_) | Type::Module(_) if constant_key.is_some() => Type::Str,
                            _ => return Err(SealedOperationError::UnsupportedIndex { receiver, key }),
                        };
                        if !crate::sema::sealed_operation::requires_uint_key_validation(&receiver, &key) {
                            this.constrain_operation_type(&required, &key, span, changes)?;
                        }
                        let key = this.resolve_with_changes(&key, 0, changes)?;
                        let Some((selection, selected)) = index_selection(&receiver, &key, constant_key) else {
                            return Err(SealedOperationError::UnsupportedIndex { receiver, key });
                        };
                        this.constrain_operation_type(&result, &selected, span, changes)?;
                        if receiver.contains_inference() || key.contains_inference() { continue; }
                        let previous = this.operations.entries.get_mut(&id).unwrap().selection.replace(selection);
                        if previous != Some(selection) { changes.push(ConstraintChange::Operation(OperationChange::Selection(id, previous))); }
                        continue;
                    }
                    SealedOperation::RecordOverride { receiver, overrides, result, span } => {
                        let receiver = this.resolve_with_changes(&receiver, 0, changes)?;
                        let overrides = overrides.iter().map(|field| Ok(crate::sema::sealed_operation::RecordOverrideField { name: field.name, ty: this.resolve_with_changes(&field.ty, 0, changes)? })).collect::<Result<Vec<_>, ConstraintResolutionError>>()?;
                        if let Type::Inference(variable) = receiver.unvalidated() {
                            let row = match this.record_binding(*variable)? {
                                Some(record) => this.resolve_row(&record.shape)?,
                                None => {
                                    let tail = this.fresh_row_inner(span, level.unwrap_or(0), BTreeSet::new(), changes);
                                    let shape = super::RowShape { fields: BTreeMap::new(), tail: RowTail::Variable(tail) };
                                    this.bind_record_inner(*variable, &shape, span, changes).map_err(|(expected, actual, resolution_error)| SealedOperationError::Constraint(ConstraintConflict { expected, actual, resolution_error, contribution: span, initializer: None, established: None }))?;
                                    shape
                                }
                            };
                            let mut fields = row.fields;
                            for field in &overrides { fields.insert(field.name, field.ty.clone()); }
                            let resolved_result = this.resolve_with_changes(&result, 0, changes)?;
                            if let Type::Inference(variable) = resolved_result.unvalidated() {
                                if let Some(record) = this.record_binding(*variable)? {
                                    let established = this.resolve_row(&record.shape)?.fields;
                                    for (name, ty) in &fields {
                                        if let Some(existing) = established.get(name) {
                                            this.constrain_operation_type(existing, ty, span, changes)?;
                                        } else {
                                            this.project_record_field_with_changes(&resolved_result, *name, ty, level.unwrap_or(0), span, changes).map_err(SealedOperationError::Constraint)?;
                                        }
                                    }
                                } else {
                                    let tail = this.fresh_row_inner(span, level.unwrap_or(0), fields.keys().copied().collect(), changes);
                                    let shape = super::RowShape { fields, tail: RowTail::Variable(tail) };
                                    this.bind_record_inner(*variable, &shape, span, changes).map_err(|(expected, actual, resolution_error)| SealedOperationError::Constraint(ConstraintConflict { expected, actual, resolution_error, contribution: span, initializer: None, established: None }))?;
                                }
                            } else if let Type::Record(known) = resolved_result.unvalidated() {
                                for (name, ty) in &fields {
                                    if let Some(existing) = known.get(name) {
                                        this.constrain_operation_type(existing, ty, span, changes)?;
                                    }
                                }
                            }
                            continue;
                        }
                        let Some((selection, selected)) = record_override_selection(&receiver, &overrides) else {
                            return Err(SealedOperationError::UnsupportedRecordSpread { receiver });
                        };
                        this.constrain_operation_type(&result, &selected, span, changes)?;
                        if receiver.contains_inference() || overrides.iter().any(|field| field.ty.contains_inference()) { continue; }
                        let previous = this.operations.entries.get_mut(&id).unwrap().selection.replace(selection);
                        if previous != Some(selection) { changes.push(ConstraintChange::Operation(OperationChange::Selection(id, previous))); }
                        continue;
                    }
                    SealedOperation::Slice { receiver, result, span, .. } => {
                        let receiver = this.resolve_with_changes(&receiver, 0, changes)?;
                        if matches!(receiver.unvalidated(), Type::Inference(_)) { continue; }
                        let Some((selection, selected)) = slice_selection(&receiver) else {
                            return Err(SealedOperationError::UnsupportedSlice { receiver });
                        };
                        this.constrain_operation_type(&result, &selected, span, changes)?;
                        if receiver.contains_inference() { continue; }
                        let previous = this.operations.entries.get_mut(&id).unwrap().selection.replace(selection);
                        if previous != Some(selection) { changes.push(ConstraintChange::Operation(OperationChange::Selection(id, previous))); }
                        continue;
                    }
                };
                let mut left = this.resolve_with_changes(&operation.left, 0, changes)?;
                let mut right = this.resolve_with_changes(&operation.right, 0, changes)?;
                let candidates = ArithmeticSignature::ALL.into_iter().filter(|signature| signature.accepts_operator(operation.operator) && {
                    let (required_left, required_right) = signature.domains();
                    (matches!(left.unvalidated(), Type::Inference(_)) || arithmetic_domain_accepts(required_left, &left))
                        && (matches!(right.unvalidated(), Type::Inference(_)) || arithmetic_domain_accepts(required_right, &right))
                }).collect::<Vec<_>>();
                let Some(&signature) = candidates.first() else { return Err(SealedOperationError::Unsupported { operator: operation.operator, left, right }); };
                if candidates.len() != 1 { continue; }
                let (left_domain, right_domain) = signature.domains();
                for (operand, domain) in [(&left, left_domain), (&right, right_domain)] {
                    if !matches!(operand.unvalidated(), Type::Inference(_)) { continue; }
                    let required = match domain {
                        ArithmeticDomain::Integer => None,
                        ArithmeticDomain::Int => Some(Type::Int),
                        ArithmeticDomain::Float => Some(Type::Float),
                        ArithmeticDomain::Str => Some(Type::Str),
                        ArithmeticDomain::Duration => Some(Type::Duration),
                        ArithmeticDomain::List => Some(Type::List(Box::new(this.fresh_at_level(operation.span, level.unwrap_or(0))))),
                        ArithmeticDomain::Set => {
                            let item = this.fresh_at_level(operation.span, level.unwrap_or(0));
                            this.require_map_key(&item, operation.span).map_err(SealedOperationError::MapKey)?;
                            Some(Type::Set(Box::new(item)))
                        }
                    };
                    if let Some(required) = required { this.constrain_operation_type(operand, &required, operation.span, changes)?; }
                }
                left = this.resolve_with_changes(&operation.left, 0, changes)?;
                right = this.resolve_with_changes(&operation.right, 0, changes)?;
                if signature == ArithmeticSignature::ListConcatenation
                    && let (Type::List(left_item), Type::List(right_item)) = (left.unvalidated(), right.unvalidated())
                    && (matches!(**left_item, Type::Inference(_)) != matches!(**right_item, Type::Inference(_)))
                {
                    this.constrain_operation_type(left_item, right_item, operation.span, changes)?;
                    left = this.resolve_with_changes(&operation.left, 0, changes)?;
                    right = this.resolve_with_changes(&operation.right, 0, changes)?;
                }
                let partial_result = arithmetic_result(signature, &left, &right);
                if let Some(result) = partial_result { this.constrain_operation_type(&operation.result, &result, operation.span, changes)?; }
                if left.contains_inference() || right.contains_inference() { continue; }
                let Some((selection, result)) = arithmetic_selection(operation.operator, &left, &right) else { return Err(SealedOperationError::Unsupported { operator: operation.operator, left, right }); };
                this.constrain_operation_type(&operation.result, &result, operation.span, changes)?;
                let selection = SealedOperationSignature::Binary(selection);
                let previous = this.operations.entries.get_mut(&id).unwrap().selection.replace(selection);
                if previous != Some(selection) { changes.push(ConstraintChange::Operation(OperationChange::Selection(id, previous))); }
            }
            Ok(())
        })
    }

    pub(crate) fn sealed_operation_at(&self, owner: super::CallableApplicationScope, span: Span) -> Option<SealedOperationId> {
        self.operations.source_sites.get(&(owner, span, None)).copied()
    }

    pub(crate) fn source_list_from_end_indexes(&self, owner: super::CallableApplicationScope) -> Result<Vec<(Span, u32)>, ConstraintResolutionError> {
        let mut indexes = Vec::new();
        for (&(scope, span, _), id) in self.operations.source_sites.range((owner, Span::at(crate::source::SourceId::new(0), 0), None)..) {
            if scope != owner { break; }
            let entry = self.operations.entries.get(id).ok_or(ConstraintResolutionError::ForeignVariable)?;
            if entry.owner != owner || entry.operation.span() != span { return Err(ConstraintResolutionError::ForeignVariable); }
            if let SealedOperation::Index { receiver, syntax: crate::sema::sealed_operation::IndexSyntax::NegativeLiteral(distance), .. } = &entry.operation
                && matches!(self.resolve(receiver)?.unvalidated(), Type::List(_)) {
                indexes.push((span, distance.get()));
            }
        }
        Ok(indexes)
    }

    pub(crate) fn source_sealed_operation_id(&self, owner: super::CallableApplicationScope, span: Span) -> Option<SealedOperationId> {
        self.sealed_operation_at(owner, span)
    }

    pub(crate) fn source_sealed_operation(&self, owner: super::CallableApplicationScope, span: Span) -> Option<&SealedOperation<Type>> {
        self.sealed_operation_at(owner, span).and_then(|id| self.operations.entries.get(&id)).map(|entry| &entry.operation)
    }

    pub(crate) fn key_validation_wrapper_requirements_for_instantiation(
        &self,
        callable: &crate::sema::schemes::CallableScheme,
        instance: &super::InstantiatedScheme,
    ) -> Result<Vec<super::WrapperOperationId>, ConstraintResolutionError> {
        if callable.requirements().operations.len() != instance.operations.len() {
            return Err(ConstraintResolutionError::ForeignVariable);
        }
        if callable.requirements().wrappers.len() != instance.wrappers.len() {
            return Err(ConstraintResolutionError::ForeignVariable);
        }
        let mut requirements = Vec::new();
        for (ordinal, requirement) in callable.requirements().wrappers.iter().enumerate() {
            let crate::sema::wrapper_operation::WrapperRequirementKind::KeyValidation {
                operation: crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(operation_ordinal), boundary,
            } = &requirement.kind else { continue; };
            let operation = *instance.operations.get(*operation_ordinal as usize).ok_or(ConstraintResolutionError::ForeignVariable)?;
            let wrapper = *instance.wrappers.get(ordinal).ok_or(ConstraintResolutionError::ForeignVariable)?;
            let wrapper_entry = self.wrappers.entries.get(&wrapper).ok_or(ConstraintResolutionError::ForeignVariable)?;
            let operation_entry = self.operations.entries.get(&operation).ok_or(ConstraintResolutionError::ForeignVariable)?;
            if wrapper_entry.owner != operation_entry.owner { return Err(ConstraintResolutionError::ForeignVariable); }
            let crate::sema::wrapper_operation::WrapperRequirementKind::KeyValidation {
                operation: crate::sema::wrapper_operation::WrapperOperationRef::Source(actual_operation), boundary: actual_boundary,
            } = &wrapper_entry.requirement.kind else { return Err(ConstraintResolutionError::ForeignVariable); };
            if actual_operation != &operation { return Err(ConstraintResolutionError::ForeignVariable); }
            if matches!(boundary, crate::sema::wrapper_operation::WrapperBoundary::Captured | crate::sema::wrapper_operation::WrapperBoundary::ProcCompletion { .. })
                || matches!(actual_boundary, crate::sema::wrapper_operation::WrapperBoundary::Captured | crate::sema::wrapper_operation::WrapperBoundary::ProcCompletion { .. })
            {
                continue;
            }
            let operation = operation_entry.operation.try_map(|ty| self.resolve(ty))?;
            let Some((receiver, key)) = operation.key_operands() else { return Err(ConstraintResolutionError::ForeignVariable); };
            if crate::sema::sealed_operation::may_require_uint_key_validation(&receiver, &key) {
                requirements.push(wrapper);
            }
        }
        Ok(requirements)
    }

    pub(crate) fn register_source_sealed_operation(&mut self, owner: super::CallableApplicationScope, operation: impl Into<SealedOperation<Type>>) -> Result<SealedOperationId, SealedOperationError> {
        let operation = operation.into();
        self.commit_constraint_transaction(|this| {
            let domain = match &operation { SealedOperation::TypePredicate { domain, .. } => Some(*domain), _ => None };
            if let Some(id) = this.operations.source_sites.get(&(owner, operation.span(), domain)).copied() {
                let previous = this.operations.entries[&id].operation.clone();
                if !previous.same_kind(&operation) { return Err(ConstraintResolutionError::ForeignVariable.into()); }
                this.apply_journaled(|this, changes| {
                    for (expected, actual) in previous.types().zip(operation.types()) {
                        this.constrain_inner(expected, actual, operation.span(), true, 0, changes).map_err(|(expected, actual, resolution_error)| SealedOperationError::Constraint(ConstraintConflict { expected, actual, resolution_error, contribution: operation.span(), initializer: None, established: None }))?;
                    }
                    Ok::<_, SealedOperationError>(())
                })?;
                return Ok(id);
            }
            let span = operation.span();
            let id = this.register_sealed_operation(owner, operation)?;
            this.apply_journaled(|this, changes| {
                assert!(this.operations.source_sites.insert((owner, span, domain), id).is_none());
                changes.push(ConstraintChange::Operation(OperationChange::Source((owner, span, domain), id)));
                Ok::<_, SealedOperationError>(())
            })?;
            Ok(id)
        })
    }

    pub(super) fn generalize_sealed_operation(&self, id: SealedOperationId, level: u32, generalization: &mut super::Generalization) -> Result<SealedOperation<crate::sema::schemes::SchemeType>, super::SchemeOperationError> {
        let operation = &self.operations.entries.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?.operation;
        operation.try_map(|ty| self.generalize_type(ty, level, generalization, 0))
    }

    pub(super) fn generalize_sealed_operations(&self, owners: &BTreeSet<super::CallableApplicationScope>, level: u32, generalization: &mut super::Generalization) -> Result<Vec<SealedOperation<crate::sema::schemes::SchemeType>>, super::SchemeOperationError> {
        let mut wrapper_operations = BTreeSet::new();
        for entry in self.wrappers.entries.values().filter(|entry| owners.contains(&entry.owner)) {
            let operations = match &entry.requirement.kind {
                crate::sema::wrapper_operation::WrapperRequirementKind::KeyValidation { operation: crate::sema::wrapper_operation::WrapperOperationRef::Source(id), .. } => vec![*id],
                crate::sema::wrapper_operation::WrapperRequirementKind::Capture { contributions, .. } => contributions.iter().filter_map(|contribution| match contribution {
                    crate::sema::wrapper_operation::WrapperErrorContribution::KeyValidation(crate::sema::wrapper_operation::WrapperOperationRef::Source(id)) => Some(*id),
                    _ => None,
                }).collect(),
                _ => continue,
            };
            for id in operations {
                let indexed = self.operations.entries.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?;
                let operation = indexed.operation.try_map(|ty| self.resolve(ty))?;
                let Some((receiver, key)) = operation.key_operands() else { return Err(ConstraintResolutionError::ForeignVariable.into()); };
                if crate::sema::sealed_operation::may_require_uint_key_validation(&receiver, &key) {
                    wrapper_operations.insert(id);
                }
            }
        }
        let mut operations = Vec::new();
        for (&id, entry) in &self.operations.entries {
            if !owners.contains(&entry.owner)
                || entry.selection.is_some() && !wrapper_operations.contains(&id)
            { continue; }
            let ordinal = u32::try_from(operations.len()).map_err(|_| ConstraintResolutionError::TypeDepth)?;
            generalization.operations.insert(id, ordinal);
            operations.push(self.generalize_sealed_operation(id, level, generalization)?);
        }
        Ok(operations)
    }

    pub(super) fn validate_generalized_operations(&self, callable: &crate::sema::schemes::CallableScheme, scope: &super::CallableGeneralizationScope) -> Result<Vec<SealedOperationId>, super::SchemeOperationError> {
        let view = self.generalized_requirement_view(scope.owner, callable, scope)?;
        self.validate_generalized_operations_view(callable, scope, &view)
    }

    pub(super) fn validate_generalized_operations_view(&self, callable: &crate::sema::schemes::CallableScheme, scope: &super::CallableGeneralizationScope, view: &super::GeneralizedRequirementView<'_>) -> Result<Vec<SealedOperationId>, super::SchemeOperationError> {
        let mut operations = view.operations.iter().map(|(&id, &ordinal)| (ordinal, id)).collect::<Vec<_>>();
        operations.sort_unstable();
        if operations.len() != view.requirements.operations.len() { return Err(super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedSealedOperation)); }
        let mut generalization = self.requirement_generalization(callable, scope, view);
        for (index, &(ordinal, id)) in operations.iter().enumerate() {
            if index != ordinal as usize || self.generalize_sealed_operation(id, 0, &mut generalization)? != view.requirements.operations[index] {
                return Err(super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedSealedOperation));
            }
        }
        if generalization.type_ids != scope.types || generalization.row_ids != scope.rows { return Err(super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::BodyTypeNotBound)); }
        Ok(operations.into_iter().map(|(_, id)| id).collect())
    }

    pub(crate) fn generalized_operation_ordinal(&self, id: SealedOperationId, owner: crate::sema::check::EffectDeclarationId, callable: &crate::sema::schemes::CallableScheme, scope: &super::CallableGeneralizationScope) -> Result<Option<u32>, super::SchemeOperationError> {
        let entry = self.operations.entries.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?;
        if scope.owner != super::CallableApplicationScope::Declaration(owner) { return Err(ConstraintResolutionError::ForeignVariable.into()); }
        let view = self.generalized_requirement_view(entry.owner, callable, scope)?;
        self.validate_generalized_operations_view(callable, scope, &view)?;
        Ok(view.operations.get(&id).copied())
    }

    pub(super) fn sealed_operation_bindings(&self, instance: &super::InstantiatedRequirements, generalization: &super::Generalization) -> Result<Vec<crate::sema::schemes::SealedOperationBinding>, super::SchemeOperationError> {
        instance.operations.iter().map(|id| {
            let entry = self.operations.entries.get(id).ok_or(ConstraintResolutionError::ForeignVariable)?;
            if let Some(ordinal) = generalization.operations.get(id) {
                return Ok(crate::sema::schemes::SealedOperationBinding::Forwarded(*ordinal));
            }
            entry.selection.map(crate::sema::schemes::SealedOperationBinding::Selected)
                .ok_or(super::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedSealedOperation))
        }).collect()
    }
}
