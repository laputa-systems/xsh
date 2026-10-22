use super::{CheckedFunctionDeclaration, Checker, CheckOutput};
use crate::sema::constants::SchemaExpectation;
use crate::sema::schemes::{CallableMonotype, SchemeRowParameterId,
    SchemeRowTail, SchemeType, SchemeTypeParameterId, SchemeEffectParameterId, TypeScheme};
use crate::symbol::SymbolOwner;
use crate::syntax::arena::{ArenaProgram, TypeDefId};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

mod render;

/// An immutable view of the checker's selected callable contract. Cloning the
/// view preserves its symbol spellings after its source arena is dropped.
#[derive(Clone, Debug)]
pub struct CheckedCallableContract {
    scheme: Arc<TypeScheme>,
    symbol_owner: SymbolOwner,
    schema_identities: Arc<BTreeMap<TypeDefId, SchemaIdentity>>,
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
}

pub(crate) struct CallableInstanceEvidence {
    constraints: crate::sema::constraints::TypeConstraints,
    instance: crate::sema::constraints::InstantiatedScheme,
    scheme: Arc<TypeScheme>,
    origin: crate::source::Span,
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
                SchemeType::Ground(_) | SchemeType::Parameter(_) => {}
            }
        }
        let mut definitions = BTreeSet::new();
        while let Some(schema) = schemas.pop() {
            definitions.extend(schema.instances.iter().map(|instance| instance.definition));
            schemas.extend(schema.children.values());
        }
        schema_identities(program, &self.record_constructors, definitions)
    }
}

pub(super) fn schema_identities(program: &ArenaProgram, constructors: &crate::sema::constants::RecordConstructors,
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
        (definition, SchemaIdentity { module, name })
    }).collect())
}

pub(super) fn same_schema(left: Option<&SchemaExpectation>, right: Option<&SchemaExpectation>,
    left_schemas: &BTreeMap<TypeDefId, SchemaIdentity>, right_schemas: &BTreeMap<TypeDefId, SchemaIdentity>) -> bool
{
    BoundEquivalence { types: BTreeMap::new(), rows: BTreeMap::new(), effects: BTreeMap::new(), left_schemas, right_schemas }.schema(left, right)
}

pub(super) fn same_bound_effects(
    left: &crate::sema::effects::CheckedEffects, left_owner: &TypeScheme,
    right: &crate::sema::effects::CheckedEffects, right_owner: &TypeScheme,
    left_schemas: &BTreeMap<TypeDefId, SchemaIdentity>, right_schemas: &BTreeMap<TypeDefId, SchemaIdentity>,
) -> bool {
    let mut equivalence = BoundEquivalence { types: BTreeMap::new(), rows: BTreeMap::new(), effects: BTreeMap::new(), left_schemas, right_schemas };
    equivalence.scheme(left_owner, right_owner) && equivalence.effects(left.contract(), right.contract())
}

pub(super) fn same_bound_operand(
    left: &SchemeType, left_owner: &TypeScheme, right: &SchemeType, right_owner: &TypeScheme,
    left_schemas: &BTreeMap<TypeDefId, SchemaIdentity>, right_schemas: &BTreeMap<TypeDefId, SchemaIdentity>,
) -> bool {
    let mut equivalence = BoundEquivalence { types: BTreeMap::new(), rows: BTreeMap::new(), effects: BTreeMap::new(), left_schemas, right_schemas };
    if !equivalence.scheme(left_owner, right_owner) { return false; }
    let bound_types = equivalence.types.len();
    let bound_rows = equivalence.rows.len();
    equivalence.ty(left, right) && equivalence.types.len() == bound_types && equivalence.rows.len() == bound_rows
}

impl CheckedCallableContract {
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
        let mut equivalence = BoundEquivalence {
            types: BTreeMap::new(), rows: BTreeMap::new(), effects: BTreeMap::new(),
            left_schemas: &self.schema_identities, right_schemas: &other.schema_identities,
        };
        equivalence.scheme(&self.scheme, &other.scheme)
            && equivalence.effects(self.scheme.effects().contract(), other.scheme.effects().contract())
    }

    /// Tests whether this concrete contract is an exact instance of the given
    /// scheme, preserving labels, layout, schema identities, and effects.
    /// Returns an error when the original is generic or the comparison cannot
    /// establish a resolved instance. An error does not establish a mismatch.
    pub fn is_instance_of(&self, generalized: &Self) -> Result<bool, ContractComparisonError> {
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
        let schemas = BoundEquivalence {
            types: BTreeMap::new(), rows: BTreeMap::new(), effects: BTreeMap::new(),
            left_schemas: &self.schema_identities, right_schemas: &generalized.schema_identities,
        };
        if !schemas.schema(self.scheme.return_schema(), generalized.scheme.return_schema())
            || !self.scheme.params().iter().zip(generalized.scheme.params()).all(|(old, new)| {
                old.name == new.name && old.defaulted == new.defaulted && old.rest == new.rest
                    && schemas.schema(old.schema_expectation.as_ref(), new.schema_expectation.as_ref())
            })
        {
            return Ok(None);
        }
        let ground = self.scheme.params().iter().map(|parameter| &parameter.ty)
            .chain(std::iter::once(self.scheme.return_type()))
            .map(SchemeType::closed_ground_type).collect::<Option<Vec<_>>>()
            .ok_or(ContractComparisonError::UnsupportedContract)?;
        let mut constraints = TypeConstraints::default();
        let origin = Span::at(SourceId::new(0), 0);
        let instance = constraints.instantiate_callable(generalized.scheme.callable(), 1, origin, crate::sema::constraints::CallableApplicationScope::Invocation)
            .map_err(|_| ContractComparisonError::UnsupportedContract)?;
        let instantiated = instance.params.iter().chain(std::iter::once(&instance.return_type));
        for (expected, actual) in instantiated.clone().zip(&ground) {
            constraints.constrain_annotation(expected, actual, origin)
                .map_err(|_| ContractComparisonError::CannotEstablishConstraints)?;
        }
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
        for (expected, actual) in instantiated.zip(&ground) {
            let resolved = constraints.resolve(expected).map_err(|_| ContractComparisonError::UnresolvedInstance)?;
            if &resolved != actual { return Ok(None); }
        }
        Ok(Some(CallableInstanceEvidence { constraints, instance, scheme: Arc::clone(&generalized.scheme), origin }))
    }

    /// Joins effect requirements under the same checked callable binder scope.
    /// An unknown contract remains unknown in the result.
    pub fn join_effects(&self, other: &Self) -> Option<Self> {
        if !self.same_shape(other) { return None; }
        let mut equivalence = BoundEquivalence {
            types: BTreeMap::new(), rows: BTreeMap::new(), effects: BTreeMap::new(),
            left_schemas: &self.schema_identities, right_schemas: &other.schema_identities,
        };
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
        BoundEquivalence {
            types: BTreeMap::new(), rows: BTreeMap::new(), effects: BTreeMap::new(),
            left_schemas: &self.schema_identities, right_schemas: &other.schema_identities,
        }.scheme(&self.scheme, &other.scheme)
    }
}

impl CheckedFunctionDeclaration {
    pub fn callable_contract(&self) -> Option<CheckedCallableContract> {
        Some(CheckedCallableContract {
            scheme: Arc::clone(self.scheme.as_ref()?),
            symbol_owner: self.symbol_owner.clone(),
            schema_identities: Arc::clone(&self.schema_identities),
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
    }

    /// Returns the declaration contract of a checked function value reference.
    /// Invocation spans have no function value reference contract.
    pub fn callable_value_contract_at(&self, span: crate::source::Span) -> Option<CheckedCallableContract> {
        self.selected_function_value_references.contains(&span)
            .then(|| self.callable_contract_at(span)).flatten()
    }
}

fn schema_has_obligations(schema: &SchemaExpectation) -> bool {
    let mut pending = vec![schema];
    while let Some(schema) = pending.pop() {
        if !schema.instances.is_empty() { return true; }
        pending.extend(schema.children.values());
    }
    false
}

struct BoundEquivalence<'a> {
    types: BTreeMap<SchemeTypeParameterId, SchemeTypeParameterId>,
    rows: BTreeMap<SchemeRowParameterId, SchemeRowParameterId>,
    effects: BTreeMap<SchemeEffectParameterId, SchemeEffectParameterId>,
    left_schemas: &'a BTreeMap<TypeDefId, SchemaIdentity>,
    right_schemas: &'a BTreeMap<TypeDefId, SchemaIdentity>,
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
            || !self.schema(left.return_schema(), right.return_schema())
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
            && left.applications.len() == right.applications.len()
            && left.effect_predicates.len() == right.effect_predicates.len()
            && left.applications.iter().zip(&right.applications).all(|(left, right)| {
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
                iteration.source_kind == other.source_kind && self.ty(&iteration.item, &other.item) && match (&iteration.failure, &other.failure) {
                    (crate::sema::schemes::IterationFailure::Infallible, crate::sema::schemes::IterationFailure::Infallible) => true,
                    (crate::sema::schemes::IterationFailure::Propagates(left), crate::sema::schemes::IterationFailure::Propagates(right)) => self.ty(left, right),
                    _ => false,
                }
            })
            && left.effect_predicates.iter().zip(&right.effect_predicates).all(|(left, right)| {
                left.try_map_contracts(|contract| contract.substitute(|id| self.effects.get(id).copied()
                    .map(crate::sema::effects::EffectContract::variable).ok_or(()))).is_ok_and(|left| &left == right)
            })
    }

    fn schema(&self, left: Option<&SchemaExpectation>, right: Option<&SchemaExpectation>) -> bool {
        let left = left.filter(|schema| schema_has_obligations(schema));
        let right = right.filter(|schema| schema_has_obligations(schema));
        match (left, right) {
            (None, None) => true,
            (Some(left), Some(right)) => left.instances.len() == right.instances.len()
                && left.instances.iter().zip(&right.instances).all(|(left, right)| {
                    self.left_schemas.get(&left.definition).is_some_and(|identity| Some(identity) == self.right_schemas.get(&right.definition))
                        && left.arguments == right.arguments
                })
                && left.children.values().filter(|child| schema_has_obligations(child)).count()
                    == right.children.values().filter(|child| schema_has_obligations(child)).count()
                && left.children.iter().filter(|(_, child)| schema_has_obligations(child)).all(|(component, left)| right.children.get(component)
                    .is_some_and(|right| self.schema(Some(left), Some(right)))),
            _ => false,
        }
    }

    fn parameters(&mut self, left: &[crate::sema::schemes::SchemeParameter], right: &[crate::sema::schemes::SchemeParameter]) -> bool {
        left.len() == right.len() && left.iter().zip(right).all(|(left, right)| {
            left.name == right.name && left.defaulted == right.defaulted && left.rest == right.rest
                && self.schema(left.schema_expectation.as_ref(), right.schema_expectation.as_ref()) && self.ty(&left.ty, &right.ty)
        })
    }

    fn monotype(&mut self, left: &CallableMonotype, right: &CallableMonotype) -> bool {
        self.parameters(left.params(), right.params()) && self.ty(left.return_type(), right.return_type())
            && self.schema(left.return_schema(), right.return_schema())
    }

    fn effects(&self, left: &crate::sema::effects::EffectContract<SchemeEffectParameterId>, right: &crate::sema::effects::EffectContract<SchemeEffectParameterId>) -> bool {
        left.substitute(|id| self.effects.get(id).copied()
            .map(crate::sema::effects::EffectContract::variable).ok_or(())).is_ok_and(|left| &left == right)
    }

    fn ty(&mut self, left: &SchemeType, right: &SchemeType) -> bool {
        if let (Some(left), Some(right)) = (left.closed_ground_type(), right.closed_ground_type()) {
            return left == right;
        }
        match (left, right) {
            (SchemeType::Ground(left), SchemeType::Ground(right)) => left == right,
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
    fn inferred_return_schema_retains_actual_constructor_identity() {
        let declarations = "type Disk = { mount: Str, used: Int }\ntype Other = { mount: Str, used: Int }\n";
        let original = root_schema_contract(&format!("{declarations}proc describe(mount: Str) -> Result[Disk] {{ Disk(mount: mount, used: mount.byte_len()) }}\n"));
        let inferred = root_schema_contract(&format!("{declarations}proc describe(mount: Str) {{ Disk(mount: mount, used: mount.byte_len()) }}\n"));
        assert!(original.equivalent_to(&inferred));
        for body in ["Other(mount: mount, used: mount.byte_len())", "{ mount: mount, used: mount.byte_len() }"] {
            let changed = root_schema_contract(&format!("{declarations}proc describe(mount: Str) {{ {body} }}\n"));
            assert!(!original.equivalent_to(&changed), "{body}");
        }
    }

    #[test]
    fn inferred_return_schema_follows_aliases_and_every_completing_return() {
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
            assert!(!original.equivalent_to(&inferred), "{body}");
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
            symbol_owner: owner.clone(), schema_identities: Default::default(),
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
