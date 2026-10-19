use super::constants::{SchemaComponent, SchemaExpectation, SchemaInstance};
use super::types::{ModuleExportType, Type};
use crate::symbol::Name;
use crate::syntax::node::Effect;
use crate::sema::validated::Validation;
use crate::syntax::arena::{ExprId, StmtId};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct SchemeTypeParameterId(pub(crate) u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct SchemeRowParameterId(pub(crate) u32);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SchemeType {
    Ground(Type),
    Parameter(SchemeTypeParameterId),
    Callable {
        kind: crate::sema::types::CallableKind,
        monotype: Arc<CallableMonotype>,
        effects: Option<Arc<[Effect]>>,
    },
    List(Box<Self>),
    Map(Box<Self>, Box<Self>),
    Stream(Box<Self>),
    Set(Box<Self>),
    Result(Box<Self>, Box<Self>),
    Optional(Box<Self>),
    Record {
        fields: BTreeMap<Name, Self>,
        tail: SchemeRowTail,
    },
    Union(Vec<Self>),
    Validated(Validation, Box<Self>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SchemeRowTail {
    Closed,
    Parameter(SchemeRowParameterId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SchemeRowParameter {
    pub(crate) id: SchemeRowParameterId,
    pub(crate) lacks: BTreeSet<Name>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SchemeParameter {
    pub(crate) name: Name,
    pub(crate) ty: SchemeType,
    pub(crate) schema_expectation: Option<SchemaExpectation>,
    pub(crate) defaulted: bool,
    pub(crate) rest: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CallableScheme {
    type_parameters: Arc<[SchemeTypeParameterId]>,
    row_parameters: Arc<[SchemeRowParameter]>,
    monotype: CallableMonotype,
    explicit_return: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CallableMonotype {
    params: Arc<[SchemeParameter]>,
    return_type: SchemeType,
    return_schema: Option<SchemaExpectation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TypeScheme {
    kind: crate::sema::types::CallableKind,
    callable: Arc<CallableScheme>,
    effects: Option<Arc<[Effect]>>,
    inferred_effects: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CallInstantiation {
    type_arguments: BTreeMap<SchemeTypeParameterId, SchemeType>,
    row_arguments: BTreeMap<SchemeRowParameterId, RowEvidence>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RowEvidence {
    fields: BTreeMap<Name, SchemeType>,
    tail: SchemeRowTail,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RegisteredArgumentCheck {
    Unsigned(Type),
    UIntMapKey,
}

#[derive(Clone, Debug)]
pub(crate) struct RegisteredCallPlan {
    pub(crate) receiver: Option<crate::modules::MethodReceiver>,
    pub(crate) name: Arc<str>,
    pub(crate) signature: &'static crate::modules::ModuleFnSig,
    pub(crate) argument_slots: Arc<[usize]>,
    pub(crate) argument_checks: Arc<[Option<RegisteredArgumentCheck>]>,
}

impl PartialEq for RegisteredCallPlan {
    fn eq(&self, other: &Self) -> bool {
        self.receiver == other.receiver
            && self.name == other.name
            && std::ptr::eq(self.signature, other.signature)
            && self.argument_slots == other.argument_slots
            && self.argument_checks == other.argument_checks
    }
}

impl Eq for RegisteredCallPlan {}

impl RegisteredCallPlan {
    pub(crate) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.name.len()
            + std::mem::size_of_val(self.argument_slots.as_ref())
            + std::mem::size_of_val(self.argument_checks.as_ref())
            + self.argument_checks.iter().flatten().map(|check| match check {
                RegisteredArgumentCheck::Unsigned(ty) => ty.retained_bytes().saturating_sub(std::mem::size_of::<Type>()),
                RegisteredArgumentCheck::UIntMapKey => 0,
            }).sum::<usize>()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SchemeBodyFacts {
    scheme: Arc<TypeScheme>,
    expression_types: BTreeMap<ExprId, SchemeType>,
    tail_types: BTreeMap<StmtId, SchemeType>,
    registered_calls: BTreeMap<ExprId, Arc<RegisteredCallPlan>>,
    read_bindings: BTreeSet<super::check::BindingId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SchemeBuildError {
    RecoveryType,
    UnresolvedType,
    UnresolvedRow,
    #[cfg(test)]
    RigidVariable,
    InvalidValidatedBase,
    UnboundTypeParameter,
    UnboundRowParameter,
    UnusedTypeParameter,
    UnusedRowParameter,
    DuplicateTypeParameter,
    DuplicateRowParameter,
    RowLacksMismatch,
    UnresolvedEffects,
    UnsupportedCallableKind,
    UnresolvedSchema,
    DynamicType,
    BodyTypeNotBound,
}

impl TypeScheme {
    #[cfg(test)]
    pub(crate) fn new(
        kind: crate::sema::types::CallableKind,
        type_parameters: Vec<SchemeTypeParameterId>,
        row_parameters: Vec<SchemeRowParameter>,
        params: Vec<SchemeParameter>,
        return_type: SchemeType,
        return_schema: Option<SchemaExpectation>,
        explicit_return: bool,
        effects: Option<Vec<Effect>>,
        inferred_effects: bool,
    ) -> Result<Self, SchemeBuildError> {
        let callable = Arc::new(CallableScheme::new(
            type_parameters,
            row_parameters,
            params,
            return_type,
            return_schema,
            explicit_return,
        )?);
        Self::with_callable(kind, callable, effects, inferred_effects)
    }

    pub(crate) fn with_callable(
        kind: crate::sema::types::CallableKind,
        callable: Arc<CallableScheme>,
        effects: Option<Vec<Effect>>,
        inferred_effects: bool,
    ) -> Result<Self, SchemeBuildError> {
        if !callable.type_parameters.is_empty() || !callable.row_parameters.is_empty() {
            match kind {
                crate::sema::types::CallableKind::Pure => {}
                crate::sema::types::CallableKind::Proc if effects.is_some() => {}
                crate::sema::types::CallableKind::Proc => return Err(SchemeBuildError::UnresolvedEffects),
                crate::sema::types::CallableKind::Stream => return Err(SchemeBuildError::UnsupportedCallableKind),
            }
        }
        Ok(Self {
            kind,
            callable,
            effects: effects.map(Arc::from),
            inferred_effects,
        })
    }

    pub(crate) fn callable(&self) -> &Arc<CallableScheme> {
        &self.callable
    }

    pub(crate) fn kind(&self) -> crate::sema::types::CallableKind {
        self.kind
    }

    pub(crate) fn type_parameters(&self) -> &[SchemeTypeParameterId] {
        self.callable.type_parameters()
    }

    pub(crate) fn row_parameters(&self) -> &[SchemeRowParameter] {
        self.callable.row_parameters()
    }

    pub(crate) fn params(&self) -> &[SchemeParameter] {
        self.callable.params()
    }

    pub(crate) fn return_type(&self) -> &SchemeType {
        self.callable.return_type()
    }

    pub(crate) fn return_schema(&self) -> Option<&SchemaExpectation> {
        self.callable.return_schema()
    }

    pub(crate) fn explicit_return(&self) -> bool {
        self.callable.explicit_return()
    }

    pub(crate) fn effects(&self) -> Option<&[Effect]> {
        self.effects.as_deref()
    }

    pub(crate) fn inferred_effects(&self) -> bool {
        self.inferred_effects
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        size_of::<Self>()
            .saturating_add(self.callable.retained_bytes())
            .saturating_add(self.effects.as_ref().map_or(0, |effects| {
                effects.len() * size_of::<Effect>()
            }))
    }

    pub(crate) fn map_ground_types(&self, mut map: impl FnMut(&mut Type)) -> Self {
        let callable = self.callable.map_ground_types(&mut map);
        Self {
            kind: self.kind,
            callable: Arc::new(callable),
            effects: self.effects.clone(),
            inferred_effects: self.inferred_effects,
        }
    }
}

impl CallableScheme {
    pub(crate) fn new(
        type_parameters: Vec<SchemeTypeParameterId>,
        row_parameters: Vec<SchemeRowParameter>,
        params: Vec<SchemeParameter>,
        return_type: SchemeType,
        return_schema: Option<SchemaExpectation>,
        explicit_return: bool,
    ) -> Result<Self, SchemeBuildError> {
        let type_ids = type_parameters.iter().copied().collect::<BTreeSet<_>>();
        if type_ids.len() != type_parameters.len() {
            return Err(SchemeBuildError::DuplicateTypeParameter);
        }
        let row_ids = row_parameters
            .iter()
            .map(|parameter| parameter.id)
            .collect::<BTreeSet<_>>();
        if row_ids.len() != row_parameters.len() {
            return Err(SchemeBuildError::DuplicateRowParameter);
        }
        let mut used_types = BTreeSet::new();
        let mut used_rows = BTreeSet::new();
        let mut row_requirements = BTreeMap::<SchemeRowParameterId, BTreeSet<Name>>::new();
        for parameter in &params {
            if let Some(schema) = &parameter.schema_expectation { validate_schema_ground_types(schema)?; }
            parameter.ty.validate(
                &type_ids,
                &row_ids,
                &mut used_types,
                &mut used_rows,
                &mut row_requirements,
            )?;
        }
        return_type.validate(
            &type_ids,
            &row_ids,
            &mut used_types,
            &mut used_rows,
            &mut row_requirements,
        )?;
        if let Some(schema) = &return_schema { validate_schema_ground_types(schema)?; }
        if used_types != type_ids {
            return Err(SchemeBuildError::UnusedTypeParameter);
        }
        if used_rows != row_ids {
            return Err(SchemeBuildError::UnusedRowParameter);
        }
        if row_parameters.iter().any(|parameter| {
            !row_requirements
                .get(&parameter.id)
                .is_some_and(|required| required.is_subset(&parameter.lacks))
        }) {
            return Err(SchemeBuildError::RowLacksMismatch);
        }
        Ok(Self {
            type_parameters: type_parameters.into(),
            row_parameters: row_parameters.into(),
            monotype: CallableMonotype::new(params, return_type, return_schema),
            explicit_return,
        })
    }

    pub(crate) fn type_parameters(&self) -> &[SchemeTypeParameterId] {
        &self.type_parameters
    }

    pub(crate) fn row_parameters(&self) -> &[SchemeRowParameter] {
        &self.row_parameters
    }

    pub(crate) fn params(&self) -> &[SchemeParameter] {
        self.monotype.params()
    }

    pub(crate) fn return_type(&self) -> &SchemeType {
        self.monotype.return_type()
    }

    pub(crate) fn return_schema(&self) -> Option<&SchemaExpectation> {
        self.monotype.return_schema()
    }

    pub(crate) fn explicit_return(&self) -> bool {
        self.explicit_return
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        let mut bytes = size_of::<Self>();
        bytes = bytes.saturating_add(self.type_parameters.len() * size_of::<SchemeTypeParameterId>());
        bytes = bytes.saturating_add(self.row_parameters.len() * size_of::<SchemeRowParameter>());
        bytes = bytes.saturating_add(self.monotype.retained_bytes().saturating_sub(size_of::<CallableMonotype>()));
        bytes = bytes.saturating_add(
            self.row_parameters
                .iter()
                .map(|parameter| {
                    parameter.lacks.len() * size_of::<Name>()
                        + parameter
                            .lacks
                            .iter()
                            .map(|name| name.as_str().len())
                            .sum::<usize>()
                })
                .sum::<usize>(),
        );
        bytes
    }

    fn map_ground_types(&self, map: &mut impl FnMut(&mut Type)) -> Self {
        Self {
            type_parameters: self.type_parameters.clone(),
            row_parameters: self.row_parameters.clone(),
            monotype: self.monotype.map_ground_types(map),
            explicit_return: self.explicit_return,
        }
    }
}

impl CallableMonotype {
    pub(crate) fn new(
        params: Vec<SchemeParameter>,
        return_type: SchemeType,
        return_schema: Option<SchemaExpectation>,
    ) -> Self {
        Self { params: params.into(), return_type, return_schema }
    }

    pub(crate) fn params(&self) -> &[SchemeParameter] {
        &self.params
    }

    pub(crate) fn return_type(&self) -> &SchemeType {
        &self.return_type
    }

    pub(crate) fn return_schema(&self) -> Option<&SchemaExpectation> {
        self.return_schema.as_ref()
    }

    pub(crate) fn has_schema_contract(&self) -> bool {
        self.return_schema.is_some() || self.params.iter().any(|parameter| parameter.schema_expectation.is_some())
    }

    fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        let mut bytes = size_of::<Self>() + self.params.len() * size_of::<SchemeParameter>();
        let mut counted_schema_children = BTreeSet::new();
        for parameter in self.params.iter() {
            bytes = bytes
                .saturating_add(parameter.name.as_str().len())
                .saturating_add(
                    parameter
                        .ty
                        .retained_bytes()
                        .saturating_sub(size_of::<SchemeType>()),
                );
            if let Some(expectation) = &parameter.schema_expectation {
                bytes = bytes.saturating_add(schema_expectation_retained_bytes(
                    expectation,
                    &mut counted_schema_children,
                ));
            }
        }
        bytes = bytes
            .saturating_add(
                self.return_type
                    .retained_bytes()
                    .saturating_sub(size_of::<SchemeType>()),
            )
            .saturating_add(self.return_schema.as_ref().map_or(0, |expectation| {
                schema_expectation_retained_bytes(expectation, &mut counted_schema_children)
            }));
        bytes
    }

    fn map_ground_types(&self, map: &mut impl FnMut(&mut Type)) -> Self {
        let mut params = self.params.to_vec();
        for parameter in &mut params {
            parameter.ty.map_ground_types(map);
        }
        let mut return_type = self.return_type.clone();
        return_type.map_ground_types(map);
        Self {
            params: params.into(),
            return_type,
            return_schema: self.return_schema.clone(),
        }
    }
}

impl CallInstantiation {
    pub(crate) fn new(
        callable: &CallableScheme,
        type_arguments: BTreeMap<SchemeTypeParameterId, SchemeType>,
        row_arguments: BTreeMap<SchemeRowParameterId, RowEvidence>,
        caller_types: &[SchemeTypeParameterId],
        caller_rows: &[SchemeRowParameter],
    ) -> Result<Self, SchemeBuildError> {
        if type_arguments.keys().copied().collect::<BTreeSet<_>>()
            != callable.type_parameters.iter().copied().collect()
        {
            return Err(SchemeBuildError::UnboundTypeParameter);
        }
        if row_arguments.keys().copied().collect::<BTreeSet<_>>()
            != callable
                .row_parameters
                .iter()
                .map(|parameter| parameter.id)
                .collect()
        {
            return Err(SchemeBuildError::UnboundRowParameter);
        }
        let caller_type_ids = caller_types.iter().copied().collect::<BTreeSet<_>>();
        let caller_row_parameters = caller_rows
            .iter()
            .map(|parameter| (parameter.id, parameter))
            .collect::<BTreeMap<_, _>>();
        let caller_row_ids = caller_row_parameters.keys().copied().collect::<BTreeSet<_>>();
        let mut used_types = BTreeSet::new();
        let mut used_rows = BTreeSet::new();
        let mut row_requirements = BTreeMap::new();
        for ty in type_arguments.values() {
            if let Some(error) = ty.body_build_error() {
                return Err(error);
            }
            ty.validate(
                &caller_type_ids,
                &caller_row_ids,
                &mut used_types,
                &mut used_rows,
                &mut row_requirements,
            )?;
        }
        for parameter in callable.row_parameters() {
            let evidence = &row_arguments[&parameter.id];
            for ty in evidence.fields.values() {
                if let Some(error) = ty.body_build_error() {
                    return Err(error);
                }
                ty.validate(
                    &caller_type_ids,
                    &caller_row_ids,
                    &mut used_types,
                    &mut used_rows,
                    &mut row_requirements,
                )?;
            }
            if evidence
                .fields
                .keys()
                .any(|field| parameter.lacks.contains(field))
            {
                return Err(SchemeBuildError::RowLacksMismatch);
            }
            if let SchemeRowTail::Parameter(id) = evidence.tail {
                let Some(caller) = caller_row_parameters.get(&id) else {
                    return Err(SchemeBuildError::UnboundRowParameter);
                };
                if !parameter
                    .lacks
                    .iter()
                    .chain(evidence.fields.keys())
                    .all(|field| caller.lacks.contains(field))
                {
                    return Err(SchemeBuildError::RowLacksMismatch);
                }
            }
        }
        if row_requirements.iter().any(|(id, required)| {
            caller_row_parameters
                .get(id)
                .is_none_or(|caller| !required.is_subset(&caller.lacks))
        }) {
            return Err(SchemeBuildError::RowLacksMismatch);
        }
        Ok(Self {
            type_arguments,
            row_arguments,
        })
    }

    pub(crate) fn type_arguments(&self) -> &BTreeMap<SchemeTypeParameterId, SchemeType> {
        &self.type_arguments
    }

    pub(crate) fn row_arguments(&self) -> &BTreeMap<SchemeRowParameterId, RowEvidence> {
        &self.row_arguments
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        size_of::<Self>()
            .saturating_add(
                self.type_arguments.len() * size_of::<SchemeTypeParameterId>(),
            )
            .saturating_add(self.type_arguments.values().map(SchemeType::retained_bytes).sum::<usize>())
            .saturating_add(
                self.row_arguments.len() * size_of::<SchemeRowParameterId>(),
            )
            .saturating_add(
                self.row_arguments
                    .values()
                    .map(RowEvidence::retained_bytes)
                    .sum::<usize>(),
            )
    }
}

impl RowEvidence {
    pub(crate) fn new(
        fields: BTreeMap<Name, SchemeType>,
        tail: SchemeRowTail,
    ) -> Result<Self, SchemeBuildError> {
        for ty in fields.values() {
            if let Some(error) = ty.body_build_error() {
                return Err(error);
            }
        }
        Ok(Self { fields, tail })
    }

    pub(crate) fn fields(&self) -> &BTreeMap<Name, SchemeType> {
        &self.fields
    }

    pub(crate) fn tail(&self) -> SchemeRowTail {
        self.tail
    }

    fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        size_of::<Self>()
            .saturating_add(self.fields.len() * size_of::<(Name, SchemeType)>())
            .saturating_add(self.fields.iter().map(|(name, ty)| {
                name.as_str().len() + ty.retained_bytes().saturating_sub(size_of::<SchemeType>())
            }).sum::<usize>())
    }
}

impl SchemeBodyFacts {
    pub(crate) fn new(
        scheme: Arc<TypeScheme>,
        expression_types: BTreeMap<ExprId, SchemeType>,
        tail_types: BTreeMap<StmtId, SchemeType>,
    ) -> Result<Self, SchemeBuildError> {
        let type_ids = scheme.type_parameters().iter().copied().collect::<BTreeSet<_>>();
        let row_ids = scheme
            .row_parameters()
            .iter()
            .map(|parameter| parameter.id)
            .collect::<BTreeSet<_>>();
        let mut used_types = BTreeSet::new();
        let mut used_rows = BTreeSet::new();
        let mut row_requirements = BTreeMap::new();
        for ty in expression_types.values().chain(tail_types.values()) {
            if let Some(error) = ty.body_build_error() {
                return Err(error);
            }
            ty.validate(
                &type_ids,
                &row_ids,
                &mut used_types,
                &mut used_rows,
                &mut row_requirements,
            )?;
        }
        Ok(Self {
            scheme,
            expression_types,
            tail_types,
            registered_calls: BTreeMap::new(),
            read_bindings: BTreeSet::new(),
        })
    }

    pub(crate) fn with_registered_calls(mut self, calls: BTreeMap<ExprId, Arc<RegisteredCallPlan>>) -> Self {
        self.registered_calls = calls;
        self
    }

    pub(crate) fn with_read_bindings(mut self, bindings: BTreeSet<super::check::BindingId>) -> Self {
        self.read_bindings = bindings;
        self
    }

    pub(crate) fn read_bindings(&self) -> &BTreeSet<super::check::BindingId> {
        &self.read_bindings
    }

    pub(crate) fn registered_calls(&self) -> &BTreeMap<ExprId, Arc<RegisteredCallPlan>> {
        &self.registered_calls
    }

    pub(crate) fn scheme(&self) -> &Arc<TypeScheme> {
        &self.scheme
    }

    pub(crate) fn expression_types(&self) -> &BTreeMap<ExprId, SchemeType> {
        &self.expression_types
    }

    pub(crate) fn tail_types(&self) -> &BTreeMap<StmtId, SchemeType> {
        &self.tail_types
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        size_of::<Self>()
            .saturating_add(self.scheme.retained_bytes())
            .saturating_add(self.expression_types.len() * size_of::<(ExprId, SchemeType)>())
            .saturating_add(
                self.expression_types
                    .values()
                    .map(SchemeType::retained_bytes)
                    .sum::<usize>(),
            )
            .saturating_add(self.read_bindings.len() * size_of::<super::check::BindingId>())
            .saturating_add(self.registered_calls.len() * size_of::<(ExprId, Arc<RegisteredCallPlan>)>())
            .saturating_add(self.registered_calls.values().map(|plan| plan.retained_bytes()).sum::<usize>())
            .saturating_add(self.tail_types.len() * size_of::<(StmtId, SchemeType)>())
            .saturating_add(
                self.tail_types
                    .values()
                    .map(SchemeType::retained_bytes)
                    .sum::<usize>(),
            )
    }
}

fn schema_expectation_retained_bytes(
    expectation: &SchemaExpectation,
    counted_children: &mut BTreeSet<usize>,
) -> usize {
    use std::mem::size_of;

    let mut bytes = expectation.instances.capacity() * size_of::<SchemaInstance>();
    for instance in &expectation.instances {
        bytes = bytes.saturating_add(instance.arguments.capacity() * size_of::<Type>());
        for argument in &instance.arguments {
            bytes = bytes.saturating_add(
                argument
                    .retained_bytes()
                    .saturating_sub(size_of::<Type>()),
            );
        }
    }
    if counted_children.insert(Arc::as_ptr(&expectation.children) as usize) {
        bytes = bytes.saturating_add(size_of::<BTreeMap<SchemaComponent, SchemaExpectation>>());
        bytes = bytes.saturating_add(
            expectation.children.len() * size_of::<(SchemaComponent, SchemaExpectation)>(),
        );
        for (component, child) in expectation.children.iter() {
            if let SchemaComponent::Field(name) = component {
                bytes = bytes.saturating_add(name.as_str().len());
            }
            bytes = bytes.saturating_add(schema_expectation_retained_bytes(
                child,
                counted_children,
            ));
        }
    }
    bytes
}

impl SchemeType {
    pub(crate) fn closed_ground_type(&self) -> Option<Type> {
        fn project(ty: &SchemeType, depth: usize) -> Option<Type> {
            if depth > crate::sema::constraints::MAX_TYPE_DEPTH {
                return None;
            }
            let nested = |ty: &SchemeType| project(ty, depth + 1);
            match ty {
                SchemeType::Ground(ty)
                    if !ty.contains_inference()
                        && !ty.contains_recovery()
                        && !SchemeType::contains_builtin_parameter(ty) =>
                {
                    Some(ty.clone())
                }
                SchemeType::Ground(_) | SchemeType::Parameter(_) => None,
                SchemeType::Callable { kind, monotype, effects } => {
                    if monotype.has_schema_contract() {
                        return None;
                    }
                    let pure = match kind {
                        crate::sema::types::CallableKind::Pure => true,
                        crate::sema::types::CallableKind::Proc => false,
                        crate::sema::types::CallableKind::Stream => return None,
                    };
                    Some(Type::Callable(Arc::new(super::types::TypedCallable {
                        pure,
                        sig: super::types::CallableType {
                            params: monotype.params().iter().map(|parameter| Some(super::types::CallableParamType {
                                name: parameter.name,
                                ty: nested(&parameter.ty)?,
                                defaulted: parameter.defaulted,
                                rest: parameter.rest,
                            })).collect::<Option<Vec<_>>>()?,
                            return_ty: Box::new(nested(monotype.return_type())?),
                            effects: effects.as_ref().map(|effects| effects.to_vec()),
                        },
                    })))
                }
                SchemeType::List(inner) => Some(Type::List(Box::new(nested(inner)?))),
                SchemeType::Map(key, value) => Some(Type::Map(
                    Box::new(nested(key)?),
                    Box::new(nested(value)?),
                )),
                SchemeType::Stream(inner) => Some(Type::Stream(Box::new(nested(inner)?))),
                SchemeType::Set(inner) => Some(Type::Set(Box::new(nested(inner)?))),
                SchemeType::Result(ok, error) => Some(Type::Result(
                    Box::new(nested(ok)?),
                    Box::new(nested(error)?),
                )),
                SchemeType::Optional(inner) => Some(Type::Optional(Box::new(nested(inner)?))),
                SchemeType::Record { fields, tail: SchemeRowTail::Closed } => {
                    let fields = fields
                        .iter()
                        .map(|(&name, ty)| Some((name, nested(ty)?)))
                        .collect::<Option<BTreeMap<_, _>>>()?;
                    Some(Type::Record(Arc::new(fields)))
                }
                SchemeType::Record { tail: SchemeRowTail::Parameter(_), .. } => None,
                SchemeType::Union(members) => Some(Type::Union(
                    members.iter().map(nested).collect::<Option<Vec<_>>>()?,
                )),
                SchemeType::Validated(validation, base) => {
                    let base = nested(base)?;
                    crate::sema::validated::ValidatedType::new(*validation, base)
                        .ok()
                        .map(|validated| Type::Validated(Box::new(validated)))
                }
            }
        }

        project(self, 0)
    }

    fn contains_builtin_parameter(ty: &Type) -> bool {
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            match ty {
                Type::BuiltinParameter(_) => return true,
                Type::List(inner)
                | Type::Stream(inner)
                | Type::Set(inner)
                | Type::Optional(inner) => pending.push(inner),
                Type::Map(key, value) | Type::Result(key, value) => {
                    pending.push(key);
                    pending.push(value);
                }
                Type::Module(module) => {
                    for export in module.exports.values() {
                        match export {
                            ModuleExportType::Value { ty, .. } => pending.push(ty),
                            ModuleExportType::Proc { sig, .. }
                            | ModuleExportType::Pure { sig, .. } => {
                                pending.push(&sig.return_ty);
                                pending.extend(sig.params.iter().map(|parameter| &parameter.ty));
                            }
                            ModuleExportType::GenericCallable { .. } => {}
                        }
                    }
                }
                Type::Record(fields) => pending.extend(fields.values()),
                Type::Union(members) => pending.extend(members),
                Type::Validated(validated) => pending.push(validated.base()),
                Type::Callable(callable) => {
                    pending.push(&callable.sig.return_ty);
                    pending.extend(callable.sig.params.iter().map(|parameter| &parameter.ty));
                }
                _ => {}
            }
        }
        false
    }

    pub(crate) fn contains_parameter(&self) -> bool {
        match self {
            Self::Parameter(_) => true,
            Self::Callable { monotype, .. } => monotype.params().iter().any(|parameter| parameter.ty.contains_parameter())
                || monotype.return_type().contains_parameter(),
            Self::List(inner)
            | Self::Stream(inner)
            | Self::Set(inner)
            | Self::Optional(inner)
            | Self::Validated(_, inner) => inner.contains_parameter(),
            Self::Map(key, value) | Self::Result(key, value) => {
                key.contains_parameter() || value.contains_parameter()
            }
            Self::Record { fields, tail } => {
                matches!(tail, SchemeRowTail::Parameter(_))
                    || fields.values().any(Self::contains_parameter)
            }
            Self::Union(members) => members.iter().any(Self::contains_parameter),
            Self::Ground(_) => false,
        }
    }

    pub(crate) fn matches_body_return(&self, expected: &Self) -> bool {
        if self == expected { return true; }
        match (self.closed_ground_type(), expected.closed_ground_type()) {
            (Some(actual), Some(expected)) => super::types::tail_type_matches_expected(&expected, &actual),
            _ => false,
        }
    }

    pub(crate) fn may_hold_resource(&self) -> bool {
        let mut pending = vec![self];
        while let Some(ty) = pending.pop() {
            match ty {
                Self::Ground(ty) if ty.may_hold_resource() => return true,
                Self::Ground(_) => {}
                Self::Parameter(_) => return true,
                Self::Callable { .. } => return true,
                Self::List(inner)
                | Self::Set(inner)
                | Self::Optional(inner)
                | Self::Validated(_, inner) => pending.push(inner),
                Self::Stream(_) => return true,
                Self::Map(key, value) | Self::Result(key, value) => {
                    pending.push(key);
                    pending.push(value);
                }
                Self::Record { fields, tail } => {
                    if matches!(tail, SchemeRowTail::Parameter(_)) {
                        return true;
                    }
                    pending.extend(fields.values());
                }
                Self::Union(members) => pending.extend(members),
            }
        }
        false
    }

    fn map_ground_types(&mut self, map: &mut impl FnMut(&mut Type)) {
        match self {
            Self::Ground(ty) => map(ty),
            Self::Callable { monotype, .. } => *monotype = Arc::new(monotype.map_ground_types(map)),
            Self::List(inner)
            | Self::Stream(inner)
            | Self::Set(inner)
            | Self::Optional(inner)
            | Self::Validated(_, inner) => inner.map_ground_types(map),
            Self::Map(key, value) => {
                key.map_ground_types(map);
                value.map_ground_types(map);
            }
            Self::Result(ok, err) => {
                ok.map_ground_types(map);
                err.map_ground_types(map);
            }
            Self::Record { fields, .. } => {
                for field in fields.values_mut() {
                    field.map_ground_types(map);
                }
            }
            Self::Union(members) => {
                for member in members {
                    member.map_ground_types(map);
                }
            }
            Self::Parameter(_) => {}
        }
    }

    fn body_build_error(&self) -> Option<SchemeBuildError> {
        match self {
            Self::Ground(ty) if ty.contains_any() => Some(SchemeBuildError::DynamicType),
            Self::Ground(ty) if ty.contains_recovery() => Some(SchemeBuildError::RecoveryType),
            Self::Ground(ty) if ty.contains_inference() => Some(SchemeBuildError::UnresolvedType),
            Self::Ground(_) | Self::Parameter(_) => None,
            Self::Callable { monotype, .. } => monotype.params().iter()
                .find_map(|parameter| parameter.ty.body_build_error())
                .or_else(|| monotype.return_type().body_build_error()),
            Self::List(inner)
            | Self::Stream(inner)
            | Self::Set(inner)
            | Self::Optional(inner)
            | Self::Validated(_, inner) => inner.body_build_error(),
            Self::Map(key, value) | Self::Result(key, value) => {
                key.body_build_error().or_else(|| value.body_build_error())
            }
            Self::Record { fields, .. } => {
                fields.values().find_map(Self::body_build_error)
            }
            Self::Union(members) => members.iter().find_map(Self::body_build_error),
        }
    }

    fn validate(
        &self,
        type_ids: &BTreeSet<SchemeTypeParameterId>,
        row_ids: &BTreeSet<SchemeRowParameterId>,
        used_types: &mut BTreeSet<SchemeTypeParameterId>,
        used_rows: &mut BTreeSet<SchemeRowParameterId>,
        row_requirements: &mut BTreeMap<SchemeRowParameterId, BTreeSet<Name>>,
    ) -> Result<(), SchemeBuildError> {
        match self {
            Self::Ground(ty) => validate_ground(ty),
            Self::Callable { monotype, .. } => {
                for parameter in monotype.params() {
                    if let Some(schema) = &parameter.schema_expectation { validate_schema_ground_types(schema)?; }
                    parameter.ty.validate(type_ids, row_ids, used_types, used_rows, row_requirements)?;
                }
                if let Some(schema) = monotype.return_schema() { validate_schema_ground_types(schema)?; }
                monotype.return_type().validate(type_ids, row_ids, used_types, used_rows, row_requirements)
            }
            Self::Parameter(id) => {
                if type_ids.contains(id) {
                    used_types.insert(*id);
                    Ok(())
                } else {
                    Err(SchemeBuildError::UnboundTypeParameter)
                }
            }
            Self::List(ty) | Self::Stream(ty) | Self::Set(ty) | Self::Optional(ty) => {
                ty.validate(type_ids, row_ids, used_types, used_rows, row_requirements)
            }
            Self::Map(key, value) | Self::Result(key, value) => {
                key.validate(type_ids, row_ids, used_types, used_rows, row_requirements)?;
                value.validate(type_ids, row_ids, used_types, used_rows, row_requirements)
            }
            Self::Record { fields, tail } => {
                for ty in fields.values() {
                    ty.validate(type_ids, row_ids, used_types, used_rows, row_requirements)?;
                }
                if let SchemeRowTail::Parameter(id) = tail {
                    if row_ids.contains(id) {
                        used_rows.insert(*id);
                        row_requirements
                            .entry(*id)
                            .or_default()
                            .extend(fields.keys().copied());
                        Ok(())
                    } else {
                        Err(SchemeBuildError::UnboundRowParameter)
                    }
                } else {
                    Ok(())
                }
            }
            Self::Union(types) => {
                for ty in types {
                    ty.validate(type_ids, row_ids, used_types, used_rows, row_requirements)?;
                }
                Ok(())
            }
            Self::Validated(validation, ty) => {
                let valid_base = match validation {
                    Validation::NonEmpty => matches!(ty.as_ref(), Self::List(_)),
                    Validation::RelPath => matches!(ty.as_ref(), Self::Ground(Type::Path)),
                    Validation::Nominal(_) => matches!(ty.as_ref(), Self::Record { .. }),
                    Validation::Range(range) => match ty.as_ref() {
                        Self::Ground(Type::Int) => true,
                        Self::Ground(Type::UInt) => range.low() >= 0,
                        _ => false,
                    },
                };
                if !valid_base {
                    return Err(SchemeBuildError::InvalidValidatedBase);
                }
                ty.validate(type_ids, row_ids, used_types, used_rows, row_requirements)
            }
        }
    }

    fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        let mut bytes = size_of::<Self>();
        match self {
            Self::Ground(ty) => {
                bytes = bytes.saturating_add(ty.retained_bytes().saturating_sub(size_of::<Type>()));
            }
            Self::Parameter(_) => {}
            Self::Callable { monotype, effects, .. } => {
                bytes = bytes.saturating_add(monotype.retained_bytes())
                    .saturating_add(effects.as_ref().map_or(0, |effects| std::mem::size_of_val(effects.as_ref())));
            }
            Self::List(ty)
            | Self::Stream(ty)
            | Self::Set(ty)
            | Self::Optional(ty)
            | Self::Validated(_, ty) => bytes = bytes.saturating_add(ty.retained_bytes()),
            Self::Map(key, value) | Self::Result(key, value) => {
                bytes = bytes
                    .saturating_add(key.retained_bytes())
                    .saturating_add(value.retained_bytes());
            }
            Self::Record { fields, .. } => {
                bytes = bytes.saturating_add(fields.len() * size_of::<(Name, Self)>());
                for (name, ty) in fields {
                    bytes = bytes
                        .saturating_add(name.as_str().len())
                        .saturating_add(ty.retained_bytes().saturating_sub(size_of::<Self>()));
                }
            }
            Self::Union(types) => {
                bytes = bytes.saturating_add(types.capacity() * size_of::<Self>());
                for ty in types {
                    bytes = bytes.saturating_add(ty.retained_bytes().saturating_sub(size_of::<Self>()));
                }
            }
        }
        bytes
    }
}

fn validate_ground(ty: &Type) -> Result<(), SchemeBuildError> {
    if ty.contains_inference() {
        Err(SchemeBuildError::UnresolvedType)
    } else if ty.contains_recovery() {
        Err(SchemeBuildError::RecoveryType)
    } else {
        Ok(())
    }
}

fn validate_schema_ground_types(schema: &SchemaExpectation) -> Result<(), SchemeBuildError> {
    let mut pending = vec![schema];
    while let Some(schema) = pending.pop() {
        for instance in &schema.instances {
            for argument in &instance.arguments { validate_ground(argument)?; }
        }
        pending.extend(schema.children.values());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SchemeBuildError, SchemeType, TypeScheme};
    use crate::sema::constants::{SchemaComponent, SchemaExpectation, SchemaInstance};
    use crate::sema::schemes::{
        CallInstantiation, CallableScheme, RowEvidence, SchemeParameter,
        SchemeRowParameter, SchemeRowParameterId, SchemeRowTail,
    };
    use crate::sema::types::{ModuleExportType, ModuleType, Type};
    use crate::symbol::Name;
    use crate::syntax::arena::TypeDefId;
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    #[test]
    fn generic_proc_schemes_reject_unresolved_effects() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let binder = super::SchemeTypeParameterId(0);
            let callable = Arc::new(super::CallableScheme::new(vec![binder], Vec::new(),
                vec![SchemeParameter { name: Name::intern("value"), ty: SchemeType::Parameter(binder),
                    schema_expectation: None, defaulted: false, rest: false }],
                SchemeType::Result(Box::new(SchemeType::Parameter(binder)), Box::new(SchemeType::Ground(Type::Error))),
                None, false).unwrap());
            assert_eq!(TypeScheme::with_callable(crate::sema::types::CallableKind::Proc, Arc::clone(&callable), None, true), Err(SchemeBuildError::UnresolvedEffects));
            assert!(TypeScheme::with_callable(crate::sema::types::CallableKind::Proc, callable, Some(Vec::new()), true).is_ok());
        });
    }

    #[test]
    fn nested_callable_ground_projection_preserves_schema_contracts() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let callback = |parameter_schema, return_schema| SchemeType::Callable {
                kind: crate::sema::types::CallableKind::Pure,
                monotype: Arc::new(super::CallableMonotype::new(
                    vec![SchemeParameter {
                        name: Name::intern("value"),
                        ty: SchemeType::Ground(Type::Int),
                        schema_expectation: parameter_schema,
                        defaulted: false,
                        rest: false,
                    }],
                    SchemeType::Ground(Type::Int),
                    return_schema,
                )),
                effects: Some(Arc::from([])),
            };
            assert!(matches!(callback(None, None).closed_ground_type(), Some(Type::Callable(_))));
            let schema = SchemaExpectation {
                instances: vec![SchemaInstance { definition: TypeDefId::from_index(0), arguments: vec![Type::Int] }],
                ..SchemaExpectation::default()
            };
            assert_eq!(callback(Some(schema.clone()), None).closed_ground_type(), None);
            assert_eq!(callback(None, Some(schema)).closed_ground_type(), None);
        });
    }

    #[test]
    fn nested_callable_schema_rejects_solver_references() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let mut constraints = crate::sema::constraints::TypeConstraints::default();
            let variable = constraints.fresh(crate::source::Span::at(crate::source::SourceId::new(0), 0));
            let schema = SchemaExpectation {
                instances: vec![SchemaInstance { definition: TypeDefId::from_index(0), arguments: vec![variable] }],
                ..SchemaExpectation::default()
            };
            for schema_in_parameter in [false, true] {
                let callback = SchemeType::Callable {
                    kind: crate::sema::types::CallableKind::Pure,
                    monotype: Arc::new(super::CallableMonotype::new(
                        vec![SchemeParameter {
                            name: Name::intern("value"), ty: SchemeType::Ground(Type::Int),
                            schema_expectation: schema_in_parameter.then(|| schema.clone()), defaulted: false, rest: false,
                        }],
                        SchemeType::Ground(Type::Int),
                        (!schema_in_parameter).then(|| schema.clone()),
                    )),
                    effects: Some(Arc::from([])),
                };
                let result = TypeScheme::new(crate::sema::types::CallableKind::Pure, Vec::new(), Vec::new(),
                    vec![SchemeParameter { name: Name::intern("action"), ty: callback, schema_expectation: None, defaulted: false, rest: false }],
                    SchemeType::Ground(Type::Int), None, false, None, false);
                assert_eq!(result, Err(SchemeBuildError::UnresolvedType));
            }
        });
    }

    #[test]
    fn closed_scheme_types_project_every_ground_shape() {
        use crate::sema::validated::{IntRange, Validation};

        crate::symbol::SymbolOwner::new().with_current(|| {
            let int = || SchemeType::Ground(Type::Int);
            let path = || SchemeType::Ground(Type::Path);
            let record = || SchemeType::Record {
                fields: BTreeMap::from([(Name::intern("path"), path())]),
                tail: SchemeRowTail::Closed,
            };
            let cases = [
                (
                    SchemeType::List(Box::new(path())),
                    Type::List(Box::new(Type::Path)),
                ),
                (
                    SchemeType::Map(Box::new(int()), Box::new(path())),
                    Type::Map(Box::new(Type::Int), Box::new(Type::Path)),
                ),
                (
                    SchemeType::Stream(Box::new(int())),
                    Type::Stream(Box::new(Type::Int)),
                ),
                (SchemeType::Set(Box::new(path())), Type::Set(Box::new(Type::Path))),
                (
                    SchemeType::Result(Box::new(int()), Box::new(path())),
                    Type::Result(Box::new(Type::Int), Box::new(Type::Path)),
                ),
                (
                    SchemeType::Optional(Box::new(path())),
                    Type::Optional(Box::new(Type::Path)),
                ),
                (
                    SchemeType::Record {
                        fields: BTreeMap::from([(Name::intern("path"), path())]),
                        tail: SchemeRowTail::Closed,
                    },
                    Type::Record(Arc::new(BTreeMap::from([(Name::intern("path"), Type::Path)]))),
                ),
                (
                    SchemeType::Union(vec![int(), path()]),
                    Type::Union(vec![Type::Int, Type::Path]),
                ),
                (
                    SchemeType::Validated(
                        Validation::NonEmpty,
                        Box::new(SchemeType::List(Box::new(path()))),
                    ),
                    Type::non_empty(Type::Path),
                ),
                (
                    SchemeType::Validated(Validation::RelPath, Box::new(path())),
                    Type::rel_path(),
                ),
                (
                    SchemeType::Validated(
                        Validation::Nominal(Name::intern("Package")),
                        Box::new(record()),
                    ),
                    Type::nominal(
                        Name::intern("Package"),
                        Type::Record(Arc::new(BTreeMap::from([(Name::intern("path"), Type::Path)]))),
                    ).unwrap(),
                ),
                (
                    SchemeType::Validated(
                        Validation::Range(IntRange::new(1, 8).unwrap()),
                        Box::new(int()),
                    ),
                    Type::bounded(IntRange::new(1, 8).unwrap(), Type::Int).unwrap(),
                ),
            ];
            for (scheme, expected) in cases {
                assert_eq!(scheme.closed_ground_type(), Some(expected));
            }
        });
    }

    #[test]
    fn closed_scheme_type_projection_accepts_any_and_rejects_open_shapes() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            assert_eq!(
                SchemeType::Ground(Type::Any).closed_ground_type(),
                Some(Type::Any),
            );
            assert_eq!(
                SchemeType::Ground(Type::Invalid).closed_ground_type(),
                None,
            );
            assert_eq!(
                SchemeType::List(Box::new(SchemeType::Parameter(
                    super::SchemeTypeParameterId(0),
                )))
                .closed_ground_type(),
                None,
            );
            assert_eq!(
                SchemeType::Record {
                    fields: BTreeMap::from([(Name::intern("path"), SchemeType::Ground(Type::Path))]),
                    tail: SchemeRowTail::Parameter(super::SchemeRowParameterId(0)),
                }
                .closed_ground_type(),
                None,
            );
        });
    }

    #[test]
    fn closed_scheme_type_projection_rejects_builtin_parameters() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let builtin_parameter = Type::BuiltinParameter(
                crate::sema::types::BuiltinTypeParameter::Element,
            );
            assert_eq!(
                SchemeType::Ground(builtin_parameter.clone()).closed_ground_type(),
                None,
            );
            assert_eq!(
                SchemeType::List(Box::new(SchemeType::Ground(Type::Record(Arc::new(
                    BTreeMap::from([(
                        Name::intern("items"),
                        Type::List(Box::new(builtin_parameter)),
                    )]),
                )))))
                .closed_ground_type(),
                None,
            );
        });
    }

    fn rejects_recovery_ground(ty: Type) {
        assert!(matches!(
            TypeScheme::new(
                crate::sema::types::CallableKind::Pure,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SchemeType::Ground(ty),
                None,
                false,
                Some(Vec::new()),
                false,
            ),
            Err(SchemeBuildError::RecoveryType)
        ));
    }

    #[test]
    fn type_scheme_rejects_recovery_nested_in_sets_and_module_exports() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            rejects_recovery_ground(Type::Set(Box::new(Type::Unknown)));
            rejects_recovery_ground(Type::Set(Box::new(Type::Invalid)));
            rejects_recovery_ground(Type::Module(Arc::new(ModuleType::open(BTreeMap::from([(
                Name::intern("value"),
                ModuleExportType::Value {
                    ty: Type::Unknown,
                    optional: false,
                },
            )])))));
        });
    }

    #[test]
    fn checked_scheme_reuses_its_callable_shape() {
        let callable = Arc::new(
            super::super::schemes::CallableScheme::new(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SchemeType::Ground(Type::Int),
                None,
                false,
            )
            .unwrap(),
        );
        let scheme = TypeScheme::with_callable(
            crate::sema::types::CallableKind::Pure,
            Arc::clone(&callable),
            None,
            false,
        )
        .unwrap();
        assert!(Arc::ptr_eq(scheme.callable(), &callable));
    }

    #[test]
    fn scheme_ground_type_mapping_visits_both_map_components() {
        let scheme = TypeScheme::new(
            crate::sema::types::CallableKind::Pure,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            SchemeType::Map(
                Box::new(SchemeType::Ground(Type::Int)),
                Box::new(SchemeType::Ground(Type::Int)),
            ),
            None,
            false,
            None,
            false,
        )
        .unwrap();

        let mapped = scheme.map_ground_types(|ty| {
            if *ty == Type::Int {
                *ty = Type::Str;
            }
        });

        assert_eq!(
            mapped.return_type(),
            &SchemeType::Map(
                Box::new(SchemeType::Ground(Type::Str)),
                Box::new(SchemeType::Ground(Type::Str)),
            )
        );
    }

    #[test]
    fn row_evidence_rejects_any_and_call_instantiations_require_valid_evidence() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let field = Name::intern("dynamic");
            assert_eq!(
                RowEvidence::new(
                    BTreeMap::from([(
                        field,
                        SchemeType::Ground(Type::Set(Box::new(Type::Any))),
                    )]),
                    SchemeRowTail::Closed,
                ),
                Err(SchemeBuildError::DynamicType)
            );

            let row_id = SchemeRowParameterId(0);
            let callable = CallableScheme::new(
                Vec::new(),
                vec![SchemeRowParameter {
                    id: row_id,
                    lacks: BTreeSet::new(),
                }],
                vec![SchemeParameter {
                    name: Name::intern("record"),
                    ty: SchemeType::Record {
                        fields: BTreeMap::new(),
                        tail: SchemeRowTail::Parameter(row_id),
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
            let evidence = RowEvidence::new(
                BTreeMap::from([(field, SchemeType::Ground(Type::Int))]),
                SchemeRowTail::Closed,
            )
            .unwrap();
            assert!(CallInstantiation::new(
                &callable,
                BTreeMap::new(),
                BTreeMap::from([(row_id, evidence)]),
                &[],
                &[],
            )
            .is_ok());
            assert!(TypeScheme::new(
                crate::sema::types::CallableKind::Pure,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SchemeType::Ground(Type::Any),
                None,
                true,
                None,
                false,
            )
            .is_ok());
        });
    }

    #[test]
    fn call_instantiation_checks_nested_record_row_requirements() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let type_id = super::SchemeTypeParameterId(0);
            let row_id = SchemeRowParameterId(0);
            let field = Name::intern("name");
            let callable = CallableScheme::new(
                vec![type_id],
                Vec::new(),
                vec![SchemeParameter {
                    name: Name::intern("value"),
                    ty: SchemeType::Parameter(type_id),
                    schema_expectation: None,
                    defaulted: false,
                    rest: false,
                }],
                SchemeType::Parameter(type_id),
                None,
                false,
            )
            .unwrap();
            let type_argument = SchemeType::Record {
                fields: BTreeMap::from([(field, SchemeType::Ground(Type::Int))]),
                tail: SchemeRowTail::Parameter(row_id),
            };

            assert_eq!(
                CallInstantiation::new(
                    &callable,
                    BTreeMap::from([(type_id, type_argument)]),
                    BTreeMap::new(),
                    &[],
                    &[SchemeRowParameter {
                        id: row_id,
                        lacks: BTreeSet::new(),
                    }],
                ),
                Err(SchemeBuildError::RowLacksMismatch),
            );
        });
    }

    #[test]
    fn type_scheme_retained_bytes_include_row_and_schema_payloads() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let field = Name::intern("record_field");
            let parameter_name = Name::intern("parameter_name");
            let plain = TypeScheme::new(
                crate::sema::types::CallableKind::Pure,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SchemeType::Ground(Type::Int),
                None,
                false,
                None,
                false,
            )
            .unwrap();
            let expectation = SchemaExpectation {
                instances: vec![SchemaInstance {
                    definition: TypeDefId::from_index(0),
                    arguments: vec![Type::List(Box::new(Type::Str))],
                }],
                children: Arc::new(BTreeMap::from([(
                    SchemaComponent::Field(field),
                    SchemaExpectation::default(),
                )])),
            };
            let scheme = TypeScheme::new(
                crate::sema::types::CallableKind::Pure,
                Vec::new(),
                vec![SchemeRowParameter {
                    id: SchemeRowParameterId(0),
                    lacks: BTreeSet::from([field]),
                }],
                vec![SchemeParameter {
                    name: parameter_name,
                    ty: SchemeType::Record {
                        fields: BTreeMap::new(),
                        tail: SchemeRowTail::Parameter(SchemeRowParameterId(0)),
                    },
                    schema_expectation: Some(expectation.clone()),
                    defaulted: false,
                    rest: false,
                }],
                SchemeType::Ground(Type::Int),
                Some(expectation),
                false,
                None,
                false,
            )
            .unwrap();

            let required_payload_bytes = field.as_str().len()
                + parameter_name.as_str().len()
                + 2 * std::mem::size_of::<Name>()
                + std::mem::size_of::<SchemaInstance>()
                + std::mem::size_of::<Type>()
                + std::mem::size_of::<(SchemaComponent, SchemaExpectation)>();
            assert!(scheme.retained_bytes() >= plain.retained_bytes() + required_payload_bytes);
        });
    }

    #[test]
    fn call_instantiation_retained_bytes_include_keys_and_evidence_payloads() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let type_id = super::SchemeTypeParameterId(0);
            let row_id = SchemeRowParameterId(0);
            let field = Name::intern("payload");
            let extra = Name::intern("extra");
            let callable = CallableScheme::new(
                vec![type_id],
                vec![SchemeRowParameter {
                    id: row_id,
                    lacks: BTreeSet::from([field]),
                }],
                vec![SchemeParameter {
                    name: Name::intern("record"),
                    ty: SchemeType::Record {
                        fields: BTreeMap::from([(
                            field,
                            SchemeType::Parameter(type_id),
                        )]),
                        tail: SchemeRowTail::Parameter(row_id),
                    },
                    schema_expectation: None,
                    defaulted: false,
                    rest: false,
                }],
                SchemeType::Parameter(type_id),
                None,
                false,
            )
            .unwrap();
            let type_argument = Type::List(Box::new(Type::Str));
            let row_evidence = RowEvidence::new(
                BTreeMap::from([(extra, SchemeType::Ground(Type::Bool))]),
                SchemeRowTail::Closed,
            )
            .unwrap();
            let instantiation = CallInstantiation::new(
                &callable,
                BTreeMap::from([(type_id, SchemeType::Ground(type_argument.clone()))]),
                BTreeMap::from([(row_id, row_evidence)]),
                &[],
                &[],
            )
            .unwrap();

            let required_bytes = std::mem::size_of::<CallInstantiation>()
                + std::mem::size_of::<super::SchemeTypeParameterId>()
                + type_argument.retained_bytes()
                + std::mem::size_of::<SchemeRowParameterId>()
                + instantiation.row_arguments().get(&row_id).unwrap().retained_bytes();
            assert!(instantiation.retained_bytes() >= required_bytes);
        });
    }
}
