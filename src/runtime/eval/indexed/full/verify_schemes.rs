mod producers;
pub(super) use producers::ProducerContexts;
pub(super) use producers::GroundProducerIndex;

use super::schema::{self, FullSchemeTypeTag, FullSchemeValidationTag};
use super::schemes::{
    FullCallableContract, FullScheme, FullSchemeBodyFactKind, FullSchemeId,
    FullSchemeRowParameter, FullSchemeTypeParameter, FullTypeArgument, SchemeTypeId,
};
use super::{
    driver_owner, FullDecoder, FullFunction, FullStore, FullTag, IrBlockId, IrFunctionId, IrRange,
    IrVerifyError, TypeId, IR_NONE,
};
use super::super::semantic::TypeMaterializationBudget;
use crate::sema::types::{Type, merge_list_literal_item_ty};
use crate::sema::schemes::{SchemeRowParameterId, SchemeRowTail, SchemeType, SchemeTypeParameterId};
use crate::sema::validated::{IntRange, ValidatedType, Validation};
use crate::symbol::Name;
use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

const SCHEME_TYPE_WORK_PER_NODE: usize = 8;
const SCHEME_TYPE_WORK_BASE: usize = 128;

struct SchemeScope {
    type_parameters: Vec<FullSchemeTypeParameter>,
    row_parameters: Vec<FullSchemeRowParameter>,
    type_binders: BTreeMap<u32, usize>,
    row_binders: BTreeMap<u32, usize>,
    row_lacks: Vec<BTreeSet<Name>>,
}

struct CallSubstitution {
    types: Vec<Type>,
    rows: Vec<BTreeMap<Name, Type>>,
}

#[derive(Clone)]
struct SchemeRowSubstitution {
    fields: BTreeMap<Name, SchemeType>,
    tail: SchemeRowTail,
}

struct SymbolicCallSubstitution {
    types: Vec<SchemeType>,
    rows: Vec<SchemeRowSubstitution>,
}

enum SchemeTypeExpectation {
    Encoded(SchemeTypeId),
    Ground(Type),
}

enum CheckedArgumentType {
    Known(Type),
    ListItems(Vec<Type>),
    EmptyList(Option<Type>),
    EmptyMap,
}

#[derive(Clone, Default)]
struct SchemeBinderEvidence {
    types: BTreeSet<usize>,
    rows: BTreeSet<usize>,
}

pub(super) struct SchemeInstantiationBudget {
    remaining: usize,
    semantic_types: TypeMaterializationBudget,
}

impl SchemeInstantiationBudget {
    fn new(store: &FullStore) -> Self {
        Self {
            remaining: scheme_type_work_limit(store)
                .saturating_add(store.tags.len().saturating_mul(8))
                .saturating_add(store.extra.len()),
            semantic_types: TypeMaterializationBudget::default(),
        }
    }

    pub(super) fn for_prepared_constants(store: &FullStore, verified_work: usize) -> Self {
        let mut budget = Self::new(store);
        budget.remaining = budget.remaining.saturating_add(verified_work.saturating_mul(SCHEME_TYPE_WORK_PER_NODE));
        budget
    }

    pub(super) fn charge(&mut self, amount: usize) -> Result<(), IrVerifyError> {
        self.remaining = self
            .remaining
            .checked_sub(amount)
            .ok_or_else(|| IrVerifyError::new("generic call type materialization exceeds its work bound"))?;
        Ok(())
    }

    fn charge_clone(&mut self, ty: &Type) -> Result<(), IrVerifyError> {
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            self.charge(1)?;
            match ty {
                Type::List(inner)
                | Type::Stream(inner)
                | Type::Set(inner)
                | Type::Optional(inner) => pending.push(inner),
                Type::Map(key, value) | Type::Result(key, value) => {
                    pending.push(key);
                    pending.push(value);
                }
                Type::Union(members) => pending.extend(members),
                Type::Validated(validated) => pending.push(validated.base()),
                _ => {}
            }
        }
        Ok(())
    }

    fn charge_scheme_clone(&mut self, ty: &SchemeType) -> Result<(), IrVerifyError> {
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            self.charge(1)?;
            match ty {
                SchemeType::Ground(ty) => self.charge_clone(ty)?,
                SchemeType::Parameter(_) => {}
                SchemeType::List(inner)
                | SchemeType::Stream(inner)
                | SchemeType::Set(inner)
                | SchemeType::Optional(inner)
                | SchemeType::Validated(_, inner) => pending.push(inner),
                SchemeType::Map(key, value) | SchemeType::Result(key, value) => {
                    pending.push(key);
                    pending.push(value);
                }
                SchemeType::Record { fields, .. } => pending.extend(fields.values()),
                SchemeType::Union(members) => pending.extend(members),
            }
        }
        Ok(())
    }
}

struct SchemaCoverage {
    expectations: Vec<bool>,
    instances: Vec<bool>,
    arguments: Vec<bool>,
    children: Vec<bool>,
}

pub(super) fn verify(store: &FullStore, producer_index: &GroundProducerIndex, contexts: producers::ProducerContexts<'_>) -> Result<Vec<(u32, super::ExecutableOwner, Arc<Type>)>, IrVerifyError> {
    let scopes = verify_function_schemes(store)?;
    verify_scheme_tables(store, &scopes)?;
    verify_scheme_body_facts(store, &scopes)?;
    verify_call_instantiations(store, &scopes)?;
    producers::verify(store, producer_index, contexts)
}

fn verify_function_schemes(store: &FullStore) -> Result<Vec<Option<SchemeScope>>, IrVerifyError> {
    let mut scheme_owners = vec![None; store.schemes.len()];
    let mut scopes = (0..store.schemes.len()).map(|_| None).collect::<Vec<_>>();
    let mut type_parameter_coverage = vec![false; store.scheme_type_parameters.len()];
    let mut row_parameter_coverage = vec![false; store.scheme_row_parameters.len()];
    let mut row_lacks_coverage = vec![false; store.scheme_row_lacks.len()];
    let mut parameter_coverage = vec![false; store.scheme_params.len()];
    let mut effect_coverage = vec![false; store.scheme_effects.len()];
    let mut body_fact_coverage = vec![false; store.scheme_body_facts.len()];

    for (function_index, function) in store.functions.iter().enumerate() {
        let FullCallableContract::Scheme(scheme_id) = function.contract else {
            continue;
        };
        let scheme_index = scheme_id.index();
        let Some(owner) = scheme_owners.get_mut(scheme_index) else {
            return Err(IrVerifyError::new("full function scheme id is out of bounds"));
        };
        if owner.replace(function_index).is_some() {
            return Err(IrVerifyError::new("full scheme belongs to multiple functions"));
        }
        let scheme = store.scheme(scheme_id)?;
        if scheme.reserved != [0; 3]
            || scheme.callable_kind != super::schemes::FullSchemeCallableKind::Pure
            || store.function_metadata[function_index].flags & 1 != 0
            || scheme.inferred_effects
        {
            return Err(IrVerifyError::new("generic function contract is invalid"));
        }
        let type_parameters = mark_range(
            scheme.type_parameters,
            &store.scheme_type_parameters,
            &mut type_parameter_coverage,
            "scheme type parameter range is invalid or overlaps",
        )?
        .iter()
        .copied()
        .collect::<Vec<_>>();
        let row_parameters = mark_range(
            scheme.row_parameters,
            &store.scheme_row_parameters,
            &mut row_parameter_coverage,
            "scheme row parameter range is invalid or overlaps",
        )?
        .iter()
        .copied()
        .collect::<Vec<_>>();
        for parameter in &row_parameters {
            mark_range(
                parameter.lacks,
                &store.scheme_row_lacks,
                &mut row_lacks_coverage,
                "scheme row lacks range is invalid or overlaps",
            )?;
        }
        let params = mark_range(
            scheme.params,
            &store.scheme_params,
            &mut parameter_coverage,
            "scheme parameter range is invalid or overlaps",
        )?
        .iter()
        .copied()
        .collect::<Vec<_>>();
        let _effects = mark_range(
            scheme.effects,
            &store.scheme_effects,
            &mut effect_coverage,
            "scheme effect range is invalid or overlaps",
        )?;
        let _facts = mark_range(
            scheme.body_facts,
            &store.scheme_body_facts,
            &mut body_fact_coverage,
            "scheme body fact range is invalid or overlaps",
        )?;
        if !scheme.has_effects && scheme.effects.len != 0 {
            return Err(IrVerifyError::new("generic scheme has effects without an effect clause"));
        }
        let function_params = bounded(function.params, &store.params, "function parameter range is invalid")?;
        if params.len() != function_params.len() {
            return Err(IrVerifyError::new("generic scheme parameters do not match the function"));
        }
        for (index, (function_param, scheme_param)) in
            function_params.iter().zip(&params).enumerate()
        {
            if function_param.reserved != [0; 3] {
                return Err(IrVerifyError::new("generic function parameter metadata is invalid"));
            }
            if function_param.flags & 1 != 0 {
                if index + 1 != function_params.len() {
                    return Err(IrVerifyError::new("generic rest parameter is not final"));
                }
                verify_scheme_list_shape(store, scheme_param.ty)?;
            }
        }
        let scope = make_scope(store, type_parameters, row_parameters)?;
        if function.checked_return_type != IR_NONE {
            return Err(IrVerifyError::new(
                "generic function duplicates its scheme return type",
            ));
        }
        scopes[scheme_index] = Some(scope);
    }

    if scheme_owners.iter().any(Option::is_none)
        || type_parameter_coverage.iter().any(|covered| !covered)
        || row_parameter_coverage.iter().any(|covered| !covered)
        || row_lacks_coverage.iter().any(|covered| !covered)
        || parameter_coverage.iter().any(|covered| !covered)
        || effect_coverage.iter().any(|covered| !covered)
        || body_fact_coverage.iter().any(|covered| !covered)
    {
        return Err(IrVerifyError::new("full scheme tables contain unowned rows"));
    }
    Ok(scopes)
}

fn make_scope(
    store: &FullStore,
    type_parameters: Vec<FullSchemeTypeParameter>,
    row_parameters: Vec<FullSchemeRowParameter>,
) -> Result<SchemeScope, IrVerifyError> {
    let mut type_binders = BTreeMap::new();
    for (index, parameter) in type_parameters.iter().enumerate() {
        if parameter.id == IR_NONE
            || parameter.binder as usize != index
            || type_binders.insert(parameter.id, index).is_some()
        {
            return Err(IrVerifyError::new("scheme type binders are duplicated or out of scope"));
        }
    }
    let mut row_binders = BTreeMap::new();
    let mut row_lacks = Vec::with_capacity(row_parameters.len());
    for (index, parameter) in row_parameters.iter().enumerate() {
        if parameter.id == IR_NONE
            || parameter.binder as usize != index
            || row_binders.insert(parameter.id, index).is_some()
        {
            return Err(IrVerifyError::new("scheme row binders are duplicated or out of scope"));
        }
        let mut lacks = BTreeSet::new();
        let mut previous = None;
        for &raw in bounded(
            parameter.lacks,
            &store.scheme_row_lacks,
            "scheme row lacks range is invalid",
        )? {
            let text = store.string(raw)?;
            if text.is_empty() || previous.is_some_and(|previous: &str| previous >= text) {
                return Err(IrVerifyError::new("scheme row lacks labels are not sorted unique"));
            }
            previous = Some(text);
            if !lacks.insert(Name::intern(text)) {
                return Err(IrVerifyError::new("scheme row lacks labels are duplicated"));
            }
        }
        row_lacks.push(lacks);
    }
    Ok(SchemeScope {
        type_parameters,
        row_parameters,
        type_binders,
        row_binders,
        row_lacks,
    })
}

fn verify_scheme_tables(
    store: &FullStore,
    scopes: &[Option<SchemeScope>],
) -> Result<(), IrVerifyError> {
    let mut type_owners = vec![None; store.scheme_types.len()];
    let mut type_states = vec![0u8; store.scheme_types.len()];
    let mut field_coverage = vec![false; store.scheme_fields.len()];
    let mut child_coverage = vec![false; store.scheme_children.len()];
    let mut validation_coverage = vec![false; store.scheme_validations.len()];
    let mut schema_coverage = SchemaCoverage {
        expectations: vec![false; store.scheme_schema_expectations.len()],
        instances: vec![false; store.scheme_schema_instances.len()],
        arguments: vec![false; store.scheme_schema_arguments.len()],
        children: vec![false; store.scheme_schema_children.len()],
    };
    let mut type_binder_uses = scopes
        .iter()
        .map(|scope| scope.as_ref().map_or(Vec::new(), |scope| vec![false; scope.type_parameters.len()]))
        .collect::<Vec<_>>();
    let mut row_binder_uses = scopes
        .iter()
        .map(|scope| scope.as_ref().map_or(Vec::new(), |scope| vec![false; scope.row_parameters.len()]))
        .collect::<Vec<_>>();
    let mut validation_references = vec![false; store.scheme_validations.len()];

    for (scheme_index, scope) in scopes.iter().enumerate() {
        let Some(scope) = scope else {
            continue;
        };
        let scheme = store
            .schemes
            .get(scheme_index)
            .ok_or_else(|| IrVerifyError::new("full scheme id is out of bounds"))?;
        let mut roots = Vec::new();
        let params = bounded(scheme.params, &store.scheme_params, "scheme parameter range is invalid")?;
        roots.extend(params.iter().map(|param| param.ty));
        roots.push(scheme.return_type);
        for fact in bounded(
            scheme.body_facts,
            &store.scheme_body_facts,
            "scheme body fact range is invalid",
        )? {
            roots.push(fact.ty);
        }
        for root in roots {
            visit_scheme_type(
                store,
                scheme_index,
                root,
                scope,
                &mut type_owners,
                &mut type_states,
                &mut field_coverage,
                &mut child_coverage,
                &mut validation_coverage,
                &mut validation_references,
                &mut type_binder_uses,
                &mut row_binder_uses,
                0,
            )?;
        }
        if type_binder_uses[scheme_index].iter().any(|used| !used)
            || row_binder_uses[scheme_index].iter().any(|used| !used)
        {
            return Err(IrVerifyError::new("generic scheme contains an unused binder"));
        }
        let mut effects = Vec::new();
        for effect in bounded(scheme.effects, &store.scheme_effects, "scheme effect range is invalid")? {
            if effects.contains(effect) {
                return Err(IrVerifyError::new("generic scheme effects are duplicated"));
            }
            effects.push(effect.clone());
        }
        for param in params {
            super::scheme_type_physical_kind(store, param.ty)?;
            if let Some(schema) = param.schema {
                verify_schema_expectation(
                    store,
                    schema,
                    &mut schema_coverage,
                    &mut BTreeSet::new(),
                    0,
                )?;
            }
        }
        super::scheme_return_kind(store, scheme.return_type)?;
        if let Some(schema) = scheme.return_schema {
            verify_schema_expectation(
                store,
                schema,
                &mut schema_coverage,
                &mut BTreeSet::new(),
                0,
            )?;
        }
    }
    let empty_scope = SchemeScope {
        type_parameters: Vec::new(),
        row_parameters: Vec::new(),
        type_binders: BTreeMap::new(),
        row_binders: BTreeMap::new(),
        row_lacks: Vec::new(),
    };
    for instantiation in &store.call_instantiations {
        let caller = caller_scheme_for_instruction(
            store,
            instantiation.instruction as usize,
            scopes,
        )?;
        let (owner, scope) = caller.map_or((scopes.len(), &empty_scope), |(id, scope)| {
            (id.index(), scope)
        });
        let type_rows = bounded(
            instantiation.type_arguments,
            &store.call_type_arguments,
            "generic call type argument range is invalid",
        )?;
        let row_rows = bounded(
            instantiation.row_arguments,
            &store.call_row_arguments,
            "generic call row argument range is invalid",
        )?;
        for argument in type_rows {
            visit_scheme_type(
                store,
                owner,
                argument.ty,
                scope,
                &mut type_owners,
                &mut type_states,
                &mut field_coverage,
                &mut child_coverage,
                &mut validation_coverage,
                &mut validation_references,
                &mut type_binder_uses,
                &mut row_binder_uses,
                0,
            )?;
        }
        for row in row_rows {
            for field in bounded(
                row.fields,
                &store.call_row_fields,
                "generic call row evidence range is invalid",
            )? {
                visit_scheme_type(
                    store,
                    owner,
                    field.ty,
                    scope,
                    &mut type_owners,
                    &mut type_states,
                    &mut field_coverage,
                    &mut child_coverage,
                    &mut validation_coverage,
                    &mut validation_references,
                    &mut type_binder_uses,
                    &mut row_binder_uses,
                    0,
                )?;
            }
        }
    }
    if type_owners.iter().any(Option::is_none)
        || type_states.iter().any(|state| *state != 2)
        || field_coverage.iter().any(|covered| !covered)
        || child_coverage.iter().any(|covered| !covered)
        || validation_coverage.iter().any(|covered| !covered)
        || validation_references.iter().any(|referenced| !referenced)
        || schema_coverage.expectations.iter().any(|covered| !covered)
        || schema_coverage.instances.iter().any(|covered| !covered)
        || schema_coverage.arguments.iter().any(|covered| !covered)
        || schema_coverage.children.iter().any(|covered| !covered)
    {
        return Err(IrVerifyError::new("scheme type tables contain unowned rows"));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn visit_scheme_type(
    store: &FullStore,
    scheme_index: usize,
    id: SchemeTypeId,
    scope: &SchemeScope,
    type_owners: &mut [Option<usize>],
    type_states: &mut [u8],
    field_coverage: &mut [bool],
    child_coverage: &mut [bool],
    validation_coverage: &mut [bool],
    validation_references: &mut [bool],
    type_binder_uses: &mut [Vec<bool>],
    row_binder_uses: &mut [Vec<bool>],
    depth: usize,
) -> Result<(), IrVerifyError> {
    if depth > 128 {
        return Err(IrVerifyError::new("scheme type nesting is too deep"));
    }
    let index = id.index();
    let Some(owner) = type_owners.get_mut(index) else {
        return Err(IrVerifyError::new("scheme type id is out of bounds"));
    };
    if owner.is_some_and(|owner| owner != scheme_index) {
        return Err(IrVerifyError::new("scheme type node escaped its binder scope"));
    }
    *owner = Some(scheme_index);
    match type_states[index] {
        1 => return Err(IrVerifyError::new("scheme type graph contains a cycle")),
        2 => return Ok(()),
        _ => {}
    }
    type_states[index] = 1;
    let node = *store.scheme_type(id)?;
    if node.reserved != [0; 3] {
        return Err(IrVerifyError::new("scheme type node has reserved bits set"));
    }
    match node.tag {
        FullSchemeTypeTag::Ground => {
            require_empty_children(node.children)?;
            if node.second != 0 {
                return Err(IrVerifyError::new("ground scheme type has payload bits set"));
            }
            let raw = TypeId::from_raw(node.first)
                .ok_or_else(|| IrVerifyError::new("ground scheme type id is invalid"))?;
            ensure_closed_type(&store.semantic.to_type(raw)?)?;
        }
        FullSchemeTypeTag::Parameter => {
            require_empty_children(node.children)?;
            if node.second != 0 {
                return Err(IrVerifyError::new("scheme type parameter has payload bits set"));
            }
            let binder = node.first as usize;
            if binder >= scope.type_parameters.len() {
                return Err(IrVerifyError::new("scheme type parameter is out of scope"));
            }
            type_binder_uses[scheme_index][binder] = true;
        }
        FullSchemeTypeTag::List
        | FullSchemeTypeTag::Stream
        | FullSchemeTypeTag::Set
        | FullSchemeTypeTag::Optional => {
            require_empty_children(node.children)?;
            if node.second != 0 {
                return Err(IrVerifyError::new("unary scheme type has payload bits set"));
            }
            visit_scheme_type(
                store,
                scheme_index,
                SchemeTypeId::from_raw(node.first)?,
                scope,
                type_owners,
                type_states,
                field_coverage,
                child_coverage,
                validation_coverage,
                validation_references,
                type_binder_uses,
                row_binder_uses,
                depth + 1,
            )?;
        }
        FullSchemeTypeTag::Map | FullSchemeTypeTag::Result => {
            require_empty_children(node.children)?;
            visit_scheme_type(
                store,
                scheme_index,
                SchemeTypeId::from_raw(node.first)?,
                scope,
                type_owners,
                type_states,
                field_coverage,
                child_coverage,
                validation_coverage,
                validation_references,
                type_binder_uses,
                row_binder_uses,
                depth + 1,
            )?;
            visit_scheme_type(
                store,
                scheme_index,
                SchemeTypeId::from_raw(node.second)?,
                scope,
                type_owners,
                type_states,
                field_coverage,
                child_coverage,
                validation_coverage,
                validation_references,
                type_binder_uses,
                row_binder_uses,
                depth + 1,
            )?;
        }
        FullSchemeTypeTag::Record => {
            if node.second != 0 {
                return Err(IrVerifyError::new("scheme record has payload bits set"));
            }
            let fields = mark_range(
                node.children,
                &store.scheme_fields,
                field_coverage,
                "scheme record field range is invalid or overlaps",
            )?;
            let mut names = BTreeSet::new();
            let mut previous = None;
            for field in fields {
                let text = store.string(field.name)?;
                let name = Name::intern(text);
                if text.is_empty() || !names.insert(name) {
                    return Err(IrVerifyError::new("scheme record fields are duplicated or empty"));
                }
                if previous.is_some_and(|previous: Name| previous >= name) {
                    return Err(IrVerifyError::new("scheme record fields are not sorted"));
                }
                previous = Some(name);
                visit_scheme_type(
                    store,
                    scheme_index,
                    field.ty,
                    scope,
                    type_owners,
                    type_states,
                    field_coverage,
                    child_coverage,
                    validation_coverage,
                    validation_references,
                    type_binder_uses,
                    row_binder_uses,
                    depth + 1,
                )?;
            }
            if node.first != IR_NONE {
                let binder = node.first as usize;
                if binder >= scope.row_parameters.len() {
                    return Err(IrVerifyError::new("scheme row parameter is out of scope"));
                }
                row_binder_uses[scheme_index][binder] = true;
                if names.iter().any(|name| !scope.row_lacks[binder].contains(name)) {
                    return Err(IrVerifyError::new("scheme row lacks omit a required field"));
                }
            }
        }
        FullSchemeTypeTag::Union => {
            if node.first != 0 || node.second != 0 {
                return Err(IrVerifyError::new("scheme union has payload bits set"));
            }
            let children = mark_range(
                node.children,
                &store.scheme_children,
                child_coverage,
                "scheme union child range is invalid or overlaps",
            )?;
            if children.len() < 2 {
                return Err(IrVerifyError::new("scheme union has too few members"));
            }
            for child in children {
                visit_scheme_type(
                    store,
                    scheme_index,
                    *child,
                    scope,
                    type_owners,
                    type_states,
                    field_coverage,
                    child_coverage,
                    validation_coverage,
                    validation_references,
                    type_binder_uses,
                    row_binder_uses,
                    depth + 1,
                )?;
            }
        }
        FullSchemeTypeTag::Validated => {
            require_empty_children(node.children)?;
            let validation_index = node.second as usize;
            let validation = store
                .scheme_validations
                .get(validation_index)
                .ok_or_else(|| IrVerifyError::new("scheme validation id is out of bounds"))?;
            verify_validation_row(store, *validation)?;
            validation_coverage[validation_index] = true;
            validation_references[validation_index] = true;
            visit_scheme_type(
                store,
                scheme_index,
                SchemeTypeId::from_raw(node.first)?,
                scope,
                type_owners,
                type_states,
                field_coverage,
                child_coverage,
                validation_coverage,
                validation_references,
                type_binder_uses,
                row_binder_uses,
                depth + 1,
            )?;
        }
    }
    type_states[index] = 2;
    Ok(())
}

fn verify_validation_row(
    store: &FullStore,
    row: super::schemes::FullSchemeValidation,
) -> Result<Validation, IrVerifyError> {
    if row.reserved != [0; 7] {
        return Err(IrVerifyError::new("scheme validation has reserved bits set"));
    }
    match row.tag {
        FullSchemeValidationTag::NonEmpty if row.name == IR_NONE && row.low == 0 && row.high == 0 => {
            Ok(Validation::NonEmpty)
        }
        FullSchemeValidationTag::RelPath if row.name == IR_NONE && row.low == 0 && row.high == 0 => {
            Ok(Validation::RelPath)
        }
        FullSchemeValidationTag::Nominal if row.low == 0 && row.high == 0 => {
            let name = store.string(row.name)?;
            if name.is_empty() {
                return Err(IrVerifyError::new("scheme nominal validation has an empty name"));
            }
            Ok(Validation::Nominal(Name::intern(name)))
        }
        FullSchemeValidationTag::Range if row.name == IR_NONE => Ok(Validation::Range(
            IntRange::new(row.low, row.high)
                .ok_or_else(|| IrVerifyError::new("scheme validation range is empty"))?,
        )),
        _ => Err(IrVerifyError::new("scheme validation payload is invalid")),
    }
}

fn verify_schema_expectation(
    store: &FullStore,
    id: super::schemes::FullSchemeSchemaExpectationId,
    coverage: &mut SchemaCoverage,
    active: &mut BTreeSet<usize>,
    depth: usize,
) -> Result<(), IrVerifyError> {
    if depth > 128 || !active.insert(id.index()) {
        return Err(IrVerifyError::new("scheme schema expectation graph is cyclic"));
    }
    let Some(expectation_covered) = coverage.expectations.get_mut(id.index()) else {
        return Err(IrVerifyError::new("scheme schema expectation id is out of bounds"));
    };
    if *expectation_covered {
        return Err(IrVerifyError::new("scheme schema expectation is shared by multiple owners"));
    }
    *expectation_covered = true;
    let expectation = store
        .scheme_schema_expectations
        .get(id.index())
        .ok_or_else(|| IrVerifyError::new("scheme schema expectation id is out of bounds"))?;
    let instances = mark_range(
        expectation.instances,
        &store.scheme_schema_instances,
        &mut coverage.instances,
        "scheme schema instance range is invalid",
    )?;
    for type_id in instances {
        if type_id.definition == IR_NONE {
            return Err(IrVerifyError::new("scheme schema definition id is invalid"));
        }
        let arguments = mark_range(
            type_id.arguments,
            &store.scheme_schema_arguments,
            &mut coverage.arguments,
            "scheme schema argument range is invalid",
        )?;
        for &argument in arguments {
            ensure_closed_type(&store.semantic.to_type(argument)?)?;
        }
    }
    let children = mark_range(
        expectation.children,
        &store.scheme_schema_children,
        &mut coverage.children,
        "scheme schema child range is invalid",
    )?;
    let mut components = BTreeSet::new();
    let mut previous = None;
    for child in children {
        if child.reserved != [0; 3] {
            return Err(IrVerifyError::new("scheme schema child has reserved bits set"));
        }
        let name = match child.component {
            super::schema::FullSchemeSchemaComponentTag::Field => {
                let text = store.string(child.name)?;
                if text.is_empty() {
                    return Err(IrVerifyError::new("scheme schema field is empty"));
                }
                Some(Name::intern(text))
            }
            _ if child.name == IR_NONE => None,
            _ => return Err(IrVerifyError::new("scheme schema child name is invalid")),
        };
        let component = (child.component as u8, name);
        if !components.insert(component)
            || previous.is_some_and(|previous| previous >= component)
        {
            return Err(IrVerifyError::new("scheme schema children are duplicated or unsorted"));
        }
        previous = Some(component);
        verify_schema_expectation(store, child.expectation, coverage, active, depth + 1)?;
    }
    active.remove(&id.index());
    Ok(())
}

fn verify_scheme_body_facts(
    store: &FullStore,
    scopes: &[Option<SchemeScope>],
) -> Result<(), IrVerifyError> {
    let mut covered = vec![false; store.scheme_body_facts.len()];
    let mut registered_coverage = BTreeSet::new();
    let mut owners = vec![None; scopes.len()];
    for (index, function) in store.functions.iter().enumerate() {
        if let FullCallableContract::Scheme(scheme) = function.contract {
            let owner = owners.get_mut(scheme.index()).ok_or_else(|| IrVerifyError::new("generic scheme owner is out of bounds"))?;
            if owner.replace(index).is_some() { return Err(IrVerifyError::new("generic scheme has multiple function owners")); }
        }
    }
    for (scheme_index, scope) in scopes.iter().enumerate() {
        let Some(scope) = scope else { continue };
        let scheme_id = FullSchemeId::from_raw(u32::try_from(scheme_index).map_err(|_| IrVerifyError::new("full scheme id is out of bounds"))?)?;
        let scheme = store.scheme(scheme_id)?;
        let function_index = owners[scheme_index].ok_or_else(|| IrVerifyError::new("generic scheme has no function owner"))?;
        let function_id = IrFunctionId::new(function_index).map_err(|_| IrVerifyError::new("generic function id is invalid"))?;
        let function = store.functions[function_index];
        let instruction_range = store.function_instruction_range(function_index)?;
        let facts_range = scheme.body_facts.bounds(store.scheme_body_facts.len()).ok_or_else(|| IrVerifyError::new("scheme body fact range is invalid"))?;
        let mut budget = SchemeInstantiationBudget::new(store);
        let mut types = BTreeMap::new();
        let mut tails = BTreeMap::new();
        for (offset, fact) in store.scheme_body_facts[facts_range.clone()].iter().enumerate() {
            covered[facts_range.start + offset] = true;
            let instruction = fact.instruction as usize;
            if fact.function != function_id || !instruction_range.contains(&instruction) || fact.reserved != [0; 3] { return Err(IrVerifyError::new("generic body fact has invalid ownership")); }
            let ty = decode_scheme_type(store, scope, fact.ty, &mut budget, 0)?;
            let target = match fact.kind {
                FullSchemeBodyFactKind::Expression if store.tags[instruction].kind() == schema::InstructionKind::Expr => &mut types,
                FullSchemeBodyFactKind::Tail if store.tags[instruction] == FullTag::StmtReturn => &mut tails,
                _ => return Err(IrVerifyError::new("generic body fact has an invalid instruction kind")),
            };
            if target.insert(instruction, ty).is_some() { return Err(IrVerifyError::new("generic body fact is duplicated")); }
        }
        let decoder = decoder_for_function(store, function_index)?;
        let mut slots = BTreeMap::new();
        for (slot, parameter) in bounded(scheme.params, &store.scheme_params, "scheme parameter range is invalid")?.iter().enumerate() {
            slots.insert(slot, decode_scheme_type(store, scope, parameter.ty, &mut budget, 0)?);
        }
        for capture in bounded(function.captures, &store.captures, "generic capture range is invalid")? {
            let slot = (capture.slot_and_flags & !(1 << 31)) as usize;
            if capture.slot_and_flags & (1 << 31) != 0 { return Err(IrVerifyError::new("generic body captures mutable state")); }
            let ty = store.semantic.to_type_with_budget(capture.type_id, &mut budget.semantic_types)?;
            ensure_closed_type(&ty)?;
            if ty.may_hold_resource() { return Err(IrVerifyError::new("generic body captures resource state")); }
            let actual = decoder.checked_slot_type(slot)?.ok_or_else(|| IrVerifyError::new("generic capture has no authenticated lexical writer type"))?;
            if store.semantic.to_type_with_budget(actual, &mut budget.semantic_types)? != ty { return Err(IrVerifyError::new("generic capture type disagrees with its lexical writer")); }
            if slots.insert(slot, SchemeType::Ground(ty)).is_some() { return Err(IrVerifyError::new("generic capture shares a parameter slot")); }
        }
        let body_id = IrBlockId::from_raw(function.body).ok_or_else(|| IrVerifyError::new("generic body block is invalid"))?;
        let block = store.blocks.get(body_id.index()).ok_or_else(|| IrVerifyError::new("generic body block is out of bounds"))?;
        let statements = store.payload(block.instructions)?;
        let mut visited = BTreeSet::new();
        let mut returned = false;
        let expected_return = decode_scheme_type(store, scope, scheme.return_type, &mut budget, 0)?;
        for (position, &raw) in statements.iter().skip(1).enumerate() {
            let instruction = raw as usize;
            match decoder.statement_view(instruction)? {
                schema::StmtView::Let { slot, value, annotation } if !returned => {
                    let ty = verify_body_expression(store, &decoder, value.raw() as usize, &types, &slots, &mut visited, &mut registered_coverage, &mut budget)?;
                    let stored = if let Some(annotation) = annotation {
                        let annotation = annotation.materialize()?;
                        if !body_type_matches_ground(&ty, &annotation) || annotation.has_unsigned_constraint() && ty.closed_ground_type() != Some(annotation.clone()) { return Err(IrVerifyError::new("generic local storage disagrees with its checked type")); }
                        SchemeType::Ground(annotation)
                    } else { ty };
                    if slot >= function.slot_count as usize || slots.insert(slot, stored).is_some() { return Err(IrVerifyError::new("generic body slot has multiple writers or is out of bounds")); }
                }
                schema::StmtView::Return { value } if !returned && position + 2 == statements.len() => {
                    let ty = verify_body_expression(store, &decoder, value.raw() as usize, &types, &slots, &mut visited, &mut registered_coverage, &mut budget)?;
                    if !ty.matches_body_return(&expected_return) { return Err(IrVerifyError::new("generic body return does not match its scheme return")); }
                    if tails.remove(&instruction).is_some_and(|tail| !tail.matches_body_return(&expected_return)) { return Err(IrVerifyError::new("generic body tail does not match its scheme return")); }
                    returned = true;
                }
                _ => return Err(IrVerifyError::new("generic body is outside immutable sequencing and terminal return")),
            }
        }
        let slot_facts = bounded(function.slot_type_facts, &store.slot_type_facts, "generic slot type fact range is invalid")?;
        for (slot, &raw) in slot_facts.iter().enumerate().filter(|(_, raw)| **raw != IR_NONE) {
            let ty = TypeId::from_raw(raw).ok_or_else(|| IrVerifyError::new("generic slot type fact is invalid"))?;
            let ground = store.semantic.to_type_with_budget(ty, &mut budget.semantic_types)?;
            let actual = slots.get(&slot).ok_or_else(|| IrVerifyError::new("generic slot type fact has no initialized writer"))?;
            let equal = match actual {
                SchemeType::Ground(actual) => actual == &ground,
                actual => actual.closed_ground_type().is_some_and(|actual| actual == ground),
            };
            if !equal { return Err(IrVerifyError::new("generic slot type fact disagrees with its initialized value")); }
        }
        if !returned || !tails.is_empty() || visited.len() != types.len() { return Err(IrVerifyError::new("generic body contains an unchecked or escaped expression")); }
        let encoded = instruction_range.filter(|instruction| store.tags[*instruction].kind() == schema::InstructionKind::Expr).collect::<BTreeSet<_>>();
        if visited != encoded { return Err(IrVerifyError::new("generic body contains an unchecked expression")); }
    }
    if covered.iter().any(|covered| !covered) || store.registered_calls.keys().any(|instruction| !registered_coverage.contains(instruction)) { return Err(IrVerifyError::new("generic body metadata contains unowned rows")); }
    Ok(())
}

fn body_type_matches(actual: &SchemeType, expected: &SchemeType) -> bool {
    actual == expected || matches!((actual.closed_ground_type(), expected.closed_ground_type()), (Some(actual), Some(expected)) if actual.matches_expected(&expected))
}

fn body_type_matches_ground(actual: &SchemeType, expected: &Type) -> bool {
    if let Some(actual) = actual.closed_ground_type() { return actual.matches_expected(expected); }
    match (actual, expected) {
        (SchemeType::Record { fields, .. }, Type::Record(expected)) => expected.iter().all(|(name, expected)| fields.get(name).is_some_and(|actual| body_type_matches_ground(actual, expected))),
        _ => false,
    }
}

fn verify_body_expression(
    store: &FullStore,
    decoder: &FullDecoder<'_>,
    instruction: usize,
    types: &BTreeMap<usize, SchemeType>,
    slots: &BTreeMap<usize, SchemeType>,
    visited: &mut BTreeSet<usize>,
    registered_coverage: &mut BTreeSet<u32>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<SchemeType, IrVerifyError> {
    budget.charge(1)?;
    if !visited.insert(instruction) { return Err(IrVerifyError::new("generic body expression is shared or cyclic")); }
    let ty = types.get(&instruction).ok_or_else(|| IrVerifyError::new("generic expression has no checked body fact"))?;
    let mut child = |value: super::decode::ExprInstructionId<'_>| verify_body_expression(store, decoder, value.raw() as usize, types, slots, visited, registered_coverage, budget);
    let expected = match decoder.expression_view(instruction)? {
        schema::ExprView::Param(slot) => slots.get(&slot).cloned().ok_or_else(|| IrVerifyError::new("generic body reads an uninitialized slot"))?,
        schema::ExprView::Null => SchemeType::Ground(Type::Null),
        schema::ExprView::Unit => SchemeType::Ground(Type::Unit),
        schema::ExprView::Bool(_) => SchemeType::Ground(Type::Bool),
        schema::ExprView::Int(_) => SchemeType::Ground(Type::Int),
        schema::ExprView::UInt(_) => SchemeType::Ground(Type::UInt),
        schema::ExprView::Float(_) => SchemeType::Ground(Type::Float),
        schema::ExprView::Str(_) => SchemeType::Ground(Type::Str),
        schema::ExprView::Bytes(_) => SchemeType::Ground(Type::Bytes),
        schema::ExprView::Duration(_) => SchemeType::Ground(Type::Duration),
        schema::ExprView::PreparedRegex(_) => SchemeType::Ground(Type::Regex),
        schema::ExprView::Path(_) => SchemeType::Ground(Type::Path),
        schema::ExprView::Field { base, name, .. } => {
            let base = child(base)?;
            match base {
                SchemeType::Record { fields, .. } => fields.get(&Name::intern(name)).cloned(),
                SchemeType::Ground(Type::Record(fields)) => fields.get(&Name::intern(name)).cloned().map(SchemeType::Ground),
                _ => None,
            }.ok_or_else(|| IrVerifyError::new("generic field projection has no required field contract"))?
        }
        schema::ExprView::Call { function, args, .. } | schema::ExprView::DirectPureCall { function, args, .. } => {
            let mut arguments = Vec::new();
            for argument in args {
                let super::decode::CallArgView::Single(value) = argument? else { return Err(IrVerifyError::new("generic body call requires explicit fixed arguments")) };
                arguments.push(child(value)?);
            }
            let target = store.functions.get(function.id.index()).ok_or_else(|| IrVerifyError::new("generic body call target is invalid"))?;
            if store.function_metadata[function.id.index()].flags & 1 != 0 { return Err(IrVerifyError::new("generic body calls an effectful function")); }
            match target.contract {
                FullCallableContract::Scheme(_) => ty.clone(),
                FullCallableContract::Ground(signature) => {
                    if store.semantic.signature_param_count(signature)? != arguments.len() || !store.semantic.signature_effects_empty(signature)? { return Err(IrVerifyError::new("generic body call signature has incompatible arguments or effects")); }
                    for (index, actual) in arguments.iter().enumerate() {
                        let (_, expected, flags) = store.semantic.signature_param(signature, index)?;
                        if flags != 0 { return Err(IrVerifyError::new("generic body call requires fixed required parameters")); }
                        let expected = store.semantic.to_type_with_budget(expected, &mut budget.semantic_types)?;
                        if !body_type_matches_ground(actual, &expected) { return Err(IrVerifyError::new("generic body call argument disagrees with its checked parameter")); }
                    }
                    let result = if target.checked_return_type != IR_NONE { TypeId::from_raw(target.checked_return_type).ok_or_else(|| IrVerifyError::new("generic body call result is invalid"))? } else { store.semantic.signature_return_type(signature)? };
                    SchemeType::Ground(store.semantic.to_type_with_budget(result, &mut budget.semantic_types)?)
                }
            }
        }
        schema::ExprView::CheckedValue { value, check, .. } => {
            let actual = child(value)?;
            let expected = check.ty.materialize()?;
            if check.schema.is_some() || !expected.has_unsigned_constraint() || !body_type_matches_ground(&actual, &expected) || !runtime_complete_unsigned_check_type(&expected, budget, 0)? { return Err(IrVerifyError::new("generic unsigned argument check is not a complete checked contract")); }
            SchemeType::Ground(expected)
        }
        schema::ExprView::Require { value, check, .. } => {
            let actual = child(value)?;
            let expected = check.ty.materialize()?;
            if check.schema.is_some() || expected.contains_any() || expected.contains_inference() || expected.contains_recovery() || expected.may_hold_resource() || !body_type_matches_ground(&actual, &expected) { return Err(IrVerifyError::new("generic storage check has an invalid ground contract")); }
            SchemeType::Result(Box::new(SchemeType::Ground(expected)), Box::new(SchemeType::Ground(Type::Error)))
        }
        schema::ExprView::Try { value, .. } => {
            let schema::ExprView::Require { check, .. } = decoder.expression_view(value.raw() as usize)? else { return Err(IrVerifyError::new("generic propagation has no checked unsigned storage")) };
            if check.schema.is_some() || check.ty.materialize()?.may_hold_resource() { return Err(IrVerifyError::new("generic propagation has an invalid ground storage contract")); }
            let actual = child(value)?;
            let SchemeType::Result(ok, error) = actual else { return Err(IrVerifyError::new("generic unsigned storage check is not a result")) };
            if *error != SchemeType::Ground(Type::Error) { return Err(IrVerifyError::new("generic unsigned storage error contract is invalid")); }
            *ok
        }
        schema::ExprView::Method { receiver, name, args, .. } => {
            let receiver = child(receiver)?;
            let mut arguments = Vec::new();
            for argument in args { arguments.push(Some(child(argument?)?)); }
            let plan = store.registered_calls.get(&(instruction as u32)).ok_or_else(|| IrVerifyError::new("generic host method has no selected registry plan"))?;
            if plan.name.as_ref() != name || plan.receiver.is_none() { return Err(IrVerifyError::new("generic host method identity disagrees with its plan")); }
            let references = arguments.iter().map(Option::as_ref).collect::<Vec<_>>();
            if !crate::sema::builtin_templates::verify_registered_call(plan, Some(&receiver), &references, ty) { return Err(IrVerifyError::new("generic host method disagrees with its selected registry signature")); }
            registered_coverage.insert(instruction as u32);
            ty.clone()
        }
        schema::ExprView::ModuleCall { op, call_plan, args, .. } => {
            let mut arguments = Vec::new();
            for argument in args { arguments.push(argument?.map(&mut child).transpose()?); }
            let plan = store.registered_calls.get(&(instruction as u32)).ok_or_else(|| IrVerifyError::new("generic module call has no selected registry plan"))?;
            let references = arguments.iter().map(Option::as_ref).collect::<Vec<_>>();
            if call_plan.is_some() || plan.receiver.is_some() || plan.signature.op != op || !crate::sema::builtin_templates::verify_registered_call(plan, None, &references, ty) { return Err(IrVerifyError::new("generic module call disagrees with its selected registry signature")); }
            registered_coverage.insert(instruction as u32);
            ty.clone()
        }
        _ => return Err(IrVerifyError::new("generic body contains an unsupported executable expression")),
    };
    if ty != &expected && !matches!((ty.closed_ground_type(), expected.closed_ground_type()), (Some(actual), Some(expected)) if actual == expected) { return Err(IrVerifyError::new("generic body expression disagrees with its checked type")); }
    Ok(ty.clone())
}

fn verify_call_instantiations(
    store: &FullStore,
    scopes: &[Option<SchemeScope>],
) -> Result<(), IrVerifyError> {
    let mut target_by_instruction = BTreeMap::new();
    for fact in &store.checked_call_targets {
        if target_by_instruction.insert(fact.instruction, fact.function).is_some() {
            return Err(IrVerifyError::new("checked call targets are duplicated"));
        }
    }
    let mut instantiation_coverage = vec![false; store.call_instantiations.len()];
    let mut type_argument_coverage = vec![false; store.call_type_arguments.len()];
    let mut row_argument_coverage = vec![false; store.call_row_arguments.len()];
    let mut row_field_coverage = vec![false; store.call_row_fields.len()];
    let mut instantiations = BTreeMap::new();
    let mut previous_instantiation = None;

    for (index, instantiation) in store.call_instantiations.iter().copied().enumerate() {
        if previous_instantiation.is_some_and(|previous| previous >= instantiation.instruction) {
            return Err(IrVerifyError::new("generic call instantiations are not strictly ordered"));
        }
        previous_instantiation = Some(instantiation.instruction);
        instantiation_coverage[index] = true;
        if instantiations.insert(instantiation.instruction, index).is_some() {
            return Err(IrVerifyError::new("generic call instantiations are duplicated"));
        }
        let instruction = instantiation.instruction as usize;
        if instruction >= store.tags.len()
            || !matches!(
                store.tags[instruction],
                FullTag::ExprCall | FullTag::ExprDirectPureCall | FullTag::ExprSelfCall
            )
        {
            return Err(IrVerifyError::new("generic call instantiation has no direct call"));
        }
        let target = *target_by_instruction
            .get(&instantiation.instruction)
            .ok_or_else(|| IrVerifyError::new("generic call has no checked target"))?;
        let target_function = store
            .functions
            .get(target.index())
            .ok_or_else(|| IrVerifyError::new("generic call target is out of bounds"))?;
        let FullCallableContract::Scheme(scheme_id) = target_function.contract else {
            return Err(IrVerifyError::new("ground call carries generic substitution evidence"));
        };
        let scope = scopes
            .get(scheme_id.index())
            .and_then(Option::as_ref)
            .ok_or_else(|| IrVerifyError::new("generic call target has no binder scope"))?;
        let scheme = store.scheme(scheme_id)?;
        let type_rows = mark_range(
            instantiation.type_arguments,
            &store.call_type_arguments,
            &mut type_argument_coverage,
            "generic call type argument range is invalid or overlaps",
        )?;
        let row_rows = mark_range(
            instantiation.row_arguments,
            &store.call_row_arguments,
            &mut row_argument_coverage,
            "generic call row argument range is invalid or overlaps",
        )?;
        if let Some((caller_scheme, caller_scope)) =
            caller_scheme_for_instruction(store, instruction, scopes)?
        {
            let substitution = build_symbolic_substitution(
                store,
                scope,
                caller_scope,
                type_rows,
                row_rows,
                &mut row_field_coverage,
            )?;
            let mut budget = SchemeInstantiationBudget::new(store);
            verify_symbolic_call_arguments(
                store,
                instruction,
                target_function,
                scheme,
                scope,
                caller_scheme,
                caller_scope,
                &substitution,
                &mut budget,
            )?;
            let result = scheme_body_fact_type(
                store,
                caller_scheme,
                instantiation.instruction,
                FullSchemeBodyFactKind::Expression,
            )?
            .ok_or_else(|| IrVerifyError::new("generic call has no checked scheme result"))?;
            let actual = decode_scheme_type(store, caller_scope, result, &mut budget, 0)?;
            let expected = substitute_scheme_type(
                &decode_scheme_type(store, scope, scheme.return_type, &mut budget, 0)?,
                scope,
                &substitution,
                &mut budget,
                0,
            )?;
            if actual != expected && !matches!((actual.closed_ground_type(), expected.closed_ground_type()), (Some(actual), Some(expected)) if actual == expected) {
                return Err(IrVerifyError::new(
                    "generic call result does not match its checked scheme fact",
                ));
            }
        } else {
            let substitution = build_substitution(
                store,
                scope,
                type_rows,
                row_rows,
                &mut row_field_coverage,
            )?;
            let mut materialization = SchemeInstantiationBudget::new(store);
            verify_instantiated_call_arguments(
                store,
                instruction,
                target_function,
                scheme,
                scope,
                &substitution,
                &mut materialization,
            )?;
            let result = checked_expression_type_id(store, instantiation.instruction)
                .ok_or_else(|| IrVerifyError::new("generic call has no checked result type"))?;
            let checked_result = store
                .semantic
                .to_type_with_budget(result, &mut materialization.semantic_types)?;
            ensure_closed_type(&checked_result)?;
            let expected_result = instantiate_scheme_type(
                store,
                scope,
                scheme.return_type,
                &substitution,
                &mut materialization,
                0,
            )?;
            if checked_result != expected_result {
                return Err(IrVerifyError::new(
                    "generic call result does not match its checked instantiation",
                ));
            }
        }
    }

    for (&instruction, &target) in &target_by_instruction {
        let target_function = store
            .functions
            .get(target.index())
            .ok_or_else(|| IrVerifyError::new("checked call target is out of bounds"))?;
        let generic = matches!(target_function.contract, FullCallableContract::Scheme(_));
        if generic != instantiations.contains_key(&instruction) {
            return Err(IrVerifyError::new(if generic {
                "generic direct call has no substitution evidence"
            } else {
                "ground direct call has generic substitution evidence"
            }));
        }
        if generic
            && checked_expression_type_id(store, instruction).is_none()
            && caller_scheme_for_instruction(store, instruction as usize, scopes)?.is_none()
        {
            return Err(IrVerifyError::new("generic direct call has no checked result fact"));
        }
    }
    for instruction in 0..store.tags.len() {
        if !matches!(
            store.tags[instruction],
            FullTag::ExprCall | FullTag::ExprDirectPureCall | FullTag::ExprSelfCall
        ) {
            continue;
        }
        let decoder = decoder_for_instruction(store, instruction)?;
        let target = match decoder.expression_view(instruction)? {
            schema::ExprView::Call { function, .. }
            | schema::ExprView::DirectPureCall { function, .. } => function.id,
            schema::ExprView::SelfCall { .. } => IrFunctionId::from_raw(decoder.owner)
                .ok_or_else(|| IrVerifyError::new("generic self call has no function owner"))?,
            _ => return Err(IrVerifyError::new("direct call tag has an invalid view")),
        };
        let target_function = store
            .functions
            .get(target.index())
            .ok_or_else(|| IrVerifyError::new("generic call target is out of bounds"))?;
        if matches!(target_function.contract, FullCallableContract::Scheme(_)) {
            let instruction = u32::try_from(instruction)
                .map_err(|_| IrVerifyError::new("generic call instruction is out of bounds"))?;
            if !target_by_instruction.contains_key(&instruction)
                || !instantiations.contains_key(&instruction)
                || (checked_expression_type_id(store, instruction).is_none()
                    && caller_scheme_for_instruction(store, instruction as usize, scopes)?.is_none())
            {
                return Err(IrVerifyError::new(
                    "generic direct call has incomplete checked evidence",
                ));
            }
        }
    }
    if instantiation_coverage.iter().any(|covered| !covered)
        || type_argument_coverage.iter().any(|covered| !covered)
        || row_argument_coverage.iter().any(|covered| !covered)
        || row_field_coverage.iter().any(|covered| !covered)
    {
        return Err(IrVerifyError::new("generic call evidence contains unowned rows"));
    }
    Ok(())
}

fn build_substitution(
    store: &FullStore,
    scope: &SchemeScope,
    type_rows: &[FullTypeArgument],
    row_rows: &[super::schemes::FullRowArgument],
    row_field_coverage: &mut [bool],
) -> Result<CallSubstitution, IrVerifyError> {
    let mut types = vec![None; scope.type_parameters.len()];
    let mut materialization = TypeMaterializationBudget::default();
    let mut type_ids = BTreeSet::new();
    for row in type_rows {
        if !type_ids.insert(row.binder) {
            return Err(IrVerifyError::new("generic call type binder is duplicated"));
        }
        let Some(&binder) = scope.type_binders.get(&row.binder) else {
            return Err(IrVerifyError::new("generic call type binder is out of scope"));
        };
        let ty = ground_call_substitution_type(store, row.ty, &mut materialization)?;
        ensure_substitution_type(&ty)?;
        if types[binder].replace(ty).is_some() {
            return Err(IrVerifyError::new("generic call type binder is duplicated"));
        }
    }
    if types.iter().any(Option::is_none) {
        return Err(IrVerifyError::new("generic call type substitution is incomplete"));
    }

    let mut rows = vec![None; scope.row_parameters.len()];
    let mut row_ids = BTreeSet::new();
    for row in row_rows {
        if !row_ids.insert(row.binder) {
            return Err(IrVerifyError::new("generic call row binder is duplicated"));
        }
        let Some(&binder) = scope.row_binders.get(&row.binder) else {
            return Err(IrVerifyError::new("generic call row binder is out of scope"));
        };
        if row.tail != IR_NONE {
            return Err(IrVerifyError::new(
                "ground generic call carries a symbolic row tail",
            ));
        }
        let mut fields = BTreeMap::new();
        let mut previous = None;
        for field in mark_range(
            row.fields,
            &store.call_row_fields,
            row_field_coverage,
            "generic call row evidence range is invalid or overlaps",
        )? {
            let text = store.string(field.name)?;
            if text.is_empty() || previous.is_some_and(|previous: &str| previous >= text) {
                return Err(IrVerifyError::new("generic call row fields are not sorted unique"));
            }
            previous = Some(text);
            let name = Name::intern(text);
            if scope.row_lacks[binder].contains(&name) {
                return Err(IrVerifyError::new("generic call row evidence violates a lacks constraint"));
            }
            let ty = ground_call_substitution_type(store, field.ty, &mut materialization)?;
            ensure_substitution_type(&ty)?;
            if fields.insert(name, ty).is_some() {
                return Err(IrVerifyError::new("generic call row field is duplicated"));
            }
        }
        if rows[binder].replace(fields).is_some() {
            return Err(IrVerifyError::new("generic call row binder is duplicated"));
        }
    }
    if rows.iter().any(Option::is_none) {
        return Err(IrVerifyError::new("generic call row substitution is incomplete"));
    }
    Ok(CallSubstitution {
        types: types.into_iter().map(Option::unwrap).collect(),
        rows: rows.into_iter().map(Option::unwrap).collect(),
    })
}

fn ground_call_substitution_type(
    store: &FullStore,
    id: SchemeTypeId,
    budget: &mut TypeMaterializationBudget,
) -> Result<Type, IrVerifyError> {
    let node = store.scheme_type(id)?;
    if node.tag != FullSchemeTypeTag::Ground {
        return Err(IrVerifyError::new(
            "ground generic call carries a symbolic type substitution",
        ));
    }
    let type_id = TypeId::from_raw(node.first)
        .ok_or_else(|| IrVerifyError::new("ground scheme type id is invalid"))?;
    let ty = store.semantic.to_type_with_budget(type_id, budget)?;
    ensure_substitution_type(&ty)?;
    Ok(ty)
}

fn caller_scheme_for_instruction<'a>(
    store: &FullStore,
    instruction: usize,
    scopes: &'a [Option<SchemeScope>],
) -> Result<Option<(FullSchemeId, &'a SchemeScope)>, IrVerifyError> {
    for (index, function) in store.functions.iter().enumerate() {
        if !store.function_instruction_range(index)?.contains(&instruction) {
            continue;
        }
        let FullCallableContract::Scheme(id) = function.contract else {
            return Ok(None);
        };
        let scope = scopes
            .get(id.index())
            .and_then(Option::as_ref)
            .ok_or_else(|| IrVerifyError::new("generic caller has no binder scope"))?;
        return Ok(Some((id, scope)));
    }
    Ok(None)
}

fn scheme_body_fact_type(
    store: &FullStore,
    scheme_id: FullSchemeId,
    instruction: u32,
    kind: FullSchemeBodyFactKind,
) -> Result<Option<SchemeTypeId>, IrVerifyError> {
    let scheme = store.scheme(scheme_id)?;
    let mut found = None;
    for fact in bounded(
        scheme.body_facts,
        &store.scheme_body_facts,
        "scheme body fact range is invalid",
    )? {
        if fact.instruction == instruction && fact.kind == kind {
            if found.replace(fact.ty).is_some() {
                return Err(IrVerifyError::new("generic body fact is duplicated"));
            }
        }
    }
    Ok(found)
}

fn build_symbolic_substitution(
    store: &FullStore,
    callee_scope: &SchemeScope,
    caller_scope: &SchemeScope,
    type_rows: &[FullTypeArgument],
    row_rows: &[super::schemes::FullRowArgument],
    row_field_coverage: &mut [bool],
) -> Result<SymbolicCallSubstitution, IrVerifyError> {
    let mut types = vec![None; callee_scope.type_parameters.len()];
    let mut type_ids = BTreeSet::new();
    let mut budget = SchemeInstantiationBudget::new(store);
    for row in type_rows {
        if !type_ids.insert(row.binder) {
            return Err(IrVerifyError::new("generic call type binder is duplicated"));
        }
        let Some(&binder) = callee_scope.type_binders.get(&row.binder) else {
            return Err(IrVerifyError::new("generic call type binder is out of scope"));
        };
        let ty = decode_scheme_type(store, caller_scope, row.ty, &mut budget, 0)?;
        validate_symbolic_substitution_type(&ty, caller_scope, 0)?;
        if types[binder].replace(ty).is_some() {
            return Err(IrVerifyError::new("generic call type binder is duplicated"));
        }
    }
    if types.iter().any(Option::is_none) {
        return Err(IrVerifyError::new("generic call type substitution is incomplete"));
    }

    let mut rows = vec![None; callee_scope.row_parameters.len()];
    let mut row_ids = BTreeSet::new();
    for row in row_rows {
        if !row_ids.insert(row.binder) {
            return Err(IrVerifyError::new("generic call row binder is duplicated"));
        }
        let Some(&callee_binder) = callee_scope.row_binders.get(&row.binder) else {
            return Err(IrVerifyError::new("generic call row binder is out of scope"));
        };
        let mut fields = BTreeMap::new();
        let mut previous = None;
        for field in mark_range(
            row.fields,
            &store.call_row_fields,
            row_field_coverage,
            "generic call row evidence range is invalid or overlaps",
        )? {
            let text = store.string(field.name)?;
            if text.is_empty() || previous.is_some_and(|previous: &str| previous >= text) {
                return Err(IrVerifyError::new("generic call row fields are not sorted unique"));
            }
            previous = Some(text);
            let name = Name::intern(text);
            if callee_scope.row_lacks[callee_binder].contains(&name) {
                return Err(IrVerifyError::new(
                    "generic call row evidence violates a lacks constraint",
                ));
            }
            let ty = decode_scheme_type(store, caller_scope, field.ty, &mut budget, 0)?;
            validate_symbolic_substitution_type(&ty, caller_scope, 0)?;
            if fields.insert(name, ty).is_some() {
                return Err(IrVerifyError::new("generic call row field is duplicated"));
            }
        }
        let tail = if row.tail == IR_NONE {
            SchemeRowTail::Closed
        } else {
            let caller_binder = row.tail as usize;
            let caller = caller_scope
                .row_parameters
                .get(caller_binder)
                .ok_or_else(|| IrVerifyError::new("generic call row tail is out of scope"))?;
            if !callee_scope.row_lacks[callee_binder]
                .iter()
                .chain(fields.keys())
                .all(|name| caller_scope.row_lacks[caller_binder].contains(name))
            {
                return Err(IrVerifyError::new(
                    "generic call row tail lacks do not cover the callee contract",
                ));
            }
            SchemeRowTail::Parameter(SchemeRowParameterId(caller.id))
        };
        if rows[callee_binder]
            .replace(SchemeRowSubstitution { fields, tail })
            .is_some()
        {
            return Err(IrVerifyError::new("generic call row binder is duplicated"));
        }
    }
    if rows.iter().any(Option::is_none) {
        return Err(IrVerifyError::new("generic call row substitution is incomplete"));
    }
    Ok(SymbolicCallSubstitution {
        types: types.into_iter().map(Option::unwrap).collect(),
        rows: rows.into_iter().map(Option::unwrap).collect(),
    })
}

fn decode_scheme_type(
    store: &FullStore,
    scope: &SchemeScope,
    id: SchemeTypeId,
    budget: &mut SchemeInstantiationBudget,
    depth: usize,
) -> Result<SchemeType, IrVerifyError> {
    if depth > 128 {
        return Err(IrVerifyError::new("scheme substitution type nesting is too deep"));
    }
    budget.charge(1)?;
    let node = *store.scheme_type(id)?;
    if node.reserved != [0; 3] {
        return Err(IrVerifyError::new("scheme substitution type has reserved bits set"));
    }
    let nested = |id, budget: &mut SchemeInstantiationBudget| {
        decode_scheme_type(store, scope, id, budget, depth + 1)
    };
    let ty = match node.tag {
        FullSchemeTypeTag::Ground => {
            let id = TypeId::from_raw(node.first)
                .ok_or_else(|| IrVerifyError::new("ground scheme type id is invalid"))?;
            let ty = store.semantic.to_type_with_budget(id, &mut budget.semantic_types)?;
            ensure_closed_type(&ty)?;
            SchemeType::Ground(ty)
        }
        FullSchemeTypeTag::Parameter => {
            let parameter = scope
                .type_parameters
                .get(node.first as usize)
                .ok_or_else(|| IrVerifyError::new("scheme type parameter is out of scope"))?;
            SchemeType::Parameter(SchemeTypeParameterId(parameter.id))
        }
        FullSchemeTypeTag::List => SchemeType::List(Box::new(nested(
            SchemeTypeId::from_raw(node.first)?,
            budget,
        )?)),
        FullSchemeTypeTag::Map => SchemeType::Map(
            Box::new(nested(SchemeTypeId::from_raw(node.first)?, budget)?),
            Box::new(nested(SchemeTypeId::from_raw(node.second)?, budget)?),
        ),
        FullSchemeTypeTag::Stream => SchemeType::Stream(Box::new(nested(
            SchemeTypeId::from_raw(node.first)?,
            budget,
        )?)),
        FullSchemeTypeTag::Set => SchemeType::Set(Box::new(nested(
            SchemeTypeId::from_raw(node.first)?,
            budget,
        )?)),
        FullSchemeTypeTag::Result => SchemeType::Result(
            Box::new(nested(SchemeTypeId::from_raw(node.first)?, budget)?),
            Box::new(nested(SchemeTypeId::from_raw(node.second)?, budget)?),
        ),
        FullSchemeTypeTag::Optional => SchemeType::Optional(Box::new(nested(
            SchemeTypeId::from_raw(node.first)?,
            budget,
        )?)),
        FullSchemeTypeTag::Record => {
            let mut fields = BTreeMap::new();
            let mut previous = None;
            for field in bounded(
                node.children,
                &store.scheme_fields,
                "scheme record field range is invalid",
            )? {
                let text = store.string(field.name)?;
                if text.is_empty() || previous.is_some_and(|previous: &str| previous >= text) {
                    return Err(IrVerifyError::new("scheme record fields are not sorted unique"));
                }
                previous = Some(text);
                let name = Name::intern(text);
                let field_type = nested(field.ty, budget)?;
                if fields.insert(name, field_type).is_some() {
                    return Err(IrVerifyError::new("scheme record fields are duplicated"));
                }
            }
            let tail = if node.first == IR_NONE {
                SchemeRowTail::Closed
            } else {
                let parameter = scope
                    .row_parameters
                    .get(node.first as usize)
                    .ok_or_else(|| IrVerifyError::new("scheme row parameter is out of scope"))?;
                SchemeRowTail::Parameter(SchemeRowParameterId(parameter.id))
            };
            SchemeType::Record { fields, tail }
        }
        FullSchemeTypeTag::Union => SchemeType::Union(
            bounded(
                node.children,
                &store.scheme_children,
                "scheme union child range is invalid",
            )?
            .iter()
            .map(|child| nested(*child, budget))
            .collect::<Result<Vec<_>, _>>()?,
        ),
        FullSchemeTypeTag::Validated => {
            let base = nested(SchemeTypeId::from_raw(node.first)?, budget)?;
            let validation = verify_validation_row(
                store,
                *store
                    .scheme_validations
                    .get(node.second as usize)
                    .ok_or_else(|| IrVerifyError::new("scheme validation id is out of bounds"))?,
            )?;
            SchemeType::Validated(validation, Box::new(base))
        }
    };
    Ok(ty)
}

fn validate_symbolic_substitution_type(
    ty: &SchemeType,
    caller_scope: &SchemeScope,
    depth: usize,
) -> Result<(), IrVerifyError> {
    if depth > 128 {
        return Err(IrVerifyError::new("scheme substitution type nesting is too deep"));
    }
    match ty {
        SchemeType::Ground(ty) => ensure_substitution_type(ty),
        SchemeType::Parameter(id) => caller_scope
            .type_binders
            .contains_key(&id.0)
            .then_some(())
            .ok_or_else(|| IrVerifyError::new("scheme substitution type binder is out of scope")),
        SchemeType::List(inner)
        | SchemeType::Stream(inner)
        | SchemeType::Set(inner)
        | SchemeType::Optional(inner)
        | SchemeType::Validated(_, inner) => {
            validate_symbolic_substitution_type(inner, caller_scope, depth + 1)
        }
        SchemeType::Map(key, value) | SchemeType::Result(key, value) => {
            validate_symbolic_substitution_type(key, caller_scope, depth + 1)?;
            validate_symbolic_substitution_type(value, caller_scope, depth + 1)
        }
        SchemeType::Record { fields, tail } => {
            for ty in fields.values() {
                validate_symbolic_substitution_type(ty, caller_scope, depth + 1)?;
            }
            if let SchemeRowTail::Parameter(id) = tail {
                let Some(&binder) = caller_scope.row_binders.get(&id.0) else {
                    return Err(IrVerifyError::new("scheme substitution row binder is out of scope"));
                };
                if fields
                    .keys()
                    .any(|name| !caller_scope.row_lacks[binder].contains(name))
                {
                    return Err(IrVerifyError::new(
                        "scheme substitution row lacks do not cover its explicit fields",
                    ));
                }
            }
            Ok(())
        }
        SchemeType::Union(members) => {
            for member in members {
                validate_symbolic_substitution_type(member, caller_scope, depth + 1)?;
            }
            Ok(())
        }
    }
}

fn substitute_scheme_type(
    ty: &SchemeType,
    callee_scope: &SchemeScope,
    substitution: &SymbolicCallSubstitution,
    budget: &mut SchemeInstantiationBudget,
    depth: usize,
) -> Result<SchemeType, IrVerifyError> {
    if depth > 128 {
        return Err(IrVerifyError::new("generic call scheme substitution is too deep"));
    }
    budget.charge(1)?;
    let nested = |ty, budget: &mut SchemeInstantiationBudget| {
        substitute_scheme_type(ty, callee_scope, substitution, budget, depth + 1)
    };
    Ok(match ty {
        SchemeType::Ground(ty) => {
            budget.charge_clone(ty)?;
            SchemeType::Ground(ty.clone())
        }
        SchemeType::Parameter(id) => {
            let Some(&binder) = callee_scope.type_binders.get(&id.0) else {
                return Err(IrVerifyError::new("scheme type parameter is out of scope"));
            };
            let ty = &substitution.types[binder];
            budget.charge_scheme_clone(ty)?;
            ty.clone()
        }
        SchemeType::List(inner) => SchemeType::List(Box::new(nested(inner, budget)?)),
        SchemeType::Map(key, value) => SchemeType::Map(
            Box::new(nested(key, budget)?),
            Box::new(nested(value, budget)?),
        ),
        SchemeType::Stream(inner) => SchemeType::Stream(Box::new(nested(inner, budget)?)),
        SchemeType::Set(inner) => SchemeType::Set(Box::new(nested(inner, budget)?)),
        SchemeType::Result(ok, error) => SchemeType::Result(
            Box::new(nested(ok, budget)?),
            Box::new(nested(error, budget)?),
        ),
        SchemeType::Optional(inner) => SchemeType::Optional(Box::new(nested(inner, budget)?)),
        SchemeType::Record { fields, tail } => {
            let mut substituted = fields
                .iter()
                .map(|(&name, ty)| Ok((name, nested(ty, budget)?)))
                .collect::<Result<BTreeMap<_, _>, IrVerifyError>>()?;
            let tail = match tail {
                SchemeRowTail::Closed => SchemeRowTail::Closed,
                SchemeRowTail::Parameter(id) => {
                    let Some(&binder) = callee_scope.row_binders.get(&id.0) else {
                        return Err(IrVerifyError::new("scheme row parameter is out of scope"));
                    };
                    let evidence = &substitution.rows[binder];
                    for (&name, ty) in &evidence.fields {
                        budget.charge(1)?;
                        budget.charge_scheme_clone(ty)?;
                        if substituted.insert(name, ty.clone()).is_some() {
                            return Err(IrVerifyError::new(
                                "generic row evidence repeats a required field",
                            ));
                        }
                    }
                    evidence.tail
                }
            };
            SchemeType::Record {
                fields: substituted,
                tail,
            }
        }
        SchemeType::Union(members) => SchemeType::Union(
            members
                .iter()
                .map(|member| nested(member, budget))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        SchemeType::Validated(validation, base) => {
            SchemeType::Validated(*validation, Box::new(nested(base, budget)?))
        }
    })
}

fn verify_symbolic_call_arguments(
    store: &FullStore,
    instruction: usize,
    target: &FullFunction,
    scheme: &FullScheme,
    callee_scope: &SchemeScope,
    caller_scheme: FullSchemeId,
    caller_scope: &SchemeScope,
    substitution: &SymbolicCallSubstitution,
    budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    let decoder = decoder_for_instruction(store, instruction)?;
    let args = match decoder.expression_view(instruction)? {
        schema::ExprView::Call { args, .. }
        | schema::ExprView::DirectPureCall { args, .. }
        | schema::ExprView::SelfCall { args, .. } => args,
        _ => return Err(IrVerifyError::new("generic call instruction has an invalid view")),
    };
    let arguments = args.collect::<Result<Vec<_>, _>>()?;
    let params = bounded(target.params, &store.params, "generic target parameter range is invalid")?;
    let scheme_params = bounded(scheme.params, &store.scheme_params, "scheme parameter range is invalid")?;
    if arguments.len() != params.len() || params.len() != scheme_params.len() || params.iter().any(|parameter| parameter.flags & 3 != 0) {
        return Err(IrVerifyError::new("generic composition target requires explicit fixed arguments"));
    }
    for (argument, parameter) in arguments.iter().zip(scheme_params) {
        let super::decode::CallArgView::Single(value) = argument else { return Err(IrVerifyError::new("generic composition call requires explicit fixed arguments")) };
        let actual = scheme_body_fact_type(store, caller_scheme, value.raw(), FullSchemeBodyFactKind::Expression)?.ok_or_else(|| IrVerifyError::new("generic call argument has no checked scheme fact"))?;
        let actual = decode_scheme_type(store, caller_scope, actual, budget, 0)?;
        let expected = decode_scheme_type(store, callee_scope, parameter.ty, budget, 0)?;
        let expected = substitute_scheme_type(&expected, callee_scope, substitution, budget, 0)?;
        if !body_type_matches(&actual, &expected) { return Err(IrVerifyError::new("generic call argument does not match its checked scheme fact")); }
    }
    Ok(())
}

fn verify_instantiated_call_arguments(
    store: &FullStore,
    instruction: usize,
    target: &FullFunction,
    scheme: &FullScheme,
    scope: &SchemeScope,
    substitution: &CallSubstitution,
    materialization: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    let decoder = decoder_for_instruction(store, instruction)?;
    let view = decoder.expression_view(instruction)?;
    let args = match view {
        schema::ExprView::Call { args, .. }
        | schema::ExprView::DirectPureCall { args, .. }
        | schema::ExprView::SelfCall { args, .. } => args,
        _ => return Err(IrVerifyError::new("generic call instruction has an invalid view")),
    };
    let mut arguments = Vec::new();
    for arg in args {
        arguments.push(arg?);
    }
    let params = bounded(target.params, &store.params, "generic target parameter range is invalid")?;
    let scheme_params = bounded(
        scheme.params,
        &store.scheme_params,
        "scheme parameter range is invalid",
    )?;
    if params.len() != scheme_params.len() {
        return Err(IrVerifyError::new(
            "generic call parameters do not match checked parameter slots",
        ));
    }
    let rest_index = params.iter().position(|param| param.flags & 1 != 0);
    if rest_index.is_some_and(|index| index + 1 != params.len()) {
        return Err(IrVerifyError::new("generic rest parameter is not final"));
    }
    let fixed_count = rest_index.unwrap_or(params.len());
    if arguments.len() < fixed_count
        || (rest_index.is_none() && arguments.len() != params.len())
    {
        return Err(IrVerifyError::new(
            "generic call arguments do not match checked parameter slots",
        ));
    }
    let rest_item = rest_index
        .map(|index| {
            scheme_list_item(
                store,
                scheme_params[index].ty,
                &mut materialization.semantic_types,
            )
        })
        .transpose()?;
    let mut checked_arguments = Vec::with_capacity(arguments.len());
    for (index, argument) in arguments.iter().enumerate() {
        let (value, expected_scheme_type, expected_ground_type, is_splice) = match argument {
            super::decode::CallArgView::Default(_) => {
                return Err(IrVerifyError::new("generic call cannot use a default argument"));
            }
            super::decode::CallArgView::Single(value) if index < fixed_count => {
                (value, Some(scheme_params[index].ty), None, false)
            }
            super::decode::CallArgView::Single(value) => {
                let item = rest_item.as_ref().ok_or_else(|| {
                    IrVerifyError::new("generic extra argument has no rest parameter")
                })?;
                match item {
                    SchemeTypeExpectation::Encoded(id) => (value, Some(*id), None, false),
                    SchemeTypeExpectation::Ground(ty) => (value, None, Some(ty), false),
                }
            }
            super::decode::CallArgView::Splice(_) if index < fixed_count => {
                return Err(IrVerifyError::new(
                    "generic fixed parameter cannot be supplied by a splice",
                ));
            }
            super::decode::CallArgView::Splice(value) => {
                let rest = rest_index
                    .ok_or_else(|| IrVerifyError::new("generic splice has no rest parameter"))?;
                (value, Some(scheme_params[rest].ty), None, true)
            }
        };
        let expected = expected_scheme_type
            .map(|id| {
                instantiate_scheme_type(
                    store,
                    scope,
                    id,
                    substitution,
                    materialization,
                    0,
                )
            })
            .transpose()?;
        let expected = expected
            .or_else(|| expected_ground_type.cloned())
            .ok_or_else(|| IrVerifyError::new("generic call argument type is invalid"))?;
        let actual = checked_argument_type(
            store,
            &decoder,
            instruction,
            value.raw() as usize,
            materialization,
            0,
        )?
        .ok_or_else(|| {
            IrVerifyError::new("generic call argument has no closed checked type")
        })?;
        checked_arguments.push((expected, expected_scheme_type, is_splice, actual));
    }
    let mut binder_evidence = SchemeBinderEvidence::default();
    for (expected, expected_scheme_type, is_splice, actual) in &checked_arguments {
        match actual {
            CheckedArgumentType::Known(actual) => {
                ensure_closed_type(actual)?;
                if !actual.matches_expected(expected) {
                    return Err(IrVerifyError::new(
                        "generic call argument does not match its checked parameter slot",
                    ));
                }
                if let Some(id) = expected_scheme_type {
                    verify_scheme_argument_evidence(
                        store,
                        scope,
                        *id,
                        actual,
                        substitution,
                        &mut binder_evidence,
                        materialization,
                        0,
                    )?;
                }
            }
            CheckedArgumentType::ListItems(items) if *is_splice => {
                let expected_item = expected_list_item(expected, !items.is_empty())
                    .ok_or_else(|| {
                        IrVerifyError::new(
                            "generic list literal does not match a list parameter",
                        )
                    })?;
                for item in items {
                    ensure_closed_type(item)?;
                    if !item.matches_expected(expected_item) {
                        return Err(IrVerifyError::new(
                            "generic list item does not match its checked parameter slot",
                        ));
                    }
                }
                if let Some(id) = expected_scheme_type
                    && let SchemeTypeExpectation::Encoded(item_id) = scheme_list_item(
                        store,
                        *id,
                        &mut materialization.semantic_types,
                    )?
                {
                    for item in items {
                        verify_scheme_argument_evidence(
                            store,
                            scope,
                            item_id,
                            item,
                            substitution,
                            &mut binder_evidence,
                            materialization,
                            0,
                        )?;
                    }
                }
            }
            CheckedArgumentType::ListItems(items) => {
                let expected_item = expected_list_item(expected, !items.is_empty())
                    .ok_or_else(|| {
                        IrVerifyError::new(
                            "generic list literal does not match a list parameter",
                        )
                    })?;
                for item in items {
                    ensure_closed_type(item)?;
                    if !item.matches_expected(expected_item) {
                        return Err(IrVerifyError::new(
                            "generic list item does not match its checked parameter slot",
                        ));
                    }
                }
                if let Some(id) = expected_scheme_type {
                    if let Some(item_scheme) = scheme_list_item_for_argument(
                        store,
                        *id,
                        &mut materialization.semantic_types,
                    )? {
                        if let SchemeTypeExpectation::Encoded(item_id) = item_scheme {
                            for item in items {
                                verify_scheme_argument_evidence(
                                    store,
                                    scope,
                                    item_id,
                                    item,
                                    substitution,
                                    &mut binder_evidence,
                                    materialization,
                                    0,
                                )?;
                            }
                        }
                    } else {
                        let actual = merge_list_item_types(items, materialization)?.ok_or_else(|| {
                            IrVerifyError::new("generic list literal has no closed checked type")
                        })?;
                        verify_scheme_argument_evidence(
                            store,
                            scope,
                            *id,
                            &actual,
                            substitution,
                            &mut binder_evidence,
                            materialization,
                            0,
                        )?;
                    }
                }
            }
            CheckedArgumentType::EmptyList(Some(actual)) => {
                ensure_closed_type(actual)?;
                if !actual.matches_expected(expected) {
                    return Err(IrVerifyError::new(
                        "generic empty list argument does not match its checked parameter slot",
                    ));
                }
                if let Some(id) = expected_scheme_type {
                    verify_scheme_argument_evidence(
                        store,
                        scope,
                        *id,
                        actual,
                        substitution,
                        &mut binder_evidence,
                        materialization,
                        0,
                    )?;
                }
            }
            CheckedArgumentType::EmptyList(None) | CheckedArgumentType::EmptyMap => {}
        }
    }
    for (expected, expected_scheme_type, _, actual) in &checked_arguments {
        let declared = match actual {
            CheckedArgumentType::EmptyList(declared) => {
                if expected_list_item(expected, false).is_none() {
                    return Err(IrVerifyError::new("empty generic list argument has no concrete list contract"));
                }
                declared.as_ref()
            }
            CheckedArgumentType::EmptyMap => {
                if !matches!(expected, Type::Map(_, _)) {
                    return Err(IrVerifyError::new("empty generic map argument has no concrete map contract"));
                }
                None
            }
            _ => continue,
        };
        if let Some(id) = expected_scheme_type {
            let (required_types, required_rows) = scheme_type_binders(
                store,
                *id,
                materialization,
            )?;
            if !required_types.is_subset(&binder_evidence.types)
                || !required_rows.is_subset(&binder_evidence.rows)
            {
                return Err(IrVerifyError::new(
                    "empty generic collection argument has no independently checked binder evidence",
                ));
            }
        }
        if let Some(declared) = declared {
            ensure_closed_type(declared)?;
            if !matches!(declared, Type::List(_)) || !declared.matches_expected(expected) {
                return Err(IrVerifyError::new(
                    "empty list initializer disagrees with its checked slot type",
                ));
            }
        }
    }
    let required_types = (0..scope.type_parameters.len()).collect::<BTreeSet<_>>();
    let required_rows = (0..scope.row_parameters.len()).collect::<BTreeSet<_>>();
    if !required_types.is_subset(&binder_evidence.types)
        || !required_rows.is_subset(&binder_evidence.rows)
    {
        return Err(IrVerifyError::new(
            "generic call substitution has no independently checked argument evidence",
        ));
    }
    Ok(())
}

fn scheme_list_item(
    store: &FullStore,
    id: SchemeTypeId,
    materialization: &mut TypeMaterializationBudget,
) -> Result<SchemeTypeExpectation, IrVerifyError> {
    let mut current = id;
    for _ in 0..=128 {
        let node = store.scheme_type(current)?;
        match node.tag {
            FullSchemeTypeTag::List => {
                return Ok(SchemeTypeExpectation::Encoded(SchemeTypeId::from_raw(
                    node.first,
                )?));
            }
            FullSchemeTypeTag::Validated => current = SchemeTypeId::from_raw(node.first)?,
            FullSchemeTypeTag::Ground => {
                let type_id = TypeId::from_raw(node.first)
                    .ok_or_else(|| IrVerifyError::new("scheme ground type id is invalid"))?;
                let ty = store
                    .semantic
                    .to_type_with_budget(type_id, materialization)?
                    .into_unvalidated();
                return match ty {
                    Type::List(item) => Ok(SchemeTypeExpectation::Ground(*item)),
                    _ => Err(IrVerifyError::new(
                        "generic rest parameter does not have a List type",
                    )),
                };
            }
            _ => {
                return Err(IrVerifyError::new(
                    "generic rest parameter does not have a List type",
                ));
            }
        }
    }
    Err(IrVerifyError::new("generic rest parameter type is too deep"))
}

fn scheme_list_item_for_argument(
    store: &FullStore,
    id: SchemeTypeId,
    materialization: &mut TypeMaterializationBudget,
) -> Result<Option<SchemeTypeExpectation>, IrVerifyError> {
    let mut current = id;
    for _ in 0..=128 {
        let node = store.scheme_type(current)?;
        match node.tag {
            FullSchemeTypeTag::List => {
                return scheme_list_item(store, id, materialization).map(Some);
            }
            FullSchemeTypeTag::Ground => {
                let type_id = TypeId::from_raw(node.first)
                    .ok_or_else(|| IrVerifyError::new("scheme ground type id is invalid"))?;
                let actual = store
                    .semantic
                    .to_type_with_budget(type_id, materialization)?
                    .into_unvalidated();
                return Ok(match actual {
                    Type::List(item) => Some(SchemeTypeExpectation::Ground(*item)),
                    _ => None,
                });
            }
            FullSchemeTypeTag::Validated => {
                current = SchemeTypeId::from_raw(node.first)?;
            }
            _ => return Ok(None),
        }
    }
    Err(IrVerifyError::new("generic list argument type is too deep"))
}

fn verify_scheme_list_shape(store: &FullStore, id: SchemeTypeId) -> Result<(), IrVerifyError> {
    let mut current = id;
    for _ in 0..=128 {
        let node = store.scheme_type(current)?;
        match node.tag {
            FullSchemeTypeTag::List => return Ok(()),
            FullSchemeTypeTag::Validated => current = SchemeTypeId::from_raw(node.first)?,
            FullSchemeTypeTag::Ground => {
                let type_id = TypeId::from_raw(node.first)
                    .ok_or_else(|| IrVerifyError::new("scheme ground type id is invalid"))?;
                return store
                    .semantic
                    .list_item_type_id(type_id)?
                    .map(|_| ())
                    .ok_or_else(|| {
                        IrVerifyError::new("generic rest parameter does not have a List type")
                    });
            }
            _ => {
                return Err(IrVerifyError::new(
                    "generic rest parameter does not have a List type",
                ));
            }
        }
    }
    Err(IrVerifyError::new("generic rest parameter type is too deep"))
}

enum ArgumentAliasCheck {
    Annotation(TypeId),
    Declared(TypeId),
}

fn checked_argument_type(
    store: &FullStore,
    decoder: &FullDecoder<'_>,
    mut use_instruction: usize,
    mut instruction: usize,
    materialization: &mut SchemeInstantiationBudget,
    depth: usize,
) -> Result<Option<CheckedArgumentType>, IrVerifyError> {
    let mut owned_decoder = None;
    let mut aliases = BTreeSet::new();
    let mut checks = Vec::new();
    let actual = loop {
        let current = owned_decoder.as_ref().unwrap_or(decoder);
        materialization.charge(1)?;
        if !aliases.insert((current.owner, instruction)) {
            return Err(IrVerifyError::new("generic argument alias chain contains a cycle"));
        }
        let schema::ExprView::Param(slot) = current.expression_view(instruction)? else {
            break checked_argument_type_inner(
                store, current, use_instruction, instruction, materialization, depth,
            )?;
        };
        if super::driver_owner_index(current.owner).is_some() {
            let source = current.driver_local_let_source(
                slot,
                |amount| materialization.charge(amount),
            )?;
            let Some(source) = source else {
                break checked_argument_type_inner(
                    store, current, use_instruction, instruction, materialization, depth,
                )?;
            };
            let next = decoder_for_driver_step(store, source.step)?;
            if !next.instruction_range.contains(&source.initializer) {
                return Err(IrVerifyError::new(
                    "driver local initializer belongs to another step",
                ));
            }
            if let Some(annotation) = source.annotation {
                let annotated = store.semantic.to_type_with_budget(
                    annotation, &mut materialization.semantic_types,
                )?;
                let checked = store.semantic.to_type_with_budget(
                    source.checked_type, &mut materialization.semantic_types,
                )?;
                if checked != annotated {
                    return Err(IrVerifyError::new(
                        "driver slot type fact disagrees with its declaration annotation",
                    ));
                }
                verify_driver_annotation_runtime_type(
                    store, source.runtime_type, source.validation_type, &annotated, materialization,
                )?;
                checks.push(ArgumentAliasCheck::Annotation(annotation));
            } else {
                checks.push(ArgumentAliasCheck::Declared(source.checked_type));
            }
            instruction = source.initializer;
            use_instruction = source.initializer;
            owned_decoder = Some(next);
        } else {
            let (has_local_write, initializer) = current.local_immutable_let_initializer(slot)?;
            let Some(initializer) = initializer.filter(|_| has_local_write) else {
                break checked_argument_type_inner(
                    store, current, use_instruction, instruction, materialization, depth,
                )?;
            };
            let annotation = current.local_immutable_let_annotation(slot)?;
            let slot_type = current.checked_slot_type(slot)?;
            if let Some(annotation) = annotation {
                let annotated = store.semantic.to_type_with_budget(
                    annotation, &mut materialization.semantic_types,
                )?;
                if let Some(slot_type) = slot_type {
                    let checked = store.semantic.to_type_with_budget(
                        slot_type, &mut materialization.semantic_types,
                    )?;
                    if checked != annotated {
                        return Err(IrVerifyError::new(
                            "local slot type fact disagrees with its declaration annotation",
                        ));
                    }
                } else if annotated != Type::Any {
                    return Err(IrVerifyError::new("annotated local has no checked slot type"));
                }
                checks.push(ArgumentAliasCheck::Annotation(annotation));
            } else if let Some(slot_type) = slot_type {
                checks.push(ArgumentAliasCheck::Declared(slot_type));
            }
            instruction = initializer;
        }
    };
    let Some(mut actual) = actual else {
        return Ok(None);
    };
    for check in checks.into_iter().rev() {
        let type_id = match check {
            ArgumentAliasCheck::Annotation(type_id) | ArgumentAliasCheck::Declared(type_id) => type_id,
        };
        let declared = store.semantic.to_type_with_budget(
            type_id, &mut materialization.semantic_types,
        )?;
        actual = match check {
            ArgumentAliasCheck::Annotation(_) => {
                validate_annotated_initializer_type(actual, &declared, materialization)?
            }
            ArgumentAliasCheck::Declared(_) => match actual {
                CheckedArgumentType::Known(actual) if actual.matches_expected(&declared) => {
                    CheckedArgumentType::Known(actual)
                }
                CheckedArgumentType::ListItems(items) => {
                    let Some(expected_item) = expected_list_item(&declared, !items.is_empty()) else {
                        return Ok(None);
                    };
                    if !items.iter().all(|item| item.matches_expected(expected_item)) {
                        return Ok(None);
                    }
                    CheckedArgumentType::ListItems(items)
                }
                CheckedArgumentType::EmptyList(_) => CheckedArgumentType::EmptyList(None),
                CheckedArgumentType::EmptyMap => CheckedArgumentType::EmptyMap,
                _ => return Ok(None),
            },
        };
    }
    Ok(Some(actual))
}

fn checked_argument_type_inner(
    store: &FullStore,
    decoder: &FullDecoder<'_>,
    use_instruction: usize,
    instruction: usize,
    materialization: &mut SchemeInstantiationBudget,
    depth: usize,
) -> Result<Option<CheckedArgumentType>, IrVerifyError> {
    if depth > 128 {
        return Err(IrVerifyError::new(
            "generic call argument nesting is too deep",
        ));
    }
    materialization.charge(1)?;
    if let schema::ExprView::CheckedValue { check, .. } =
        decoder.expression_view(instruction)?
    {
        if check.schema.is_some() {
            return Ok(None);
        }
        let checked_type = store
            .semantic
            .to_type_with_budget(check.ty.id, &mut materialization.semantic_types)?;
        ensure_closed_type(&checked_type)?;
        if !checked_type.has_unsigned_constraint()
            || !runtime_complete_unsigned_check_type(&checked_type, materialization, 0)?
        {
            return Ok(None);
        }
        return Ok(Some(CheckedArgumentType::Known(checked_type)));
    }
    if decoder.is_empty_list_literal_expression(instruction)? {
        return Ok(Some(CheckedArgumentType::EmptyList(None)));
    }
    match decoder.expression_view(instruction)? {
        schema::ExprView::EmptyMap => return Ok(Some(CheckedArgumentType::EmptyMap)),
        schema::ExprView::List(mut values) => {
            let mut item_types = Vec::new();
            for value in &mut values {
                let Some(item) = checked_argument_type(
                    store,
                    decoder,
                    use_instruction,
                    value?.raw() as usize,
                    materialization,
                    depth + 1,
                )?
                else {
                    return Ok(None);
                };
                match item {
                    CheckedArgumentType::Known(ty) => item_types.push(ty),
                    CheckedArgumentType::ListItems(items) => {
                        let Some(ty) = merge_list_item_types(&items, materialization)? else {
                            return Ok(None);
                        };
                        item_types.push(ty);
                    }
                    CheckedArgumentType::EmptyList(_) | CheckedArgumentType::EmptyMap => return Ok(None),
                }
            }
            return Ok(Some(CheckedArgumentType::ListItems(item_types)));
        }
        schema::ExprView::ListBuild(mut values) => {
            let mut item_types = Vec::new();
            for entry in &mut values {
                let (splice, value, _) = entry?;
                let Some(actual) = checked_argument_type(
                    store,
                    decoder,
                    use_instruction,
                    value.raw() as usize,
                    materialization,
                    depth + 1,
                )?
                else {
                    return Ok(None);
                };
                match (splice, actual) {
                    (true, CheckedArgumentType::EmptyList(None)) => {}
                    (true, CheckedArgumentType::EmptyList(Some(actual))) => {
                        let Type::List(item) = actual.unvalidated() else {
                            return Ok(None);
                        };
                        materialization.charge_clone(item)?;
                        item_types.push(item.as_ref().clone());
                    }
                    (true, CheckedArgumentType::Known(actual)) => {
                        let Type::List(item) = actual.unvalidated() else {
                            return Ok(None);
                        };
                        materialization.charge_clone(item)?;
                        item_types.push(item.as_ref().clone());
                    }
                    (true, CheckedArgumentType::ListItems(items)) => {
                        let Some(Type::List(item)) =
                            merge_list_item_types(&items, materialization)?
                        else {
                            if items.is_empty() {
                                continue;
                            }
                            return Ok(None);
                        };
                        item_types.push(*item);
                    }
                    (false, CheckedArgumentType::Known(actual)) => item_types.push(actual),
                    (false, CheckedArgumentType::ListItems(items)) => {
                        let Some(actual) = merge_list_item_types(&items, materialization)? else {
                            return Ok(None);
                        };
                        item_types.push(actual);
                    }
                    (false, CheckedArgumentType::EmptyList(Some(actual))) => {
                        item_types.push(actual);
                    }
                    (false, CheckedArgumentType::EmptyList(None)) | (_, CheckedArgumentType::EmptyMap) => return Ok(None),
                }
            }
            if item_types.is_empty() {
                return Ok(Some(CheckedArgumentType::EmptyList(None)));
            }
            return Ok(Some(CheckedArgumentType::ListItems(item_types)));
        }
        schema::ExprView::Param(slot) => {
            if super::driver_owner_index(decoder.owner).is_none() {
                let (has_local_write, initializer) = decoder.local_immutable_let_initializer(slot)?;
                if has_local_write {
                    if initializer.is_some() {
                        return Err(IrVerifyError::new("generic argument alias was not resolved"));
                    }
                    let Some(actual) = decoder.local_immutable_let_scalar_type(slot)? else {
                        return Ok(None);
                    };
                    materialization.charge_clone(&actual)?;
                    if let Some(slot_type) = decoder.checked_slot_type(slot)?
                        && store.semantic.to_type_with_budget(
                            slot_type,
                            &mut materialization.semantic_types,
                        )? != actual
                    {
                        return Err(IrVerifyError::new(
                            "local scalar writer disagrees with its checked slot type",
                        ));
                    }
                    return Ok(Some(CheckedArgumentType::Known(actual)));
                }
            }
            let initializer = {
                let mut charge = |amount| materialization.charge(amount);
                decoder.match_bind_initializer_for_use(use_instruction, slot, &mut charge)?
            };
            if let Some(initializer) = initializer {
                return checked_argument_type(
                    store,
                    decoder,
                    use_instruction,
                    initializer,
                    materialization,
                    depth + 1,
                );
            }
            if super::driver_owner_index(decoder.owner).is_none()
                && let Some(type_id) = decoder.checked_slot_type(slot)?
            {
                return store
                    .semantic
                    .to_type_with_budget(type_id, &mut materialization.semantic_types)
                    .map(|ty| Some(CheckedArgumentType::Known(ty)));
            }
        }
        schema::ExprView::Field { base, name, .. } => {
            let base_instruction = base.raw() as usize;
            if decoder.instruction_range.contains(&instruction)
                && decoder.instruction_range.contains(&base_instruction)
                && decoder.store.tags.get(base_instruction) == Some(&FullTag::ExprParam)
            {
                let Some(CheckedArgumentType::Known(base_type)) = checked_argument_type(
                    store,
                    decoder,
                    use_instruction,
                    base_instruction,
                    materialization,
                    depth + 1,
                )?
                else {
                    return Ok(None);
                };
                ensure_closed_type(&base_type)?;
                let Type::Record(fields) = base_type.into_unvalidated() else {
                    return Ok(None);
                };
                let field = Name::intern(name);
                let Some(field_type) = fields.get(&field) else {
                    return Ok(None);
                };
                materialization.charge_clone(field_type)?;
                return Ok(Some(CheckedArgumentType::Known(field_type.clone())));
            }
        }
        _ => {}
    }
    let instruction = u32::try_from(instruction)
        .map_err(|_| IrVerifyError::new("checked argument instruction is out of bounds"))?;
    if let Some(type_id) = checked_expression_type_id(store, instruction) {
        return store
            .semantic
            .to_type_with_budget(type_id, &mut materialization.semantic_types)
            .map(|ty| Some(CheckedArgumentType::Known(ty)));
    }
    decoder
        .derive_checked_expression_type(instruction as usize)
        .map(|ty| ty.map(CheckedArgumentType::Known))
}

fn verify_driver_annotation_runtime_type(
    store: &FullStore,
    runtime_type: Option<TypeId>,
    validation_type: Option<TypeId>,
    annotation: &Type,
    materialization: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    let runtime_type = runtime_type
        .ok_or_else(|| IrVerifyError::new("annotated driver local has no runtime type"))?;
    let runtime_type = store
        .semantic
        .to_type_with_budget(runtime_type, &mut materialization.semantic_types)?;
    let annotation_kind = super::lowered_type_from_type(annotation)?;
    if super::lowered_type_from_type(&runtime_type)? != annotation_kind {
        return Err(IrVerifyError::new(
            "driver annotation disagrees with its runtime type",
        ));
    }
    if let Some(validation_type) = validation_type {
        let validation = store
            .semantic
            .to_type_with_budget(validation_type, &mut materialization.semantic_types)?;
        if validation != *annotation {
            return Err(IrVerifyError::new(
                "driver annotation disagrees with its runtime validation",
            ));
        }
    }
    Ok(())
}

fn validate_annotated_initializer_type(
    actual: CheckedArgumentType,
    annotation: &Type,
    materialization: &mut SchemeInstantiationBudget,
) -> Result<CheckedArgumentType, IrVerifyError> {
    let matches = match actual {
        CheckedArgumentType::Known(actual) => {
            ensure_closed_type(&actual)?;
            initializer_type_matches_annotation(&actual, annotation, materialization, 0)?
        }
        CheckedArgumentType::ListItems(items) => {
            let Some(expected_item) = expected_list_item(annotation, !items.is_empty()) else {
                return Err(IrVerifyError::new("local initializer disagrees with its declaration annotation"));
            };
            let mut matches = true;
            for item in &items {
                ensure_closed_type(item)?;
                if !initializer_type_matches_annotation(
                    item,
                    expected_item,
                    materialization,
                    0,
                )? {
                    matches = false;
                    break;
                }
            }
            matches
        }
        CheckedArgumentType::EmptyList(Some(actual)) => {
            ensure_closed_type(&actual)?;
            matches!(&actual, Type::List(_))
                && initializer_type_matches_annotation(
                    &actual,
                    annotation,
                    materialization,
                    0,
                )?
        }
        CheckedArgumentType::EmptyList(None) => matches!(annotation, Type::List(_)),
        CheckedArgumentType::EmptyMap => matches!(annotation, Type::Map(_, _)),
    };
    if !matches {
        return Err(IrVerifyError::new(
            "local initializer disagrees with its declaration annotation",
        ));
    }
    materialization.charge_clone(annotation)?;
    Ok(CheckedArgumentType::Known(annotation.clone()))
}

fn initializer_type_matches_annotation(
    actual: &Type,
    annotation: &Type,
    materialization: &mut SchemeInstantiationBudget,
    depth: usize,
) -> Result<bool, IrVerifyError> {
    if depth > 128 {
        return Err(IrVerifyError::new(
            "annotated initializer type nesting is too deep",
        ));
    }
    materialization.charge(1)?;
    match (actual, annotation) {
        (Type::Int, Type::UInt) => Ok(false),
        (Type::Validated(actual), Type::Validated(annotation)) => {
            Ok(actual.validation().implies(annotation.validation())
                && initializer_type_matches_annotation(
                    actual.base(),
                    annotation.base(),
                    materialization,
                    depth + 1,
                )?)
        }
        (Type::Validated(actual), annotation) => initializer_type_matches_annotation(
            actual.base(),
            annotation,
            materialization,
            depth + 1,
        ),
        (Type::Union(actual), Type::Union(annotation)) => {
            for actual_member in actual {
                let mut matched = false;
                for annotation_member in annotation {
                    if initializer_type_matches_annotation(
                        actual_member,
                        annotation_member,
                        materialization,
                        depth + 1,
                    )? {
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (actual, Type::Union(members)) => {
            for member in members {
                if initializer_type_matches_annotation(
                    actual,
                    member,
                    materialization,
                    depth + 1,
                )? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        (Type::Union(members), annotation) => {
            for member in members {
                if !initializer_type_matches_annotation(
                    member,
                    annotation,
                    materialization,
                    depth + 1,
                )? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (Type::List(actual), Type::List(annotation))
        | (Type::Stream(actual), Type::Stream(annotation))
        | (Type::Set(actual), Type::Set(annotation))
        | (Type::Optional(actual), Type::Optional(annotation)) => {
            initializer_type_matches_annotation(
                actual,
                annotation,
                materialization,
                depth + 1,
            )
        }
        (Type::Map(actual_key, actual_value), Type::Map(annotation_key, annotation_value)) => {
            Ok(initializer_types_match_invariant(
                actual_key,
                annotation_key,
                materialization,
                depth + 1,
            )? && initializer_type_matches_annotation(
                actual_value,
                annotation_value,
                materialization,
                depth + 1,
            )?)
        }
        (Type::Null, Type::Optional(_)) => Ok(true),
        (actual, Type::Optional(annotation)) => initializer_type_matches_annotation(
            actual,
            annotation,
            materialization,
            depth + 1,
        ),
        (Type::Result(actual_ok, actual_error), Type::Result(annotation_ok, annotation_error)) => {
            Ok(initializer_type_matches_annotation(
                actual_ok,
                annotation_ok,
                materialization,
                depth + 1,
            )? && initializer_type_matches_annotation(
                actual_error,
                annotation_error,
                materialization,
                depth + 1,
            )?)
        }
        (Type::Record(actual_fields), Type::Record(annotation_fields)) => {
            for (name, annotation_field) in annotation_fields.iter() {
                let Some(actual_field) = actual_fields.get(name) else {
                    return Ok(false);
                };
                if !initializer_type_matches_annotation(
                    actual_field,
                    annotation_field,
                    materialization,
                    depth + 1,
                )? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(actual.matches_expected(annotation)),
    }
}

fn initializer_types_match_invariant(
    actual: &Type,
    annotation: &Type,
    materialization: &mut SchemeInstantiationBudget,
    depth: usize,
) -> Result<bool, IrVerifyError> {
    if depth > 128 {
        return Err(IrVerifyError::new(
            "annotated initializer type nesting is too deep",
        ));
    }
    materialization.charge(1)?;
    if actual == annotation {
        return Ok(true);
    }
    match (actual, annotation) {
        (Type::Validated(actual), Type::Validated(annotation)) => {
            Ok(actual.validation() == annotation.validation()
                && initializer_types_match_invariant(
                    actual.base(),
                    annotation.base(),
                    materialization,
                    depth + 1,
                )?)
        }
        (Type::List(actual), Type::List(annotation))
        | (Type::Stream(actual), Type::Stream(annotation))
        | (Type::Set(actual), Type::Set(annotation))
        | (Type::Optional(actual), Type::Optional(annotation)) => {
            initializer_types_match_invariant(
                actual,
                annotation,
                materialization,
                depth + 1,
            )
        }
        (Type::Map(actual_key, actual_value), Type::Map(annotation_key, annotation_value))
        | (
            Type::Result(actual_key, actual_value),
            Type::Result(annotation_key, annotation_value),
        ) => Ok(initializer_types_match_invariant(
            actual_key,
            annotation_key,
            materialization,
            depth + 1,
        )? && initializer_types_match_invariant(
            actual_value,
            annotation_value,
            materialization,
            depth + 1,
        )?),
        (Type::Record(actual_fields), Type::Record(annotation_fields)) => {
            if actual_fields.len() != annotation_fields.len() {
                return Ok(false);
            }
            for (name, actual_field) in actual_fields.iter() {
                let Some(annotation_field) = annotation_fields.get(name) else {
                    return Ok(false);
                };
                if !initializer_types_match_invariant(
                    actual_field,
                    annotation_field,
                    materialization,
                    depth + 1,
                )? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (Type::Union(actual_members), Type::Union(annotation_members)) => {
            if actual_members.len() != annotation_members.len() {
                return Ok(false);
            }
            for actual_member in actual_members {
                let mut matched = false;
                for annotation_member in annotation_members {
                    if initializer_types_match_invariant(
                        actual_member,
                        annotation_member,
                        materialization,
                        depth + 1,
                    )? {
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn runtime_complete_unsigned_check_type(
    ty: &Type,
    materialization: &mut SchemeInstantiationBudget,
    depth: usize,
) -> Result<bool, IrVerifyError> {
    if depth > 128 {
        return Err(IrVerifyError::new(
            "generic checked-value type nesting is too deep",
        ));
    }
    materialization.charge(1)?;
    Ok(match ty {
        Type::Null
        | Type::Bool
        | Type::Int
        | Type::UInt
        | Type::Float
        | Type::Duration
        | Type::Str
        | Type::Bytes
        | Type::Digest
        | Type::Regex
        | Type::Path
        | Type::ErasedRecord
        | Type::Status
        | Type::EnvPathList
        | Type::Error
        | Type::ErrorFamily(_)
        | Type::ErrorVariant { .. }
        | Type::ErrorFacet(_)
        | Type::ProcessError
        | Type::Pure
        | Type::Proc
        | Type::Command
        | Type::ProcessHandle
        | Type::NetJob
        | Type::FsRoot
        | Type::FsLock
        | Type::Unit
        | Type::Tag(_) => true,
        Type::List(item) | Type::Optional(item) => {
            runtime_complete_unsigned_check_type(item, materialization, depth + 1)?
        }
        Type::Map(key, value) => {
            key.is_map_key()
                && runtime_complete_unsigned_check_type(value, materialization, depth + 1)?
        }
        Type::Set(item) => item.is_map_key(),
        Type::Record(fields) => {
            let mut complete = true;
            for field_type in fields.values() {
                complete &=
                    runtime_complete_unsigned_check_type(field_type, materialization, depth + 1)?;
            }
            complete
        }
        Type::Result(success, error) => {
            runtime_complete_unsigned_check_type(success, materialization, depth + 1)?
                && runtime_complete_unsigned_check_type(error, materialization, depth + 1)?
        }
        Type::Union(members) => {
            if members.is_empty() {
                false
            } else {
                let mut complete = true;
                for member in members {
                    complete &=
                        runtime_complete_unsigned_check_type(member, materialization, depth + 1)?;
                }
                complete
            }
        }
        Type::Validated(validated) => match validated.validation() {
            Validation::Nominal(_) => false,
            _ => runtime_complete_unsigned_check_type(
                validated.base(),
                materialization,
                depth + 1,
            )?,
        },
        Type::BuiltinParameter(_)
        | Type::Inference(_)
        | Type::Any
        | Type::Unknown
        | Type::Invalid
        | Type::Stream(_)
        | Type::Module(_)
        | Type::DynamicModule
        | Type::Callable(_) => false,
    })
}

fn merge_list_item_types(
    items: &[Type],
    materialization: &mut SchemeInstantiationBudget,
) -> Result<Option<Type>, IrVerifyError> {
    let mut items = items.iter();
    let Some(first) = items.next() else {
        return Ok(None);
    };
    materialization.charge_clone(first)?;
    let mut item = first.clone();
    for next in items {
        materialization.charge_clone(&item)?;
        materialization.charge_clone(next)?;
        let Some(merged) = merge_list_literal_item_ty(&item, next) else {
            return Ok(None);
        };
        item = merged;
    }
    materialization.charge(1)?;
    Ok(Some(Type::List(Box::new(item))))
}

fn expected_list_item(ty: &Type, nonempty: bool) -> Option<&Type> {
    match ty {
        Type::List(item) => Some(item),
        Type::Validated(validated)
            if nonempty && validated.validation() == Validation::NonEmpty =>
        {
            let Type::List(item) = validated.base() else {
                return None;
            };
            Some(item)
        }
        _ => None,
    }
}

fn scheme_type_binders(
    store: &FullStore,
    root: SchemeTypeId,
    materialization: &mut SchemeInstantiationBudget,
) -> Result<(BTreeSet<usize>, BTreeSet<usize>), IrVerifyError> {
    let mut pending = vec![root];
    let mut visited = BTreeSet::new();
    let mut types = BTreeSet::new();
    let mut rows = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }
        materialization.charge(1)?;
        let node = store.scheme_type(id)?;
        match node.tag {
            FullSchemeTypeTag::Parameter => {
                types.insert(node.first as usize);
            }
            FullSchemeTypeTag::List
            | FullSchemeTypeTag::Stream
            | FullSchemeTypeTag::Set
            | FullSchemeTypeTag::Optional => pending.push(SchemeTypeId::from_raw(node.first)?),
            FullSchemeTypeTag::Map | FullSchemeTypeTag::Result => {
                pending.push(SchemeTypeId::from_raw(node.first)?);
                pending.push(SchemeTypeId::from_raw(node.second)?);
            }
            FullSchemeTypeTag::Record => {
                if node.first != IR_NONE {
                    rows.insert(node.first as usize);
                }
                for field in bounded(
                    node.children,
                    &store.scheme_fields,
                    "scheme record field range is invalid",
                )? {
                    pending.push(field.ty);
                }
            }
            FullSchemeTypeTag::Union => {
                for child in bounded(
                    node.children,
                    &store.scheme_children,
                    "scheme child range is invalid",
                )? {
                    pending.push(*child);
                }
            }
            FullSchemeTypeTag::Validated => pending.push(SchemeTypeId::from_raw(node.first)?),
            FullSchemeTypeTag::Ground => {}
        }
    }
    Ok((types, rows))
}

fn checked_expression_type_id(store: &FullStore, instruction: u32) -> Option<TypeId> {
    store
        .checked_expr_types
        .binary_search_by_key(&instruction, |fact| fact.instruction)
        .ok()
        .and_then(|index| { let fact = store.checked_expr_types[index]; fact.is_contract().then_some(fact.type_id) })
}

fn verify_scheme_argument_evidence(
    store: &FullStore,
    scope: &SchemeScope,
    scheme_type: SchemeTypeId,
    actual: &Type,
    substitution: &CallSubstitution,
    evidence: &mut SchemeBinderEvidence,
    materialization: &mut SchemeInstantiationBudget,
    depth: usize,
) -> Result<(), IrVerifyError> {
    if depth > 128 {
        return Err(IrVerifyError::new("generic call argument type nesting is too deep"));
    }
    materialization.charge(1)?;
    let node = store.scheme_type(scheme_type)?;
    let actual_shape = actual.unvalidated();
    match node.tag {
        FullSchemeTypeTag::Parameter => {
            let binder = node.first as usize;
            if binder >= scope.type_parameters.len() {
                return Err(IrVerifyError::new("scheme type parameter is out of scope"));
            }
            if substitution.types.get(binder) != Some(actual) {
                return Err(IrVerifyError::new(
                    "generic type binder disagrees with the checked argument",
                ));
            }
            evidence.types.insert(binder);
        }
        FullSchemeTypeTag::Record if node.first != IR_NONE => {
            let Type::Record(actual_fields) = actual_shape else {
                return Err(IrVerifyError::new("generic row argument is not a checked record"));
            };
            let binder = node.first as usize;
            if binder >= scope.row_parameters.len() {
                return Err(IrVerifyError::new("scheme row parameter is out of scope"));
            }
            let row_evidence = &substitution.rows[binder];
            let fields = bounded(node.children, &store.scheme_fields, "scheme record field range is invalid")?;
            let explicit = fields
                .iter()
                .map(|field| Ok((Name::intern(store.string(field.name)?), field.ty)))
                .collect::<Result<BTreeMap<_, _>, IrVerifyError>>()?;
            let actual_extras = actual_fields
                .iter()
                .filter(|(name, _)| !explicit.contains_key(name))
                .map(|(name, ty)| (*name, ty))
                .collect::<BTreeMap<_, _>>();
            if actual_extras.len() != row_evidence.len()
                || actual_extras
                    .iter()
                    .any(|(name, ty)| row_evidence.get(name).is_none_or(|expected| expected != *ty))
            {
                return Err(IrVerifyError::new("generic row substitution disagrees with the checked argument"));
            }
            evidence.rows.insert(binder);
            for (name, field_type) in explicit {
                let actual_field = actual_fields
                    .get(&name)
                    .ok_or_else(|| IrVerifyError::new("generic call is missing a required record field"))?;
                verify_scheme_argument_evidence(
                    store,
                    scope,
                    field_type,
                    actual_field,
                    substitution,
                    evidence,
                    materialization,
                    depth + 1,
                )?;
            }
            return Ok(());
        }
        FullSchemeTypeTag::Record => {
            if let Type::Record(actual_fields) = actual_shape {
                for field in bounded(node.children, &store.scheme_fields, "scheme record field range is invalid")? {
                    let name = Name::intern(store.string(field.name)?);
                    if let Some(actual_field) = actual_fields.get(&name) {
                        verify_scheme_argument_evidence(
                            store,
                            scope,
                            field.ty,
                            actual_field,
                            substitution,
                            evidence,
                            materialization,
                            depth + 1,
                        )?;
                    }
                }
            }
        }
        FullSchemeTypeTag::List | FullSchemeTypeTag::Stream | FullSchemeTypeTag::Set => {
            let inner = match (node.tag, actual_shape) {
                (FullSchemeTypeTag::List, Type::List(inner))
                | (FullSchemeTypeTag::Stream, Type::Stream(inner))
                | (FullSchemeTypeTag::Set, Type::Set(inner)) => inner,
                _ => return Ok(()),
            };
            verify_scheme_argument_evidence(
                store,
                scope,
                SchemeTypeId::from_raw(node.first)?,
                inner,
                substitution,
                evidence,
                materialization,
                depth + 1,
            )?;
        }
        FullSchemeTypeTag::Optional => {
            if matches!(actual_shape, Type::Null) {
                return Ok(());
            }
            let inner = match actual_shape {
                Type::Optional(inner) => inner,
                actual => actual,
            };
            verify_scheme_argument_evidence(
                store,
                scope,
                SchemeTypeId::from_raw(node.first)?,
                inner,
                substitution,
                evidence,
                materialization,
                depth + 1,
            )?;
        }
        FullSchemeTypeTag::Map => {
            if let Type::Map(key, value) = actual_shape {
                verify_scheme_argument_evidence(store, scope, SchemeTypeId::from_raw(node.first)?, key, substitution, evidence, materialization, depth + 1)?;
                verify_scheme_argument_evidence(store, scope, SchemeTypeId::from_raw(node.second)?, value, substitution, evidence, materialization, depth + 1)?;
            }
        }
        FullSchemeTypeTag::Result => {
            if let Type::Result(ok, error) = actual_shape {
                verify_scheme_argument_evidence(store, scope, SchemeTypeId::from_raw(node.first)?, ok, substitution, evidence, materialization, depth + 1)?;
                verify_scheme_argument_evidence(store, scope, SchemeTypeId::from_raw(node.second)?, error, substitution, evidence, materialization, depth + 1)?;
            }
        }
        FullSchemeTypeTag::Validated => {
            verify_scheme_argument_evidence(
                store,
                scope,
                SchemeTypeId::from_raw(node.first)?,
                actual_shape,
                substitution,
                evidence,
                materialization,
                depth + 1,
            )?;
        }
        FullSchemeTypeTag::Union => {
            let members = match actual_shape {
                Type::Union(members) => members.iter().collect(),
                actual => vec![actual],
            };
            let children = bounded(
                node.children,
                &store.scheme_children,
                "scheme union child range is invalid",
            )?;
            for actual_member in members {
                let mut best = None;
                let mut best_evidence = evidence.types.len() + evidence.rows.len();
                for child in children {
                    let child_expected = instantiate_scheme_type(
                        store,
                        scope,
                        *child,
                        substitution,
                        materialization,
                        depth + 1,
                    )?;
                    if !actual_member.matches_expected(&child_expected) {
                        continue;
                    }
                    let mut candidate = evidence.clone();
                    if verify_scheme_argument_evidence(
                        store,
                        scope,
                        *child,
                        actual_member,
                        substitution,
                        &mut candidate,
                        materialization,
                        depth + 1,
                    )
                    .is_ok()
                    {
                        let candidate_count = candidate.types.len() + candidate.rows.len();
                        if best.is_none() || candidate_count > best_evidence {
                            best = Some(candidate);
                            best_evidence = candidate_count;
                        }
                    }
                }
                let Some(best) = best else {
                    return Err(IrVerifyError::new(
                        "generic union argument disagrees with its checked substitution",
                    ));
                };
                *evidence = best;
            }
        }
        _ => {}
    }
    Ok(())
}

fn scheme_type_work_limit(store: &FullStore) -> usize {
    store
        .scheme_types
        .len()
        .saturating_mul(SCHEME_TYPE_WORK_PER_NODE)
        .saturating_add(SCHEME_TYPE_WORK_BASE)
}

fn instantiate_scheme_type(
    store: &FullStore,
    scope: &SchemeScope,
    id: SchemeTypeId,
    substitution: &CallSubstitution,
    budget: &mut SchemeInstantiationBudget,
    depth: usize,
) -> Result<Type, IrVerifyError> {
    let ty = instantiate_scheme_type_inner(store, scope, id, substitution, budget, depth)?;
    ensure_closed_type(&ty)?;
    Ok(ty)
}

fn instantiate_scheme_type_inner(
    store: &FullStore,
    scope: &SchemeScope,
    id: SchemeTypeId,
    substitution: &CallSubstitution,
    budget: &mut SchemeInstantiationBudget,
    depth: usize,
) -> Result<Type, IrVerifyError> {
    if depth > 128 {
        return Err(IrVerifyError::new("generic call result type nesting is too deep"));
    }
    budget.charge(1)?;
    let node = store.scheme_type(id)?;
    let ty = match node.tag {
        FullSchemeTypeTag::Ground => {
            let id = TypeId::from_raw(node.first)
                .ok_or_else(|| IrVerifyError::new("ground scheme type id is invalid"))?;
            store
                .semantic
                .to_type_with_budget(id, &mut budget.semantic_types)?
        }
        FullSchemeTypeTag::Parameter => {
            let binder = node.first as usize;
            if binder >= scope.type_parameters.len() {
                return Err(IrVerifyError::new("scheme type parameter is out of scope"));
            }
            budget.charge_clone(&substitution.types[binder])?;
            substitution.types[binder].clone()
        }
        FullSchemeTypeTag::List => Type::List(Box::new(instantiate_scheme_type_inner(store, scope, SchemeTypeId::from_raw(node.first)?, substitution, budget, depth + 1)?)),
        FullSchemeTypeTag::Stream => Type::Stream(Box::new(instantiate_scheme_type_inner(store, scope, SchemeTypeId::from_raw(node.first)?, substitution, budget, depth + 1)?)),
        FullSchemeTypeTag::Set => Type::Set(Box::new(instantiate_scheme_type_inner(store, scope, SchemeTypeId::from_raw(node.first)?, substitution, budget, depth + 1)?)),
        FullSchemeTypeTag::Optional => Type::Optional(Box::new(instantiate_scheme_type_inner(store, scope, SchemeTypeId::from_raw(node.first)?, substitution, budget, depth + 1)?)),
        FullSchemeTypeTag::Map => Type::Map(
            Box::new(instantiate_scheme_type_inner(store, scope, SchemeTypeId::from_raw(node.first)?, substitution, budget, depth + 1)?),
            Box::new(instantiate_scheme_type_inner(store, scope, SchemeTypeId::from_raw(node.second)?, substitution, budget, depth + 1)?),
        ),
        FullSchemeTypeTag::Result => Type::Result(
            Box::new(instantiate_scheme_type_inner(store, scope, SchemeTypeId::from_raw(node.first)?, substitution, budget, depth + 1)?),
            Box::new(instantiate_scheme_type_inner(store, scope, SchemeTypeId::from_raw(node.second)?, substitution, budget, depth + 1)?),
        ),
        FullSchemeTypeTag::Record => {
            let mut fields = BTreeMap::new();
            for field in bounded(node.children, &store.scheme_fields, "scheme record field range is invalid")? {
                let name = Name::intern(store.string(field.name)?);
                let ty = instantiate_scheme_type_inner(store, scope, field.ty, substitution, budget, depth + 1)?;
                if fields.insert(name, ty).is_some() {
                    return Err(IrVerifyError::new("scheme record fields are duplicated"));
                }
            }
            if node.first != IR_NONE {
                let binder = node.first as usize;
                if binder >= scope.row_parameters.len() {
                    return Err(IrVerifyError::new("scheme row parameter is out of scope"));
                }
                for (name, ty) in &substitution.rows[binder] {
                    budget.charge(1)?;
                    budget.charge_clone(ty)?;
                    if fields.insert(*name, ty.clone()).is_some() {
                        return Err(IrVerifyError::new("generic row evidence repeats a required field"));
                    }
                }
            }
            Type::Record(Arc::new(fields))
        }
        FullSchemeTypeTag::Union => {
            let members = bounded(node.children, &store.scheme_children, "scheme union child range is invalid")?
                .iter()
                .map(|child| instantiate_scheme_type_inner(store, scope, *child, substitution, budget, depth + 1))
                .collect::<Result<Vec<_>, _>>()?;
            if crate::sema::types::union_member_error(&members).is_some() {
                return Err(IrVerifyError::new("instantiated generic union is invalid"));
            }
            Type::Union(members)
        }
        FullSchemeTypeTag::Validated => {
            let base = instantiate_scheme_type_inner(store, scope, SchemeTypeId::from_raw(node.first)?, substitution, budget, depth + 1)?;
            let row = *store
                .scheme_validations
                .get(node.second as usize)
                .ok_or_else(|| IrVerifyError::new("scheme validation id is out of bounds"))?;
            let validation = verify_validation_row(store, row)?;
            Type::Validated(Box::new(
                ValidatedType::new(validation, base)
                    .map_err(|_| IrVerifyError::new("scheme validation does not fit its base type"))?,
            ))
        }
    };
    Ok(ty)
}

fn decoder_for_instruction(
    store: &FullStore,
    instruction: usize,
) -> Result<FullDecoder<'_>, IrVerifyError> {
    for (index, function) in store.functions.iter().enumerate() {
        let range = store.function_instruction_range(index)?;
        if range.contains(&instruction) {
            return decoder_for_function(store, index);
        }
        let _ = function;
    }
    for index in 0..store.driver_steps.len() {
        let range = store.driver_instruction_range(index)?;
        if range.contains(&instruction) {
            return decoder_for_driver_step(store, index);
        }
    }
    Err(IrVerifyError::new("generic call instruction has no executable owner"))
}

fn decoder_for_driver_step(
    store: &FullStore,
    index: usize,
) -> Result<FullDecoder<'_>, IrVerifyError> {
    let step = store
        .driver_steps
        .get(index)
        .ok_or_else(|| IrVerifyError::new("generic driver step is out of bounds"))?;
    Ok(FullDecoder {
        store,
        owner: driver_owner(index).map_err(|_| IrVerifyError::new("driver owner is invalid"))?,
        instruction_range: store.driver_instruction_range(index)?,
        instruction_states: None,
        instruction_proof: None,
        block_states: None,
        slot_count: step.slot_count,
        pattern_ceiling: Cell::new(usize::MAX),
        local_immutable_let_initializers: RefCell::default(),
        binding_identity_stack: OnceCell::new(),
        proven_callable_writers: RefCell::default(),
        function_signature_index: None,
        return_analysis: RefCell::default(),
        prepared_constant_work: Cell::new(0),
        verified: false,
    })
}

fn decoder_for_function(store: &FullStore, function_index: usize) -> Result<FullDecoder<'_>, IrVerifyError> {
    let function = store
        .functions
        .get(function_index)
        .ok_or_else(|| IrVerifyError::new("generic function is out of bounds"))?;
    Ok(FullDecoder {
        store,
        owner: IrFunctionId::new(function_index)
            .map_err(|_| IrVerifyError::new("generic function id is invalid"))?
            .raw(),
        instruction_range: store.function_instruction_range(function_index)?,
        instruction_states: None,
        instruction_proof: None,
        block_states: None,
        slot_count: function.slot_count,
        pattern_ceiling: Cell::new(usize::MAX),
        local_immutable_let_initializers: RefCell::default(),
        binding_identity_stack: OnceCell::new(),
        proven_callable_writers: RefCell::default(),
        function_signature_index: None,
        return_analysis: RefCell::default(),
        prepared_constant_work: Cell::new(0),
        verified: false,
    })
}

fn ensure_closed_type(ty: &Type) -> Result<(), IrVerifyError> {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        match ty {
            Type::BuiltinParameter(_) | Type::Inference(_) | Type::Unknown | Type::Invalid => return Err(IrVerifyError::new("scheme type contains an inference, recovery, or builtin parameter type")),
            Type::List(inner) | Type::Stream(inner) | Type::Set(inner) | Type::Optional(inner) => pending.push(inner),
            Type::Map(key, value) | Type::Result(key, value) => { pending.push(key); pending.push(value); }
            Type::Record(fields) => pending.extend(fields.values()),
            Type::Union(members) => pending.extend(members),
            Type::Validated(validated) => pending.push(validated.base()),
            Type::Callable(callable) => { pending.push(&callable.sig.return_ty); pending.extend(callable.sig.params.iter().map(|parameter| &parameter.ty)); }
            Type::Module(module) => {
                for export in module.values() {
                    match export {
                        crate::sema::types::ModuleExportType::Value { ty, .. } => pending.push(ty),
                        crate::sema::types::ModuleExportType::Pure { sig, .. } | crate::sema::types::ModuleExportType::Proc { sig, .. } => { pending.push(&sig.return_ty); pending.extend(sig.params.iter().map(|parameter| &parameter.ty)); }
                        crate::sema::types::ModuleExportType::GenericPure { .. } => {}
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn ensure_substitution_type(ty: &Type) -> Result<(), IrVerifyError> {
    ensure_closed_type(ty)?;
    if ty.contains_any() {
        return Err(IrVerifyError::new(
            format!("generic call substitution contains dynamic type {ty}"),
        ));
    }
    Ok(())
}

fn bounded<'a, T>(range: IrRange, values: &'a [T], message: &'static str) -> Result<&'a [T], IrVerifyError> {
    let bounds = range.bounds(values.len()).ok_or_else(|| IrVerifyError::new(message))?;
    Ok(&values[bounds])
}

fn mark_range<'a, T>(
    range: IrRange,
    values: &'a [T],
    coverage: &mut [bool],
    message: &'static str,
) -> Result<&'a [T], IrVerifyError> {
    let _ = range.bounds(values.len()).ok_or_else(|| IrVerifyError::new(message))?;
    let bounds = range.start as usize..range.start as usize + range.len as usize;
    if bounds.end > coverage.len() {
        return Err(IrVerifyError::new(message));
    }
    if coverage[bounds.clone()].iter().any(|covered| *covered) {
        return Err(IrVerifyError::new(message));
    }
    coverage[bounds.clone()].fill(true);
    Ok(&values[bounds])
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::super::semantic::SemanticPoolBuilder;
    use super::super::schemes::FullSchemeTypeNode;

    fn symbolic_substitution_test_scope() -> SchemeScope {
        SchemeScope {
            type_parameters: vec![FullSchemeTypeParameter { id: 10, binder: 0 }],
            row_parameters: vec![FullSchemeRowParameter {
                id: 20,
                binder: 0,
                lacks: IrRange::default(),
            }],
            type_binders: BTreeMap::from([(10, 0)]),
            row_binders: BTreeMap::from([(20, 0)]),
            row_lacks: vec![BTreeSet::new()],
        }
    }

    #[test]
    fn symbolic_substitution_charges_cloned_type_and_row_evidence() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let scope = symbolic_substitution_test_scope();
            let type_argument = SchemeType::List(Box::new(SchemeType::Ground(Type::Str)));
            let substitution = SymbolicCallSubstitution {
                types: vec![type_argument],
                rows: vec![SchemeRowSubstitution {
                    fields: BTreeMap::from([(
                        Name::intern("field"),
                        SchemeType::List(Box::new(SchemeType::Ground(Type::Str))),
                    )]),
                    tail: SchemeRowTail::Closed,
                }],
            };
            let ground_error = substitute_scheme_type(
                &SchemeType::Ground(Type::List(Box::new(Type::Str))),
                &scope,
                &substitution,
                &mut SchemeInstantiationBudget {
                    remaining: 1,
                    semantic_types: TypeMaterializationBudget::default(),
                },
                0,
            )
            .unwrap_err();
            assert!(ground_error.message.contains("exceeds its work bound"));

            let type_error = substitute_scheme_type(
                &SchemeType::Parameter(SchemeTypeParameterId(10)),
                &scope,
                &substitution,
                &mut SchemeInstantiationBudget {
                    remaining: 3,
                    semantic_types: TypeMaterializationBudget::default(),
                },
                0,
            )
            .unwrap_err();
            assert!(type_error.message.contains("exceeds its work bound"));

            let row_error = substitute_scheme_type(
                &SchemeType::Record {
                    fields: BTreeMap::new(),
                    tail: SchemeRowTail::Parameter(SchemeRowParameterId(20)),
                },
                &scope,
                &substitution,
                &mut SchemeInstantiationBudget {
                    remaining: 4,
                    semantic_types: TypeMaterializationBudget::default(),
                },
                0,
            )
            .unwrap_err();
            assert!(row_error.message.contains("exceeds its work bound"));
        });
    }

    #[test]
    fn substitutions_share_one_type_materialization_budget() {
        let mut store = FullStore::default();
        let mut semantic = SemanticPoolBuilder::default();
        let mut ty = Type::Int;
        for _ in 0..15 {
            ty = Type::Result(Box::new(ty.clone()), Box::new(ty));
        }
        let type_id = semantic.intern_type(&mut store.semantic, &ty).unwrap();
        assert!(store.semantic.to_type(type_id).is_ok());
        let scheme_type_id = SchemeTypeId::from_raw(0).unwrap();
        store.scheme_types.push(FullSchemeTypeNode {
            first: type_id.raw(),
            second: 0,
            children: IrRange::default(),
            tag: FullSchemeTypeTag::Ground,
            reserved: [0; 3],
        });

        let scope = SchemeScope {
            type_parameters: vec![
                FullSchemeTypeParameter { id: 10, binder: 0 },
                FullSchemeTypeParameter { id: 11, binder: 1 },
            ],
            row_parameters: Vec::new(),
            type_binders: BTreeMap::from([(10, 0), (11, 1)]),
            row_binders: BTreeMap::new(),
            row_lacks: Vec::new(),
        };
        let type_rows = [
            FullTypeArgument {
                binder: 10,
                ty: scheme_type_id,
            },
            FullTypeArgument {
                binder: 11,
                ty: scheme_type_id,
            },
        ];
        let error = build_substitution(&store, &scope, &type_rows, &[], &mut [])
            .err()
            .expect("aggregate type materialization should exceed the shared budget");

        assert!(error.message.contains("semantic type materialization exceeds its work limit"));
    }
}

fn require_empty_children(range: IrRange) -> Result<(), IrVerifyError> {
    if range.len == 0 {
        Ok(())
    } else {
        Err(IrVerifyError::new("scheme type node has unexpected child rows"))
    }
}
