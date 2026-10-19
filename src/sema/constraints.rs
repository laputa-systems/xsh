use super::types::{CallableType, ModuleExportType, Type};
use super::schemes::{
    CallInstantiation, CallableScheme, RowEvidence, SchemeBuildError, SchemeRowParameter,
    SchemeRowParameterId, SchemeRowTail, SchemeType, SchemeTypeParameterId,
};
#[cfg(test)]
use super::schemes::TypeScheme;
use crate::source::Span;
use crate::syntax::arena::{ExprId, StmtId};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_VARIABLE: AtomicU64 = AtomicU64::new(1);
static NEXT_ROW_VARIABLE: AtomicU64 = AtomicU64::new(1);
#[cfg(test)]
static NEXT_RIGID_ROW: AtomicU64 = AtomicU64::new(1);
pub(crate) const MAX_TYPE_DEPTH: usize = 256;

/// A transient semantic identity. It never names dynamic data or a recovery
/// type, and must be substituted before a checked contract is published.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TypeVariableId(u64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct RowVariableId(u64);

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct RigidTypeId(u32);

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct RigidRowId(u64);

#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RigidRow {
    id: RigidRowId,
    lacks: Arc<BTreeSet<crate::symbol::Name>>,
}

#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct RigidRowScope;

#[cfg(test)]
impl RigidRowScope {
    pub(crate) fn fresh(
        &mut self,
        lacks: impl IntoIterator<Item = crate::symbol::Name>,
    ) -> RigidRow {
        RigidRow {
            id: RigidRowId(NEXT_RIGID_ROW.fetch_add(1, Ordering::Relaxed)),
            lacks: Arc::new(lacks.into_iter().collect()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RowShape {
    pub(crate) fields: BTreeMap<crate::symbol::Name, Type>,
    pub(crate) tail: RowTail,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RowTail {
    Closed,
    Variable(RowVariableId),
    #[cfg(test)]
    Rigid(RigidRow),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ConstraintType {
    Ground(Type),
    Variable(TypeVariableId),
    Record(RowShape),
    #[cfg(test)]
    Rigid(RigidTypeId),
}

#[derive(Clone, Debug)]
enum Binding {
    Type { ty: Type, contribution: Span },
    Record(RecordBinding),
    #[cfg(test)]
    Rigid { id: RigidTypeId, contribution: Span },
}

#[derive(Clone, Debug)]
struct Representative {
    canonical: TypeVariableId,
    rank: usize,
    level: u32,
    origin: Span,
    earliest_origin: Span,
    binding: Option<Binding>,
    annotation: bool,
}

#[derive(Clone, Debug)]
struct RecordBinding {
    shape: RowShape,
    contribution: Span,
}

#[derive(Clone, Debug)]
struct RowRepresentative {
    rank: usize,
    level: u32,
    origin: Span,
    earliest_origin: Span,
    lacks: BTreeSet<crate::symbol::Name>,
    binding: Option<RowBinding>,
}

#[derive(Clone, Debug)]
struct RowBinding {
    shape: RowShape,
    contribution: Span,
}

enum OccursNode<'a> {
    Type(&'a Type, usize),
    Row(RowTail, usize),
}

enum LevelNode {
    Type(Type, usize),
    Row(RowTail, usize),
}

#[derive(Debug)]
enum RowVariable {
    Representative(RowRepresentative),
    Parent(AtomicU64),
}

impl Clone for RowVariable {
    fn clone(&self) -> Self {
        match self {
            Self::Representative(representative) => {
                Self::Representative(representative.clone())
            }
            Self::Parent(parent) => Self::Parent(AtomicU64::new(parent.load(Ordering::Relaxed))),
        }
    }
}

#[derive(Clone, Debug)]
enum ConstraintChange {
    Type(TypeVariableId, Variable),
    FreshType(TypeVariableId),
    Row(RowVariableId, Option<RowVariable>),
}

#[derive(Clone, Copy)]
enum ConstraintAuthority {
    Inference,
    CheckedContext,
    NullableSeedCollection(TypeVariableId),
}

enum ResolutionAccess<'a> {
    #[cfg(test)]
    ReadOnly,
    Compress,
    Journal(&'a mut Vec<ConstraintChange>),
}

#[derive(Debug, Default)]
struct ConstraintCounters {
    fresh_variables: AtomicU64,
    generated_constraints: AtomicU64,
    representative_hops: AtomicU64,
    compression_writes: AtomicU64,
    unifications: AtomicU64,
    occurs_visits: AtomicU64,
    row_field_visits: AtomicU64,
    scheme_instantiations: AtomicU64,
    successful_generic_call_resolutions: AtomicU64,
    worklist_wakeups: AtomicU64,
    rollbacks: AtomicU64,
    rollback_changes: AtomicU64,
}

impl Clone for ConstraintCounters {
    fn clone(&self) -> Self {
        Self {
            fresh_variables: AtomicU64::new(self.fresh_variables.load(Ordering::Relaxed)),
            generated_constraints: AtomicU64::new(
                self.generated_constraints.load(Ordering::Relaxed),
            ),
            representative_hops: AtomicU64::new(
                self.representative_hops.load(Ordering::Relaxed),
            ),
            compression_writes: AtomicU64::new(
                self.compression_writes.load(Ordering::Relaxed),
            ),
            unifications: AtomicU64::new(self.unifications.load(Ordering::Relaxed)),
            occurs_visits: AtomicU64::new(self.occurs_visits.load(Ordering::Relaxed)),
            row_field_visits: AtomicU64::new(self.row_field_visits.load(Ordering::Relaxed)),
            scheme_instantiations: AtomicU64::new(
                self.scheme_instantiations.load(Ordering::Relaxed),
            ),
            successful_generic_call_resolutions: AtomicU64::new(
                self.successful_generic_call_resolutions.load(Ordering::Relaxed),
            ),
            worklist_wakeups: AtomicU64::new(self.worklist_wakeups.load(Ordering::Relaxed)),
            rollbacks: AtomicU64::new(self.rollbacks.load(Ordering::Relaxed)),
            rollback_changes: AtomicU64::new(self.rollback_changes.load(Ordering::Relaxed)),
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ConstraintMetrics {
    pub(crate) fresh_variables: u64,
    pub(crate) generated_constraints: u64,
    pub(crate) representative_hops: u64,
    pub(crate) compression_writes: u64,
    pub(crate) unifications: u64,
    pub(crate) occurs_visits: u64,
    pub(crate) row_field_visits: u64,
    pub(crate) scheme_instantiations: u64,
    pub(crate) successful_generic_call_resolutions: u64,
    pub(crate) worklist_wakeups: u64,
    pub(crate) rollbacks: u64,
    pub(crate) rollback_changes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GeneralizedTypes {
    pub(crate) types: Vec<SchemeType>,
    pub(crate) type_parameters: Vec<SchemeTypeParameterId>,
    pub(crate) row_parameters: Vec<SchemeRowParameter>,
}

#[derive(Clone, Debug)]
pub(crate) struct InstantiatedScheme {
    pub(crate) params: Vec<Type>,
    pub(crate) return_type: Type,
    pub(crate) type_arguments: BTreeMap<SchemeTypeParameterId, Type>,
    pub(crate) row_arguments: BTreeMap<SchemeRowParameterId, RowVariableId>,
}

#[derive(Clone, Debug)]
pub(crate) enum SchemeOperationError {
    Resolution(ConstraintResolutionError),
    Build(SchemeBuildError),
    Constraint(ConstraintConflict),
}

impl From<ConstraintResolutionError> for SchemeOperationError {
    fn from(error: ConstraintResolutionError) -> Self {
        Self::Resolution(error)
    }
}

#[derive(Default)]
struct Generalization {
    type_ids: BTreeMap<TypeVariableId, SchemeTypeParameterId>,
    row_ids: BTreeMap<RowVariableId, SchemeRowParameterId>,
    type_parameters: Vec<SchemeTypeParameterId>,
    row_parameters: Vec<SchemeRowParameter>,
}

#[derive(Debug)]
enum Variable {
    Representative(Representative),
    Parent(AtomicU64),
}

impl Clone for Variable {
    fn clone(&self) -> Self {
        match self {
            Self::Representative(representative) => {
                Self::Representative(representative.clone())
            }
            Self::Parent(parent) => Self::Parent(AtomicU64::new(parent.load(Ordering::Relaxed))),
        }
    }
}

/// One local or declaration inference problem. Cloned probes share existing
/// variable identities, while new variables in either probe remain distinct.
/// Substitutions and provenance belong to this value, never a global cache.
#[derive(Debug, Default)]
pub struct TypeConstraints {
    variables: FxHashMap<TypeVariableId, Variable>,
    rows: FxHashMap<RowVariableId, RowVariable>,
    counters: ConstraintCounters,
    trial_changes: Option<Vec<ConstraintChange>>,
}

impl Clone for TypeConstraints {
    fn clone(&self) -> Self {
        assert!(self.trial_changes.is_none(), "an active constraint trial cannot be cloned");
        Self {
            variables: self.variables.clone(),
            rows: self.rows.clone(),
            counters: self.counters.clone(),
            trial_changes: None,
        }
    }
}

struct ConstraintTrial<'a>(&'a mut TypeConstraints);

impl Drop for ConstraintTrial<'_> {
    fn drop(&mut self) {
        let changes = self.0.trial_changes.take().expect("constraint trial owns its journal");
        self.0.rollback(changes);
        self.0.counters.rollbacks.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstraintResolutionError {
    ForeignVariable,
    TypeDepth,
}

#[derive(Clone, Debug)]
pub struct ConstraintConflict {
    pub expected: Type,
    pub actual: Type,
    pub initializer: Option<Span>,
    pub established: Option<Span>,
    pub contribution: Span,
    pub resolution_error: Option<ConstraintResolutionError>,
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct ConstraintTypeConflict {
    pub(crate) expected: ConstraintType,
    pub(crate) actual: ConstraintType,
    pub(crate) initializer: Option<Span>,
    pub(crate) established: Option<Span>,
    pub(crate) contribution: Span,
    pub(crate) resolution_error: Option<ConstraintResolutionError>,
}

impl TypeConstraints {
    pub(crate) fn trial(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Result<(), ConstraintConflict>,
    ) -> Result<bool, ConstraintResolutionError> {
        assert!(self.trial_changes.is_none(), "constraint trials cannot nest");
        self.trial_changes = Some(Vec::new());
        let guard = ConstraintTrial(self);
        match operation(&mut *guard.0) {
            Ok(()) => Ok(true),
            Err(conflict) => match conflict.resolution_error {
                Some(error) => Err(error),
                None => Ok(false),
            },
        }
    }

    fn retain_trial_changes(&mut self, changes: Vec<ConstraintChange>) {
        if let Some(journal) = &mut self.trial_changes {
            journal.extend(changes);
        }
    }

    fn apply_journaled<E>(
        &mut self,
        operation: impl FnOnce(&mut Self, &mut Vec<ConstraintChange>) -> Result<(), E>,
    ) -> Result<(), E> {
        let mut changes = Vec::new();
        let result = if self.trial_changes.is_some() {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(self, &mut changes))) {
                Ok(result) => result,
                Err(payload) => {
                    self.rollback(changes);
                    self.counters.rollbacks.fetch_add(1, Ordering::Relaxed);
                    std::panic::resume_unwind(payload)
                }
            }
        } else {
            operation(self, &mut changes)
        };
        match result {
            Ok(()) => {
                self.retain_trial_changes(changes);
                Ok(())
            }
            Err(error) => {
                self.rollback(changes);
                self.counters.rollbacks.fetch_add(1, Ordering::Relaxed);
                Err(error)
            }
        }
    }

    pub fn fresh(&mut self, origin: Span) -> Type {
        self.fresh_at_level(origin, 0)
    }

    pub(crate) fn fresh_at_level(&mut self, origin: Span, level: u32) -> Type {
        let id = TypeVariableId(NEXT_VARIABLE.fetch_add(1, Ordering::Relaxed));
        self.variables.insert(
            id,
            Variable::Representative(Representative {
                canonical: id,
                rank: 0,
                level,
                origin,
                earliest_origin: origin,
                binding: None,
                annotation: false,
            }),
        );
        self.counters.fresh_variables.fetch_add(1, Ordering::Relaxed);
        if let Some(journal) = &mut self.trial_changes {
            journal.push(ConstraintChange::FreshType(id));
        }
        Type::Inference(id)
    }

    #[cfg(test)]
    pub(crate) fn fresh_row(
        &mut self,
        origin: Span,
        lacks: impl IntoIterator<Item = crate::symbol::Name>,
    ) -> RowVariableId {
        self.fresh_row_at_level(origin, 0, lacks)
    }

    pub(crate) fn fresh_row_at_level(
        &mut self,
        origin: Span,
        level: u32,
        lacks: impl IntoIterator<Item = crate::symbol::Name>,
    ) -> RowVariableId {
        let id = RowVariableId(NEXT_ROW_VARIABLE.fetch_add(1, Ordering::Relaxed));
        self.rows.insert(
            id,
            RowVariable::Representative(RowRepresentative {
                rank: 0,
                level,
                origin,
                earliest_origin: origin,
                lacks: lacks.into_iter().collect(),
                binding: None,
            }),
        );
        self.counters.fresh_variables.fetch_add(1, Ordering::Relaxed);
        if let Some(journal) = &mut self.trial_changes {
            journal.push(ConstraintChange::Row(id, None));
        }
        id
    }

    #[cfg(test)]
    pub(crate) fn metrics(&self) -> ConstraintMetrics {
        ConstraintMetrics {
            fresh_variables: self.counters.fresh_variables.load(Ordering::Relaxed),
            generated_constraints: self.counters.generated_constraints.load(Ordering::Relaxed),
            representative_hops: self.counters.representative_hops.load(Ordering::Relaxed),
            compression_writes: self.counters.compression_writes.load(Ordering::Relaxed),
            unifications: self.counters.unifications.load(Ordering::Relaxed),
            occurs_visits: self.counters.occurs_visits.load(Ordering::Relaxed),
            row_field_visits: self.counters.row_field_visits.load(Ordering::Relaxed),
            scheme_instantiations: self.counters.scheme_instantiations.load(Ordering::Relaxed),
            successful_generic_call_resolutions: self
                .counters
                .successful_generic_call_resolutions
                .load(Ordering::Relaxed),
            worklist_wakeups: self.counters.worklist_wakeups.load(Ordering::Relaxed),
            rollbacks: self.counters.rollbacks.load(Ordering::Relaxed),
            rollback_changes: self.counters.rollback_changes.load(Ordering::Relaxed),
        }
    }

    #[cfg(test)]
    pub(crate) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        let mut retained = size_of::<Self>()
            .saturating_add(
                self.variables.capacity() * size_of::<(TypeVariableId, Variable)>(),
            )
            .saturating_add(self.rows.capacity() * size_of::<(RowVariableId, RowVariable)>());
        for variable in self.variables.values() {
            retained = retained.saturating_add(variable_owned_payload_bytes(variable));
        }
        for row in self.rows.values() {
            retained = retained.saturating_add(row_owned_payload_bytes(row));
        }
        retained
    }

    pub(crate) fn note_scheme_instantiation(&self) {
        self.counters
            .scheme_instantiations
            .fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(test)]
    pub(crate) fn generalize_types(
        &self,
        types: &[Type],
        environment_level: u32,
    ) -> Result<GeneralizedTypes, SchemeOperationError> {
        let mut generalization = Generalization::default();
        let types = types
            .iter()
            .map(|ty| self.generalize_type(ty, environment_level, &mut generalization, 0))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(GeneralizedTypes {
            types,
            type_parameters: generalization.type_parameters,
            row_parameters: generalization.row_parameters,
        })
    }

    pub(crate) fn generalize_callable_body_with_calls(
        &self,
        callable_types: &[Type],
        body_types: &BTreeMap<ExprId, Type>,
        tail_types: &BTreeMap<StmtId, Type>,
        calls: &BTreeMap<Span, (Arc<CallableScheme>, InstantiatedScheme)>,
        environment_level: u32,
    ) -> Result<(
        GeneralizedTypes,
        BTreeMap<ExprId, SchemeType>,
        BTreeMap<StmtId, SchemeType>,
        BTreeMap<Span, CallInstantiation>,
    ), SchemeOperationError> {
        let mut generalization = Generalization::default();
        let types = callable_types
            .iter()
            .map(|ty| self.generalize_type(ty, environment_level, &mut generalization, 0))
            .collect::<Result<Vec<_>, _>>()?;
        let type_parameter_count = generalization.type_parameters.len();
        let row_parameter_count = generalization.row_parameters.len();
        let quantified = type_parameter_count != 0 || row_parameter_count != 0;
        let body_types = body_types
            .iter()
            .filter(|_| quantified)
            .map(|(&id, ty)| {
                Ok((
                    id,
                    self.generalize_type(ty, environment_level, &mut generalization, 0)?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, SchemeOperationError>>()?;
        let tail_types = tail_types
            .iter()
            .filter(|_| quantified)
            .map(|(&statement, ty)| {
                Ok((
                    statement,
                    self.generalize_type(ty, environment_level, &mut generalization, 0)?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, SchemeOperationError>>()?;
        if generalization.type_parameters.len() != type_parameter_count
            || generalization.row_parameters.len() != row_parameter_count
        {
            return Err(SchemeOperationError::Build(
                SchemeBuildError::BodyTypeNotBound,
            ));
        }
        let call_type_parameter_count = generalization.type_parameters.len();
        let call_row_parameter_count = generalization.row_parameters.len();
        let ground_caller = call_type_parameter_count == 0 && call_row_parameter_count == 0;
        let mut call_instantiations = BTreeMap::new();
        for (&span, (callable, instantiated)) in calls {
            let call = self.generalize_call_instantiation(
                callable,
                instantiated,
                &mut generalization,
                environment_level,
                ground_caller,
            )?;
            call_instantiations.insert(span, call);
        }
        if generalization.type_parameters.len() != call_type_parameter_count
            || generalization.row_parameters.len() != call_row_parameter_count
        {
            return Err(SchemeOperationError::Build(
                SchemeBuildError::BodyTypeNotBound,
            ));
        }
        Ok((
            GeneralizedTypes {
                types,
                type_parameters: generalization.type_parameters,
                row_parameters: generalization.row_parameters,
            },
            body_types,
            tail_types,
            call_instantiations,
        ))
    }

    fn generalize_call_instantiation(
        &self,
        callable: &CallableScheme,
        instantiated: &InstantiatedScheme,
        generalization: &mut Generalization,
        environment_level: u32,
        ground_caller: bool,
    ) -> Result<CallInstantiation, SchemeOperationError> {
        let type_arguments = instantiated
            .type_arguments
            .iter()
            .map(|(&id, ty)| {
                let ty = self.generalize_type(ty, environment_level, generalization, 0)?;
                let ty = if ground_caller {
                    ty.closed_ground_type()
                        .map(SchemeType::Ground)
                        .unwrap_or(ty)
                } else {
                    ty
                };
                Ok((id, ty))
            })
            .collect::<Result<BTreeMap<_, _>, SchemeOperationError>>()?;
        let mut row_arguments = BTreeMap::new();
        for parameter in callable.row_parameters() {
            let id = *instantiated
                .row_arguments
                .get(&parameter.id)
                .ok_or(SchemeOperationError::Build(
                    SchemeBuildError::UnboundRowParameter,
                ))?;
            let evidence = self.generalize_row(
                &RowShape {
                    fields: BTreeMap::new(),
                    tail: RowTail::Variable(id),
                },
                environment_level,
                generalization,
                0,
            )?;
            let SchemeType::Record { fields, tail } = evidence else {
                return Err(SchemeOperationError::Build(SchemeBuildError::UnresolvedRow));
            };
            let fields = if ground_caller {
                fields
                    .into_iter()
                    .map(|(name, ty)| {
                        Ok((
                            name,
                            ty.closed_ground_type()
                                .map(SchemeType::Ground)
                                .unwrap_or(ty),
                        ))
                    })
                    .collect::<Result<_, SchemeOperationError>>()?
            } else {
                fields
            };
            row_arguments.insert(
                parameter.id,
                RowEvidence::new(fields, tail).map_err(SchemeOperationError::Build)?,
            );
        }
        CallInstantiation::new(
            callable,
            type_arguments,
            row_arguments,
            &generalization.type_parameters,
            &generalization.row_parameters,
        )
        .map_err(SchemeOperationError::Build)
    }

    #[cfg(test)]
    pub(crate) fn instantiate_scheme(
        &mut self,
        scheme: &TypeScheme,
        level: u32,
        origin: Span,
    ) -> Result<InstantiatedScheme, SchemeOperationError> {
        self.instantiate_callable(scheme.callable(), level, origin)
    }

    pub(crate) fn instantiate_callable(
        &mut self,
        callable: &CallableScheme,
        level: u32,
        origin: Span,
    ) -> Result<InstantiatedScheme, SchemeOperationError> {
        self.note_scheme_instantiation();
        let mut type_arguments = BTreeMap::new();
        for &id in callable.type_parameters() {
            type_arguments.insert(id, self.fresh_at_level(origin, level));
        }
        let mut row_arguments = BTreeMap::new();
        for parameter in callable.row_parameters() {
            row_arguments.insert(
                parameter.id,
                self.fresh_row_at_level(origin, level, parameter.lacks.iter().copied()),
            );
        }
        let params = callable
            .params()
            .iter()
            .map(|parameter| {
                self.instantiate_scheme_type(
                    &parameter.ty,
                    &type_arguments,
                    &row_arguments,
                    level,
                    origin,
                    0,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let return_type = self.instantiate_scheme_type(
            callable.return_type(),
            &type_arguments,
            &row_arguments,
            level,
            origin,
            0,
        )?;
        Ok(InstantiatedScheme {
            params,
            return_type,
            type_arguments,
            row_arguments,
        })
    }

    pub(crate) fn resolve_call_instantiation(
        &self,
        callable: &CallableScheme,
        instantiated: &InstantiatedScheme,
    ) -> Result<CallInstantiation, SchemeOperationError> {
        let type_arguments = instantiated
            .type_arguments
            .iter()
            .map(|(&id, ty)| Ok((id, SchemeType::Ground(self.resolve(ty)?))) )
            .collect::<Result<BTreeMap<_, _>, SchemeOperationError>>()?;
        let mut row_arguments = BTreeMap::new();
        for parameter in callable.row_parameters() {
            let id = *instantiated
                .row_arguments
                .get(&parameter.id)
                .ok_or(SchemeOperationError::Build(
                    SchemeBuildError::UnboundRowParameter,
                ))?;
            let row = self.resolve_row(&RowShape {
                fields: BTreeMap::new(),
                tail: RowTail::Variable(id),
            })?;
            if row.tail != RowTail::Closed {
                return Err(SchemeOperationError::Build(SchemeBuildError::UnresolvedRow));
            }
            let fields = row
                .fields
                .into_iter()
                .map(|(name, ty)| Ok((name, SchemeType::Ground(self.resolve(&ty)?))) )
                .collect::<Result<BTreeMap<_, _>, SchemeOperationError>>()?;
            row_arguments.insert(
                parameter.id,
                RowEvidence::new(fields, SchemeRowTail::Closed)
                    .map_err(SchemeOperationError::Build)?,
            );
        }
        let evidence = CallInstantiation::new(callable, type_arguments, row_arguments, &[], &[])
            .map_err(SchemeOperationError::Build)?;
        if !callable.type_parameters().is_empty() || !callable.row_parameters().is_empty() {
            self.counters
                .successful_generic_call_resolutions
                .fetch_add(1, Ordering::Relaxed);
        }
        Ok(evidence)
    }

    fn generalize_type(
        &self,
        ty: &Type,
        environment_level: u32,
        generalization: &mut Generalization,
        depth: usize,
    ) -> Result<SchemeType, SchemeOperationError> {
        if depth > MAX_TYPE_DEPTH {
            return Err(SchemeOperationError::Resolution(
                ConstraintResolutionError::TypeDepth,
            ));
        }
        match ty {
            Type::Inference(id) => {
                match self.resolve_constraint(&ConstraintType::Variable(*id))? {
                    ConstraintType::Ground(ty) => {
                        self.generalize_type(&ty, environment_level, generalization, depth + 1)
                    }
                    ConstraintType::Record(row) => self.generalize_row(
                        &row,
                        environment_level,
                        generalization,
                        depth + 1,
                    ),
                    ConstraintType::Variable(id) => {
                        let root = self.representative_root(id)?;
                        let Some(Variable::Representative(variable)) = self.variables.get(&root)
                        else {
                            unreachable!("representative lookup returns a root");
                        };
                        if variable.level <= environment_level {
                            return Err(SchemeOperationError::Build(
                                SchemeBuildError::UnresolvedType,
                            ));
                        }
                        let canonical = variable.canonical;
                        let parameter = if let Some(parameter) =
                            generalization.type_ids.get(&canonical)
                        {
                            *parameter
                        } else {
                            let parameter = SchemeTypeParameterId(
                                generalization.type_parameters.len() as u32,
                            );
                            generalization.type_ids.insert(canonical, parameter);
                            generalization.type_parameters.push(parameter);
                            parameter
                        };
                        Ok(SchemeType::Parameter(parameter))
                    }
                    #[cfg(test)]
                    ConstraintType::Rigid(_) => Err(SchemeOperationError::Build(
                        SchemeBuildError::RigidVariable,
                    )),
                }
            }
            Type::List(inner) => Ok(SchemeType::List(Box::new(self.generalize_type(
                inner,
                environment_level,
                generalization,
                depth + 1,
            )?))),
            Type::Map(key, value) => Ok(SchemeType::Map(
                Box::new(self.generalize_type(
                    key,
                    environment_level,
                    generalization,
                    depth + 1,
                )?),
                Box::new(self.generalize_type(
                    value,
                    environment_level,
                    generalization,
                    depth + 1,
                )?),
            )),
            Type::Stream(inner) => Ok(SchemeType::Stream(Box::new(self.generalize_type(
                inner,
                environment_level,
                generalization,
                depth + 1,
            )?))),
            Type::Set(inner) => Ok(SchemeType::Set(Box::new(self.generalize_type(
                inner,
                environment_level,
                generalization,
                depth + 1,
            )?))),
            Type::Result(ok, error) => Ok(SchemeType::Result(
                Box::new(self.generalize_type(
                    ok,
                    environment_level,
                    generalization,
                    depth + 1,
                )?),
                Box::new(self.generalize_type(
                    error,
                    environment_level,
                    generalization,
                    depth + 1,
                )?),
            )),
            Type::Optional(inner) => Ok(SchemeType::Optional(Box::new(self.generalize_type(
                inner,
                environment_level,
                generalization,
                depth + 1,
            )?))),
            Type::Record(fields) => {
                let row = RowShape {
                    fields: fields.as_ref().clone(),
                    tail: RowTail::Closed,
                };
                self.generalize_row(&row, environment_level, generalization, depth + 1)
            }
            Type::Union(types) => Ok(SchemeType::Union(
                types
                    .iter()
                    .map(|ty| {
                        self.generalize_type(
                            ty,
                            environment_level,
                            generalization,
                            depth + 1,
                        )
                    })
                    .collect::<Result<_, _>>()?,
            )),
            Type::Validated(validated) => Ok(SchemeType::Validated(
                validated.validation(),
                Box::new(self.generalize_type(
                    validated.base(),
                    environment_level,
                    generalization,
                    depth + 1,
                )?),
            )),
            ground => {
                if ground.contains_inference() {
                    return Err(SchemeOperationError::Build(SchemeBuildError::UnresolvedType));
                }
                if ground.contains_recovery() {
                    return Err(SchemeOperationError::Build(SchemeBuildError::RecoveryType));
                }
                Ok(SchemeType::Ground(ground.clone()))
            }
        }
    }

    fn generalize_row(
        &self,
        row: &RowShape,
        environment_level: u32,
        generalization: &mut Generalization,
        depth: usize,
    ) -> Result<SchemeType, SchemeOperationError> {
        let resolved = self.resolve_row(row)?;
        let fields = resolved
            .fields
            .iter()
            .map(|(&name, ty)| {
                Ok((
                    name,
                    self.generalize_type(ty, environment_level, generalization, depth + 1)?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, SchemeOperationError>>()?;
        let tail = match resolved.tail {
            RowTail::Closed => SchemeRowTail::Closed,
            RowTail::Variable(id) => {
                let root = self.row_representative_root(id)?;
                let Some(RowVariable::Representative(variable)) = self.rows.get(&root) else {
                    unreachable!("representative lookup returns a root");
                };
                if variable.level <= environment_level {
                    return Err(SchemeOperationError::Build(SchemeBuildError::UnresolvedRow));
                }
                let parameter = if let Some(parameter) = generalization.row_ids.get(&root) {
                    *parameter
                } else {
                    let parameter =
                        SchemeRowParameterId(generalization.row_parameters.len() as u32);
                    generalization.row_ids.insert(root, parameter);
                    generalization.row_parameters.push(SchemeRowParameter {
                        id: parameter,
                        lacks: variable.lacks.clone(),
                    });
                    parameter
                };
                SchemeRowTail::Parameter(parameter)
            }
            #[cfg(test)]
            RowTail::Rigid(_) => {
                return Err(SchemeOperationError::Build(SchemeBuildError::RigidVariable));
            }
        };
        Ok(SchemeType::Record { fields, tail })
    }

    fn instantiate_scheme_type(
        &mut self,
        ty: &SchemeType,
        type_arguments: &BTreeMap<SchemeTypeParameterId, Type>,
        row_arguments: &BTreeMap<SchemeRowParameterId, RowVariableId>,
        level: u32,
        origin: Span,
        depth: usize,
    ) -> Result<Type, SchemeOperationError> {
        if depth > MAX_TYPE_DEPTH {
            return Err(SchemeOperationError::Resolution(
                ConstraintResolutionError::TypeDepth,
            ));
        }
        let nested = |this: &mut Self, ty: &SchemeType, depth| {
            this.instantiate_scheme_type(
                ty,
                type_arguments,
                row_arguments,
                level,
                origin,
                depth,
            )
        };
        match ty {
            SchemeType::Ground(ty) => Ok(ty.clone()),
            SchemeType::Parameter(id) => type_arguments
                .get(id)
                .cloned()
                .ok_or(SchemeOperationError::Build(SchemeBuildError::UnboundTypeParameter)),
            SchemeType::List(inner) => Ok(Type::List(Box::new(nested(self, inner, depth + 1)?))),
            SchemeType::Map(key, value) => Ok(Type::Map(
                Box::new(nested(self, key, depth + 1)?),
                Box::new(nested(self, value, depth + 1)?),
            )),
            SchemeType::Stream(inner) => {
                Ok(Type::Stream(Box::new(nested(self, inner, depth + 1)?)))
            }
            SchemeType::Set(inner) => Ok(Type::Set(Box::new(nested(self, inner, depth + 1)?))),
            SchemeType::Result(ok, error) => Ok(Type::Result(
                Box::new(nested(self, ok, depth + 1)?),
                Box::new(nested(self, error, depth + 1)?),
            )),
            SchemeType::Optional(inner) => {
                Ok(Type::Optional(Box::new(nested(self, inner, depth + 1)?)))
            }
            SchemeType::Record { fields, tail } => {
                let fields = fields
                    .iter()
                    .map(|(&name, field)| Ok((name, nested(self, field, depth + 1)?)))
                    .collect::<Result<BTreeMap<_, _>, SchemeOperationError>>()?;
                match tail {
                    SchemeRowTail::Closed => Ok(Type::Record(Arc::new(fields))),
                    SchemeRowTail::Parameter(id) => {
                        let row = *row_arguments.get(id).ok_or(SchemeOperationError::Build(
                            SchemeBuildError::UnboundRowParameter,
                        ))?;
                        let ty = self.fresh_at_level(origin, level);
                        let Type::Inference(variable) = ty else {
                            unreachable!("fresh type variables use inference types");
                        };
                        self.bind_record(
                            variable,
                            &RowShape {
                                fields,
                                tail: RowTail::Variable(row),
                            },
                            origin,
                        )
                        .map_err(SchemeOperationError::Constraint)?;
                        Ok(Type::Inference(variable))
                    }
                }
            }
            SchemeType::Union(types) => Ok(Type::Union(
                types
                    .iter()
                    .map(|ty| nested(self, ty, depth + 1))
                    .collect::<Result<_, _>>()?,
            )),
            SchemeType::Validated(validation, base) => {
                let base = nested(self, base, depth + 1)?;
                let validated = crate::sema::validated::ValidatedType::new(*validation, base)
                    .map_err(|_| {
                        SchemeOperationError::Build(SchemeBuildError::InvalidValidatedBase)
                    })?;
                Ok(Type::Validated(Box::new(validated)))
            }
        }
    }

    /// Add one directional expectation without widening concrete operands.
    /// A failed nested constraint rolls back every substitution it attempted.
    pub fn constrain(
        &mut self,
        expected: &Type,
        actual: &Type,
        contribution: Span,
    ) -> Result<(), ConstraintConflict> {
        self.constrain_with_authority(
            expected,
            actual,
            contribution,
            ConstraintAuthority::Inference,
        )
    }

    pub(super) fn constrain_nullable_seed(
        &mut self,
        seed: TypeVariableId,
        actual: &Type,
        contribution: Span,
    ) -> Result<(), ConstraintConflict> {
        let expected = Type::Optional(Box::new(Type::Inference(seed)));
        self.constrain_with_authority(
            &expected,
            actual,
            contribution,
            ConstraintAuthority::NullableSeedCollection(seed),
        )
    }

    pub(super) fn check_nullable_seed(
        &self,
        seed: TypeVariableId,
        actual: &Type,
        contribution: Span,
    ) -> Result<(), ConstraintConflict> {
        let expected = Type::Optional(Box::new(Type::Inference(seed)));
        let provenance = self
            .provenance(&expected)
            .or_else(|| self.provenance(actual));
        let conflict = |expected, actual, resolution_error| ConstraintConflict {
            expected,
            actual,
            initializer: provenance.map(|value| value.0),
            established: provenance.and_then(|value| value.1),
            contribution,
            resolution_error,
        };
        let resolved_expected = self
            .resolve(&expected)
            .map_err(|error| conflict(expected.clone(), actual.clone(), Some(error)))?;
        let resolved_actual = self
            .resolve(actual)
            .map_err(|error| conflict(expected.clone(), actual.clone(), Some(error)))?;
        if resolved_expected.contains_inference()
            && (resolved_actual.contains_inference()
                || !has_anchor(&resolved_actual, false)
                    .map_err(|error| conflict(expected.clone(), actual.clone(), Some(error)))?)
        {
            return Ok(());
        }
        if !resolved_expected.contains_inference()
            && !resolved_actual.contains_inference()
            && resolved_actual.matches_expected(&resolved_expected)
        {
            Ok(())
        } else {
            Err(conflict(resolved_expected, resolved_actual, None))
        }
    }

    pub(crate) fn missing_required_open_record_field(
        &self,
        expected: &Type,
        actual: &Type,
    ) -> Result<Option<crate::symbol::Name>, ConstraintResolutionError> {
        let Type::Inference(id) = expected else {
            return Ok(None);
        };
        let Some(expected_record) = self.record_binding(*id)? else {
            return Ok(None);
        };
        let expected_row = self.resolve_row(&expected_record.shape)?;
        if !matches!(expected_row.tail, RowTail::Variable(_)) {
            return Ok(None);
        }
        let actual = self.resolve(actual)?;
        let Type::Record(actual_fields) = actual.unvalidated() else {
            return Ok(None);
        };
        Ok(expected_row
            .fields
            .keys()
            .find(|field| !actual_fields.contains_key(field))
            .copied())
    }

    /// A checked annotation may deliberately choose Null, Any, or an exact
    /// empty shape. Such choices cannot be inferred from ordinary operands.
    pub fn constrain_annotation(
        &mut self,
        expected: &Type,
        annotation: &Type,
        contribution: Span,
    ) -> Result<(), ConstraintConflict> {
        let has_anchor = has_anchor(annotation, true).map_err(|error| ConstraintConflict {
            expected: expected.clone(),
            actual: annotation.clone(),
            initializer: self.provenance(expected).map(|value| value.0),
            established: self.provenance(expected).and_then(|value| value.1),
            contribution,
            resolution_error: Some(error),
        })?;
        if !has_anchor {
            let provenance = self.provenance(expected);
            return Err(ConstraintConflict {
                expected: expected.clone(),
                actual: annotation.clone(),
                initializer: provenance.map(|value| value.0),
                established: provenance.and_then(|value| value.1),
                contribution,
                resolution_error: None,
            });
        }
        self.constrain_with_authority(
            expected,
            annotation,
            contribution,
            ConstraintAuthority::CheckedContext,
        )
    }

    /// A grounded expected context can anchor holes, including an explicitly
    /// dynamic domain, while preserving directional value assignability.
    pub fn constrain_context(
        &mut self,
        expected: &Type,
        actual: &Type,
        contribution: Span,
    ) -> Result<(), ConstraintConflict> {
        self.constrain_with_authority(
            expected,
            actual,
            contribution,
            ConstraintAuthority::CheckedContext,
        )
    }

    pub(crate) fn constrain_rows(
        &mut self,
        expected: &RowShape,
        actual: &RowShape,
        contribution: Span,
    ) -> Result<(), ConstraintConflict> {
        self.counters
            .generated_constraints
            .fetch_add(1, Ordering::Relaxed);
        let provenance = expected
            .fields
            .values()
            .find_map(|ty| self.provenance(ty))
            .or_else(|| self.row_provenance(expected))
            .or_else(|| {
                actual
                    .fields
                    .values()
                    .find_map(|ty| self.provenance(ty))
            })
            .or_else(|| self.row_provenance(actual));
        if let Err((expected, actual, resolution_error)) = self.apply_journaled(|constraints, changes| {
            constraints.unify_rows_inner(expected, actual, contribution, 0, changes)
        }) {
            return Err(ConstraintConflict {
                expected,
                actual,
                initializer: provenance.map(|value| value.0),
                established: provenance.and_then(|value| value.1),
                contribution,
                resolution_error,
            });
        }
        Ok(())
    }

    pub(crate) fn resolve_row(
        &self,
        row: &RowShape,
    ) -> Result<RowShape, ConstraintResolutionError> {
        let mut access = ResolutionAccess::Compress;
        self.resolve_row_inner(row, 0, &mut access)
    }

    pub(crate) fn project_record_field(
        &mut self,
        receiver: &Type,
        field: crate::symbol::Name,
        level: u32,
        contribution: Span,
    ) -> Result<Type, ConstraintConflict> {
        let field_type = self.fresh_at_level(contribution, level);
        let row_variable = self.fresh_row_at_level(contribution, level, [field]);
        let required = RowShape {
            fields: BTreeMap::from([(field, field_type.clone())]),
            tail: RowTail::Variable(row_variable),
        };
        let result = match receiver {
            Type::Inference(id) => {
                self.counters
                    .generated_constraints
                    .fetch_add(1, Ordering::Relaxed);
                self.bind_record(*id, &required, contribution)
            }
            Type::Record(fields) => self.constrain_rows(
                &required,
                &RowShape {
                    fields: fields.as_ref().clone(),
                    tail: RowTail::Closed,
                },
                contribution,
            ),
            _ => Err(ConstraintConflict {
                expected: row_type(&required),
                actual: receiver.clone(),
                initializer: None,
                established: None,
                contribution,
                resolution_error: None,
            }),
        };
        result?;
        self.resolve(&field_type).map_err(|error| ConstraintConflict {
            expected: field_type.clone(),
            actual: receiver.clone(),
            initializer: None,
            established: None,
            contribution,
            resolution_error: Some(error),
        })
    }

    #[cfg(test)]
    pub(crate) fn constrain_types(
        &mut self,
        expected: &ConstraintType,
        actual: &ConstraintType,
        contribution: Span,
    ) -> Result<(), ConstraintTypeConflict> {
        match (expected, actual) {
            (ConstraintType::Rigid(left), ConstraintType::Rigid(right)) if left == right => {
                self.counters
                    .generated_constraints
                    .fetch_add(1, Ordering::Relaxed);
                self.counters.unifications.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            (ConstraintType::Variable(id), ConstraintType::Rigid(rigid))
            | (ConstraintType::Rigid(rigid), ConstraintType::Variable(id)) => {
                self.counters
                    .generated_constraints
                    .fetch_add(1, Ordering::Relaxed);
                self.counters.unifications.fetch_add(1, Ordering::Relaxed);
                self.bind_rigid(*id, *rigid, contribution).map_err(|resolution_error| {
                    self.constraint_type_conflict(
                        expected,
                        actual,
                        contribution,
                        resolution_error,
                    )
                })
            }
            (ConstraintType::Rigid(_), _) | (_, ConstraintType::Rigid(_)) => {
                self.counters
                    .generated_constraints
                    .fetch_add(1, Ordering::Relaxed);
                self.counters.unifications.fetch_add(1, Ordering::Relaxed);
                Err(self.constraint_type_conflict(expected, actual, contribution, None))
            }
            (ConstraintType::Record(left), ConstraintType::Record(right)) => {
                self.constrain_rows(left, right, contribution).map_err(|conflict| {
                    self.constraint_type_conflict(
                        expected,
                        actual,
                        contribution,
                        conflict.resolution_error,
                    )
                })
            }
            (ConstraintType::Variable(id), ConstraintType::Record(row))
            | (ConstraintType::Record(row), ConstraintType::Variable(id)) => {
                self.bind_record(*id, row, contribution).map_err(|conflict| {
                    self.constraint_type_conflict(
                        expected,
                        actual,
                        contribution,
                        conflict.resolution_error,
                    )
                })
            }
            (ConstraintType::Ground(ty), ConstraintType::Record(row)) => match ty {
                Type::Record(fields) => self.constrain_rows(
                    &RowShape {
                        fields: fields.as_ref().clone(),
                        tail: RowTail::Closed,
                    },
                    row,
                    contribution,
                )
                .map_err(|conflict| {
                    self.constraint_type_conflict(
                        expected,
                        actual,
                        contribution,
                        conflict.resolution_error,
                    )
                }),
                _ => Err(self.constraint_type_conflict(expected, actual, contribution, None)),
            },
            (ConstraintType::Record(row), ConstraintType::Ground(ty)) => match ty {
                Type::Record(fields) => self.constrain_rows(
                    row,
                    &RowShape {
                        fields: fields.as_ref().clone(),
                        tail: RowTail::Closed,
                    },
                    contribution,
                )
                .map_err(|conflict| {
                    self.constraint_type_conflict(
                        expected,
                        actual,
                        contribution,
                        conflict.resolution_error,
                    )
                }),
                _ => Err(self.constraint_type_conflict(expected, actual, contribution, None)),
            },
            (ConstraintType::Variable(id), ConstraintType::Ground(ty)) => {
                let variable = Type::Inference(*id);
                self.constrain(&variable, ty, contribution).map_err(|conflict| {
                    self.constraint_type_conflict(
                        expected,
                        actual,
                        contribution,
                        conflict.resolution_error,
                    )
                })
            }
            (ConstraintType::Ground(ty), ConstraintType::Variable(id)) => {
                let variable = Type::Inference(*id);
                self.constrain(ty, &variable, contribution).map_err(|conflict| {
                    self.constraint_type_conflict(
                        expected,
                        actual,
                        contribution,
                        conflict.resolution_error,
                    )
                })
            }
            (ConstraintType::Variable(left), ConstraintType::Variable(right)) => self.constrain(
                &Type::Inference(*left),
                &Type::Inference(*right),
                contribution,
            )
            .map_err(|conflict| {
                self.constraint_type_conflict(
                    expected,
                    actual,
                    contribution,
                    conflict.resolution_error,
                )
            }),
            (ConstraintType::Ground(left), ConstraintType::Ground(right)) => {
                self.constrain(left, right, contribution).map_err(|conflict| {
                    self.constraint_type_conflict(
                        expected,
                        actual,
                        contribution,
                        conflict.resolution_error,
                    )
                })
            }
        }
    }

    #[cfg(test)]
    fn bind_rigid(
        &mut self,
        id: TypeVariableId,
        rigid: RigidTypeId,
        contribution: Span,
    ) -> Result<(), Option<ConstraintResolutionError>> {
        self.apply_journaled(|constraints, changes| {
            constraints.bind_rigid_inner(id, rigid, contribution, changes)
        })
    }

    #[cfg(test)]
    fn bind_rigid_inner(
        &mut self,
        id: TypeVariableId,
        rigid: RigidTypeId,
        contribution: Span,
        changes: &mut Vec<ConstraintChange>,
    ) -> Result<(), Option<ConstraintResolutionError>> {
        let current = self
            .resolve_constraint_with_changes(&ConstraintType::Variable(id), changes)
            .map_err(Some)?;
        match current {
            ConstraintType::Rigid(existing) if existing == rigid => Ok(()),
            ConstraintType::Variable(current) => {
                let root = self.representative(current, Some(changes)).map_err(Some)?;
                self.record(root, changes);
                let Some(Variable::Representative(variable)) = self.variables.get_mut(&root) else {
                    unreachable!("representative lookup returns a root");
                };
                variable.binding = Some(Binding::Rigid {
                    id: rigid,
                    contribution,
                });
                Ok(())
            }
            ConstraintType::Ground(_) | ConstraintType::Record(_) | ConstraintType::Rigid(_) => {
                Err(None)
            }
        }
    }

    #[cfg(test)]
    fn constraint_type_conflict(
        &self,
        expected: &ConstraintType,
        actual: &ConstraintType,
        contribution: Span,
        resolution_error: Option<ConstraintResolutionError>,
    ) -> ConstraintTypeConflict {
        let resolved_expected = self.resolve_constraint_without_compression(expected);
        let resolved_actual = self.resolve_constraint_without_compression(actual);
        let resolution_error = resolution_error
            .or_else(|| resolved_expected.as_ref().err().copied())
            .or_else(|| resolved_actual.as_ref().err().copied());
        let provenance = self
            .constraint_provenance(expected)
            .or_else(|| self.constraint_provenance(actual));
        ConstraintTypeConflict {
            expected: resolved_expected.unwrap_or_else(|_| expected.clone()),
            actual: resolved_actual.unwrap_or_else(|_| actual.clone()),
            initializer: provenance.map(|value| value.0),
            established: provenance.and_then(|value| value.1),
            contribution,
            resolution_error,
        }
    }

    #[cfg(test)]
    fn constraint_provenance(
        &self,
        ty: &ConstraintType,
    ) -> Option<(Span, Option<Span>)> {
        match ty {
            ConstraintType::Ground(ty) => self.provenance(ty),
            ConstraintType::Variable(id) => self.provenance(&Type::Inference(*id)),
            ConstraintType::Record(row) => self
                .provenance(&row_type(row))
                .or_else(|| self.row_provenance(row)),
            ConstraintType::Rigid(_) => None,
        }
    }

    fn bind_record(
        &mut self,
        id: TypeVariableId,
        shape: &RowShape,
        contribution: Span,
    ) -> Result<(), ConstraintConflict> {
        let provenance = self.provenance(&Type::Inference(id));
        if let Err((expected, actual, resolution_error)) = self.apply_journaled(|constraints, changes| {
            constraints.bind_record_inner(id, shape, contribution, changes)
        }) {
            return Err(ConstraintConflict {
                expected,
                actual,
                initializer: provenance.map(|value| value.0),
                established: provenance.and_then(|value| value.1),
                contribution,
                resolution_error,
            });
        }
        Ok(())
    }

    fn bind_record_inner(
        &mut self,
        id: TypeVariableId,
        shape: &RowShape,
        contribution: Span,
        changes: &mut Vec<ConstraintChange>,
    ) -> Result<(), (Type, Type, Option<ConstraintResolutionError>)> {
        let root = self
            .representative(id, Some(changes))
            .map_err(|error| (Type::Inference(id), row_type(shape), Some(error)))?;
        if self
            .occurs_in_row(root, shape)
            .map_err(|error| (Type::Inference(root), row_type(shape), Some(error)))?
        {
            return Err((Type::Inference(root), row_type(shape), None));
        }
        let level = self
            .type_level(root)
            .map_err(|error| (Type::Inference(root), row_type(shape), Some(error)))?;
        self.lower_levels_in_row(shape, level, changes)
            .map_err(|error| (Type::Inference(root), row_type(shape), Some(error)))?;
        for field in shape.fields.values() {
            self.resolve_with_changes(field, 1, changes)
                .map_err(|error| (Type::Inference(root), row_type(shape), Some(error)))?;
        }
        let (canonical, binding) = match self.variables.get(&root) {
            Some(Variable::Representative(variable)) => (
                variable.canonical,
                variable.binding.clone(),
            ),
            _ => unreachable!("representative lookup returns a root"),
        };
        if let Some(binding) = binding {
            return match binding {
                Binding::Type { ty: Type::Record(fields), .. } => self.unify_rows_inner(
                    shape,
                    &RowShape {
                        fields: fields.as_ref().clone(),
                        tail: RowTail::Closed,
                    },
                    contribution,
                    0,
                    changes,
                ),
                Binding::Type { ty, .. } => Err((Type::Inference(canonical), ty, None)),
                Binding::Record(record) => {
                    self.unify_rows_inner(&record.shape, shape, contribution, 0, changes)
                }
                #[cfg(test)]
                Binding::Rigid { .. } => Err((Type::Inference(canonical), row_type(shape), None)),
            };
        }
        self.record(root, changes);
        let Some(Variable::Representative(variable)) = self.variables.get_mut(&root) else {
            unreachable!("representative lookup returns a root");
        };
        variable.binding = Some(Binding::Record(RecordBinding {
            shape: shape.clone(),
            contribution,
        }));
        Ok(())
    }

    fn unify_rows_inner(
        &mut self,
        expected: &RowShape,
        actual: &RowShape,
        contribution: Span,
        depth: usize,
        changes: &mut Vec<ConstraintChange>,
    ) -> Result<(), (Type, Type, Option<ConstraintResolutionError>)> {
        if depth > MAX_TYPE_DEPTH {
            return Err((
                row_type(expected),
                row_type(actual),
                Some(ConstraintResolutionError::TypeDepth),
            ));
        }
        self.counters.unifications.fetch_add(1, Ordering::Relaxed);
        self.counters
            .worklist_wakeups
            .fetch_add(1, Ordering::Relaxed);
        let (expected, actual) = {
            let mut access = ResolutionAccess::Journal(changes);
            let expected = self
                .resolve_row_inner(expected, depth, &mut access)
                .map_err(|error| (row_type(expected), row_type(actual), Some(error)))?;
            let actual = self
                .resolve_row_inner(actual, depth, &mut access)
                .map_err(|error| (row_type(&expected), row_type(actual), Some(error)))?;
            (expected, actual)
        };
        let mut left_only = BTreeMap::new();
        let mut right_only = BTreeMap::new();
        for (name, expected_field) in &expected.fields {
            self.counters
                .row_field_visits
                .fetch_add(1, Ordering::Relaxed);
            if let Some(actual_field) = actual.fields.get(name) {
                self.constrain_inner(
                    expected_field,
                    actual_field,
                    contribution,
                    false,
                    depth + 1,
                    changes,
                )?;
            } else {
                left_only.insert(*name, expected_field.clone());
            }
        }
        for (name, actual_field) in &actual.fields {
            self.counters
                .row_field_visits
                .fetch_add(1, Ordering::Relaxed);
            if !expected.fields.contains_key(name) {
                right_only.insert(*name, actual_field.clone());
            }
        }
        self.unify_row_tails(
            expected.tail.clone(),
            actual.tail.clone(),
            &left_only,
            &right_only,
            contribution,
            depth,
            changes,
        )
        .map_err(|error| (row_type(&expected), row_type(&actual), error))
    }

    fn unify_row_tails(
        &mut self,
        left: RowTail,
        right: RowTail,
        left_only: &BTreeMap<crate::symbol::Name, Type>,
        right_only: &BTreeMap<crate::symbol::Name, Type>,
        contribution: Span,
        depth: usize,
        changes: &mut Vec<ConstraintChange>,
    ) -> Result<(), Option<ConstraintResolutionError>> {
        let left = match left {
            RowTail::Variable(id) => RowTail::Variable(
                self.row_representative(id, Some(changes)).map_err(Some)?,
            ),
            other => other,
        };
        let right = match right {
            RowTail::Variable(id) => RowTail::Variable(
                self.row_representative(id, Some(changes)).map_err(Some)?,
            ),
            other => other,
        };
        match (left, right) {
            (RowTail::Closed, RowTail::Closed) => {
                if left_only.is_empty() && right_only.is_empty() {
                    Ok(())
                } else {
                    Err(None)
                }
            }
            (RowTail::Variable(left), RowTail::Closed) => {
                if !left_only.is_empty() {
                    return Err(None);
                }
                self.bind_row(
                    left,
                    RowShape {
                        fields: right_only.clone(),
                        tail: RowTail::Closed,
                    },
                    contribution,
                    depth,
                    changes,
                )
            }
            (RowTail::Closed, RowTail::Variable(right)) => {
                if !right_only.is_empty() {
                    return Err(None);
                }
                self.bind_row(
                    right,
                    RowShape {
                        fields: left_only.clone(),
                        tail: RowTail::Closed,
                    },
                    contribution,
                    depth,
                    changes,
                )
            }
            (RowTail::Variable(left), RowTail::Variable(right)) if left == right => {
                if left_only.is_empty() && right_only.is_empty() {
                    Ok(())
                } else {
                    Err(None)
                }
            }
            (RowTail::Variable(left), RowTail::Variable(right)) => {
                if left_only.is_empty() && right_only.is_empty() {
                    return self.union_rows(left, right, changes).map_err(Some);
                }
                let mut lacks = self.row_lacks(left).map_err(Some)?.clone();
                lacks.extend(self.row_lacks(right).map_err(Some)?.iter().copied());
                lacks.extend(left_only.keys().copied());
                lacks.extend(right_only.keys().copied());
                let level = self
                    .row_level(left)
                    .map_err(Some)?
                    .min(self.row_level(right).map_err(Some)?);
                let shared = self.fresh_row_inner(contribution, level, lacks, changes);
                self.bind_row(
                    left,
                    RowShape {
                        fields: right_only.clone(),
                        tail: RowTail::Variable(shared),
                    },
                    contribution,
                    depth,
                    changes,
                )?;
                self.bind_row(
                    right,
                    RowShape {
                        fields: left_only.clone(),
                        tail: RowTail::Variable(shared),
                    },
                    contribution,
                    depth,
                    changes,
                )
            }
            #[cfg(test)]
            (RowTail::Variable(left), RowTail::Rigid(right)) => {
                if !left_only.is_empty() {
                    return Err(None);
                }
                self.bind_row(
                    left,
                    RowShape {
                        fields: right_only.clone(),
                        tail: RowTail::Rigid(right),
                    },
                    contribution,
                    depth,
                    changes,
                )
            }
            #[cfg(test)]
            (RowTail::Rigid(left), RowTail::Variable(right)) => {
                if !right_only.is_empty() {
                    return Err(None);
                }
                self.bind_row(
                    right,
                    RowShape {
                        fields: left_only.clone(),
                        tail: RowTail::Rigid(left),
                    },
                    contribution,
                    depth,
                    changes,
                )
            }
            #[cfg(test)]
            (RowTail::Rigid(left), RowTail::Rigid(right)) if left.id == right.id => {
                if left_only.is_empty() && right_only.is_empty() {
                    Ok(())
                } else {
                    Err(None)
                }
            }
            #[cfg(test)]
            (RowTail::Closed, RowTail::Rigid(_)) | (RowTail::Rigid(_), RowTail::Closed) => {
                Err(None)
            }
            #[cfg(test)]
            (RowTail::Rigid(_), RowTail::Rigid(_)) => Err(None),
        }
    }

    fn bind_row(
        &mut self,
        id: RowVariableId,
        shape: RowShape,
        contribution: Span,
        depth: usize,
        changes: &mut Vec<ConstraintChange>,
    ) -> Result<(), Option<ConstraintResolutionError>> {
        let root = self.row_representative(id, Some(changes)).map_err(Some)?;
        let shape = {
            let mut access = ResolutionAccess::Journal(changes);
            self.resolve_row_inner(&shape, depth, &mut access).map_err(Some)?
        };
        if shape.fields.values().any(Type::contains_any) {
            return Err(None);
        }
        if let Some(binding) = self.row_binding(root).map_err(Some)?.cloned() {
            return self
                .unify_rows_inner(&binding.shape, &shape, contribution, depth, changes)
                .map_err(|(_, _, error)| error);
        }
        let lacks = self.row_lacks(root).map_err(Some)?.clone();
        if shape.fields.keys().any(|name| lacks.contains(name)) {
            return Err(None);
        }
        if shape.tail == RowTail::Variable(root) {
            if shape.fields.is_empty() {
                return Ok(());
            }
            return Err(None);
        }
        let level = self.row_level(root).map_err(Some)?;
        self.lower_levels_in_row(&shape, level, changes).map_err(Some)?;
        let mut inherited_lacks = lacks;
        inherited_lacks.extend(shape.fields.keys().copied());
        self.add_row_lacks(shape.tail.clone(), &inherited_lacks, changes)?;
        self.record_row(root, changes);
        let Some(RowVariable::Representative(variable)) = self.rows.get_mut(&root) else {
            unreachable!("row representative lookup returns a root");
        };
        variable.binding = Some(RowBinding { shape, contribution });
        Ok(())
    }

    fn add_row_lacks(
        &mut self,
        tail: RowTail,
        labels: &BTreeSet<crate::symbol::Name>,
        changes: &mut Vec<ConstraintChange>,
    ) -> Result<(), Option<ConstraintResolutionError>> {
        let id = match tail {
            RowTail::Closed => return Ok(()),
            #[cfg(test)]
            RowTail::Rigid(rigid) => {
                return if labels.is_subset(&rigid.lacks) {
                    Ok(())
                } else {
                    Err(None)
                };
            }
            RowTail::Variable(id) => id,
        };
        let root = self.row_representative(id, Some(changes)).map_err(Some)?;
        let (binding, existing_lacks) = match self.rows.get(&root) {
            Some(RowVariable::Representative(variable)) => {
                (variable.binding.clone(), variable.lacks.clone())
            }
            _ => unreachable!("row representative lookup returns a root"),
        };
        if let Some(binding) = binding {
            if binding.shape.fields.keys().any(|name| labels.contains(name)) {
                return Err(None);
            }
            let mut inherited = labels.clone();
            inherited.extend(binding.shape.fields.keys().copied());
            let nested = binding.shape.tail.clone();
            return self.add_row_lacks(nested, &inherited, changes);
        }
        if labels.is_subset(&existing_lacks) {
            return Ok(());
        }
        self.record_row(root, changes);
        let Some(RowVariable::Representative(variable)) = self.rows.get_mut(&root) else {
            unreachable!("row representative lookup returns a root");
        };
        variable.lacks.extend(labels.iter().copied());
        Ok(())
    }

    fn union_rows(
        &mut self,
        left: RowVariableId,
        right: RowVariableId,
        changes: &mut Vec<ConstraintChange>,
    ) -> Result<(), ConstraintResolutionError> {
        let left = self.row_representative(left, Some(changes))?;
        let right = self.row_representative(right, Some(changes))?;
        if left == right {
            return Ok(());
        }
        let (left_rank, left_level, left_origin, left_earliest, left_lacks) = match self.rows.get(&left) {
            Some(RowVariable::Representative(variable)) => (
                variable.rank,
                variable.level,
                variable.origin,
                variable.earliest_origin,
                variable.lacks.clone(),
            ),
            _ => unreachable!("row representative lookup returns roots"),
        };
        let (right_rank, right_level, right_origin, right_earliest, right_lacks) = match self.rows.get(&right) {
            Some(RowVariable::Representative(variable)) => (
                variable.rank,
                variable.level,
                variable.origin,
                variable.earliest_origin,
                variable.lacks.clone(),
            ),
            _ => unreachable!("row representative lookup returns roots"),
        };
        let attach_left = left_rank < right_rank || (left_rank == right_rank && right < left);
        let (root, child, root_rank, child_rank) = if attach_left {
            (right, left, right_rank, left_rank)
        } else {
            (left, right, left_rank, right_rank)
        };
        self.record_row(child, changes);
        self.rows.insert(child, RowVariable::Parent(AtomicU64::new(root.0)));
        self.record_row(root, changes);
        let level = left_level.min(right_level);
        {
            let Some(RowVariable::Representative(root_value)) = self.rows.get_mut(&root) else {
                unreachable!("row union root remains a representative");
            };
            if root_rank == child_rank {
                root_value.rank += 1;
            }
            root_value.level = level;
            root_value.earliest_origin = earlier_span(left_earliest, right_earliest);
            root_value.origin = if left <= right { left_origin } else { right_origin };
            root_value.lacks = left_lacks.union(&right_lacks).copied().collect();
        }
        self.lower_levels_in_row(
            &RowShape {
                fields: BTreeMap::new(),
                tail: RowTail::Variable(root),
            },
            level,
            changes,
        )
    }

    fn fresh_row_inner(
        &mut self,
        origin: Span,
        level: u32,
        lacks: BTreeSet<crate::symbol::Name>,
        changes: &mut Vec<ConstraintChange>,
    ) -> RowVariableId {
        let id = RowVariableId(NEXT_ROW_VARIABLE.fetch_add(1, Ordering::Relaxed));
        changes.push(ConstraintChange::Row(id, None));
        self.rows.insert(
            id,
            RowVariable::Representative(RowRepresentative {
                rank: 0,
                level,
                origin,
                earliest_origin: origin,
                lacks,
                binding: None,
            }),
        );
        self.counters.fresh_variables.fetch_add(1, Ordering::Relaxed);
        id
    }

    fn type_level(&self, id: TypeVariableId) -> Result<u32, ConstraintResolutionError> {
        let root = self.representative_root(id)?;
        let Some(Variable::Representative(variable)) = self.variables.get(&root) else {
            unreachable!("representative lookup returns a root");
        };
        Ok(variable.level)
    }

    fn row_level(&self, id: RowVariableId) -> Result<u32, ConstraintResolutionError> {
        let root = self.row_representative_root(id)?;
        let Some(RowVariable::Representative(variable)) = self.rows.get(&root) else {
            unreachable!("row representative lookup returns a root");
        };
        Ok(variable.level)
    }

    fn lower_levels_in_type(
        &mut self,
        ty: &Type,
        level: u32,
        changes: &mut Vec<ConstraintChange>,
    ) -> Result<(), ConstraintResolutionError> {
        self.lower_levels(
            [LevelNode::Type(ty.clone(), 0)],
            level,
            changes,
        )
    }

    fn lower_levels_in_row(
        &mut self,
        row: &RowShape,
        level: u32,
        changes: &mut Vec<ConstraintChange>,
    ) -> Result<(), ConstraintResolutionError> {
        let mut pending = row
            .fields
            .values()
            .cloned()
            .map(|ty| LevelNode::Type(ty, 1))
            .collect::<Vec<_>>();
        pending.push(LevelNode::Row(row.tail.clone(), 0));
        self.lower_levels(pending, level, changes)
    }

    fn lower_levels(
        &mut self,
        initial: impl IntoIterator<Item = LevelNode>,
        level: u32,
        changes: &mut Vec<ConstraintChange>,
    ) -> Result<(), ConstraintResolutionError> {
        let mut pending = initial.into_iter().collect::<Vec<_>>();
        let mut seen_types = FxHashSet::default();
        let mut seen_rows = FxHashSet::default();
        while let Some(node) = pending.pop() {
            match node {
                LevelNode::Type(ty, depth) => {
                    if depth > MAX_TYPE_DEPTH {
                        return Err(ConstraintResolutionError::TypeDepth);
                    }
                    match ty {
                        Type::Inference(id) => {
                            let root = self.representative(id, Some(changes))?;
                            if !seen_types.insert(root) {
                                continue;
                            }
                            let (old_level, binding) = match self.variables.get(&root) {
                                Some(Variable::Representative(variable)) => {
                                    (variable.level, variable.binding.clone())
                                }
                                _ => unreachable!("representative lookup returns a root"),
                            };
                            if old_level <= level {
                                continue;
                            }
                            self.record(root, changes);
                            let Some(Variable::Representative(variable)) =
                                self.variables.get_mut(&root)
                            else {
                                unreachable!("representative lookup returns a root");
                            };
                            variable.level = level;
                            match binding {
                                Some(Binding::Type { ty, .. }) => {
                                    pending.push(LevelNode::Type(ty, depth + 1));
                                }
                                Some(Binding::Record(record)) => {
                                    pending.extend(record.shape.fields.into_values().map(|ty| {
                                        LevelNode::Type(ty, depth + 1)
                                    }));
                                    pending.push(LevelNode::Row(record.shape.tail, depth + 1));
                                }
                                #[cfg(test)]
                                Some(Binding::Rigid { .. }) => {}
                                None => {}
                            }
                        }
                        Type::List(inner)
                        | Type::Stream(inner)
                        | Type::Set(inner)
                        | Type::Optional(inner) => {
                            pending.push(LevelNode::Type(*inner, depth + 1));
                        }
                        Type::Map(key, value) | Type::Result(key, value) => {
                            pending.push(LevelNode::Type(*key, depth + 1));
                            pending.push(LevelNode::Type(*value, depth + 1));
                        }
                        Type::Record(fields) => {
                            pending.extend(fields.values().cloned().map(|ty| {
                                LevelNode::Type(ty, depth + 1)
                            }));
                        }
                        Type::Module(exports) => {
                            for export in exports.values() {
                                match export {
                                    ModuleExportType::Value { ty, .. } => pending
                                        .push(LevelNode::Type(ty.clone(), depth + 1)),
                                    ModuleExportType::Pure { sig, .. }
                                    | ModuleExportType::Proc { sig, .. } => {
                                        pending.push(LevelNode::Type(
                                            (*sig.return_ty).clone(),
                                            depth + 1,
                                        ));
                                        pending.extend(sig.params.iter().map(|parameter| {
                                            LevelNode::Type(parameter.ty.clone(), depth + 1)
                                        }));
                                    }
                                    ModuleExportType::GenericPure { .. } => {}
                                }
                            }
                        }
                        Type::Union(types) => {
                            pending.extend(types.into_iter().map(|ty| {
                                LevelNode::Type(ty, depth + 1)
                            }));
                        }
                        Type::Callable(callable) => {
                            pending.push(LevelNode::Type(
                                (*callable.sig.return_ty).clone(),
                                depth + 1,
                            ));
                            pending.extend(callable.sig.params.iter().map(|parameter| {
                                LevelNode::Type(parameter.ty.clone(), depth + 1)
                            }));
                        }
                        Type::Validated(validated) => {
                            pending.push(LevelNode::Type(validated.base().clone(), depth + 1));
                        }
                        _ => {}
                    }
                }
                LevelNode::Row(tail, depth) => {
                    if depth > MAX_TYPE_DEPTH {
                        return Err(ConstraintResolutionError::TypeDepth);
                    }
                    let RowTail::Variable(id) = tail else {
                        continue;
                    };
                    let root = self.row_representative(id, Some(changes))?;
                    if !seen_rows.insert(root) {
                        continue;
                    }
                    let (old_level, binding) = match self.rows.get(&root) {
                        Some(RowVariable::Representative(variable)) => {
                            (variable.level, variable.binding.clone())
                        }
                        _ => unreachable!("row representative lookup returns a root"),
                    };
                    if old_level <= level {
                        continue;
                    }
                    self.record_row(root, changes);
                    let Some(RowVariable::Representative(variable)) = self.rows.get_mut(&root)
                    else {
                        unreachable!("row representative lookup returns a root");
                    };
                    variable.level = level;
                    if let Some(binding) = binding {
                        pending.extend(binding.shape.fields.into_values().map(|ty| {
                            LevelNode::Type(ty, depth + 1)
                        }));
                        pending.push(LevelNode::Row(binding.shape.tail, depth + 1));
                    }
                }
            }
        }
        Ok(())
    }

    fn resolve_row_inner(
        &self,
        row: &RowShape,
        depth: usize,
        access: &mut ResolutionAccess<'_>,
    ) -> Result<RowShape, ConstraintResolutionError> {
        if depth > MAX_TYPE_DEPTH {
            return Err(ConstraintResolutionError::TypeDepth);
        }
        let mut fields = row.fields.clone();
        let mut tail = row.tail.clone();
        let mut seen = FxHashSet::default();
        while let RowTail::Variable(id) = &tail {
            let root = self.row_representative_for_resolution(*id, access)?;
            if !seen.insert(root) {
                return Err(ConstraintResolutionError::TypeDepth);
            }
            let Some(RowVariable::Representative(variable)) = self.rows.get(&root) else {
                unreachable!("row representative lookup returns a root");
            };
            match &variable.binding {
                Some(binding) => {
                    for (name, ty) in &binding.shape.fields {
                        if fields.insert(*name, ty.clone()).is_some() {
                            return Err(ConstraintResolutionError::TypeDepth);
                        }
                    }
                    tail = binding.shape.tail.clone();
                }
                None => {
                    tail = RowTail::Variable(root);
                    break;
                }
            }
        }
        for ty in fields.values_mut() {
            self.resolve_depth_inner(ty, depth + 1, access)?;
        }
        Ok(RowShape { fields, tail })
    }

    fn row_representative_root(
        &self,
        id: RowVariableId,
    ) -> Result<RowVariableId, ConstraintResolutionError> {
        let mut current = id;
        let mut seen = FxHashSet::default();
        for _ in 0..=self.rows.len() {
            if !seen.insert(current) {
                return Err(ConstraintResolutionError::TypeDepth);
            }
            match self.rows.get(&current) {
                Some(RowVariable::Representative(_)) => return Ok(current),
                Some(RowVariable::Parent(parent)) => {
                    self.counters
                        .representative_hops
                        .fetch_add(1, Ordering::Relaxed);
                    current = RowVariableId(parent.load(Ordering::Relaxed));
                }
                None => return Err(ConstraintResolutionError::ForeignVariable),
            }
        }
        Err(ConstraintResolutionError::TypeDepth)
    }

    fn row_representative(
        &self,
        id: RowVariableId,
        mut changes: Option<&mut Vec<ConstraintChange>>,
    ) -> Result<RowVariableId, ConstraintResolutionError> {
        let root = self.row_representative_root(id)?;
        let mut current = id;
        while current != root {
            let Some(RowVariable::Parent(parent)) = self.rows.get(&current) else {
                unreachable!("non-root row representatives have parents");
            };
            let next = RowVariableId(parent.load(Ordering::Relaxed));
            if next != root {
                if let Some(changes) = changes.as_deref_mut() {
                    self.record_row(current, changes);
                }
                let Some(RowVariable::Parent(parent)) = self.rows.get(&current) else {
                    unreachable!("non-root row representatives have parents");
                };
                parent.store(root.0, Ordering::Relaxed);
                self.counters
                    .compression_writes
                    .fetch_add(1, Ordering::Relaxed);
            }
            current = next;
        }
        Ok(root)
    }

    fn row_representative_for_resolution(
        &self,
        id: RowVariableId,
        access: &mut ResolutionAccess<'_>,
    ) -> Result<RowVariableId, ConstraintResolutionError> {
        match access {
            #[cfg(test)]
            ResolutionAccess::ReadOnly => self.row_representative_root(id),
            ResolutionAccess::Compress if self.trial_changes.is_some() => self.row_representative_root(id),
            ResolutionAccess::Compress => self.row_representative(id, None),
            ResolutionAccess::Journal(changes) => {
                self.row_representative(id, Some(&mut **changes))
            }
        }
    }

    fn row_binding(
        &self,
        id: RowVariableId,
    ) -> Result<Option<&RowBinding>, ConstraintResolutionError> {
        let root = self.row_representative_root(id)?;
        let Some(RowVariable::Representative(variable)) = self.rows.get(&root) else {
            unreachable!("row representative lookup returns a root");
        };
        Ok(variable.binding.as_ref())
    }

    fn record_binding(
        &self,
        id: TypeVariableId,
    ) -> Result<Option<RecordBinding>, ConstraintResolutionError> {
        let root = self.representative_root(id)?;
        let Some(Variable::Representative(variable)) = self.variables.get(&root) else {
            unreachable!("type representative lookup returns a root");
        };
        Ok(match &variable.binding {
            Some(Binding::Record(record)) => Some(record.clone()),
            Some(Binding::Type { .. }) | None => None,
            #[cfg(test)]
            Some(Binding::Rigid { .. }) => None,
        })
    }

    #[cfg(test)]
    fn rigid_binding(
        &self,
        id: TypeVariableId,
    ) -> Result<Option<RigidTypeId>, ConstraintResolutionError> {
        let root = self.representative_root(id)?;
        let Some(Variable::Representative(variable)) = self.variables.get(&root) else {
            unreachable!("representative lookup returns a root");
        };
        Ok(match &variable.binding {
            Some(Binding::Rigid { id, .. }) => Some(*id),
            _ => None,
        })
    }

    fn row_lacks(
        &self,
        id: RowVariableId,
    ) -> Result<&BTreeSet<crate::symbol::Name>, ConstraintResolutionError> {
        let root = self.row_representative_root(id)?;
        let Some(RowVariable::Representative(variable)) = self.rows.get(&root) else {
            unreachable!("row representative lookup returns a root");
        };
        Ok(&variable.lacks)
    }

    fn row_provenance(&self, row: &RowShape) -> Option<(Span, Option<Span>)> {
        let RowTail::Variable(id) = &row.tail else {
            return None;
        };
        let root = self.row_representative_root(*id).ok()?;
        let Some(RowVariable::Representative(variable)) = self.rows.get(&root) else {
            return None;
        };
        Some((
            variable.earliest_origin,
            variable.binding.as_ref().map(|binding| binding.contribution),
        ))
    }

    fn constrain_with_authority(
        &mut self,
        expected: &Type,
        actual: &Type,
        contribution: Span,
        authority: ConstraintAuthority,
    ) -> Result<(), ConstraintConflict> {
        self.counters
            .generated_constraints
            .fetch_add(1, Ordering::Relaxed);
        let provenance = self
            .provenance(expected)
            .or_else(|| self.provenance(actual));
        let result = self.apply_journaled(|constraints, changes| {
            let constrained = match authority {
                ConstraintAuthority::NullableSeedCollection(seed) => constraints
                    .constrain_nullable_seed_inner(seed, actual, contribution, changes),
                ConstraintAuthority::Inference | ConstraintAuthority::CheckedContext => {
                    constraints.constrain_inner(
                        expected,
                        actual,
                        contribution,
                        matches!(authority, ConstraintAuthority::CheckedContext),
                        0,
                        changes,
                    )
                }
            };
            constrained.and_then(|()| {
                let resolved_expected = constraints
                    .resolve_with_changes(expected, 0, changes)
                    .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?;
                let resolved_actual = constraints
                    .resolve_with_changes(actual, 0, changes)
                    .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?;
                let unresolved = constraints
                    .variable_ids(&resolved_expected)
                    .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?;
                let actual_unresolved = constraints
                    .variable_ids(&resolved_actual)
                    .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?;
                if unresolved.is_empty()
                    && actual_unresolved.is_empty()
                    && !resolved_actual.matches_expected(&resolved_expected)
                {
                    return Err((resolved_expected, resolved_actual, None));
                }
                Ok(())
            })
        });
        if let Err((expected, actual, resolution_error)) = result {
            return Err(ConstraintConflict {
                expected,
                actual,
                initializer: provenance.map(|value| value.0),
                established: provenance.and_then(|value| value.1),
                contribution,
                resolution_error,
            });
        }
        Ok(())
    }

    fn constrain_nullable_seed_inner(
        &mut self,
        id: TypeVariableId,
        actual: &Type,
        contribution: Span,
        changes: &mut Vec<ConstraintChange>,
    ) -> Result<(), (Type, Type, Option<ConstraintResolutionError>)> {
        let expected = Type::Optional(Box::new(Type::Inference(id)));
        let inner = Type::Inference(id);
        let actual = self
            .resolve_with_changes(actual, 0, changes)
            .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?;
        let current = self
            .resolve_with_changes(&inner, 0, changes)
            .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?;
        if actual.matches_expected(&Type::Optional(Box::new(current.clone()))) {
            return Ok(());
        }
        let Type::Validated(validated) = &current else {
            return self.constrain_inner(&expected, &actual, contribution, false, 0, changes);
        };
        if current.contains_inference() {
            return self.constrain_inner(&expected, &actual, contribution, false, 0, changes);
        }
        let current_base = validated.base();
        let candidate = match &actual {
            Type::Null => None,
            Type::Optional(inner) => Some(inner.as_ref()),
            other => Some(other),
        };
        let Some(candidate) = candidate else {
            return self.constrain_inner(&expected, &actual, contribution, false, 0, changes);
        };
        if !has_anchor(candidate, false)
            .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?
        {
            return self.constrain_inner(&expected, &actual, contribution, false, 0, changes);
        }
        let widened = if current_base.matches_expected(candidate) {
            candidate
        } else {
            return self.constrain_inner(&expected, &actual, contribution, false, 0, changes);
        };
        let root = self
            .representative(id, Some(changes))
            .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?;
        let can_widen = matches!(
            self.variables.get(&root),
            Some(Variable::Representative(variable))
                if !variable.annotation
                    && matches!(
                        &variable.binding,
                        Some(Binding::Type { ty, .. }) if ty == &current
                    )
        );
        if !can_widen {
            return self.constrain_inner(&expected, &actual, contribution, false, 0, changes);
        }
        self.record(root, changes);
        let Some(Variable::Representative(variable)) = self.variables.get_mut(&root) else {
            unreachable!("representative lookup returns a root");
        };
        variable.binding = Some(Binding::Type {
            ty: widened.clone(),
            contribution,
        });
        Ok(())
    }

    fn constrain_inner(
        &mut self,
        expected: &Type,
        actual: &Type,
        contribution: Span,
        annotation: bool,
        depth: usize,
        changes: &mut Vec<ConstraintChange>,
    ) -> Result<(), (Type, Type, Option<ConstraintResolutionError>)> {
        let mut pending = vec![(expected.clone(), actual.clone(), depth)];
        'pairs: while let Some((expected, actual, depth)) = pending.pop() {
            if depth > MAX_TYPE_DEPTH {
                return Err((
                    expected,
                    actual,
                    Some(ConstraintResolutionError::TypeDepth),
                ));
            }
            self.counters
                .unifications
                .fetch_add(1, Ordering::Relaxed);
            self.counters
                .worklist_wakeups
                .fetch_add(1, Ordering::Relaxed);
            match (&expected, &actual) {
                (Type::Inference(id), Type::Record(fields))
                    if self.record_binding(*id).ok().flatten().is_some() =>
                {
                    let record = self
                        .record_binding(*id)
                        .expect("constraint variable belongs to this problem")
                        .expect("guard checked for a record binding");
                    self.unify_rows_inner(
                        &record.shape,
                        &RowShape {
                            fields: fields.as_ref().clone(),
                            tail: RowTail::Closed,
                        },
                        contribution,
                        depth,
                        changes,
                    )?;
                    continue;
                }
                (Type::Record(fields), Type::Inference(id))
                    if self.record_binding(*id).ok().flatten().is_some() =>
                {
                    let record = self
                        .record_binding(*id)
                        .expect("constraint variable belongs to this problem")
                        .expect("guard checked for a record binding");
                    self.unify_rows_inner(
                        &RowShape {
                            fields: fields.as_ref().clone(),
                            tail: RowTail::Closed,
                        },
                        &record.shape,
                        contribution,
                        depth,
                        changes,
                    )?;
                    continue;
                }
                (Type::Inference(left), Type::Inference(right)) => {
                    let left_root = self
                        .representative_root(*left)
                        .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?;
                    let right_root = self
                        .representative_root(*right)
                        .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?;
                    if left_root != right_root
                        && (self
                            .occurs_in_type_at(*left, &Type::Inference(*right), depth)
                            .map_err(|error| {
                                (expected.clone(), actual.clone(), Some(error))
                            })?
                            || self
                                .occurs_in_type_at(*right, &Type::Inference(*left), depth)
                                .map_err(|error| {
                                    (expected.clone(), actual.clone(), Some(error))
                                })?)
                    {
                        return Err((expected, actual, None));
                    }
                    let left_record = self
                        .record_binding(*left)
                        .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?;
                    let right_record = self
                        .record_binding(*right)
                        .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?;
                    #[cfg(test)]
                    {
                        let left_rigid = self.rigid_binding(*left).map_err(|error| {
                            (expected.clone(), actual.clone(), Some(error))
                        })?;
                        let right_rigid = self.rigid_binding(*right).map_err(|error| {
                            (expected.clone(), actual.clone(), Some(error))
                        })?;
                        if matches!((left_rigid, right_rigid), (Some(left), Some(right)) if left != right)
                            || (left_rigid.is_some() && right_record.is_some())
                            || (right_rigid.is_some() && left_record.is_some())
                        {
                            return Err((expected, actual, None));
                        }
                    }
                    if let (Some(left_record), Some(right_record)) =
                        (&left_record, &right_record)
                    {
                        self.unify_rows_inner(
                            &left_record.shape,
                            &right_record.shape,
                            contribution,
                            depth,
                            changes,
                        )?;
                    }
                }
                _ => {}
            }
            for (variable, value) in [(&expected, &actual), (&actual, &expected)] {
                if let Type::Inference(id) = variable
                    && self
                        .record_binding(*id)
                        .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?
                        .is_some()
                    && !matches!(value, Type::Inference(_) | Type::Record(_))
                {
                    if !has_anchor(value, annotation)
                        .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?
                    {
                        continue 'pairs;
                    }
                    return Err((expected.clone(), actual.clone(), None));
                }
            }
            #[cfg(test)]
            for (variable, value) in [(&expected, &actual), (&actual, &expected)] {
                if let Type::Inference(id) = variable
                    && self
                        .rigid_binding(*id)
                        .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?
                        .is_some()
                    && !matches!(value, Type::Inference(_))
                {
                    return Err((expected.clone(), actual.clone(), None));
                }
            }
            let contribution_to_variable =
                matches!(expected, Type::Inference(_)) && !self.annotation_variable(&expected);
            let expected = if matches!(expected, Type::Inference(_)) {
                self.resolve_with_changes(&expected, depth, changes)
                    .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?
            } else {
                expected
            };
            let actual = if matches!(actual, Type::Inference(_)) {
                self.resolve_with_changes(&actual, depth, changes)
                    .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?
            } else {
                actual
            };
            if let (Type::Inference(id), value) = (&expected, &actual)
                && self.record_binding(*id).ok().flatten().is_some()
                && !matches!(value, Type::Inference(_))
            {
                pending.push((expected, actual, depth));
                continue 'pairs;
            }
            if let (value, Type::Inference(id)) = (&expected, &actual)
                && self.record_binding(*id).ok().flatten().is_some()
                && !matches!(value, Type::Inference(_))
            {
                pending.push((expected, actual, depth));
                continue 'pairs;
            }
            #[cfg(test)]
            for (variable, value) in [(&expected, &actual), (&actual, &expected)] {
                if let Type::Inference(id) = variable
                    && self
                        .rigid_binding(*id)
                        .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?
                        .is_some()
                    && !matches!(value, Type::Inference(_))
                {
                    return Err((expected.clone(), actual.clone(), None));
                }
            }
            if contribution_to_variable
                && self
                    .variable_ids(&expected)
                    .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?
                    .is_empty()
                && self
                    .variable_ids(&actual)
                    .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?
                    .is_empty()
                && !actual.matches_invariant(&expected)
            {
                return Err((expected, actual, None));
            }
            if expected == actual {
                continue;
            }
            match (&expected, &actual) {
                (Type::Inference(left), Type::Inference(right)) => {
                    self.union(*left, *right, changes)
                        .map_err(|error| (expected, actual, Some(error)))?;
                }
                (Type::Inference(id), value) | (value, Type::Inference(id)) => {
                    if !has_anchor(value, annotation)
                        .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?
                    {
                        continue;
                    }
                    if self
                        .occurs_in_type_at(*id, value, depth)
                        .map_err(|error| (expected.clone(), actual.clone(), Some(error)))?
                    {
                        return Err((expected, actual, None));
                    }
                    self.bind(*id, value.clone(), contribution, annotation, changes)
                        .map_err(|error| (expected, actual, Some(error)))?;
                }
                (Type::Optional(_), Type::Null) => {}
                (Type::Optional(left), Type::Optional(right))
                | (Type::List(left), Type::List(right))
                | (Type::Stream(left), Type::Stream(right))
                | (Type::Set(left), Type::Set(right)) => {
                    pending.push(((**left).clone(), (**right).clone(), depth + 1))
                }
                (Type::Map(left_key, left_value), Type::Map(right_key, right_value)) => {
                    pending.push((
                        (**left_value).clone(),
                        (**right_value).clone(),
                        depth + 1,
                    ));
                    pending.push((
                        (**left_key).clone(),
                        (**right_key).clone(),
                        depth + 1,
                    ));
                }
                (Type::Optional(left), right) => {
                    pending.push(((**left).clone(), right.clone(), depth + 1));
                }
                (Type::Validated(left), Type::Validated(right))
                    if right.validation().implies(left.validation()) =>
                {
                    pending.push((left.base().clone(), right.base().clone(), depth + 1))
                }
                (left, Type::Validated(right))
                    if std::mem::discriminant(left) == std::mem::discriminant(right.base()) =>
                {
                    pending.push((left.clone(), right.base().clone(), depth + 1))
                }
                (Type::Result(left_ok, left_error), Type::Result(right_ok, right_error)) => {
                    pending.push((
                        (**left_error).clone(),
                        (**right_error).clone(),
                        depth + 1,
                    ));
                    pending.push((
                        (**left_ok).clone(),
                        (**right_ok).clone(),
                        depth + 1,
                    ));
                }
                (Type::Record(left), Type::Record(right)) => {
                    for (name, expected_field) in left.iter().rev() {
                        let Some(actual_field) = right.get(name) else {
                            return Err((expected, actual, None));
                        };
                        pending.push((
                            expected_field.clone(),
                            actual_field.clone(),
                            depth + 1,
                        ));
                    }
                }
                _ if matches!(expected, Type::Any | Type::Unknown | Type::Invalid)
                    || matches!(actual, Type::Unknown | Type::Invalid) => {}
                _ if actual.matches_expected(&expected) => {}
                _ => return Err((expected, actual, None)),
            }
        }
        Ok(())
    }

    fn annotation_variable(&self, ty: &Type) -> bool {
        let Type::Inference(id) = *ty else {
            return false;
        };
        let Ok(root) = self.representative_root(id) else {
            return false;
        };
        matches!(self.variables.get(&root), Some(Variable::Representative(value)) if value.annotation)
    }

    fn bind(
        &mut self,
        id: TypeVariableId,
        binding: Type,
        contribution: Span,
        annotation: bool,
        changes: &mut Vec<ConstraintChange>,
    ) -> Result<(), ConstraintResolutionError> {
        let root = self
            .representative(id, Some(changes))
            .expect("constraint variable belongs to this problem");
        let level = self.type_level(root)?;
        self.lower_levels_in_type(&binding, level, changes)?;
        self.record(root, changes);
        let Some(Variable::Representative(variable)) = self.variables.get_mut(&root) else {
            unreachable!("representative lookup returns a root");
        };
        if variable.binding.is_none() {
            variable.binding = Some(Binding::Type {
                ty: binding,
                contribution,
            });
        }
        variable.annotation |= annotation;
        Ok(())
    }

    /// Canonical partial substitution retains unresolved variable identities.
    /// Callers must separately reject unresolved material contracts.
    pub fn resolve(&self, ty: &Type) -> Result<Type, ConstraintResolutionError> {
        let mut resolved = ty.clone();
        self.resolve_in_place(&mut resolved)?;
        Ok(resolved)
    }

    pub(crate) fn resolve_constraint(
        &self,
        ty: &ConstraintType,
    ) -> Result<ConstraintType, ConstraintResolutionError> {
        let mut access = ResolutionAccess::Compress;
        self.resolve_constraint_depth(ty, 0, &mut FxHashSet::default(), &mut access)
    }

    #[cfg(test)]
    fn resolve_constraint_with_changes(
        &self,
        ty: &ConstraintType,
        changes: &mut Vec<ConstraintChange>,
    ) -> Result<ConstraintType, ConstraintResolutionError> {
        let mut access = ResolutionAccess::Journal(changes);
        self.resolve_constraint_depth(
            ty,
            0,
            &mut FxHashSet::default(),
            &mut access,
        )
    }

    #[cfg(test)]
    fn resolve_constraint_without_compression(
        &self,
        ty: &ConstraintType,
    ) -> Result<ConstraintType, ConstraintResolutionError> {
        let mut access = ResolutionAccess::ReadOnly;
        self.resolve_constraint_depth(ty, 0, &mut FxHashSet::default(), &mut access)
    }

    fn resolve_constraint_depth(
        &self,
        ty: &ConstraintType,
        depth: usize,
        seen: &mut FxHashSet<TypeVariableId>,
        access: &mut ResolutionAccess<'_>,
    ) -> Result<ConstraintType, ConstraintResolutionError> {
        if depth > MAX_TYPE_DEPTH {
            return Err(ConstraintResolutionError::TypeDepth);
        }
        match ty {
            #[cfg(test)]
            ConstraintType::Rigid(id) => Ok(ConstraintType::Rigid(*id)),
            ConstraintType::Record(row) => Ok(ConstraintType::Record(
                self.resolve_row_inner(row, depth, access)?,
            )),
            ConstraintType::Ground(Type::Inference(id)) => self.resolve_constraint_depth(
                &ConstraintType::Variable(*id),
                depth,
                seen,
                access,
            ),
            ConstraintType::Ground(Type::Record(fields)) => Ok(ConstraintType::Record(
                self.resolve_row_inner(
                    &RowShape {
                        fields: fields.as_ref().clone(),
                        tail: RowTail::Closed,
                    },
                    depth,
                    access,
                )?,
            )),
            ConstraintType::Ground(ty) => Ok(ConstraintType::Ground(
                self.resolve_type_with_access(ty, depth, access)?,
            )),
            ConstraintType::Variable(id) => {
                let root = self.representative_for_resolution(*id, access)?;
                let Some(Variable::Representative(variable)) = self.variables.get(&root) else {
                    unreachable!("representative lookup returns a root");
                };
                let canonical = variable.canonical;
                let binding = variable.binding.clone();
                if !seen.insert(root) {
                    return Err(ConstraintResolutionError::TypeDepth);
                }
                let result = match binding {
                    #[cfg(test)]
                    Some(Binding::Rigid { id, .. }) => ConstraintType::Rigid(id),
                    Some(Binding::Type {
                        ty: Type::Inference(id),
                        ..
                    }) => {
                        seen.remove(&root);
                        return self.resolve_constraint_depth(
                            &ConstraintType::Variable(id),
                            depth + 1,
                            seen,
                            access,
                        );
                    }
                    Some(Binding::Type {
                        ty: Type::Record(fields),
                        ..
                    }) => ConstraintType::Record(self.resolve_row_inner(
                        &RowShape {
                            fields: fields.as_ref().clone(),
                            tail: RowTail::Closed,
                        },
                        depth,
                        access,
                    )?),
                    Some(Binding::Type { ty, .. }) => ConstraintType::Ground(
                        self.resolve_type_with_access(&ty, depth, access)?,
                    ),
                    Some(Binding::Record(record)) => ConstraintType::Record(
                        self.resolve_row_inner(&record.shape, depth, access)?,
                    ),
                    None => ConstraintType::Variable(canonical),
                };
                seen.remove(&root);
                Ok(result)
            }
        }
    }

    fn resolve_with_changes(
        &self,
        ty: &Type,
        depth: usize,
        changes: &mut Vec<ConstraintChange>,
    ) -> Result<Type, ConstraintResolutionError> {
        let mut access = ResolutionAccess::Journal(changes);
        self.resolve_type_with_access(ty, depth, &mut access)
    }

    fn resolve_type_with_access(
        &self,
        ty: &Type,
        depth: usize,
        access: &mut ResolutionAccess<'_>,
    ) -> Result<Type, ConstraintResolutionError> {
        let mut resolved = ty.clone();
        self.resolve_depth_inner(&mut resolved, depth, access)?;
        Ok(resolved)
    }

    /// `resolve` without rebuilding the type: only inference identities are
    /// rewritten. On error the type may be partially rewritten.
    pub fn resolve_in_place(&self, ty: &mut Type) -> Result<(), ConstraintResolutionError> {
        self.resolve_depth(ty, 0)
    }

    fn resolve_depth(&self, ty: &mut Type, depth: usize) -> Result<(), ConstraintResolutionError> {
        let mut access = ResolutionAccess::Compress;
        self.resolve_depth_inner(ty, depth, &mut access)
    }

    fn resolve_depth_inner(
        &self,
        ty: &mut Type,
        depth: usize,
        access: &mut ResolutionAccess<'_>,
    ) -> Result<(), ConstraintResolutionError> {
        if depth > MAX_TYPE_DEPTH {
            return Err(ConstraintResolutionError::TypeDepth);
        }
        let record_needs_rewrite = match ty {
            Type::Record(_) => has_inference(ty, depth)?,
            _ => false,
        };
        match ty {
            Type::Inference(id) => {
                let root = self.representative_for_resolution(*id, access)?;
                let Some(Variable::Representative(variable)) = self.variables.get(&root) else {
                    unreachable!("representative lookup returns a root");
                };
                match &variable.binding {
                    Some(Binding::Type { ty: bound_ty, .. }) => {
                        let mut resolved = bound_ty.clone();
                        self.resolve_depth_inner(
                            &mut resolved,
                            depth,
                            access,
                        )?;
                        *ty = resolved;
                    }
                    Some(Binding::Record(record)) => {
                        let row = self.resolve_row_inner(
                            &record.shape,
                            depth,
                            access,
                        )?;
                        if row.tail == RowTail::Closed {
                            *ty = Type::Record(Arc::new(row.fields));
                        } else {
                            *ty = Type::Inference(variable.canonical);
                        }
                    }
                    #[cfg(test)]
                    Some(Binding::Rigid { .. }) => *ty = Type::Inference(variable.canonical),
                    None => *ty = Type::Inference(variable.canonical),
                }
            }
            Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) | Type::Set(inner) => {
                self.resolve_depth_inner(
                    inner,
                    depth + 1,
                    access,
                )?
            }
            Type::Map(first, second) | Type::Result(first, second) => {
                self.resolve_depth_inner(
                    first,
                    depth + 1,
                    access,
                )?;
                self.resolve_depth_inner(
                    second,
                    depth + 1,
                    access,
                )?;
            }
            Type::Record(fields) if record_needs_rewrite => {
                for field in Arc::make_mut(fields).values_mut() {
                    self.resolve_depth_inner(
                        field,
                        depth + 1,
                        access,
                    )?;
                }
            }
            Type::Record(_) => {}
            Type::Union(members) => {
                for member in members {
                    self.resolve_depth_inner(
                        member,
                        depth + 1,
                        access,
                    )?;
                }
            }
            Type::Validated(validated) => {
                validated.rewrite_base(|base| {
                    self.resolve_depth_inner(
                        base,
                        depth + 1,
                        access,
                    )
                })?
            }
            Type::Module(exports) if module_has_inference(exports, depth)? => {
                for export in Arc::make_mut(exports).exports.values_mut() {
                    match export {
                        ModuleExportType::Value { ty, .. } => {
                            self.resolve_depth_inner(
                                ty,
                                depth + 1,
                                access,
                            )?
                        }
                        ModuleExportType::Pure { sig, .. } | ModuleExportType::Proc { sig, .. } => {
                            self.resolve_callable_depth_inner(
                                sig,
                                depth + 1,
                                access,
                            )?
                        }
                        ModuleExportType::GenericPure { .. } => {}
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn resolve_callable(
        &self,
        signature: &CallableType,
    ) -> Result<CallableType, ConstraintResolutionError> {
        let mut signature = signature.clone();
        self.resolve_callable_depth(&mut signature, 0)?;
        Ok(signature)
    }

    fn resolve_callable_depth(
        &self,
        signature: &mut CallableType,
        depth: usize,
    ) -> Result<(), ConstraintResolutionError> {
        let mut access = ResolutionAccess::Compress;
        self.resolve_callable_depth_inner(signature, depth, &mut access)
    }

    fn resolve_callable_depth_inner(
        &self,
        signature: &mut CallableType,
        depth: usize,
        access: &mut ResolutionAccess<'_>,
    ) -> Result<(), ConstraintResolutionError> {
        for parameter in &mut signature.params {
            self.resolve_depth_inner(
                &mut parameter.ty,
                depth + 1,
                access,
            )?;
        }
        self.resolve_depth_inner(
            &mut signature.return_ty,
            depth + 1,
            access,
        )
    }

    pub fn unresolved(&self, ty: &Type) -> Result<Vec<Span>, ConstraintResolutionError> {
        let resolved = self.resolve(ty)?;
        let mut origins = self
            .variable_ids(&resolved)?
            .into_iter()
            .map(|id| {
                let root = self
                    .representative_root(id)
                    .expect("variable_ids returns local representatives");
                let Some(Variable::Representative(variable)) = self.variables.get(&root) else {
                    unreachable!("representative lookup returns a root");
                };
                variable.origin
            })
            .collect::<Vec<_>>();
        origins.sort_by_key(|span| (span.source_id, span.start(), span.end()));
        origins.dedup();
        Ok(origins)
    }

    fn variable_ids(
        &self,
        ty: &Type,
    ) -> Result<FxHashSet<TypeVariableId>, ConstraintResolutionError> {
        let mut variables = FxHashSet::default();
        let mut pending = vec![(ty, 0)];
        while let Some((ty, depth)) = pending.pop() {
            if depth > MAX_TYPE_DEPTH {
                return Err(ConstraintResolutionError::TypeDepth);
            }
            match ty {
                Type::Inference(id) => {
                    let root = self.representative_root(*id)?;
                    let Some(Variable::Representative(variable)) = self.variables.get(&root) else {
                        unreachable!("representative lookup returns a root");
                    };
                    variables.insert(variable.canonical);
                }
                Type::List(inner)
                | Type::Optional(inner)
                | Type::Stream(inner)
                | Type::Set(inner) => pending.push((inner, depth + 1)),
                Type::Map(key, value) => {
                    pending.push((key, depth + 1));
                    pending.push((value, depth + 1));
                }
                Type::Result(ok, error) => {
                    pending.push((ok, depth + 1));
                    pending.push((error, depth + 1));
                }
                Type::Record(fields) => pending.extend(fields.values().map(|ty| (ty, depth + 1))),
                Type::Union(members) => pending.extend(members.iter().map(|ty| (ty, depth + 1))),
                Type::Validated(validated) => pending.push((validated.base(), depth + 1)),
                Type::Module(exports) => {
                    for export in exports.values() {
                        match export {
                            ModuleExportType::Value { ty, .. } => pending.push((ty, depth + 1)),
                            ModuleExportType::Pure { sig, .. }
                            | ModuleExportType::Proc { sig, .. } => {
                                pending.push((&sig.return_ty, depth + 1));
                                pending.extend(
                                    sig.params
                                        .iter()
                                        .map(|parameter| (&parameter.ty, depth + 1)),
                                );
                            }
                            ModuleExportType::GenericPure { .. } => {}
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(variables)
    }

    fn occurs_in_type_at(
        &self,
        id: TypeVariableId,
        ty: &Type,
        depth: usize,
    ) -> Result<bool, ConstraintResolutionError> {
        self.occurs_in_nodes(id, [OccursNode::Type(ty, depth)])
    }

    fn occurs_in_row(
        &self,
        id: TypeVariableId,
        row: &RowShape,
    ) -> Result<bool, ConstraintResolutionError> {
        self.occurs_in_nodes(
            id,
            row.fields
                .values()
                .map(|ty| OccursNode::Type(ty, 1))
                .chain(std::iter::once(OccursNode::Row(row.tail.clone(), 0))),
        )
    }

    fn occurs_in_nodes<'a>(
        &self,
        id: TypeVariableId,
        nodes: impl IntoIterator<Item = OccursNode<'a>>,
    ) -> Result<bool, ConstraintResolutionError> {
        let root = self.representative_root(id)?;
        let mut pending = nodes.into_iter().collect::<Vec<_>>();
        let mut visited_types = FxHashSet::default();
        let mut visited_rows = FxHashSet::default();
        while let Some(node) = pending.pop() {
            self.counters.occurs_visits.fetch_add(1, Ordering::Relaxed);
            match node {
                OccursNode::Type(ty, depth) => {
                    if depth > MAX_TYPE_DEPTH {
                        return Err(ConstraintResolutionError::TypeDepth);
                    }
                    match ty {
                        Type::Inference(other) => {
                            let other_root = self.representative_root(*other)?;
                            if other_root == root {
                                return Ok(true);
                            }
                            if visited_types.insert(other_root) {
                                let Some(Variable::Representative(variable)) =
                                    self.variables.get(&other_root)
                                else {
                                    unreachable!("representative lookup returns a root");
                                };
                                match &variable.binding {
                                    Some(Binding::Type { ty, .. }) => {
                                        pending.push(OccursNode::Type(ty, depth));
                                    }
                                    Some(Binding::Record(record)) => {
                                        pending.extend(record.shape.fields.values().map(|ty| {
                                            OccursNode::Type(ty, depth + 1)
                                        }));
                                        pending.push(OccursNode::Row(
                                            record.shape.tail.clone(),
                                            depth,
                                        ));
                                    }
                                    #[cfg(test)]
                                    Some(Binding::Rigid { .. }) => {}
                                    None => {}
                                }
                            }
                        }
                        Type::List(inner)
                        | Type::Stream(inner)
                        | Type::Optional(inner)
                        | Type::Set(inner) => {
                            pending.push(OccursNode::Type(inner, depth + 1));
                        }
                        Type::Map(key, value) | Type::Result(key, value) => {
                            pending.push(OccursNode::Type(key, depth + 1));
                            pending.push(OccursNode::Type(value, depth + 1));
                        }
                        Type::Record(fields) => {
                            pending.extend(
                                fields
                                    .values()
                                    .map(|field| OccursNode::Type(field, depth + 1)),
                            );
                        }
                        Type::Module(exports) => {
                            for export in exports.values() {
                                match export {
                                    ModuleExportType::Value { ty, .. } => {
                                        pending.push(OccursNode::Type(ty, depth + 1));
                                    }
                                    ModuleExportType::Pure { sig, .. }
                                    | ModuleExportType::Proc { sig, .. } => {
                                        pending.push(OccursNode::Type(
                                            &sig.return_ty,
                                            depth + 1,
                                        ));
                                        pending.extend(sig.params.iter().map(|parameter| {
                                            OccursNode::Type(&parameter.ty, depth + 1)
                                        }));
                                    }
                                    ModuleExportType::GenericPure { .. } => {}
                                }
                            }
                        }
                        Type::Union(members) => {
                            pending.extend(
                                members
                                    .iter()
                                    .map(|member| OccursNode::Type(member, depth + 1)),
                            );
                        }
                        Type::Callable(callable) => {
                            pending.push(OccursNode::Type(
                                &callable.sig.return_ty,
                                depth + 1,
                            ));
                            pending.extend(callable.sig.params.iter().map(|parameter| {
                                OccursNode::Type(&parameter.ty, depth + 1)
                            }));
                        }
                        Type::Validated(validated) => {
                            pending.push(OccursNode::Type(validated.base(), depth + 1));
                        }
                        _ => {}
                    }
                }
                OccursNode::Row(tail, depth) => {
                    if depth > MAX_TYPE_DEPTH {
                        return Err(ConstraintResolutionError::TypeDepth);
                    }
                    let RowTail::Variable(id) = tail else {
                        continue;
                    };
                    let row_root = self.row_representative_root(id)?;
                    if !visited_rows.insert(row_root) {
                        continue;
                    }
                    let Some(RowVariable::Representative(variable)) = self.rows.get(&row_root)
                    else {
                        unreachable!("row representative lookup returns a root");
                    };
                    if let Some(binding) = &variable.binding {
                        pending.extend(binding.shape.fields.values().map(|ty| {
                            OccursNode::Type(ty, depth + 1)
                        }));
                        pending.push(OccursNode::Row(binding.shape.tail.clone(), depth));
                    }
                }
            }
        }
        Ok(false)
    }

    fn provenance(&self, ty: &Type) -> Option<(Span, Option<Span>)> {
        let mut earliest = None;
        for id in self.variable_ids(ty).ok()? {
            let root = self.representative_root(id).ok()?;
            let Some(Variable::Representative(variable)) = self.variables.get(&root) else {
                return None;
            };
            let candidate = (
                variable.earliest_origin,
                variable.binding.as_ref().map(|binding| match binding {
                    Binding::Type { contribution, .. } => *contribution,
                    Binding::Record(record) => record.contribution,
                    #[cfg(test)]
                    Binding::Rigid { contribution, .. } => *contribution,
                }),
                variable.canonical,
            );
            if earliest.is_none_or(|value: (Span, Option<Span>, TypeVariableId)| {
                (
                    candidate.0.source_id,
                    candidate.0.start(),
                    candidate.0.end(),
                    candidate.2,
                ) < (value.0.source_id, value.0.start(), value.0.end(), value.2)
            }) {
                earliest = Some(candidate);
            }
        }
        earliest.map(|(origin, contribution, _)| (origin, contribution))
    }

    fn representative_root(
        &self,
        id: TypeVariableId,
    ) -> Result<TypeVariableId, ConstraintResolutionError> {
        let mut current = id;
        let mut seen = FxHashSet::default();
        for _ in 0..=self.variables.len() {
            if !seen.insert(current) {
                return Err(ConstraintResolutionError::TypeDepth);
            }
            match self.variables.get(&current) {
                Some(Variable::Representative(_)) => return Ok(current),
                Some(Variable::Parent(parent)) => {
                    self.counters
                        .representative_hops
                        .fetch_add(1, Ordering::Relaxed);
                    current = TypeVariableId(parent.load(Ordering::Relaxed));
                }
                None => return Err(ConstraintResolutionError::ForeignVariable),
            }
        }
        Err(ConstraintResolutionError::TypeDepth)
    }

    fn representative(
        &self,
        id: TypeVariableId,
        mut changes: Option<&mut Vec<ConstraintChange>>,
    ) -> Result<TypeVariableId, ConstraintResolutionError> {
        let root = self.representative_root(id)?;
        let mut current = id;
        while current != root {
            let Some(Variable::Parent(parent)) = self.variables.get(&current) else {
                unreachable!("non-root representatives have parents");
            };
            let next = TypeVariableId(parent.load(Ordering::Relaxed));
            if next != root {
                if let Some(changes) = changes.as_deref_mut() {
                    self.record(current, changes);
                }
                let Some(Variable::Parent(parent)) = self.variables.get(&current) else {
                    unreachable!("non-root representatives have parents");
                };
                parent.store(root.0, Ordering::Relaxed);
                self.counters
                    .compression_writes
                    .fetch_add(1, Ordering::Relaxed);
            }
            current = next;
        }
        Ok(root)
    }

    fn representative_for_resolution(
        &self,
        id: TypeVariableId,
        access: &mut ResolutionAccess<'_>,
    ) -> Result<TypeVariableId, ConstraintResolutionError> {
        match access {
            #[cfg(test)]
            ResolutionAccess::ReadOnly => self.representative_root(id),
            ResolutionAccess::Compress if self.trial_changes.is_some() => self.representative_root(id),
            ResolutionAccess::Compress => self.representative(id, None),
            ResolutionAccess::Journal(changes) => self.representative(id, Some(&mut **changes)),
        }
    }

    fn record(&self, id: TypeVariableId, changes: &mut Vec<ConstraintChange>) {
        changes.push(ConstraintChange::Type(
            id,
            self.variables
                .get(&id)
                .expect("constraint variable belongs to this problem")
                .clone(),
        ));
    }

    fn record_row(&self, id: RowVariableId, changes: &mut Vec<ConstraintChange>) {
        changes.push(ConstraintChange::Row(id, self.rows.get(&id).cloned()));
    }

    fn rollback(&mut self, changes: Vec<ConstraintChange>) {
        self.counters
            .rollback_changes
            .fetch_add(changes.len() as u64, Ordering::Relaxed);
        for change in changes.into_iter().rev() {
            match change {
                ConstraintChange::Type(id, previous) => {
                    self.variables.insert(id, previous);
                }
                ConstraintChange::FreshType(id) => {
                    self.variables.remove(&id);
                }
                ConstraintChange::Row(id, Some(previous)) => {
                    self.rows.insert(id, previous);
                }
                ConstraintChange::Row(id, None) => {
                    self.rows.remove(&id);
                }
            }
        }
    }

    fn union(
        &mut self,
        left: TypeVariableId,
        right: TypeVariableId,
        changes: &mut Vec<ConstraintChange>,
    ) -> Result<(), ConstraintResolutionError> {
        let left_root = self
            .representative(left, Some(changes))
            .expect("constraint variable belongs to this problem");
        let right_root = self
            .representative(right, Some(changes))
            .expect("constraint variable belongs to this problem");
        if left_root == right_root {
            return Ok(());
        }
        let (
            left_rank,
            left_level,
            left_canonical,
            left_origin,
            left_earliest,
            left_annotation,
            left_binding,
        ) =
            match self.variables.get(&left_root) {
                Some(Variable::Representative(variable)) => (
                    variable.rank,
                    variable.level,
                    variable.canonical,
                    variable.origin,
                    variable.earliest_origin,
                    variable.annotation,
                    variable.binding.clone(),
                ),
                _ => unreachable!("representative lookup returns roots"),
            };
        let (
            right_rank,
            right_level,
            right_canonical,
            right_origin,
            right_earliest,
            right_annotation,
            right_binding,
        ) =
            match self.variables.get(&right_root) {
                Some(Variable::Representative(variable)) => (
                    variable.rank,
                    variable.level,
                    variable.canonical,
                    variable.origin,
                    variable.earliest_origin,
                    variable.annotation,
                    variable.binding.clone(),
                ),
                _ => unreachable!("representative lookup returns roots"),
            };
        assert!(!matches!(&left_binding, Some(Binding::Type { .. })));
        assert!(!matches!(&right_binding, Some(Binding::Type { .. })));
        let attach_left = left_rank < right_rank
            || (left_rank == right_rank && right_root < left_root);
        let (root, child, root_rank, child_rank) = if attach_left {
            (right_root, left_root, right_rank, left_rank)
        } else {
            (left_root, right_root, left_rank, right_rank)
        };
        let canonical = left_canonical.min(right_canonical);
        let origin = if left_canonical <= right_canonical {
            left_origin
        } else {
            right_origin
        };
        let earliest_origin = earlier_span(left_earliest, right_earliest);
        let annotation = left_annotation || right_annotation;
        let binding = if left_canonical <= right_canonical {
            left_binding.or(right_binding)
        } else {
            right_binding.or(left_binding)
        };
        self.record(child, changes);
        self.variables
            .insert(child, Variable::Parent(AtomicU64::new(root.0)));
        self.record(root, changes);
        let level = left_level.min(right_level);
        {
            let Some(Variable::Representative(root_value)) = self.variables.get_mut(&root) else {
                unreachable!("union root remains a representative");
            };
            if root_rank == child_rank {
                root_value.rank += 1;
            }
            root_value.canonical = canonical;
            root_value.level = level;
            root_value.origin = origin;
            root_value.earliest_origin = earliest_origin;
            root_value.annotation = annotation;
            root_value.binding = binding.clone();
        }
        match binding {
            Some(Binding::Type { ty, .. }) => self.lower_levels_in_type(&ty, level, changes),
            Some(Binding::Record(record)) => {
                self.lower_levels_in_row(&record.shape, level, changes)
            }
            #[cfg(test)]
            Some(Binding::Rigid { .. }) => Ok(()),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
fn variable_owned_payload_bytes(variable: &Variable) -> usize {
    match variable {
        Variable::Representative(representative) => match &representative.binding {
            Some(Binding::Type { ty, .. }) => type_owned_payload_bytes(ty),
            Some(Binding::Record(record)) => row_shape_owned_payload_bytes(&record.shape),
            Some(Binding::Rigid { .. }) | None => 0,
        },
        Variable::Parent(_) => 0,
    }
}

#[cfg(test)]
fn row_owned_payload_bytes(row: &RowVariable) -> usize {
    match row {
        RowVariable::Representative(representative) => {
            let lacks_bytes = representative.lacks.len()
                .saturating_mul(std::mem::size_of::<crate::symbol::Name>());
            lacks_bytes.saturating_add(representative.binding.as_ref().map_or(0, |binding| {
                row_shape_owned_payload_bytes(&binding.shape)
            }))
        }
        RowVariable::Parent(_) => 0,
    }
}

#[cfg(test)]
fn row_shape_owned_payload_bytes(shape: &RowShape) -> usize {
    let entries = shape
        .fields
        .len()
        .saturating_mul(std::mem::size_of::<(crate::symbol::Name, Type)>());
    let fields = shape
        .fields
        .values()
        .map(type_owned_payload_bytes)
        .fold(0usize, usize::saturating_add);
    entries.saturating_add(fields)
}

#[cfg(test)]
fn type_owned_payload_bytes(ty: &Type) -> usize {
    use std::mem::{size_of, size_of_val};
    match ty {
        Type::List(inner)
        | Type::Stream(inner)
        | Type::Set(inner)
        | Type::Optional(inner) => {
            size_of::<Type>().saturating_add(type_owned_payload_bytes(inner))
        }
        Type::Map(key, value) | Type::Result(key, value) => {
            2usize
                .saturating_mul(size_of::<Type>())
                .saturating_add(type_owned_payload_bytes(key))
                .saturating_add(type_owned_payload_bytes(value))
        }
        Type::Union(members) => {
            members
                .capacity()
                .saturating_mul(size_of::<Type>())
                .saturating_add(
                    members
                        .iter()
                        .map(type_owned_payload_bytes)
                        .fold(0usize, usize::saturating_add),
                )
        }
        Type::Validated(validated) => {
            size_of_val(validated.as_ref())
                .saturating_add(type_owned_payload_bytes(validated.base()))
        }
        Type::Record(_) | Type::Module(_) | Type::Callable(_) => 0,
        _ => 0,
    }
}

fn earlier_span(left: Span, right: Span) -> Span {
    if (left.source_id, left.start(), left.end()) <= (right.source_id, right.start(), right.end()) {
        left
    } else {
        right
    }
}

fn row_type(row: &RowShape) -> Type {
    Type::Record(Arc::new(row.fields.clone()))
}

fn has_anchor(
    ty: &Type,
    annotation: bool,
) -> Result<bool, ConstraintResolutionError> {
    if matches!(ty, Type::Inference(_)) {
        return Ok(false);
    }
    let mut pending = vec![(ty, 0)];
    while let Some((ty, depth)) = pending.pop() {
        if depth > MAX_TYPE_DEPTH {
            return Err(ConstraintResolutionError::TypeDepth);
        }
        match ty {
            Type::Unknown | Type::Invalid => return Ok(false),
            Type::Inference(_) if annotation => return Ok(false),
            Type::Any | Type::ErasedRecord | Type::Null if !annotation => return Ok(false),
            Type::List(inner) | Type::Optional(inner) | Type::Stream(inner) | Type::Set(inner) => {
                pending.push((inner, depth + 1))
            }
            Type::Map(key, value) => {
                pending.push((key, depth + 1));
                pending.push((value, depth + 1));
            }
            Type::Result(ok, error) => {
                pending.push((ok, depth + 1));
                pending.push((error, depth + 1));
            }
            Type::Record(fields) => {
                if fields.is_empty() && !annotation {
                    return Ok(false);
                }
                pending.extend(fields.values().map(|ty| (ty, depth + 1)));
            }
            Type::Union(members) => pending.extend(members.iter().map(|ty| (ty, depth + 1))),
            Type::Validated(validated) => pending.push((validated.base(), depth + 1)),
            Type::Module(exports) => {
                for export in exports.values() {
                    match export {
                        ModuleExportType::Value { ty, .. } => pending.push((ty, depth + 1)),
                        ModuleExportType::Pure { sig, .. } | ModuleExportType::Proc { sig, .. } => {
                            pending.push((&sig.return_ty, depth + 1));
                            pending.extend(
                                sig.params
                                    .iter()
                                    .map(|parameter| (&parameter.ty, depth + 1)),
                            );
                        }
                        ModuleExportType::GenericPure { .. } => {}
                    }
                }
            }
            _ => {}
        }
    }
    Ok(true)
}

fn has_inference(ty: &Type, depth: usize) -> Result<bool, ConstraintResolutionError> {
    if depth > MAX_TYPE_DEPTH {
        return Err(ConstraintResolutionError::TypeDepth);
    }
    Ok(match ty {
        Type::Inference(_) => true,
        Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) | Type::Set(inner) => {
            has_inference(inner, depth + 1)?
        }
        Type::Map(first, second) | Type::Result(first, second) => {
            has_inference(first, depth + 1)? || has_inference(second, depth + 1)?
        }
        Type::Record(fields) => {
            for field in fields.values() {
                if has_inference(field, depth + 1)? {
                    return Ok(true);
                }
            }
            false
        }
        Type::Union(members) => {
            for member in members {
                if has_inference(member, depth + 1)? {
                    return Ok(true);
                }
            }
            false
        }
        Type::Validated(validated) => has_inference(validated.base(), depth + 1)?,
        Type::Module(exports) => module_has_inference(exports, depth)?,
        _ => false,
    })
}

fn module_has_inference(
    exports: &std::collections::BTreeMap<crate::symbol::Name, ModuleExportType>,
    depth: usize,
) -> Result<bool, ConstraintResolutionError> {
    for export in exports.values() {
        let found = match export {
            ModuleExportType::Value { ty, .. } => has_inference(ty, depth + 1)?,
            ModuleExportType::Pure { sig, .. } | ModuleExportType::Proc { sig, .. } => {
                let mut found = false;
                for parameter in &sig.params {
                    if has_inference(&parameter.ty, depth + 2)? {
                        found = true;
                        break;
                    }
                }
                found || has_inference(&sig.return_ty, depth + 2)?
            }
            ModuleExportType::GenericPure { .. } => false,
        };
        if found {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;

    fn span(start: usize) -> Span {
        Span::new(SourceId::new(0), start, start + 1)
    }

    fn variable_id(ty: &Type) -> TypeVariableId {
        let Type::Inference(id) = ty else {
            panic!("expected an inference variable")
        };
        *id
    }

    fn parent(constraints: &TypeConstraints, ty: &Type) -> Option<TypeVariableId> {
        match constraints.variables.get(&variable_id(ty)) {
            Some(Variable::Parent(parent)) => {
                Some(TypeVariableId(parent.load(Ordering::Relaxed)))
            }
            _ => None,
        }
    }

    fn rank(constraints: &TypeConstraints, ty: &Type) -> usize {
        let root = constraints
            .representative_root(variable_id(ty))
            .unwrap();
        let Some(Variable::Representative(representative)) = constraints.variables.get(&root) else {
            panic!("expected a representative")
        };
        representative.rank
    }

    fn union_balanced(constraints: &mut TypeConstraints, variables: &[Type], offset: usize) {
        for (left, right) in [(0, 1), (2, 3), (0, 2), (4, 5), (6, 7), (4, 6), (0, 4)] {
            constraints
                .constrain(&variables[left], &variables[right], span(offset + left))
                .unwrap();
        }
    }

    #[test]
    fn constraint_trials_restore_cells_rows_provenance_and_fresh_id_ownership() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            enum End { Match, Conflict, Panic }
            for end in [End::Match, End::Conflict, End::Panic] {
                let mut constraints = TypeConstraints::default();
                let _: &(dyn Send + Sync) = &constraints;
                let variables = (0..8).map(|index| constraints.fresh(span(index + 1))).collect::<Vec<_>>();
                union_balanced(&mut constraints, &variables, 20);
                let field = crate::symbol::Name::intern("field");
                let extra = crate::symbol::Name::intern("extra");
                let record = constraints.fresh(span(40));
                constraints.project_record_field(&record, field, 1, span(41)).unwrap();
                let before = constraints.clone();
                let before_metrics = constraints.metrics();
                let mut trial_ids = Vec::new();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| constraints.trial(|constraints| {
                    let fresh = constraints.fresh(span(50));
                    trial_ids.push(variable_id(&fresh));
                    constraints.fresh_row_at_level(span(51), 0, [extra]);
                    constraints.constrain(&fresh, &Type::Str, span(52))?;
                    constraints.constrain(&variables[7], &Type::Int, span(53))?;
                    constraints.constrain(&record, &Type::Record(Arc::new(BTreeMap::from([(field, Type::Int), (extra, Type::Bool)]))), span(54))?;
                    assert_eq!(constraints.resolve(&variables[7]), Ok(Type::Int));
                    match end {
                        End::Match => Ok(()),
                        End::Conflict => constraints.constrain(&Type::Int, &Type::Str, span(55)),
                        End::Panic => panic!("interrupt candidate trial"),
                    }
                })));
                match end {
                    End::Match => assert_eq!(result.unwrap(), Ok(true)),
                    End::Conflict => assert_eq!(result.unwrap(), Ok(false)),
                    End::Panic => assert!(result.is_err()),
                }
                assert!(constraints.trial_changes.is_none());
                assert_eq!(constraints.variables.len(), before.variables.len());
                assert_eq!(constraints.rows.len(), before.rows.len());
                for (id, variable) in &before.variables {
                    assert_eq!(format!("{:?}", constraints.variables.get(id).unwrap()), format!("{variable:?}"));
                }
                for (id, row) in &before.rows {
                    assert_eq!(format!("{:?}", constraints.rows.get(id).unwrap()), format!("{row:?}"));
                }
                let after = constraints.metrics();
                assert!(after.fresh_variables > before_metrics.fresh_variables);
                assert!(after.generated_constraints > before_metrics.generated_constraints);
                assert!(after.compression_writes > before_metrics.compression_writes);
                assert!(after.rollback_changes > before_metrics.rollback_changes);
                for id in trial_ids {
                    assert_eq!(constraints.resolve(&Type::Inference(id)), Err(ConstraintResolutionError::ForeignVariable));
                    assert_ne!(variable_id(&constraints.fresh(span(60))), id);
                }
            }
        });
    }

    #[test]
    fn constraint_trials_restore_an_in_flight_journal_on_panic() {
        let mut constraints = TypeConstraints::default();
        let earlier = constraints.fresh(span(1));
        let current = constraints.fresh(span(2));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| constraints.trial(|constraints| {
            constraints.constrain(&earlier, &Type::Str, span(3))?;
            let _: Result<(), ()> = constraints.apply_journaled(|constraints, changes| {
                constraints.constrain_inner(&current, &Type::Int, span(4), false, 0, changes).map_err(|_| ())?;
                panic!("interrupt active unification");
            });
            Ok(())
        })));
        assert!(result.is_err());
        assert_eq!(constraints.resolve(&earlier), Ok(earlier));
        assert_eq!(constraints.resolve(&current), Ok(current));
        assert!(constraints.trial_changes.is_none());
    }

    #[test]
    fn constraint_trial_work_follows_touched_cells_instead_of_unrelated_wide_state() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let fields = (0..64).map(|index| (crate::symbol::Name::intern(format!("field_{index}")), Type::Int)).collect::<BTreeMap<_, _>>();
            for touched in [1, 4, 16, 64] {
                let mut reference = None;
                for unrelated in [0, 8, 32] {
                    let mut constraints = TypeConstraints::default();
                    for index in 0..unrelated {
                        let variable = constraints.fresh(span(index + 1));
                        let tail = constraints.fresh_row(span(index + 40), fields.keys().copied());
                        constraints.bind_record(variable_id(&variable), &RowShape { fields: fields.clone(), tail: RowTail::Variable(tail) }, span(index + 80)).unwrap();
                    }
                    let variables = (0..touched).map(|index| constraints.fresh(span(index + 200))).collect::<Vec<_>>();
                    let baseline_clone_cells = constraints.variables.len() + constraints.rows.len();
                    let before = constraints.metrics();
                    assert_eq!(constraints.trial(|constraints| {
                        for variable in &variables {
                            constraints.constrain(variable, &Type::Int, span(300))?;
                        }
                        Ok(())
                    }), Ok(true));
                    let after = constraints.metrics();
                    let work = (
                        after.generated_constraints - before.generated_constraints,
                        after.unifications - before.unifications,
                        after.occurs_visits - before.occurs_visits,
                        after.row_field_visits - before.row_field_visits,
                        after.rollback_changes - before.rollback_changes,
                    );
                    if let Some(reference) = reference {
                        assert_eq!(work, reference);
                    } else {
                        reference = Some(work);
                    }
                    assert_eq!(work.0, touched as u64);
                    assert_eq!(work.3, 0);
                    assert!(work.4 > 0 && work.4 <= 4 * touched as u64);
                    assert!(variables.iter().all(|variable| constraints.resolve(variable).unwrap() == *variable));
                    println!("unrelated_wide_records={unrelated} touched={touched} previous_clone_cells={baseline_clone_cells} actual_trial_work={work:?}");
                }
            }
        });
    }

    #[test]
    fn aliases_share_one_solution_and_conflicts_retain_source_contributions() {
        let mut constraints = TypeConstraints::default();
        let seed = constraints.fresh(span(1));
        let alias = constraints.fresh(span(2));
        constraints.constrain(&alias, &seed, span(3)).unwrap();
        constraints.constrain(&seed, &Type::Path, span(4)).unwrap();
        assert_eq!(constraints.resolve(&alias).unwrap(), Type::Path);
        let error = constraints
            .constrain(&seed, &Type::Int, span(5))
            .unwrap_err();
        assert_eq!(error.initializer, Some(span(1)));
        assert_eq!(error.established, Some(span(4)));
        assert_eq!(error.contribution, span(5));
        assert_eq!(constraints.resolve(&seed).unwrap(), Type::Path);
        let alias_error = constraints
            .constrain(&alias, &Type::Int, span(6))
            .unwrap_err();
        assert_eq!(alias_error.initializer, Some(span(1)));
        assert_eq!(alias_error.established, Some(span(4)));
    }

    #[test]
    fn alias_chain_of_400_variables_resolves_to_ground_type() {
        let mut constraints = TypeConstraints::default();
        let span = Span::at(SourceId::new(0), 0);
        let variables = (0..400)
            .map(|_| constraints.fresh(span))
            .collect::<Vec<_>>();
        for index in (1..variables.len()).rev() {
            constraints
                .constrain(&variables[index], &variables[index - 1], span)
                .unwrap();
        }
        constraints
            .constrain(&variables[0], &Type::Int, span)
            .unwrap();
        assert_eq!(constraints.resolve(&variables[399]), Ok(Type::Int));
        let metrics = constraints.metrics();
        assert_eq!(metrics.fresh_variables, 400);
        assert_eq!(metrics.generated_constraints, 400);
        assert_eq!(metrics.representative_hops, 7_560);
        assert_eq!(metrics.compression_writes, 0);
        assert_eq!(metrics.unifications, 400);
    }

    #[test]
    fn alias_chain_metrics_scale_with_chain_length() {
        for (length, expected_hops) in [(1, 0), (4, 36), (16, 264), (64, 1_176)] {
            let mut constraints = TypeConstraints::default();
            let variables = (0..length)
                .map(|index| constraints.fresh(span(index)))
                .collect::<Vec<_>>();
            for index in (1..length).rev() {
                constraints
                    .constrain(&variables[index], &variables[index - 1], span(index))
                    .unwrap();
            }
            constraints
                .constrain(&variables[0], &Type::Int, span(length))
                .unwrap();
            assert_eq!(constraints.resolve(variables.last().unwrap()), Ok(Type::Int));

            let metrics = constraints.metrics();
            assert_eq!(metrics.fresh_variables, length as u64);
            assert_eq!(metrics.generated_constraints, length as u64);
            assert_eq!(metrics.unifications, length as u64);
            assert_eq!(metrics.representative_hops, expected_hops);
            assert_eq!(metrics.compression_writes, 0);
        }
    }

    #[test]
    fn row_field_visit_metrics_count_both_operand_scans() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            for width in [1, 4, 16, 64] {
                let fields = (0..width)
                    .map(|index| {
                        (
                            crate::symbol::Name::intern(format!("field_{index}")),
                            Type::Int,
                        )
                    })
                    .collect::<BTreeMap<_, _>>();
                let row = RowShape {
                    fields: fields.clone(),
                    tail: RowTail::Closed,
                };
                let mut constraints = TypeConstraints::default();
                constraints.constrain_rows(&row, &row, span(width)).unwrap();

                let metrics = constraints.metrics();
                assert_eq!(metrics.row_field_visits, 2 * width as u64);
                assert_eq!(metrics.generated_constraints, 1);
                assert_eq!(metrics.unifications, width as u64 + 1);
                assert_eq!(metrics.worklist_wakeups, width as u64 + 1);

                let disjoint = (width..2 * width)
                    .map(|index| {
                        (
                            crate::symbol::Name::intern(format!("field_{index}")),
                            Type::Int,
                        )
                    })
                    .collect::<BTreeMap<_, _>>();
                let mut rejected = TypeConstraints::default();
                assert!(rejected
                    .constrain_rows(
                        &row,
                        &RowShape {
                            fields: disjoint,
                            tail: RowTail::Closed,
                        },
                        span(width + 100),
                    )
                    .is_err());
                assert_eq!(rejected.metrics().row_field_visits, 2 * width as u64);
            }
        });
    }

    #[test]
    fn generic_call_resolution_counts_fresh_evidence_separately_from_scheme_instantiation() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let parameter = SchemeTypeParameterId(0);
            let callable = CallableScheme::new(
                vec![parameter],
                Vec::new(),
                vec![super::super::schemes::SchemeParameter {
                    name: crate::symbol::Name::intern("value"),
                    ty: SchemeType::Parameter(parameter),
                    schema_expectation: None,
                    defaulted: false,
                    rest: false,
                }],
                SchemeType::Parameter(parameter),
                None,
                false,
            )
            .unwrap();
            let monomorphic = CallableScheme::new(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SchemeType::Ground(Type::Int),
                None,
                false,
            )
            .unwrap();
            let mut constraints = TypeConstraints::default();

            let integer = constraints
                .instantiate_callable(&callable, 1, span(1))
                .unwrap();
            let text = constraints
                .instantiate_callable(&callable, 1, span(2))
                .unwrap();
            assert_ne!(integer.params[0], text.params[0]);
            constraints
                .constrain(&integer.params[0], &Type::Int, span(11))
                .unwrap();
            constraints
                .constrain(&text.params[0], &Type::Str, span(12))
                .unwrap();
            let integer_evidence = constraints
                .resolve_call_instantiation(&callable, &integer)
                .unwrap();
            let text_evidence = constraints
                .resolve_call_instantiation(&callable, &text)
                .unwrap();
            assert_eq!(
                integer_evidence.type_arguments().get(&parameter),
                Some(&SchemeType::Ground(Type::Int))
            );
            assert_eq!(
                text_evidence.type_arguments().get(&parameter),
                Some(&SchemeType::Ground(Type::Str))
            );

            let instantiated = constraints
                .instantiate_callable(&monomorphic, 1, span(3))
                .unwrap();
            constraints
                .resolve_call_instantiation(&monomorphic, &instantiated)
                .unwrap();

            let metrics = constraints.metrics();
            assert_eq!(metrics.scheme_instantiations, 3);
            assert_eq!(metrics.successful_generic_call_resolutions, 2);
        });
    }

    #[test]
    fn monomorphic_callable_does_not_generalize_absent_result_arms() {
        let result = Type::Result(Box::new(Type::Int), Box::new(Type::Error));
        let body = BTreeMap::from([(
            ExprId::from_index(0),
            Type::Result(Box::new(Type::Unknown), Box::new(Type::Error)),
        )]);
        let constraints = TypeConstraints::default();
        let (callable, body, tails, calls) = constraints
            .generalize_callable_body_with_calls(
                std::slice::from_ref(&result),
                &body,
                &BTreeMap::new(),
                &BTreeMap::new(),
                0,
            )
            .unwrap();
        assert!(callable.type_parameters.is_empty());
        assert!(callable.row_parameters.is_empty());
        assert_eq!(callable.types[0].closed_ground_type(), Some(result));
        assert!(body.is_empty());
        assert!(tails.is_empty());
        assert!(calls.is_empty());
    }

    #[test]
    fn callable_body_generalization_rejects_call_only_binders() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let parameter = SchemeTypeParameterId(0);
            let generic = CallableScheme::new(
                vec![parameter],
                Vec::new(),
                vec![super::super::schemes::SchemeParameter {
                    name: crate::symbol::Name::intern("value"),
                    ty: SchemeType::Parameter(parameter),
                    schema_expectation: None,
                    defaulted: false,
                    rest: false,
                }],
                SchemeType::Parameter(parameter),
                None,
                false,
            )
            .unwrap();
            let mut constraints = TypeConstraints::default();
            let instantiated = constraints
                .instantiate_callable(&generic, 1, span(30))
                .unwrap();
            let calls = BTreeMap::from([(
                span(31),
                (Arc::new(generic), instantiated),
            )]);
            let error = constraints
                .generalize_callable_body_with_calls(
                    &[Type::Int],
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                    &calls,
                    0,
                )
                .unwrap_err();
            assert!(matches!(
                error,
                SchemeOperationError::Build(SchemeBuildError::BodyTypeNotBound)
            ));

            let row_parameter = SchemeRowParameterId(1);
            let required = crate::symbol::Name::intern("required");
            let row_callable = CallableScheme::new(
                Vec::new(),
                vec![SchemeRowParameter {
                    id: row_parameter,
                    lacks: BTreeSet::from([required]),
                }],
                vec![super::super::schemes::SchemeParameter {
                    name: crate::symbol::Name::intern("value"),
                    ty: SchemeType::Record {
                        fields: BTreeMap::from([(
                            crate::symbol::Name::intern("required"),
                            SchemeType::Ground(Type::Int),
                        )]),
                        tail: SchemeRowTail::Parameter(row_parameter),
                    },
                    schema_expectation: None,
                    defaulted: false,
                    rest: false,
                }],
                SchemeType::Ground(Type::Int),
                None,
                false,
            )
            .unwrap();
            let instantiated = constraints
                .instantiate_callable(&row_callable, 1, span(32))
                .unwrap();
            let calls = BTreeMap::from([(
                span(33),
                (Arc::new(row_callable), instantiated),
            )]);
            let error = constraints
                .generalize_callable_body_with_calls(
                    &[Type::Int],
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                    &calls,
                    0,
                )
                .unwrap_err();
            assert!(matches!(
                error,
                SchemeOperationError::Build(SchemeBuildError::BodyTypeNotBound)
            ));
        });
    }

    #[test]
    fn constraint_type_ground_and_variable_preserve_operand_direction() {
        let expected = Type::Optional(Box::new(Type::Int));
        let actual = Type::Int;
        let mut adapted = TypeConstraints::default();
        let actual_variable = adapted.fresh(span(1));
        adapted.constrain(&actual_variable, &actual, span(2)).unwrap();
        let direct_actual = adapted.resolve(&actual_variable).unwrap();
        let direct = adapted
            .clone()
            .constrain(&expected, &direct_actual, span(3));
        assert!(direct.is_ok());
        assert!(adapted
            .constrain_types(
                &ConstraintType::Ground(expected),
                &ConstraintType::Variable(variable_id(&actual_variable)),
                span(4),
            )
            .is_ok());

        let expected = Type::Int;
        let actual = Type::Optional(Box::new(Type::Int));
        let mut adapted = TypeConstraints::default();
        let actual_variable = adapted.fresh(span(5));
        adapted.constrain(&actual_variable, &actual, span(6)).unwrap();
        let direct_actual = adapted.resolve(&actual_variable).unwrap();
        let direct = adapted
            .clone()
            .constrain(&expected, &direct_actual, span(7));
        assert!(direct.is_err());
        assert!(adapted
            .constrain_types(
                &ConstraintType::Ground(expected),
                &ConstraintType::Variable(variable_id(&actual_variable)),
                span(8),
            )
            .is_err());
    }

    #[test]
    fn constraint_type_ground_and_record_preserve_operand_direction() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let field = crate::symbol::Name::intern("field");
            let expected = Type::Record(Arc::new(BTreeMap::from([(
                field,
                Type::Optional(Box::new(Type::Int)),
            )])));
            let mut constraints = TypeConstraints::default();
            let actual_field = constraints.fresh(span(1));
            constraints
                .constrain(&actual_field, &Type::Int, span(2))
                .unwrap();
            let direct_actual = Type::Record(Arc::new(BTreeMap::from([(
                field,
                constraints.resolve(&actual_field).unwrap(),
            )])));
            let direct = constraints
                .clone()
                .constrain(&expected, &direct_actual, span(3));
            assert!(direct.is_ok());

            let actual_row = RowShape {
                fields: BTreeMap::from([(field, actual_field)]),
                tail: RowTail::Closed,
            };
            assert!(constraints
                .constrain_types(
                    &ConstraintType::Ground(expected),
                    &ConstraintType::Record(actual_row),
                    span(4),
                )
                .is_ok());
        });
    }

    #[test]
    fn failed_constraint_does_not_compress_nested_alias_paths() {
        let mut constraints = TypeConstraints::default();
        let outer = constraints.fresh(span(1));
        let mut aliases = (0..7)
            .map(|index| constraints.fresh(span(index + 2)))
            .collect::<Vec<_>>();
        let nested = constraints.fresh(span(9));
        constraints
            .constrain(&outer, &Type::List(Box::new(nested.clone())), span(10))
            .unwrap();
        aliases.push(nested.clone());
        union_balanced(&mut constraints, &aliases, 20);
        assert_eq!(parent(&constraints, &nested), Some(variable_id(&aliases[6])));
        assert_eq!(parent(&constraints, &aliases[6]), Some(variable_id(&aliases[4])));

        assert!(constraints
            .constrain_types(
                &ConstraintType::Variable(variable_id(&outer)),
                &ConstraintType::Rigid(RigidTypeId(94)),
                span(40),
            )
            .is_err());

        assert_eq!(parent(&constraints, &nested), Some(variable_id(&aliases[6])));
        assert_eq!(parent(&constraints, &aliases[6]), Some(variable_id(&aliases[4])));
    }

    #[test]
    fn flexible_row_tail_accepts_a_rigid_row_tail() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let field = crate::symbol::Name::intern("field");
            let mut scope = RigidRowScope;
            let rigid_tail = scope.fresh([field]);
            let mut constraints = TypeConstraints::default();
            let variable = constraints.fresh_row(span(1), [field]);
            let flexible = RowShape {
                fields: BTreeMap::new(),
                tail: RowTail::Variable(variable),
            };
            let rigid = RowShape {
                fields: BTreeMap::new(),
                tail: RowTail::Rigid(rigid_tail),
            };

            constraints.constrain_rows(&flexible, &rigid, span(2)).unwrap();
            assert_eq!(constraints.resolve_row(&flexible).unwrap(), rigid);
        });
    }

    #[test]
    fn rigid_row_tails_preserve_identity_and_reject_distinct_skolems() {
        let mut scope = RigidRowScope;
        let first = scope.fresh(std::iter::empty());
        let same = first.clone();
        let distinct = scope.fresh(std::iter::empty());
        let first_row = RowShape {
            fields: BTreeMap::new(),
            tail: RowTail::Rigid(first.clone()),
        };
        let same_row = RowShape {
            fields: BTreeMap::new(),
            tail: RowTail::Rigid(same),
        };
        let distinct_row = RowShape {
            fields: BTreeMap::new(),
            tail: RowTail::Rigid(distinct),
        };
        let mut constraints = TypeConstraints::default();

        constraints
            .constrain_rows(&first_row, &same_row, span(1))
            .unwrap();
        assert_eq!(constraints.resolve_row(&first_row).unwrap(), same_row);
        assert!(constraints
            .constrain_rows(&first_row, &distinct_row, span(2))
            .is_err());
    }

    #[test]
    fn rigid_row_lacks_are_fixed_in_both_operand_orientations() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let field = crate::symbol::Name::intern("field");
            let extra = crate::symbol::Name::intern("extra");
            for rigid_first in [false, true] {
                let mut scope = RigidRowScope;
                let rigid_tail = scope.fresh([field]);
                let mut constraints = TypeConstraints::default();
                let flexible_tail = constraints.fresh_row(span(1), [field, extra]);
                let flexible = RowShape {
                    fields: BTreeMap::new(),
                    tail: RowTail::Variable(flexible_tail),
                };
                let rigid = RowShape {
                    fields: BTreeMap::new(),
                    tail: RowTail::Rigid(rigid_tail),
                };
                let (expected, actual) = if rigid_first {
                    (&rigid, &flexible)
                } else {
                    (&flexible, &rigid)
                };

                assert!(constraints
                    .constrain_rows(expected, actual, span(2))
                    .is_err());
                assert_eq!(constraints.resolve_row(&flexible).unwrap(), flexible);
                assert_eq!(constraints.resolve_row(&rigid).unwrap(), rigid);
            }
        });
    }

    #[test]
    fn rigid_row_lacks_reject_explicit_fields_in_both_operand_orientations() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let field = crate::symbol::Name::intern("field");
            for rigid_first in [false, true] {
                let mut scope = RigidRowScope;
                let rigid_tail = scope.fresh([field]);
                let mut constraints = TypeConstraints::default();
                let flexible_tail = constraints.fresh_row(span(1), [field]);
                let flexible = RowShape {
                    fields: BTreeMap::new(),
                    tail: RowTail::Variable(flexible_tail),
                };
                let rigid = RowShape {
                    fields: BTreeMap::from([(field, Type::Int)]),
                    tail: RowTail::Rigid(rigid_tail),
                };
                let (expected, actual) = if rigid_first {
                    (&rigid, &flexible)
                } else {
                    (&flexible, &rigid)
                };

                assert!(constraints
                    .constrain_rows(expected, actual, span(2))
                    .is_err());
                assert_eq!(constraints.resolve_row(&flexible).unwrap(), flexible);
                assert_eq!(constraints.resolve_row(&rigid).unwrap(), rigid);
            }
        });
    }

    #[test]
    fn indirect_recursive_type_equations_are_rejected() {
        let mut constraints = TypeConstraints::default();
        let first = constraints.fresh(span(1));
        let alias = constraints.fresh(span(2));
        constraints.constrain(&first, &alias, span(3)).unwrap();

        let recursive = Type::List(Box::new(first.clone()));
        let conflict = constraints
            .constrain(&alias, &recursive, span(4))
            .unwrap_err();
        assert_eq!(conflict.expected, first);
        assert_eq!(conflict.actual, recursive);
        assert_eq!(conflict.initializer, Some(span(1)));
        assert_eq!(conflict.established, None);
        assert_eq!(conflict.contribution, span(4));
        assert_eq!(conflict.resolution_error, None);
        assert_eq!(constraints.resolve(&first).unwrap(), first);
        assert_eq!(constraints.resolve(&alias).unwrap(), first);
    }

    #[test]
    fn flexible_type_variables_accept_rigid_type_identities() {
        let mut constraints = TypeConstraints::default();
        let variable = constraints.fresh(span(1));
        let rigid = ConstraintType::Rigid(RigidTypeId(7));
        constraints
            .constrain_types(
                &ConstraintType::Variable(variable_id(&variable)),
                &rigid,
                span(2),
            )
            .unwrap();
        assert!(constraints
            .constrain_types(
                &ConstraintType::Variable(variable_id(&variable)),
                &rigid,
                span(3),
            )
            .is_ok());
        let alias = constraints.fresh(span(7));
        constraints
            .constrain_types(
                &ConstraintType::Variable(variable_id(&variable)),
                &ConstraintType::Variable(variable_id(&alias)),
                span(8),
            )
            .unwrap();
        assert_eq!(
            constraints.resolve_constraint(&ConstraintType::Variable(variable_id(&alias))),
            Ok(rigid.clone())
        );
        let ground_conflict = constraints
            .constrain_types(
                &ConstraintType::Variable(variable_id(&variable)),
                &ConstraintType::Ground(Type::Int),
                span(4),
            )
            .unwrap_err();
        assert_eq!(ground_conflict.expected, rigid);
        assert_eq!(ground_conflict.actual, ConstraintType::Ground(Type::Int));
        assert_eq!(ground_conflict.initializer, Some(span(1)));
        assert_eq!(ground_conflict.established, Some(span(2)));
        assert_eq!(ground_conflict.contribution, span(4));
        assert_eq!(ground_conflict.resolution_error, None);
        let distinct_rigid_conflict = constraints
            .constrain_types(
                &rigid,
                &ConstraintType::Rigid(RigidTypeId(8)),
                span(5),
            )
            .unwrap_err();
        assert_eq!(distinct_rigid_conflict.expected, rigid);
        assert_eq!(
            distinct_rigid_conflict.actual,
            ConstraintType::Rigid(RigidTypeId(8))
        );
        assert!(constraints
            .constrain_types(&rigid, &rigid, span(6))
            .is_ok());
        assert_eq!(constraints.resolve(&variable).unwrap(), variable);
        assert_eq!(
            constraints.resolve_constraint(&ConstraintType::Variable(variable_id(&variable))),
            Ok(rigid)
        );
    }

    #[test]
    fn occurs_check_traverses_open_record_bindings_before_aliasing() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let field_type = constraints.fresh(span(1));
            let record_type = constraints.fresh(span(2));
            let field = crate::symbol::Name::intern("field");
            let tail = constraints.fresh_row(span(3), [field]);
            let row = RowShape {
                fields: std::collections::BTreeMap::from([(field, field_type.clone())]),
                tail: RowTail::Variable(tail),
            };
            constraints
                .constrain_types(
                    &ConstraintType::Variable(variable_id(&record_type)),
                    &ConstraintType::Record(row.clone()),
                    span(4),
                )
                .unwrap();

            let conflict = constraints
                .constrain(&field_type, &record_type, span(5))
                .unwrap_err();

            assert_eq!(conflict.resolution_error, None);
            assert_eq!(constraints.resolve(&field_type).unwrap(), field_type);
            assert_eq!(constraints.resolve(&record_type).unwrap(), record_type);
            assert_eq!(constraints.resolve_row(&row).unwrap(), row);
        });
    }

    #[test]
    fn record_binding_resolution_propagates_enclosing_type_depth() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let record_type = constraints.fresh(span(1));
            let row = RowShape {
                fields: std::collections::BTreeMap::from([(
                    crate::symbol::Name::intern("field"),
                    Type::Int,
                )]),
                tail: RowTail::Closed,
            };
            constraints
                .constrain_types(
                    &ConstraintType::Variable(variable_id(&record_type)),
                    &ConstraintType::Record(row),
                    span(2),
                )
                .unwrap();
            let mut nested = record_type;
            for _ in 0..MAX_TYPE_DEPTH {
                nested = Type::List(Box::new(nested));
            }

            assert_eq!(
                constraints.resolve(&nested),
                Err(ConstraintResolutionError::TypeDepth)
            );
        });
    }

    #[test]
    fn compatible_open_record_aliases_close_through_either_variable() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let left_field = crate::symbol::Name::intern("left");
            let right_field = crate::symbol::Name::intern("right");
            let left = constraints.fresh(span(1));
            let right = constraints.fresh(span(2));
            let left_tail = constraints.fresh_row(span(3), [left_field]);
            let right_tail = constraints.fresh_row(span(4), [right_field]);
            let left_row = RowShape {
                fields: std::collections::BTreeMap::from([(left_field, Type::Int)]),
                tail: RowTail::Variable(left_tail),
            };
            let right_row = RowShape {
                fields: std::collections::BTreeMap::from([(right_field, Type::Bool)]),
                tail: RowTail::Variable(right_tail),
            };
            constraints
                .constrain_types(
                    &ConstraintType::Variable(variable_id(&left)),
                    &ConstraintType::Record(left_row),
                    span(5),
                )
                .unwrap();
            constraints
                .constrain_types(
                    &ConstraintType::Variable(variable_id(&right)),
                    &ConstraintType::Record(right_row),
                    span(6),
                )
                .unwrap();
            constraints.constrain(&left, &right, span(7)).unwrap();

            let closed = Type::Record(Arc::new(std::collections::BTreeMap::from([
                (left_field, Type::Int),
                (right_field, Type::Bool),
            ])));
            for alias in [&left, &right] {
                let mut probe = constraints.clone();
                probe.constrain(alias, &closed, span(8)).unwrap();
                assert_eq!(probe.resolve(&left).unwrap(), closed);
                assert_eq!(probe.resolve(&right).unwrap(), closed);
            }
        });
    }

    #[test]
    fn ranked_aliases_compress_per_clone_and_keep_canonical_identity() {
        let mut base = TypeConstraints::default();
        let variables = (0..8)
            .map(|index| base.fresh(span(index + 1)))
            .collect::<Vec<_>>();
        union_balanced(&mut base, &variables, 20);

        assert_eq!(rank(&base, &variables[0]), 3);
        assert_eq!(parent(&base, &variables[7]), Some(variable_id(&variables[6])));
        assert_eq!(parent(&base, &variables[6]), Some(variable_id(&variables[4])));

        let base_metrics = base.metrics();
        let probe = base.clone();
        let probe_metrics = probe.metrics();
        assert_eq!(probe.resolve(&variables[7]), Ok(variables[0].clone()));
        let probe_after = probe.metrics();
        assert_eq!(probe_after.representative_hops - probe_metrics.representative_hops, 3);
        assert_eq!(probe_after.compression_writes - probe_metrics.compression_writes, 2);
        assert_eq!(base.metrics(), base_metrics);
        assert_eq!(parent(&probe, &variables[7]), Some(variable_id(&variables[0])));
        assert_eq!(parent(&base, &variables[7]), Some(variable_id(&variables[6])));
        assert_eq!(base.resolve(&variables[7]), Ok(variables[0].clone()));
        let base_after = base.metrics();
        assert_eq!(base_after.representative_hops - base_metrics.representative_hops, 3);
        assert_eq!(base_after.compression_writes - base_metrics.compression_writes, 2);
        assert_eq!(parent(&base, &variables[7]), Some(variable_id(&variables[0])));
    }

    #[test]
    fn failed_unification_rolls_back_rank_binding_and_path_compression() {
        let mut constraints = TypeConstraints::default();
        let variables = (0..16)
            .map(|index| constraints.fresh(span(index + 1)))
            .collect::<Vec<_>>();
        union_balanced(&mut constraints, &variables[..8], 30);
        union_balanced(&mut constraints, &variables[8..], 50);
        let item = constraints.fresh(span(80));
        assert_eq!(rank(&constraints, &variables[0]), 3);
        assert_eq!(rank(&constraints, &variables[8]), 3);
        assert_eq!(parent(&constraints, &variables[7]), Some(variable_id(&variables[6])));
        assert_eq!(parent(&constraints, &variables[15]), Some(variable_id(&variables[14])));

        let expected = Type::Result(
            Box::new(Type::Result(
                Box::new(variables[7].clone()),
                Box::new(item.clone()),
            )),
            Box::new(Type::Int),
        );
        let actual = Type::Result(
            Box::new(Type::Result(
                Box::new(variables[15].clone()),
                Box::new(Type::Path),
            )),
            Box::new(Type::Str),
        );
        let before_failure = constraints.metrics();
        assert!(constraints.constrain(&expected, &actual, span(81)).is_err());
        let after_failure = constraints.metrics();
        assert_eq!(after_failure.rollbacks - before_failure.rollbacks, 1);
        assert!(after_failure.compression_writes > before_failure.compression_writes);
        assert!(after_failure.rollback_changes > before_failure.rollback_changes);
        assert!(after_failure.rollback_changes - before_failure.rollback_changes
            >= after_failure.compression_writes - before_failure.compression_writes);

        assert_eq!(rank(&constraints, &variables[0]), 3);
        assert_eq!(rank(&constraints, &variables[8]), 3);
        assert_eq!(parent(&constraints, &variables[7]), Some(variable_id(&variables[6])));
        assert_eq!(parent(&constraints, &variables[6]), Some(variable_id(&variables[4])));
        assert_eq!(parent(&constraints, &variables[15]), Some(variable_id(&variables[14])));
        assert_eq!(parent(&constraints, &variables[14]), Some(variable_id(&variables[12])));
        assert_eq!(constraints.resolve(&item).unwrap(), item);
    }

    #[test]
    fn nested_conflicts_roll_back_earlier_variable_bindings() {
        let mut constraints = TypeConstraints::default();
        let item = constraints.fresh(span(1));
        let expected = Type::Result(Box::new(item.clone()), Box::new(Type::Int));
        let actual = Type::Result(Box::new(Type::Path), Box::new(Type::Str));
        assert!(constraints.constrain(&expected, &actual, span(2)).is_err());
        assert_eq!(constraints.resolve(&item).unwrap(), item);
        assert_eq!(constraints.unresolved(&expected).unwrap(), vec![span(1)]);
    }

    #[test]
    fn failed_unification_after_alias_walk_keeps_probes_independent() {
        let mut base = TypeConstraints::default();
        let root = base.fresh(span(1));
        let middle = base.fresh(span(2));
        let leaf = base.fresh(span(3));
        base.constrain(&leaf, &middle, span(4)).unwrap();
        base.constrain(&middle, &root, span(5)).unwrap();
        base.constrain(&leaf, &root, span(6)).unwrap();

        let mut failed = base.clone();
        let expected = Type::Result(Box::new(leaf.clone()), Box::new(Type::Int));
        let actual = Type::Result(Box::new(Type::Path), Box::new(Type::Str));
        assert!(failed.constrain(&expected, &actual, span(7)).is_err());
        assert_eq!(failed.resolve(&leaf).unwrap(), root);
        assert_eq!(base.resolve(&leaf).unwrap(), root);

        let mut int_probe = base.clone();
        let mut path_probe = base;
        int_probe.constrain(&leaf, &Type::Int, span(8)).unwrap();
        path_probe.constrain(&leaf, &Type::Path, span(9)).unwrap();
        assert_eq!(int_probe.resolve(&leaf).unwrap(), Type::Int);
        assert_eq!(path_probe.resolve(&leaf).unwrap(), Type::Path);
    }

    #[test]
    fn optional_evidence_and_reversed_alias_order_have_canonical_solutions() {
        for reversed in [false, true] {
            let mut constraints = TypeConstraints::default();
            let inner = constraints.fresh(span(1));
            let alias = constraints.fresh(span(2));
            let (left, right) = if reversed {
                (&alias, &inner)
            } else {
                (&inner, &alias)
            };
            constraints.constrain(left, right, span(3)).unwrap();
            let nullable = Type::Optional(Box::new(inner.clone()));
            for inert in [Type::Null, Type::Any, Type::Unknown, Type::Invalid] {
                constraints.constrain(&nullable, &inert, span(4)).unwrap();
                assert_eq!(constraints.unresolved(&nullable).unwrap(), vec![span(1)]);
            }
            constraints
                .constrain(&nullable, &Type::Path, span(5))
                .unwrap();
            assert_eq!(
                constraints.resolve(&nullable).unwrap(),
                Type::Optional(Box::new(Type::Path))
            );
            assert_eq!(constraints.resolve(&alias).unwrap(), Type::Path);
        }
    }

    #[test]
    fn nullable_seed_accepts_path_and_relpath_contributions_in_either_order() {
        for reversed in [false, true] {
            let mut constraints = TypeConstraints::default();
            let seed = constraints.fresh(span(1));
            let seed_id = variable_id(&seed);
            let alias = constraints.fresh(span(2));
            constraints.constrain(&alias, &seed, span(3)).unwrap();
            let nullable = Type::Optional(Box::new(seed.clone()));
            let contributions = if reversed {
                [Type::rel_path(), Type::Path]
            } else {
                [Type::Path, Type::rel_path()]
            };
            for (index, contribution) in contributions.iter().enumerate() {
                constraints
                    .constrain_nullable_seed(seed_id, contribution, span(4 + index))
                    .unwrap();
            }
            assert_eq!(
                constraints.resolve(&nullable).unwrap(),
                Type::Optional(Box::new(Type::Path))
            );
            assert_eq!(constraints.resolve(&alias).unwrap(), Type::Path);
        }

        let mut ordinary = TypeConstraints::default();
        let inner = ordinary.fresh(span(10));
        let nullable = Type::Optional(Box::new(inner));
        ordinary.constrain(&nullable, &Type::rel_path(), span(11)).unwrap();
        assert!(ordinary.constrain(&nullable, &Type::Path, span(12)).is_err());
    }

    #[test]
    fn nullable_seed_resolves_existing_alias_contributions() {
        let mut constraints = TypeConstraints::default();
        let seed = constraints.fresh(span(1));
        let seed_id = variable_id(&seed);
        let alias = constraints.fresh(span(2));
        let nullable = Type::Optional(Box::new(seed));

        constraints
            .constrain_nullable_seed(seed_id, &Type::rel_path(), span(3))
            .unwrap();
        constraints.constrain(&alias, &Type::Path, span(4)).unwrap();
        constraints
            .constrain_nullable_seed(seed_id, &alias, span(5))
            .unwrap();

        assert_eq!(
            constraints.resolve(&nullable).unwrap(),
            Type::Optional(Box::new(Type::Path))
        );
        assert_eq!(constraints.resolve(&alias).unwrap(), Type::Path);
    }

    #[test]
    fn nullable_seed_keeps_a_sole_relpath_contribution_refined() {
        let mut constraints = TypeConstraints::default();
        let inner = constraints.fresh(span(1));
        let seed_id = variable_id(&inner);
        let nullable = Type::Optional(Box::new(inner));
        constraints
            .constrain_nullable_seed(seed_id, &Type::rel_path(), span(2))
            .unwrap();
        assert_eq!(
            constraints.resolve(&nullable).unwrap(),
            Type::Optional(Box::new(Type::rel_path()))
        );
    }

    #[test]
    fn nullable_seed_uses_the_existing_base_of_other_validated_types() {
        let expected = Type::Optional(Box::new(Type::List(Box::new(Type::Str))));
        for reversed in [false, true] {
            let mut constraints = TypeConstraints::default();
            let inner = constraints.fresh(span(1));
            let seed_id = variable_id(&inner);
            let nullable = Type::Optional(Box::new(inner));
            let contributions = if reversed {
                [Type::List(Box::new(Type::Str)), Type::non_empty(Type::Str)]
            } else {
                [Type::non_empty(Type::Str), Type::List(Box::new(Type::Str))]
            };
            for (index, contribution) in contributions.iter().enumerate() {
                constraints
                    .constrain_nullable_seed(seed_id, contribution, span(2 + index))
                    .unwrap();
            }
            assert_eq!(constraints.resolve(&nullable).unwrap(), expected);
        }
    }

    #[test]
    fn nullable_seed_does_not_merge_distinct_validations_through_their_base() {
        let ranges = [
            Type::bounded(
                crate::sema::validated::IntRange::new(1, 3).unwrap(),
                Type::Int,
            )
            .unwrap(),
            Type::bounded(
                crate::sema::validated::IntRange::new(8, 10).unwrap(),
                Type::Int,
            )
            .unwrap(),
        ];
        for reversed in [false, true] {
            let mut constraints = TypeConstraints::default();
            let inner = constraints.fresh(span(1));
            let seed_id = variable_id(&inner);
            let nullable = Type::Optional(Box::new(inner));
            let contributions = if reversed {
                [ranges[1].clone(), ranges[0].clone()]
            } else {
                [ranges[0].clone(), ranges[1].clone()]
            };
            constraints
                .constrain_nullable_seed(seed_id, &contributions[0], span(2))
                .unwrap();
            assert!(constraints
                .constrain_nullable_seed(seed_id, &contributions[1], span(3))
                .is_err());
            assert_eq!(
                constraints.resolve(&nullable).unwrap(),
                Type::Optional(Box::new(contributions[0].clone()))
            );
        }
    }

    #[test]
    fn nullable_seed_retains_repeated_validation() {
        let range = Type::bounded(
            crate::sema::validated::IntRange::new(1, 3).unwrap(),
            Type::Int,
        )
        .unwrap();
        let mut constraints = TypeConstraints::default();
        let inner = constraints.fresh(span(1));
        let seed_id = variable_id(&inner);
        let nullable = Type::Optional(Box::new(inner));
        constraints
            .constrain_nullable_seed(seed_id, &range, span(2))
            .unwrap();
        constraints
            .constrain_nullable_seed(seed_id, &range, span(3))
            .unwrap();
        assert_eq!(
            constraints.resolve(&nullable).unwrap(),
            Type::Optional(Box::new(range))
        );
    }

    #[test]
    fn checked_nullable_seed_uses_its_fixed_base_without_rebinding() {
        let mut constraints = TypeConstraints::default();
        let inner = constraints.fresh(span(1));
        let seed_id = variable_id(&inner);
        let nullable = Type::Optional(Box::new(inner.clone()));
        constraints.constrain(&nullable, &Type::Path, span(2)).unwrap();

        constraints
            .check_nullable_seed(seed_id, &Type::rel_path(), span(3))
            .unwrap();
        constraints
            .check_nullable_seed(seed_id, &Type::Path, span(4))
            .unwrap();
        assert!(constraints
            .check_nullable_seed(seed_id, &Type::Str, span(5))
            .is_err());
        assert_eq!(constraints.resolve(&inner).unwrap(), Type::Path);
    }

    #[test]
    fn nullable_seed_widening_does_not_override_annotation_authority() {
        let mut constraints = TypeConstraints::default();
        let inner = constraints.fresh(span(1));
        let seed_id = variable_id(&inner);
        let nullable = Type::Optional(Box::new(inner));
        constraints
            .constrain_annotation(&nullable, &Type::rel_path(), span(2))
            .unwrap();

        assert!(constraints
            .constrain_nullable_seed(seed_id, &Type::Path, span(3))
            .is_err());
        assert_eq!(
            constraints.resolve(&nullable).unwrap(),
            Type::Optional(Box::new(Type::rel_path()))
        );
    }

    #[test]
    fn nullable_seed_rejects_incompatible_and_invariant_contributions() {
        for contributions in [
            [Type::Path, Type::Str],
            [Type::Str, Type::Path],
            [Type::rel_path(), Type::Str],
            [Type::Str, Type::rel_path()],
        ] {
            let mut constraints = TypeConstraints::default();
            let inner = constraints.fresh(span(1));
            let seed_id = variable_id(&inner);
            constraints
                .constrain_nullable_seed(seed_id, &contributions[0], span(2))
                .unwrap();
            assert!(constraints
                .constrain_nullable_seed(seed_id, &contributions[1], span(3))
                .is_err());
        }

        for (first, second) in [
            (
                Type::List(Box::new(Type::Str)),
                Type::List(Box::new(Type::Path)),
            ),
            (
                Type::Map(Box::new(Type::Str), Box::new(Type::Str)),
                Type::Map(Box::new(Type::Str), Box::new(Type::Path)),
            ),
        ] {
            let mut constraints = TypeConstraints::default();
            let inner = constraints.fresh(span(4));
            let seed_id = variable_id(&inner);
            constraints
                .constrain_nullable_seed(seed_id, &first, span(5))
                .unwrap();
            assert!(constraints
                .constrain_nullable_seed(seed_id, &second, span(6))
                .is_err());
        }
    }

    #[test]
    fn nullable_seed_rebinding_rolls_back_without_breaking_aliases() {
        let mut constraints = TypeConstraints::default();
        let seed = constraints.fresh(span(1));
        let seed_id = variable_id(&seed);
        let alias = constraints.fresh(span(2));
        constraints.constrain(&alias, &seed, span(3)).unwrap();
        let nullable = Type::Optional(Box::new(seed));
        constraints
            .constrain_nullable_seed(seed_id, &Type::rel_path(), span(4))
            .unwrap();

        let result = constraints.trial(|constraints| {
            constraints.constrain_nullable_seed(seed_id, &Type::Path, span(5))?;
            constraints.constrain(&Type::Int, &Type::Str, span(6))?;
            Ok(())
        });

        assert_eq!(result, Ok(false));
        assert_eq!(
            constraints.resolve(&nullable).unwrap(),
            Type::Optional(Box::new(Type::rel_path()))
        );
        assert_eq!(constraints.resolve(&alias).unwrap(), Type::rel_path());
    }

    #[test]
    fn probes_keep_existing_aliases_but_new_variables_cannot_cross_problems() {
        let mut constraints = TypeConstraints::default();
        let seed = constraints.fresh(span(1));
        let mut probe = constraints.clone();
        let probe_variable = probe.fresh(span(2));
        let other_variable = constraints.fresh(span(2));
        assert_ne!(probe_variable, other_variable);
        assert_eq!(
            constraints.resolve(&probe_variable),
            Err(ConstraintResolutionError::ForeignVariable)
        );
        assert!(
            probe
                .constrain(&seed, &Type::List(Box::new(seed.clone())), span(3))
                .is_err()
        );
        assert_eq!(probe.resolve(&seed).unwrap(), seed);
    }

    #[test]
    fn shared_nested_records_resolve_independently() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut base = TypeConstraints::default();
            let inference = base.fresh(span(1));
            let inner_name = crate::symbol::Name::intern("inner");
            let outer_name = crate::symbol::Name::intern("outer");
            let original = Type::Record(Arc::new(std::collections::BTreeMap::from([(
                outer_name,
                Type::Record(Arc::new(std::collections::BTreeMap::from([(
                    inner_name,
                    inference.clone(),
                )]))),
            )])));
            let copied = original.clone();
            let mut int_constraints = base.clone();
            let mut path_constraints = base.clone();
            int_constraints
                .constrain(&inference, &Type::Int, span(2))
                .unwrap();
            path_constraints
                .constrain(&inference, &Type::Path, span(3))
                .unwrap();

            let int_record = int_constraints.resolve(&copied).unwrap();
            let path_record = path_constraints.resolve(&original).unwrap();

            assert_eq!(
                int_record,
                Type::Record(Arc::new(std::collections::BTreeMap::from([(
                    outer_name,
                    Type::Record(Arc::new(std::collections::BTreeMap::from([(
                        inner_name,
                        Type::Int,
                    )]))),
                )])))
            );
            assert_eq!(
                path_record,
                Type::Record(Arc::new(std::collections::BTreeMap::from([(
                    outer_name,
                    Type::Record(Arc::new(std::collections::BTreeMap::from([(
                        inner_name,
                        Type::Path,
                    )]))),
                )])))
            );
            assert_eq!(
                original,
                Type::Record(Arc::new(std::collections::BTreeMap::from([(
                    outer_name,
                    Type::Record(Arc::new(std::collections::BTreeMap::from([(
                        inner_name,
                        inference,
                    )]))),
                )])))
            );
        });
    }

    #[test]
    fn ground_record_resolution_preserves_depth_errors() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let name = crate::symbol::Name::intern("next");
            let mut ground = Type::Int;
            for _ in 0..=MAX_TYPE_DEPTH {
                ground = Type::Record(Arc::new(std::collections::BTreeMap::from([(
                    name, ground,
                )])));
            }

            assert_eq!(
                TypeConstraints::default().resolve(&ground),
                Err(ConstraintResolutionError::TypeDepth)
            );
        });
    }

    #[test]
    fn record_resolution_preserves_foreign_variable_errors() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut owner = TypeConstraints::default();
            let foreign = owner.fresh(span(1));
            let record = Type::Record(Arc::new(std::collections::BTreeMap::from([(
                crate::symbol::Name::intern("value"),
                foreign,
            )])));
            let receiver = TypeConstraints::default();

            assert_eq!(
                receiver.resolve(&record),
                Err(ConstraintResolutionError::ForeignVariable)
            );
            assert_eq!(receiver.metrics().representative_hops, 0);
            assert_eq!(receiver.metrics().compression_writes, 0);
            assert_eq!(receiver.metrics().rollback_changes, 0);
        });
    }

    #[test]
    fn retained_bytes_cover_solver_tables_and_owned_row_structure() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let empty = TypeConstraints::default();
            let empty_bytes = empty.retained_bytes();
            assert!(empty_bytes >= std::mem::size_of::<TypeConstraints>());

            let mut constraints = TypeConstraints::default();
            let receiver = constraints.fresh(span(1));
            let field = crate::symbol::Name::intern("retained_field");
            let projected = constraints
                .project_record_field(&receiver, field, 0, span(2))
                .unwrap();
            assert_eq!(constraints.metrics().generated_constraints, 1);
            let row_bytes = constraints.retained_bytes();
            assert!(row_bytes > empty_bytes);
            assert!(row_bytes
                >= std::mem::size_of::<TypeConstraints>()
                    + constraints.variables.capacity()
                        * std::mem::size_of::<(TypeVariableId, Variable)>()
                    + constraints.rows.capacity()
                        * std::mem::size_of::<(RowVariableId, RowVariable)>());

            constraints
                .constrain(&projected, &Type::List(Box::new(Type::Str)), span(3))
                .unwrap();
            assert_eq!(constraints.metrics().generated_constraints, 2);
            assert!(constraints.retained_bytes() > row_bytes);
        });
    }

    #[test]
    fn nested_dynamic_and_recovery_operands_do_not_establish_contracts() {
        let mut constraints = TypeConstraints::default();
        let seed = constraints.fresh(span(1));
        for inert in [
            Type::List(Box::new(Type::Any)),
            Type::List(Box::new(Type::Unknown)),
            Type::Optional(Box::new(Type::Null)),
        ] {
            constraints.constrain(&seed, &inert, span(2)).unwrap();
            assert_eq!(constraints.resolve(&seed).unwrap(), seed);
        }
        let item = constraints.fresh(span(3));
        constraints
            .constrain(&seed, &Type::List(Box::new(item.clone())), span(4))
            .unwrap();
        constraints.constrain(&item, &Type::Path, span(5)).unwrap();
        assert_eq!(
            constraints.resolve(&seed).unwrap(),
            Type::List(Box::new(Type::Path))
        );
    }

    #[test]
    fn checked_annotations_can_choose_types_that_ordinary_evidence_cannot_anchor() {
        for annotation in [
            Type::Any,
            Type::Null,
            Type::Record(Arc::new(Default::default())),
            Type::List(Box::new(Type::Any)),
        ] {
            let mut constraints = TypeConstraints::default();
            let seed = constraints.fresh(span(1));
            constraints.constrain(&seed, &annotation, span(2)).unwrap();
            assert_eq!(constraints.resolve(&seed).unwrap(), seed);
            constraints
                .constrain_annotation(&seed, &annotation, span(3))
                .unwrap();
            assert_eq!(constraints.resolve(&seed).unwrap(), annotation);
        }
        let mut constraints = TypeConstraints::default();
        let seed = constraints.fresh(span(1));
        let hole = constraints.fresh(span(2));
        for invalid in [
            Type::Invalid,
            Type::List(Box::new(Type::Unknown)),
            Type::Optional(Box::new(hole)),
        ] {
            assert!(
                constraints
                    .constrain_annotation(&seed, &invalid, span(3))
                    .is_err()
            );
            assert_eq!(constraints.resolve(&seed).unwrap(), seed);
        }
    }

    #[test]
    fn dynamic_values_cannot_satisfy_resolved_concrete_constraints() {
        let mut constraints = TypeConstraints::default();
        for expected in [
            Type::Int,
            Type::Optional(Box::new(Type::Int)),
            Type::List(Box::new(Type::Int)),
        ] {
            assert!(
                constraints
                    .constrain(&expected, &Type::Any, span(1))
                    .is_err()
            );
        }
        let record = Type::Record(Arc::new(std::collections::BTreeMap::from([(
            crate::symbol::Name::intern("value"),
            Type::Int,
        )])));
        assert!(
            constraints
                .constrain(&record, &Type::ErasedRecord, span(2))
                .is_err()
        );
        assert!(constraints.constrain(&Type::Any, &record, span(3)).is_ok());
        assert!(
            constraints
                .constrain(&Type::ErasedRecord, &record, span(4))
                .is_ok()
        );
    }

    #[test]
    fn container_record_width_cannot_bypass_invariance_during_substitution() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let item = constraints.fresh(span(1));
            let name = crate::symbol::Name::intern("value");
            let expected = Type::List(Box::new(Type::Record(Arc::new(
                std::collections::BTreeMap::from([(name, item.clone())]),
            ))));
            let actual = Type::List(Box::new(Type::Record(Arc::new(
                std::collections::BTreeMap::from([
                    (name, Type::Int),
                    (crate::symbol::Name::intern("extra"), Type::Bool),
                ]),
            ))));
            assert!(constraints.constrain(&expected, &actual, span(2)).is_err());
            assert_eq!(constraints.resolve(&item).unwrap(), item);
        });
    }
    #[test]
    fn optional_record_contributions_keep_one_shape_in_either_order() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let name = crate::symbol::Name::intern("value");
            let narrow = Type::Record(Arc::new(std::collections::BTreeMap::from([(
                name,
                Type::Int,
            )])));
            let wide = Type::Record(Arc::new(std::collections::BTreeMap::from([
                (name, Type::Int),
                (crate::symbol::Name::intern("extra"), Type::Bool),
            ])));
            for (first, second) in [(&narrow, &wide), (&wide, &narrow)] {
                let mut constraints = TypeConstraints::default();
                let item = constraints.fresh(span(1));
                let optional = Type::Optional(Box::new(item.clone()));
                constraints.constrain(&optional, first, span(2)).unwrap();
                assert!(constraints.constrain(&optional, second, span(3)).is_err());
                assert_eq!(constraints.resolve(&item).unwrap(), *first);
                assert!(constraints.constrain(&narrow, &wide, span(4)).is_ok());
            }
        });
    }

    #[test]
    fn authoritative_dynamic_destination_accepts_concrete_contributions() {
        let mut constraints = TypeConstraints::default();
        let chosen = constraints.fresh(span(1));
        constraints
            .constrain_annotation(&chosen, &Type::Any, span(2))
            .unwrap();
        constraints.constrain(&chosen, &Type::Int, span(3)).unwrap();
        assert_eq!(constraints.resolve(&chosen).unwrap(), Type::Any);
    }

    #[test]
    fn type_occurs_rejects_a_nested_recursive_binding() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let item = constraints.fresh(span(1));
            let recursive = ConstraintType::Record(RowShape {
                fields: std::collections::BTreeMap::from([(
                    crate::symbol::Name::intern("item"),
                    item.clone(),
                )]),
                tail: RowTail::Closed,
            });
            assert!(constraints
                .constrain_types(
                    &ConstraintType::Variable(variable_id(&item)),
                    &recursive,
                    span(2),
                )
                .is_err());
            assert_eq!(constraints.resolve(&item).unwrap(), item);
        });
    }

    #[test]
    fn row_occurs_rejects_a_recursive_tail() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let field = crate::symbol::Name::intern("field");
            let tail = constraints.fresh_row(span(1), std::iter::empty());
            let empty = RowShape {
                fields: std::collections::BTreeMap::new(),
                tail: RowTail::Variable(tail),
            };
            let recursive = RowShape {
                fields: std::collections::BTreeMap::from([(field, Type::Int)]),
                tail: RowTail::Variable(tail),
            };
            assert!(constraints
                .constrain_rows(&empty, &recursive, span(2))
                .is_err());
            assert_eq!(constraints.resolve_row(&empty).unwrap(), empty);
        });
    }

    #[test]
    fn failed_row_unification_rolls_back_type_bindings() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let first = constraints.fresh(span(1));
            let alias = constraints.fresh(span(2));
            let field = crate::symbol::Name::intern("field");
            constraints.constrain(&alias, &first, span(3)).unwrap();
            let left = RowShape {
                fields: std::collections::BTreeMap::from([
                    (field, alias.clone()),
                    (crate::symbol::Name::intern("fixed"), Type::Int),
                ]),
                tail: RowTail::Closed,
            };
            let right = RowShape {
                fields: std::collections::BTreeMap::from([
                    (field, Type::Path),
                    (crate::symbol::Name::intern("fixed"), Type::Str),
                ]),
                tail: RowTail::Closed,
            };
            assert!(constraints.constrain_rows(&left, &right, span(4)).is_err());
            assert_eq!(constraints.resolve(&alias).unwrap(), first);
            assert_eq!(parent(&constraints, &alias), Some(variable_id(&first)));
        });
    }

    #[test]
    fn failed_row_unification_rolls_back_row_lacks_and_fresh_tails() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let variables = (0..8)
                .map(|index| constraints.fresh(span(index + 1)))
                .collect::<Vec<_>>();
            union_balanced(&mut constraints, &variables, 20);
            let field = crate::symbol::Name::intern("field");
            let extra = crate::symbol::Name::intern("extra");
            let left_tail = constraints.fresh_row(span(40), [field, extra]);
            let right_tail = constraints.fresh_row(span(41), [field, extra]);
            let left = RowShape {
                fields: std::collections::BTreeMap::from([(field, variables[7].clone())]),
                tail: RowTail::Variable(left_tail),
            };
            let right = RowShape {
                fields: std::collections::BTreeMap::from([
                    (field, Type::Int),
                    (extra, Type::Bool),
                ]),
                tail: RowTail::Variable(right_tail),
            };
            let row_count = constraints.rows.len();
            assert!(constraints.constrain_rows(&left, &right, span(42)).is_err());
            assert_eq!(constraints.rows.len(), row_count);
            assert_eq!(parent(&constraints, &variables[7]), Some(variable_id(&variables[6])));
            assert_eq!(parent(&constraints, &variables[6]), Some(variable_id(&variables[4])));
            assert_eq!(constraints.resolve(&variables[7]).unwrap(), variables[0]);
            assert_eq!(constraints.resolve(&variables[0]).unwrap(), variables[0]);
            assert_eq!(
                constraints.resolve_row(&left).unwrap(),
                RowShape {
                    fields: std::collections::BTreeMap::from([(field, variables[0].clone())]),
                    tail: RowTail::Variable(left_tail),
                }
            );
        });
    }

    #[test]
    fn any_bearing_row_binding_fails_and_rolls_back_prior_field_constraints() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let field = crate::symbol::Name::intern("field");
            let extra = crate::symbol::Name::intern("extra");
            let field_type = constraints.fresh(span(1));
            let tail = constraints.fresh_row(span(2), [field]);
            let open = RowShape {
                fields: std::collections::BTreeMap::from([(field, field_type.clone())]),
                tail: RowTail::Variable(tail),
            };
            let closed = RowShape {
                fields: std::collections::BTreeMap::from([
                    (field, Type::Int),
                    (extra, Type::Set(Box::new(Type::Any))),
                ]),
                tail: RowTail::Closed,
            };
            let row_count = constraints.rows.len();

            let conflict = constraints
                .constrain_rows(&open, &closed, span(3))
                .unwrap_err();

            assert_eq!(conflict.resolution_error, None);
            assert_eq!(constraints.rows.len(), row_count);
            assert_eq!(constraints.resolve(&field_type).unwrap(), field_type);
            assert_eq!(constraints.resolve_row(&open).unwrap(), open);
        });
    }

    #[test]
    fn row_constraint_probes_keep_clones_independent() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut base = TypeConstraints::default();
            let field = crate::symbol::Name::intern("field");
            let tail = base.fresh_row(span(1), std::iter::empty());
            let open = RowShape {
                fields: std::collections::BTreeMap::from([(field, Type::Int)]),
                tail: RowTail::Variable(tail),
            };
            let mut integer_probe = base.clone();
            let mut string_probe = base.clone();
            let integer_row = RowShape {
                fields: std::collections::BTreeMap::from([
                    (field, Type::Int),
                    (crate::symbol::Name::intern("extra"), Type::Bool),
                ]),
                tail: RowTail::Closed,
            };
            let string_row = RowShape {
                fields: std::collections::BTreeMap::from([
                    (field, Type::Int),
                    (crate::symbol::Name::intern("extra"), Type::Str),
                ]),
                tail: RowTail::Closed,
            };
            integer_probe
                .constrain_rows(&open, &integer_row, span(2))
                .unwrap();
            string_probe
                .constrain_rows(&open, &string_row, span(3))
                .unwrap();
            assert_eq!(integer_probe.resolve_row(&open).unwrap(), integer_row);
            assert_eq!(string_probe.resolve_row(&open).unwrap(), string_row);
            assert_eq!(base.resolve_row(&open).unwrap(), open);
        });
    }

    #[test]
    fn rigid_type_variables_reject_ground_unification() {
        let mut constraints = TypeConstraints::default();
        let rigid = ConstraintType::Rigid(RigidTypeId(0));
        assert!(constraints
            .constrain_types(&rigid, &ConstraintType::Ground(Type::Int), span(1))
            .is_err());
        assert!(constraints.constrain_types(&rigid, &rigid, span(2)).is_ok());
    }

    #[test]
    fn closed_rows_reject_a_missing_required_field() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let field = crate::symbol::Name::intern("required");
            let expected = RowShape {
                fields: std::collections::BTreeMap::from([(field, Type::Int)]),
                tail: RowTail::Closed,
            };
            let actual = RowShape {
                fields: std::collections::BTreeMap::new(),
                tail: RowTail::Closed,
            };
            assert!(constraints
                .constrain_rows(&expected, &actual, span(1))
                .is_err());
        });
    }

    #[test]
    fn open_record_requirements_identify_a_missing_field() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let required = crate::symbol::Name::intern("name");
            let present = crate::symbol::Name::intern("version");
            let receiver = constraints.fresh_at_level(span(1), 1);
            constraints
                .project_record_field(&receiver, required, 1, span(2))
                .unwrap();
            let missing = Type::Record(Arc::new(BTreeMap::from([(present, Type::Str)])));
            assert_eq!(
                constraints
                    .missing_required_open_record_field(&receiver, &missing)
                    .unwrap(),
                Some(required)
            );

            let present_record = Type::Record(Arc::new(BTreeMap::from([
                (required, Type::Str),
                (present, Type::Str),
            ])));
            assert_eq!(
                constraints
                    .missing_required_open_record_field(&receiver, &present_record)
                    .unwrap(),
                None
            );

            let closed_expected = constraints.fresh(span(3));
            constraints
                .constrain(
                    &closed_expected,
                    &Type::Record(Arc::new(BTreeMap::from([(required, Type::Str)]))),
                    span(4),
                )
                .unwrap();
            assert_eq!(
                constraints
                    .missing_required_open_record_field(&closed_expected, &missing)
                    .unwrap(),
                None
            );

            let resolved_closed_expected = constraints.fresh_at_level(span(5), 1);
            constraints
                .project_record_field(&resolved_closed_expected, required, 1, span(6))
                .unwrap();
            constraints
                .constrain(
                    &resolved_closed_expected,
                    &Type::Record(Arc::new(BTreeMap::from([
                        (required, Type::Str),
                        (present, Type::Str),
                    ]))),
                    span(7),
                )
                .unwrap();
            assert_eq!(
                constraints
                    .missing_required_open_record_field(&resolved_closed_expected, &missing)
                    .unwrap(),
                None
            );
        });
    }

    #[test]
    fn type_scheme_generalization_and_instantiation_reuse_one_fresh_binder() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let value = constraints.fresh_at_level(span(1), 1);
            let generalized = constraints
                .generalize_types(&[value.clone(), value], 0)
                .unwrap();
            assert_eq!(generalized.type_parameters.len(), 1);
            assert!(generalized.row_parameters.is_empty());
            assert_eq!(generalized.types[0], generalized.types[1]);
            let scheme = TypeScheme::new(
                super::super::schemes::SchemeCallableKind::Pure,
                generalized.type_parameters,
                generalized.row_parameters,
                vec![super::super::schemes::SchemeParameter {
                    name: crate::symbol::Name::intern("value"),
                    ty: generalized.types[0].clone(),
                    schema_expectation: None,
                    defaulted: false,
                    rest: false,
                }],
                generalized.types[1].clone(),
                None,
                false,
                Some(Vec::new()),
                false,
            )
            .unwrap();
            let integer = constraints
                .instantiate_callable(scheme.callable(), 2, span(2))
                .unwrap();
            let text = constraints
                .instantiate_scheme(&scheme, 2, span(3))
                .unwrap();
            assert_ne!(integer.params[0], text.params[0]);
            assert_eq!(integer.params[0], integer.return_type);
            assert_eq!(text.params[0], text.return_type);
            constraints
                .constrain(&integer.params[0], &Type::Int, span(4))
                .unwrap();
            constraints
                .constrain(&text.params[0], &Type::Str, span(5))
                .unwrap();
            assert_eq!(constraints.resolve(&integer.return_type).unwrap(), Type::Int);
            assert_eq!(constraints.resolve(&text.return_type).unwrap(), Type::Str);
        });
    }

    #[test]
    fn type_scheme_generalization_preserves_open_record_rows() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let field = crate::symbol::Name::intern("name");
            let receiver = constraints.fresh_at_level(span(1), 1);
            let field_type = constraints
                .project_record_field(&receiver, field, 1, span(2))
                .unwrap();
            let generalized = constraints
                .generalize_types(&[receiver, field_type], 0)
                .unwrap();
            assert_eq!(generalized.type_parameters.len(), 1);
            assert_eq!(generalized.row_parameters.len(), 1);
            assert!(generalized.row_parameters[0].lacks.contains(&field));
            let scheme = TypeScheme::new(
                super::super::schemes::SchemeCallableKind::Pure,
                generalized.type_parameters,
                generalized.row_parameters,
                vec![super::super::schemes::SchemeParameter {
                    name: crate::symbol::Name::intern("value"),
                    ty: generalized.types[0].clone(),
                    schema_expectation: None,
                    defaulted: false,
                    rest: false,
                }],
                generalized.types[1].clone(),
                None,
                false,
                Some(Vec::new()),
                false,
            )
            .unwrap();
            let instance = constraints
                .instantiate_scheme(&scheme, 2, span(3))
                .unwrap();
            let record = Type::Record(Arc::new(BTreeMap::from([
                (field, Type::Int),
                (crate::symbol::Name::intern("extra"), Type::Bool),
            ])));
            constraints
                .constrain(&instance.params[0], &record, span(4))
                .unwrap();
            assert_eq!(constraints.resolve(&instance.return_type).unwrap(), Type::Int);
        });
    }

    #[test]
    fn projecting_into_outer_record_lowers_new_field_and_tail_levels() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let field = crate::symbol::Name::intern("captured");
            let receiver = constraints.fresh_at_level(span(1), 0);
            let field_type = constraints
                .project_record_field(&receiver, field, 1, span(2))
                .unwrap();
            let receiver_id = match &receiver {
                Type::Inference(id) => *id,
                _ => unreachable!(),
            };
            let row = match constraints
                .resolve_constraint(&ConstraintType::Variable(receiver_id))
                .unwrap()
            {
                ConstraintType::Record(row) => row,
                _ => panic!("record projection did not bind the receiver to a row"),
            };
            let field_id = match &row.fields[&field] {
                Type::Inference(id) => *id,
                _ => panic!("projected field did not remain an inference variable"),
            };
            let row_id = match row.tail {
                RowTail::Variable(id) => id,
                _ => panic!("projected row tail did not remain open"),
            };
            assert_eq!(constraints.type_level(field_id), Ok(0));
            assert_eq!(constraints.row_level(row_id), Ok(0));
            assert!(matches!(
                constraints.generalize_types(&[receiver, field_type], 0),
                Err(SchemeOperationError::Build(SchemeBuildError::UnresolvedType))
            ));
        });
    }

    #[test]
    fn union_lowers_levels_of_inherited_open_record_descendants() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut constraints = TypeConstraints::default();
            let field = crate::symbol::Name::intern("captured");
            let receiver = constraints.fresh_at_level(span(1), 1);
            constraints
                .project_record_field(&receiver, field, 1, span(2))
                .unwrap();
            let alias = constraints.fresh_at_level(span(3), 0);
            constraints.constrain(&receiver, &alias, span(4)).unwrap();
            let receiver_id = match &receiver {
                Type::Inference(id) => *id,
                _ => unreachable!(),
            };
            let row = match constraints
                .resolve_constraint(&ConstraintType::Variable(receiver_id))
                .unwrap()
            {
                ConstraintType::Record(row) => row,
                _ => panic!("record alias did not preserve its row binding"),
            };
            let field_id = match &row.fields[&field] {
                Type::Inference(id) => *id,
                _ => panic!("projected field did not remain an inference variable"),
            };
            let row_id = match row.tail {
                RowTail::Variable(id) => id,
                _ => panic!("projected row tail did not remain open"),
            };
            assert_eq!(constraints.type_level(field_id), Ok(0));
            assert_eq!(constraints.row_level(row_id), Ok(0));
            assert!(matches!(
                constraints.generalize_types(&[receiver], 0),
                Err(SchemeOperationError::Build(SchemeBuildError::UnresolvedType))
            ));
        });
    }

    #[test]
    fn type_scheme_generalization_rejects_monomorphic_environment_variables() {
        let mut constraints = TypeConstraints::default();
        let variable = constraints.fresh(span(1));
        assert!(matches!(
            constraints.generalize_types(&[variable], 0),
            Err(SchemeOperationError::Build(SchemeBuildError::UnresolvedType))
        ));
    }
}
