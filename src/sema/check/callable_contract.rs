use super::{CheckedFunctionDeclaration, Checker, CheckOutput};
use crate::sema::constants::{SchemaComponent, SchemaExpectation};
use crate::sema::types::Type;
use crate::sema::wire_enums::PreparedWireEnums;
use crate::sema::schemes::{CallableMonotype, SchemeRowParameterId,
    SchemeRowTail, SchemeType, SchemeTypeParameterId, SchemeEffectParameterId, TypeScheme};
use crate::symbol::SymbolOwner;
use crate::syntax::arena::{ArenaProgram, ArenaTypeDefBody, TypeDefId};
use std::collections::{BTreeMap, BTreeSet};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

mod render;

/// An immutable view of the checker's selected callable contract. Cloning the
/// view preserves its symbol spellings after its source arena is dropped.
#[derive(Clone, Debug)]
pub struct CheckedCallableContract {
    scheme: Arc<TypeScheme>,
    symbol_owner: SymbolOwner,
    schema_identities: Arc<BTreeMap<TypeDefId, SchemaIdentity>>,
    scheme_callables: Option<Arc<super::super::scheme_callable::SchemeCallableTable>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractComparisonError {
    NonConcreteOriginal,
    UnsupportedContract,
    CannotEstablishConstraints,
    UnresolvedInstance,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SchemaIdentity {
    pub(super) module: Option<Arc<str>>,
    pub(super) name: crate::symbol::Name,
    semantics: SchemaSemantics,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SchemaSemantics {
    Structural(Arc<Type>),
    Retained,
}

pub(crate) struct CallableInstanceEvidence {
    constraints: crate::sema::constraints::TypeConstraints,
    instance: crate::sema::constraints::InstantiatedScheme,
    scheme: Arc<TypeScheme>,
    origin: crate::source::Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SchemeBinderCorrespondence {
    types: BTreeMap<SchemeTypeParameterId, SchemeTypeParameterId>,
    rows: BTreeMap<SchemeRowParameterId, SchemeRowParameterId>,
    effects: BTreeMap<SchemeEffectParameterId, SchemeEffectParameterId>,
}

impl SchemeBinderCorrespondence {
    pub(crate) fn retained_heap_bytes(&self) -> usize {
        self.types.len() * std::mem::size_of::<(SchemeTypeParameterId, SchemeTypeParameterId)>()
            + self.rows.len() * std::mem::size_of::<(SchemeRowParameterId, SchemeRowParameterId)>()
            + self.effects.len() * std::mem::size_of::<(SchemeEffectParameterId, SchemeEffectParameterId)>()
    }
    pub(crate) fn types(&self) -> &BTreeMap<SchemeTypeParameterId, SchemeTypeParameterId> { &self.types }
    pub(crate) fn rows(&self) -> &BTreeMap<SchemeRowParameterId, SchemeRowParameterId> { &self.rows }
    pub(crate) fn effects(&self) -> &BTreeMap<SchemeEffectParameterId, SchemeEffectParameterId> { &self.effects }
}

impl CallableInstanceEvidence {
    pub(crate) fn specialize_effects(&self, effects: &crate::sema::effects::CheckedEffects, owner: &Arc<TypeScheme>) -> Option<crate::sema::effects::CheckedEffects> {
        if !Arc::ptr_eq(owner, &self.scheme) { return None; }
        let instantiated = self.constraints.instantiate_effect_contract(effects, &self.instance).ok()?;
        let resolved = self.constraints.resolve_effect(instantiated.contract()).ok()?;
        Some(crate::sema::effects::CheckedEffects::from_clause(resolved.concrete().ok()?))
    }

    pub(crate) fn specialize(&self, ty: &SchemeType, owner: &Arc<TypeScheme>) -> Option<crate::sema::types::Type> {
        if !Arc::ptr_eq(owner, &self.scheme) { return None; }
        self.constraints.resolve_instance_operand(ty, &self.instance, self.origin).ok()
    }
}

impl Checker {
    pub(super) fn scheme_value_contract(&self, program: &ArenaProgram, signature: &super::FunctionSig) -> CheckedCallableContract {
        CheckedCallableContract {
            scheme: Arc::clone(signature.scheme()),
            symbol_owner: program.symbol_owner().clone(),
            schema_identities: self.callable_schema_identities(program, signature.scheme()),
            scheme_callables: None,
        }
    }

    pub(super) fn callable_schema_identities(&self, program: &ArenaProgram, scheme: &TypeScheme) -> Arc<BTreeMap<TypeDefId, SchemaIdentity>> {
        let mut types = scheme.params().iter().map(|parameter| &parameter.ty)
            .chain(std::iter::once(scheme.return_type())).collect::<Vec<_>>();
        let mut schemas = scheme.params().iter().filter_map(|parameter| parameter.schema_expectation.as_ref())
            .chain(scheme.return_schema()).collect::<Vec<_>>();
        while let Some(ty) = types.pop() {
            match ty {
                SchemeType::Callable { monotype, .. } => {
                    types.extend(monotype.params().iter().map(|parameter| &parameter.ty));
                    types.push(monotype.return_type());
                    schemas.extend(monotype.params().iter().filter_map(|parameter| parameter.schema_expectation.as_ref()));
                    schemas.extend(monotype.return_schema());
                }
                SchemeType::List(inner) | SchemeType::Stream(inner) | SchemeType::Set(inner)
                | SchemeType::Optional(inner) | SchemeType::Validated(_, inner) => types.push(inner),
                SchemeType::Map(key, value) | SchemeType::Result(key, value) => { types.push(key); types.push(value); }
                SchemeType::Record { fields, .. } => types.extend(fields.values()),
                SchemeType::Union(members) => types.extend(members),
                SchemeType::Ground(_) | SchemeType::Parameter(_) | SchemeType::SchemeCallable(_) => {}
            }
        }
        let mut definitions = BTreeSet::new();
        while let Some(schema) = schemas.pop() {
            definitions.extend(schema.instances.iter().map(|instance| instance.definition));
            schemas.extend(schema.children.values());
        }
        schema_identities(program, &self.record_constructors, &self.wire_enums, definitions)
    }
}

pub(super) fn schema_identities(program: &ArenaProgram, constructors: &crate::sema::constants::RecordConstructors,
    wire_enums: &PreparedWireEnums,
    definitions: impl IntoIterator<Item = TypeDefId>) -> Arc<BTreeMap<TypeDefId, SchemaIdentity>>
{
    Arc::new(definitions.into_iter().map(|definition| {
        let name = program.arena.type_def(definition).name;
        let namespace = constructors.namespace(definition);
        assert_eq!(constructors.definition(namespace, name), Some(definition), "checked schema identity matches its declaration");
        let module = namespace.map(|namespace| {
            Arc::from(program.modules.iter().find(|module| module.name == namespace)
                .expect("checked schema belongs to a declared module").key.as_str())
        });
        let declaration = program.arena.type_def(definition);
        let semantics = match &declaration.body {
            ArenaTypeDefBody::RecordSchema(fields)
                if !declaration.nominal && declaration.bounds.is_none() && declaration.type_parameters.is_empty()
                    && program.arena.schema_fields(*fields).iter().all(|field| field.default.is_none())
                    && constructors.defaults(definition).is_some_and(BTreeMap::is_empty) =>
            {
                match constructors.instantiate_checked(&program.arena, definition, &[]) {
                    Ok(ty) if structural_storage_type(&ty, wire_enums) => SchemaSemantics::Structural(Arc::new(ty)),
                    _ => SchemaSemantics::Retained,
                }
            }
            _ => SchemaSemantics::Retained,
        };
        (definition, SchemaIdentity { module, name, semantics })
    }).collect())
}

fn structural_storage_type(ty: &Type, wire_enums: &PreparedWireEnums) -> bool {
    match ty {
        Type::Unit | Type::Null | Type::Bool | Type::Int | Type::Float | Type::Str
        | Type::Bytes | Type::Duration | Type::Path => true,
        Type::List(item) | Type::Optional(item) => structural_storage_type(item, wire_enums),
        Type::Map(key, value) => structural_storage_type(key, wire_enums) && structural_storage_type(value, wire_enums),
        Type::Record(fields) => fields.values().all(|field| structural_storage_type(field, wire_enums)),
        Type::Tag(name) => wire_enums.mappings.get(name).is_some_and(|mapping| mapping.type_name == *name),
        _ => false,
    }
}

fn project_structural_schema<T: Clone>(schema: &SchemaExpectation<T>, ty: &Type,
    identities: &BTreeMap<TypeDefId, SchemaIdentity>) -> SchemaExpectation<T>
{
    let instances = schema.instances.iter().filter(|instance| {
        !instance.arguments.is_empty() || !identities.get(&instance.definition).is_some_and(|identity| {
            matches!(&identity.semantics, SchemaSemantics::Structural(expected) if expected.as_ref() == ty)
        })
    }).cloned().collect();
    let children = schema.children.iter().map(|(component, schema)| {
        let child = match (component, ty) {
            (SchemaComponent::Field(name), Type::Record(fields)) => fields.get(name),
            (SchemaComponent::Item, Type::List(item)) | (SchemaComponent::Optional, Type::Optional(item))
            | (SchemaComponent::Success, Type::Result(item, _)) | (SchemaComponent::Error, Type::Result(_, item))
            | (SchemaComponent::Value, Type::Map(_, item)) | (SchemaComponent::Key, Type::Map(item, _)) => Some(item.as_ref()),
            _ => None,
        };
        (*component, child.map_or_else(|| schema.clone(), |ty| project_structural_schema(schema, ty, identities)))
    }).collect();
    SchemaExpectation { instances, children: Arc::new(children) }
}

pub(super) fn structural_schema_is_redundant(schema: &SchemaExpectation, ty: &Type,
    identities: &BTreeMap<TypeDefId, SchemaIdentity>) -> bool
{
    schema_has_obligations(schema)
        && !schema_has_obligations(&project_structural_schema(schema, ty, identities))
}

pub(super) fn same_schema(left: Option<&SchemaExpectation>, right: Option<&SchemaExpectation>,
    left_schemas: &BTreeMap<TypeDefId, SchemaIdentity>, right_schemas: &BTreeMap<TypeDefId, SchemaIdentity>) -> bool
{
    let left = left.map(|schema| schema.map(&mut |ty| SchemeType::Ground(ty.clone())));
    let right = right.map(|schema| schema.map(&mut |ty| SchemeType::Ground(ty.clone())));
    BoundEquivalence::new(left_schemas, right_schemas, None, None).schema(left.as_ref(), right.as_ref())
}

pub(super) fn same_bound_effects(
    left: &crate::sema::effects::CheckedEffects, left_owner: &TypeScheme,
    right: &crate::sema::effects::CheckedEffects, right_owner: &TypeScheme,
    left_schemas: &BTreeMap<TypeDefId, SchemaIdentity>, right_schemas: &BTreeMap<TypeDefId, SchemaIdentity>,
) -> bool {
    let mut equivalence = BoundEquivalence::new(left_schemas, right_schemas, None, None);
    equivalence.scheme(left_owner, right_owner) && equivalence.effects(left.contract(), right.contract())
}

pub(super) fn same_bound_operand(
    left: &SchemeType, left_owner: &TypeScheme, right: &SchemeType, right_owner: &TypeScheme,
    left_schemas: &BTreeMap<TypeDefId, SchemaIdentity>, right_schemas: &BTreeMap<TypeDefId, SchemaIdentity>,
) -> bool {
    let mut equivalence = BoundEquivalence::new(left_schemas, right_schemas, None, None);
    if !equivalence.scheme(left_owner, right_owner) { return false; }
    let bound_types = equivalence.types.len();
    let bound_rows = equivalence.rows.len();
    equivalence.ty(left, right) && equivalence.types.len() == bound_types && equivalence.rows.len() == bound_rows
}

impl CheckedCallableContract {
    pub(crate) fn with_rebased_scheme(&self, scheme: Arc<TypeScheme>) -> Self {
        Self { scheme, symbol_owner: self.symbol_owner.clone(), schema_identities: Arc::clone(&self.schema_identities), scheme_callables: None }
    }

    pub(crate) fn with_scheme_callables(mut self, scheme_callables: Arc<super::super::scheme_callable::SchemeCallableTable>) -> Self {
        self.scheme_callables = Some(scheme_callables);
        self
    }

    pub(crate) fn retained_heap_bytes(&self, schemes: &mut BTreeSet<usize>, schemas: &mut BTreeSet<usize>) -> usize {
        let scheme = if schemes.insert(Arc::as_ptr(&self.scheme) as usize) { self.scheme.retained_bytes() } else { 0 };
        let identities = if schemas.insert(Arc::as_ptr(&self.schema_identities) as usize) {
            std::mem::size_of::<BTreeMap<TypeDefId, SchemaIdentity>>()
                + self.schema_identities.len() * std::mem::size_of::<(TypeDefId, SchemaIdentity)>()
                + self.schema_identities.values().filter_map(|identity| identity.module.as_ref()).map(|module| module.len()).sum::<usize>()
                + self.schema_identities.values().map(|identity| match &identity.semantics {
                    SchemaSemantics::Structural(ty) => ty.retained_bytes(),
                    SchemaSemantics::Retained => 0,
                }).sum::<usize>()
        } else { 0 };
        scheme.saturating_add(identities)
    }

    pub(crate) fn scheme(&self) -> &Arc<TypeScheme> { &self.scheme }

    pub(crate) fn structural_interface_schema(&self, ty: &SchemeType, schema: Option<&SchemaExpectation<SchemeType>>, checked: &Type) -> bool {
        let Some(schema) = schema else { return false; };
        let Some(ground) = ty.closed_ground_type() else { return false; };
        ground == *checked && structural_schema_is_redundant(schema, &ground, &self.schema_identities)
    }

    pub(crate) fn binder_correspondence(
        &self,
        other: &Self,
        callables: &super::super::scheme_callable::SchemeCallableTable,
    ) -> Option<SchemeBinderCorrespondence> {
        let mut equivalence = BoundEquivalence::new(&self.schema_identities, &other.schema_identities,
            Some(callables), Some(callables));
        if !equivalence.scheme(&self.scheme, &other.scheme)
            || !equivalence.effects(self.scheme.effects().contract(), other.scheme.effects().contract())
            || equivalence.types.len() != self.scheme.type_parameters().len()
            || equivalence.rows.len() != self.scheme.row_parameters().len()
            || equivalence.effects.len() != self.scheme.callable().requirements().effect_parameters.len()
        { return None; }
        Some(SchemeBinderCorrespondence { types: equivalence.types, rows: equivalence.rows, effects: equivalence.effects })
    }

    pub fn kind(&self) -> crate::sema::types::CallableKind {
        self.scheme.kind()
    }

    pub fn is_generic(&self) -> bool {
        self.scheme.callable().is_quantified()
    }

    /// Compares checked structure modulo bound type and row parameter names.
    /// Free and nominal identities, labels, schemas, layout, and effects remain
    /// part of the contract; annotation and inference provenance do not.
    pub fn equivalent_to(&self, other: &Self) -> bool {
        self.equivalent_to_with(other, self.scheme_callables.as_deref(), other.scheme_callables.as_deref())
    }

    pub(crate) fn equivalent_to_in_table(
        &self,
        other: &Self,
        callables: &super::super::scheme_callable::SchemeCallableTable,
    ) -> bool {
        self.equivalent_to_with(other, Some(callables), Some(callables))
    }

    fn equivalent_to_with(
        &self,
        other: &Self,
        left_callables: Option<&super::super::scheme_callable::SchemeCallableTable>,
        right_callables: Option<&super::super::scheme_callable::SchemeCallableTable>,
    ) -> bool {
        let mut equivalence = BoundEquivalence::new(&self.schema_identities, &other.schema_identities,
            left_callables, right_callables);
        equivalence.scheme(&self.scheme, &other.scheme)
            && equivalence.effects(self.scheme.effects().contract(), other.scheme.effects().contract())
    }

    /// Tests whether this concrete contract is an instance of the given scheme,
    /// preserving labels, layout, schemas, parameter and success types, and
    /// effects. A resolved Result error may narrow under callable return rules.
    /// Returns an error when the original is generic or the comparison cannot
    /// establish a resolved instance. An error does not establish a mismatch.
    pub fn is_instance_of(&self, generalized: &Self) -> Result<bool, ContractComparisonError> {
        if !self.is_generic() && !generalized.is_generic()
            && (scheme_has_callable_relationship(&self.scheme) || scheme_has_callable_relationship(&generalized.scheme))
        {
            return Ok(self.equivalent_to(generalized));
        }
        self.instance_evidence(generalized).map(|evidence| evidence.is_some())
    }

    pub(crate) fn instance_evidence(&self, generalized: &Self)
        -> Result<Option<CallableInstanceEvidence>, ContractComparisonError>
    {
        use crate::sema::constraints::TypeConstraints;
        use crate::source::{SourceId, Span};

        if self.is_generic() { return Err(ContractComparisonError::NonConcreteOriginal); }
        if self.kind() != generalized.kind()
            || self.scheme.params().len() != generalized.scheme.params().len()
        {
            return Ok(None);
        }
        let mut schemas = BoundEquivalence::new(&self.schema_identities, &generalized.schema_identities,
            self.scheme_callables.as_deref(), generalized.scheme_callables.as_deref());
        if !schemas.interface_schema(self.scheme.return_schema(), generalized.scheme.return_schema(), self.scheme.return_type(), generalized.scheme.return_type())
            || !self.scheme.params().iter().zip(generalized.scheme.params()).all(|(old, new)| {
                old.name == new.name && old.defaulted == new.defaulted && old.rest == new.rest
                    && schemas.interface_schema(old.schema_expectation.as_ref(), new.schema_expectation.as_ref(), &old.ty, &new.ty)
            })
        { return Ok(None); }
        let ground = self.scheme.params().iter().map(|parameter| &parameter.ty)
            .chain(std::iter::once(self.scheme.return_type()))
            .map(SchemeType::closed_ground_type).collect::<Option<Vec<_>>>()
            .ok_or(ContractComparisonError::UnsupportedContract)?;
        let mut constraints = TypeConstraints::default();
        let origin = Span::at(SourceId::new(0), 0);
        let instance = constraints.instantiate_callable(generalized.scheme.callable(), 1, origin, crate::sema::constraints::CallableApplicationScope::Invocation)
            .map_err(|_| ContractComparisonError::UnsupportedContract)?;
        for (expected, actual) in instance.params.iter().zip(&ground) {
            constraints.constrain_annotation(expected, actual, origin)
                .map_err(|_| ContractComparisonError::CannotEstablishConstraints)?;
        }
        let original_return = ground.last().ok_or(ContractComparisonError::UnsupportedContract)?;
        let constrained_return = constraints.resolve(&instance.return_type).map_err(|_| ContractComparisonError::UnresolvedInstance)?;
        let (expected_return, actual_return) = match (&constrained_return, original_return) {
            (Type::Result(expected_success, _), Type::Result(actual_success, _)) =>
                (expected_success.as_ref(), actual_success.as_ref()),
            (expected, actual) => (expected, actual),
        };
        constraints.constrain_annotation(expected_return, actual_return, origin)
            .map_err(|_| ContractComparisonError::CannotEstablishConstraints)?;
        constraints.solve_callable_applications().map_err(|_| ContractComparisonError::CannotEstablishConstraints)?;
        let expected_effects = constraints.instantiate_effect_contract(generalized.scheme.effects(), &instance)
            .map_err(|_| ContractComparisonError::UnresolvedInstance)?;
        let actual_effects = self.scheme.effects().contract().substitute(|_| Err(ContractComparisonError::NonConcreteOriginal))?;
        match constraints.constrain_effect(crate::sema::effects::EffectPredicate::Equal(actual_effects.clone(), expected_effects.contract().clone())) {
            Ok(()) => {}
            Err(crate::sema::constraints::EffectConstraintError::Mismatch) => return Ok(None),
            Err(_) => return Err(ContractComparisonError::CannotEstablishConstraints),
        }
        constraints.finish_effect_constraints().map_err(|_| ContractComparisonError::UnresolvedInstance)?;
        if constraints.resolve_effect(expected_effects.contract()).map_err(|_| ContractComparisonError::UnresolvedInstance)? != actual_effects { return Ok(None); }
        for (expected, actual) in instance.params.iter().zip(&ground) {
            let resolved = constraints.resolve(expected).map_err(|_| ContractComparisonError::UnresolvedInstance)?;
            if &resolved != actual { return Ok(None); }
        }
        let resolved_return = constraints.resolve(&instance.return_type).map_err(|_| ContractComparisonError::UnresolvedInstance)?;
        if resolved_return.contains_recovery() || resolved_return.contains_inference() {
            return Err(ContractComparisonError::UnresolvedInstance);
        }
        if resolved_return != *original_return && !matches!((&resolved_return, original_return),
            (Type::Result(actual_success, _), Type::Result(expected_success, _))
                if actual_success == expected_success
                    && crate::sema::types::callable_return_fits(&resolved_return, original_return))
        { return Ok(None); }
        Ok(Some(CallableInstanceEvidence { constraints, instance, scheme: Arc::clone(&generalized.scheme), origin }))
    }

    /// Joins effect requirements under the same checked callable binder scope.
    /// An unknown contract remains unknown in the result.
    pub fn join_effects(&self, other: &Self) -> Option<Self> {
        if !self.same_shape(other) { return None; }
        let mut equivalence = BoundEquivalence::new(&self.schema_identities, &other.schema_identities,
            self.scheme_callables.as_deref(), other.scheme_callables.as_deref());
        if !equivalence.scheme(&self.scheme, &other.scheme) { return None; }
        let right = other.scheme.effects().contract().substitute(|id| {
            equivalence.effects.iter().find_map(|(left, right)| (right == id).then_some(*left))
                .map(crate::sema::effects::EffectContract::variable).ok_or(())
        }).ok()?;
        let effects = crate::sema::effects::CheckedEffects::from_contract(self.scheme.effects().contract().clone().union(right));
        Some(Self {
            scheme: Arc::new(TypeScheme::with_callable(self.scheme.kind(), Arc::clone(self.scheme.callable()), effects,
                self.scheme.inferred_effects() || other.scheme.inferred_effects()).ok()?),
            symbol_owner: self.symbol_owner.clone(),
            schema_identities: Arc::clone(&self.schema_identities),
            scheme_callables: self.scheme_callables.as_ref().map(Arc::clone),
        })
    }

    /// Returns an annotation only when the source callable-type grammar can
    /// express the checked contract. Callers still recheck it in its scope.
    pub fn source_annotation(&self) -> Option<String> {
        use crate::sema::types::{CallableParamType, CallableType, Type, TypedCallable};
        if self.is_generic() || self.scheme.return_schema().is_some_and(schema_has_obligations) { return None; }
        let pure = match self.scheme.kind() {
            crate::sema::types::CallableKind::Pure => true,
            crate::sema::types::CallableKind::Proc => false,
            crate::sema::types::CallableKind::Stream => return None,
        };
        let params = self.scheme.params().iter().map(|parameter| {
            if parameter.defaulted || parameter.rest || parameter.schema_expectation.as_ref().is_some_and(schema_has_obligations) { return None; }
            Some(CallableParamType { name: parameter.name, ty: parameter.ty.closed_ground_type()?, defaulted: false, rest: false })
        }).collect::<Option<Vec<_>>>()?;
        let ty = Type::Callable(Arc::new(TypedCallable { kind: if pure { crate::sema::types::CallableKind::Pure } else { crate::sema::types::CallableKind::Proc }, sig: CallableType {
            params, return_ty: Box::new(self.scheme.return_type().closed_ground_type()?),
            effects: crate::sema::types::CallableEffects::from_contract(if pure { crate::sema::effects::EffectContract::Unknown }
                else { self.scheme.effects().contract().substitute(|_| Err(())).ok()? }),
        } }));
        let mut pending = vec![&ty];
        while let Some(ty) = pending.pop() {
            if matches!(ty, Type::Callable(callable) if callable.sig.params.iter().any(|parameter| parameter.defaulted || parameter.rest)) {
                return None;
            }
            ty.for_each_child(|child| pending.push(child));
        }
        self.symbol_owner.with_current(|| ty.annotation_source())
    }

    fn same_shape(&self, other: &Self) -> bool {
        BoundEquivalence::new(&self.schema_identities, &other.schema_identities,
            self.scheme_callables.as_deref(), other.scheme_callables.as_deref()).scheme(&self.scheme, &other.scheme)
    }
}

impl CheckedFunctionDeclaration {
    pub fn callable_contract(&self) -> Option<CheckedCallableContract> {
        Some(CheckedCallableContract {
            scheme: Arc::clone(self.scheme.as_ref()?),
            symbol_owner: self.symbol_owner.clone(),
            schema_identities: Arc::clone(&self.schema_identities),
            scheme_callables: None,
        })
    }
}

impl CheckOutput {
    /// Returns the selected declaration contract before per-call generic
    /// instantiation. An invocation span selects its callee's contract.
    pub fn callable_contract_at(&self, span: crate::source::Span) -> Option<CheckedCallableContract> {
        let target = self.selected_function_calls.get(&span)?;
        self.checked_declarations.functions.get(&target.definition)
            .filter(|declaration| declaration.name == target.name && declaration.namespace == target.namespace)?
            .callable_contract()
            .map(|contract| contract.with_scheme_callables(Arc::clone(&self.scheme_callables)))
    }

    /// Returns a checked declaration contract with the relationship context
    /// published by the same check.
    pub fn callable_declaration_contract(&self, definition: crate::syntax::arena::FunctionDefId) -> Option<CheckedCallableContract> {
        self.checked_declarations.functions.get(&definition)?
            .callable_contract()
            .map(|contract| contract.with_scheme_callables(Arc::clone(&self.scheme_callables)))
    }

    /// Returns the declaration contract of a checked function value reference.
    /// Invocation spans have no function value reference contract.
    pub fn callable_value_contract_at(&self, span: crate::source::Span) -> Option<CheckedCallableContract> {
        self.selected_function_value_references.contains(&span)
            .then(|| self.callable_contract_at(span)).flatten()
    }
}

fn schema_has_obligations<T>(schema: &SchemaExpectation<T>) -> bool {
    let mut pending = vec![schema];
    while let Some(schema) = pending.pop() {
        if !schema.instances.is_empty() { return true; }
        pending.extend(schema.children.values());
    }
    false
}

fn scheme_has_callable_relationship(scheme: &TypeScheme) -> bool {
    scheme.params().iter().map(|parameter| &parameter.ty).chain(std::iter::once(scheme.return_type()))
        .filter_map(SchemeType::closed_ground_type)
        .any(|ty| ty.contains_scheme_callable())
}

struct BoundEquivalence<'a> {
    types: BTreeMap<SchemeTypeParameterId, SchemeTypeParameterId>,
    rows: BTreeMap<SchemeRowParameterId, SchemeRowParameterId>,
    effects: BTreeMap<SchemeEffectParameterId, SchemeEffectParameterId>,
    left_schemas: &'a BTreeMap<TypeDefId, SchemaIdentity>,
    right_schemas: &'a BTreeMap<TypeDefId, SchemaIdentity>,
    left_callables: Option<&'a super::super::scheme_callable::SchemeCallableTable>,
    right_callables: Option<&'a super::super::scheme_callable::SchemeCallableTable>,
    active_callables: BTreeSet<(crate::sema::scheme_callable::SchemeCallableId, crate::sema::scheme_callable::SchemeCallableId)>,
    completed_callables: Option<Rc<RefCell<SchemeCallableComparisonMemo>>>,
}

#[derive(Default)]
struct SchemeCallableComparisonMemo {
    completed: BTreeMap<(
        crate::sema::scheme_callable::SchemeCallableId,
        crate::sema::scheme_callable::SchemeCallableId,
        usize,
    ), bool>,
    #[cfg(test)]
    expansions: usize,
}

impl<'a> BoundEquivalence<'a> {
    fn new(left_schemas: &'a BTreeMap<TypeDefId, SchemaIdentity>, right_schemas: &'a BTreeMap<TypeDefId, SchemaIdentity>,
        left_callables: Option<&'a super::super::scheme_callable::SchemeCallableTable>,
        right_callables: Option<&'a super::super::scheme_callable::SchemeCallableTable>) -> Self
    {
        Self::with_memo(left_schemas, right_schemas, left_callables, right_callables, None)
    }

    fn with_memo(left_schemas: &'a BTreeMap<TypeDefId, SchemaIdentity>, right_schemas: &'a BTreeMap<TypeDefId, SchemaIdentity>,
        left_callables: Option<&'a super::super::scheme_callable::SchemeCallableTable>,
        right_callables: Option<&'a super::super::scheme_callable::SchemeCallableTable>,
        completed_callables: Option<Rc<RefCell<SchemeCallableComparisonMemo>>>) -> Self
    {
        Self { types: BTreeMap::new(), rows: BTreeMap::new(), effects: BTreeMap::new(), left_schemas, right_schemas,
            left_callables, right_callables, active_callables: BTreeSet::new(), completed_callables }
    }
}

fn bind<Key: Copy + Ord>(mapping: &mut BTreeMap<Key, Key>, left: Key, right: Key) -> bool {
    if let Some(mapped) = mapping.get(&left) { return *mapped == right; }
    if mapping.values().any(|mapped| *mapped == right) { return false; }
    mapping.insert(left, right);
    true
}

impl BoundEquivalence<'_> {
    fn scheme(&mut self, left: &TypeScheme, right: &TypeScheme) -> bool {
        let left_effects = &left.callable().requirements().effect_parameters;
        let right_effects = &right.callable().requirements().effect_parameters;
        if left_effects.len() != right_effects.len()
            || !left_effects.iter().zip(right_effects).all(|(left, right)| bind(&mut self.effects, *left, *right))
        { return false; }
        if left.kind() != right.kind()
            || left.type_parameters().len() != right.type_parameters().len()
            || left.row_parameters().len() != right.row_parameters().len()
            || !self.parameters(left.params(), right.params())
            || !self.ty(left.return_type(), right.return_type())
            || !self.interface_schema(left.return_schema(), right.return_schema(), left.return_type(), right.return_type())
            || !self.requirements(left.callable().requirements(), right.callable().requirements())
        { return false; }
        left.row_parameters().iter().all(|left| {
            self.rows.get(&left.id).and_then(|id| right.row_parameters().iter().find(|parameter| parameter.id == *id))
                .is_some_and(|right| left.lacks == right.lacks)
        })
    }

    fn requirements(&mut self, left: &crate::sema::schemes::CallableRequirements, right: &crate::sema::schemes::CallableRequirements) -> bool {
        left.map_key_parameters.len() == right.map_key_parameters.len()
            && left.map_key_parameters.iter().all(|parameter| self.types.get(parameter).is_some_and(|mapped| right.map_key_parameters.contains(mapped)))
            && left.conversion_parameters.len() == right.conversion_parameters.len()
            && left.conversion_parameters.iter().all(|(parameter, conversions)| self.types.get(parameter).is_some_and(|mapped| right.conversion_parameters.get(mapped) == Some(conversions)))
            && left.applications.len() == right.applications.len()
            && left.effect_predicates.len() == right.effect_predicates.len()
            && left.wrappers.len() == right.wrappers.len()
            && left.wrappers.iter().zip(&right.wrappers).all(|(left, right)| {
                left.shape() == right.shape() && left.types().into_iter().zip(right.types()).all(|(left, right)| self.ty(left, right))
                    && match (&left.kind, &right.kind) {
                        (crate::sema::wrapper_operation::WrapperRequirementKind::Field { effects: left, .. }, crate::sema::wrapper_operation::WrapperRequirementKind::Field { effects: right, .. }) => self.effects(left, right),
                        _ => true,
                    }
            })
            && left.operations.len() == right.operations.len()
            && left.operations.iter().zip(&right.operations).all(|(left, right)| left.same_kind(right) && left.types().zip(right.types()).all(|(left, right)| self.ty(left, right)))
            && left.methods.len() == right.methods.len()
            && left.methods.iter().zip(&right.methods).all(|(left, right)| {
                left.family == right.family
                    && left.argument_slots == right.argument_slots
                    && left.arguments.len() == right.arguments.len()
                    && self.ty(&left.receiver, &right.receiver)
                    && left.arguments.iter().zip(&right.arguments).all(|(left, right)| self.ty(left, right))
                    && self.ty(&left.result, &right.result)
            })
            && left.applications.iter().zip(&right.applications).all(|(left, right)| {
                use crate::sema::callable_application::CallableApplication;
                if let (CallableApplication::KnownArguments(left), CallableApplication::KnownArguments(right)) = (left, right) {
                    return left.parameters.len() == right.parameters.len()
                        && left.arguments.len() == right.arguments.len()
                        && left.parameters.iter().zip(right.parameters.iter()).all(|(left, right)| left.layout == right.layout && self.ty(&left.ty, &right.ty))
                        && left.arguments.iter().zip(right.arguments.iter()).all(|(left, right)| left.kind == right.kind && self.ty(&left.ty, &right.ty));
                }
                let (CallableApplication::Invocation(left), CallableApplication::Invocation(right)) = (left, right) else { return false; };
                left.kind == right.kind && left.arguments.len() == right.arguments.len()
                    && left.proc_returns.len() == right.proc_returns.len()
                    && self.ty(&left.callee, &right.callee) && self.ty(&left.result, &right.result)
                    && self.effects(&left.effects, &right.effects)
                    && left.arguments.iter().zip(right.arguments.iter()).all(|(left, right)| left.kind == right.kind && self.ty(&left.ty, &right.ty))
                    && left.proc_returns.iter().zip(right.proc_returns.iter()).all(|(left, right)| {
                        left.propagated_errors.len() == right.propagated_errors.len() && self.ty(&left.result, &right.result)
                            && left.propagated_errors.iter().zip(right.propagated_errors.iter()).all(|(left, right)| self.ty(left, right))
                    })
            })
            && left.iteration_parameters.len() == right.iteration_parameters.len()
            && left.iteration_parameters.iter().all(|(source, iteration)| {
                let Some(mapped) = self.types.get(source) else { return false };
                let Some(other) = right.iteration_parameters.get(mapped) else { return false };
                iteration.source_kind == other.source_kind && self.ty(&iteration.item, &other.item) && match (&iteration.policy, &other.policy) {
                    (crate::sema::schemes::IterationPolicy::IterableInfallible, crate::sema::schemes::IterationPolicy::IterableInfallible)
                    | (crate::sema::schemes::IterationPolicy::PipelineInfallible, crate::sema::schemes::IterationPolicy::PipelineInfallible)
                    | (crate::sema::schemes::IterationPolicy::PipelineTransport, crate::sema::schemes::IterationPolicy::PipelineTransport) => true,
                    (crate::sema::schemes::IterationPolicy::IterablePropagates(left), crate::sema::schemes::IterationPolicy::IterablePropagates(right))
                    | (crate::sema::schemes::IterationPolicy::PipelinePropagates(left), crate::sema::schemes::IterationPolicy::PipelinePropagates(right)) => self.ty(left, right),
                    _ => false,
                }
            })
            && left.effect_predicates.iter().zip(&right.effect_predicates).all(|(left, right)| {
                left.try_map_contracts(|contract| contract.substitute(|id| self.effects.get(id).copied()
                    .map(crate::sema::effects::EffectContract::variable).ok_or(()))).is_ok_and(|left| &left == right)
            })
    }

    fn schema(&mut self, left: Option<&SchemaExpectation<SchemeType>>, right: Option<&SchemaExpectation<SchemeType>>) -> bool {
        let left = left.filter(|schema| schema_has_obligations(schema));
        let right = right.filter(|schema| schema_has_obligations(schema));
        match (left, right) {
            (None, None) => true,
            (Some(left), Some(right)) => left.instances.len() == right.instances.len()
                && left.instances.iter().zip(&right.instances).all(|(left, right)| {
                    self.left_schemas.get(&left.definition).is_some_and(|identity| Some(identity) == self.right_schemas.get(&right.definition))
                        && left.arguments.len() == right.arguments.len()
                        && left.arguments.iter().zip(&right.arguments).all(|(left, right)| self.ty(left, right))
                })
                && left.children.values().filter(|child| schema_has_obligations(child)).count()
                    == right.children.values().filter(|child| schema_has_obligations(child)).count()
                && left.children.iter().filter(|(_, child)| schema_has_obligations(child)).all(|(component, left)| right.children.get(component)
                    .is_some_and(|right| self.schema(Some(left), Some(right)))),
            _ => false,
        }
    }

    fn interface_schema(&mut self, left: Option<&SchemaExpectation<SchemeType>>, right: Option<&SchemaExpectation<SchemeType>>, left_type: &SchemeType, right_type: &SchemeType) -> bool {
        match (left_type.closed_ground_type(), right_type.closed_ground_type()) {
            (Some(left_type), Some(right_type)) if left_type == right_type => {
                let left = left.map(|schema| project_structural_schema(schema, &left_type, self.left_schemas));
                let right = right.map(|schema| project_structural_schema(schema, &right_type, self.right_schemas));
                self.schema(left.as_ref(), right.as_ref())
            }
            _ => self.schema(left, right),
        }
    }

    fn parameters(&mut self, left: &[crate::sema::schemes::SchemeParameter], right: &[crate::sema::schemes::SchemeParameter]) -> bool {
        left.len() == right.len() && left.iter().zip(right).all(|(left, right)| {
            left.name == right.name && left.defaulted == right.defaulted && left.rest == right.rest
                && self.interface_schema(left.schema_expectation.as_ref(), right.schema_expectation.as_ref(), &left.ty, &right.ty) && self.ty(&left.ty, &right.ty)
        })
    }

    fn monotype(&mut self, left: &CallableMonotype, right: &CallableMonotype) -> bool {
        self.parameters(left.params(), right.params()) && self.ty(left.return_type(), right.return_type())
            && self.interface_schema(left.return_schema(), right.return_schema(), left.return_type(), right.return_type())
    }

    fn effects(&self, left: &crate::sema::effects::EffectContract<SchemeEffectParameterId>, right: &crate::sema::effects::EffectContract<SchemeEffectParameterId>) -> bool {
        left.substitute(|id| self.effects.get(id).copied()
            .map(crate::sema::effects::EffectContract::variable).ok_or(())).is_ok_and(|left| &left == right)
    }

    fn contract(&mut self, left: &CheckedCallableContract, right: &CheckedCallableContract) -> bool {
        let mut equivalence = BoundEquivalence::with_memo(&left.schema_identities, &right.schema_identities,
            self.left_callables, self.right_callables, self.completed_callables.as_ref().map(Rc::clone));
        equivalence.active_callables = self.active_callables.clone();
        equivalence.scheme(&left.scheme, &right.scheme)
            && equivalence.effects(left.scheme.effects().contract(), right.scheme.effects().contract())
    }

    fn scheme_callable(&mut self, left: crate::sema::scheme_callable::SchemeCallableId,
        right: crate::sema::scheme_callable::SchemeCallableId) -> bool
    {
        use crate::sema::constraints::MAX_TYPE_DEPTH;
        let pair = (left, right);
        if self.active_callables.contains(&pair) || self.active_callables.len() >= MAX_TYPE_DEPTH { return false; }
        let key = (left, right, self.active_callables.len());
        if let Some(memo) = &self.completed_callables {
            if let Some(&completed) = memo.borrow().completed.get(&key) { return completed; }
        }
        let (Some(left_table), Some(right_table)) = (self.left_callables, self.right_callables) else { return false; };
        let (Ok(left), Ok(right)) = (left_table.relationship(left), right_table.relationship(right)) else { return false; };
        if left.targets().len() != right.targets().len() { return false; }
        let mut left_targets = left.targets().iter().collect::<Vec<_>>();
        let mut right_targets = right.targets().iter().collect::<Vec<_>>();
        left_targets.sort_by_key(|target| { let identity = target.target(); (identity.namespace, identity.name) });
        right_targets.sort_by_key(|target| { let identity = target.target(); (identity.namespace, identity.name) });
        if left_targets.windows(2).any(|pair| {
            let first = pair[0].target(); let second = pair[1].target();
            (first.namespace, first.name) == (second.namespace, second.name)
        }) || right_targets.windows(2).any(|pair| {
            let first = pair[0].target(); let second = pair[1].target();
            (first.namespace, first.name) == (second.namespace, second.name)
        }) {
            return false;
        }
        let memo = Rc::clone(self.completed_callables.get_or_insert_with(|| {
            Rc::new(RefCell::new(SchemeCallableComparisonMemo::default()))
        }));
        let mut nested = BoundEquivalence::with_memo(&left.canonical().schema_identities, &right.canonical().schema_identities,
            self.left_callables, self.right_callables, Some(Rc::clone(&memo)));
        nested.active_callables = self.active_callables.clone();
        nested.active_callables.insert(pair);
        #[cfg(test)]
        { memo.borrow_mut().expansions += 1; }
        let equivalent = nested.contract(left.canonical(), right.canonical())
            && left_targets.iter().zip(&right_targets).all(|(left, right)| {
                let left_target = left.target(); let right_target = right.target();
                left_target.namespace == right_target.namespace && left_target.name == right_target.name
                    && nested.contract(left.contract(), right.contract())
            });
        memo.borrow_mut().completed.insert(key, equivalent);
        equivalent
    }

    fn ground_type(&mut self, left: &crate::sema::types::Type, right: &crate::sema::types::Type) -> bool {
        use crate::sema::types::{ModuleExportType, Type};
        if !left.contains_scheme_callable() && !right.contains_scheme_callable() { return left == right; }
        match (left, right) {
            (Type::SchemeCallable(left), Type::SchemeCallable(right)) => self.scheme_callable(*left, *right),
            (Type::List(left), Type::List(right)) | (Type::Stream(left), Type::Stream(right))
            | (Type::Set(left), Type::Set(right)) | (Type::Optional(left), Type::Optional(right)) => self.ground_type(left, right),
            (Type::Map(left_key, left_value), Type::Map(right_key, right_value))
            | (Type::Result(left_key, left_value), Type::Result(right_key, right_value)) => {
                self.ground_type(left_key, right_key) && self.ground_type(left_value, right_value)
            }
            (Type::Record(left), Type::Record(right)) => left.len() == right.len() && left.iter()
                .all(|(name, left)| right.get(name).is_some_and(|right| self.ground_type(left, right))),
            (Type::Union(left), Type::Union(right)) => left.len() == right.len()
                && left.iter().zip(right).all(|(left, right)| self.ground_type(left, right)),
            (Type::Callable(left), Type::Callable(right)) => left.kind == right.kind
                && left.sig.effects == right.sig.effects
                && left.sig.params.len() == right.sig.params.len()
                && left.sig.params.iter().zip(&right.sig.params).all(|(left, right)| {
                    left.name == right.name && left.defaulted == right.defaulted && left.rest == right.rest
                        && self.ground_type(&left.ty, &right.ty)
                }) && self.ground_type(&left.sig.return_ty, &right.sig.return_ty),
            (Type::Module(left), Type::Module(right)) => left.exact == right.exact && left.exports.len() == right.exports.len()
                && left.exports.iter().all(|(name, left)| right.exports.get(name).is_some_and(|right| match (left, right) {
                    (ModuleExportType::Value { ty: left, optional: left_optional }, ModuleExportType::Value { ty: right, optional: right_optional }) => left_optional == right_optional && self.ground_type(left, right),
                    (ModuleExportType::Callable { kind: left_kind, sig: left, optional: left_optional }, ModuleExportType::Callable { kind: right_kind, sig: right, optional: right_optional }) => {
                        left_kind == right_kind && left_optional == right_optional && left.effects == right.effects
                            && left.params.len() == right.params.len() && left.params.iter().zip(&right.params).all(|(left, right)| {
                                left.name == right.name && left.defaulted == right.defaulted && left.rest == right.rest
                                    && self.ground_type(&left.ty, &right.ty)
                            }) && self.ground_type(&left.return_ty, &right.return_ty)
                    }
                    (ModuleExportType::GenericCallable { kind: left, optional: left_optional }, ModuleExportType::GenericCallable { kind: right, optional: right_optional }) => left == right && left_optional == right_optional,
                    _ => false,
                })),
            (Type::Validated(left), Type::Validated(right)) => left.validation() == right.validation() && self.ground_type(left.base(), right.base()),
            _ => false,
        }
    }

    fn ty(&mut self, left: &SchemeType, right: &SchemeType) -> bool {
        if let (Some(left), Some(right)) = (left.closed_ground_type(), right.closed_ground_type()) {
            return self.ground_type(&left, &right);
        }
        match (left, right) {
            (SchemeType::Ground(left), SchemeType::Ground(right)) => self.ground_type(left, right),
            (SchemeType::SchemeCallable(left), SchemeType::SchemeCallable(right)) => self.scheme_callable(*left, *right),
            (SchemeType::Parameter(left), SchemeType::Parameter(right)) => bind(&mut self.types, *left, *right),
            (SchemeType::Callable { kind: left_kind, monotype: left, effects: left_effects },
             SchemeType::Callable { kind: right_kind, monotype: right, effects: right_effects }) => {
                left_kind == right_kind && self.effects(left_effects, right_effects) && self.monotype(left, right)
            }
            (SchemeType::List(left), SchemeType::List(right))
            | (SchemeType::Stream(left), SchemeType::Stream(right))
            | (SchemeType::Set(left), SchemeType::Set(right))
            | (SchemeType::Optional(left), SchemeType::Optional(right)) => self.ty(left, right),
            (SchemeType::Map(left_key, left_value), SchemeType::Map(right_key, right_value))
            | (SchemeType::Result(left_key, left_value), SchemeType::Result(right_key, right_value)) => {
                self.ty(left_key, right_key) && self.ty(left_value, right_value)
            }
            (SchemeType::Record { fields: left, tail: left_tail }, SchemeType::Record { fields: right, tail: right_tail }) => {
                left.len() == right.len() && left.iter().all(|(name, left)| right.get(name).is_some_and(|right| self.ty(left, right)))
                    && match (left_tail, right_tail) {
                        (SchemeRowTail::Closed, SchemeRowTail::Closed) => true,
                        (SchemeRowTail::Parameter(left), SchemeRowTail::Parameter(right)) => bind(&mut self.rows, *left, *right),
                        _ => false,
                    }
            }
            (SchemeType::Union(left), SchemeType::Union(right)) => left.len() == right.len()
                && left.iter().zip(right).all(|(left, right)| self.ty(left, right)),
            (SchemeType::Validated(left_validation, left), SchemeType::Validated(right_validation, right)) => {
                left_validation == right_validation && self.ty(left, right)
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::check::{CheckOptions, Checker};
    use crate::sema::schemes::{SchemeParameter, SchemeRowParameter};
    use crate::sema::types::Type;
    use crate::source::SourceId;
    use crate::symbol::Name;
    use crate::syntax::parser::Parser;
    use std::collections::BTreeSet;

    fn identity(owner: &SymbolOwner, binder: u32, result: Option<Type>) -> CheckedCallableContract {
        owner.with_current(|| {
            let binder = SchemeTypeParameterId(binder);
            CheckedCallableContract {
                scheme: Arc::new(TypeScheme::new(crate::sema::types::CallableKind::Pure, vec![binder], Vec::new(),
                    vec![SchemeParameter { name: Name::intern("value"), ty: SchemeType::Parameter(binder),
                        schema_expectation: None, defaulted: false, rest: false }],
                    result.map_or(SchemeType::Parameter(binder), SchemeType::Ground), None, false, None, false).unwrap()),
                symbol_owner: owner.clone(),
                schema_identities: Default::default(),
                scheme_callables: None,
            }
        })
    }

    fn root_schema_contract(source: &str) -> CheckedCallableContract {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        parsed.arena.symbol_owner().with_current(|| checked.checked_declarations.functions.values()
            .find(|declaration| declaration.name.as_str() == "describe").unwrap().callable_contract().unwrap())
    }

    #[test]
    fn structural_interface_aliases_preserve_known_ground_contracts() {
        let declared = root_schema_contract("type Pieces = {texts: List[Str], kinds: List[Int]}\npure describe() -> Pieces { {texts: [\"a\"], kinds: [0]} }\n");
        let inferred = root_schema_contract("type Pieces = {texts: List[Str], kinds: List[Int]}\npure describe() { {texts: [\"a\"], kinds: [0]} }\n");
        let before = declared.to_string();
        assert!(declared.equivalent_to(&inferred));
        assert!(declared.is_instance_of(&inferred).unwrap());
        assert!(inferred.equivalent_to(&declared));
        assert_eq!(declared.to_string(), before);
        assert!(before.contains("schema Pieces[]"));
        let named = root_schema_contract("type Entry = {value: Int}\npure describe(value: Entry) -> Int { value.value }\n");
        let structural = root_schema_contract("type Other = {value: Int}\npure describe(value: Other) -> Int { value.value }\n");
        assert!(named.equivalent_to(&structural));
    }

    #[test]
    fn structural_interface_projection_retains_semantic_obligations() {
        for (declarations, annotated, inferred) in [
            ("type Count = {value: UInt}\n", "pure describe(value: Int) -> Count { {value} }\n", "pure describe(value: Int) { {value} }\n"),
            ("", "pure describe() -> Result[Int] { 1 }\n", "pure describe() { 1 }\n"),
            ("nominal type Entry = {value: Int}\n", "pure describe() -> Entry { Entry(value: 1) }\n", "pure describe() { {value: 1} }\n"),
            ("type Entry = {value: Int = 1}\n", "pure describe() -> Entry { {value: 1} }\n", "pure describe() { {value: 1} }\n"),
            ("type Entry[T] = {value: Int}\n", "pure describe() -> Entry[Int] { {value: 1} }\n", "pure describe() { {value: 1} }\n"),
            ("type Inner = {value: Int = 1}\ntype Entry = {inner: Inner}\n", "pure describe() -> Entry { {inner: {value: 1}} }\n", "pure describe() { {inner: {value: 1}} }\n"),
        ] {
            let annotated = root_schema_contract(&format!("{declarations}{annotated}"));
            let inferred = root_schema_contract(&format!("{declarations}{inferred}"));
            assert!(!annotated.equivalent_to(&inferred));
            assert!(!annotated.is_instance_of(&inferred).unwrap_or(false));
        }
        let annotated = root_schema_contract("type Entry = {value: Int}\npure describe(value: Entry) -> Int { 1 }\n");
        let erased = root_schema_contract("pure describe(value) -> Int { 1 }\n");
        assert!(!annotated.is_instance_of(&erased).unwrap_or(false));
    }

    #[test]
    fn wire_enum_interfaces_project_only_exact_ground_families() {
        let declarations = "enum State: Str { Ready = \"ready\" }\nenum Other: Str { OtherReady = \"ready\" }\ntype Info = {state: State, count: Int}\n";
        let original = root_schema_contract(&format!("{declarations}pure describe() -> Info {{ {{state: Ready, count: 1}} }}\n"));
        let inferred = root_schema_contract(&format!("{declarations}pure describe() {{ {{state: Ready, count: 1}} }}\n"));
        let other = root_schema_contract(&format!("{declarations}pure describe() {{ {{state: OtherReady, count: 1}} }}\n"));
        assert!(original.equivalent_to(&inferred));
        assert!(original.is_instance_of(&inferred).unwrap());
        assert!(!original.equivalent_to(&other));
        assert!(!original.is_instance_of(&other).unwrap_or(false));
    }

    #[test]
    fn result_return_instance_preserves_existing_callable_error_compatibility() {
        let declarations = "error Failure { Bad }\nerror Other { Wrong }\n";
        let original = root_schema_contract(&format!("{declarations}pure describe(ok: Bool) -> Result[Str] {{ guard ok else {{ return Err(Failure.Bad(\"bad\")) }}; \"yes\" }}\n"));
        let inferred = root_schema_contract(&format!("{declarations}pure describe(ok: Bool) {{ guard ok else {{ return Err(Failure.Bad(\"bad\")) }}; \"yes\" }}\n"));
        assert!(original.is_instance_of(&inferred).unwrap());
        let owner = SymbolOwner::new();
        owner.with_current(|| {
            let binder = SchemeTypeParameterId(7);
            let original = instance_contract(&owner,
                vec![instance_parameter(SchemeType::Ground(Type::Str))],
                SchemeType::Result(Box::new(SchemeType::Ground(Type::Str)), Box::new(SchemeType::Ground(Type::Error))),
                Vec::new(), Vec::new(), Some(Vec::new()));
            let inferred = instance_contract(&owner,
                vec![instance_parameter(SchemeType::Parameter(binder))],
                SchemeType::Result(Box::new(SchemeType::Parameter(binder)), Box::new(SchemeType::Ground(Type::ErrorVariant {
                    family: Name::intern("Failure"), variant: Name::intern("Bad"),
                }))), vec![binder], Vec::new(), Some(Vec::new()));
            assert!(inferred.is_generic());
            assert!(original.is_instance_of(&inferred).unwrap());
            let narrow = Type::Result(Box::new(Type::Str), Box::new(Type::ErrorVariant {
                family: Name::intern("Failure"), variant: Name::intern("Bad"),
            }));
            let original_identity = instance_contract(&owner,
                vec![instance_parameter(SchemeType::Ground(narrow))],
                SchemeType::Result(Box::new(SchemeType::Ground(Type::Str)), Box::new(SchemeType::Ground(Type::Error))),
                Vec::new(), Vec::new(), Some(Vec::new()));
            let generic_identity = instance_contract(&owner,
                vec![instance_parameter(SchemeType::Parameter(binder))], SchemeType::Parameter(binder),
                vec![binder], Vec::new(), Some(Vec::new()));
            assert!(original_identity.is_instance_of(&generic_identity).unwrap());
            let error = SchemeTypeParameterId(8);
            let unresolved = instance_contract(&owner,
                vec![instance_parameter(SchemeType::Parameter(binder))],
                SchemeType::Result(Box::new(SchemeType::Parameter(binder)), Box::new(SchemeType::Parameter(error))),
                vec![binder, error], Vec::new(), Some(Vec::new()));
            assert_eq!(original.is_instance_of(&unresolved), Err(ContractComparisonError::UnresolvedInstance));
        });
    }

    #[test]
    fn result_error_compatibility_retains_bare_errors_families_and_wrapping() {
        let declarations = "error Failure { Bad }\nerror Other { Wrong }\n";
        for (original, candidate) in [
            ("pure describe() -> Error { Failure.Bad(\"bad\") }\n", "pure describe() { Failure.Bad(\"bad\") }\n"),
            ("pure describe(ok: Bool) -> Result[Str, Failure] { guard ok else { return Err(Failure.Bad(\"bad\")) }; \"yes\" }\n", "pure describe(ok: Bool) { guard ok else { return Err(Other.Wrong(\"bad\")) }; \"yes\" }\n"),
            ("pure describe(ok: Bool) -> Result[Any] { guard ok else { return Err(Failure.Bad(\"bad\")) }; \"yes\" }\n", "pure describe(ok: Bool) { guard ok else { return Err(Failure.Bad(\"bad\")) }; \"yes\" }\n"),
            ("pure describe() -> Result[Str] { \"yes\" }\n", "pure describe() { \"yes\" }\n"),
        ] {
            let original = root_schema_contract(&format!("{declarations}{original}"));
            let candidate = root_schema_contract(&format!("{declarations}{candidate}"));
            assert!(!original.is_instance_of(&candidate).unwrap_or(false));
        }
    }

    #[test]
    fn wire_enum_projection_retains_other_schema_obligations() {
        for (declarations, original, candidate) in [
            ("enum State { Ready }\ntype Info = {state: State}\n", "pure describe() -> Info { {state: Ready} }\n", "pure describe() { {state: Ready} }\n"),
            ("enum State: Str { Ready = \"ready\" }\ntype Info = {state: State, count: UInt}\n", "pure describe(count: Int) -> Info { {state: Ready, count} }\n", "pure describe(count: Int) { {state: Ready, count} }\n"),
            ("enum State: Str { Ready = \"ready\" }\ntype Info = {state: State, count: Int = 1}\n", "pure describe() -> Info { {state: Ready, count: 1} }\n", "pure describe() { {state: Ready, count: 1} }\n"),
            ("enum State: Str { Ready = \"ready\" }\ntype Info[T] = {state: State}\n", "pure describe() -> Info[Int] { {state: Ready} }\n", "pure describe() { {state: Ready} }\n"),
            ("enum State: Str { Ready = \"ready\" }\nnominal type Info = {state: State}\n", "pure describe() -> Info { Info(state: Ready) }\n", "pure describe() { {state: Ready} }\n"),
        ] {
            let original = root_schema_contract(&format!("{declarations}{original}"));
            let candidate = root_schema_contract(&format!("{declarations}{candidate}"));
            assert!(!original.equivalent_to(&candidate));
            assert!(!original.is_instance_of(&candidate).unwrap_or(false));
        }
    }

    #[test]
    fn structural_return_interfaces_preserve_ground_shape_across_constructors() {
        let declarations = "type Disk = { mount: Str, used: Int }\ntype Other = { mount: Str, used: Int }\n";
        let original = root_schema_contract(&format!("{declarations}proc describe(mount: Str) -> Result[Disk] {{ Disk(mount: mount, used: mount.byte_len()) }}\n"));
        let inferred = root_schema_contract(&format!("{declarations}proc describe(mount: Str) {{ Disk(mount: mount, used: mount.byte_len()) }}\n"));
        assert!(original.equivalent_to(&inferred));
        for body in ["Other(mount: mount, used: mount.byte_len())", "{ mount: mount, used: mount.byte_len() }"] {
            let changed = root_schema_contract(&format!("{declarations}proc describe(mount: Str) {{ {body} }}\n"));
            assert!(original.equivalent_to(&changed), "{body}");
        }
    }

    #[test]
    fn structural_return_interfaces_preserve_ground_shape_across_all_returns() {
        let declarations = "type Disk = { mount: Str, used: Int }\ntype Other = { mount: Str, used: Int }\n";
        for body in [
            "let result = Disk(mount: mount, used: 7); result",
            "if mount == \"early\" { return Disk(mount: mount, used: 7) }; Disk(mount: mount, used: 9)",
            "if mount == \"early\" { let result = Disk(mount: mount, used: 7); result } else { Disk(mount: mount, used: 9) }",
            "match mount { \"early\" => Disk(mount: mount, used: 7); _ => Disk(mount: mount, used: 9) }",
        ] {
            let original = root_schema_contract(&format!("{declarations}proc describe(mount: Str) -> Result[Disk] {{ {body} }}\n"));
            let inferred = root_schema_contract(&format!("{declarations}proc describe(mount: Str) {{ {body} }}\n"));
            assert!(original.equivalent_to(&inferred), "{body}");
        }
        let original = root_schema_contract(&format!("{declarations}proc describe(mount: Str) -> Result[Disk] {{ Disk(mount: mount, used: 7) }}\n"));
        for body in [
            "if mount == \"early\" { return Other(mount: mount, used: 7) }; Disk(mount: mount, used: 9)",
            "if mount == \"early\" { Disk(mount: mount, used: 7) } else { Other(mount: mount, used: 9) }",
            "if mount == \"early\" { Disk(mount: mount, used: 7) } else { { mount: mount, used: 9 } }",
            "var result = Disk(mount: mount, used: 7); result = Other(mount: mount, used: 9); let alias = result; alias",
        ] {
            let inferred = root_schema_contract(&format!("{declarations}proc describe(mount: Str) {{ {body} }}\n"));
            assert!(original.equivalent_to(&inferred), "{body}");
        }
    }

    #[test]
    fn checked_callable_value_contract_distinguishes_calls_from_references() {
        let source = "pure integer(value: Int) -> Int { value }\nlet action = integer\nlet number = integer(1)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let reference = source.find("= integer\n").unwrap() + 2;
        let reference = crate::source::Span::new(SourceId::new(0), reference, reference + "integer".len());
        let invocation = source.find("integer(1)").unwrap();
        let invocation = crate::source::Span::new(SourceId::new(0), invocation, invocation + "integer(1)".len());
        assert!(checked.callable_value_contract_at(reference).is_some());
        assert!(checked.callable_value_contract_at(invocation).is_none());
        assert!(checked.callable_contract_at(invocation).is_some());
    }

    #[test]
    fn checked_callable_value_contract_does_not_replace_factory_result_with_callee() {
        let source = "type Callback = pure(value: Int) -> Int\npure integer(value: Int) -> Int { value }\npure factory() -> Callback { integer }\nlet action = factory()\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let invocation = source.rfind("factory()").unwrap();
        let invocation = crate::source::Span::new(SourceId::new(0), invocation, invocation + "factory()".len());
        assert!(matches!(checked.expr_types.get(&invocation), Some(Type::Callable(_))));
        assert!(checked.callable_value_contract_at(invocation).is_none());
        assert!(checked.callable_contract_at(invocation).is_some());
    }

    #[test]
    fn checked_callable_value_contract_preserves_generic_references_without_tagging_results() {
        let source = "pure identity(value) { value }\nlet action = identity\nlet number = identity(1)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let reference = source.find("= identity\n").unwrap() + 2;
        let reference = crate::source::Span::new(SourceId::new(0), reference, reference + "identity".len());
        let invocation = source.find("identity(1)").unwrap();
        let invocation = crate::source::Span::new(SourceId::new(0), invocation, invocation + "identity(1)".len());
        assert!(checked.callable_value_contract_at(reference).unwrap().is_generic());
        assert!(checked.callable_value_contract_at(invocation).is_none());
        assert!(checked.callable_contract_at(invocation).unwrap().is_generic());
    }

    #[test]
    fn checked_callable_contract_accessors_preserve_returned_relationship_context() {
        let source = "pure identity(value) { value }\npure factory() { let forwarded = identity; forwarded }\nlet saved = factory\nlet result = factory()\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let reference = source.find("= factory\n").unwrap() + 2;
        let reference = crate::source::Span::new(SourceId::new(0), reference, reference + "factory".len());
        let invocation = source.find("= factory()").unwrap() + 2;
        let invocation = crate::source::Span::new(SourceId::new(0), invocation, invocation + "factory()".len());
        let value_contract = checked.callable_value_contract_at(reference).unwrap();
        let call_contract = checked.callable_contract_at(invocation).unwrap();
        assert!(value_contract.equivalent_to(&call_contract));
        assert!(value_contract.to_string().contains("identity: forall T0. pure(value: T0) -> T0 effects=unknown"));
        assert!(checked.callable_value_contract_at(invocation).is_none());
    }

    #[test]
    fn generic_factory_retains_a_nested_callable_relationship() {
        let source = "pure identity(value) { value }\npure factory(marker) { let _ = marker; let forwarded = identity; forwarded }\nlet saved = factory\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let start = source.rfind("factory").unwrap();
        let reference = crate::source::Span::new(SourceId::new(0), start, start + "factory".len());
        let contract = checked.callable_value_contract_at(reference).expect("the factory value keeps its selected callable contract");
        assert!(contract.is_generic());
        assert!(contract.to_string().contains("identity: forall T0. pure(value: T0) -> T0 effects=unknown"));
    }

    #[test]
    fn nested_factory_equivalence_reuses_completed_relationship_pairs() {
        const FACTORIES: usize = 14;
        let mut source = String::from("pure identity(value) { value }\n");
        for index in 0..FACTORIES {
            let target = if index == 0 { "identity".to_owned() } else { format!("factory{}", index - 1) };
            source.push_str(&format!(
                "pure factory{index}(marker) {{ let _ = marker; let forwarded = {target}; forwarded }}\n"
            ));
        }
        source.push_str(
            "pure first(marker) { let _ = marker; let forwarded = factory13; forwarded }\npure second(marker) { let _ = marker; let forwarded = factory13; forwarded }\nlet operations = [first, second]\n"
        );

        fn checked_factory(source: &str) -> (CheckedCallableContract, Type) {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, source,
                CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let (&definition, _) = checked.checked_declarations.functions.iter()
                .find(|(_, declaration)| declaration.name.as_str() == "factory13").unwrap();
            let contract = checked.callable_declaration_contract(definition).unwrap();
            let list = parsed.arena.arena.source_rows().expr_ids(SourceId::new(0)).find(|&expression| {
                matches!(parsed.arena.arena.expr(expression).kind, crate::syntax::arena::ArenaExprKind::List(_))
            }).unwrap();
            (contract, checked.expr_types[&parsed.arena.arena.expr(list).span].clone())
        }

        let (left, left_family) = checked_factory(&source);
        let (right, right_family) = checked_factory(&source);
        let mut equivalence = BoundEquivalence::new(&left.schema_identities, &right.schema_identities,
            left.scheme_callables.as_deref(), right.scheme_callables.as_deref());
        assert!(equivalence.scheme(&left.scheme, &right.scheme));
        assert!(equivalence.effects(left.scheme.effects().contract(), right.scheme.effects().contract()));
        let expansions = equivalence.completed_callables.as_ref().unwrap().borrow().expansions;
        assert!(expansions > 0, "the checked factory chain has relationship pairs to compare");
        assert!(expansions <= FACTORIES,
            "expanded {} completed relationship pairs for {FACTORIES} nested factories",
            expansions);

        let mut family_equivalence = BoundEquivalence::new(&left.schema_identities, &right.schema_identities,
            left.scheme_callables.as_deref(), right.scheme_callables.as_deref());
        assert!(family_equivalence.ground_type(&left_family, &right_family));
        let family_expansions = family_equivalence.completed_callables.as_ref().unwrap().borrow().expansions;
        assert!(family_expansions > 0, "the two-target family has nested relationship pairs to compare");
        assert!(family_expansions <= FACTORIES + 2,
            "expanded {} completed relationship pairs for a two-target family over {FACTORIES} nested factories",
            family_expansions);
    }

    #[test]
    fn nested_callable_rendering_labels_shared_relationships_once() {
        const FACTORIES: usize = 8;
        let mut source = String::from("pure identity(value) { value }\n");
        for index in 0..FACTORIES {
            let target = if index == 0 { "identity".to_owned() } else { format!("factory{}", index - 1) };
            source.push_str(&format!(
                "pure factory{index}(marker) {{ let _ = marker; let forwarded = {target}; forwarded }}\n"
            ));
        }
        source.push_str(
            "pure first(marker) { let _ = marker; let forwarded = factory7; forwarded }\npure second(marker) { let _ = marker; let forwarded = factory7; forwarded }\npure expose() { [first, second] }\n"
        );

        fn render(source: &str) -> String {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, source,
                CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let (&definition, _) = checked.checked_declarations.functions.iter()
                .find(|(_, declaration)| declaration.name.as_str() == "expose").unwrap();
            checked.callable_declaration_contract(definition).unwrap().to_string()
        }

        let rendered = render(&source);
        assert_eq!(rendered, render(&source));
        for name in std::iter::once("identity".to_owned())
            .chain((0..FACTORIES).map(|index| format!("factory{index}")))
            .chain(["first".to_owned(), "second".to_owned()])
        {
            assert_eq!(rendered.matches(&format!("{name}: ")).count(), 1, "{name}: {rendered}");
        }
        let definitions = (0..32)
            .map(|label| rendered.matches(&format!("callable#{label}{{")).count())
            .sum::<usize>();
        assert!(definitions >= FACTORIES, "{rendered}");
        assert!(definitions < rendered.matches("callable#").count(), "the shared family renders a reference: {rendered}");
        for label in 0..definitions {
            assert_eq!(rendered.matches(&format!("callable#{label}{{")).count(), 1, "{rendered}");
        }
    }

    #[test]
    fn inferred_declaration_contract_can_return_a_generic_callable_relationship() {
        let source = "pure identity(value) { value }\npure get_identity() { let action = identity; action }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let (&declaration_id, declaration) = checked.checked_declarations.functions.iter()
            .find(|(_, declaration)| declaration.name.as_str() == "get_identity").unwrap();
        let contract = declaration.callable_contract().unwrap();
        let contextual = checked.callable_declaration_contract(declaration_id).unwrap();
        let SchemeType::SchemeCallable(relationship) = contract.scheme().return_type() else {
            panic!("the inferred declaration retains its generic callable relationship: {:?}", contract.scheme().return_type());
        };
        let relationship = checked.scheme_callables.get(*relationship).unwrap();
        assert_eq!(relationship.targets().len(), 1);
        assert_eq!(relationship.targets()[0].contract().to_string(), "forall T0. pure(value: T0) -> T0 effects=unknown");
        assert!(!contract.equivalent_to(&contextual));
        assert!(contextual.equivalent_to(&contextual));
        assert!(contextual.to_string().contains("identity: forall T0. pure(value: T0) -> T0 effects=unknown"));
    }

    fn returned_scheme_callable(owner: &SymbolOwner, namespace: &str, target_contract: CheckedCallableContract) -> CheckedCallableContract {
        let (table, relationship) = owner.with_current(|| {
            let target = super::super::CheckedFunctionTarget {
                definition: crate::syntax::arena::FunctionDefId::from_index(0),
                namespace: Some(Name::intern(namespace)),
                name: Name::intern("identity"),
            };
            let mut table = crate::sema::scheme_callable::SchemeCallableTable::default();
            let id = table.insert(vec![(target, target_contract)]).unwrap();
            (table, id)
        });
        owner.with_current(|| CheckedCallableContract {
            scheme: Arc::new(TypeScheme::new(crate::sema::types::CallableKind::Pure, Vec::new(), Vec::new(), Vec::new(),
                SchemeType::SchemeCallable(relationship), None, false, None, false).unwrap()),
            symbol_owner: owner.clone(),
            schema_identities: Default::default(),
            scheme_callables: Some(Arc::new(table)),
        })
    }

    #[test]
    fn checked_callable_contract_compares_scoped_callable_relationships() {
        let owner = SymbolOwner::new();
        owner.with_current(|| {
            let left = returned_scheme_callable(&owner, "model", identity(&owner, 7, None));
            let renamed = returned_scheme_callable(&owner, "model", identity(&owner, 91, None));
            assert!(matches!((left.scheme.return_type(), renamed.scheme.return_type()),
                (SchemeType::SchemeCallable(left), SchemeType::SchemeCallable(right)) if left != right));
            assert!(left.equivalent_to(&renamed));
            assert_eq!(left.to_string(), renamed.to_string());

            let integer = CheckedCallableContract {
                scheme: Arc::new(TypeScheme::new(crate::sema::types::CallableKind::Pure, Vec::new(), Vec::new(),
                    vec![SchemeParameter { name: Name::intern("value"), ty: SchemeType::Ground(Type::Int),
                        schema_expectation: None, defaulted: false, rest: false }],
                    SchemeType::Ground(Type::Int), None, false, None, false).unwrap()),
                symbol_owner: owner.clone(), schema_identities: Default::default(), scheme_callables: None,
            };
            let changed_domain = returned_scheme_callable(&owner, "model", integer);
            assert!(!left.equivalent_to(&changed_domain));
            assert_eq!(left.is_instance_of(&changed_domain), Ok(false));

            let binder = SchemeTypeParameterId(91);
            let effectful = CheckedCallableContract {
                scheme: Arc::new(TypeScheme::new(crate::sema::types::CallableKind::Proc, vec![binder], Vec::new(),
                    vec![SchemeParameter { name: Name::intern("value"), ty: SchemeType::Parameter(binder),
                        schema_expectation: None, defaulted: false, rest: false }],
                    SchemeType::Parameter(binder), None, false, Some(vec![crate::syntax::node::Effect::Fs]), false).unwrap()),
                symbol_owner: owner.clone(), schema_identities: Default::default(), scheme_callables: None,
            };
            let changed_effect = returned_scheme_callable(&owner, "model", effectful);
            assert!(!left.equivalent_to(&changed_effect));
            assert_eq!(left.is_instance_of(&changed_effect), Ok(false));
            assert!(!left.equivalent_to(&returned_scheme_callable(&owner, "other", identity(&owner, 91, None))));
        });
    }

    #[test]
    fn checked_callable_contract_distinguishes_independently_parsed_schema_names() {
        let first = root_schema_contract("type First[T] = {label: Str}\npure describe(value: First[Int]) -> Str { value.label }\n");
        let second = root_schema_contract("type Second[T] = {label: Str}\npure describe(value: Second[Int]) -> Str { value.label }\n");
        assert!(!first.equivalent_to(&second));
        assert_ne!(first.to_string(), second.to_string());
        assert!(first.source_annotation().is_none());
        assert!(second.source_annotation().is_none());
    }

    #[test]
    fn checked_callable_contract_preserves_schema_identity_when_arena_indices_shift() {
        let first = root_schema_contract("type Entry[T] = {label: Str}\npure describe(value: Entry[Int]) -> Str { value.label }\n");
        let shifted = root_schema_contract("type Unrelated = Int\ntype Entry[T] = {label: Str}\npure describe(value: Entry[Int]) -> Str { value.label }\n");
        assert!(first.equivalent_to(&shifted));
        assert_eq!(first.to_string(), shifted.to_string());
        assert!(first.join_effects(&shifted).unwrap().equivalent_to(&first));
    }

    #[test]
    fn checked_callable_contract_projects_empty_schema_contexts_without_mutating_them() {
        let checked = root_schema_contract("pure describe(value: List[Int]) -> Int { value.len() }\n");
        let context = checked.scheme.params()[0].schema_expectation.clone();
        assert!(context.is_some());
        let absent = CheckedCallableContract {
            scheme: Arc::new(TypeScheme::new(checked.scheme.kind(), Vec::new(), Vec::new(),
                checked.scheme.params().iter().cloned().map(|mut parameter| {
                    parameter.schema_expectation = None;
                    parameter
                }).collect(), checked.scheme.return_type().clone(), None, checked.scheme.explicit_return(),
                checked.scheme.effects().concrete().unwrap(), checked.scheme.inferred_effects()).unwrap()),
            symbol_owner: checked.symbol_owner.clone(),
            schema_identities: Arc::clone(&checked.schema_identities),
            scheme_callables: None,
        };
        assert_eq!(checked.source_annotation().as_deref(), Some("pure(value: List[Int]) -> Int"));
        assert!(checked.equivalent_to(&absent));
        assert_eq!(checked.scheme.params()[0].schema_expectation, context);
    }

    fn imported_schema_contract(key: &str, namespace: &str) -> CheckedCallableContract {
        imported_schema_contract_with_prefix(key, namespace, "")
    }

    fn imported_schema_contract_with_prefix(key: &str, namespace: &str, prefix: &str) -> CheckedCallableContract {
        use crate::syntax::arena::ArenaProgramBuilder;
        let source = format!("{prefix}use shared as api\n");
        let module_source = "##! Schema module.\n## A phantom schema.\nexport type Entry[T] = {label: Str}\n## Reads its label.\nexport pure describe(value: Entry[Int]) -> Str { value.label }\n";
        let mut builder = ArenaProgramBuilder::with_token_capacity(128);
        let root = Parser::parse_source_into_arena_builder(SourceId::new(0), &source, &mut builder);
        let module = Parser::parse_source_into_arena_builder(SourceId::new(1), module_source, &mut builder);
        assert!(root.diagnostics.is_empty(), "{:?}", root.diagnostics);
        assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
        let name = builder.symbol_owner().intern(namespace);
        builder.push_arena_module(key.to_owned(), name, module.statements);
        let (use_id, _, _) = builder.statement_ids(root.statements).iter()
            .find_map(|statement| builder.use_stmt_for_statement(*statement)).unwrap();
        builder.set_use_resolved(use_id, Arc::from(key));
        let program = builder.finish_with_statements(root.statements);
        let checked = Checker::check_arena_with_options(&program, &source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        program.symbol_owner().with_current(|| checked.checked_declarations.functions.values()
            .find(|declaration| declaration.name.as_str() == "describe").unwrap().callable_contract().unwrap())
    }

    #[test]
    fn checked_callable_contract_anchors_imported_schemas_to_canonical_module_keys() {
        let first = imported_schema_contract("/library/first.xsh", "shared");
        let second = imported_schema_contract("/library/second.xsh", "shared");
        let renamed = imported_schema_contract("/library/first.xsh", "renamed");
        assert!(!first.equivalent_to(&second));
        assert_ne!(first.to_string(), second.to_string());
        assert!(first.equivalent_to(&renamed));
        assert_eq!(first.to_string(), renamed.to_string());
    }

    #[test]
    fn checked_callable_contract_preserves_exact_mapped_conversion_requirements() {
        use crate::sema::schemes::{CallableScheme, ValueConversion, ValueConversionSet};
        let owner = SymbolOwner::new();
        let constrained = |binder, conversion| {
            let mut contract = identity(&owner, binder, None);
            let original = contract.scheme.callable();
            let mut requirements = original.requirements().clone();
            if let Some(conversion) = conversion {
                let mut conversions = ValueConversionSet::default();
                conversions.insert(conversion);
                requirements.conversion_parameters.insert(SchemeTypeParameterId(binder), conversions);
            }
            let callable = CallableScheme::new(original.type_parameters().to_vec(), original.row_parameters().to_vec(), original.params().to_vec(), original.return_type().clone(), original.return_schema().cloned(), original.explicit_return(), requirements).unwrap();
            contract.scheme = Arc::new(TypeScheme::with_callable(contract.scheme.kind(), Arc::new(callable), contract.scheme.effects().clone(), false).unwrap());
            contract
        };
        let display = constrained(7, Some(ValueConversion::Display));
        let renamed = constrained(91, Some(ValueConversion::Display));
        assert!(display.equivalent_to(&renamed));
        let callables = crate::sema::scheme_callable::SchemeCallableTable::default();
        let correspondence = display.binder_correspondence(&renamed, &callables).unwrap();
        assert_eq!(correspondence.types().get(&SchemeTypeParameterId(7)), Some(&SchemeTypeParameterId(91)));
        for changed in [constrained(91, Some(ValueConversion::Argv)), constrained(91, None)] {
            assert!(!display.equivalent_to(&changed));
            assert!(display.binder_correspondence(&changed, &callables).is_none());
        }
    }

    #[test]
    fn checked_callable_contract_preserves_binder_relations_and_nominal_identities() {
        let owner = SymbolOwner::new();
        let left = identity(&owner, 7, None);
        let right = identity(&owner, 91, None);
        assert!(left.equivalent_to(&right));
        assert_eq!(left.to_string(), right.to_string());
        assert_eq!(left.kind(), crate::sema::types::CallableKind::Pure);
        assert!(left.is_generic());
        assert!(left.source_annotation().is_none());
        assert!(!left.equivalent_to(&identity(&owner, 91, Some(Type::Int))));
        owner.with_current(|| {
            let first = identity(&owner, 7, Some(Type::Tag(Name::intern("ContractFirstNominal"))));
            let second = identity(&owner, 7, Some(Type::Tag(Name::intern("ContractSecondNominal"))));
            assert!(!first.equivalent_to(&second));
            assert_ne!(first.to_string(), second.to_string());
        });
    }

    #[test]
    fn checked_callable_contract_retains_iteration_relationships_from_callback_results() {
        let first = root_schema_contract("pure describe(factory) { [value for value in factory()] }\n");
        let renamed = root_schema_contract("pure unrelated(value) { value }\npure describe(factory) { [item for item in factory()] }\n");
        assert!(first.to_string().contains("iterable("), "{first}");
        assert_eq!(first.to_string(), renamed.to_string());
        assert!(first.equivalent_to(&renamed));
    }

    #[test]
    fn checked_callable_contract_retains_pipeline_policy_across_binder_renaming() {
        let pipeline = root_schema_contract("pure describe(values) { values |> collect }\n");
        let shifted = root_schema_contract("pure unrelated(value) { value }\npure describe(values) { values |> collect }\n");
        let iterable = root_schema_contract("pure describe(values) { [value for value in values] }\n");
        assert!(pipeline.equivalent_to(&shifted));
        assert_eq!(pipeline.to_string(), shifted.to_string());
        assert!(!pipeline.equivalent_to(&iterable));
        assert_ne!(pipeline.to_string(), iterable.to_string());
        assert!(pipeline.to_string().contains("pipeline-transport"));
        assert!(pipeline.source_annotation().is_none());
    }

    #[test]
    fn checked_callable_contract_displays_map_key_bounds_without_source_syntax() {
        let bounded = root_schema_contract("pure describe(value) -> Unit { var seen = set.empty(); seen = seen.add(value); let _ = seen }\n");
        let shifted = root_schema_contract("pure unrelated(value) { value }\npure describe(value) -> Unit { var entries = set.empty(); entries = entries.add(value); let _ = entries }\n");
        let unbounded = root_schema_contract("pure describe(value) -> Unit { let _ = value }\n");
        assert!(bounded.to_string().starts_with("forall T0: MapKey. pure(value: T0) -> Unit"), "{bounded}");
        assert_eq!(bounded.to_string(), shifted.to_string());
        assert!(bounded.equivalent_to(&shifted));
        assert!(!bounded.equivalent_to(&unbounded));
        assert!(unbounded.to_string().starts_with("forall T0. pure(value: T0) -> Unit"), "{unbounded}");
        assert!(bounded.source_annotation().is_none());
        assert!(shifted.source_annotation().is_none());
        assert!(unbounded.source_annotation().is_none());
    }

    #[test]
    fn checked_callable_contract_compares_row_binders_and_lacks() {
        let owner = SymbolOwner::new();
        owner.with_current(|| {
            let build = |id, extra| {
                let field = Name::intern("value");
                let mut lacks = BTreeSet::from([field]);
                if extra { lacks.insert(Name::intern("other")); }
                let row = SchemeRowParameterId(id);
                CheckedCallableContract {
                    scheme: Arc::new(TypeScheme::new(crate::sema::types::CallableKind::Pure, Vec::new(),
                        vec![SchemeRowParameter { id: row, lacks }],
                        vec![SchemeParameter { name: Name::intern("record"),
                            ty: SchemeType::Record { fields: BTreeMap::from([(field, SchemeType::Ground(Type::Int))]), tail: SchemeRowTail::Parameter(row) },
                            schema_expectation: None, defaulted: false, rest: false }],
                        SchemeType::Ground(Type::Int), None, false, None, false).unwrap()),
                    symbol_owner: owner.clone(),
                    schema_identities: Default::default(),
                    scheme_callables: None,
                }
            };
            let first = build(3, false);
            let renamed = build(42, false);
            assert!(first.equivalent_to(&renamed));
            assert_eq!(first.to_string(), renamed.to_string());
            assert!(!first.equivalent_to(&build(3, true)));
        });
    }

    fn instance_contract(owner: &SymbolOwner, params: Vec<SchemeParameter>, result: SchemeType,
        types: Vec<SchemeTypeParameterId>, rows: Vec<SchemeRowParameter>,
        effects: Option<Vec<crate::syntax::node::Effect>>) -> CheckedCallableContract
    {
        CheckedCallableContract {
            scheme: Arc::new(TypeScheme::new(crate::sema::types::CallableKind::Proc, types, rows,
                params, result, None, false, effects, false).unwrap()),
            symbol_owner: owner.clone(), schema_identities: Default::default(), scheme_callables: None,
        }
    }

    fn instance_parameter(ty: SchemeType) -> SchemeParameter {
        SchemeParameter { name: Name::intern("value"), ty, schema_expectation: None, defaulted: false, rest: false }
    }

    #[test]
    fn checked_callable_contract_instance_solves_shared_parameter_and_return_together() {
        let owner = SymbolOwner::new();
        owner.with_current(|| {
            let generic = identity(&owner, 7, None);
            let concrete = |result| {
                let mut contract = instance_contract(&owner, vec![instance_parameter(SchemeType::Ground(Type::Int))],
                    SchemeType::Ground(result), Vec::new(), Vec::new(), None);
                contract.scheme = Arc::new(TypeScheme::with_callable(crate::sema::types::CallableKind::Pure,
                    Arc::clone(contract.scheme.callable()), crate::sema::effects::CheckedEffects::unknown(), false).unwrap());
                contract
            };
            let integer = concrete(Type::Int);
            assert_eq!(integer.is_instance_of(&generic), Ok(true));
            assert_eq!(concrete(Type::Str).is_instance_of(&generic), Err(ContractComparisonError::CannotEstablishConstraints));
            assert_eq!(generic.is_instance_of(&integer), Err(ContractComparisonError::NonConcreteOriginal));
            assert_eq!(generic.is_instance_of(&generic), Err(ContractComparisonError::NonConcreteOriginal));
        });
    }

    #[test]
    fn checked_callable_contract_instance_reuses_header_solution_without_body_constraints() {
        let owner = SymbolOwner::new();
        owner.with_current(|| {
            let binder = SchemeTypeParameterId(7);
            let generic = instance_contract(&owner, vec![instance_parameter(SchemeType::Parameter(binder))],
                SchemeType::Parameter(binder), vec![binder], Vec::new(), Some(Vec::new()));
            let concrete = instance_contract(&owner, vec![instance_parameter(SchemeType::Ground(Type::Int))],
                SchemeType::Ground(Type::Int), Vec::new(), Vec::new(), Some(Vec::new()));
            let instance = concrete.instance_evidence(&generic).unwrap().unwrap();
            let metrics = instance.constraints.metrics();
            assert_eq!(metrics.scheme_instantiations, 1);
            let operand = SchemeType::List(Box::new(SchemeType::Parameter(binder)));
            assert_eq!(instance.specialize(&operand, &generic.scheme), Some(Type::List(Box::new(Type::Int))));
            assert_eq!(instance.specialize(&SchemeType::Parameter(SchemeTypeParameterId(99)), &generic.scheme), None);
            assert_eq!(instance.constraints.metrics(), metrics);
        });
    }

    #[test]
    fn checked_callable_contract_instance_rejects_concrete_widening() {
        let owner = SymbolOwner::new();
        owner.with_current(|| {
            let make = |parameter| instance_contract(&owner, vec![instance_parameter(SchemeType::Ground(parameter))],
                SchemeType::Ground(Type::Int), Vec::new(), Vec::new(), None);
            assert_eq!(make(Type::Int).is_instance_of(&make(Type::Any)), Ok(false));
            assert_eq!(make(Type::Record(Arc::new(BTreeMap::from([(Name::intern("extra"), Type::Int)]))))
                .is_instance_of(&make(Type::Record(Arc::new(BTreeMap::new())))), Ok(false));
        });
    }

    #[test]
    fn checked_callable_contract_instance_anchors_checked_dynamic_and_empty_types() {
        let owner = SymbolOwner::new();
        owner.with_current(|| {
            let binder = SchemeTypeParameterId(0);
            let generic = instance_contract(&owner, vec![instance_parameter(SchemeType::Parameter(binder))],
                SchemeType::Parameter(binder), vec![binder], Vec::new(), Some(Vec::new()));
            for ty in [Type::Null, Type::Any, Type::Record(Arc::new(BTreeMap::new())),
                Type::List(Box::new(Type::Any))]
            {
                let concrete = instance_contract(&owner, vec![instance_parameter(SchemeType::Ground(ty.clone()))],
                    SchemeType::Ground(ty.clone()), Vec::new(), Vec::new(), Some(Vec::new()));
                assert_eq!(concrete.is_instance_of(&generic), Ok(true), "{ty:?}");
                let mismatched = instance_contract(&owner, vec![instance_parameter(SchemeType::Ground(ty.clone()))],
                    SchemeType::Ground(Type::Int), Vec::new(), Vec::new(), Some(Vec::new()));
                let expected = if ty == Type::Any { Ok(false) } else { Err(ContractComparisonError::CannotEstablishConstraints) };
                assert_eq!(mismatched.is_instance_of(&generic), expected, "{ty:?}");
            }
        });
    }

    #[test]
    fn checked_callable_contract_instance_preserves_layout_kind_and_effects() {
        use crate::syntax::node::Effect;
        let owner = SymbolOwner::new();
        owner.with_current(|| {
            let parameter = instance_parameter(SchemeType::Ground(Type::Int));
            let make = |parameter, effects| instance_contract(&owner, vec![parameter], SchemeType::Ground(Type::Int),
                Vec::new(), Vec::new(), effects);
            let original = make(parameter.clone(), Some(vec![Effect::Fs]));
            assert_eq!(original.is_instance_of(&original), Ok(true));
            for mutated in [SchemeParameter { name: Name::intern("other"), ..parameter.clone() },
                SchemeParameter { defaulted: true, ..parameter.clone() },
                SchemeParameter { rest: true, ..parameter.clone() }]
            {
                assert_eq!(original.is_instance_of(&make(mutated, Some(vec![Effect::Fs]))), Ok(false));
            }
            assert_eq!(original.is_instance_of(&make(parameter.clone(), Some(vec![Effect::Process]))), Ok(false));
            assert_eq!(make(parameter.clone(), None).is_instance_of(&make(parameter.clone(), Some(Vec::new()))), Ok(false));
            let mut pure = original.clone();
            pure.scheme = Arc::new(TypeScheme::with_callable(crate::sema::types::CallableKind::Pure,
                Arc::clone(original.scheme.callable()), original.scheme.effects().clone(), false).unwrap());
            assert_eq!(original.is_instance_of(&pure), Ok(false));
        });
    }

    #[test]
    fn checked_callable_contract_instance_uses_canonical_schema_module_identity() {
        let first = imported_schema_contract("/library/first.xsh", "shared");
        let shifted = imported_schema_contract_with_prefix("/library/first.xsh", "renamed", "type Unrelated = Int\n");
        assert_ne!(first.schema_identities.keys().next(), shifted.schema_identities.keys().next());
        assert_eq!(first.is_instance_of(&shifted), Ok(true));
        assert_eq!(shifted.is_instance_of(&first), Ok(true));
        assert_eq!(first.is_instance_of(&imported_schema_contract("/library/second.xsh", "shared")), Ok(false));
        assert_eq!(root_schema_contract("type Entry[T] = {label: Str}\npure describe(value: Entry[Int]) -> Str { value.label }\n")
            .is_instance_of(&root_schema_contract("type Entry[T] = {label: Str}\npure describe(value: Entry[Str]) -> Str { value.label }\n")), Ok(false));
    }

    #[test]
    fn checked_callable_contract_instance_preserves_row_lacks() {
        let owner = SymbolOwner::new();
        owner.with_current(|| {
            let value = Name::intern("value");
            let extra = Name::intern("extra");
            let row = SchemeRowParameterId(4);
            let generalized = |lacks| instance_contract(&owner,
                vec![instance_parameter(SchemeType::Record { fields: BTreeMap::from([(value, SchemeType::Ground(Type::Int))]),
                    tail: SchemeRowTail::Parameter(row) })], SchemeType::Ground(Type::Int), Vec::new(),
                vec![SchemeRowParameter { id: row, lacks }], Some(Vec::new()));
            let concrete = instance_contract(&owner, vec![instance_parameter(SchemeType::Ground(Type::Record(Arc::new(
                BTreeMap::from([(value, Type::Int), (extra, Type::Str)])))))], SchemeType::Ground(Type::Int),
                Vec::new(), Vec::new(), Some(Vec::new()));
            assert_eq!(concrete.is_instance_of(&generalized(BTreeSet::from([value]))), Ok(true));
            assert_eq!(concrete.is_instance_of(&generalized(BTreeSet::from([value, extra]))), Err(ContractComparisonError::CannotEstablishConstraints));
        });
    }

    #[test]
    fn checked_callable_contract_instance_reports_unsupported_generic_union_constraints() {
        let owner = SymbolOwner::new();
        owner.with_current(|| {
            let binder = SchemeTypeParameterId(0);
            let generalized = instance_contract(&owner,
                vec![instance_parameter(SchemeType::Union(vec![SchemeType::Parameter(binder), SchemeType::Ground(Type::Int)]))],
                SchemeType::Parameter(binder), vec![binder], Vec::new(), Some(Vec::new()));
            let concrete = instance_contract(&owner,
                vec![instance_parameter(SchemeType::Union(vec![SchemeType::Ground(Type::Str), SchemeType::Ground(Type::Int)]))],
                SchemeType::Ground(Type::Str), Vec::new(), Vec::new(), Some(Vec::new()));
            assert_eq!(concrete.is_instance_of(&generalized), Err(ContractComparisonError::CannotEstablishConstraints));
        });
    }

    #[test]
    fn checked_callable_contract_instance_preserves_nested_schema_limit_and_stream_kind() {
        let owner = SymbolOwner::new();
        owner.with_current(|| {
            let callback = |kind, schema| SchemeType::Callable {
                kind, effects: crate::sema::effects::EffectContract::finite(crate::sema::effects::EffectSet::default()),
                monotype: Arc::new(CallableMonotype::new(
                    vec![instance_parameter(SchemeType::Ground(Type::Int))], SchemeType::Ground(Type::Int), schema)),
            };
            let make = |ty| instance_contract(&owner, vec![instance_parameter(ty)], SchemeType::Ground(Type::Int),
                Vec::new(), Vec::new(), Some(Vec::new()));
            let pure = make(callback(crate::sema::types::CallableKind::Pure, None));
            let schema = SchemaExpectation {
                instances: vec![crate::sema::constants::SchemaInstance {
                    definition: TypeDefId::from_index(0), arguments: Vec::new(),
                }],
                ..SchemaExpectation::default()
            };
            let schema = make(callback(crate::sema::types::CallableKind::Pure, Some(schema)));
            let stream = make(callback(crate::sema::types::CallableKind::Stream, None));
            assert_eq!(pure.is_instance_of(&pure), Ok(true));
            assert_eq!(schema.is_instance_of(&schema), Err(ContractComparisonError::UnsupportedContract));
            assert_eq!(pure.is_instance_of(&schema), Err(ContractComparisonError::UnsupportedContract));
            assert_eq!(stream.is_instance_of(&stream), Ok(true));
            assert_eq!(pure.is_instance_of(&stream), Err(ContractComparisonError::CannotEstablishConstraints));
            assert_eq!(stream.is_instance_of(&pure), Err(ContractComparisonError::CannotEstablishConstraints));
        });
    }

    #[test]
    fn checked_callable_contract_survives_frontend_disposal_and_selected_calls() {
        let (generic, ground, call_contract) = {
            let source = "pure contract_identity(value) { value }\npure contract_integer(value: Int) -> Int { value }\nlet result: Int = contract_identity(42)\n";
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, source,
                CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            parsed.arena.symbol_owner().with_current(|| {
                let declaration = |name| checked.checked_declarations.functions.values()
                    .find(|declaration| declaration.name.as_str() == name).unwrap().callable_contract().unwrap();
                let generic = declaration("contract_identity");
                let ground = declaration("contract_integer");
                let call = checked.selected_function_calls.iter().find(|(_, target)| target.name.as_str() == "contract_identity").unwrap().0;
                let selected = checked.callable_contract_at(*call).unwrap();
                assert!(generic.equivalent_to(&selected));
                (generic, ground, selected)
            })
        };
        let unrelated_owner = SymbolOwner::new();
        unrelated_owner.with_current(|| {
            assert!(generic.is_generic());
            assert!(!ground.is_generic());
            assert!(generic.equivalent_to(&call_contract));
            assert!(generic.to_string().contains("value"));
            assert!(ground.to_string().contains("Int"));
        });
    }
}

#[cfg(test)]
mod effect_join_tests {
    use super::*;
    use crate::sema::schemes::SchemeParameter;
    use crate::sema::types::Type;
    use crate::symbol::Name;
    use crate::syntax::node::Effect;

    #[test]
    fn checked_callable_contract_joins_effects_without_widening_shape() {
        let owner = SymbolOwner::new();
        owner.with_current(|| {
            let build = |effects, return_type| CheckedCallableContract {
                scheme: Arc::new(TypeScheme::new(crate::sema::types::CallableKind::Proc, Vec::new(), Vec::new(),
                    vec![SchemeParameter { name: Name::intern("value"), ty: SchemeType::Ground(Type::Str),
                        schema_expectation: None, defaulted: false, rest: false }],
                    SchemeType::Ground(return_type), None, false, effects, false).unwrap()),
                symbol_owner: owner.clone(),
                schema_identities: Default::default(),
                scheme_callables: None,
            };
            let fs = build(Some(vec![Effect::Fs, Effect::Error]), Type::Str);
            let process = build(Some(vec![Effect::Process, Effect::Error]), Type::Str);
            assert!(!fs.equivalent_to(&process));
            let joined = fs.join_effects(&process).unwrap();
            assert!(joined.equivalent_to(&build(Some(vec![Effect::Fs, Effect::Process, Effect::Error]), Type::Str)));
            assert!(joined.equivalent_to(&process.join_effects(&fs).unwrap()));
            assert_eq!(joined.source_annotation().as_deref(), Some("proc(value: Str) [fs, process, error] -> Str"));
            assert!(fs.join_effects(&build(Some(vec![Effect::Process]), Type::Int)).is_none());
            assert!(fs.join_effects(&build(None, Type::Str)).unwrap().equivalent_to(&build(None, Type::Str)));
        });
    }
}
